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

pub const URL: &str = "https://data.gov.lv/dati/lv/dataset/varis-atvertie-dati";

/// The columns `tests/registry/varis-csv.sql` writes.
const COLUMNS: [&str; 18] = [
    "KODS",
    "NOVADS",
    "NOVADS_PILNS",
    "PAGASTS",
    "PAGASTS_PILNS",
    "PILSETA",
    "CIEMS",
    "VIETA_CD",
    "IELA",
    "NOSAUKUMS",
    "ATRIB",
    "FOR_BUILD",
    "DAT_SAK",
    "STD",
    "KOORD_X",
    "KOORD_Y",
    "DD_N",
    "DD_E",
];

#[derive(Clone, Debug)]
struct Address {
    id: String,
    /// The municipality ("Tukuma nov.", "Tukuma novads"); empty in a state city.
    municipality: String,
    municipality_full: String,
    /// The parish ("Engures pag.", "Engures pagasts"); empty in a town.
    parish: String,
    parish_full: String,
    /// The town (pilsēta) or village (ciems) the address names, if any.
    settlement: String,
    street: String,
    /// The number and letter on a street, otherwise the house's name.
    number: String,
    postcode: String,
    /// The register's full address ("Akmeņsalas iela 8, Engure, Engures pag., Tukuma nov., LV-3113").
    full: String,
    point: [f64; 2],
    record: Value,
}

impl Address {
    fn class(&self) -> &'static str {
        if self.street.is_empty() {
            "name"
        } else if self.number.ends_with(|c: char| c.is_alphabetic()) {
            "letter"
        } else {
            "street"
        }
    }

    /// The municipality, or the state city outside every municipality.
    fn stratum(&self) -> &str {
        if self.municipality.is_empty() {
            &self.settlement
        } else {
            &self.municipality
        }
    }

    /// The register's address with types written out, without postcode or the name's quotes:
    /// "Akmeņsalas iela 8, Engure, Engures pagasts, Tukuma novads", "Riņņi, Vecates pagasts,
    /// Valmieras novads", "Matīsa iela 58A, Rīga".
    fn spelt(&self) -> String {
        let house = if self.street.is_empty() {
            self.number.clone()
        } else {
            format!("{} {}", self.street, self.number)
        };
        [
            house.as_str(),
            &self.settlement,
            &self.parish_full,
            &self.municipality_full,
            "Latvia",
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
    }

    fn places(&self) -> Vec<String> {
        super::places([
            self.settlement.as_str(),
            &self.parish,
            &self.parish_full,
            &self.municipality,
            &self.municipality_full,
        ])
    }
}

/// A street number: digits without leading zero and at most one capital letter ("12", "12A").
fn is_number(number: &str) -> bool {
    let digits = number.trim_end_matches(|c: char| c.is_uppercase());
    number.starts_with(|c: char| ('1'..='9').contains(&c))
        && digits.bytes().all(|b| b.is_ascii_digit())
        && number[digits.len()..].chars().count() <= 1
}

/// A house name without digits or quotes ("Riņņi", "Naukšēnu muiža").
fn is_name(name: &str) -> bool {
    name.chars().any(char::is_alphabetic)
        && name.trim() == name
        && !name.chars().any(|c| c.is_ascii_digit() || c == '"')
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty VZD address CSV"
    );
    ensure!(
        line.trim_end().split(',').eq(COLUMNS),
        "{} is not the VZD address extract tests/registry/varis-csv.sql writes",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "VZD address CSV row {n}: column count"
        );
        let field = |name: &str| fields[COLUMNS.iter().position(|c| *c == name).unwrap()].as_ref();
        let coordinate = |name: &str| {
            field(name)
                .parse::<f64>()
                .context(format!("VZD address {name}"))
        };
        let (lat, lon) = (coordinate("DD_N")?, coordinate("DD_E")?);
        ensure!(
            (55.6..58.1).contains(&lat) && (20.9..28.3).contains(&lon),
            "VZD address CSV row {n}: needs WGS84 latitude DD_N, longitude DD_E"
        );
        let (id, street, number, postcode) = (
            field("KODS"),
            field("IELA"),
            field("NOSAUKUMS"),
            field("ATRIB"),
        );
        let settlement = [field("CIEMS"), field("PILSETA")].concat();
        let house = if street.is_empty() {
            format!("\"{number}\"")
        } else {
            format!("{street} {number}")
        };
        let full = [
            house.as_str(),
            &settlement,
            field("PAGASTS"),
            field("NOVADS"),
            postcode,
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
        ensure!(
            full == field("STD"),
            "VZD address CSV row {n}: STD is not its components"
        );
        if id.is_empty()
            || !id.bytes().all(|b| b.is_ascii_digit())
            || !(if street.is_empty() {
                is_name(number)
            } else {
                is_number(number)
            })
            || postcode.len() != 7
            || !postcode.starts_with("LV-")
            || !postcode[3..].bytes().all(|b| b.is_ascii_digit())
            || field("NOVADS").is_empty() && field("PILSETA").is_empty()
        {
            continue;
        }
        eligible += 1;
        visit(Address {
            id: id.into(),
            municipality: field("NOVADS").into(),
            municipality_full: field("NOVADS_PILNS").into(),
            parish: field("PAGASTS").into(),
            parish_full: field("PAGASTS_PILNS").into(),
            settlement,
            street: street.into(),
            number: number.into(),
            postcode: postcode.into(),
            full,
            point: [lat, lon],
            record: json!(
                COLUMNS
                    .iter()
                    .map(|c| (c.to_string(), json!(field(c))))
                    .collect::<serde_json::Map<_, _>>()
            ),
        })?;
    }
    eprintln!("VZD addresses: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate KODS {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "VZD address CSV lacks requested KODS (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("VZD address file name")?;
    let mut picks: BTreeMap<(String, &'static str), (String, Address)> = BTreeMap::new();
    for_each(path, |a| {
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        let key = (a.stratum().to_string(), a.class());
        if picks.get(&key).is_none_or(|(best, _)| hash < *best) {
            picks.insert(key, (hash, a));
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((municipality, class), (_, a)) in picks {
        let query = format!("{}, Latvia", a.full);
        let base = json!({"kind":"address", "source":"varis", "ref":a.id, "cohort":format!("lv_{class}"), "queries":[query], "label":query, "tags":["registry-sample", "lv", "varis", release, format!("municipality:{municipality}"), class]});
        let mut spelt = base.clone();
        spelt["cohort"] = json!("lv_spelt");
        spelt["queries"] = json!([a.spelt()]);
        spelt["tags"]
            .as_array_mut()
            .unwrap()
            .push(json!("spelt_form"));
        lines.extend([base, spelt]);
    }
    eprintln!("VZD addresses: {} locations", lines.len() / 2);
    ensure!(!lines.is_empty(), "no eligible VZD addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "varis",
    command: "sample-lv",
    country: "lv",
    input: "varis",
    license: "CC-BY-4.0",
    location: "varis",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("DD_N, DD_E: WGS84 latitude, longitude of the address point as the register publishes them (AW_EKA.CSV), with KOORD_X, KOORD_Y in LKS-2020 TM (EPSG:10306)"),
    );
    metadata.insert(
        "attribution".into(),
        json!("Valsts zemes dienests, Valsts adrešu reģistrs; data date in source.release"),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(id, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), a.record.clone());
            let mut components = indexmap::IndexMap::from([
                ("house_number", a.number.clone()),
                ("postcode", a.postcode.clone()),
            ]);
            if !a.street.is_empty() {
                components.insert("road", a.street.clone());
            }
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(a.point.map(|c| (c * 1e6).round() / 1e6)),
                    components,
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
        let path = std::env::temp_dir().join(format!("varis-test-{}.csv", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let rows = [
            "1,Tukuma nov.,Tukuma novads,Engures pag.,Engures pagasts,\"\",Engure,9,Akmeņsalas iela,8,LV-3113,N,2009.01.01,\"Akmeņsalas iela 8, Engure, Engures pag., Tukuma nov., LV-3113\",340000,480000,57.15,23.21\n",
            "2,Tukuma nov.,Tukuma novads,\"\",\"\",Tukums,\"\",8,Rīgas iela,20A,LV-3101,N,2009.01.01,\"Rīgas iela 20A, Tukums, Tukuma nov., LV-3101\",310000,460000,56.97,23.15\n",
            "3,Valmieras nov.,Valmieras novads,Vecates pag.,Vecates pagasts,\"\",\"\",\"\",\"\",Riņņi,LV-4211,N,1999.03.19,\"\"\"Riņņi\"\", Vecates pag., Valmieras nov., LV-4211\",403627.035,568822.131,57.769418,25.156928\n",
            "4,\"\",\"\",\"\",\"\",Rīga,\"\",7,Matīsa iela,58,LV-1009,N,1999.03.19,\"Matīsa iela 58, Rīga, LV-1009\",312000,508000,56.958,24.135\n",
            "5,\"\",\"\",\"\",\"\",Rīga,\"\",7,Matīsa iela,58 k-2,LV-1009,N,1999.03.19,\"Matīsa iela 58 k-2, Rīga, LV-1009\",312000,508000,56.958,24.135\n",
            "6,Tukuma nov.,Tukuma novads,Sēmes pag.,Sēmes pagasts,\"\",\"\",\"\",\"\",Silmači 1,LV-3110,N,1999.03.19,\"\"\"Silmači 1\"\", Sēmes pag., Tukuma nov., LV-3110\",320000,470000,57.0,23.3\n",
            "7,Tukuma nov.,Tukuma novads,\"\",\"\",Tukums,\"\",8,Rīgas iela,05,LV-3101,N,2009.01.01,\"Rīgas iela 05, Tukums, Tukuma nov., LV-3101\",310000,460000,56.97,23.15\n",
            "8,Tukuma nov.,Tukuma novads,\"\",\"\",Tukums,\"\",8,Rīgas iela,22,\"\",N,2009.01.01,\"Rīgas iela 22, Tukums, Tukuma nov.\",310000,460000,56.97,23.15\n",
        ];
        let write =
            |rows: &[&str]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        let queries: BTreeSet<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            BTreeSet::from([
                "Akmeņsalas iela 8, Engure, Engures pag., Tukuma nov., LV-3113, Latvia",
                "Akmeņsalas iela 8, Engure, Engures pagasts, Tukuma novads, Latvia",
                "Rīgas iela 20A, Tukums, Tukuma nov., LV-3101, Latvia",
                "Rīgas iela 20A, Tukums, Tukuma novads, Latvia",
                "\"Riņņi\", Vecates pag., Valmieras nov., LV-4211, Latvia",
                "Riņņi, Vecates pagasts, Valmieras novads, Latvia",
                "Matīsa iela 58, Rīga, LV-1009, Latvia",
                "Matīsa iela 58, Rīga, Latvia",
            ])
        );
        let mut reversed = rows.to_vec();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["1".into(), "3".into()])).unwrap();
        assert_eq!(
            found["1"].places(),
            [
                "Engure",
                "Engures pag.",
                "Engures pagasts",
                "Tukuma nov.",
                "Tukuma novads"
            ]
        );
        assert_eq!(found["3"].class(), "name");
        for id in ["5", "6", "7", "8"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        write(&[&rows[0].replace("57.15,23.21", "23.21,57.15")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("WGS84")
        );
        write(&[&rows[0].replace("Engures pag., Tukuma", "Tukuma")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("STD")
        );
        std::fs::remove_file(path).unwrap();
    }
}
