use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use flate2::read::MultiGzDecoder;
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
};

pub const URL: &str = "https://avoindata.suomi.fi/data/fi/dataset/rakennetun-ympariston-tietojarjestelman-rakennusten-osoitetiedot";

const COLUMNS: [&str; 19] = [
    "address_key",
    "municipality_number",
    "municipality",
    "municipality_sv",
    "region_code",
    "region",
    "street_fin",
    "street_swe",
    "number",
    "letter",
    "number2",
    "letter2",
    "postal_code",
    "postal_office_fin",
    "postal_office_swe",
    "buildings",
    "spread_m",
    "lat",
    "lon",
];

/// Locations per region and class, each from a different municipality.
const TAKE: [(&str, usize); 2] = [("plain", 5), ("letter", 3)];

/// The farthest apart, in metres, the buildings sharing one address may be.
const SPREAD_M: f64 = 50.;

/// One address of `tests/registry/ryhti-csv.sql`: its first building's point, and the
/// municipality and region names Statistics Finland gives its municipality number.
#[derive(Clone, Debug)]
struct Address {
    key: String,
    municipality_number: String,
    municipality: String,
    municipality_sv: String,
    region: String,
    street_fin: String,
    street_swe: String,
    number: String,
    letter: String,
    postcode: String,
    office_fin: String,
    office_swe: String,
    point: [f64; 2],
}

impl Address {
    fn number(&self) -> String {
        format!("{}{}", self.number, self.letter)
    }

    /// The Finnish form, then the Swedish one where its street or post office differs: Posti's
    /// "Tora-alhonkatu 7b, 20100 TURKU", the post office in capitals as the register writes it.
    fn queries(&self) -> Vec<(&'static str, String)> {
        let form = |street: &str, office: &str| {
            format!(
                "{street} {}, {} {office}, Finland",
                self.number(),
                self.postcode
            )
        };
        let mut out = vec![];
        if !self.street_fin.is_empty() && !self.office_fin.is_empty() {
            out.push(("finnish", form(&self.street_fin, &self.office_fin)));
        }
        if !self.street_swe.is_empty()
            && !self.office_swe.is_empty()
            && (out.is_empty()
                || self.street_swe != self.street_fin
                || self.office_swe != self.office_fin)
        {
            out.push(("swedish", form(&self.street_swe, &self.office_swe)));
        }
        out
    }

    fn category(&self) -> &'static str {
        if self.letter.is_empty() {
            "plain"
        } else {
            "letter"
        }
    }

    fn places(&self) -> Vec<String> {
        super::places([
            self.municipality.as_str(),
            &self.municipality_sv,
            &self.office_fin,
            &self.office_swe,
        ])
    }
}

fn digits(s: &str) -> bool {
    (1..=5).contains(&s.len()) && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit())
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, MultiGzDecoder::new(File::open(path)?));
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty Ryhti CSV"
    );
    let header = crate::registry::nad::split(line.trim_end(), ',');
    ensure!(
        header.iter().map(|s| s.as_ref()).eq(COLUMNS),
        "{} is not a CSV prepared by tests/registry/ryhti-csv.sql",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(500_000) {
            eprintln!("Ryhti: {n} addresses read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "Ryhti CSV row {n}: column count"
        );
        let field = |i: usize| fields[i].trim();
        let lat: f64 = field(17).parse().context("Ryhti latitude")?;
        let lon: f64 = field(18).parse().context("Ryhti longitude")?;
        ensure!(
            (59.5..=70.2).contains(&lat) && (19.0..=31.7).contains(&lon),
            "Ryhti CSV row {n}: needs Finnish WGS84 latitude, longitude"
        );
        let (number, letter, postcode) = (field(8), field(9), field(12));
        // A second number is a range ("Peurantie 7-9") or a side road's number on a main road
        // ("Uusi Porvoontie 849-15"), which the register does not tell apart.
        if !digits(number)
            || !(letter.is_empty()
                || letter.len() == 1 && letter.bytes().all(|b| b.is_ascii_lowercase()))
            || !field(10).is_empty()
            || !field(11).is_empty()
            || postcode.len() != 5
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || field(16).parse::<f64>().context("Ryhti spread")? > SPREAD_M
            || [6, 7, 13, 14].iter().any(|&i| field(i).contains(','))
            || [0, 1, 2, 3, 4, 5].iter().any(|&i| field(i).is_empty())
        {
            continue;
        }
        let address = Address {
            key: field(0).into(),
            municipality_number: field(1).into(),
            municipality: field(2).into(),
            municipality_sv: field(3).into(),
            region: field(5).into(),
            street_fin: field(6).into(),
            street_swe: field(7).into(),
            number: number.into(),
            letter: letter.into(),
            postcode: postcode.into(),
            office_fin: field(13).into(),
            office_swe: field(14).into(),
            point: [lat, lon],
        };
        if address.queries().is_empty() {
            continue;
        }
        eligible += 1;
        visit(address)?;
    }
    eprintln!("Ryhti: {n} addresses, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, keys: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if keys.contains(&a.key) {
            ensure!(!found.contains_key(&a.key), "duplicate Ryhti key {}", a.key);
            found.insert(a.key.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == keys.len(),
        "Ryhti CSV lacks requested address keys (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("Ryhti file name")?;
    let mut pools: BTreeMap<(String, &'static str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let pool = pools.entry((a.region.clone(), a.category())).or_default();
        let hash = sha256_hex(format!("{seed}\0{}", a.key).as_bytes());
        // ponytail: municipality diversity draws on the 200 smallest hashes per region and class; enlarge the pool if it cannot supply enough municipalities.
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
        let take = TAKE.iter().find(|(c, _)| *c == category).unwrap().1;
        let mut municipalities = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| municipalities.insert(&a.municipality_number))
            .take(take)
            .collect();
        eprintln!("Ryhti {region} {category}: {} locations", selected.len());
        for a in selected {
            let queries = a.queries();
            let label = queries[0].1.clone();
            for (i, (language, query)) in queries.into_iter().enumerate() {
                let cohort = if i == 0 {
                    format!("fi_{category}")
                } else {
                    "fi_swedish".into()
                };
                lines.push(json!({"kind":"address", "source":"ryhti", "ref":a.key, "cohort":cohort, "queries":[query], "label":label, "tags":["registry-sample", "fi", "ryhti", release, format!("region:{region}"), format!("municipality:{}", a.municipality_number), category, language]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible Ryhti addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "ryhti",
    command: "sample-fi",
    country: "fi",
    input: "ryhti",
    license: "CC-BY-4.0",
    location: "ryhti",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert(
        "dataset".into(),
        json!("Lähde: Syke Ryhti (Rakennetun ympäristön tietojärjestelmän rakennusten osoitetiedot, open_address)"),
    );
    metadata.insert(
        "coordinates".into(),
        json!("the first (by address_key) building's point, served by the WFS in CRS:84 from EPSG:3067"),
    );
    metadata.insert(
        "transformation".into(),
        json!("one row per address, prepared by tests/registry/ryhti-csv.sql with municipality and region names from Statistics Finland's classifications 2026; queries constructed from the fields, not by Syke"),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(key, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), json!({"address_key":key, "municipality_number":a.municipality_number, "address_name_fin":a.street_fin, "address_name_swe":a.street_swe, "number_part_of_address_number":a.number, "subdivision_letter_of_address_number":a.letter, "postal_code":a.postcode, "postal_office_fin":a.office_fin, "postal_office_swe":a.office_swe}));
            let road = [&a.street_fin, &a.street_swe]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap()
                .clone();
            Ok((
                key,
                Entry {
                    point: Point::Wgs84(a.point),
                    components: indexmap::IndexMap::from([
                        ("house_number", a.number()),
                        ("postcode", a.postcode.clone()),
                        ("road", road),
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
    use std::io::Write;

    #[test]
    fn eligible_addresses_language_forms_and_order_independent_sampling() {
        let path = std::env::temp_dir().join(format!("ryhti-test-{}.csv.gz", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let row = |key: &str, edit: &dyn Fn(&mut Vec<String>)| {
            let mut f: Vec<String> = "091,Helsinki,Helsingfors,01,Uusimaa,Ulvilantie,Ulfsbyvägen,2,\"\",\"\",\"\",00350,HELSINKI,HELSINGFORS,2,35.0,60.2040347,24.8784703"
                .split(',')
                .map(String::from)
                .collect();
            edit(&mut f);
            format!("{key},{}\n", f.join(","))
        };
        let rows = [
            row("a1", &|_| {}),
            row("a2", &|f| {
                [f[0], f[1], f[2], f[3], f[4]] = [
                    "179".into(),
                    "Jyväskylä".into(),
                    "Jyväskylä".into(),
                    "13".into(),
                    "Keski-Suomi".into(),
                ];
                [f[5], f[6], f[7], f[8]] = [
                    "Valkeajärventie".into(),
                    "\"\"".into(),
                    "11".into(),
                    "b".into(),
                ];
                [f[11], f[12], f[13]] = ["41800".into(), "KORPILAHTI".into(), "KORPILAHTI".into()];
            }),
            row("a3", &|f| {
                [f[0], f[1], f[2], f[3], f[4]] = [
                    "478".into(),
                    "Maarianhamina".into(),
                    "Mariehamn".into(),
                    "21".into(),
                    "Ahvenanmaa".into(),
                ];
                [f[5], f[6]] = ["Gnistvägen".into(), "Gnistvägen".into()];
                [f[12], f[13]] = ["MARIEHAMN".into(), "MARIEHAMN".into()];
            }),
            row("a4", &|f| {
                [f[0], f[1], f[2], f[3], f[4]] = [
                    "035".into(),
                    "Brändö".into(),
                    "Brändö".into(),
                    "21".into(),
                    "Ahvenanmaa".into(),
                ];
                [f[5], f[6]] = ["\"\"".into(), "Västernäsvägen".into()];
                [f[12], f[13]] = ["\"\"".into(), "JURMO".into()];
            }),
            row("b1", &|f| f[9] = "9".into()),
            row("b2", &|f| f[8] = "AB".into()),
            row("b3", &|f| f[7] = "02".into()),
            row("b4", &|f| f[15] = "51.0".into()),
            row("b5", &|f| f[11] = "0035".into()),
            row("b6", &|f| f[5] = "\"Tie, 2\"".into()),
            row("b7", &|f| [f[12], f[13]] = ["\"\"".into(), "\"\"".into()]),
            row("b8", &|f| f[1] = "\"\"".into()),
        ];
        let write = |rows: &[String]| {
            let mut gz = flate2::write::GzEncoder::new(
                File::create(&path).unwrap(),
                flate2::Compression::default(),
            );
            gz.write_all(format!("{header}{}", rows.concat()).as_bytes())
                .unwrap();
            gz.finish().unwrap();
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
                "Ulvilantie 2, 00350 HELSINKI, Finland",
                "Ulfsbyvägen 2, 00350 HELSINGFORS, Finland",
                "Valkeajärventie 11b, 41800 KORPILAHTI, Finland",
                "Gnistvägen 2, 00350 MARIEHAMN, Finland",
                "Västernäsvägen 2, 00350 JURMO, Finland",
            ])
        );
        let swedish: Vec<_> = first
            .iter()
            .filter(|s| s["cohort"] == "fi_swedish")
            .collect();
        assert_eq!(swedish.len(), 1);
        assert_eq!(swedish[0]["label"], "Ulvilantie 2, 00350 HELSINKI, Finland");
        let mut reversed = rows.clone();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let entries = prepare(&path, &BTreeSet::from(["a2".into(), "a4".into()])).unwrap();
        assert_eq!(entries["a2"].components["house_number"], "11b");
        assert_eq!(entries["a2"].components["postcode"], "41800");
        assert_eq!(entries["a2"].places, ["Jyväskylä", "KORPILAHTI"]);
        assert_eq!(entries["a4"].components["road"], "Västernäsvägen");
        assert_eq!(entries["a4"].places, ["Brändö", "JURMO"]);
        for key in ["b1", "b2", "b3", "b4", "b5", "b6", "b7", "b8"] {
            assert!(
                lookup(&path, &BTreeSet::from([key.into()])).is_err(),
                "{key}"
            );
        }
        write(&[rows[0].clone(), rows[0].clone()]);
        assert!(
            lookup(&path, &BTreeSet::from(["a1".into()]))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        write(&[rows[0].replace("60.2040347,24.8784703", "24.8784703,60.2040347")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("Finnish")
        );
        std::fs::remove_file(path).unwrap();
    }
}
