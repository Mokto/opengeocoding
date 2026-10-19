use anyhow::{Context, Result};
use opengeocoding::index::schema::{fold, number_key, unpack};
use serde_json::{Value, json};
use std::{collections::HashMap, io::BufRead, path::Path};
use tantivy::{DocAddress, TantivyDocument, directory::Directory, schema::Value as _};
use tantivy_fst::Streamer;

type Point = (f64, f64);

fn distance(a: Point, b: Point) -> f64 {
    ((a.0 - b.0) * a.1.to_radians().cos()).hypot(a.1 - b.1) * 111_195.
}

fn simple_number(number: &str) -> bool {
    let digits = number.trim_end_matches(|c: char| c.is_ascii_lowercase());
    !digits.is_empty()
        && number.len() - digits.len() <= 1
        && digits.bytes().all(|b| b.is_ascii_digit())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() == 3,
        "usage: triage_169_overlap INDEX_DIR OSM_NODES.geojsonseq"
    );
    let mut source: HashMap<(String, String), Vec<Point>> = HashMap::new();
    let mut rows = 0;
    for line in std::io::BufReader::new(std::fs::File::open(&args[2])?).lines() {
        let line = line?;
        let feature: Value = serde_json::from_str(line.trim_start_matches('\u{1e}'))?;
        if feature["geometry"]["type"] != "Point" {
            continue;
        }
        let point = &feature["geometry"]["coordinates"];
        let (x, y) = (
            point[0].as_f64().context("longitude")?,
            point[1].as_f64().context("latitude")?,
        );
        let p = &feature["properties"];
        let (Some(road), Some(number)) =
            (p["addr:street"].as_str(), p["addr:housenumber"].as_str())
        else {
            continue;
        };
        if !(-122.2..=-122.05).contains(&x)
            || !(48.05..=48.25).contains(&y)
            || road.trim().is_empty()
            || number.trim().is_empty()
        {
            continue;
        }
        source
            .entry((fold(road), number_key(number)))
            .or_default()
            .push((x, y));
        rows += 1;
    }
    let dir = Path::new(&args[1]);
    let index = tantivy::Index::open_in_dir(dir)?;
    let searcher = index.reader()?.searcher();
    let mut docs = HashMap::new();
    for (seg, reader) in searcher.segment_readers().iter().enumerate() {
        let (ids, places) = (
            reader.fast_fields().u64("id")?,
            reader.fast_fields().u64("place")?,
        );
        for doc in 0..reader.max_doc() {
            if places.index.has_value(doc)
                && let Some(sid) = ids.first(doc)
            {
                docs.insert(sid as u32, DocAddress::new(seg as u32, doc));
            }
        }
    }
    let mmap = tantivy::directory::MmapDirectory::open(dir)?;
    let shards: usize = std::fs::read_to_string(dir.join("numbers.shards"))?
        .trim()
        .parse()?;
    anyhow::ensure!(shards > 0, "numbers.shards must be positive");
    let mut names = HashMap::new();
    let name = index.schema().get_field("name")?;
    let mut same_name: HashMap<(String, String), Vec<Point>> = HashMap::new();
    let mut same_number: HashMap<String, Vec<Point>> = HashMap::new();
    for shard in 0..shards {
        eprintln!("number shard {shard}/{shards}");
        let file = mmap.open_read(Path::new(&format!("numbers-{shard:02}.fst")))?;
        let map = tantivy_fst::Map::from(tantivy_fst::raw::Fst::new(file.read_bytes()?)?);
        let mut stream = map.stream();
        while let Some((key, value)) = stream.next() {
            let (x, y, _) = unpack(value);
            if !(-122.7..=-120.8).contains(&x) || !(47.7..=48.5).contains(&y) {
                continue;
            }
            let sid = u32::from_be_bytes(key.get(..4).context("street id")?.try_into()?);
            let aliases = names.entry(sid).or_insert_with(Vec::new);
            if aliases.is_empty() {
                let doc =
                    searcher.doc::<TantivyDocument>(*docs.get(&sid).context("street document")?)?;
                aliases.extend(
                    doc.get_all(name)
                        .filter_map(|v| v.as_str())
                        .flat_map(|n| n.split('\u{1f}'))
                        .map(fold),
                );
            }
            let number = std::str::from_utf8(&key[4..])?;
            for alias in aliases.iter() {
                let key = (alias.clone(), number.to_string());
                if source.contains_key(&key) {
                    same_name.entry(key).or_default().push((x, y));
                }
            }
            same_number
                .entry(number.to_string())
                .or_default()
                .push((x, y));
        }
    }
    let mut present = 0;
    let mut far = 0;
    let mut unmatched = 0;
    let mut simple = 0;
    let mut candidates = 0;
    for (key, points) in &source {
        if let Some(existing) = same_name.get(key) {
            if points
                .iter()
                .any(|&a| existing.iter().any(|&b| distance(a, b) <= 100.))
            {
                present += 1;
            } else {
                far += 1;
            }
        } else {
            unmatched += 1;
            if simple_number(&key.1) {
                simple += 1;
                let nearby = same_number.get(&key.1).is_some_and(|existing| {
                    points
                        .iter()
                        .any(|&a| existing.iter().any(|&b| distance(a, b) <= 100.))
                });
                candidates += usize::from(!nearby);
            }
        }
    }
    println!(
        "{}",
        json!({"source_rows": rows, "source_keys": source.len(), "present_within_100m": present, "same_key_farther": far, "unmatched_keys": unmatched, "simple_unmatched": simple, "candidate_keys": candidates})
    );
    Ok(())
}

#[test]
fn the_measurement_excludes_embedded_units_and_accepts_one_letter_suffix() {
    for number in ["1", "123", "123a"] {
        assert!(simple_number(number));
    }
    for number in ["", "a", "123ab", "123 #a", "12-14", "1/2"] {
        assert!(!simple_number(number));
    }
}
