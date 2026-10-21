//! Fetches the build's sources into a scratch directory, mirroring each into the store as it
//! arrives: a later build, or a rerun after a crash, reads the store instead of upstream. Each
//! source directory is sealed (`MANIFEST`, `_COMPLETE`) once whole, after which upstream is not
//! consulted again for it.
use crate::progress::Progress;
use crate::store::{self, Store};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::{BufRead, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, sync::Semaphore, task::JoinSet};

/// What to build from, as the chart's `index.sources`. Everything here is part of the index id.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sources {
    pub overture_release: String,
    /// Labels one fetch of the sources that only publish "latest" (OSM, OpenAddresses,
    /// GeoNames); a new label fetches them again.
    pub snapshot: String,
    /// `[min lon, min lat, max lon, max lat]`; empty for the world.
    #[serde(default)]
    pub bbox: Vec<f64>,
    /// Geofabrik extract ids, such as `europe/germany` or `liechtenstein`.
    #[serde(default)]
    pub osm: Vec<String>,
    /// Geofabrik extract ids read for landmarks only: countries whose addresses come from
    /// Overture, where OSM's would change the answers.
    #[serde(default)]
    pub osm_landmarks: Vec<String>,
    /// Retired switches (`openaddresses.enabled` and `anyLicence`, `geonamesPlaces`): every source
    /// is always read, and OpenAddresses layers always pass the licence check. Accepted and
    /// ignored so a source list that still has them parses.
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    pub openaddresses: serde::de::IgnoredAny,
    /// GeoNames postal-code exports, such as `allCountries` or `GB_full.csv`; earlier ones win.
    #[serde(default)]
    pub geonames_postal: Vec<String>,
    #[serde(default, skip_serializing)]
    #[allow(dead_code)]
    pub geonames_places: serde::de::IgnoredAny,
}

/// Local paths of the fetched sources, for `build_index`.
pub struct Fetched {
    pub overture: PathBuf,
    pub osm: Option<PathBuf>,
    pub osm_landmarks: Option<PathBuf>,
    pub openaddresses: Option<PathBuf>,
    pub geonames: Vec<PathBuf>,
    /// The directory holding `cities500.txt` and `admin1CodesASCII.txt`.
    pub geonames_places: Option<PathBuf>,
}

fn safe(what: &str, value: &str, extra: &str) -> Result<()> {
    ensure!(
        !value.is_empty()
            && !value.starts_with(['.', '/'])
            && !value.contains("..")
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c) || extra.contains(c)),
        "invalid {what}: {value:?}"
    );
    Ok(())
}

impl Sources {
    pub fn validate(&self) -> Result<()> {
        safe("overtureRelease", &self.overture_release, "")?;
        safe("snapshot", &self.snapshot, "")?;
        for id in self.osm.iter().chain(&self.osm_landmarks) {
            safe("osm extract", id, "/")?;
        }
        for name in &self.geonames_postal {
            safe("geonamesPostal entry", name, "")?;
        }
        if let Some(b) = self.bbox() {
            ensure!(
                (-180.0..=180.0).contains(&b[0])
                    && (-180.0..=180.0).contains(&b[2])
                    && (-90.0..=90.0).contains(&b[1])
                    && (-90.0..=90.0).contains(&b[3])
                    && b[0] < b[2]
                    && b[1] < b[3],
                "bbox must be [min lon, min lat, max lon, max lat], got {:?}",
                self.bbox
            );
        } else {
            ensure!(
                self.bbox.is_empty(),
                "bbox needs four numbers, got {:?}",
                self.bbox
            );
        }
        Ok(())
    }

    pub fn bbox(&self) -> Option<[f64; 4]> {
        self.bbox.as_slice().try_into().ok()
    }

    /// Names the extent in store keys, so a bbox build never reuses a world mirror or the reverse.
    fn scope(&self) -> String {
        match self.bbox() {
            None => "world".into(),
            Some(b) => format!("bbox_{}_{}_{}_{}", b[0], b[1], b[2], b[3]),
        }
    }
}

fn intersects(b: [f64; 4], xmin: f64, ymin: f64, xmax: f64, ymax: f64) -> bool {
    xmin <= b[2] && xmax >= b[0] && ymin <= b[3] && ymax >= b[1]
}

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!(
            "opengeocoding-build_index/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/Mokto/opengeocoding)"
        ))
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(300))
        .build()?)
}

/// Downloads `url` to `dest`, resuming a `.part` file after a dropped connection. `Ok(false)` if
/// the server has no such file.
pub async fn http_get(client: &reqwest::Client, url: &str, dest: &Path) -> Result<bool> {
    let part = store::part(dest);
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut last = String::new();
    for attempt in 0..8u64 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(5 * attempt)).await;
        }
        let have = std::fs::metadata(&part).map_or(0, |m| m.len());
        let mut request = client.get(url);
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                last = e.to_string();
                continue;
            }
        };
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            std::fs::remove_file(&part)?;
            continue;
        }
        if !status.is_success() {
            last = format!("HTTP {status}");
            continue;
        }
        let append = status == reqwest::StatusCode::PARTIAL_CONTENT;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(&part)
            .await?;
        let done = loop {
            match response.chunk().await {
                Ok(Some(chunk)) => file.write_all(&chunk).await?,
                Ok(None) => break true,
                Err(e) => {
                    last = e.to_string();
                    break false;
                }
            }
        };
        file.flush().await?;
        if done {
            std::fs::rename(&part, dest)?;
            return Ok(true);
        }
    }
    bail!("download {url}: {last}")
}

/// Makes `dir` hold the sealed store directory `rel`. Unless it is sealed already, each item
/// already in the store is downloaded and the rest come from `produce`, which writes the named
/// file (or returns `Ok(false)` to leave it out) and is then stored, so a rerun resumes.
async fn mirror<T, F, Fut>(
    store: &Store,
    rel: &str,
    dir: &Path,
    parallel: usize,
    items: impl FnOnce() -> Fut,
    produce: F,
) -> Result<()>
where
    T: Send + 'static,
    F: Fn(T, PathBuf) -> std::pin::Pin<Box<dyn Future<Output = Result<bool>> + Send>>
        + Send
        + Sync
        + 'static,
    Fut: Future<Output = Result<Vec<(String, T)>>>,
{
    std::fs::create_dir_all(dir)?;
    if store.exists(&format!("{rel}/_COMPLETE")).await? {
        println!("{rel}: from the store");
        store.fetch(rel, dir).await?;
        let keep = store.list(rel).await?.into_iter().map(|(n, _)| n).collect();
        return prune(dir, &keep);
    }
    let stored: BTreeMap<String, u64> = store.list(rel).await?.into_iter().collect();
    let items = items().await?;
    println!(
        "{rel}: {} items, {} already stored",
        items.len(),
        items.iter().filter(|(n, _)| stored.contains_key(n)).count()
    );
    let done = Progress::items(rel.to_string(), items.len() as u64, "files");
    let (produce, permits) = (Arc::new(produce), Arc::new(Semaphore::new(parallel)));
    let mut tasks = JoinSet::new();
    for (name, item) in items {
        let (store, produce, permits) = (store.clone(), produce.clone(), permits.clone());
        let done = done.clone();
        let (key, local, size) = (
            format!("{rel}/{name}"),
            dir.join(&name),
            stored.get(&name).copied(),
        );
        tasks.spawn(async move {
            let _permit = permits.acquire().await?;
            let kept = if let Some(size) = size {
                store.download(&key, size, &local, None).await?;
                true
            } else if produce(item, local.clone())
                .await
                .with_context(|| key.clone())?
            {
                store.upload(&local, &key, None).await?;
                true
            } else {
                false
            };
            done.add(1);
            anyhow::Ok(kept.then_some(name))
        });
    }
    let mut kept = BTreeSet::new();
    while let Some(r) = tasks.join_next().await {
        kept.extend(r??);
    }
    prune(dir, &kept)?;
    let files = store::walk(dir)?;
    store.seal(rel, store::hash_all(dir, &files).await?).await?;
    println!("{rel}: sealed, {} files", files.len());
    Ok(())
}

/// Deletes the files in `dir` that `keep` does not name. A kept scratch volume still holds an
/// earlier build's sources (another Overture release, a dropped extract or layer); left there,
/// the builder would read them and a new mirror's MANIFEST would name files never uploaded.
fn prune(dir: &Path, keep: &BTreeSet<String>) -> Result<()> {
    for f in store::walk(dir)? {
        if !keep.contains(&f) {
            std::fs::remove_file(dir.join(&f))?;
        }
    }
    Ok(())
}

const OVERTURE_TYPES: [(&str, &str); 5] = [
    ("division", "theme=divisions/type=division"),
    ("division_area", "theme=divisions/type=division_area"),
    ("segment", "theme=transportation/type=segment"),
    ("address", "theme=addresses/type=address"),
    ("place", "theme=places/type=place"),
];

pub async fn fetch(store: &Store, src: &Sources, scratch: &Path) -> Result<Fetched> {
    let client = client()?;
    let overture = Store::overture()?;
    let local = scratch.join("overture");
    // Each upstream has its own limit (Overture and OpenAddresses share the path to AWS, Geofabrik is
    // kept to two files at a time), so they run side by side. OpenAddresses needs Overture's addresses.
    let overture_then_openaddresses = async {
        for (dir, upstream) in OVERTURE_TYPES {
            let prefix = format!("release/{}/{upstream}", src.overture_release);
            let bbox = src.bbox();
            let up = overture.clone();
            let list = up.clone();
            mirror(
                store,
                &format!(
                    "sources/overture/{}/{}/{dir}",
                    src.overture_release,
                    src.scope()
                ),
                &local.join(dir),
                4,
                || async move {
                    let files = list.list(&prefix).await?;
                    ensure!(!files.is_empty(), "no Overture files under {prefix}");
                    Ok(files
                        .into_iter()
                        .filter(|(n, _)| n.ends_with(".parquet"))
                        .map(|(n, size)| (n.clone(), (format!("{prefix}/{n}"), size)))
                        .collect())
                },
                move |(key, size): (String, u64), path: PathBuf| {
                    let up = up.clone();
                    Box::pin(async move {
                        match bbox {
                            None => up.download(&key, size, &path, None).await.map(|_| true),
                            Some(b) => overture_within(&up, &key, size, b, &path).await,
                        }
                    })
                },
            )
            .await?;
        }

        let dir = scratch.join("openaddresses");
        openaddresses(store, src, &local.join("address"), &dir).await?;

        anyhow::Ok(Some(dir))
    };
    let osm_files = async {
        let osm = geofabrik(store, &client, src, "osm", &src.osm, scratch).await?;
        let landmarks = geofabrik(
            store,
            &client,
            src,
            "osm-landmarks",
            &src.osm_landmarks,
            scratch,
        )
        .await?;
        anyhow::Ok((osm, landmarks))
    };
    let geonames_files = async {
        let mut geonames = Vec::new();
        for name in &src.geonames_postal {
            let dir = scratch.join("geonames");
            let (zip, entry) = match name.strip_suffix(".csv") {
                Some(stem) => (format!("{name}.zip"), format!("{stem}.txt")),
                None => (format!("{name}.zip"), format!("{name}.txt")),
            };
            let bbox = src.bbox();
            let c = self::client()?;
            let file = entry.clone();
            mirror(
                store,
                &format!("sources/{}/geonames/{}/{name}", src.snapshot, src.scope()),
                &dir.join(name),
                1,
                || async move { Ok(vec![(file, ())]) },
                move |(), path: PathBuf| {
                    let (c, zip, entry) = (c.clone(), zip.clone(), entry.clone());
                    Box::pin(async move {
                        let url = format!("https://download.geonames.org/export/zip/{zip}");
                        let tmp = path.with_extension("zip");
                        ensure!(http_get(&c, &url, &tmp).await?, "{url} not found");
                        let out = path.clone();
                        tokio::task::spawn_blocking(move || {
                            let keep = |line: &str| {
                                let f: Vec<&str> = line.split('\t').collect();
                                let (Some(lat), Some(lon)) = (
                                    f.get(9).and_then(|v| v.parse::<f64>().ok()),
                                    f.get(10).and_then(|v| v.parse::<f64>().ok()),
                                ) else {
                                    return false;
                                };
                                bbox.is_none_or(|b| intersects(b, lon, lat, lon, lat))
                            };
                            unzip_lines(&tmp, &entry, &out, keep)?;
                            std::fs::remove_file(&tmp)?;
                            anyhow::Ok(())
                        })
                        .await??;
                        Ok(true)
                    })
                },
            )
            .await?;
            geonames.push(
                dir.join(name)
                    .join(name.strip_suffix(".csv").unwrap_or(name).to_string() + ".txt"),
            );
        }

        anyhow::Ok(geonames)
    };
    let geonames_places = async {
        let dir = scratch.join("geonames-places");
        let bbox = src.bbox();
        let c = self::client()?;
        mirror(
            store,
            &format!("sources/{}/geonames-places/{}", src.snapshot, src.scope()),
            &dir,
            1,
            || async {
                Ok(vec![
                    ("cities500.txt".to_string(), true),
                    ("admin1CodesASCII.txt".to_string(), false),
                ])
            },
            move |zipped: bool, path: PathBuf| {
                let c = c.clone();
                Box::pin(async move {
                    let base = "https://download.geonames.org/export/dump";
                    if !zipped {
                        let url = format!("{base}/admin1CodesASCII.txt");
                        ensure!(http_get(&c, &url, &path).await?, "{url} not found");
                        return Ok(true);
                    }
                    let url = format!("{base}/cities500.zip");
                    let tmp = path.with_extension("zip");
                    ensure!(http_get(&c, &url, &tmp).await?, "{url} not found");
                    tokio::task::spawn_blocking(move || {
                        let keep = |line: &str| {
                            let f: Vec<&str> = line.split('\t').collect();
                            let (Some(lat), Some(lon)) = (
                                f.get(4).and_then(|v| v.parse::<f64>().ok()),
                                f.get(5).and_then(|v| v.parse::<f64>().ok()),
                            ) else {
                                return false;
                            };
                            bbox.is_none_or(|b| intersects(b, lon, lat, lon, lat))
                        };
                        unzip_lines(&tmp, "cities500.txt", &path, keep)?;
                        std::fs::remove_file(&tmp)?;
                        anyhow::Ok(())
                    })
                    .await??;
                    Ok(true)
                })
            },
        )
        .await?;
        anyhow::Ok(Some(dir))
    };
    let (openaddresses, (osm, osm_landmarks), geonames, geonames_places) = tokio::try_join!(
        overture_then_openaddresses,
        osm_files,
        geonames_files,
        geonames_places
    )?;

    Ok(Fetched {
        overture: local,
        osm,
        osm_landmarks,
        openaddresses,
        geonames,
        geonames_places,
    })
}

/// Mirrors the Geofabrik extracts `ids` into `scratch/<name>`, through `sources/<snapshot>/<name>`.
async fn geofabrik(
    store: &Store,
    client: &reqwest::Client,
    src: &Sources,
    name: &str,
    ids: &[String],
    scratch: &Path,
) -> Result<Option<PathBuf>> {
    if ids.is_empty() {
        return Ok(None);
    }
    let dir = scratch.join(name);
    let file = |id: &String| format!("{}.osm.pbf", id.replace('/', "_"));
    // Once a snapshot's mirror is sealed it is never refilled, but an extract can move between
    // the address and landmark lists: it is then taken from the other list's mirror of the
    // same fetch. One neither holds is fetched on its own into `geofabrik/<file>`, so adding an
    // extract costs that extract, not a new label that fetches every source again.
    let rel = |list: &str| format!("sources/{}/{list}", src.snapshot);
    if store.exists(&format!("{}/_COMPLETE", rel(name))).await? {
        std::fs::create_dir_all(&dir)?;
        let mut missing: BTreeSet<String> = ids.iter().map(file).collect();
        let wanted = missing.clone();
        for list in [
            name,
            if name == "osm" {
                "osm-landmarks"
            } else {
                "osm"
            },
        ] {
            if missing.is_empty() || !store.exists(&format!("{}/_COMPLETE", rel(list))).await? {
                continue;
            }
            let held: BTreeSet<String> = store
                .list(&rel(list))
                .await?
                .into_iter()
                .map(|(n, _)| n)
                .filter(|n| missing.contains(n))
                .collect();
            if !held.is_empty() {
                println!("{}: {} extracts from the store", rel(list), held.len());
                store.fetch_files(&rel(list), &dir, Some(&held)).await?;
                missing.retain(|n| !held.contains(n));
            }
        }
        let mut upstream = Vec::new();
        for f in missing {
            let added = format!("sources/{}/geofabrik/{f}", src.snapshot);
            if store.exists(&format!("{added}/_COMPLETE")).await? {
                println!("{added}: from the store");
                store
                    .fetch_files(&added, &dir, Some(&BTreeSet::from([f])))
                    .await?;
            } else {
                upstream.push((f, added));
            }
        }
        if !upstream.is_empty() {
            let urls = geofabrik_urls(client).await?;
            for (f, added) in upstream {
                let id = ids.iter().find(|id| file(id) == f).expect("named by ids");
                let url = urls
                    .get(id)
                    .with_context(|| format!("no Geofabrik extract {id}"))?;
                add_extract(store, client, url, &added, &dir, &f).await?;
            }
        }
        prune(&dir, &wanted)?;
        return Ok(Some(dir));
    }
    let ids = ids.to_vec();
    let (c, client) = (client.clone(), client.clone());
    mirror(
        store,
        &format!("sources/{}/{name}", src.snapshot),
        &dir,
        2,
        || async move {
            let urls = geofabrik_urls(&c).await?;
            ids.iter()
                .map(|id| {
                    let url = urls
                        .get(id)
                        .with_context(|| format!("no Geofabrik extract {id}"))?;
                    Ok((format!("{}.osm.pbf", id.replace('/', "_")), url.clone()))
                })
                .collect()
        },
        move |url: String, path: PathBuf| {
            let c = client.clone();
            Box::pin(async move {
                ensure!(http_get(&c, &url, &path).await?, "{url} not found");
                Ok(true)
            })
        },
    )
    .await?;
    Ok(Some(dir))
}

/// Geofabrik's extract ids and their `.osm.pbf` URLs.
async fn geofabrik_urls(client: &reqwest::Client) -> Result<BTreeMap<String, String>> {
    let index: serde_json::Value = client
        .get("https://download.geofabrik.de/index-v1-nogeom.json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(index["features"]
        .as_array()
        .context("Geofabrik index")?
        .iter()
        .filter_map(|f| {
            Some((
                f["properties"]["id"].as_str()?.to_string(),
                f["properties"]["urls"]["pbf"].as_str()?.to_string(),
            ))
        })
        .collect())
}

/// Downloads one extract into `dir/name` and seals it alone at `rel`, so the next build with
/// the same snapshot takes it from the store.
async fn add_extract(
    store: &Store,
    client: &reqwest::Client,
    url: &str,
    rel: &str,
    dir: &Path,
    name: &str,
) -> Result<()> {
    let local = dir.join(name);
    ensure!(http_get(client, url, &local).await?, "{url} not found");
    store.upload(&local, &format!("{rel}/{name}"), None).await?;
    store
        .seal(rel, store::hash_all(dir, &[name.to_string()]).await?)
        .await?;
    println!("{rel}: sealed, from {url}");
    Ok(())
}

/// Copies the row groups of an Overture GeoParquet file whose `bbox` statistics meet `b` into
/// `path`, unchanged and without decoding them. Overture sorts rows spatially, so a small area
/// reads few row groups. `Ok(false)` if none meet it.
async fn overture_within(
    up: &Store,
    key: &str,
    size: u64,
    b: [f64; 4],
    path: &Path,
) -> Result<bool> {
    use parquet::file::{metadata::ParquetMetaDataReader, statistics::Statistics};
    ensure!(size > 12, "{key}: too small for parquet");
    let tail = up.get_range(key, size - 8..size).await?;
    ensure!(&tail[4..] == b"PAR1", "{key}: not parquet");
    let len = u32::from_le_bytes(tail[..4].try_into()?) as u64;
    let footer = up.get_range(key, size - 8 - len..size - 8).await?;
    let meta = ParquetMetaDataReader::decode_metadata(&footer)?;
    let schema = meta.file_metadata().schema_descr();
    let column = |name: &str| {
        (0..schema.num_columns())
            .find(|&i| schema.column(i).path().string() == name)
            .with_context(|| format!("{key}: no {name} column"))
    };
    let cols = [
        column("bbox.xmin")?,
        column("bbox.ymin")?,
        column("bbox.xmax")?,
        column("bbox.ymax")?,
    ];
    let stat =
        |rg: &parquet::file::metadata::RowGroupMetaData, i: usize, max: bool| -> Option<f64> {
            match rg.column(cols[i]).statistics()? {
                Statistics::Float(s) => {
                    (if max { s.max_opt() } else { s.min_opt() }).map(|v| *v as f64)
                }
                Statistics::Double(s) => (if max { s.max_opt() } else { s.min_opt() }).copied(),
                _ => None,
            }
        };
    let keep: Vec<usize> = (0..meta.num_row_groups())
        .filter(|&i| {
            let rg = meta.row_group(i);
            match (
                stat(rg, 0, false),
                stat(rg, 1, false),
                stat(rg, 2, true),
                stat(rg, 3, true),
            ) {
                (Some(x0), Some(y0), Some(x1), Some(y1)) => intersects(b, x0, y0, x1, y1),
                // without statistics the row group can't be ruled out
                _ => true,
            }
        })
        .collect();
    if keep.is_empty() {
        return Ok(false);
    }
    // The kept row groups' bytes land at their original offsets in a sparse copy, so the column
    // chunk metadata can be copied as is.
    let sparse = path.with_extension("sparse");
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = Arc::new(std::fs::File::create(&sparse)?);
    file.set_len(size)?;
    let mut tasks = JoinSet::new();
    for &i in &keep {
        let rg = meta.row_group(i);
        let start = rg
            .columns()
            .iter()
            .map(|c| c.dictionary_page_offset().unwrap_or(c.data_page_offset()) as u64)
            .min()
            .unwrap_or(0);
        let end = rg
            .columns()
            .iter()
            .map(|c| {
                c.dictionary_page_offset().unwrap_or(c.data_page_offset()) as u64
                    + c.compressed_size() as u64
            })
            .max()
            .unwrap_or(0);
        let (up, key, file) = (up.clone(), key.to_string(), file.clone());
        tasks.spawn(async move {
            use std::os::unix::fs::FileExt;
            let bytes = up.get_range(&key, start..end).await?;
            tokio::task::spawn_blocking(move || file.write_all_at(&bytes, start)).await??;
            anyhow::Ok(())
        });
    }
    while let Some(r) = tasks.join_next().await {
        r??;
    }
    let (out, sparse_path) = (path.to_path_buf(), sparse.clone());
    tokio::task::spawn_blocking(move || {
        use parquet::{
            column::writer::ColumnCloseResult,
            file::{properties::WriterProperties, writer::SerializedFileWriter},
        };
        let input = std::fs::File::open(&sparse_path)?;
        let part = store::part(&out);
        let mut writer = SerializedFileWriter::new(
            std::fs::File::create(&part)?,
            meta.file_metadata().schema_descr().root_schema_ptr(),
            Arc::new(WriterProperties::builder().build()),
        )?;
        for &i in &keep {
            let rg = meta.row_group(i);
            let mut rg_out = writer.next_row_group()?;
            for column in rg.columns() {
                rg_out.append_column(
                    &input,
                    ColumnCloseResult {
                        bytes_written: column.compressed_size() as _,
                        rows_written: rg.num_rows() as _,
                        metadata: column.clone(),
                        bloom_filter: None,
                        column_index: None,
                        offset_index: None,
                    },
                )?;
            }
            rg_out.close()?;
        }
        writer.close()?;
        std::fs::rename(&part, &out)?;
        std::fs::remove_file(&sparse_path)?;
        anyhow::Ok(())
    })
    .await??;
    Ok(true)
}

/// Writes the lines of zip entry `name` that `keep` accepts to `out`. Stored and deflated
/// entries only, without ZIP64, as GeoNames publishes them.
fn unzip_lines(zip: &Path, name: &str, out: &Path, keep: impl Fn(&str) -> bool) -> Result<()> {
    let mut f = std::fs::File::open(zip)?;
    let len = f.metadata()?.len();
    let tail_len = len.min(65_557);
    f.seek(SeekFrom::End(-(tail_len as i64)))?;
    let mut tail = vec![0; tail_len as usize];
    f.read_exact(&mut tail)?;
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]) as u64;
    let u32_at = |b: &[u8], i: usize| u32::from_le_bytes(b[i..i + 4].try_into().unwrap()) as u64;
    let end = tail
        .windows(4)
        .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
        .context("not a zip file")?;
    let (cd_size, cd_offset) = (u32_at(&tail, end + 12), u32_at(&tail, end + 16));
    f.seek(SeekFrom::Start(cd_offset))?;
    let mut cd = vec![0; cd_size as usize];
    f.read_exact(&mut cd)?;
    let mut i = 0;
    while i + 46 <= cd.len() && u32_at(&cd, i) == 0x0201_4b50 {
        let (method, csize) = (u16_at(&cd, i + 10), u32_at(&cd, i + 20));
        let (nlen, elen, clen) = (
            u16_at(&cd, i + 28),
            u16_at(&cd, i + 30),
            u16_at(&cd, i + 32),
        );
        let offset = u32_at(&cd, i + 42);
        let entry = String::from_utf8_lossy(&cd[i + 46..i + 46 + nlen as usize]).into_owned();
        i += 46 + (nlen + elen + clen) as usize;
        if entry != name {
            continue;
        }
        let mut header = [0; 30];
        f.seek(SeekFrom::Start(offset))?;
        f.read_exact(&mut header)?;
        let skip = 30 + u16_at(&header, 26) + u16_at(&header, 28);
        f.seek(SeekFrom::Start(offset + skip))?;
        let data = std::io::Read::take(&mut f, csize);
        let reader: Box<dyn Read> = match method {
            0 => Box::new(data),
            8 => Box::new(flate2::read::DeflateDecoder::new(data)),
            m => bail!("{name}: unsupported zip method {m}"),
        };
        let part = store::part(out);
        let mut w = std::io::BufWriter::new(std::fs::File::create(&part)?);
        for line in std::io::BufReader::new(reader).lines() {
            let line = line?;
            if keep(&line) {
                writeln!(w, "{line}")?;
            }
        }
        w.into_inner()?.sync_all()?;
        std::fs::rename(&part, out)?;
        return Ok(());
    }
    bail!("{} holds no {name}", zip.display())
}

/// OpenAddresses address layers whose source Overture's address theme doesn't already carry,
/// each with the source definition its batch job ran (for its licence, see `sources.rs`).
async fn openaddresses(
    store: &Store,
    src: &Sources,
    overture_addresses: &Path,
    dir: &Path,
) -> Result<()> {
    let client = client()?;
    let bbox = src.bbox();
    let addresses = overture_addresses.to_path_buf();
    let c = client.clone();
    mirror(
        store,
        &format!("sources/{}/openaddresses/{}", src.snapshot, src.scope()),
        dir,
        8,
        || async move {
            let carried = tokio::task::spawn_blocking(move || crate::overture_datasets(&addresses)).await?;
            let layers: Vec<serde_json::Value> = c
                .get("https://batch.openaddresses.io/api/data")
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let layers: Vec<serde_json::Value> = layers
                .into_iter()
                .filter(|l| l["layer"] == "addresses" && l["job"].is_u64())
                .filter(|l| l["source"].as_str().is_some_and(|s| !carried.contains(s)))
                .collect();
            println!(
                "openaddresses: {} address layers not in Overture ({} sources Overture carries)",
                layers.len(),
                carried.len()
            );
            let permits = Arc::new(Semaphore::new(8));
            let mut tasks = JoinSet::new();
            for layer in layers {
                let (c, permits) = (c.clone(), permits.clone());
                tasks.spawn(async move {
                    let _permit = permits.acquire().await?;
                    let job = layer["job"].as_u64().unwrap_or_default();
                    let url = format!("https://batch.openaddresses.io/api/job/{job}");
                    let meta: serde_json::Value = c.get(&url).send().await?.error_for_status()?.json().await?;
                    anyhow::Ok((layer, meta))
                });
            }
            let mut items = Vec::new();
            while let Some(r) = tasks.join_next().await {
                let (layer, meta) = r??;
                if let Some(b) = bbox {
                    let ring = meta["bounds"]["coordinates"][0].as_array().cloned().unwrap_or_default();
                    let xs = ring.iter().filter_map(|p| p[0].as_f64());
                    let ys = ring.iter().filter_map(|p| p[1].as_f64());
                    let (x0, x1) = xs.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
                    let (y0, y1) = ys.fold((f64::MAX, f64::MIN), |(a, b), y| (a.min(y), b.max(y)));
                    if x0 > x1 || !intersects(b, x0, y0, x1, y1) {
                        continue;
                    }
                }
                let (Some(source), Some(name)) = (layer["source"].as_str(), layer["name"].as_str()) else {
                    continue;
                };
                let stem = format!("{}-{name}", source.replace('/', "_"));
                let job = layer["job"].as_u64().unwrap_or_default();
                items.push((
                    format!("{stem}.geojson.gz"),
                    OaItem::Data(format!("https://v2.openaddresses.io/batch-prod/job/{job}/source.geojson.gz")),
                ));
                items.push((format!("{stem}.source.json"), OaItem::Definition(meta)));
            }
            items.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(items)
        },
        move |item: OaItem, path: PathBuf| {
            let c = client.clone();
            Box::pin(async move {
                match item {
                    OaItem::Data(url) => http_get(&c, &url, &path).await,
                    OaItem::Definition(meta) => {
                        let Some(url) = meta["source"].as_str() else {
                            return Ok(false);
                        };
                        let response = c.get(url).send().await?;
                        // A source deleted upstream has no definition left at the URL its job
                        // ran; without one, the file counts as having no known licence.
                        if response.status() == reqwest::StatusCode::NOT_FOUND {
                            return Ok(false);
                        }
                        let mut def: serde_json::Value = response.error_for_status()?.json().await?;
                        def["_job"] = serde_json::json!({
                            "id": meta["id"], "source": meta["source"], "source_name": meta["source_name"],
                            "layer": meta["layer"], "name": meta["name"], "created": meta["created"],
                        });
                        tokio::fs::write(&path, serde_json::to_vec(&def)?).await?;
                        Ok(true)
                    }
                }
            })
        },
    )
    .await
}

enum OaItem {
    Data(String),
    Definition(serde_json::Value),
}

/// libpostal's models, fetched with libpostal's own `libpostal_data` script on first use.
pub async fn models(store: &Store, version: &str, dir: &Path) -> Result<()> {
    safe("models version", version, "")?;
    let rel = format!("models/libpostal/{version}");
    if store.exists(&format!("{rel}/_COMPLETE")).await? {
        println!("{rel}: from the store");
        return store.fetch(&rel, dir).await;
    }
    println!("{rel}: downloading with libpostal_data");
    let status = tokio::process::Command::new("libpostal_data")
        .args(["download", "all"])
        .arg(dir)
        .status()
        .await
        .context("run libpostal_data (installed with libpostal)")?;
    ensure!(status.success(), "libpostal_data failed: {status}");
    let found = std::fs::read_to_string(dir.join("data_version")).unwrap_or_default();
    ensure!(
        found.trim() == version,
        "libpostal_data fetched models {:?}, not {version}",
        found.trim()
    );
    store.publish(dir, &rel).await
}

/// Waits until `rel` is sealed, logging every ten minutes.
pub async fn wait_for(store: &Store, rel: &str) -> Result<()> {
    let mut waited = 0u64;
    while !store.exists(&format!("{rel}/_COMPLETE")).await? {
        if waited.is_multiple_of(600) {
            println!("waiting for {rel} ({}m)", waited / 60);
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
        waited += 30;
    }
    Ok(())
}

pub fn carried_datasets(
    batches: impl Iterator<Item = arrow_array::RecordBatch>,
    out: &mut HashSet<String>,
) {
    use arrow_array::{Array, StringArray, cast::AsArray};
    for b in batches {
        let Some(sources) = b.column_by_name("sources") else {
            continue;
        };
        let list = sources.as_list::<i32>();
        let Some(dataset) = list.values().as_struct().column_by_name("dataset") else {
            continue;
        };
        let dataset: &StringArray = dataset.as_string::<i32>();
        for i in 0..dataset.len() {
            if !dataset.is_null(i) {
                out.insert(dataset.value(i).to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_validate() {
        let src = |json: &str| serde_json::from_str::<Sources>(json).unwrap();
        // every key the chart's index.sources renders
        let ok = src(
            r#"{"bbox":[9.4,47.0,9.7,47.3],"geonamesPostal":["GB_full.csv","allCountries"],
            "osm":["europe/liechtenstein"],
            "overtureRelease":"2026-09-23.1","snapshot":"2026-10"}"#,
        );
        ok.validate().unwrap();
        // retired switches still parse and leave no trace in the serialized sources
        let old = src(
            r#"{"overtureRelease":"r","snapshot":"a","geonamesPlaces":false,
            "openaddresses":{"anyLicence":true,"enabled":false}}"#,
        );
        let json = serde_json::to_string(&old).unwrap();
        assert!(!json.contains("geonamesPlaces") && !json.contains("openaddresses"));
        assert_eq!(ok.scope(), "bbox_9.4_47_9.7_47.3");
        assert!(
            src(r#"{"overtureRelease":"../x","snapshot":"a"}"#)
                .validate()
                .is_err()
        );
        assert!(
            src(r#"{"overtureRelease":"r","snapshot":"a","bbox":[1,2,3]}"#)
                .validate()
                .is_err()
        );
        assert!(
            src(r#"{"overtureRelease":"r","snapshot":"a","bbox":[3,2,1,4]}"#)
                .validate()
                .is_err()
        );
        assert!(
            src(r#"{"overtureRelease":"r","snapshot":"a","osm":["/etc"]}"#)
                .validate()
                .is_err()
        );
        assert!(
            serde_json::from_str::<Sources>(r#"{"overtureRelease":"r","snapshot":"a","typo":1}"#)
                .is_err()
        );
        assert_eq!(
            src(r#"{"overtureRelease":"r","snapshot":"a"}"#).scope(),
            "world"
        );
    }

    #[test]
    fn unzips_one_entry_filtering_lines() {
        let dir = std::env::temp_dir().join(format!("unzip-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zip = dir.join("x.zip");
        // readme.txt (stored) then data.txt (deflated), written by hand: local headers,
        // central directory, end record.
        let deflated = {
            let mut e =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(b"keep 1\ndrop 2\nkeep 3\n").unwrap();
            e.finish().unwrap()
        };
        let entries: [(&str, u16, Vec<u8>); 2] = [
            ("readme.txt", 0, b"hello\n".to_vec()),
            ("data.txt", 8, deflated),
        ];
        let (mut body, mut cd) = (Vec::new(), Vec::new());
        for (name, method, data) in &entries {
            let offset = body.len() as u32;
            body.extend(0x0403_4b50u32.to_le_bytes());
            body.extend([20, 0, 0, 0]);
            body.extend(method.to_le_bytes());
            body.extend([0; 8]);
            body.extend((data.len() as u32).to_le_bytes());
            body.extend([0; 4]);
            body.extend((name.len() as u16).to_le_bytes());
            body.extend([0, 0]);
            body.extend(name.as_bytes());
            body.extend(data);
            cd.extend(0x0201_4b50u32.to_le_bytes());
            cd.extend([20, 0, 20, 0, 0, 0]);
            cd.extend(method.to_le_bytes());
            cd.extend([0; 8]);
            cd.extend((data.len() as u32).to_le_bytes());
            cd.extend([0; 4]);
            cd.extend((name.len() as u16).to_le_bytes());
            cd.extend([0; 12]);
            cd.extend(offset.to_le_bytes());
            cd.extend(name.as_bytes());
        }
        let cd_offset = body.len() as u32;
        body.extend(&cd);
        body.extend(0x0605_4b50u32.to_le_bytes());
        body.extend([0, 0, 0, 0, 2, 0, 2, 0]);
        body.extend((cd.len() as u32).to_le_bytes());
        body.extend(cd_offset.to_le_bytes());
        body.extend([0, 0]);
        std::fs::write(&zip, body).unwrap();
        let out = dir.join("data.txt");
        unzip_lines(&zip, "data.txt", &out, |l| l.starts_with("keep")).unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "keep 1\nkeep 3\n");
        unzip_lines(&zip, "readme.txt", &out, |_| true).unwrap();
        assert_eq!(std::fs::read_to_string(&out).unwrap(), "hello\n");
        assert!(unzip_lines(&zip, "missing.txt", &out, |_| true).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn an_extract_comes_from_the_other_list_or_is_added_to_a_sealed_snapshot() {
        let base = std::env::temp_dir().join(format!("geofabrik-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let store = Store::open(&format!("file://{}", base.join("bucket").display())).unwrap();
        for (list, files) in [
            ("osm", ["chile", "peru"]),
            ("osm-landmarks", ["japan", "france"]),
        ] {
            let dir = base.join("seed").join(list);
            std::fs::create_dir_all(&dir).unwrap();
            for f in files {
                std::fs::write(dir.join(format!("{f}.osm.pbf")), f).unwrap();
            }
            store
                .publish(&dir, &format!("sources/2026-10/{list}"))
                .await
                .unwrap();
        }
        let src: Sources =
            serde_json::from_str(r#"{"overtureRelease": "2026-09-23.1", "snapshot": "2026-10"}"#)
                .unwrap();
        let (client, scratch) = (reqwest::Client::new(), base.join("scratch"));
        std::fs::create_dir_all(scratch.join("osm")).unwrap();
        // left over from a build when peru was still on the list
        std::fs::write(scratch.join("osm/peru.osm.pbf"), "peru").unwrap();
        let ids = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let dir = geofabrik(
            &store,
            &client,
            &src,
            "osm",
            &ids(&["chile", "japan"]),
            &scratch,
        )
        .await
        .unwrap()
        .unwrap();
        let mut got: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        got.sort();
        assert_eq!(got, ["chile.osm.pbf", "japan.osm.pbf"]);
        assert_eq!(
            std::fs::read_to_string(dir.join("japan.osm.pbf")).unwrap(),
            "japan"
        );
        // an extract neither list holds is fetched alone, sealed, and reused by the next build
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/mexico.osm.pbf", server.local_addr().unwrap());
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            let (mut conn, _) = server.accept().unwrap();
            let _ = conn.read(&mut [0; 4096]).unwrap();
            conn.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nmexico",
            )
            .unwrap();
        });
        let added = "sources/2026-10/geofabrik/mexico.osm.pbf";
        add_extract(
            &store,
            &client,
            &url,
            added,
            &base.join("other"),
            "mexico.osm.pbf",
        )
        .await
        .unwrap();
        let dir = geofabrik(
            &store,
            &client,
            &src,
            "osm",
            &ids(&["chile", "mexico"]),
            &scratch,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            store::walk(&dir).unwrap(),
            ["chile.osm.pbf", "mexico.osm.pbf"]
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("mexico.osm.pbf")).unwrap(),
            "mexico"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
    #[tokio::test]
    async fn a_kept_scratch_directory_loses_an_earlier_builds_files() {
        let base = std::env::temp_dir().join(format!("mirror-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let store = Store::open(&format!("file://{}", base.join("bucket").display())).unwrap();
        let dir = base.join("scratch/address");
        std::fs::create_dir_all(&dir).unwrap();
        // the previous release's part file, and one the new release no longer produces
        std::fs::write(dir.join("part-old.parquet"), "old").unwrap();
        std::fs::write(dir.join("gone.parquet"), "gone").unwrap();
        let fill = || {
            mirror(
                &store,
                "sources/overture/new/address",
                &dir,
                1,
                || async {
                    Ok(vec![
                        ("part-new.parquet".to_string(), true),
                        ("gone.parquet".to_string(), false),
                    ])
                },
                |write: bool, path: PathBuf| {
                    Box::pin(async move {
                        if write {
                            std::fs::write(path, "new")?;
                        }
                        Ok(write)
                    })
                },
            )
        };
        let listed = |p: &Path| store::walk(p).unwrap();
        fill().await.unwrap();
        assert_eq!(listed(&dir), ["part-new.parquet"]);
        let manifest =
            std::fs::read_to_string(base.join("bucket/sources/overture/new/address/MANIFEST"))
                .unwrap();
        assert!(
            !manifest.contains("old") && !manifest.contains("gone"),
            "{manifest}"
        );
        // the sealed mirror, fetched into a directory still holding an earlier build's file
        std::fs::write(dir.join("part-old.parquet"), "old").unwrap();
        fill().await.unwrap();
        assert_eq!(listed(&dir), ["part-new.parquet"]);
        std::fs::remove_dir_all(&base).unwrap();
    }
}
