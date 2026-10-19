use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
    sync::LazyLock,
};

const URL: &str = "https://datosabiertos.bogota.gov.co/dataset/placa-domiciliaria";
static PLATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]+[A-Z]*(?:BIS[A-Z]*)?)\s+([0-9]+)(?:\s+([ES]))?$").unwrap()
});

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
    #[serde(rename = "PDOCODIGO")]
    id: String,
    #[serde(rename = "PDOTEXTO")]
    plate: String,
    #[serde(rename = "PDONVIAL")]
    road: String,
    #[serde(rename = "PDOCLOTE")]
    lot: String,
    #[serde(rename = "PDOTIPO")]
    kind: u32,
    #[serde(skip)]
    point: [f64; 2],
}
#[derive(Deserialize)]
struct Geometry {
    x: f64,
    y: f64,
}

impl Address {
    fn prefix(&self) -> &str {
        self.road.split_whitespace().next().unwrap_or("")
    }
    fn number(&self) -> Option<String> {
        let c = PLATE.captures(self.plate.trim())?;
        Some(format!(
            "{}-{}{}",
            &c[1],
            &c[2],
            c.get(3)
                .map_or(String::new(), |d| format!(" {}", d.as_str()))
        ))
    }
    fn expanded_road(&self) -> Option<String> {
        let prefix = match self.prefix() {
            "CL" => "Calle",
            "KR" => "Carrera",
            "DG" => "Diagonal",
            "TV" => "Transversal",
            "AC" => "Avenida Calle",
            "AK" => "Avenida Carrera",
            _ => return None,
        };
        Some(format!(
            "{prefix} {}",
            self.road
                .split_whitespace()
                .skip(1)
                .collect::<Vec<_>>()
                .join(" ")
        ))
    }
    fn category(&self) -> &'static str {
        if self
            .road
            .split_whitespace()
            .chain(self.plate.split_whitespace())
            .any(|t| matches!(t, "E" | "S"))
        {
            "direction"
        } else if self
            .road
            .chars()
            .skip(3)
            .chain(self.plate.chars())
            .any(char::is_alphabetic)
        {
            "suffix"
        } else {
            "plain"
        }
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Address>> {
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    let mut ids = BTreeSet::new();
    let mut rows = BTreeMap::new();
    for f in snapshot.features {
        let mut a = f.attributes;
        ensure!(
            !a.id.is_empty() && ids.insert(a.id.clone()),
            "empty or duplicate Bogotá plate ID"
        );
        let Some(p) = f.geometry else {
            continue;
        };
        ensure!(
            (3.5..5.0).contains(&p.y) && (-74.6..-73.5).contains(&p.x),
            "Bogotá needs local WGS84 coordinates"
        );
        if a.kind != 1 || a.lot.is_empty() || a.number().is_none() || a.expanded_road().is_none() {
            continue;
        }
        a.point = [p.y, p.x];
        rows.insert(a.id.clone(), a);
    }
    Ok(rows)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<_, Vec<Address>>::new();
    for a in load(path)?.into_values() {
        pools
            .entry((a.prefix().to_owned(), a.category()))
            .or_default()
            .push(a);
    }
    let mut lots = BTreeSet::new();
    let mut lines = vec![];
    for ((prefix, category), mut pool) in pools {
        pool.sort_by_key(|a| sha256_hex(format!("{seed}\0{}", a.id).as_bytes()));
        let mut streets = BTreeSet::new();
        for a in pool
            .into_iter()
            .filter(|a| {
                !lots.contains(&a.lot)
                    && streets.insert(a.road.split_whitespace().collect::<Vec<_>>().join(" "))
                    && lots.insert(a.lot.clone())
            })
            .take(20)
        {
            let number = a.number().unwrap();
            for (form, street, separator) in [
                (
                    "native",
                    a.road.split_whitespace().collect::<Vec<_>>().join(" "),
                    " # ",
                ),
                ("expanded", a.expanded_road().unwrap(), " # "),
                ("no_hash", a.expanded_road().unwrap(), " "),
            ] {
                lines.push(json!({"kind":"address", "source":"bogota-plates", "ref":a.id, "cohort":format!("co_{prefix}"), "queries":[format!("{street}{separator}{number}, Bogotá, Colombia")], "label":format!("{} # {number}, Bogotá", a.road.trim()), "tags":["registry-sample", "municipal-sample", "co", prefix, category, form]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible Bogotá principal plates");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "bogota-plates",
    command: "sample-co",
    country: "co",
    input: "bogota",
    license: "CC-BY-4.0",
    location: "bogota-plate",
    url: |id| format!("{URL}#{id}"),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    let rows = load(path)?;
    refs.iter().map(|id| {
        let a = rows.get(id).with_context(|| format!("Bogotá lacks eligible plate {id}"))?;
        let mut provenance = metadata.clone();
        provenance.extend(serde_json::from_value::<serde_json::Map<String, Value>>(json!({"plate_id":a.id,"lot_id":a.lot,"road":a.road,"plate_text":a.plate,"plate_type":a.kind,"coordinate_system":"EPSG:4326, official ArcGIS outSR=4326"}))?);
        Ok((id.clone(), Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.number().unwrap())]), radius_floor_km: 0.15, provenance, number_aliases: None }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn crossing_number_letters_and_direction_survive_without_lot_or_unit_invention() {
        let path = std::env::temp_dir().join(format!("bogota-test-{}.json", std::process::id()));
        let a = json!({"attributes":{"PDOCODIGO":"A","PDOTEXTO":"3A 71 E","PDONVIAL":"CL 41A S","PDOCLOTE":"lot1","PDOTIPO":1},"geometry":{"x":-74.1,"y":4.6}});
        let mut b = a.clone();
        b["attributes"]["PDOCODIGO"] = json!("B");
        b["attributes"]["PDOTEXTO"] = json!("MJ");
        let write = |rows: Vec<Value>| {
            std::fs::write(
                &path,
                serde_json::to_vec(&json!({"features":rows})).unwrap(),
            )
            .unwrap()
        };
        write(vec![a.clone(), b.clone()]);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 3);
        assert!(first.iter().all(|s| s["ref"] == "A"));
        assert_eq!(
            first[1]["queries"][0],
            "Calle 41A S # 3A-71 E, Bogotá, Colombia"
        );
        write(vec![b, a.clone()]);
        assert_eq!(first, sample(&path, "test").unwrap());
        assert_eq!(
            prepare(&path, &BTreeSet::from(["A".into()])).unwrap()["A"].components["house_number"],
            "3A-71 E"
        );
        write(vec![a.clone(), a]);
        assert!(load(&path).is_err());
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
