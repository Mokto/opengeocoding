use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
};

const URL: &str = "https://opendata.bosa.be/index.fr.html";

#[derive(Clone)]
struct Address {
    id: String,
    registry_id: String,
    municipality: String,
    region: String,
    street_id: String,
    names: Vec<(String, String, String)>,
    number: String,
    unit: String,
    postcode: String,
    point: [f64; 2],
}

impl Address {
    fn category(&self) -> &'static str {
        if !self.unit.is_empty() {
            "unit"
        } else if self.number.chars().any(char::is_alphabetic) {
            "suffix"
        } else if self.names.len() > 1 {
            "multilingual"
        } else {
            "plain"
        }
    }

    fn queries(&self) -> Vec<(&str, String)> {
        let mut queries: Vec<_> = self
            .names
            .iter()
            .map(|(language, street, town)| {
                (
                    language.as_str(),
                    format!(
                        "{street} {}, {} {town}, Belgium",
                        self.number, self.postcode
                    ),
                )
            })
            .collect();
        if !self.unit.is_empty() {
            let (language, street, town) = &self.names[0];
            let marker = match language.as_str() {
                "nl" => "bus",
                "fr" => "boîte",
                _ => "Box",
            };
            queries.push((
                "unit",
                format!(
                    "{street} {} {marker} {}, {} {town}, Belgium",
                    self.number, self.unit, self.postcode
                ),
            ));
        }
        let mut seen = BTreeSet::new();
        queries.retain(|(_, q)| seen.insert(q.clone()));
        queries
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Address>> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        super::ban::record(&mut reader, &mut line)?,
        "empty BeSt CSV"
    );
    let columns: BTreeMap<_, _> = super::nad::split(line.trim_end(), ',')
        .into_iter()
        .enumerate()
        .map(|(i, s)| (s.to_string(), i))
        .collect();
    for name in [
        "address_id",
        "municipality_id",
        "street_id",
        "region_code",
        "house_number",
        "box_number",
        "postcode",
        "status",
        "EPSG:4326_lat",
        "EPSG:4326_lon",
        "streetname_nl",
        "streetname_fr",
        "streetname_de",
        "municipality_name_nl",
        "municipality_name_fr",
        "municipality_name_de",
    ] {
        ensure!(columns.contains_key(name), "BeSt CSV missing {name}");
    }
    let mut addresses = BTreeMap::<String, Address>::new();
    let mut count = 0usize;
    let mut invalid_points = 0usize;
    while super::ban::record(&mut reader, &mut line)? {
        count += 1;
        if count.is_multiple_of(1_000_000) {
            eprintln!("BeSt: {count} rows");
        }
        let fields = super::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == columns.len(),
            "BeSt row {count}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        if field("status") != "current" {
            continue;
        }
        let lat = field("EPSG:4326_lat").parse::<f64>().unwrap_or(f64::NAN);
        let lon = field("EPSG:4326_lon").parse::<f64>().unwrap_or(f64::NAN);
        if !(49. ..=52.).contains(&lat) || !(2. ..=7.).contains(&lon) {
            invalid_points += 1;
            continue;
        }
        let names: Vec<_> = ["nl", "fr", "de"]
            .into_iter()
            .filter_map(|language| {
                let street = field(&format!("streetname_{language}"));
                let town = field(&format!("municipality_name_{language}"));
                (!street.is_empty() && !town.is_empty())
                    .then(|| (language.into(), street.into(), town.into()))
            })
            .collect();
        if names.is_empty()
            || ["address_id", "municipality_id", "street_id", "house_number"]
                .iter()
                .any(|n| field(n).is_empty())
            || field("postcode").len() != 4
        {
            continue;
        }
        ensure!(
            ["BE-BRU", "BE-VLG", "BE-WAL"].contains(&field("region_code")),
            "BeSt region"
        );
        let id = sha256_hex(
            format!(
                "{}\0{}\0{}\0{}",
                field("region_code"),
                field("municipality_id"),
                field("street_id"),
                field("house_number")
            )
            .as_bytes(),
        );
        let row = Address {
            id: id.clone(),
            registry_id: field("address_id").into(),
            municipality: field("municipality_id").into(),
            region: field("region_code").into(),
            street_id: field("street_id").into(),
            names,
            number: field("house_number").into(),
            unit: field("box_number").into(),
            postcode: field("postcode").into(),
            point: [lat, lon],
        };
        if addresses.get(&id).is_none_or(|old| {
            (row.unit.is_empty(), &row.registry_id) < (old.unit.is_empty(), &old.registry_id)
        }) {
            addresses.insert(id, row);
        }
    }
    eprintln!(
        "BeSt: {count} rows, {invalid_points} current rows without a valid local point, {} eligible building keys",
        addresses.len()
    );
    Ok(addresses)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<_, Vec<Address>>::new();
    for a in load(path)?.into_values() {
        pools
            .entry((a.region.clone(), a.category()))
            .or_default()
            .push(a);
    }
    let mut lines = vec![];
    for ((region, category), mut pool) in pools {
        pool.sort_by_key(|a| sha256_hex(format!("{seed}\0{}", a.id).as_bytes()));
        let mut streets = BTreeSet::new();
        for a in pool
            .into_iter()
            .filter(|a| streets.insert((a.municipality.clone(), a.street_id.clone())))
            .take(20)
        {
            for (form, query) in a.queries() {
                lines.push(json!({"kind":"address", "source":"best-csv", "ref":a.id, "cohort":format!("be_{form}"), "queries":[query], "label":a.queries()[0].1, "tags":["registry-sample", "be", region, category, form]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible BeSt addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "best-csv",
    command: "sample-be",
    country: "be",
    input: "best",
    license: "CC-BY-4.0",
    location: "best-building",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    let rows = load(path)?;
    refs.iter().map(|id| {
        let a = rows.get(id).with_context(|| format!("BeSt lacks building {id}"))?;
        let mut provenance = metadata.clone();
        provenance.extend(serde_json::from_value::<serde_json::Map<String, Value>>(json!({"address_id":a.registry_id, "municipality_id":a.municipality, "street_id":a.street_id, "region":a.region, "names":a.names, "house_number":a.number, "box_number":a.unit, "coordinate_system":"EPSG:4326", "location_key":"SHA256(region, municipality_id, street_id, house_number), NUL separated"}))?);
        Ok((id.clone(), Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.number.clone()), ("postcode", a.postcode.clone())]), radius_floor_km: 0.15, provenance, number_aliases: None, places: super::places(a.names.iter().map(|(_, _, municipality)| municipality.as_str())) }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilingual_and_unit_forms_share_a_building_and_ignore_input_order() {
        let path = std::env::temp_dir().join(format!("best-test-{}.csv", std::process::id()));
        let header = "address_id,municipality_id,street_id,region_code,house_number,box_number,postcode,status,EPSG:4326_lat,EPSG:4326_lon,streetname_nl,streetname_fr,streetname_de,municipality_name_nl,municipality_name_fr,municipality_name_de\n";
        let rows = [
            "1,21001,2,BE-BRU,12,,1070,current,50.83,4.31,Bergense Steenweg,Chaussée de Mons,,Anderlecht,Anderlecht,\n",
            "2,21001,2,BE-BRU,12,A,1070,current,50.83,4.31,Bergense Steenweg,Chaussée de Mons,,Anderlecht,Anderlecht,\n",
        ];
        std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 3);
        assert!(first.iter().all(|s| s["ref"] == first[0]["ref"]));
        assert!(first[2]["queries"][0].as_str().unwrap().contains("bus A"));
        std::fs::write(&path, format!("{header}{}{}", rows[1], rows[0])).unwrap();
        assert_eq!(first, sample(&path, "test").unwrap());
        let entries = prepare(
            &path,
            &BTreeSet::from([first[0]["ref"].as_str().unwrap().into()]),
        )
        .unwrap();
        assert_eq!(
            entries.values().next().unwrap().components["house_number"],
            "12"
        );
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
