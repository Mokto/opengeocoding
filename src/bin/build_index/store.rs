//! Object storage for sources, models and indexes: any S3-compatible store (`s3://bucket/prefix`,
//! configured by the standard `AWS_*` variables) or a local directory (`file:///path`).
//!
//! A directory is published as its files, then `MANIFEST` (paths, sizes, SHA-256), then
//! `_COMPLETE`; readers trust nothing without `_COMPLETE` and check every file against `MANIFEST`.
//!
//! A local directory store hard-links the files of a published or fetched directory instead of
//! copying them, when they share a file system: nothing writes such a file in place (the builder
//! recreates its output directory, readers only read), so the copies can share their blocks.
//! A linked file is checked by size only: it is the store's own file, hashed when it was published,
//! and rehashing a full index's sources reads about 135 GB, a quarter of an hour on a network disk.
use crate::progress::Progress;
use anyhow::{Context, Result, bail, ensure};
use object_store::{
    ClientConfigKey, ObjectStore, ObjectStoreExt, PutPayload, WriteMultipart,
    aws::{AmazonS3Builder, AmazonS3ConfigKey},
    local::LocalFileSystem,
    path::Path as Key,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::{
    os::unix::fs::FileExt,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{sync::Semaphore, task::JoinSet};

const CHUNK: u64 = 32 << 20;
const TRANSFERS: usize = 64;

#[derive(Clone)]
pub struct Store {
    inner: Arc<dyn ObjectStore>,
    root: String,
    permits: Arc<Semaphore>,
    local: Option<Arc<LocalFileSystem>>,
    mirror: Option<Arc<Store>>,
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
                let local = Arc::new(LocalFileSystem::new_with_prefix(dir)?);
                return Ok(Self {
                    local: Some(local.clone()),
                    ..Self::new(local, String::new())
                });
            } else if let Some(rest) = url.strip_prefix("s3://") {
                let (bucket, prefix) = rest.split_once('/').unwrap_or((rest, ""));
                ensure!(!bucket.is_empty(), "store URL {url} names no bucket");
                let s3 = AmazonS3Builder::from_env()
                    .with_bucket_name(bucket)
                    // The default 30 s covers the whole body: too short for a 32 MiB range when
                    // many parallel ranges share the pod's bandwidth. Set the key, not
                    // with_client_options: that replaces the options from_env read, including
                    // AWS_ALLOW_HTTP, and plain-http endpoints then fail with "URL scheme is not allowed".
                    .with_config(AmazonS3ConfigKey::Client(ClientConfigKey::Timeout), "300s")
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
            local: None,
            mirror: None,
        }
    }

    /// Reads a sealed `sources/` directory this store lacks from `mirror`, which is never
    /// written: anything else, unsealed source directories included, stays in this store.
    pub fn with_mirror(self, mirror: Store) -> Self {
        Self {
            mirror: Some(Arc::new(mirror)),
            ..self
        }
    }

    /// The store that reads of the directory `rel` go to.
    async fn reader(&self, rel: &str) -> Result<&Store> {
        let sealed = format!("{}/_COMPLETE", rel.trim_end_matches('/'));
        if let Some(m) = &self.mirror
            && rel.starts_with("sources/")
            && !self.has(&sealed).await?
            && m.has(&sealed).await?
        {
            return Ok(m);
        }
        Ok(self)
    }

    /// The file holding `rel`, when the store is a local directory.
    fn file(&self, rel: &str) -> Option<PathBuf> {
        self.local.as_ref()?.path_to_filesystem(&self.key(rel)).ok()
    }

    pub fn key(&self, rel: &str) -> Key {
        let rel = rel.trim_matches('/');
        Key::from(match (self.root.is_empty(), rel.is_empty()) {
            (true, _) => rel.to_string(),
            (false, true) => self.root.clone(),
            (false, false) => format!("{}/{rel}", self.root),
        })
    }

    /// Whether `rel` is in this store, or is the `_COMPLETE` of a source directory its mirror
    /// holds sealed.
    pub async fn exists(&self, rel: &str) -> Result<bool> {
        if self.has(rel).await? {
            return Ok(true);
        }
        match &self.mirror {
            Some(m) if rel.starts_with("sources/") && rel.ends_with("/_COMPLETE") => {
                m.has(rel).await
            }
            _ => Ok(false),
        }
    }

    async fn has(&self, rel: &str) -> Result<bool> {
        match self.inner.head(&self.key(rel)).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    /// Objects directly under `rel`, by file name, with their sizes.
    pub async fn list(&self, rel: &str) -> Result<Vec<(String, u64)>> {
        let from = self.reader(rel).await?;
        let prefix = from.key(rel);
        let listed = from.inner.list_with_delimiter(Some(&prefix)).await?;
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
    pub async fn upload(&self, local: &Path, rel: &str, progress: Option<&Progress>) -> Result<()> {
        use tokio::io::AsyncReadExt;
        let size = std::fs::metadata(local)?.len();
        if size <= CHUNK {
            let _permit = self.permits.acquire().await?;
            self.put(rel, tokio::fs::read(local).await?).await?;
            progress.inspect(|p| p.add(size));
            return Ok(());
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
            progress.inspect(|p| p.add(n as u64));
        }
        writer.finish().await?;
        Ok(())
    }

    /// Downloads `rel` (`size` bytes) to `local` with parallel range requests, through a
    /// `.part` file renamed into place, and returns the SHA-256 of what was written. The hash
    /// is computed as the leading chunks land, so no second pass over the file follows.
    pub async fn download(
        &self,
        rel: &str,
        size: u64,
        local: &Path,
        progress: Option<&Progress>,
    ) -> Result<String> {
        if let Some(dir) = local.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let part = part(local);
        let file = Arc::new(
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&part)?,
        );
        file.set_len(size)?;
        let chunks = size.div_ceil(CHUNK);
        let (done, mut landed) = tokio::sync::mpsc::unbounded_channel::<u64>();
        let hasher = tokio::spawn({
            let file = file.clone();
            async move {
                let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
                let (mut pending, mut next) = (BTreeSet::new(), 0);
                while next < chunks {
                    pending.insert(landed.recv().await.context("download ended early")?);
                    while pending.remove(&next) {
                        let file = file.clone();
                        let (start, end) = (next * CHUNK, ((next + 1) * CHUNK).min(size));
                        ctx = tokio::task::spawn_blocking(move || {
                            hash_range(&file, start, end, &mut ctx)?;
                            anyhow::Ok(ctx)
                        })
                        .await??;
                        next += 1;
                    }
                }
                anyhow::Ok(hex(ctx.finish()))
            }
        });
        let mut tasks = JoinSet::new();
        for i in 0..chunks {
            let (store, rel, file, done) =
                (self.clone(), rel.to_string(), file.clone(), done.clone());
            tasks.spawn(async move {
                let start = i * CHUNK;
                let bytes = store
                    .get_range(&rel, start..(start + CHUNK).min(size))
                    .await?;
                let len = bytes.len() as u64;
                tokio::task::spawn_blocking(move || file.write_all_at(&bytes, start)).await??;
                let _ = done.send(i);
                anyhow::Ok(len)
            });
        }
        drop(done);
        let mut landed_chunks = 0;
        while let Some(r) = tasks.join_next().await {
            let len = r??;
            landed_chunks += 1;
            if let Some(p) = progress {
                p.add(len);
            } else if chunks > 1
                // Single-chunk files finish at once; a line each would only be noise.
                && landed_chunks * 10 / chunks > (landed_chunks - 1) * 10 / chunks
            {
                let got = (landed_chunks * CHUNK).min(size);
                println!(
                    "{rel}: {}% ({} of {} MiB)",
                    landed_chunks * 100 / chunks,
                    got >> 20,
                    size >> 20
                );
            }
        }
        let sha = hasher.await??;
        // Not fsynced: the volume is disposable, and a torn file fails its hash on the next pull.
        std::fs::rename(&part, local)?;
        Ok(sha)
    }

    /// Publishes every file under `dir` to `rel`, then `MANIFEST` and `_COMPLETE`. Lock files
    /// and dot-directories (the house-number spill) are left out.
    pub async fn publish(&self, dir: &Path, rel: &str) -> Result<()> {
        let files = walk(dir)?;
        println!("  {rel}: hashing {} files", files.len());
        let entries = hash_all(dir, &files).await?;
        let progress = Progress::new(
            format!("upload {rel}"),
            entries.iter().map(|e| e.size).sum(),
        );
        let mut tasks = JoinSet::new();
        for name in files {
            let (store, local, key) = (self.clone(), dir.join(&name), format!("{rel}/{name}"));
            let progress = progress.clone();
            tasks.spawn(async move {
                if let Some(file) = store.file(&key)
                    && link(&local, &file).is_ok()
                {
                    progress.add(std::fs::metadata(&local)?.len());
                    return Ok(());
                }
                store.upload(&local, &key, Some(&progress)).await
            });
        }
        while let Some(r) = tasks.join_next().await {
            r??;
        }
        println!("  {rel}: sealing");
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
    /// every file against `MANIFEST`. Once a pass completes, a marker beside `dir` records the
    /// `MANIFEST` it verified; while that still names the current `MANIFEST`, a file of the right
    /// size is kept without rehashing it.
    pub async fn fetch(&self, rel: &str, dir: &Path) -> Result<()> {
        self.fetch_files(rel, dir, None).await
    }

    /// [`Self::fetch`], of only the files named in `only` when given.
    pub async fn fetch_files(
        &self,
        rel: &str,
        dir: &Path,
        only: Option<&std::collections::BTreeSet<String>>,
    ) -> Result<()> {
        let from = self.reader(rel).await?;
        ensure!(
            from.has(&format!("{rel}/_COMPLETE")).await?,
            "{rel} is not complete"
        );
        let raw = from.get(&format!("{rel}/MANIFEST")).await?;
        let digest = hex(ring::digest::digest(&ring::digest::SHA256, &raw));
        // the marker vouches for the whole directory, which a partial fetch does not check
        let marker = marker(dir).filter(|_| only.is_none());
        let trusted = marker
            .as_ref()
            .is_some_and(|m| std::fs::read_to_string(m).is_ok_and(|d| d == digest));
        let mut manifest: Manifest = serde_json::from_slice(&raw)?;
        if let Some(only) = only {
            manifest.files.retain(|e| only.contains(&e.path));
        }
        // Largest first, so the biggest files' hashing overlaps the rest of the transfer.
        manifest.files.sort_by_key(|e| std::cmp::Reverse(e.size));
        // Files already on disk count as done at once, so the percentage covers the directory.
        let progress = Progress::new(
            format!("download {rel}"),
            manifest.files.iter().map(|e| e.size).sum(),
        );
        let mut tasks = JoinSet::new();
        for e in manifest.files {
            let (store, rel, progress) = (from.clone(), rel.to_string(), progress.clone());
            let local = dir.join(&e.path);
            tasks.spawn(async move {
                let key = format!("{rel}/{}", e.path);
                if matches(&local, &e, trusted).await? {
                    progress.skip(e.size);
                } else {
                    let ok = match store.file(&key).filter(|f| link(f, &local).is_ok()) {
                        Some(_) => {
                            progress.add(e.size);
                            std::fs::metadata(&local)?.len() == e.size
                        }
                        None => {
                            store
                                .download(&key, e.size, &local, Some(&progress))
                                .await?
                                == e.sha256
                        }
                    };
                    if !ok {
                        let _ = std::fs::remove_file(&local);
                        bail!("{rel}/{} does not match its MANIFEST entry", e.path);
                    }
                }
                anyhow::Ok(())
            });
        }
        while let Some(r) = tasks.join_next().await {
            r??;
        }
        // Best effort: without the marker the next fetch only rehashes.
        if let Some(m) = marker {
            let _ = std::fs::write(m, digest);
        }
        Ok(())
    }
}

/// Beside `dir`, not in it: the directory's readers and `walk` must not see it.
fn marker(dir: &Path) -> Option<PathBuf> {
    let name = dir.file_name()?.to_string_lossy();
    Some(dir.with_file_name(format!(".{name}.verified")))
}

/// Hard-links `from` at `to` through a `.part` name, so `to` appears whole or not at all.
fn link(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let part = part(to);
    let _ = std::fs::remove_file(&part);
    std::fs::hard_link(from, &part)?;
    std::fs::rename(&part, to)
}

pub fn part(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    path.with_file_name(name)
}

async fn matches(local: &Path, e: &Entry, trusted: bool) -> Result<bool> {
    if std::fs::metadata(local).map_or(true, |m| m.len() != e.size) {
        return Ok(false);
    }
    if trusted {
        return Ok(true);
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

fn hex(digest: ring::digest::Digest) -> String {
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn hash_range(
    file: &std::fs::File,
    start: u64,
    end: u64,
    ctx: &mut ring::digest::Context,
) -> Result<()> {
    let mut buf = vec![0; 1 << 20];
    let mut at = start;
    while at < end {
        let n = ((end - at) as usize).min(buf.len());
        file.read_exact_at(&mut buf[..n], at)?;
        ctx.update(&buf[..n]);
        at += n as u64;
    }
    Ok(())
}

pub fn sha256(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    hash_range(&file, 0, file.metadata()?.len(), &mut ctx)?;
    Ok(hex(ctx.finish()))
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
        std::fs::write(src.join("empty"), b"").unwrap();
        std::fs::write(src.join(".tantivy-meta.lock"), b"").unwrap();
        std::fs::write(src.join(".numbers-spill/0"), b"spill").unwrap();
        let store = Store::open(&format!("file://{}", bucket.display())).unwrap();
        assert!(store.fetch("indexes/x", &dst).await.is_err());
        store.publish(&src, "indexes/x").await.unwrap();
        store.fetch("indexes/x", &dst).await.unwrap();
        assert_eq!(std::fs::read(dst.join("big.bin")).unwrap(), big);
        // a local store links the files, it does not copy them
        use std::os::unix::fs::MetadataExt;
        let inode = |p: PathBuf| std::fs::metadata(p).unwrap().ino();
        let stored = bucket.join("indexes/x/big.bin");
        assert_eq!(inode(src.join("big.bin")), inode(stored.clone()));
        assert_eq!(inode(dst.join("big.bin")), inode(stored));
        // so damage is simulated by replacing a file, as a writer of a new file would
        let damage = |p: &Path, data: &[u8]| {
            std::fs::remove_file(p).unwrap();
            std::fs::write(p, data).unwrap();
        };
        assert_eq!(
            std::fs::read(dst.join("nested/small.txt")).unwrap(),
            b"hello"
        );
        assert!(!dst.join(".tantivy-meta.lock").exists());
        assert!(!dst.join(".numbers-spill").exists());
        // a verified directory is trusted by size: same-size damage survives until the marker goes
        let kept = dst.join("nested/small.txt");
        damage(&kept, b"HELLO");
        store.fetch("indexes/x", &dst).await.unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"HELLO");
        std::fs::remove_file(base.join(".dst.verified")).unwrap();
        store.fetch("indexes/x", &dst).await.unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"hello");
        // a size change is repaired even with the marker
        damage(&kept, b"hi");
        store.fetch("indexes/x", &dst).await.unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"hello");
        // a linked object is trusted by size: same-size damage is served, a size change fails
        damage(&bucket.join("indexes/x/nested/small.txt"), b"HELLO");
        std::fs::remove_file(dst.join("nested/small.txt")).unwrap();
        store.fetch("indexes/x", &dst).await.unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"HELLO");
        damage(&bucket.join("indexes/x/nested/small.txt"), b"HELLO!");
        std::fs::remove_file(dst.join("nested/small.txt")).unwrap();
        assert!(store.fetch("indexes/x", &dst).await.is_err());
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[tokio::test]
    async fn s3_store_honours_aws_allow_http() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for mut conn in listener.incoming().flatten() {
                let _ = conn.read(&mut [0; 4096]);
                let _ = conn.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
            }
        });
        // SAFETY: no other test in this binary reads or writes the environment.
        unsafe {
            std::env::set_var("AWS_ENDPOINT", endpoint);
            std::env::set_var("AWS_ALLOW_HTTP", "true");
            std::env::set_var("AWS_ACCESS_KEY_ID", "k");
            std::env::set_var("AWS_SECRET_ACCESS_KEY", "s");
            std::env::set_var("AWS_DEFAULT_REGION", "us-east-1");
        }
        let store = Store::open("s3://bucket").unwrap();
        assert!(!store.exists("indexes/x/_COMPLETE").await.unwrap());
    }
}
