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

const URL: &str = "https://www150.statcan.gc.ca/n1/pub/46-26-0002/462600022022001-eng.htm";
const PROVINCES: &[(&str, &str)] = &[
    ("10", "NL"),
    ("11", "PE"),
    ("12", "NS"),
    ("13", "NB"),
    ("24", "QC"),
    ("35", "ON"),
    ("46", "MB"),
    ("47", "SK"),
    ("48", "AB"),
    ("59", "BC"),
    ("60", "YT"),
    ("61", "NT"),
    ("62", "NU"),
];

#[derive(Clone)]
struct Address {
    id: String,
    province: String,
    number: String,
    unit: String,
    postcode: String,
    street: String,
    city: String,
    geographic_street: String,
    geographic_city: String,
    category: &'static str,
    point: [f64; 2],
    attributes: Value,
}

fn text<'a>(row: &'a Value, key: &str) -> Result<&'a str> {
    let value = row[key]
        .as_str()
        .with_context(|| format!("NAR missing {key}"))?;
    ensure!(
        !value.chars().any(char::is_control),
        "NAR control character in {key}"
    );
    Ok(value.trim())
}

fn street(row: &Value, prefix: &str, province: &str) -> Result<String> {
    static ORDINAL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\d+(?:re|er|e|[eè]me)$").unwrap());
    let name = text(row, &format!("{prefix}_STREET_NAME"))?;
    let kind = text(row, &format!("{prefix}_STREET_TYPE"))?;
    let direction = text(row, &format!("{prefix}_STREET_DIR"))?;
    if name.is_empty() {
        return Ok(String::new());
    }
    let french = [
        "RUE",
        "CH",
        "CHEMIN",
        "BOUL",
        "BOULEVARD",
        "RANG",
        "RTE",
        "ROUTE",
        "MTEE",
        "MONTÉE",
        "CÔTE",
        "COTE",
        "AV",
        "CROIS",
        "CROISSANT",
        "ALLÉE",
        "ALLEE",
        "IMP",
        "IMPASSE",
        "RLE",
        "RUELLE",
    ]
    .contains(&kind.to_uppercase().as_str())
        || (province == "QC"
            && [
                "AVENUE",
                "PLACE",
                "PL",
                "TERRASSE",
                "TSSE",
                "PROM",
                "PROMENADE",
            ]
            .contains(&kind.to_uppercase().as_str()));
    let parts = if french && !ORDINAL.is_match(name) {
        [kind, name, direction]
    } else {
        [name, kind, direction]
    };
    Ok(parts
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" "))
}

impl Address {
    fn query(&self, number: &str, street: &str, city: &str) -> String {
        format!(
            "{number} {street}, {city}, {} {}, Canada",
            self.province, self.postcode
        )
    }

    fn queries(&self) -> Vec<(&'static str, String)> {
        let base = self.query(&self.number, &self.street, &self.city);
        let mut queries = vec![("building", base.clone())];
        if !self.unit.is_empty() {
            queries.push((
                "unit",
                self.query(
                    &format!("{}-{}", self.unit, self.number),
                    &self.street,
                    &self.city,
                ),
            ));
        }
        if !self.geographic_street.is_empty() && !self.geographic_city.is_empty() {
            let query = self.query(&self.number, &self.geographic_street, &self.geographic_city);
            if query.to_lowercase() != base.to_lowercase() {
                queries.push(("geographic", query));
            }
        }
        queries
    }
}

fn load(path: &Path) -> Result<(Value, BTreeMap<String, Address>)> {
    static GUID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$").unwrap());
    static CIVIC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[1-9][0-9]*$").unwrap());
    static SUFFIX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?:[A-Za-z]|[0-9]+/[0-9]+)?$").unwrap());
    static POSTCODE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[ABCEGHJ-NPRSTVXY][0-9][ABCEGHJ-NPRSTVWXYZ][0-9][ABCEGHJ-NPRSTVWXYZ][0-9]$")
            .unwrap()
    });
    static UNIT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?:[A-Za-z0-9][A-Za-z0-9 /-]*)?$").unwrap());
    let data: Value = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    ensure!(
        data["capture"]["geometry_crs"] == "EPSG:4326" && data["capture"]["snapshot"] == "2026-06",
        "NAR snapshot/CRS"
    );
    let mut rows = BTreeMap::new();
    let mut address_ids = BTreeSet::new();
    for attributes in data["addresses"].as_array().context("NAR addresses")? {
        let location = &attributes["location"];
        let row = &attributes["address"];
        let id = text(location, "LOC_GUID")?;
        let address_id = text(row, "ADDR_GUID")?;
        ensure!(
            GUID.is_match(id)
                && GUID.is_match(address_id)
                && text(row, "LOC_GUID")? == id
                && address_ids.insert(address_id.to_owned()),
            "NAR duplicate or invalid UUID"
        );
        let code = text(row, "PROV_CODE")?;
        let province = text(row, "MAIL_PROV_ABVN")?;
        let csd = text(location, "CSD_CODE")?;
        ensure!(
            PROVINCES.contains(&(code, province))
                && csd.len() == 7
                && csd.starts_with(code)
                && csd.bytes().all(|b| b.is_ascii_digit()),
            "NAR province/CSD mismatch"
        );
        let first = text(row, "CIVIC_NO")?;
        let suffix = text(row, "CIVIC_NO_SUFFIX")?;
        let unit = text(row, "APT_NO_LABEL")?;
        let raw_postcode = text(row, "MAIL_POSTAL_CODE")?.replace(' ', "");
        ensure!(
            CIVIC.is_match(first)
                && SUFFIX.is_match(suffix)
                && UNIT.is_match(unit)
                && POSTCODE.is_match(&raw_postcode),
            "NAR invalid civic number, unit or postcode"
        );
        let number = format!(
            "{first}{}{suffix}",
            if suffix.contains('/') { " " } else { "" }
        );
        let point = [
            text(location, "BG_LATITUDE")?.parse::<f64>()?,
            text(location, "BG_LONGITUDE")?.parse::<f64>()?,
        ];
        ensure!(
            (41. ..=84.).contains(&point[0]) && (-142. ..=-50.).contains(&point[1]),
            "NAR invalid BG point"
        );
        let mailing_street = street(row, "MAIL", province)?;
        let city = text(row, "MAIL_MUN_NAME")?;
        ensure!(
            !mailing_street.is_empty() && !city.is_empty(),
            "NAR empty mailing street/place"
        );
        let category = if !unit.is_empty() {
            "unit"
        } else if !suffix.is_empty() {
            "suffix"
        } else if !text(row, "MAIL_STREET_DIR")?.is_empty() {
            "direction"
        } else if province == "QC" {
            "quebec"
        } else {
            "plain"
        };
        let geographic_city = text(
            row,
            if province == "QC" {
                "CSD_FRE_NAME"
            } else {
                "CSD_ENG_NAME"
            },
        )?;
        let address = Address {
            id: id.into(),
            province: province.into(),
            number,
            unit: unit.into(),
            postcode: format!("{} {}", &raw_postcode[..3], &raw_postcode[3..]),
            street: mailing_street,
            city: city.into(),
            geographic_street: street(row, "OFFICIAL", province)?,
            geographic_city: geographic_city.into(),
            category,
            point,
            attributes: attributes.clone(),
        };
        ensure!(
            rows.insert(id.to_owned(), address).is_none(),
            "NAR duplicate location"
        );
    }
    ensure!(!rows.is_empty(), "no eligible NAR addresses");
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
            .filter(|a| streets.insert((a.city.clone(), a.street.clone())))
            .take(8)
        {
            for (form, query) in a.queries() {
                lines.push(json!({"kind":"address", "source":"nar-json", "ref":a.id, "cohort":format!("ca_{category}"), "queries":[query], "label":a.query(&a.number, &a.street, &a.city), "tags":["registry-sample", "ca", province, category, form]}));
            }
        }
    }
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "nar-json",
    command: "sample-ca",
    country: "ca",
    input: "nar",
    license: "Statistics-Canada-Open-Licence AND OGL-Yukon-2.0",
    location: "nar-location",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let (capture, rows) = load(path)?;
    let mut metadata = super::provenance(path)?;
    metadata.insert("capture".into(), capture);
    metadata.insert("attribution".into(), json!("Adapted from Statistics Canada, National Address Register, June 2026. This does not constitute an endorsement by Statistics Canada of this product. Contains information licensed under the Open Government Licence – Yukon."));
    refs.iter()
        .map(|id| {
            let a = rows
                .get(id)
                .with_context(|| format!("NAR lacks location {id}"))?;
            let mut provenance = metadata.clone();
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
                    number_aliases: (!a.unit.is_empty()).then(|| {
                        BTreeSet::from([a.number.clone(), format!("{}-{}", a.unit, a.number)])
                    }),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Value {
        json!({"capture":{"snapshot":"2026-06","geometry_crs":"EPSG:4326"},"addresses":[{
            "location":{"LOC_GUID":"00000001-0000-4000-8000-000000000000","CSD_CODE":"3500001","BG_LATITUDE":"45.42","BG_LONGITUDE":"-75.7"},
            "address":{"LOC_GUID":"00000001-0000-4000-8000-000000000000","ADDR_GUID":"00000002-0000-4000-8000-000000000000","PROV_CODE":"35","MAIL_PROV_ABVN":"ON","CIVIC_NO":"12","CIVIC_NO_SUFFIX":"A","APT_NO_LABEL":"7","MAIL_POSTAL_CODE":"K1A0B1","MAIL_STREET_NAME":"Example","MAIL_STREET_TYPE":"AVE","MAIL_STREET_DIR":"","MAIL_MUN_NAME":"Exampleville","OFFICIAL_STREET_NAME":"Example","OFFICIAL_STREET_TYPE":"AVENUE","OFFICIAL_STREET_DIR":"","CSD_ENG_NAME":"Other Exampleville","CSD_FRE_NAME":"Autre Exempleville"}
        }]})
    }

    #[test]
    fn source_units_and_locality_forms_share_building_expectations() {
        let path = std::env::temp_dir().join(format!("nar-test-{}.json", std::process::id()));
        let mut data = fixture();
        let mut second = data["addresses"][0].clone();
        second["location"]["LOC_GUID"] = json!("00000003-0000-4000-8000-000000000000");
        second["address"]["LOC_GUID"] = second["location"]["LOC_GUID"].clone();
        second["address"]["ADDR_GUID"] = json!("00000004-0000-4000-8000-000000000000");
        data["addresses"].as_array_mut().unwrap().push(second);
        let write =
            |value: &Value| std::fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
        write(&data);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 3);
        assert!(
            first[1]["queries"][0]
                .as_str()
                .unwrap()
                .starts_with("7-12A Example AVE")
        );
        let refs = BTreeSet::from([first[0]["ref"].as_str().unwrap().to_owned()]);
        let entries = prepare(&path, &refs).unwrap();
        let entry = entries.values().next().unwrap();
        assert_eq!(entry.components["house_number"], "12A");
        assert_eq!(entry.components["postcode"], "K1A 0B1");
        assert_eq!(
            entry.number_aliases,
            Some(BTreeSet::from(["12A".into(), "7-12A".into()]))
        );
        data["addresses"].as_array_mut().unwrap().reverse();
        write(&data);
        assert_eq!(sample(&path, "test").unwrap(), first);
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        data["addresses"][0]["location"]["BG_LATITUDE"] = json!("");
        write(&data);
        assert!(sample(&path, "test").is_err());
        data = fixture();
        data["addresses"][0]["address"]["MAIL_PROV_ABVN"] = json!("QC");
        write(&data);
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn french_designators_and_ordinals_keep_their_order() {
        let mut row = fixture()["addresses"][0]["address"].clone();
        row["MAIL_STREET_NAME"] = json!("du Lac");
        row["MAIL_STREET_TYPE"] = json!("CH");
        assert_eq!(street(&row, "MAIL", "QC").unwrap(), "CH du Lac");
        row["MAIL_STREET_NAME"] = json!("1re");
        row["MAIL_STREET_TYPE"] = json!("AVENUE");
        assert_eq!(street(&row, "MAIL", "QC").unwrap(), "1re AVENUE");
    }
}
