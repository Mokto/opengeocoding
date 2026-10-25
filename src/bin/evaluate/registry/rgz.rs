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

pub const URL: &str = "https://data.gov.rs/sr/datasets/adresni-registar/";

/// Locations per class, each from a different municipality.
const PER_CLASS: usize = 60;

const COLUMNS: [&str; 18] = [
    "id",
    "municipality_id",
    "municipality",
    "municipality_lat",
    "settlement_id",
    "settlement",
    "settlement_lat",
    "street_id",
    "street",
    "street_lat",
    "number",
    "number_lat",
    "kind",
    "place",
    "place_lat",
    "copies",
    "lat",
    "lon",
];

#[derive(Clone, Debug)]
struct Address {
    id: String,
    municipality_id: String,
    municipality: String,
    settlement_id: String,
    settlement: String,
    street_id: String,
    street: String,
    street_lat: String,
    number: String,
    number_lat: String,
    kind: String,
    place: String,
    place_lat: String,
    point: [f64; 2],
}

impl Address {
    fn queries(&self) -> [(&'static str, String); 2] {
        [
            (
                "cyrillic",
                format!("{} {}, {}, Србија", self.street, self.number, self.place),
            ),
            (
                "latin",
                format!(
                    "{} {}, {}, Serbia",
                    self.street_lat, self.number_lat, self.place_lat
                ),
            ),
        ]
    }

    fn category(&self) -> &'static str {
        if self.street == self.settlement {
            "village"
        } else if self.number_lat.ends_with(|c: char| c.is_ascii_uppercase()) {
            "letter"
        } else {
            "plain"
        }
    }

    /// The register writes a letter in each script ("27А", "27A"); an answer may use either.
    fn number_aliases(&self) -> Option<BTreeSet<String>> {
        (self.number != self.number_lat)
            .then(|| BTreeSet::from([self.number.clone(), self.number_lat.clone()]))
    }
}

/// A house number of one to four digits and at most one letter, the same in both scripts; the
/// register's digraph letters ("158Њ", "158NJ") are left out.
fn number_ok(cyrillic: &str, latin: &str) -> bool {
    let digits = latin.trim_end_matches(|c: char| c.is_ascii_uppercase());
    let letter = |s: &str, is_letter: fn(char) -> bool| {
        s.strip_prefix(digits)
            .is_some_and(|rest| rest.chars().count() <= 1 && rest.chars().all(is_letter))
    };
    (1..=4).contains(&digits.len())
        && !digits.starts_with('0')
        && digits.bytes().all(|b| b.is_ascii_digit())
        && letter(latin, |c| c.is_ascii_uppercase())
        && letter(cyrillic, |c| {
            ('А'..='Я').contains(&c) || "ЂЈЉЊЋЏ".contains(c)
        })
        && (cyrillic == digits) == (latin == digits)
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty RGZ CSV"
    );
    let header = crate::registry::nad::split(line.trim_end(), ',');
    ensure!(
        header.iter().map(|s| s.as_ref()).eq(COLUMNS),
        "{} is not a CSV prepared by tests/registry/rgz-csv.sql",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(1_000_000) {
            eprintln!("RGZ: {n} rows read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "RGZ CSV row {n}: column count"
        );
        let field = |i: usize| fields[i].trim();
        let lat: f64 = field(16).parse().context("RGZ latitude")?;
        let lon: f64 = field(17).parse().context("RGZ longitude")?;
        ensure!(
            (41.8..=46.3).contains(&lat) && (18.8..=23.1).contains(&lon),
            "RGZ CSV row {n}: needs Serbian EPSG:4326 coordinates"
        );
        let name_ok =
            |s: &str| s.chars().filter(|c| c.is_alphabetic()).count() >= 2 && !s.contains(',');
        if field(15) != "1"
            || !number_ok(field(10), field(11))
            || ![8, 9, 13, 14].iter().all(|&i| name_ok(field(i)))
            || [0, 1, 2, 4, 5, 7].iter().any(|&i| field(i).is_empty())
        {
            continue;
        }
        eligible += 1;
        visit(Address {
            id: field(0).into(),
            municipality_id: field(1).into(),
            municipality: field(2).into(),
            settlement_id: field(4).into(),
            settlement: field(5).into(),
            street_id: field(7).into(),
            street: field(8).into(),
            street_lat: field(9).into(),
            number: field(10).into(),
            number_lat: field(11).into(),
            kind: field(12).into(),
            place: field(13).into(),
            place_lat: field(14).into(),
            point: [lat, lon],
        })?;
    }
    eprintln!("RGZ: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate RGZ id {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "RGZ CSV lacks requested house number ids (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("RGZ file name")?;
    let mut firsts: BTreeMap<(&'static str, String), (String, Address)> = BTreeMap::new();
    for_each(path, |a| {
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        let key = (a.category(), a.municipality_id.clone());
        if firsts.get(&key).is_none_or(|(old, _)| hash < *old) {
            firsts.insert(key, (hash, a));
        }
        Ok(())
    })?;
    let mut classes: BTreeMap<&'static str, Vec<(String, Address)>> = BTreeMap::new();
    for ((category, _), first) in firsts {
        classes.entry(category).or_default().push(first);
    }
    let mut lines = vec![];
    for (category, mut firsts) in classes {
        firsts.sort_by(|a, b| a.0.cmp(&b.0));
        firsts.truncate(PER_CLASS);
        firsts.sort_by(|a, b| a.1.id.cmp(&b.1.id));
        eprintln!("RGZ {category}: {} locations", firsts.len());
        for (_, a) in firsts {
            let [(_, label), _] = a.queries();
            for (form, query) in a.queries() {
                lines.push(json!({"kind":"address", "source":"rgz", "ref":a.id, "cohort":format!("rs_{category}"), "queries":[query], "label":label, "tags":["registry-sample", "rs", "rgz", release, format!("municipality:{}", a.municipality_id), category, form, if a.kind.is_empty() { "unestablished" } else { "established" }]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible RGZ addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "rgz",
    command: "sample-rs",
    country: "rs",
    input: "rgz",
    license: "SODL-1.0",
    location: "rgz",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert(
        "coordinates".into(),
        json!("WKT point in ETRS89 / UTM zone 34N (EPSG:25834), transformed to EPSG:4326 with DuckDB spatial / PROJ"),
    );
    lookup(path, refs)?.into_iter().map(|(id, a)| {
        let mut provenance = metadata.clone();
        provenance.insert("record".into(), json!({"kucni_broj_id":a.id, "kucni_broj":a.number, "kucni_broj_lat":a.number_lat, "tip_lat":a.kind, "ulica_maticni_broj":a.street_id, "ulica_ime":a.street, "ulica_ime_lat":a.street_lat, "naselje_maticni_broj":a.settlement_id, "naselje_ime":a.settlement, "opstina_maticni_broj":a.municipality_id, "opstina_ime":a.municipality}));
        Ok((id, Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.number_lat.clone())]), radius_floor_km: 0., provenance, number_aliases: a.number_aliases(), places: super::places([a.place_lat.as_str(), &a.place, &a.settlement, &a.municipality]) }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligible_numbers_both_scripts_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("rgz-test-{}.csv", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let rows = [
            "1,89010,НОВИ САД,NOVI SAD,802867,СРЕМСКА КАМЕНИЦА,SREMSKA KAMENICA,802867000973,ЦАРА ЛАЗАРА,CARA LAZARA,61,61,UTVRĐENI KBr,СРЕМСКА КАМЕНИЦА,SREMSKA KAMENICA,1,45.2175541,19.8437968\n",
            "2,70602,КЊАЖЕВАЦ,KNJAŽEVAC,717754,ОРЕШАЦ,OREŠAC,717754000003,АЛБАНСКА,ALBANSKA,27А,27A,,ОРЕШАЦ,OREŠAC,1,43.5,22.2\n",
            "3,70157,ЗЕМУН,ZEMUN,703915,БЕОГРАД (ЗЕМУН),BEOGRAD (ZEMUN),703915098603,ЊЕГОШЕВА,NJEGOŠEVA,10,10,,БЕОГРАД,BEOGRAD,2,44.8,20.4\n",
            "4,70157,ЗЕМУН,ZEMUN,703916,УГРИНОВЦИ,UGRINOVCI,703916000001,ПУТ,PUT,158Њ,158NJ,,УГРИНОВЦИ,UGRINOVCI,1,44.8,20.2\n",
            "5,71218,УБ,UB,744697,ЛОНЧАНИК,LONČANIK,744697000001,ЛОНЧАНИК,LONČANIK,83,83,UTVRĐENI KBr,УБ,UB,1,44.5,20.1\n",
            "6,71218,УБ,UB,744697,ЛОНЧАНИК,LONČANIK,744697000002,\"СРЦ, ОАЗА\",\"SRC, OAZA\",3,3,,ЛОНЧАНИК,LONČANIK,1,44.5,20.1\n",
            "7,71218,УБ,UB,744697,ЛОНЧАНИК,LONČANIK,744697000003,ОБРЕНОВАЧКИ ПУТ,OBRENOVAČKI PUT,083,083,,ЛОНЧАНИК,LONČANIK,1,44.5,20.1\n",
        ];
        let write =
            |rows: &[&str]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        let queries: Vec<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            [
                "АЛБАНСКА 27А, ОРЕШАЦ, Србија",
                "ALBANSKA 27A, OREŠAC, Serbia",
                "ЦАРА ЛАЗАРА 61, СРЕМСКА КАМЕНИЦА, Србија",
                "CARA LAZARA 61, SREMSKA KAMENICA, Serbia",
                "ЛОНЧАНИК 83, УБ, Србија",
                "LONČANIK 83, UB, Serbia",
            ]
        );
        assert_eq!(first[0]["cohort"], "rs_letter");
        assert_eq!(first[4]["cohort"], "rs_village");
        let mut reversed = rows;
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["1".into(), "2".into()])).unwrap();
        assert_eq!(found["1"].number_aliases(), None);
        let expect = crate::registry::with_number_aliases(
            json!({"country":["rs"], "level":"house", "max_level":"house", "point":found["2"].point, "radius_km":0.15, "good_match":true, "components":{"house_number":"27A"}}),
            &found["2"].number_aliases(),
        );
        let expect: crate::score::Expect = serde_json::from_value(expect).unwrap();
        for number in ["27A", "27а", "27", "27B"] {
            let response = json!({"goodMatch":true, "geoCoordinates":{"lat":43.5,"lon":22.2}, "addressDetails":{"countryCode":"rs","road":"Albanska","houseNumber":number}});
            let class = crate::score::score(Some(&expect), &response, true).class;
            assert_eq!(
                class == "pass",
                ["27A", "27а"].contains(&number),
                "{number}: {class}"
            );
        }
        for id in ["3", "4", "6", "7"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        write(&[rows[0], rows[0]]);
        assert!(
            lookup(&path, &BTreeSet::from(["1".into()]))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        write(&[&rows[0].replace("45.2175541,19.8437968", "5007768.54,409219.05")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("Serbian")
        );
        std::fs::remove_file(path).unwrap();
    }
}
