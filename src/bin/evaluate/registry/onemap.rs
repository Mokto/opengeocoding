use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
};

const URL: &str = "https://www.onemap.gov.sg/apidocs/search";

/// `tests/registry/export-onemap-postcodes.py`'s output: the search results for every drawn
/// postcode, found or not.
#[derive(Deserialize)]
struct Snapshot {
    seed: String,
    probes: Vec<Probe>,
}
#[derive(Deserialize)]
struct Probe {
    postal: String,
    results: Vec<Hit>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
struct Hit {
    postal: String,
    blk_no: String,
    road_name: String,
    building: String,
    latitude: String,
    longitude: String,
}

/// A postcode whose every result is one block number on one road.
struct Address {
    postal: String,
    block: String,
    road: String,
    buildings: Vec<String>,
    point: [f64; 2],
}

impl Address {
    fn queries(&self) -> [(&'static str, String); 2] {
        let street = format!("{} {}", self.block, self.road);
        [
            ("postcode", format!("{street}, SINGAPORE {}", self.postal)),
            ("street", format!("{street}, SINGAPORE")),
        ]
    }

    fn category(&self) -> &'static str {
        if self.block.ends_with(|c: char| c.is_ascii_alphabetic()) {
            "suffix"
        } else {
            "plain"
        }
    }
}

fn load(path: &Path) -> Result<(String, BTreeMap<String, Address>)> {
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    let mut rows = BTreeMap::new();
    let mut probed = BTreeSet::new();
    for probe in snapshot.probes {
        ensure!(
            probed.insert(probe.postal.clone()),
            "duplicate OneMap postcode {}",
            probe.postal
        );
        // The search matches other fields too, so only results carrying the postcode belong to it.
        let hits: Vec<_> = probe
            .results
            .into_iter()
            .filter(|h| h.postal == probe.postal)
            .collect();
        let mut points = vec![];
        for h in &hits {
            let point = [h.latitude.parse::<f64>()?, h.longitude.parse::<f64>()?];
            ensure!(
                (1.15..=1.48).contains(&point[0]) && (103.6..=104.1).contains(&point[1]),
                "OneMap postcode {} needs Singapore WGS84 coordinates",
                probe.postal
            );
            points.push(point);
        }
        let Some(first) = hits.first() else {
            continue;
        };
        // A postcode with several block numbers or roads, or whose buildings lie apart, is several
        // addresses; a block "number" with other characters ("NIL", ranges) is not a house number.
        let digits = first
            .blk_no
            .trim_end_matches(|c: char| c.is_ascii_uppercase());
        let spread_m = points
            .iter()
            .map(|p| {
                let dy = (p[0] - points[0][0]) * 111_320.;
                let dx = (p[1] - points[0][1]) * 111_320. * points[0][0].to_radians().cos();
                dx.hypot(dy)
            })
            .fold(0., f64::max);
        if hits
            .iter()
            .any(|h| h.blk_no != first.blk_no || h.road_name != first.road_name)
            || spread_m > 50.
            || !(1..=4).contains(&digits.len())
            || digits.starts_with('0')
            || !digits.bytes().all(|b| b.is_ascii_digit())
            || first.blk_no.len() > digits.len() + 1
            || first.road_name.is_empty()
            || first.road_name == "NIL"
            || first.road_name.contains(',')
        {
            continue;
        }
        rows.insert(
            probe.postal.clone(),
            Address {
                postal: probe.postal,
                block: first.blk_no.clone(),
                road: first.road_name.clone(),
                buildings: hits
                    .iter()
                    .filter(|h| h.building != "NIL")
                    .map(|h| h.building.clone())
                    .collect(),
                point: points[0],
            },
        );
    }
    Ok((snapshot.seed, rows))
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let (drawn, rows) = load(path)?;
    ensure!(
        drawn == seed,
        "the OneMap snapshot drew its postcodes with seed {drawn:?}, not {seed:?}"
    );
    let mut lines = vec![];
    for a in rows.values() {
        let category = a.category();
        let [(_, label), _] = a.queries();
        for (form, query) in a.queries() {
            lines.push(json!({"kind":"address", "source":"onemap", "ref":a.postal, "cohort":format!("sg_{category}"), "queries":[query], "label":label, "tags":["registry-sample", "sg", "onemap", category, form, format!("sector:{}", &a.postal[..2]), if a.buildings.is_empty() { "unnamed" } else { "named" }]}));
        }
    }
    ensure!(!lines.is_empty(), "no eligible OneMap postcodes");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "onemap",
    command: "sample-sg",
    country: "sg",
    input: "onemap",
    license: "SG-ODL-1.0",
    location: "onemap",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert(
        "coordinates".into(),
        json!(
            "LATITUDE/LONGITUDE of OneMap's first search result for the postcode, WGS84 as returned"
        ),
    );
    let (_, rows) = load(path)?;
    refs.iter().map(|postal| {
        let a = rows.get(postal).with_context(|| format!("OneMap snapshot lacks eligible postcode {postal}"))?;
        let mut provenance = metadata.clone();
        provenance.insert("record".into(), json!({"postal":a.postal, "blk_no":a.block, "road_name":a.road, "buildings":a.buildings}));
        Ok((postal.clone(), Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.block.clone()), ("postcode", a.postal.clone())]), radius_floor_km: 0., provenance, number_aliases: None }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_block_per_postcode_query_forms_and_seed() {
        let path = std::env::temp_dir().join(format!("onemap-test-{}.json", std::process::id()));
        let hit = |postal: &str, blk: &str, road: &str, building: &str, lat: f64| json!({"POSTAL":postal, "BLK_NO":blk, "ROAD_NAME":road, "BUILDING":building, "LATITUDE":lat.to_string(), "LONGITUDE":"103.85"});
        let probes = json!([
            {"postal":"200640", "results":[hit("200640", "640", "ROWELL ROAD", "NIL", 1.3074)]},
            {"postal":"018989", "results":[hit("018989", "1", "MARINA BOULEVARD", "ONE MARINA BOULEVARD", 1.2822), hit("018989", "1", "MARINA BOULEVARD", "ANNEX", 1.2823)]},
            {"postal":"123456", "results":[]},
            {"postal":"238823", "results":[hit("238823", "", "ORCHARD ROAD", "ISTANA", 1.307)]},
            {"postal":"520201", "results":[hit("520201", "201", "TAMPINES STREET 21", "NIL", 1.35), hit("520201", "203", "TAMPINES STREET 21", "NIL", 1.35)]},
            {"postal":"560101", "results":[hit("560101", "101", "ANG MO KIO AVENUE 3", "A", 1.36), hit("560101", "101", "ANG MO KIO AVENUE 3", "B", 1.37)]},
            {"postal":"650123", "results":[hit("999999", "12", "OTHER ROAD", "NIL", 1.35), hit("650123", "123A", "BUKIT BATOK STREET 1", "NIL", 1.35)]},
            {"postal":"760201", "results":[hit("760201", "2-4", "YISHUN ROAD", "NIL", 1.43)]},
        ]);
        let write = |probes: &Value| {
            std::fs::write(
                &path,
                serde_json::to_vec(&json!({"seed":"test", "probes":probes})).unwrap(),
            )
            .unwrap()
        };
        write(&probes);
        let lines = sample(&path, "test").unwrap();
        let queries: Vec<_> = lines
            .iter()
            .map(|l| {
                (
                    l["ref"].as_str().unwrap(),
                    l["queries"][0].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            queries,
            [
                ("018989", "1 MARINA BOULEVARD, SINGAPORE 018989"),
                ("018989", "1 MARINA BOULEVARD, SINGAPORE"),
                ("200640", "640 ROWELL ROAD, SINGAPORE 200640"),
                ("200640", "640 ROWELL ROAD, SINGAPORE"),
                ("650123", "123A BUKIT BATOK STREET 1, SINGAPORE 650123"),
                ("650123", "123A BUKIT BATOK STREET 1, SINGAPORE"),
            ]
        );
        assert_eq!(lines[4]["cohort"], "sg_suffix");
        assert!(sample(&path, "other").is_err());
        let entries = prepare(&path, &BTreeSet::from(["018989".into()])).unwrap();
        let entry = &entries["018989"];
        assert_eq!(entry.components["house_number"], "1");
        assert_eq!(entry.components["postcode"], "018989");
        assert!(matches!(entry.point, Point::Wgs84([lat, _]) if lat == 1.2822));
        assert!(prepare(&path, &BTreeSet::from(["520201".into()])).is_err());
        let mut duplicate = probes.as_array().unwrap().clone();
        duplicate.push(duplicate[0].clone());
        write(&Value::from(duplicate));
        assert!(sample(&path, "test").is_err());
        write(
            &json!([{"postal":"200640", "results":[hit("200640", "640", "ROWELL ROAD", "NIL", 45.)]}]),
        );
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
