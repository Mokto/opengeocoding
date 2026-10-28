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

const URL: &str = "https://data.gov.hk/en-data/dataset/hk-dpo-als_01-als";
const PER_POOL: usize = 4;

/// The district files of `ALS-GeoJSON.zip`, concatenated by `unzip -p`: one FeatureCollection
/// after another.
#[derive(Deserialize)]
struct Collection {
    features: Vec<Feature>,
}
#[derive(Deserialize)]
struct Feature {
    geometry: Geometry,
    properties: Properties,
}
#[derive(Deserialize)]
struct Geometry {
    coordinates: Option<[f64; 2]>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Properties {
    address: Premises,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Premises {
    premises_address: Record,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Record {
    geo_address: String,
    eng_premises_address: Eng,
    chi_premises_address: Chi,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Eng {
    region: String,
    eng_district: String,
    eng_street: Option<Part>,
    eng_village: Option<Part>,
    building_name: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Chi {
    region: String,
    chi_district: String,
    chi_street: Option<Part>,
    chi_village: Option<Part>,
    building_name: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Part {
    #[serde(alias = "VillageName")]
    street_name: Option<String>,
    location_name: Option<String>,
    building_no_from: Option<String>,
    building_no_to: Option<String>,
}

/// One street or village number of the register, in English and Chinese.
#[derive(Clone, PartialEq)]
struct Address {
    kind: &'static str,
    region: String,
    district: String,
    name: String,
    location: String,
    region_zh: String,
    district_zh: String,
    name_zh: String,
    location_zh: String,
    number: String,
}

impl Address {
    fn id(&self) -> String {
        sha256_hex(
            [
                self.district.as_str(),
                self.kind,
                &self.name,
                &self.location,
                &self.number,
            ]
            .join("\0")
            .as_bytes(),
        )
    }

    fn queries(&self) -> [(&'static str, String); 2] {
        let region = match self.region.as_str() {
            "KLN" => "KOWLOON, HONG KONG",
            "NT" => "NEW TERRITORIES, HONG KONG",
            _ => "HONG KONG",
        };
        let location = match self.location.as_str() {
            "" => String::new(),
            l => format!("{l}, "),
        };
        let region_zh = if self.region_zh == "香港" {
            ""
        } else {
            &self.region_zh
        };
        [
            (
                "en",
                format!(
                    "{} {}, {location}{}, {region}",
                    self.number, self.name, self.district
                ),
            ),
            (
                "zh",
                format!(
                    "香港{region_zh}{}{}{}{}號",
                    self.district_zh, self.location_zh, self.name_zh, self.number
                ),
            ),
        ]
    }

    fn category(&self) -> &'static str {
        if self.number.ends_with(|c: char| c.is_ascii_uppercase()) {
            "suffix"
        } else {
            "plain"
        }
    }
}

/// The address a record names, if it is one plain number (no range, no lot or block letters before
/// it) on one named street or village, the same in both languages.
fn address(r: &Record) -> Option<Address> {
    let (e, c) = (&r.eng_premises_address, &r.chi_premises_address);
    let (kind, en, zh) = match (&e.eng_street, &e.eng_village, &c.chi_street, &c.chi_village) {
        (Some(en), None, Some(zh), None) => ("street", en, zh),
        (None, Some(en), None, Some(zh)) => ("village", en, zh),
        _ => return None,
    };
    let number = en.building_no_from.as_deref()?;
    let digits = number.trim_end_matches(|c: char| c.is_ascii_uppercase());
    let name = en.street_name.as_deref()?.trim();
    let name_zh = zh.street_name.as_deref()?.trim();
    if zh.building_no_from.as_deref() != Some(number)
        || en.building_no_to.is_some()
        || zh.building_no_to.is_some()
        || !(1..=4).contains(&digits.len())
        || number.len() > digits.len() + 1
        || digits.starts_with('0')
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || name.is_empty()
        || name.contains(',')
        || name_zh.is_empty()
    {
        return None;
    }
    let text = |s: &Option<String>| s.as_deref().unwrap_or("").trim().to_owned();
    Some(Address {
        kind,
        region: e.region.clone(),
        district: e.eng_district.clone(),
        name: name.into(),
        location: text(&en.location_name),
        region_zh: c.region.clone(),
        district_zh: c.chi_district.clone(),
        name_zh: name_zh.into(),
        location_zh: text(&zh.location_name),
        number: number.into(),
    })
}

/// An address and the records that give it, ordered by GeoAddress.
struct Group {
    address: Address,
    records: Vec<(String, [f64; 2], Vec<String>)>,
    conflicting: bool,
}

impl Group {
    fn point(&self) -> [f64; 2] {
        self.records[0].1
    }

    /// The farthest of its records' points from the first, in metres.
    fn spread_m(&self) -> f64 {
        let [lat, lon] = self.point();
        self.records
            .iter()
            .map(|(_, p, _)| {
                let dy = (p[0] - lat) * 111_320.;
                let dx = (p[1] - lon) * 111_320. * lat.to_radians().cos();
                dx.hypot(dy)
            })
            .fold(0., f64::max)
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Group>> {
    let reader = BufReader::new(File::open(path)?);
    let mut groups = BTreeMap::<String, Group>::new();
    let mut count = 0usize;
    for collection in serde_json::Deserializer::from_reader(reader).into_iter::<Collection>() {
        for feature in collection.context("ALS GeoJSON")?.features {
            count += 1;
            let r = feature.properties.address.premises_address;
            let (Some([lon, lat]), Some(a)) = (feature.geometry.coordinates, address(&r)) else {
                continue;
            };
            ensure!(
                (22.1..=22.6).contains(&lat) && (113.8..=114.5).contains(&lon),
                "ALS record {} needs Hong Kong WGS84 coordinates",
                r.geo_address
            );
            let buildings = [
                &r.eng_premises_address.building_name,
                &r.chi_premises_address.building_name,
            ]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
            let record = (r.geo_address, [lat, lon], buildings);
            let group = groups.entry(a.id()).or_insert_with(|| Group {
                address: a.clone(),
                records: vec![],
                conflicting: false,
            });
            group.conflicting |= group.address != a;
            group.records.push(record);
        }
    }
    ensure!(count > 0, "no ALS records");
    for group in groups.values_mut() {
        group.records.sort_by(|a, b| {
            (a.0.cmp(&b.0))
                .then(a.1[0].total_cmp(&b.1[0]))
                .then(a.1[1].total_cmp(&b.1[1]))
                .then(a.2.cmp(&b.2))
        });
    }
    let keys = groups.len();
    // Several buildings may share one number (an estate's blocks); farther apart than this, the
    // number does not name one place.
    groups.retain(|_, g| !g.conflicting && g.spread_m() <= 50.);
    eprintln!(
        "ALS: {count} records, {keys} single-number addresses, {} with one place",
        groups.len()
    );
    Ok(groups)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("ALS file name")?;
    let mut pools = BTreeMap::<_, Vec<(String, Group)>>::new();
    for (id, g) in load(path)? {
        pools
            .entry((g.address.kind, g.address.district.clone()))
            .or_default()
            .push((sha256_hex(format!("{seed}\0{id}").as_bytes()), g));
    }
    let mut lines = vec![];
    for ((kind, district), mut pool) in pools {
        pool.sort_by(|a, b| a.0.cmp(&b.0));
        let mut names = BTreeSet::new();
        for (_, g) in pool
            .into_iter()
            .filter(|(_, g)| names.insert(g.address.name.clone()))
            .take(PER_POOL)
        {
            let a = &g.address;
            let [(_, label), _] = a.queries();
            for (form, query) in a.queries() {
                lines.push(json!({"kind":"address", "source":"als", "ref":a.id(), "cohort":format!("hk_{kind}"), "queries":[query], "label":label, "tags":["registry-sample", "hk", "als", release, format!("district:{district}"), kind, a.category(), form]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible ALS addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "als",
    command: "sample-hk",
    country: "hk",
    input: "als",
    license: "DATA.GOV.HK-Terms-1.2",
    location: "als",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert(
        "coordinates".into(),
        json!("GeoJSON point (WGS84) of the address's record with the smallest GeoAddress, as published"),
    );
    metadata.insert(
        "location_key".into(),
        json!("SHA256(English district, street or village, its name, location name, building number), NUL separated"),
    );
    let groups = load(path)?;
    refs.iter().map(|id| {
        let g = groups.get(id).with_context(|| format!("ALS lacks eligible address {id}"))?;
        let a = &g.address;
        let mut provenance = metadata.clone();
        provenance.insert("record".into(), json!({"kind":a.kind, "region":a.region, "district":a.district, "name":a.name, "location_name":a.location, "region_zh":a.region_zh, "district_zh":a.district_zh, "name_zh":a.name_zh, "location_name_zh":a.location_zh, "building_no":a.number, "geo_address":g.records.iter().map(|r| &r.0).collect::<Vec<_>>(), "buildings":g.records.iter().flat_map(|r| &r.2).collect::<Vec<_>>()}));
        Ok((id.clone(), Entry { point: Point::Wgs84(g.point()), components: indexmap::IndexMap::from([("house_number", a.number.clone())]), radius_floor_km: 0., provenance, number_aliases: None, places: vec![] }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_place_per_number_in_both_languages() {
        let path = std::env::temp_dir().join(format!("als-test-{}.geojson", std::process::id()));
        let feature = |geo: &str,
                       street: Option<(&str, &str)>,
                       village: Option<(&str, &str, &str)>,
                       number: &str,
                       lat: f64| {
            let mut eng = json!({"Region":"NT", "EngDistrict":"SAI KUNG DISTRICT"});
            let mut chi = json!({"Region":"新界", "ChiDistrict":"西貢區"});
            if let Some((en, zh)) = street {
                eng["EngStreet"] = json!({"StreetName":en, "BuildingNoFrom":number});
                chi["ChiStreet"] = json!({"StreetName":zh, "BuildingNoFrom":number});
            }
            if let Some((en, zh, location)) = village {
                eng["EngVillage"] =
                    json!({"VillageName":en, "LocationName":location, "BuildingNoFrom":number});
                chi["ChiVillage"] =
                    json!({"VillageName":zh, "LocationName":"西貢", "BuildingNoFrom":number});
            }
            json!({"type":"Feature", "geometry":{"type":"Point", "coordinates":[114.27, lat]}, "properties":{"Address":{"PremisesAddress":{"GeoAddress":geo, "BuildingCsuInformation":{"CsuId":geo}, "EngPremisesAddress":eng, "ChiPremisesAddress":chi}}}})
        };
        let road = Some(("HIRAM'S HIGHWAY", "西貢公路"));
        let collections = [
            json!({"type":"FeatureCollection", "features":[
                feature("2", road, None, "12A", 22.3801),
                feature("1", road, None, "12A", 22.3800),
                feature("3", road, None, "14", 22.38),
                feature("4", road, None, "14", 22.39),
                feature("5", road, None, "LOT 5", 22.38),
                feature("6", road, None, "012", 22.38),
            ]}),
            json!({"type":"FeatureCollection", "features":[
                feature("7", None, Some(("NGAU LIU", "牛寮", "SAI KUNG")), "185", 22.38),
                feature("8", None, None, "9", 22.38),
            ]}),
        ];
        let write = |collections: &[Value]| {
            std::fs::write(
                &path,
                collections
                    .iter()
                    .map(|c| c.to_string() + "\n")
                    .collect::<String>(),
            )
            .unwrap()
        };
        write(&collections);
        let lines = sample(&path, "test").unwrap();
        let queries: Vec<_> = lines
            .iter()
            .map(|l| l["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            [
                "12A HIRAM'S HIGHWAY, SAI KUNG DISTRICT, NEW TERRITORIES, HONG KONG",
                "香港新界西貢區西貢公路12A號",
                "185 NGAU LIU, SAI KUNG, SAI KUNG DISTRICT, NEW TERRITORIES, HONG KONG",
                "香港新界西貢區西貢牛寮185號",
            ]
        );
        assert_eq!(lines[0]["cohort"], "hk_street");
        assert!(
            lines[0]["tags"]
                .as_array()
                .unwrap()
                .contains(&json!("suffix"))
        );
        let refs = BTreeSet::from([lines[0]["ref"].as_str().unwrap().to_owned()]);
        let entry = prepare(&path, &refs).unwrap().into_values().next().unwrap();
        assert_eq!(entry.components["house_number"], "12A");
        assert!(matches!(entry.point, Point::Wgs84([lat, _]) if lat == 22.38));
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        write(&[collections[1].clone(), collections[0].clone()]);
        assert_eq!(sample(&path, "test").unwrap(), lines);
        let mut outside = collections[1].clone();
        outside["features"][0]["geometry"]["coordinates"] = json!([2.35, 48.85]);
        write(&[outside]);
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
