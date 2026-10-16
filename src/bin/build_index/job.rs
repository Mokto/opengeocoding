//! `build_index job`, `pull` and `hash`: building an index into a store and fetching it for
//! serving, as the Helm chart in `manifests/` runs them.
use crate::{
    fetch::{self, Sources},
    progress,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

pub const BUILDER_HASH: &str = env!("BUILDER_HASH");

struct Args(Vec<String>);

impl Args {
    fn get(&self, flag: &str) -> Option<&str> {
        self.0
            .windows(2)
            .find(|w| w[0] == flag)
            .map(|w| w[1].as_str())
    }

    fn required(&self, flag: &str) -> Result<&str> {
        self.get(flag)
            .with_context(|| format!("{flag} is required"))
    }
}

fn index_id(id: &str) -> Result<&str> {
    ensure!(
        !id.is_empty()
            && !id.starts_with('.')
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
        "invalid index id {id:?}"
    );
    Ok(id)
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?)
}

/// `build_index job --store URL --index ID --builder-hash HASH --sources FILE --scratch DIR
/// --models DIR [--models-version v1]`: does nothing if the store holds index `ID`; otherwise
/// fetches the sources and models, builds, checks and publishes it.
pub fn job(args: Vec<String>) -> Result<()> {
    let args = Args(args);
    let expected = args.required("--builder-hash")?;
    ensure!(
        expected == BUILDER_HASH,
        "this image's builder is {BUILDER_HASH}, but the chart expects {expected}: use the image \
         built from the chart's commit"
    );
    let id = index_id(args.required("--index")?)?;
    let sources: Sources = serde_json::from_slice(
        &std::fs::read(args.required("--sources")?).context("read --sources")?,
    )
    .context("parse --sources")?;
    sources.validate()?;
    let scratch = PathBuf::from(args.required("--scratch")?);
    let models = PathBuf::from(args.required("--models")?);
    let version = args.get("--models-version").unwrap_or("v1");
    let store = Store::open(args.required("--store")?)?;
    let rt = runtime()?;
    let rel = format!("indexes/{id}");
    if rt.block_on(store.exists(&format!("{rel}/_COMPLETE")))? {
        println!("{rel} is already published");
        return Ok(());
    }
    progress::plan_job();
    progress::phase("Download models");
    rt.block_on(fetch::models(&store, version, &models))?;
    progress::phase("Download sources");
    let got = rt.block_on(fetch::fetch(&store, &sources, &scratch))?;

    let out = scratch.join("index");
    let mut build = vec![
        "build_index".to_string(),
        got.overture.to_string_lossy().into_owned(),
        out.to_string_lossy().into_owned(),
    ];
    if let Some(d) = &got.openaddresses {
        build.extend(["--openaddresses".into(), d.to_string_lossy().into_owned()]);
        if sources.openaddresses.any_licence {
            build.push("--openaddresses-any-licence".into());
        }
    }
    if let Some(d) = &got.osm {
        build.extend(["--osm".into(), d.to_string_lossy().into_owned()]);
    }
    if let Some(b) = sources.bbox() {
        build.extend([
            "--bbox".into(),
            format!("{},{},{},{}", b[0], b[1], b[2], b[3]),
        ]);
    }
    for f in &got.geonames {
        build.extend(["--geonames-postal".into(), f.to_string_lossy().into_owned()]);
    }
    crate::build(&build);

    std::fs::write(
        out.join("build.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "builder_hash": BUILDER_HASH,
            "index": id,
            "sources": sources,
            "models_version": version,
        }))?,
    )?;
    std::fs::write(out.join("ATTRIBUTION.txt"), attribution(&sources, &out)?)?;
    progress::phase("Verify");
    verify(&out)?;
    progress::phase("Upload");
    rt.block_on(store.publish(&out, &rel))?;
    println!("published {rel}");
    progress::summary();
    Ok(())
}

/// Places asked for by their own name, and the share that must come back non-empty. A place
/// can legitimately miss itself (about 3% did in a world index), so one miss must not fail a
/// build that takes half an hour; a broken index misses most of them.
const VERIFY_SAMPLE: usize = 200;
const VERIFY_MIN_PERCENT: usize = 80;

/// Evenly spaced place names from `places.tsv`, skipping names too short to search for.
fn sample_names(places: &str) -> Vec<&str> {
    let names: Vec<&str> = places
        .lines()
        .filter_map(|l| l.split('\t').nth(13))
        .filter(|n| n.len() > 3)
        .collect();
    let step = (names.len() / VERIFY_SAMPLE).max(1);
    names
        .into_iter()
        .step_by(step)
        .take(VERIFY_SAMPLE)
        .collect()
}

/// Opens the built index the way the service does and resolves a sample of its own places.
fn verify(dir: &Path) -> Result<()> {
    use opengeocoding::index::{Index, Parser};
    let parser = std::sync::Arc::new(Parser::setup()?);
    let index = Index::open(dir, parser)?;
    ensure!(index.places_loaded() > 0, "the index holds no places");
    let places = std::fs::read_to_string(dir.join("places.tsv"))?;
    let sample = sample_names(&places);
    ensure!(!sample.is_empty(), "places.tsv names no place");
    let misses: Vec<&str> = sample
        .iter()
        .copied()
        .filter(|n| index.search(n).is_empty())
        .collect();
    let found = sample.len() - misses.len();
    ensure!(
        found * 100 >= sample.len() * VERIFY_MIN_PERCENT,
        "the index finds only {found} of {} sampled places by their own name; missing, for \
         example: {:?}",
        sample.len(),
        &misses[..misses.len().min(10)]
    );
    println!(
        "verify: PASS, {} places; {found} of {} sampled resolve ({}%, at least {VERIFY_MIN_PERCENT}% required)",
        index.places_loaded(),
        sample.len(),
        found * 100 / sample.len()
    );
    if !misses.is_empty() {
        progress::warn(format!(
            "{} sampled places did not resolve by their own name, for example {:?}",
            misses.len(),
            &misses[..misses.len().min(10)]
        ));
    }
    Ok(())
}

fn attribution(sources: &Sources, out: &Path) -> Result<String> {
    let mut text = format!(
        "Built from these sources; their licences and attribution requirements apply to the index \
         and to answers derived from it.\n\n\
         Overture Maps Foundation, release {}: divisions and transportation under ODbL \
         (© OpenStreetMap contributors and others), addresses under each source's licence. \
         See https://docs.overturemaps.org/attribution/\n",
        sources.overture_release
    );
    if !sources.osm.is_empty() {
        text += "OpenStreetMap: © OpenStreetMap contributors, ODbL 1.0, \
                 https://www.openstreetmap.org/copyright (extracts by Geofabrik)\n";
    }
    if !sources.geonames_postal.is_empty() {
        text += "GeoNames postal codes: CC BY 4.0, https://www.geonames.org/\n";
    }
    if let Ok(read) = std::fs::read_to_string(out.join("openaddresses.tsv")) {
        text += "OpenAddresses (https://openaddresses.io), sources read and their licences:\n";
        for line in read.lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            if f.get(2) == Some(&"true") {
                text += &format!("  {}: {}\n", f[0], f.get(4).unwrap_or(&""));
            }
        }
    }
    Ok(text)
}

/// `build_index pull --store URL --index ID --to DIR [--models DIR] [--models-version v1]`:
/// waits until index `ID` is published, then downloads and checks it (and the models).
pub fn pull(args: Vec<String>) -> Result<()> {
    let args = Args(args);
    let id = index_id(args.required("--index")?)?;
    let to = PathBuf::from(args.required("--to")?);
    let store = Store::open(args.required("--store")?)?;
    let rt = runtime()?;
    let rel = format!("indexes/{id}");
    rt.block_on(async {
        fetch::wait_for(&store, &rel).await?;
        store.fetch(&rel, &to).await?;
        if let Some(models) = args.get("--models") {
            let version = args.get("--models-version").unwrap_or("v1");
            let rel = format!("models/libpostal/{version}");
            fetch::wait_for(&store, &rel).await?;
            store.fetch(&rel, Path::new(models)).await?;
        }
        anyhow::Ok(())
    })?;
    println!("pulled {rel}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_is_spread_capped_and_skips_short_names() {
        let row = |n: &str| format!("a\tb\tc\td\te\tf\tg\th\ti\tj\tk\tl\tm\t{n}\tx");
        let places: Vec<String> = (0..1000)
            .map(|i| {
                row(&if i % 10 == 0 {
                    "ab".into()
                } else {
                    format!("place {i}")
                })
            })
            .collect();
        let all = places.join("\n");
        let sample = sample_names(&all);
        assert_eq!(sample.len(), VERIFY_SAMPLE);
        assert!(sample.iter().all(|n| n.len() > 3));
        assert!(
            sample
                .last()
                .unwrap()
                .ends_with(|c: char| c.is_ascii_digit())
        );
        let small = places[..5].join("\n");
        assert_eq!(sample_names(&small).len(), 4);
        assert!(sample_names("").is_empty());
    }
}
