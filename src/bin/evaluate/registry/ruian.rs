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

pub const URL: &str = "https://atom.cuzk.gov.cz/RUIAN-CSV-ADR-ST/RUIAN-CSV-ADR-ST.xml";

#[derive(Clone, Debug)]
pub struct Address {
    pub id: String,
    pub street: String,
    pub postcode: String,
    pub descriptive: String,
    pub orientation: String,
    pub number_type: String,
    pub point: [f64; 2],
    city: String,
    part: String,
    municipality: String,
    city_district: String,
    region: String,
    pub letter: String,
}

impl Address {
    pub fn number(&self) -> String {
        if self.orientation.is_empty() {
            self.descriptive.clone()
        } else {
            format!("{}/{}{}", self.descriptive, self.orientation, self.letter)
        }
    }

    fn query(&self, number: &str) -> String {
        format!(
            "{} {number}, {} {}, Czechia",
            if self.street.is_empty() {
                &self.part
            } else {
                &self.street
            },
            self.postcode,
            self.city
        )
    }

    fn category(&self) -> &'static str {
        if self.number_type == "č.ev." {
            "evidential"
        } else if !self.orientation.is_empty() {
            if self.letter.is_empty() {
                "dual"
            } else {
                "dual_letter"
            }
        } else if self.street.is_empty() {
            "village"
        } else {
            "descriptive"
        }
    }

    fn number_aliases(&self) -> Option<BTreeSet<String>> {
        (!self.orientation.is_empty()).then(|| {
            BTreeSet::from([
                self.number(),
                self.descriptive.clone(),
                format!("{}{}", self.orientation, self.letter),
            ])
        })
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty RUIAN CSV"
    );
    let columns: BTreeMap<_, _> = crate::registry::nad::split(line.trim_end(), ',')
        .iter()
        .enumerate()
        .map(|(i, s)| (s.to_string(), i))
        .collect();
    for name in [
        "id",
        "municipality",
        "city",
        "city_district",
        "part",
        "street",
        "number_type",
        "descriptive",
        "orientation",
        "letter",
        "postcode",
        "region",
        "lat",
        "lon",
    ] {
        ensure!(columns.contains_key(name), "RUIAN CSV missing {name}");
    }
    let mut n = 0usize;
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(1_000_000) {
            eprintln!("RUIAN: {n} rows read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == columns.len(),
            "RUIAN CSV row {n}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        let lat: f64 = field("lat").parse().context("RUIAN latitude")?;
        let lon: f64 = field("lon").parse().context("RUIAN longitude")?;
        ensure!(
            (48. ..=52.).contains(&lat) && (12. ..=19.).contains(&lon),
            "RUIAN CSV row {n}: needs Czech EPSG:4326 coordinates"
        );
        let positive = |name: &str| field(name).parse::<u32>().is_ok_and(|n| n > 0);
        if !["id", "municipality", "descriptive", "region"]
            .iter()
            .all(|name| positive(name))
            || field("municipality").len() != 6
            || field("postcode").len() != 5
            || !field("postcode").bytes().all(|b| b.is_ascii_digit())
            || !matches!(field("number_type"), "č.p." | "č.ev.")
            || ["city", "part"].iter().any(|name| field(name).is_empty())
            || (!field("orientation").is_empty()
                && (!positive("orientation") || field("street").is_empty()))
            || (field("orientation").is_empty() && !field("letter").is_empty())
            || field("letter").len() > 1
            || !field("letter").bytes().all(|b| b.is_ascii_alphabetic())
            || (field("number_type") == "č.ev." && !field("orientation").is_empty())
        {
            continue;
        }
        visit(Address {
            id: field("id").into(),
            municipality: field("municipality").into(),
            city: field("city").into(),
            city_district: field("city_district").into(),
            part: field("part").into(),
            street: field("street").into(),
            number_type: field("number_type").into(),
            descriptive: field("descriptive").into(),
            orientation: field("orientation").into(),
            letter: field("letter").into(),
            postcode: field("postcode").into(),
            region: field("region").into(),
            point: [lat, lon],
        })?;
    }
    Ok(())
}

pub fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate RUIAN id {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "RUIAN CSV lacks requested address ids"
    );
    Ok(found)
}

fn key(seed: &str, id: &str) -> String {
    sha256_hex(format!("{seed}\0{id}").as_bytes())
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("RUIAN file name")?;
    let mut pools: BTreeMap<(String, &'static str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let pool = pools.entry((a.region.clone(), a.category())).or_default();
        let hash = key(seed, &a.id);
        // ponytail: geographic diversity is limited to 200 candidates per region and numbering class; enlarge the pool if it cannot supply five municipalities or city districts.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((region, category), pool) in pools {
        let mut places = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| places.insert((&a.municipality, &a.city_district)))
            .take(5)
            .collect();
        eprintln!("RUIAN {region} {category}: {} locations", selected.len());
        for a in selected {
            let number = if category == "evidential" {
                format!("č.ev. {}", a.descriptive)
            } else {
                a.number()
            };
            let base = json!({"kind":"address", "source":"ruian-csv", "ref":a.id, "cohort":format!("cz_{category}"), "queries":[a.query(&number)], "label":a.query(&number), "tags":["registry-sample", "cz", "ruian", release, format!("region:{region}"), format!("municipality:{}", a.municipality), category]});
            lines.push(base.clone());
            if !a.orientation.is_empty() {
                for (form, number) in [
                    ("orientation_only", format!("{}{}", a.orientation, a.letter)),
                    ("descriptive_only", format!("č.p. {}", a.descriptive)),
                ] {
                    let mut variant = base.clone();
                    variant["cohort"] = json!(format!("cz_{form}"));
                    variant["queries"] = json!([a.query(&number)]);
                    variant["tags"]
                        .as_array_mut()
                        .unwrap()
                        .extend([json!("number_form"), json!(form)]);
                    lines.push(variant);
                }
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible RUIAN addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "ruian-csv",
    command: "sample-cz",
    country: "cz",
    input: "ruian",
    license: "CC-BY-4.0",
    location: "ruian",
    url: |id| format!("https://vdp.cuzk.gov.cz/vdp/ruian/adresnimista/{id}"),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    lookup(path, refs)?.into_iter().map(|(id, row)| {
        let number_aliases = row.number_aliases(); let mut metadata = metadata.clone(); metadata.insert("dataset".into(), json!(URL)); metadata.insert("coordinate_transform".into(), json!({"from":"EPSG:5514", "to":"EPSG:4326", "input":"(-Y,-X)", "tool":"DuckDB spatial / PROJ", "always_xy":true})); metadata.insert("numbering".into(), json!({"type":row.number_type, "descriptive":row.descriptive, "orientation":row.orientation, "letter":row.letter})); let mut components = indexmap::IndexMap::from([("house_number", row.number()), ("postcode", row.postcode)]); if !row.street.is_empty() { components.insert("road", row.street); }
        Ok((id, Entry {
            point: Point::Wgs84(row.point),
            components,
            radius_floor_km: 0.,
            provenance: metadata.clone(),
            number_aliases,
        }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_numbers_share_one_point_and_sampling_does_not_depend_on_row_order() {
        let path = std::env::temp_dir().join(format!("ruian-test-{}.csv", std::process::id()));
        let header = "id,municipality,city,city_district,part,street,number_type,descriptive,orientation,letter,postcode,region,lat,lon\n";
        let rows = [
            "1,554782,Praha,500054,Nové Město,Testovací,č.p.,123,45,a,11000,19,50.08,14.43\n",
            "2,500011,Ves,,Ves,,č.p.,12,,,12345,27,50.1,14.5\n",
            "3,500012,Ves,,Ves,\"Druhá, ulice\",č.p.,13,,,12345,27,50.1,14.5\n",
            "4,500013,Ves,,Ves,,č.ev.,14,,,12345,27,50.1,14.5\n",
            "5,500014,Ves,,Ves,Ulice,č.p.,15,,a,12345,27,50.1,14.5\n",
        ];
        let write =
            |rows: &[&str]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 6);
        let mut reversed = rows;
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let ids = BTreeSet::from(["1".into(), "3".into(), "4".into()]);
        let found = lookup(&path, &ids).unwrap();
        let a = &found["1"];
        assert_eq!(a.number(), "123/45a");
        assert_eq!(a.point, [50.08, 14.43]);
        assert_eq!(found["3"].street, "Druhá, ulice");
        let expect = crate::registry::with_number_aliases(
            json!({"country":["cz"], "level":"house", "max_level":"house", "point":a.point, "radius_km":0.15, "good_match":true, "components":{"house_number":a.number(), "postcode":a.postcode}}),
            &a.number_aliases(),
        );
        let expect: crate::score::Expect = serde_json::from_value(expect).unwrap();
        for number in ["123/45a", "123", "45a", "45", "999"] {
            let response = json!({"goodMatch":true,"displayName":"Testovací", "geoCoordinates":{"lat":50.08,"lon":14.43}, "addressDetails":{"countryCode":"cz","road":"Testovací","houseNumber":number,"postcode":"11000"}});
            let class = crate::score::score(Some(&expect), &response, true).class;
            assert_eq!(
                class == "pass",
                ["123/45a", "123", "45a"].contains(&number),
                "{number}: {class}"
            );
        }
        assert!(lookup(&path, &BTreeSet::from(["missing".into()])).is_err());
        write(&[rows[0], rows[0]]);
        assert!(
            lookup(&path, &ids)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        write(&["1,554782,Praha,,Praha,Ulice,č.p.,1,,,11000,19,1040000,740000\n"]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("EPSG:4326")
        );
        std::fs::remove_file(path).unwrap();
    }
}
