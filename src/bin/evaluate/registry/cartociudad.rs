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

const URL: &str = "https://api-features.idee.es/collections/address";

#[derive(Clone)]
struct Address {
    id: String,
    registry_id: String,
    street: String,
    number: String,
    municipality: String,
    area: String,
    province: String,
    category: &'static str,
    point: [f64; 2],
    properties: Value,
}

impl Address {
    fn query(&self, street: &str, town: &str) -> String {
        let mut parts = vec![format!("{street} {}", self.number)];
        if !self.area.is_empty() && !self.municipality.split('/').any(|s| s == self.area) {
            parts.push(self.area.clone());
        }
        parts.extend([town.into(), self.province.clone(), "Spain".into()]);
        parts.join(", ")
    }

    fn queries(&self) -> Vec<(&'static str, String)> {
        let mut queries = vec![("native", self.query(&self.street, &self.municipality))];
        if self.municipality.contains('/') {
            for town in self
                .municipality
                .split('/')
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                queries.push(("municipal_alias", self.query(&self.street, town)));
            }
        }
        if let Some((kind, rest)) = self.street.split_once(' ') {
            let short = match kind {
                "CALLE" => Some("C/"),
                "AVENIDA" => Some("Av."),
                "CARRETERA" => Some("Ctra."),
                _ => None,
            };
            if let Some(short) = short {
                queries.push((
                    "abbreviation",
                    self.query(&format!("{short} {rest}"), &self.municipality),
                ));
            }
        }
        let mut seen = BTreeSet::new();
        queries.retain(|(_, query)| seen.insert(query.clone()));
        queries
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Address>> {
    let data: Value = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    ensure!(
        data["capture"]["geometry_crs"] == "OGC:CRS84",
        "CartoCiudad capture CRS"
    );
    let features = data["features"]
        .as_array()
        .context("CartoCiudad features")?;
    let mut ids = BTreeSet::new();
    let mut groups = BTreeMap::<String, Vec<Address>>::new();
    for feature in features {
        let registry_id = feature["id"]
            .as_str()
            .context("CartoCiudad feature ID")?
            .to_string();
        ensure!(
            !registry_id.is_empty() && ids.insert(registry_id.clone()),
            "duplicate or empty CartoCiudad ID"
        );
        let row = &feature["properties"];
        let s = |name: &str| -> Result<&str> {
            match row
                .get(name)
                .with_context(|| format!("CartoCiudad missing {name}"))?
            {
                Value::String(s) => Ok(s.trim()),
                Value::Null => Ok(""),
                _ => anyhow::bail!("CartoCiudad invalid {name}"),
            }
        };
        ensure!(
            row.get("component_PostalDescriptor").is_none(),
            "CartoCiudad postal-code redistribution is not established"
        );
        let street = s("component_ThoroughfareName")?;
        let municipality = s("component_AdminUnitName_4")?;
        let province = s("component_AdminUnitName_3")?;
        let area = s("component_AddressAreaName")?;
        let number = s("locator_designator_addressNumber")?;
        let extension = s("locator_designator_addressNumberExtension")?;
        let provider = s("dataProvider")?;
        if [street, municipality, province, provider]
            .iter()
            .any(|s| s.is_empty())
            || !number.parse::<u32>().is_ok_and(|n| n > 0)
            || !number.bytes().all(|b| b.is_ascii_digit())
            || !extension.bytes().all(|b| b.is_ascii_alphabetic())
            || provider == "IGR-Redes de Transporte"
        {
            continue;
        }
        ensure!(
            feature["geometry"]["type"] == "Point",
            "CartoCiudad non-point address"
        );
        let coords = &feature["geometry"]["coordinates"];
        let point = [
            coords[1].as_f64().context("CartoCiudad latitude")?,
            coords[0].as_f64().context("CartoCiudad longitude")?,
        ];
        ensure!(
            (27. ..=44.).contains(&point[0]) && (-19. ..=5.).contains(&point[1]),
            "CartoCiudad non-Spanish point"
        );
        let number = format!("{number}{extension}");
        let category = if !extension.is_empty() {
            "suffix"
        } else if !area.is_empty() && area != municipality {
            "area"
        } else if [
            "CAMINO ",
            "LUGAR ",
            "ALDEA ",
            "BARRIO ",
            "DISEMINADO ",
            "CARRETERA ",
        ]
        .iter()
        .any(|prefix| street.starts_with(prefix))
        {
            "other_road"
        } else {
            "plain"
        };
        let id = sha256_hex(
            format!("{province}\0{municipality}\0{area}\0{street}\0{number}").as_bytes(),
        );
        groups.entry(id.clone()).or_default().push(Address {
            id,
            registry_id,
            street: street.into(),
            number,
            municipality: municipality.into(),
            area: area.into(),
            province: province.into(),
            category,
            point,
            properties: row.clone(),
        });
    }
    let mut addresses = BTreeMap::new();
    let mut conflicts = 0;
    for (id, mut group) in groups {
        group.sort_by(|a, b| a.registry_id.cmp(&b.registry_id));
        let first = group[0].point;
        if group
            .iter()
            .any(|a| crate::score::haversine(first, a.point) > 0.15)
        {
            conflicts += 1;
            continue;
        }
        addresses.insert(id, group.remove(0));
    }
    eprintln!(
        "CartoCiudad: {} captured rows; {} eligible address groups; {conflicts} conflicting-point groups excluded",
        features.len(),
        addresses.len()
    );
    Ok(addresses)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<_, Vec<Address>>::new();
    for address in load(path)?.into_values() {
        pools
            .entry((address.province.clone(), address.category))
            .or_default()
            .push(address);
    }
    let mut lines = vec![];
    for ((province, category), mut pool) in pools {
        pool.sort_by_key(|a| sha256_hex(format!("{seed}\0{}", a.id).as_bytes()));
        let mut streets = BTreeSet::new();
        for a in pool
            .into_iter()
            .filter(|a| streets.insert((a.municipality.clone(), a.street.clone())))
            .take(5)
        {
            for (form, query) in a.queries() {
                lines.push(json!({"kind":"address", "source":"cartociudad-features-json", "ref":a.id, "cohort":format!("es_{category}"), "queries":[query], "label":a.query(&a.street, &a.municipality), "tags":["registry-sample", "es", province, category, form]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible CartoCiudad addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "cartociudad-features-json",
    command: "sample-es",
    country: "es",
    input: "cartociudad",
    license: "CC-BY-4.0",
    location: "cartociudad-building",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    let data: Value = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    metadata.insert("capture".into(), data["capture"].clone());
    metadata.insert("attribution".into(), json!("Obra derivada de CartoCiudad, CC-BY 4.0 scne.es (IGN/CNIG); consultation date in source.retrieved"));
    let rows = load(path)?;
    refs.iter().map(|id| {
        let a = rows.get(id).with_context(|| format!("CartoCiudad lacks address {id}"))?;
        let mut provenance = metadata.clone();
        provenance.insert("feature_id".into(), json!(a.registry_id));
        provenance.insert("properties".into(), a.properties.clone());
        provenance.insert("location_key".into(), json!("SHA256(province, municipality, address area, street, complete house number), NUL separated"));
        Ok((id.clone(), Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.number.clone())]), radius_floor_km: 0.15, provenance, number_aliases: None }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_aliases_and_suffixes_share_a_location_and_conflicts_are_excluded() {
        let path =
            std::env::temp_dir().join(format!("cartociudad-test-{}.json", std::process::id()));
        let mut data = json!({"capture":{"geometry_crs":"OGC:CRS84"}, "features":[{"id":"test-1", "geometry":{"type":"Point", "coordinates":[-2.5,42.8]}, "properties":{"component_ThoroughfareName":"CALLE EJEMPLO", "component_AdminUnitName_4":"Example/Adibide", "component_AdminUnitName_3":"Araba/Álava", "component_AddressAreaName":"Example/Adibide", "locator_designator_addressNumber":"12", "locator_designator_addressNumberExtension":"BIS", "dataProvider":"Test registry"}}]});
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 4);
        assert!(
            first[3]["queries"][0]
                .as_str()
                .unwrap()
                .starts_with("C/ EJEMPLO 12BIS")
        );
        assert!(first.iter().all(|s| s["ref"] == first[0]["ref"]));
        let refs = BTreeSet::from([first[0]["ref"].as_str().unwrap().into()]);
        assert_eq!(
            prepare(&path, &refs)
                .unwrap()
                .values()
                .next()
                .unwrap()
                .components["house_number"],
            "12BIS"
        );
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        let mut duplicate = data["features"][0].clone();
        duplicate["id"] = json!("test-2");
        data["features"].as_array_mut().unwrap().push(duplicate);
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert_eq!(first, sample(&path, "test").unwrap());
        data["features"].as_array_mut().unwrap().reverse();
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert_eq!(first, sample(&path, "test").unwrap());
        data["features"][0]["geometry"]["coordinates"] = json!([-3.5, 42.8]);
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(sample(&path, "test").is_err());
        data["features"] = json!([data["features"][0].clone()]);
        data["features"][0]["properties"]["locator_designator_addressNumber"] = json!("-997");
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
