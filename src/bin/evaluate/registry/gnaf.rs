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

const URL: &str =
    "https://digital.atlas.gov.au/datasets/digitalatlas::national-address-points/about";

#[derive(Clone)]
struct Address {
    id: String,
    registry_id: String,
    state: String,
    street: String,
    locality: String,
    postcode: String,
    number: String,
    flat: String,
    flat_type: String,
    level: String,
    level_type: String,
    category: &'static str,
    point: [f64; 2],
    attributes: Value,
}

fn text(row: &Value, name: &str) -> Result<String> {
    match row
        .get(name)
        .with_context(|| format!("G-NAF missing {name}"))?
    {
        Value::String(s) => Ok(s.trim().into()),
        Value::Number(n) if n.is_i64() => Ok(n.to_string()),
        Value::Null => Ok(String::new()),
        _ => anyhow::bail!("G-NAF invalid {name}"),
    }
}

impl Address {
    fn query(&self, address: &str) -> String {
        format!(
            "{address} {}, {} {} {}, Australia",
            self.street, self.locality, self.state, self.postcode
        )
    }

    fn queries(&self) -> Vec<(&'static str, String)> {
        let mut queries = vec![("building", self.query(&self.number))];
        if !self.flat.is_empty() {
            queries.push((
                "unit",
                self.query(&format!("{} {} {}", self.flat_type, self.flat, self.number)),
            ));
            queries.push((
                "slash",
                self.query(&format!("{}/{}", self.flat, self.number)),
            ));
        }
        if !self.level.is_empty() {
            queries.push((
                "level",
                self.query(&format!(
                    "{} {}, {}",
                    self.level_type, self.level, self.number
                )),
            ));
        }
        queries
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Address>> {
    let data: Value = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    ensure!(
        data["capture"]["geometry_crs"] == "EPSG:4326",
        "G-NAF capture CRS"
    );
    let features = data["features"].as_array().context("G-NAF features")?;
    let mut ids = BTreeSet::new();
    let mut addresses = BTreeMap::<String, Address>::new();
    for feature in features {
        let row = &feature["attributes"];
        let s = |name: &str| text(row, name);
        let registry_id = s("ADDRESS_DETAIL_PID")?;
        ensure!(
            !registry_id.is_empty() && ids.insert(registry_id.clone()),
            "duplicate or empty G-NAF PID"
        );
        let state = s("STATE")?;
        let quality = s("GEOCODE_TYPE")?;
        if !["ACT", "NSW", "NT", "QLD", "SA", "TAS", "VIC", "WA"].contains(&state.as_str())
            || row.get("DATE_RETIRED").context("G-NAF retirement")? != &Value::Null
            || s("ALIAS_PRINCIPAL")? != "PRINCIPAL"
            || s("LEVEL_GEOCODED")?.replace(' ', "") != "LOCALITY,STREET,ADDRESS"
            || ![
                "PROPERTY CENTROID",
                "PROPERTY CENTROID MANUAL",
                "PROPERTY ACCESS POINT",
                "PROPERTY ACCESS POINT SETBACK",
                "BUILDING CENTROID",
                "BUILDING CENTROID MANUAL",
                "FRONTAGE CENTRE SETBACK",
                "UNIT CENTROID",
            ]
            .contains(&quality.as_str())
            || row["CONFIDENCE"].as_i64().unwrap_or(-1) < 1
            || row["NUMBER_FIRST"].as_i64().unwrap_or(0) <= 0
        {
            continue;
        }
        let part = |prefix: &str| -> Result<String> {
            Ok(format!(
                "{}{}{}",
                s(&format!("{prefix}_PREFIX"))?,
                s(prefix)?,
                s(&format!("{prefix}_SUFFIX"))?
            ))
        };
        let mut number = part("NUMBER_FIRST")?;
        let last = part("NUMBER_LAST")?;
        if !last.is_empty() {
            ensure!(
                row["NUMBER_LAST"].as_i64().is_some_and(|n| n > 0),
                "invalid G-NAF range end"
            );
            number.push_str(&format!("-{last}"));
        }
        let flat = part("FLAT_NUMBER")?;
        let level = part("LEVEL_NUMBER")?;
        let flat_type = s("FLAT_TYPE")?;
        let level_type = s("LEVEL_TYPE")?;
        let street = [s("STREET_NAME")?, s("STREET_TYPE")?, s("STREET_SUFFIX")?]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let locality = s("LOCALITY_NAME")?;
        let postcode = s("POSTCODE")?;
        if street.is_empty()
            || locality.is_empty()
            || postcode.len() != 4
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || (!flat.is_empty() && flat_type.is_empty())
            || (!level.is_empty() && level_type.is_empty())
        {
            continue;
        }
        let point = [
            feature["geometry"]["y"]
                .as_f64()
                .context("G-NAF latitude")?,
            feature["geometry"]["x"]
                .as_f64()
                .context("G-NAF longitude")?,
        ];
        ensure!(
            (-45. ..=-8.).contains(&point[0]) && (96. ..=169.).contains(&point[1]),
            "G-NAF non-Australian point"
        );
        let category = if !flat.is_empty() {
            "unit"
        } else if !level.is_empty() {
            "level"
        } else if !last.is_empty() {
            "range"
        } else if !s("NUMBER_FIRST_SUFFIX")?.is_empty() {
            "suffix"
        } else {
            "plain"
        };
        let id = sha256_hex(format!("{state}\0{locality}\0{street}\0{number}").as_bytes());
        let address = Address {
            id: id.clone(),
            registry_id,
            state,
            street,
            locality,
            postcode,
            number,
            flat,
            flat_type,
            level,
            level_type,
            category,
            point,
            attributes: row.clone(),
        };
        if addresses.get(&id).is_none_or(|old| {
            (
                address.flat.is_empty(),
                address.level.is_empty(),
                &address.registry_id,
            ) < (old.flat.is_empty(), old.level.is_empty(), &old.registry_id)
        }) {
            addresses.insert(id, address);
        }
    }
    eprintln!(
        "G-NAF: {} captured rows; {} eligible building groups",
        features.len(),
        addresses.len()
    );
    Ok(addresses)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<_, Vec<Address>>::new();
    for address in load(path)?.into_values() {
        pools
            .entry((address.state.clone(), address.category))
            .or_default()
            .push(address);
    }
    let mut lines = vec![];
    for ((state, category), mut pool) in pools {
        pool.sort_by_key(|a| sha256_hex(format!("{seed}\0{}", a.id).as_bytes()));
        let mut streets = BTreeSet::new();
        for a in pool
            .into_iter()
            .filter(|a| streets.insert((a.locality.clone(), a.street.clone())))
            .take(12)
        {
            for (form, query) in a.queries() {
                lines.push(json!({"kind":"address", "source":"gnaf-atlas-json", "ref":a.id, "cohort":format!("au_{category}"), "queries":[query], "label":a.query(&a.number), "tags":["registry-sample", "au", state, category, form]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible G-NAF addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "gnaf-atlas-json",
    command: "sample-au",
    country: "au",
    input: "gnaf",
    license: "Open-GNAF-EULA",
    location: "gnaf-building",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    let data: Value = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    metadata.insert("capture".into(), data["capture"].clone());
    metadata.insert("attribution".into(), json!("National Address Points, Geoscience Australia; G-NAF © Geoscape Australia, licensed by the Commonwealth of Australia under the Open G-NAF EULA"));
    let rows = load(path)?;
    refs.iter().map(|id| {
        let a = rows.get(id).with_context(|| format!("G-NAF lacks building {id}"))?;
        let mut provenance = metadata.clone();
        provenance.insert("attributes".into(), a.attributes.clone());
        provenance.insert("location_key".into(), json!("SHA256(state, locality, street, complete building number), NUL separated; conservative unit grouping"));
        Ok((id.clone(), Entry {
            point: Point::Wgs84(a.point),
            components: indexmap::IndexMap::from([("house_number", a.number.clone()), ("postcode", a.postcode.clone())]),
            radius_floor_km: 0.15,
            provenance,
            number_aliases: (!a.flat.is_empty()).then(|| BTreeSet::from([a.number.clone(), format!("{}/{}", a.flat, a.number)])),
            places: super::places([a.locality.as_str()]),
        }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_and_range_keep_the_building_and_do_not_depend_on_input_order() {
        let path = std::env::temp_dir().join(format!("gnaf-test-{}.json", std::process::id()));
        let mut attributes = json!({"ADDRESS_DETAIL_PID":"test-1", "STATE":"NSW", "GEOCODE_TYPE":"PROPERTY ACCESS POINT", "DATE_RETIRED":null, "ALIAS_PRINCIPAL":"PRINCIPAL", "LEVEL_GEOCODED":"LOCALITY,STREET, ADDRESS", "CONFIDENCE":2, "NUMBER_FIRST":12, "NUMBER_LAST":14, "FLAT_NUMBER":3, "LEVEL_NUMBER":null, "FLAT_TYPE":"UNIT", "LEVEL_TYPE":"", "STREET_NAME":"EXAMPLE", "STREET_TYPE":"ROAD", "STREET_SUFFIX":"", "LOCALITY_NAME":"EXAMPLETOWN", "POSTCODE":"2000"});
        for prefix in ["NUMBER_FIRST", "NUMBER_LAST", "FLAT_NUMBER", "LEVEL_NUMBER"] {
            for suffix in ["PREFIX", "SUFFIX"] {
                attributes[format!("{prefix}_{suffix}")] = json!("");
            }
        }
        let mut data = json!({"capture":{"geometry_crs":"EPSG:4326"}, "features":[{"attributes":attributes, "geometry":{"x":151.2,"y":-33.9}}]});
        let mut plain = data["features"][0].clone();
        plain["attributes"]["ADDRESS_DETAIL_PID"] = json!("test-2");
        plain["attributes"]["FLAT_NUMBER"] = Value::Null;
        data["features"].as_array_mut().unwrap().push(plain.clone());
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 3);
        assert!(first[2]["queries"][0].as_str().unwrap().contains("3/12-14"));
        assert!(first.iter().all(|s| s["ref"] == first[0]["ref"]));
        let refs = BTreeSet::from([first[0]["ref"].as_str().unwrap().into()]);
        let entries = prepare(&path, &refs).unwrap();
        let entry = entries.values().next().unwrap();
        assert_eq!(entry.components["house_number"], "12-14");
        assert_eq!(
            entry.number_aliases,
            Some(BTreeSet::from(["12-14".into(), "3/12-14".into()]))
        );
        data["features"].as_array_mut().unwrap().reverse();
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert_eq!(first, sample(&path, "test").unwrap());
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        plain["attributes"]["GEOCODE_TYPE"] = json!("LOCALITY");
        data["features"] = json!([plain]);
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
