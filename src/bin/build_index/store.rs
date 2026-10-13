//! Object storage for sources, models and indexes: any S3-compatible store (`s3://bucket/prefix`,
//! configured by the standard `AWS_*` variables) or a local directory (`file:///path`).
//!
//! A directory is published as its files, then `MANIFEST` (paths, sizes, SHA-256), then
//! `_COMPLETE`; readers trust nothing without `_COMPLETE` and check every file against `MANIFEST`.
use anyhow::{Context, Result, bail, ensure};
use object_store::{
    ObjectStore, ObjectStoreExt, PutPayload, WriteMultipart, aws::AmazonS3Builder,
    local::LocalFileSystem, path::Path as Key,
};
use serde::{Deserialize, Serialize};
use std::{
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{sync::Semaphore, task::JoinSet};

const CHUNK: u64 = 32 << 20;
const TRANSFERS: usize = 16;

#[derive(Clone)]
pub struct Store {
    inner: Arc<dyn ObjectStore>,
    root: String,
    permits: Arc<Semaphore>,
}

#[derive(Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub files: Vec<Entry>,
}

impl Store {
    pub fn open(url: &str) -> Result<Self> {
        let (inner, root): (Arc<dyn ObjectStore>, String) =
            if let Some(dir) = url.strip_prefix("file://") {
                std::fs::create_dir_all(dir).with_context(|| format!("create {dir}"))?;
                (
                    Arc::new(LocalFileSystem::new_with_prefix(dir)?),
                    String::new(),
                )
            } else if let Some(rest) = url.strip_prefix("s3://") {
                let (bucket, prefix) = rest.split_once('/').unwrap_or((rest, ""));
                ensure!(!bucket.is_empty(), "store URL {url} names no bucket");
                let s3 = AmazonS3Builder::from_env()
                    .with_bucket_name(bucket)
                    .build()
                    .context("configure S3 from the AWS_* environment")?;
                (Arc::new(s3), prefix.trim_matches('/').to_string())
            } else {
                bail!("store URL must be s3://bucket/prefix or file:///path, got {url}");
            };
        Ok(Self::new(inner, root))
    }

    /// Overture's public release bucket, read without credentials.
    pub fn overture() -> Result<Self> {
        let s3 = AmazonS3Builder::new()
            .with_bucket_name("overturemaps-us-west-2")
            .with_region("us-west-2")
            .with_skip_signature(true)
            .build()?;
        Ok(Self::new(Arc::new(s3), String::new()))
    }

    fn new(inner: Arc<dyn ObjectStore>, root: String) -> Self {
        let permits = Arc::new(Semaphore::new(TRANSFERS));
        Self {
            inner,
            root,
            permits,
        }
    }

    pub fn key(&self, rel: &str) -> Key {
        let rel = rel.trim_matches('/');
        Key::from(match (self.root.is_empty(), rel.is_empty()) {
            (true, _) => rel.to_string(),
            (false, true) => self.root.clone(),
            (false, false) => format!("{}/{rel}", self.root),
        })
    }

    pub async fn exists(&self, rel: &str) -> Result<bool> {
        match self.inner.head(&self.key(rel)).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Objects directly under `rel`, by file name, with their sizes.
    pub async fn list(&self, rel: &str) -> Result<Vec<(String, u64)>> {
        let prefix = self.key(rel);
        let listed = self.inner.list_with_delimiter(Some(&prefix)).await?;
        Ok(listed
            .objects
            .into_iter()
            .filter_map(|o| Some((o.location.filename()?.to_string(), o.size)))
            .collect())
    }

    pub async fn get(&self, rel: &str) -> Result<Vec<u8>> {
        Ok(self
            .inner
            .get(&self.key(rel))
            .await?
            .bytes()
            .await?
            .to_vec())
    }

    pub async fn get_range(&self, rel: &str, range: std::ops::Range<u64>) -> Result<Vec<u8>> {
        let _permit = self.permits.acquire().await?;
        Ok(self.inner.get_range(&self.key(rel), range).await?.to_vec())
    }

    pub async fn put(&self, rel: &str, data: Vec<u8>) -> Result<()> {
        self.inner
            .put(&self.key(rel), PutPayload::from(data))
            .await?;
        Ok(())
    }

    /// Uploads `local` to `rel`; it appears whole or not at all.
    pub async fn upload(&self, local: &Path, rel: &str) -> Result<()> {
        use tokio::io::AsyncReadExt;
        let size = std::fs::metadata(local)?.len();
        if size <= CHUNK {
            let _permit = self.permits.acquire().await?;
            return self.put(rel, tokio::fs::read(local).await?).await;
        }
        let mut file = tokio::fs::File::open(local).await?;
        let upload = self.inner.put_multipart(&self.key(rel)).await?;
        let mut writer = WriteMultipart::new_with_chunk_size(upload, CHUNK as usize);
        let mut buf = vec![0; CHUNK as usize];
        loop {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            writer.wait_for_capacity(4).await?;
            writer.write(&buf[..n]);
        }
        writer.finish().await?;
        Ok(())
    }

    /// Downloads `rel` (`size` bytes) to `local` with parallel range requests, through a
    /// `.part` file renamed into place.
    pub async fn download(&self, rel: &str, size: u64, local: &Path) -> Result<()> {
        if let Some(dir) = local.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let part = part(local);
        let file = Arc::new(std::fs::File::create(&part)?);
        file.set_len(size)?;
        let mut tasks = JoinSet::new();
        for start in (0..size).step_by(CHUNK as usize) {
            let (store, rel, file) = (self.clone(), rel.to_string(), file.clone());
            tasks.spawn(async move {
                let bytes = store
                    .get_range(&rel, start..(start + CHUNK).min(size))
                    .await?;
                tokio::task::spawn_blocking(move || file.write_all_at(&bytes, start)).await??;
                anyhow::Ok(())
            });
        }
        while let Some(r) = tasks.join_next().await {
            r??;
        }
        file.sync_all()?;
        std::fs::rename(&part, local)?;
        Ok(())
    }

    /// Publishes every file under `dir` to `rel`, then `MANIFEST` and `_COMPLETE`. Lock files
    /// and dot-directories (the house-number spill) are left out.
    pub async fn publish(&self, dir: &Path, rel: &str) -> Result<()> {
        let files = walk(dir)?;
        let entries = hash_all(dir, &files).await?;
        let mut tasks = JoinSet::new();
        for name in files {
            let (store, local, key) = (self.clone(), dir.join(&name), format!("{rel}/{name}"));
            tasks.spawn(async move { store.upload(&local, &key).await });
        }
        while let Some(r) = tasks.join_next().await {
            r??;
        }
        self.seal(rel, entries).await
    }

    /// Writes `MANIFEST` and then `_COMPLETE` for files already under `rel`.
    pub async fn seal(&self, rel: &str, files: Vec<Entry>) -> Result<()> {
        self.put(
            &format!("{rel}/MANIFEST"),
            serde_json::to_vec_pretty(&Manifest { files })?,
        )
        .await?;
        self.put(&format!("{rel}/_COMPLETE"), Vec::new()).await
    }

    /// Downloads a sealed directory into `dir`, keeping files that already match, and checks
    /// every file against `MANIFEST`.
    pub async fn fetch(&self, rel: &str, dir: &Path) -> Result<()> {
        ensure!(
            self.exists(&format!("{rel}/_COMPLETE")).await?,
            "{rel} is not complete"
        );
        let manifest: Manifest =
            serde_json::from_slice(&self.get(&format!("{rel}/MANIFEST")).await?)?;
        let mut tasks = JoinSet::new();
        for e in manifest.files {
            let (store, rel) = (self.clone(), rel.to_string());
            let local = dir.join(&e.path);
            tasks.spawn(async move {
                if !matches(&local, &e).await? {
                    store
                        .download(&format!("{rel}/{}", e.path), e.size, &local)
                        .await?;
                    ensure!(
                        matches(&local, &e).await?,
                        "{rel}/{} does not match its MANIFEST entry",
                        e.path
                    );
                }
                anyhow::Ok(())
            });
        }
        while let Some(r) = tasks.join_next().await {
            r??;
        }
        Ok(())
    }
}

pub fn part(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    path.with_file_name(name)
}

async fn matches(local: &Path, e: &Entry) -> Result<bool> {
    if std::fs::metadata(local).map_or(true, |m| m.len() != e.size) {
        return Ok(false);
    }
    let local = local.to_path_buf();
    Ok(tokio::task::spawn_blocking(move || sha256(&local)).await?? == e.sha256)
}

/// Relative paths of the files under `dir`, skipping lock files, `.part` files and
/// dot-directories.
pub fn walk(dir: &Path) -> Result<Vec<String>> {
    fn go(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
        for e in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            if e.file_type()?.is_dir() {
                if !name.starts_with('.') {
                    go(root, &path, out)?;
                }
            } else if !name.ends_with(".lock") && !name.ends_with(".part") {
                out.push(path.strip_prefix(root)?.to_string_lossy().into_owned());
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    go(dir, dir, &mut out)?;
    out.sort();
    Ok(out)
}

pub async fn hash_all(dir: &Path, files: &[String]) -> Result<Vec<Entry>> {
    let mut tasks = JoinSet::new();
    for name in files {
        let (path, name) = (dir.join(name), name.clone());
        tasks.spawn_blocking(move || {
            anyhow::Ok(Entry {
                size: std::fs::metadata(&path)?.len(),
                sha256: sha256(&path)?,
                path: name,
            })
        });
    }
    let mut entries = Vec::new();
    while let Some(r) = tasks.join_next().await {
        entries.push(r??);
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

pub fn sha256(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(ctx
        .finish()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_then_fetch_checks_every_file() {
        let base = std::env::temp_dir().join(format!("store-test-{}", std::process::id()));
        let (src, bucket, dst) = (base.join("src"), base.join("bucket"), base.join("dst"));
        std::fs::create_dir_all(src.join("nested")).unwrap();
        std::fs::create_dir_all(src.join(".numbers-spill")).unwrap();
        let big: Vec<u8> = (0..(CHUNK * 2 + 5)).map(|i| (i % 251) as u8).collect();
        std::fs::write(src.join("big.bin"), &big).unwrap();
        std::fs::write(src.join("nested/small.txt"), b"hello").unwrap();
        std::fs::write(src.join(".tantivy-meta.lock"), b"").unwrap();
        std::fs::write(src.join(".numbers-spill/0"), b"spill").unwrap();
        let store = Store::open(&format!("file://{}", bucket.display())).unwrap();
        assert!(store.fetch("indexes/x", &dst).await.is_err());
        store.publish(&src, "indexes/x").await.unwrap();
        store.fetch("indexes/x", &dst).await.unwrap();
        assert_eq!(std::fs::read(dst.join("big.bin")).unwrap(), big);
        assert_eq!(
            std::fs::read(dst.join("nested/small.txt")).unwrap(),
            b"hello"
        );
        assert!(!dst.join(".tantivy-meta.lock").exists());
        assert!(!dst.join(".numbers-spill").exists());
        // a corrupted object fails the check instead of being served
        std::fs::write(bucket.join("indexes/x/nested/small.txt"), b"HELLO").unwrap();
        std::fs::remove_file(dst.join("nested/small.txt")).unwrap();
        assert!(store.fetch("indexes/x", &dst).await.is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }
}
