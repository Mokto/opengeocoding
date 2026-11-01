use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

const URL: &str = "https://www.asiaq.gl/data-distribution/";
const LOCALITIES: [(&str, &str, &str); 6] = [
    ("0200QAQ", "Qaqortoq", "EPSG:3183"),
    ("0600NUK", "Nuuk", "EPSG:3182"),
    ("0800SIS", "Sisimiut", "EPSG:3182"),
    ("1000AAS", "Aasiaat", "EPSG:3182"),
    ("1200ILU", "Ilulissat", "EPSG:3182"),
    ("1800TAS", "Tasiilaq", "EPSG:3184"),
];

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Building {
    locality: String,
    crs: String,
    point: [f64; 2],
    projected: [f64; 2],
    extent_m: f64,
    record: Value,
}

impl Building {
    fn number(&self) -> &str {
        self.record["B_nummer"].as_str().unwrap_or("")
    }

    fn id(&self) -> String {
        format!("{}:{}", self.locality, self.number())
    }

    fn town(&self) -> &str {
        LOCALITIES.iter().find(|l| l.0 == self.locality).unwrap().1
    }

    fn radius(&self) -> f64 {
        ((self.extent_m + 25.) / 1000.).max(0.05)
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Building>> {
    let rows = crate::model::load_jsonl::<Building>(path)?;
    let total = rows.len();
    let mut eligible = 0;
    let mut buildings = BTreeMap::<String, Building>::new();
    let mut conflicts = BTreeSet::new();
    for b in rows {
        let (_, _, crs) = LOCALITIES
            .iter()
            .find(|l| l.0 == b.locality)
            .with_context(|| format!("unknown Asiaq locality {}", b.locality))?;
        ensure!(
            b.crs == *crs && b.record["lokalitet"] == b.locality,
            "Asiaq locality/CRS mismatch"
        );
        ensure!(
            (59. ..84.).contains(&b.point[0]) && (-74. ..-10.).contains(&b.point[1]),
            "Asiaq needs Greenlandic WGS84 latitude/longitude, not swapped axes"
        );
        ensure!(
            (100_000. ..900_000.).contains(&b.projected[0])
                && (6_000_000. ..9_400_000.).contains(&b.projected[1]),
            "Asiaq needs projected easting/northing in metres"
        );
        ensure!(
            b.extent_m.is_finite() && b.extent_m > 0.,
            "Asiaq needs a positive finite building extent"
        );
        let Some(number) = b.number().strip_prefix("B-") else {
            continue;
        };
        if !(1..=5).contains(&number.len())
            || number.starts_with('0')
            || !number.bytes().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        let feature = b.record["lokal_id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("Asiaq needs lokal_id")?;
        eligible += 1;
        let id = b.id();
        if let Some(old) = buildings.get_mut(&id) {
            if old.point != b.point || old.extent_m != b.extent_m {
                conflicts.insert(id);
            }
            if feature < old.record["lokal_id"].as_str().unwrap() {
                *old = b;
            }
        } else {
            buildings.insert(id, b);
        }
    }
    let groups = buildings.len();
    buildings.retain(|id, _| !conflicts.contains(id));
    eprintln!(
        "Asiaq: {total} valid building outlines, {eligible} numbered rows, {groups} B-number keys, {} unambiguous",
        buildings.len()
    );
    Ok(buildings)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<String, Vec<(String, Building)>>::new();
    for b in load(path)?.into_values() {
        pools
            .entry(b.locality.clone())
            .or_default()
            .push((sha256_hex(format!("{seed}\0{}", b.id()).as_bytes()), b));
    }
    let mut lines = vec![];
    for (locality, mut pool) in pools {
        pool.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, b) in pool.into_iter().take(12) {
            let town = b.town();
            let label = format!("{}, {town}", b.number());
            for (form, query) in [
                ("number_first", format!("{label}, Greenland")),
                ("town_first", format!("{town}, {}, Greenland", b.number())),
            ] {
                lines.push(json!({"kind":"building", "source":"asiaq-buildings", "ref":b.id(), "cohort":"gl_building", "country":["gl","dk"], "radius_km":b.radius(), "queries":[query], "label":label, "tags":["source-sample","gl","asiaq",locality,town,"building-not-postal-address",form]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible Asiaq numbered buildings");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "asiaq-buildings",
    command: "sample-gl-buildings",
    country: "gl",
    input: "asiaq-buildings",
    license: "Asiaq-Geodata-Terms",
    location: "asiaq-building",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    let buildings = load(path)?;
    refs.iter().map(|id| {
        let b = buildings.get(id).with_context(|| format!("Asiaq lacks unambiguous building {id}"))?;
        let mut provenance = metadata.clone();
        provenance.insert("record".into(), serde_json::to_value(b)?);
        provenance.insert("upstream".into(), json!(format!("https://kortforsyning.asiaq.gl/files/{}_{}/SHP/{}_TekniskGrundkort_SHP.zip", b.locality, b.town(), b.locality)));
        provenance.insert("attribution".into(), json!("Contains data from Asiaq, Greenland Survey, Technical Base Map (BYGNING), obtained 2026-10-31; https://www.asiaq.gl/wp-content/uploads/2026/04/EN_Terms_of_use_for_Asiaq_geodata.pdf"));
        provenance.insert("coordinate_transform".into(), json!("Building-outline point-on-surface, GR96 / UTM to WGS84 using DuckDB spatial / PROJ, always_xy=true; source measurement reference retained, not an entrance"));
        Ok((id.clone(), Entry { point: Point::Wgs84(b.point), components: indexmap::IndexMap::new(), radius_floor_km: b.radius(), provenance, number_aliases: None, places: vec![] }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_identity_conflicts_extent_and_axes() {
        let path = std::env::temp_dir().join(format!("asiaq-test-{}.jsonl", std::process::id()));
        let row = |id: &str, number: &str, lat: f64| json!({"locality":"0600NUK", "crs":"EPSG:3182", "point":[lat,-51.7], "projected":[465000.,7120000.], "extent_m":40., "record":{"B_nummer":number,"lokalitet":"0600NUK","lokal_id":id}});
        let mut rows = vec![
            row("a", "B-12", 64.1),
            row("b", "B-13", 64.1),
            row("c", "B-13", 64.2),
            row("d", "B-014", 64.1),
            row("e", "B-15 og B-16", 64.1),
            row("z", "B-12", 64.1),
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
        assert_eq!(first.len(), 2);
        assert_eq!(first[0]["kind"], "building");
        assert_eq!(first[0]["country"], json!(["gl", "dk"]));
        assert_eq!(first[0]["ref"], "0600NUK:B-12");
        rows.reverse();
        write(&rows);
        assert_eq!(first, sample(&path, "test").unwrap());
        let entries = prepare(&path, &BTreeSet::from(["0600NUK:B-12".into()])).unwrap();
        assert!(entries["0600NUK:B-12"].components.is_empty());
        assert_eq!(entries["0600NUK:B-12"].radius_floor_km, 0.065);
        assert_eq!(
            entries["0600NUK:B-12"].provenance["record"]["record"]["lokal_id"],
            "a"
        );
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        let mut bad = row("a", "B-12", 64.1);
        bad["point"] = json!([-51.7, 64.1]);
        write(&[bad]);
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
