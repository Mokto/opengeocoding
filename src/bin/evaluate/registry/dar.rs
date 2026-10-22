use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
};

const URL: &str =
    "https://datafordeler.dk/dataoversigt/danmarks-adresseregister-dar/dar-fildownload";

#[derive(Deserialize, Serialize)]
struct Capture {
    register: String,
    version: u32,
    generation: u64,
    point_in_time: String,
    seed: String,
    candidate_limit: u64,
    geometry_crs: String,
    source_crs: String,
    duckdb: String,
    entity_counts: BTreeMap<String, BTreeMap<String, u64>>,
    selection_counts: BTreeMap<String, u64>,
}

#[derive(Deserialize, Serialize)]
struct Archive {
    entity: String,
    filename: String,
    bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
struct Snapshot {
    capture: Capture,
    sources: Vec<Archive>,
    addresses: Vec<Address>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Address {
    id: String,
    location: String,
    building_label: String,
    number: String,
    street: String,
    road_id: String,
    road_authority: String,
    postcode_id: String,
    postcode: String,
    postal_town: String,
    supplementary_id: Option<String>,
    supplementary: Option<String>,
    point_id: String,
    point_status: String,
    position_25832: String,
    technical_standard: String,
    accuracy_class: String,
    point_source: Option<String>,
    lat: f64,
    lon: f64,
    unit_id: Option<String>,
    unit_label: Option<String>,
    floor: Option<String>,
    door: Option<String>,
}

impl Address {
    fn floor(&self) -> &str {
        self.floor.as_deref().unwrap_or("").trim()
    }

    fn door(&self) -> &str {
        self.door.as_deref().unwrap_or("").trim()
    }

    fn category(&self) -> &'static str {
        if ["th", "tv", "mf"].contains(&self.door()) {
            "door_side"
        } else if self.door().bytes().any(|b| b.is_ascii_digit()) {
            "door_number"
        } else if !self.door().is_empty() {
            "door_other"
        } else if !self.floor().is_empty() {
            "floor"
        } else if self.number.bytes().any(|b| b.is_ascii_alphabetic()) {
            "letter"
        } else if self.supplementary.as_deref().is_some_and(|s| !s.is_empty()) {
            "supplementary"
        } else {
            "plain"
        }
    }

    fn queries(&self) -> Vec<(&'static str, String)> {
        let mut queries = vec![("building", format!("{}, Denmark", self.building_label))];
        if !self.floor().is_empty() || !self.door().is_empty() {
            queries.push((
                "unit",
                format!("{}, Denmark", self.unit_label.as_deref().unwrap()),
            ));
        }
        queries
    }
}

fn load(path: &Path) -> Result<Snapshot> {
    let snapshot: Snapshot = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    let capture = &snapshot.capture;
    ensure!(
        capture.register == "DAR" && capture.version == 4,
        "requires a DAR v4 capture"
    );
    ensure!(
        capture.source_crs == "EPSG:25832" && capture.geometry_crs == "EPSG:4326",
        "DAR capture CRS"
    );
    let mut ids = BTreeSet::new();
    let mut locations = BTreeSet::new();
    for a in &snapshot.addresses {
        ensure!(
            !a.id.is_empty() && ids.insert(&a.id),
            "duplicate or empty DAR entrance UUID"
        );
        ensure!(
            ["bbr:", "geodk:", "entrance:"]
                .iter()
                .any(|p| a.location.starts_with(p) && a.location.len() > p.len())
                && locations.insert(&a.location),
            "duplicate or invalid DAR location group"
        );
        let digits = a.number.trim_end_matches(|c: char| c.is_ascii_alphabetic());
        ensure!(
            digits.parse::<u32>().is_ok_and(|n| n > 0)
                && digits.bytes().all(|b| b.is_ascii_digit())
                && a.number.len() - digits.len() <= 1,
            "DAR house number"
        );
        ensure!(
            a.postcode.len() == 4 && a.postcode.bytes().all(|b| b.is_ascii_digit()),
            "DAR postcode"
        );
        ensure!(
            a.road_authority.len() == 4 && a.road_authority.bytes().all(|b| b.is_ascii_digit()),
            "DAR road authority"
        );
        ensure!(
            !a.street.is_empty()
                && !a.postal_town.is_empty()
                && !a.road_id.is_empty()
                && !a.point_id.is_empty(),
            "incomplete DAR address"
        );
        ensure!(
            a.point_status == "8"
                && ["TD", "TK", "TN"].contains(&a.technical_standard.as_str())
                && ["A", "B"].contains(&a.accuracy_class.as_str()),
            "DAR point is not an eligible in-use building access point"
        );
        ensure!(
            (54. ..=58.5).contains(&a.lat) && (7.5..=16.).contains(&a.lon),
            "DAR non-Danish point"
        );
        let prefix = format!("{} {},", a.street, a.number);
        let suffix = format!("{} {}", a.postcode, a.postal_town);
        ensure!(
            a.building_label.starts_with(&prefix) && a.building_label.ends_with(&suffix),
            "inconsistent DAR entrance label"
        );
        if !a.floor().is_empty() || !a.door().is_empty() {
            ensure!(
                a.unit_id.as_deref().is_some_and(|s| !s.is_empty())
                    && a.unit_label
                        .as_deref()
                        .is_some_and(|s| s.starts_with(&prefix)
                            && s.ends_with(&suffix)
                            && s != a.building_label),
                "inconsistent DAR unit label"
            );
        }
    }
    Ok(snapshot)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<_, Vec<Address>>::new();
    for address in load(path)?.addresses {
        pools
            .entry((address.road_authority.clone(), address.category()))
            .or_default()
            .push(address);
    }
    let mut lines = vec![];
    for ((authority, category), mut pool) in pools {
        pool.sort_by_key(|a| sha256_hex(format!("{seed}\0{}", a.id).as_bytes()));
        let mut roads = BTreeSet::new();
        for a in pool
            .into_iter()
            .filter(|a| roads.insert(a.road_id.clone()))
            .take(3)
        {
            for (form, query) in a.queries() {
                let mut tags = vec!["registry-sample", "dk", &authority, category, form];
                if a.number.bytes().any(|b| b.is_ascii_alphabetic()) {
                    tags.push("house_letter");
                }
                if a.supplementary.as_deref().is_some_and(|s| !s.is_empty()) {
                    tags.push("supplementary_place");
                }
                if !a.floor().is_empty() {
                    tags.push("published_floor");
                }
                lines.push(json!({"kind":"address", "source":"dar-json", "ref":a.id, "location":format!("dar-building:{}", a.location), "cohort":format!("dk_{category}"), "queries":[query], "label":a.building_label, "tags":tags}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible DAR addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "dar-json",
    command: "sample-dk",
    country: "dk",
    input: "dar",
    license: "CC-BY-4.0",
    location: "dar-entrance",
    url: |_| URL.into(),
    sample,
    prepare,
};

/// A postal district's town: "Aarhus C" and "København NV" are districts of Aarhus and København.
fn town_of_district(postal_town: &str) -> &str {
    postal_town
        .rsplit_once(' ')
        .filter(|(_, district)| {
            ["C", "K", "N", "NV", "NØ", "S", "SV", "SØ", "V", "Ø"].contains(district)
        })
        .map_or(postal_town, |(town, _)| town)
}

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let snapshot = load(path)?;
    let mut metadata = super::provenance(path)?;
    metadata.insert("capture".into(), serde_json::to_value(snapshot.capture)?);
    metadata.insert("archives".into(), serde_json::to_value(snapshot.sources)?);
    metadata.insert(
        "attribution".into(),
        json!("Danmarks Adresseregister, Klimadatastyrelsen, CC BY 4.0; sampled and reformatted"),
    );
    let rows: BTreeMap<_, _> = snapshot
        .addresses
        .into_iter()
        .map(|a| (a.id.clone(), a))
        .collect();
    refs.iter()
        .map(|id| {
            let a = rows
                .get(id)
                .with_context(|| format!("DAR lacks entrance {id}"))?;
            let mut provenance = metadata.clone();
            provenance.insert("address".into(), serde_json::to_value(a)?);
            Ok((
                id.clone(),
                Entry {
                    point: Point::Wgs84([a.lat, a.lon]),
                    components: indexmap::IndexMap::from([
                        ("house_number", a.number.clone()),
                        ("postcode", a.postcode.clone()),
                    ]),
                    radius_floor_km: 0.15,
                    provenance,
                    number_aliases: None,
                    places: super::places([
                        a.supplementary.as_deref().unwrap_or_default(),
                        &a.postal_town,
                        town_of_district(&a.postal_town),
                    ]),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_units_share_the_source_entrance_and_reject_wrong_point_status() {
        let path = std::env::temp_dir().join(format!("dar-test-{}.json", std::process::id()));
        let mut data = json!({"capture":{"register":"DAR", "version":4, "generation":1, "point_in_time":"2026-01-01T00:00:00Z", "seed":"test", "candidate_limit":10, "geometry_crs":"EPSG:4326", "source_crs":"EPSG:25832", "duckdb":"test", "entity_counts":{}, "selection_counts":{}}, "sources":[], "addresses":[{"id":"house-1", "location":"bbr:building-1", "building_label":"Eksempelvej 12A, Lilleby, 1000 Testby", "number":"12A", "street":"Eksempelvej", "road_id":"road-1", "road_authority":"0101", "postcode_id":"postal-1", "postcode":"1000", "postal_town":"Testby", "supplementary_id":"place-1", "supplementary":"Lilleby", "point_id":"point-1", "point_status":"8", "position_25832":"POINT (720000 6175000)", "technical_standard":"TD", "accuracy_class":"A", "point_source":"Test source", "lat":55.7, "lon":12.5, "unit_id":"unit-1", "unit_label":"Eksempelvej 12A, 2. tv., Lilleby, 1000 Testby", "floor":"2", "door":"tv"}]});
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!(first[0]["cohort"], "dk_door_side");
        assert_eq!(first[0]["location"], first[1]["location"]);
        assert!(first[1]["queries"][0].as_str().unwrap().contains("2. tv."));
        let refs = BTreeSet::from(["house-1".into()]);
        let entries = prepare(&path, &refs).unwrap();
        let entry = &entries["house-1"];
        assert_eq!(entry.components["house_number"], "12A");
        assert_eq!(entry.places, ["Lilleby", "Testby"]);
        assert_eq!(town_of_district("København NV"), "København");
        assert_eq!(town_of_district("Lyngby"), "Lyngby");
        assert!(entry.number_aliases.is_none());
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        let mut second = data["addresses"][0].clone();
        second["id"] = json!("house-2");
        second["location"] = json!("entrance:house-2");
        second["road_id"] = json!("road-2");
        data["addresses"].as_array_mut().unwrap().push(second);
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        let sampled = sample(&path, "test").unwrap();
        data["addresses"].as_array_mut().unwrap().reverse();
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert_eq!(sampled, sample(&path, "test").unwrap());
        data["addresses"][0]["point_status"] = json!("3");
        std::fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(sample(&path, "test").is_err());
        let mut a: Address = serde_json::from_value(data["addresses"][1].clone()).unwrap();
        a.door = Some("0001".into());
        assert_eq!(a.category(), "door_number");
        a.door = Some("a".into());
        assert_eq!(a.category(), "door_other");
        a.door = None;
        assert_eq!(a.category(), "floor");
        a.floor = None;
        assert_eq!(a.category(), "letter");
        a.number = "12".into();
        assert_eq!(a.category(), "supplementary");
        a.supplementary = None;
        assert_eq!(a.category(), "plain");
        std::fs::remove_file(path).unwrap();
    }
}
