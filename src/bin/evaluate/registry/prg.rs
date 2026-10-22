use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
    sync::LazyLock,
};

const URL: &str = "https://www.geoportal.gov.pl/pl/dane/panstwowy-rejestr-granic-prg/";

#[derive(Clone)]
struct Address {
    id: String,
    province: String,
    municipality: String,
    city: String,
    street: String,
    number: String,
    postcode: String,
    category: &'static str,
    point: [f64; 2],
    attributes: Value,
}

fn text<'a>(row: &'a Value, key: &str) -> Result<&'a str> {
    let value = row[key]
        .as_str()
        .with_context(|| format!("PRG missing {key}"))?;
    ensure!(
        !value.chars().any(char::is_control),
        "PRG control character in {key}"
    );
    Ok(value.trim())
}

impl Address {
    fn query(&self) -> String {
        if self.street.is_empty() {
            format!("{} {}, {}, Poland", self.city, self.number, self.postcode)
        } else {
            format!(
                "{} {}, {} {}, Poland",
                self.street, self.number, self.postcode, self.city
            )
        }
    }

    fn queries(&self) -> Vec<(&'static str, String)> {
        let query = self.query();
        let ascii = deunicode::deunicode(&query);
        let mut queries = vec![("native", query.clone())];
        if ascii != query {
            queries.push(("ascii", ascii));
        }
        queries
    }
}

fn load(path: &Path) -> Result<(Value, BTreeMap<String, Address>)> {
    static GUID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$").unwrap());
    static NUMBER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[1-9][0-9]*[ A-Za-z0-9/-]*$").unwrap());
    static POSTCODE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[0-9]{2}-[0-9]{3}$").unwrap());
    let data: Value = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    ensure!(
        data["capture"]["geometry_crs"] == "EPSG:4326"
            && data["capture"]["source_crs"] == "EPSG:2180"
            && data["capture"]["gml_axis_order"] == "northing,easting"
            && data["capture"]["always_xy"] == true,
        "PRG CRS/axis metadata"
    );
    let mut rows = BTreeMap::new();
    for attributes in data["addresses"].as_array().context("PRG addresses")? {
        let id = text(attributes, "id")?;
        let province = text(attributes, "province")?;
        let province_number: u8 = province.parse()?;
        ensure!(
            GUID.is_match(id)
                && text(attributes, "namespace")? == "PL.PZGIK.200"
                && text(attributes, "valid_to")?.is_empty(),
            "PRG invalid/closed address identity"
        );
        ensure!(
            province.len() == 2
                && (2..=32).contains(&province_number)
                && province_number.is_multiple_of(2),
            "PRG province"
        );
        let locality = &attributes["locality"];
        let municipality = text(locality, "municipality")?;
        ensure!(
            municipality.len() == 7
                && municipality.starts_with(province)
                && municipality.bytes().all(|b| b.is_ascii_digit())
                && text(attributes, "locality_ref")? == text(locality, "gml_id")?
                && text(locality, "valid_to")?.is_empty(),
            "PRG locality reference/municipality"
        );
        let city = text(locality, "name")?;
        let number = text(attributes, "number")?;
        let postcode = text(attributes, "postcode")?;
        ensure!(
            !city.is_empty() && NUMBER.is_match(number) && POSTCODE.is_match(postcode),
            "PRG invalid city, number or postcode"
        );
        let street_ref = text(attributes, "street_ref")?;
        let (street, street_kind) = if attributes["street"].is_null() {
            ensure!(street_ref.is_empty(), "PRG unresolved street");
            ("", "")
        } else {
            let street = &attributes["street"];
            ensure!(
                text(street, "gml_id")? == street_ref
                    && text(street, "valid_to")?.is_empty()
                    && street["places"]
                        .as_array()
                        .context("PRG street places")?
                        .iter()
                        .any(|v| v == &attributes["locality_ref"]),
                "PRG conflicting street locality"
            );
            let name = text(street, "name")?;
            let kind = text(street, "kind")?;
            ensure!(
                !name.is_empty() && kind.parse::<u8>().is_ok_and(|k| (1..=16).contains(&k)),
                "PRG invalid street name/type"
            );
            (name, kind)
        };
        let suffix = number.bytes().any(|b| b.is_ascii_alphabetic());
        let category = if number.contains(['/', '-']) {
            "compound"
        } else if street.is_empty() {
            if suffix { "village_suffix" } else { "village" }
        } else if suffix {
            "suffix"
        } else if street_kind != "1" {
            "other_road"
        } else {
            "plain"
        };
        let point = [
            attributes["point"][0].as_f64().context("PRG latitude")?,
            attributes["point"][1].as_f64().context("PRG longitude")?,
        ];
        ensure!(
            (48. ..=56.).contains(&point[0]) && (14. ..=25.).contains(&point[1]),
            "PRG point outside Poland"
        );
        let address = Address {
            id: id.into(),
            province: province.into(),
            municipality: municipality.into(),
            city: city.into(),
            street: street.into(),
            number: number.into(),
            postcode: postcode.into(),
            category,
            point,
            attributes: attributes.clone(),
        };
        ensure!(
            rows.insert(id.to_owned(), address).is_none(),
            "duplicate PRG address UUID"
        );
    }
    ensure!(!rows.is_empty(), "no eligible PRG addresses");
    Ok((data["capture"].clone(), rows))
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let (_, rows) = load(path)?;
    let mut pools = BTreeMap::<_, Vec<_>>::new();
    for row in rows.into_values() {
        pools
            .entry((row.province.clone(), row.category))
            .or_default()
            .push(row);
    }
    let mut lines = vec![];
    for ((province, category), mut pool) in pools {
        pool.sort_by_key(|a| sha256_hex(format!("{seed}\0{}", a.id).as_bytes()));
        let mut streets = BTreeSet::new();
        for a in pool
            .into_iter()
            .filter(|a| {
                streets.insert((
                    a.municipality.clone(),
                    if a.street.is_empty() {
                        a.city.clone()
                    } else {
                        a.street.clone()
                    },
                ))
            })
            .take(6)
        {
            for (form, query) in a.queries() {
                lines.push(json!({"kind":"address", "source":"prg-json", "ref":a.id, "cohort":format!("pl_{category}"), "queries":[query], "label":a.query(), "tags":["registry-sample", "pl", province, category, form]}));
            }
        }
    }
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "prg-json",
    command: "sample-pl",
    country: "pl",
    input: "prg",
    license: "GUGiK-PRG-open-data",
    location: "prg-address-point",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let (mut capture, rows) = load(path)?;
    let members = capture
        .as_object_mut()
        .context("PRG capture")?
        .remove("members")
        .context("PRG capture members")?;
    let members = members.as_array().context("PRG capture member array")?;
    let mut metadata = super::provenance(path)?;
    metadata.insert("capture".into(), capture);
    metadata.insert("attribution".into(), json!("Główny Urząd Geodezji i Kartografii (GUGiK), Państwowy Rejestr Granic; adapted from the official national address-point export. Geoportal terms, Article 3: publication permitted with source attribution."));
    refs.iter()
        .map(|id| {
            let a = rows
                .get(id)
                .with_context(|| format!("PRG lacks address {id}"))?;
            let mut provenance = metadata.clone();
            let member = members
                .iter()
                .find(|m| {
                    m["file"]
                        .as_str()
                        .is_some_and(|s| s.starts_with(&format!("{}_", a.province)))
                })
                .context("PRG province capture absent")?;
            provenance.insert("province_capture".into(), member.clone());
            provenance.insert("attributes".into(), a.attributes.clone());
            Ok((
                id.clone(),
                Entry {
                    point: Point::Wgs84(a.point),
                    components: indexmap::IndexMap::from([
                        ("house_number", a.number.clone()),
                        ("postcode", a.postcode.clone()),
                    ]),
                    radius_floor_km: 0.15,
                    provenance,
                    number_aliases: None,
                    places: super::places([a.city.as_str()]),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_and_ascii_forms_keep_complete_numbers_and_source_point() {
        let path = std::env::temp_dir().join(format!("prg-test-{}.json", std::process::id()));
        let mut data = json!({"capture":{"geometry_crs":"EPSG:4326","source_crs":"EPSG:2180","gml_axis_order":"northing,easting","always_xy":true,"members":[{"file":"26_example.gml"}]},"addresses":[{
            "id":"00000001-0000-4000-8000-000000000000","namespace":"PL.PZGIK.200","valid_to":"","province":"26","locality_ref":"city","street_ref":"street","number":"12A/14","postcode":"26-000","point":[51.0,20.7],
            "locality":{"gml_id":"city","name":"Wieś Próbna","municipality":"2600001","valid_to":""},
            "street":{"gml_id":"street","name":"Próbna","kind":"1","places":["city"],"valid_to":""}
        }]});
        let write =
            |value: &Value| std::fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
        write(&data);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0]["cohort"], "pl_compound");
        assert!(
            first[1]["queries"][0]
                .as_str()
                .unwrap()
                .starts_with("Probna 12A/14")
        );
        assert_eq!(first[0]["ref"], first[1]["ref"]);
        let refs = BTreeSet::from([first[0]["ref"].as_str().unwrap().to_owned()]);
        let entries = prepare(&path, &refs).unwrap();
        let entry = entries.values().next().unwrap();
        assert_eq!(entry.components["house_number"], "12A/14");
        assert!(entry.number_aliases.is_none());
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        data["addresses"][0]["street"] = Value::Null;
        data["addresses"][0]["street_ref"] = json!("");
        data["addresses"][0]["number"] = json!("12A");
        write(&data);
        let village = sample(&path, "test").unwrap();
        assert_eq!(village[0]["cohort"], "pl_village_suffix");
        assert_eq!(village[0]["queries"][0], "Wieś Próbna 12A, 26-000, Poland");
        data["addresses"][0]["point"] = json!([20.7, 51.0]);
        write(&data);
        assert!(sample(&path, "test").is_err());
        data["addresses"][0]["point"] = json!([51.0, 20.7]);
        data["addresses"][0]["valid_to"] = json!("2020-01-01");
        write(&data);
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
