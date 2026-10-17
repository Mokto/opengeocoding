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

pub const URL: &str = "https://data.geo.admin.ch/ch.swisstopo.amtliches-gebaeudeadressverzeichnis/amtliches-gebaeudeadressverzeichnis_ch/amtliches-gebaeudeadressverzeichnis_ch_2056.csv.zip";
pub const REFRAME: &str = "https://geodesy.geo.admin.ch/reframe/lv95towgs84";
const CANTONS: [&str; 26] = [
    "AG", "AI", "AR", "BE", "BL", "BS", "FR", "GE", "GL", "GR", "JU", "LU", "NE", "NW", "OW", "SG",
    "SH", "SO", "SZ", "TG", "TI", "UR", "VD", "VS", "ZG", "ZH",
];

#[derive(Clone, Debug)]
pub struct Address {
    pub id: String,
    pub number: String,
    pub street: String,
    pub postcode: String,
    pub canton: String,
    city: String,
    municipality: String,
    pub lv95: [f64; 2],
}

impl Address {
    fn query(&self) -> String {
        format!(
            "{} {}, {} {}, Switzerland",
            self.street, self.number, self.postcode, self.city
        )
    }
}

pub fn reframed_point(response: &Value) -> Result<[f64; 2]> {
    let coordinate = |key: &str| -> Result<f64> {
        response[key]
            .as_str()
            .context("REFRAME coordinate string")?
            .parse()
            .context("REFRAME coordinate")
    };
    let point = [coordinate("northing")?, coordinate("easting")?];
    ensure!(
        (-90. ..=90.).contains(&point[0]) && (-180. ..=180.).contains(&point[1]),
        "invalid REFRAME coordinates"
    );
    Ok(point)
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty swisstopo CSV"
    );
    let columns: BTreeMap<String, usize> =
        crate::registry::nad::split(line.trim_end().trim_start_matches('\u{feff}'), ';')
            .iter()
            .enumerate()
            .map(|(i, s)| (s.to_string(), i))
            .collect();
    for name in [
        "ADR_EGAID",
        "STN_LABEL",
        "ADR_NUMBER",
        "BDG_CATEGORY",
        "ZIP_LABEL",
        "COM_FOSNR",
        "COM_CANTON",
        "ADR_STATUS",
        "ADR_OFFICIAL",
        "ADR_EASTING",
        "ADR_NORTHING",
    ] {
        ensure!(columns.contains_key(name), "swisstopo CSV missing {name}");
    }
    let mut n = 0usize;
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(1_000_000) {
            eprintln!("swisstopo: {n} rows read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ';');
        ensure!(
            fields.len() == columns.len(),
            "swisstopo CSV row {n}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        if field("ADR_STATUS") != "real"
            || field("ADR_OFFICIAL") != "true"
            || field("BDG_CATEGORY") != "residential"
        {
            continue;
        }
        ensure!(
            CANTONS.contains(&field("COM_CANTON")),
            "unknown Swiss canton {}",
            field("COM_CANTON")
        );
        let Some((postcode, city)) = field("ZIP_LABEL").split_once(' ') else {
            continue;
        };
        let Ok(easting) = field("ADR_EASTING").parse::<f64>() else {
            continue;
        };
        let Ok(northing) = field("ADR_NORTHING").parse::<f64>() else {
            continue;
        };
        if !(2_400_000. ..=2_900_000.).contains(&easting)
            || !(1_000_000. ..=1_400_000.).contains(&northing)
            || postcode.len() != 4
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || city.trim().is_empty()
            || !field("ADR_EGAID").parse::<u64>().is_ok_and(|n| n > 0)
            || ["STN_LABEL", "ADR_NUMBER", "COM_FOSNR"]
                .iter()
                .any(|name| field(name).is_empty())
        {
            continue;
        }
        visit(Address {
            id: field("ADR_EGAID").into(),
            number: field("ADR_NUMBER").into(),
            street: field("STN_LABEL").into(),
            postcode: postcode.into(),
            city: city.trim().into(),
            canton: field("COM_CANTON").into(),
            municipality: field("COM_FOSNR").into(),
            lv95: [easting, northing],
        })?;
    }
    Ok(())
}

pub fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(
                !found.contains_key(&a.id),
                "duplicate swisstopo EGAID {}",
                a.id
            );
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "swisstopo CSV lacks {} requested EGAIDs",
        ids.len() - found.len()
    );
    Ok(found)
}

fn key(seed: &str, id: &str) -> String {
    sha256_hex(format!("{seed}\0{id}").as_bytes())
}

fn variants(a: &Address, other: &Address) -> Vec<(&'static str, String, Option<bool>)> {
    let query = a.query();
    let mut out = vec![
        (
            "no_country",
            query.trim_end_matches(", Switzerland").into(),
            Some(true),
        ),
        (
            "no_postcode",
            format!("{} {}, {}, Switzerland", a.street, a.number, a.city),
            Some(true),
        ),
        ("uppercase", query.to_uppercase(), Some(true)),
        ("lowercase", query.to_lowercase(), Some(true)),
        ("no_commas", query.replace(',', ""), Some(true)),
        (
            "reordered",
            format!(
                "Switzerland, {} {}, {} {}",
                a.postcode, a.city, a.street, a.number
            ),
            Some(true),
        ),
        ("ascii", deunicode::deunicode(&query), Some(true)),
    ];
    if let Some(street) = crate::registry::ban::typo(&a.street) {
        let mut copy = a.clone();
        copy.street = street;
        out.push(("typo_street", copy.query(), None));
    }
    if let Some(city) = crate::registry::ban::typo(&a.city) {
        let mut copy = a.clone();
        copy.city = city;
        out.push(("typo_town", copy.query(), None));
    }
    if a.postcode != other.postcode {
        let mut copy = a.clone();
        copy.postcode = other.postcode.clone();
        out.push(("wrong_postcode", copy.query(), None));
    }
    if a.city != other.city {
        let mut copy = a.clone();
        copy.city = other.city.clone();
        out.push(("wrong_town", copy.query(), None));
    }
    out.retain(|(_, text, _)| *text != query);
    out
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("swisstopo file name")?;
    let mut pools: BTreeMap<String, BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        // ponytail: diversity is limited to 500 candidates per canton; increase the pool if it cannot supply 20 streets.
        let pool = pools.entry(a.canton.clone()).or_default();
        let hash = key(seed, &a.id);
        if pool.len() < 500 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 500 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    let mut counts = BTreeMap::<&str, usize>::new();
    for (canton, pool) in pools {
        let mut streets = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| streets.insert((a.municipality.clone(), a.street.to_lowercase())))
            .take(20)
            .collect();
        eprintln!("swisstopo {canton}: {} locations", selected.len());
        for (i, a) in selected.iter().enumerate() {
            let tags = vec![
                "registry-sample".to_string(),
                "ch".into(),
                "swisstopo".into(),
                release.into(),
                format!("canton:{canton}"),
                format!("municipality:{}", a.municipality),
                "official".into(),
                "residential".into(),
            ];
            let base = json!({"kind":"address", "source":"swisstopo-csv", "ref":a.id, "cohort":"ch_house", "queries":[a.query()], "label":a.query(), "tags":tags, "extra_regions":[format!("CH-{canton}")]});
            lines.push(base.clone());
            if i % 2 == 0 && selected.len() > 1 {
                let other = selected[(i + 1) % selected.len()];
                let (kind, query, good_match) = variants(a, other)
                    .into_iter()
                    .min_by_key(|(kind, _, _)| {
                        (
                            counts.get(kind).copied().unwrap_or(0),
                            key(seed, &format!("{}:{kind}", a.id)),
                        )
                    })
                    .context("swisstopo variants")?;
                *counts.entry(kind).or_default() += 1;
                let mut variant = base;
                variant["cohort"] = json!("ch_degraded");
                variant["queries"] = json!([query]);
                variant["good_match"] = json!(good_match);
                variant["tags"].as_array_mut().unwrap().extend([
                    json!("variant"),
                    json!("degraded"),
                    json!(kind),
                ]);
                lines.push(variant);
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible swisstopo LV95 addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "swisstopo-csv",
    command: "sample-ch",
    country: "ch",
    input: "swisstopo",
    license: "swisstopo-OGD",
    location: "swisstopo",
    url: |id| format!("{URL}#{id}"),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = {
        let mut metadata = super::provenance(path)?;
        metadata.insert(
            "coordinate_transform".into(),
            json!({"url":REFRAME, "altitude":0}),
        );
        metadata
    };
    lookup(path, refs)?
        .into_iter()
        .map(|(id, row)| {
            Ok((
                id,
                Entry {
                    point: Point::Lv95(row.lv95),
                    components: indexmap::IndexMap::from([
                        ("house_number", row.number),
                        ("road", row.street),
                        ("postcode", row.postcode),
                    ]),
                    radius_floor_km: 0.,
                    provenance: metadata.clone(),
                    number_aliases: None,
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_rows_and_reframe_axes_define_the_expectations() {
        let path = std::env::temp_dir().join(format!("swisstopo-test-{}.csv", std::process::id()));
        let header = "ADR_EGAID;STN_LABEL;ADR_NUMBER;BDG_CATEGORY;ZIP_LABEL;COM_FOSNR;COM_CANTON;ADR_STATUS;ADR_OFFICIAL;ADR_EASTING;ADR_NORTHING\n";
        let rows = [
            "1;Münstergasse;12a;residential;3011 Bern;351;BE;real;true;2600000;1200000\n",
            "2;\"Rue du; Lac\";4;residential;2502 Biel/Bienne;371;BE;real;true;2600100;1200100\n",
            "3;Voie future;5;residential;2502 Biel/Bienne;371;BE;planned;true;2600100;1200100\n",
            "4;Voie officieuse;5;residential;2502 Biel/Bienne;371;BE;real;false;2600100;1200100\n",
            "5;Voie invalide;5;residential;2502 Biel/Bienne;371;BE;real;true;NaN;1200100\n",
        ];
        let write =
            |rows: &[&str]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        let mut reversed = rows;
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        assert_eq!(first.len(), 3);
        assert!(first.iter().all(|s| s["extra_regions"] == json!(["CH-BE"])));
        let found = lookup(&path, &BTreeSet::from(["1".into(), "2".into()])).unwrap();
        assert_eq!(found["1"].number, "12a");
        assert_eq!(found["1"].postcode, "3011");
        assert_eq!(found["1"].lv95, [2_600_000., 1_200_000.]);
        assert_eq!(found["2"].city, "Biel/Bienne");
        assert_eq!(found["2"].street, "Rue du; Lac");
        assert!(lookup(&path, &BTreeSet::from(["absent".into()])).is_err());
        let point =
            reframed_point(&json!({"easting":"7.438632502714563", "northing":"46.95108288705888"}))
                .unwrap();
        assert_eq!(point, [46.95108288705888, 7.438632502714563]);
        assert!(reframed_point(&json!({"easting":"7", "northing":"NaN"})).is_err());
        assert!(reframed_point(&json!({"easting":"7", "northing":"91"})).is_err());
        assert!(reframed_point(&json!({"error":"unavailable"})).is_err());
        write(&["1;Münstergasse;12a;residential;3011 Bern;351;BE;real;true;600000;200000\n"]);
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
