use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs::File,
    io::BufReader,
    path::Path,
};

pub const URL: &str = "https://data.slovensko.sk/datasety/b27f57f1-7e76-45e0-8968-631f9176b2e9";

const COLUMNS: [&str; 11] = [
    "IDENTIFIKATOR",
    "KRAJ",
    "OKRES",
    "OBEC",
    "CAST_OBCE",
    "ULICA",
    "SUPISNE_CISLO",
    "ORIENTACNE_CISLO_CELE",
    "PSC",
    "ADRBOD_X",
    "ADRBOD_Y",
];

/// Locations per region and class, each from a different municipality.
const PER_CLASS: usize = 5;

#[derive(Clone, Debug)]
struct Address {
    id: String,
    region: String,
    district: String,
    municipality: String,
    part: String,
    street: String,
    descriptive: String,
    orientation: String,
    postcode: String,
    point: [f64; 2],
}

impl Address {
    fn number(&self) -> String {
        if self.orientation.is_empty() {
            self.descriptive.clone()
        } else {
            format!("{}/{}", self.descriptive, self.orientation)
        }
    }

    /// "Banská 150/2, 976 32 Badín": a village house is numbered on its part or municipality.
    fn query(&self, number: &str) -> String {
        let place = [&self.street, &self.part, &self.municipality]
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap();
        format!(
            "{place} {number}, {} {} {}, Slovakia",
            &self.postcode[..3],
            &self.postcode[3..],
            self.municipality
        )
    }

    fn category(&self) -> &'static str {
        if self.orientation.is_empty() {
            "village"
        } else if self.orientation.ends_with(|c: char| c.is_ascii_uppercase()) {
            "dual_letter"
        } else {
            "dual"
        }
    }

    fn number_aliases(&self) -> Option<BTreeSet<String>> {
        (!self.orientation.is_empty()).then(|| {
            BTreeSet::from([
                self.number(),
                self.descriptive.clone(),
                self.orientation.clone(),
            ])
        })
    }

    /// The municipality, its part, and for a borough of Bratislava or Košice
    /// ("Bratislava-Ružinov" in district "Bratislava II") the city.
    fn places(&self) -> Vec<String> {
        let city = self
            .municipality
            .split_once('-')
            .map(|(city, _)| city)
            .filter(|city| self.district.starts_with(&format!("{city} ")));
        super::places(
            [self.part.as_str(), &self.municipality]
                .into_iter()
                .chain(city),
        )
    }
}

fn digits(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit())
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty Register adries CSV"
    );
    let header = crate::registry::nad::split(line.trim_start_matches('\u{feff}').trim_end(), ';');
    ensure!(
        header.iter().map(|s| s.as_ref()).eq(COLUMNS),
        "{} is not the Register adries \"Adresy podľa krajov\" CSV",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(500_000) {
            eprintln!("RA: {n} rows read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ';');
        ensure!(
            fields.len() == COLUMNS.len(),
            "RA CSV row {n}: column count"
        );
        let field = |i: usize| fields[i].trim();
        if field(9).is_empty() && field(10).is_empty() {
            continue;
        }
        let coordinate = |i: usize| field(i).replace(',', ".").parse::<f64>();
        let (lon, lat) = (
            coordinate(9).context("RA longitude")?,
            coordinate(10).context("RA latitude")?,
        );
        ensure!(
            (47.7..=49.7).contains(&lat) && (16.8..=22.6).contains(&lon),
            "RA CSV row {n}: needs Slovak WGS84 longitude, latitude"
        );
        let (street, orientation) = (field(5), field(7));
        let orientation_ok = orientation.is_empty()
            || digits(orientation.trim_end_matches(|c: char| c.is_ascii_uppercase()))
                && orientation.len()
                    - orientation
                        .trim_end_matches(|c: char| c.is_ascii_uppercase())
                        .len()
                    <= 1;
        let id = field(0);
        if id.is_empty()
            || !id.trim_end_matches('A').bytes().all(|b| b.is_ascii_digit())
            || !digits(field(6))
            || !orientation_ok
            || street.is_empty() != orientation.is_empty()
            || field(8).len() != 5
            || !field(8).bytes().all(|b| b.is_ascii_digit())
            || [1, 2, 3].iter().any(|&i| field(i).is_empty())
        {
            continue;
        }
        eligible += 1;
        visit(Address {
            id: id.into(),
            region: field(1).into(),
            district: field(2).into(),
            municipality: field(3).into(),
            part: field(4).into(),
            street: street.into(),
            descriptive: field(6).into(),
            orientation: orientation.into(),
            postcode: field(8).into(),
            point: [lat, lon],
        })?;
    }
    eprintln!("RA: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate RA id {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "RA CSV lacks requested address ids (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("RA file name")?;
    let (mut seen, mut repeated) = (HashSet::new(), HashSet::new());
    let mut pools: BTreeMap<(String, &'static str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        if !seen.insert(a.id.clone()) {
            repeated.insert(a.id.clone());
        }
        let pool = pools.entry((a.region.clone(), a.category())).or_default();
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        // ponytail: municipality diversity draws on the 200 smallest hashes per region and class; enlarge the pool if it cannot supply PER_CLASS municipalities.
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
            .filter(|a| !repeated.contains(&a.id) && places.insert((&a.district, &a.municipality)))
            .take(PER_CLASS)
            .collect();
        eprintln!("RA {region} {category}: {} locations", selected.len());
        for a in selected {
            let query = a.query(&a.number());
            let base = json!({"kind":"address", "source":"ra", "ref":a.id, "cohort":format!("sk_{category}"), "queries":[query], "label":query, "tags":["registry-sample", "sk", "ra", release, format!("region:{region}"), format!("district:{}", a.district), category]});
            lines.push(base.clone());
            if !a.orientation.is_empty() {
                let mut variant = base;
                variant["cohort"] = json!("sk_orientation_only");
                variant["queries"] = json!([a.query(&a.orientation)]);
                variant["tags"]
                    .as_array_mut()
                    .unwrap()
                    .extend([json!("number_form"), json!("orientation_only")]);
                lines.push(variant);
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible RA addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "ra",
    command: "sample-sk",
    country: "sk",
    input: "ra",
    license: "CC0-1.0",
    location: "ra",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("ADRBOD_X, ADRBOD_Y: WGS84 longitude, latitude with decimal commas"),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(id, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), json!({"IDENTIFIKATOR":id, "KRAJ":a.region, "OKRES":a.district, "OBEC":a.municipality, "CAST_OBCE":a.part, "ULICA":a.street, "SUPISNE_CISLO":a.descriptive, "ORIENTACNE_CISLO_CELE":a.orientation, "PSC":a.postcode}));
            let mut components = indexmap::IndexMap::from([
                ("house_number", a.number()),
                (
                    "postcode",
                    format!("{} {}", &a.postcode[..3], &a.postcode[3..]),
                ),
            ]);
            if !a.street.is_empty() {
                components.insert("road", a.street.clone());
            }
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(a.point),
                    components,
                    radius_floor_km: 0.,
                    provenance,
                    number_aliases: a.number_aliases(),
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
    fn eligible_rows_number_forms_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("ra-test-{}.csv", std::process::id()));
        let header = format!("{}\r\n", COLUMNS.join(";"));
        let rows = [
            "4320536;Banskobystrický;Banská Bystrica;Badín;;Banská;150;2;97632;19,121502;48,6661543\r\n",
            "5;Bratislavský;Bratislava II;Bratislava-Ružinov;;Bajkalská;1234;5A;82108;17,14;48,15\r\n",
            "6A;Banskobystrický;Banská Bystrica;Horná Lehota;Horná Lehota;;12;;97652;19,5;48,8\r\n",
            "7;Banskobystrický;Banská Bystrica;Dlhé Pole;Stolečné;;13;;01332;19,5;48,8\r\n",
            "8;Banskobystrický;Banská Bystrica;Ves;;;14;;;19,5;48,8\r\n",
            "9;Banskobystrický;Banská Bystrica;Ves;;Ulica;15;;97652;19,5;48,8\r\n",
            "10;Banskobystrický;Banská Bystrica;Ves;;;16;;97652;;\r\n",
            "11;Banskobystrický;Banská Bystrica;Ves;;Ulica;17;2AB;97652;19,5;48,8\r\n",
            "12;Banskobystrický;Banská Bystrica;Ves2;;Ulica;18;3;97652;19,5;48,8\r\n",
            "12;Banskobystrický;Banská Bystrica;Ves2;;;18;;97652;19,5;48,8\r\n",
        ];
        let write = |rows: &[&str]| {
            std::fs::write(&path, format!("\u{feff}{header}{}", rows.concat())).unwrap()
        };
        write(&rows);
        let first = sample(&path, "test").unwrap();
        let queries: BTreeSet<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            BTreeSet::from([
                "Banská 150/2, 976 32 Badín, Slovakia",
                "Banská 2, 976 32 Badín, Slovakia",
                "Bajkalská 1234/5A, 821 08 Bratislava-Ružinov, Slovakia",
                "Bajkalská 5A, 821 08 Bratislava-Ružinov, Slovakia",
                "Horná Lehota 12, 976 52 Horná Lehota, Slovakia",
                "Stolečné 13, 013 32 Dlhé Pole, Slovakia",
            ])
        );
        let mut reversed = rows;
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["5".into(), "7".into()])).unwrap();
        assert_eq!(found["5"].places(), ["Bratislava-Ružinov", "Bratislava"]);
        assert_eq!(found["7"].places(), ["Stolečné", "Dlhé Pole"]);
        assert_eq!(found["7"].number_aliases(), None);
        let a = &found["5"];
        let expect = crate::registry::with_number_aliases(
            json!({"country":["sk"], "level":"house", "max_level":"house", "point":a.point, "radius_km":0.15, "good_match":true, "components":{"house_number":a.number(), "postcode":"821 08"}}),
            &a.number_aliases(),
        );
        let expect: crate::score::Expect = serde_json::from_value(expect).unwrap();
        for number in ["1234/5A", "1234", "5A", "5", "999"] {
            let response = json!({"goodMatch":true, "geoCoordinates":{"lat":48.15,"lon":17.14}, "addressDetails":{"countryCode":"sk","road":"Bajkalská","houseNumber":number,"postcode":"82108"}});
            let class = crate::score::score(Some(&expect), &response, true).class;
            assert_eq!(
                class == "pass",
                ["1234/5A", "1234", "5A"].contains(&number),
                "{number}: {class}"
            );
        }
        for id in ["8", "9", "10", "11"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        assert!(
            lookup(&path, &BTreeSet::from(["12".into()]))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        write(&[&rows[0].replace("19,121502;48,6661543", "48,6661543;19,121502")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("Slovak")
        );
        std::fs::remove_file(path).unwrap();
    }
}
