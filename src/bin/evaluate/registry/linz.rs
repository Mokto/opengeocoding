use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
};

const URL: &str = "https://catalogue.data.govt.nz/dataset/nz-addresses";

#[derive(Deserialize)]
struct Snapshot {
    features: Vec<Feature>,
}
#[derive(Deserialize)]
struct Feature {
    attributes: Address,
    geometry: Option<Geometry>,
}
#[derive(Clone, Deserialize)]
struct Address {
    address_id: u64,
    road_id: u64,
    full_address: String,
    full_address_ascii: String,
    full_address_number: String,
    full_road_name: String,
    territorial_authority: String,
    unit: Option<String>,
    address_number: u64,
    address_number_suffix: Option<String>,
    address_number_high: Option<u64>,
    address_lifecycle: String,
    #[serde(skip)]
    point: [f64; 2],
}
#[derive(Deserialize)]
struct Geometry {
    x: f64,
    y: f64,
}

impl Address {
    fn building_number(&self) -> String {
        let mut number = format!(
            "{}{}",
            self.address_number,
            self.address_number_suffix.as_deref().unwrap_or("")
        );
        if let Some(high) = self.address_number_high {
            number.push_str(&format!("-{high}"));
        }
        number
    }

    fn id(&self) -> String {
        sha256_hex(format!("{}\0{}", self.road_id, self.building_number()).as_bytes())
    }

    fn category(&self) -> &'static str {
        if self.unit.as_ref().is_some_and(|s| !s.is_empty()) {
            "unit"
        } else if self.address_number_high.is_some() {
            "range"
        } else if self
            .address_number_suffix
            .as_ref()
            .is_some_and(|s| !s.is_empty())
        {
            "suffix"
        } else if self.full_address != self.full_address_ascii {
            "macron"
        } else {
            "plain"
        }
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Address>> {
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    let mut ids = BTreeSet::new();
    let mut rows = BTreeMap::<String, Address>::new();
    for feature in snapshot.features {
        let mut a = feature.attributes;
        ensure!(ids.insert(a.address_id), "duplicate LINZ address_id");
        let Some(point) = feature.geometry else {
            continue;
        };
        ensure!(
            (-48. ..=-33.).contains(&point.y)
                && ((165. ..=180.).contains(&point.x) || (-180. ..=-175.).contains(&point.x)),
            "LINZ needs New Zealand WGS84 coordinates"
        );
        if a.address_lifecycle != "Current"
            || a.address_number == 0
            || a.road_id == 0
            || a.territorial_authority.is_empty()
            || a.full_address.is_empty()
            || a.full_road_name.is_empty()
        {
            continue;
        }
        a.point = [point.y, point.x];
        let id = a.id();
        if rows
            .get(&id)
            .is_none_or(|old| a.address_id < old.address_id)
        {
            rows.insert(id, a);
        }
    }
    Ok(rows)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<_, Vec<Address>>::new();
    for a in load(path)?.into_values() {
        pools
            .entry((a.territorial_authority.clone(), a.category()))
            .or_default()
            .push(a);
    }
    let mut lines = vec![];
    for ((authority, category), mut pool) in pools {
        pool.sort_by_key(|a| sha256_hex(format!("{seed}\0{}", a.id()).as_bytes()));
        let mut streets = BTreeSet::new();
        for a in pool
            .into_iter()
            .filter(|a| streets.insert(a.road_id))
            .take(2)
        {
            let mut queries = vec![("native", a.full_address.clone())];
            if !a.full_address_ascii.is_empty() && a.full_address_ascii != a.full_address {
                queries.push(("ascii", a.full_address_ascii.clone()));
            }
            for (form, query) in queries {
                lines.push(json!({"kind":"address", "source":"linz-addresses", "ref":a.id(), "cohort":format!("nz_{category}"), "queries":[format!("{query}, New Zealand")], "label":a.full_address, "tags":["registry-sample", "nz", category, form, format!("authority:{authority}")]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible LINZ addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "linz-addresses",
    command: "sample-nz",
    country: "nz",
    input: "linz",
    license: "CC-BY-4.0",
    location: "linz-building",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    let rows = load(path)?;
    refs.iter().map(|id| {
        let a = rows.get(id).with_context(|| format!("LINZ lacks building {id}"))?;
        let mut provenance = metadata.clone();
        provenance.extend(serde_json::from_value::<serde_json::Map<String, Value>>(json!({"address_id":a.address_id, "road_id":a.road_id, "address":a.full_address, "ascii_address":a.full_address_ascii, "unit":a.unit, "territorial_authority":a.territorial_authority, "coordinate_transform":"LINZ ArcGIS EPSG:2193 to EPSG:4326, outSR=4326", "location_key":"SHA256(road_id, building_number), NUL separated"}))?);
        let numbers = BTreeSet::from([a.building_number(), a.full_address_number.clone()]);
        Ok((id.clone(), Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.building_number())]), radius_floor_km: 0.15, provenance, number_aliases: (numbers.len() > 1).then_some(numbers), places: super::places(a.full_address.split(", ").skip(1).chain([a.territorial_authority.as_str()])) }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_ascii_and_units_keep_one_building_without_expanding_ranges() {
        let path = std::env::temp_dir().join(format!("linz-test-{}.json", std::process::id()));
        let a = json!({"attributes":{"address_id":1,"road_id":2,"full_address":"2/12 Māori Road, Town","full_address_ascii":"2/12 Maori Road, Town","full_address_number":"2/12","full_road_name":"Māori Road","territorial_authority":"Authority","unit":"2","address_number":12,"address_number_suffix":null,"address_number_high":null,"address_lifecycle":"Current"},"geometry":{"x":174.1,"y":-41.1}});
        let mut b = a.clone();
        b["attributes"]["address_id"] = json!(2);
        let write = |rows: Vec<Value>| {
            std::fs::write(
                &path,
                serde_json::to_vec(&json!({"features":rows})).unwrap(),
            )
            .unwrap()
        };
        write(vec![a.clone(), b.clone()]);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0]["ref"], first[1]["ref"]);
        write(vec![b, a.clone()]);
        assert_eq!(first, sample(&path, "test").unwrap());
        let entries = prepare(
            &path,
            &BTreeSet::from([first[0]["ref"].as_str().unwrap().into()]),
        )
        .unwrap();
        assert_eq!(
            entries.values().next().unwrap().number_aliases,
            Some(BTreeSet::from(["12".into(), "2/12".into()]))
        );
        assert_eq!(
            entries.values().next().unwrap().places,
            ["Town", "Authority"]
        );
        let mut range: Address = serde_json::from_value(a["attributes"].clone()).unwrap();
        range.address_number_high = Some(16);
        assert_eq!(range.building_number(), "12-16");
        write(vec![a.clone(), a]);
        assert!(load(&path).is_err());
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
