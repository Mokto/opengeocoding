use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const URL: &str = "https://opendata.bkk.hu/data-sources";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Station {
    record: Value,
    metro_lines: Vec<String>,
    point: [f64; 2],
    feed_version: String,
    feed_start_date: String,
    feed_end_date: String,
    source_sha256: String,
}

impl Station {
    fn id(&self) -> &str {
        self.record["stop_id"].as_str().unwrap()
    }
    fn name(&self) -> &str {
        self.record["stop_name"].as_str().unwrap()
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Station>> {
    let rows = crate::model::load_jsonl::<Station>(path)?;
    let total = rows.len();
    let mut ids = BTreeSet::new();
    let mut names = BTreeMap::<String, Station>::new();
    let mut conflicts = BTreeSet::new();
    for s in rows {
        let id = s.record["stop_id"]
            .as_str()
            .filter(|n| !n.is_empty())
            .context("BKK needs stop_id")?;
        let name = s.record["stop_name"]
            .as_str()
            .filter(|n| !n.trim().is_empty() && !n.chars().any(char::is_control))
            .context("BKK needs stop_name")?;
        ensure!(ids.insert(id.to_string()), "duplicate BKK stop_id {id}");
        ensure!(
            s.record["location_type"] == "1" && s.record["parent_station"] == "",
            "BKK needs a top-level GTFS station, not a platform or entrance"
        );
        ensure!(
            !s.metro_lines.is_empty()
                && s.metro_lines
                    .iter()
                    .all(|l| ["M1", "M2", "M3", "M4"].contains(&l.as_str())),
            "BKK needs a station served by a metro route"
        );
        ensure!(
            (45.7..48.7).contains(&s.point[0]) && (16. ..23.).contains(&s.point[1]),
            "BKK needs Hungarian WGS84 latitude/longitude"
        );
        ensure!(
            s.record["stop_lat"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                == Some(s.point[0])
                && s.record["stop_lon"]
                    .as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    == Some(s.point[1]),
            "BKK point differs from the published station coordinates"
        );
        ensure!(
            !s.feed_version.is_empty()
                && s.source_sha256.len() == 64
                && s.source_sha256.bytes().all(|c| c.is_ascii_hexdigit()),
            "BKK needs the feed version and source SHA-256"
        );
        let key = name.trim().to_lowercase();
        if let Some(old) = names.get_mut(&key) {
            if old.point != s.point {
                conflicts.insert(key);
            }
            if s.id() < old.id() {
                *old = s;
            }
        } else {
            names.insert(key, s);
        }
    }
    let groups = names.len();
    names.retain(|name, _| !conflicts.contains(name));
    eprintln!(
        "BKK: {total} metro parents, {groups} name groups, {} unambiguous",
        names.len()
    );
    Ok(names
        .into_values()
        .map(|s| (s.id().to_string(), s))
        .collect())
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pool: Vec<_> = load(path)?
        .into_values()
        .map(|s| (sha256_hex(format!("{seed}\0{}", s.id()).as_bytes()), s))
        .collect();
    pool.sort_by(|a, b| a.0.cmp(&b.0));
    let mut lines = vec![];
    for (_, s) in pool.into_iter().take(64) {
        for (form, query) in [
            ("station_first", format!("{}, Budapest, Hungary", s.name())),
            ("city_first", format!("Budapest, {}, Hungary", s.name())),
            (
                "native_country",
                format!("{}, Budapest, Magyarország", s.name()),
            ),
        ] {
            lines.push(json!({"kind":"suggest_landmark", "source":"bkk-metro", "ref":s.id(), "cohort":"hu_metro", "queries":[query], "label":s.name(), "radius_km":0.3, "tags":["source-sample","hu","bkk","metro-station-not-address",form]}));
        }
    }
    ensure!(!lines.is_empty(), "no eligible BKK metro stations");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "bkk-metro",
    command: "sample-hu-metro",
    country: "hu",
    input: "bkk-metro",
    license: "CC-BY-4.0",
    location: "bkk-station",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    let stations = load(path)?;
    refs.iter().map(|id| {
        let s = stations.get(id).with_context(|| format!("BKK lacks unambiguous station {id}"))?;
        let mut provenance = metadata.clone();
        provenance.insert("record".into(), serde_json::to_value(s)?);
        provenance.insert("upstream".into(), json!("https://go.bkk.hu/api/static/v1/public-gtfs/budapest_gtfs.zip"));
        provenance.insert("attribution".into(), json!("Data source: BKK Zrt., CC BY 4.0; https://opendata.bkk.hu/data-sources"));
        provenance.insert("coordinate_reference".into(), json!("Published GTFS parent-station WGS84 latitude/longitude, not a platform or entrance point; no geocoder used"));
        Ok((id.clone(), Entry { point: Point::Wgs84(s.point), components: indexmap::IndexMap::from([("category", "station".into()), ("name", s.name().into())]), radius_floor_km:0.3, provenance, number_aliases:None, places:vec![] }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_not_address_and_ambiguous_names() {
        let path = std::env::temp_dir().join(format!("bkk-test-{}.jsonl", std::process::id()));
        let row = |id: &str, name: &str, lat: &str| json!({"record":{"stop_id":id,"stop_name":name,"stop_lat":lat,"stop_lon":"19.06","location_type":"1","parent_station":""}, "metro_lines":["M1"],"point":[lat.parse::<f64>().unwrap(),19.06], "feed_version":"test", "feed_start_date":"20261031", "feed_end_date":"20261201", "source_sha256":"0".repeat(64)});
        let mut rows = vec![
            row("2", "Állomás", "47.5"),
            row("1", "Állomás", "47.5"),
            row("3", "Másik", "47.5"),
            row("4", "Másik", "47.6"),
        ];
        let write = |rows: &[Value]| {
            std::fs::write(
                &path,
                rows.iter().map(|r| format!("{r}\n")).collect::<String>(),
            )
            .unwrap()
        };
        write(&rows);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 3);
        assert!(
            first
                .iter()
                .all(|s| s["kind"] == "suggest_landmark" && s["ref"] == "1")
        );
        rows.reverse();
        write(&rows);
        assert_eq!(first, sample(&path, "test").unwrap());
        let entries = prepare(&path, &BTreeSet::from(["1".into()])).unwrap();
        assert_eq!(entries["1"].components.len(), 2);
        assert_eq!(entries["1"].components["category"], "station");
        assert_eq!(entries["1"].components["name"], "Állomás");
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        let mut bad = row("1", "Állomás", "47.5");
        bad["record"]["location_type"] = json!("0");
        write(&[bad]);
        assert!(sample(&path, "test").is_err());
        let mut bad = row("1", "Állomás", "47.5");
        bad["point"] = json!([19.06, 47.5]);
        write(&[bad]);
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
