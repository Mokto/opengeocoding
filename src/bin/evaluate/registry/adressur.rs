use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

pub const URL: &str = "https://gis.us.fo/arcgis/rest/services/adressur/us_adr_husanr/MapServer/0";

/// Locations drawn per municipality: plain numbers and numbers with a letter.
const PER_CLASS: [(&str, usize); 2] = [("street", 4), ("letter", 1)];

#[derive(Clone, Debug)]
struct Address {
    /// address_id, the register's number of the address.
    id: String,
    street: String,
    /// house_no: the number and its letter ("47B").
    number: String,
    postcode: String,
    /// The village or town of the postcode ("Oyndarfjørður").
    city: String,
    /// The municipality in the genitive, as in "Runavíkar kommuna".
    municipality: String,
    point: [f64; 2],
    record: Value,
}

impl Address {
    fn class(&self) -> &'static str {
        if self.number.ends_with(|c: char| c.is_alphabetic()) {
            "letter"
        } else {
            "street"
        }
    }

    /// The village, also without the island or town that tells namesakes apart ("Nes, Vágur").
    fn places(&self) -> Vec<String> {
        super::places([self.city.as_str(), self.city.split(", ").next().unwrap()])
    }

    fn query(&self) -> String {
        format!(
            "{} {}, {} {}, Faroe Islands",
            self.street, self.number, self.postcode, self.city
        )
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let data: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    ensure!(
        data["spatialReference"]["wkid"] == 4326 && data["exceededTransferLimit"].is_null(),
        "{} is not the whole adressur layer queried with outSR=4326",
        path.display()
    );
    let features = data["features"]
        .as_array()
        .context("adressur JSON features")?;
    let (mut candidates, mut per_text) = (vec![], HashMap::new());
    for (n, feature) in features.iter().enumerate() {
        let a = &feature["attributes"];
        let text = |name: &str| a[name].as_str().unwrap_or_default().trim().to_string();
        let int = |name: &str| a[name].as_i64().map(|v| v.to_string()).unwrap_or_default();
        let (lat, lon) = (
            a["latitude"].as_f64().context("adressur latitude")?,
            a["longitude"].as_f64().context("adressur longitude")?,
        );
        ensure!(
            (61.3..62.5).contains(&lat) && (-7.8..-6.1).contains(&lon),
            "adressur feature {n}: needs WGS84 latitude, longitude"
        );
        let (street, number, digit, letter) = (
            text("street_name"),
            text("house_no"),
            int("house_digit"),
            text("house_letter"),
        );
        let (postcode, city) = (int("zip"), text("city"));
        *per_text
            .entry((street.clone(), number.clone(), postcode.clone()))
            .or_insert(0) += 1;
        let geometry = [
            feature["geometry"]["y"].as_f64().unwrap_or(f64::NAN),
            feature["geometry"]["x"].as_f64().unwrap_or(f64::NAN),
        ];
        let (dy, dx) = (
            (geometry[0] - lat) * 111_195.,
            (geometry[1] - lon) * 111_195. * lat.to_radians().cos(),
        );
        if int("address_id").is_empty()
            || street.is_empty()
            || street.contains(',')
            || digit.starts_with('0')
            || digit.len() > 4
            || number != format!("{digit}{letter}")
            || letter.chars().count() > 1
            || !letter.chars().all(char::is_uppercase)
            || postcode.len() != 3
            || city.is_empty()
            // the point's latitude/longitude fields disagree with its geometry at 8 addresses,
            // by up to 31 m, and the register does not say which is right
            || dx.hypot(dy) > 1.
        {
            continue;
        }
        candidates.push(Address {
            id: int("address_id"),
            street,
            number,
            postcode,
            city,
            municipality: text("municipality"),
            point: [lat, lon],
            record: a.clone(),
        });
    }
    let mut eligible = 0usize;
    for a in candidates {
        // no other address written the same way in its postcode
        if per_text[&(a.street.clone(), a.number.clone(), a.postcode.clone())] == 1 {
            eligible += 1;
            visit(a)?;
        }
    }
    eprintln!("adressur: {} features, {eligible} eligible", features.len());
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate address_id {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "adressur JSON lacks requested address_id (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("adressur file name")?;
    let mut pools: BTreeMap<(String, &str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        pools
            .entry((a.municipality.clone(), a.class()))
            .or_default()
            .insert(hash, a);
        Ok(())
    })?;
    let mut lines = vec![];
    for ((_, class), pool) in pools {
        let take = PER_CLASS.iter().find(|(c, _)| *c == class).unwrap().1;
        let mut streets = BTreeSet::new();
        for a in pool
            .values()
            .filter(|a| streets.insert(a.street.clone()))
            .take(take)
        {
            let query = a.query();
            lines.push(json!({"kind":"address", "source":"adressur", "ref":a.id, "cohort":format!("fo_{class}"), "queries":[query], "label":query, "tags":["registry-sample", "fo", "adressur", release, format!("municipality:{}", a.municipality), class]}));
        }
    }
    eprintln!("adressur: {} locations", lines.len());
    ensure!(!lines.is_empty(), "no eligible adressur addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "adressur",
    command: "sample-fo",
    country: "fo",
    input: "adressur",
    license: "Umhvorvisstovan-Terms",
    location: "adressur",
    url: |id| format!("{URL}#{id}"),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("latitude, longitude: WGS84 fields of the address point, kept only where they agree with the point's geometry (queried with outSR=4326) within 1 m"),
    );
    metadata.insert(
        "attribution".into(),
        json!("The Faroese Environment Agency, Umhvørvisstovan (adressur, us_adr_husanr); terms: https://www.foroyakort.fo/um-foeroyakort/terms-and-conditions-in-english; data date in source.release"),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(id, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), a.record.clone());
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(a.point),
                    components: indexmap::IndexMap::from([
                        ("house_number", a.number.clone()),
                        ("road", a.street.clone()),
                        ("postcode", a.postcode.clone()),
                    ]),
                    radius_floor_km: 0.,
                    provenance,
                    number_aliases: None,
                    places: a.places(),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligible_rows_query_forms_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("adressur-test-{}.json", std::process::id()));
        let feature = |id: i64, street: &str, digit: i64, letter: &str, zip: i64, y: f64| json!({"attributes": {"address_id": id, "street_name": street, "house_no": format!("{digit}{letter}"), "house_digit": digit, "house_letter": letter, "zip": zip, "city": "Tórshavn", "municipality": "Tórshavnar", "latitude": 62.0, "longitude": -6.8}, "geometry": {"x": -6.8, "y": y}});
        let features = vec![
            feature(1, "Eystara Bryggja", 20, "", 100, 62.0),
            feature(2, "Brekkulág", 47, "B", 100, 62.0),
            feature(3, "Gøta", 5, "", 100, 62.001),
            feature(4, "Gøta", 6, "", 100, 62.0),
            feature(5, "Gøta", 6, "", 100, 62.0),
            feature(6, "Gøta", 7, "b", 100, 62.0),
        ];
        let write = |features: &[Value]| {
            std::fs::write(
                &path,
                json!({"spatialReference": {"wkid": 4326}, "features": features}).to_string(),
            )
            .unwrap()
        };
        write(&features);
        let first = sample(&path, "test").unwrap();
        let queries: BTreeSet<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            BTreeSet::from([
                "Eystara Bryggja 20, 100 Tórshavn, Faroe Islands",
                "Brekkulág 47B, 100 Tórshavn, Faroe Islands",
            ])
        );
        let mut reversed = features.clone();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let mut found = lookup(&path, &BTreeSet::from(["2".into()])).unwrap();
        assert_eq!(found["2"].class(), "letter");
        assert_eq!(found["2"].places(), ["Tórshavn"]);
        found.get_mut("2").unwrap().city = "Nes, Vágur".into();
        assert_eq!(found["2"].places(), ["Nes, Vágur", "Nes"]);
        // a point 111 m from its geometry, two addresses written alike, a lowercase letter
        for id in ["3", "4", "6"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        let mut swapped = features[0].clone();
        swapped["attributes"]["latitude"] = json!(-6.8);
        swapped["attributes"]["longitude"] = json!(62.0);
        write(&[swapped]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("WGS84")
        );
        std::fs::write(
            &path,
            json!({"spatialReference": {"wkid": 5316}, "features": []}).to_string(),
        )
        .unwrap();
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
