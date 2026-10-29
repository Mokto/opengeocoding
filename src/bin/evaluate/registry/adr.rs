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

pub const URL: &str = "https://www.registrucentras.lt/atviri-duomenys-ir-statistika/adresu-registro-pirminiai-duomenys-raw-data";

/// The columns `tests/registry/adr-csv.sql` writes.
const COLUMNS: [&str; 18] = [
    "AOB_KODAS",
    "ADM_VARDAS_K",
    "SAV_KODAS",
    "SAV_VARDAS_K",
    "GYV_KODAS",
    "GYV_TIPAS",
    "GYV_SANTRUMPA",
    "GYV_VARDAS_K",
    "GYV_VARDAS",
    "GAT_KODAS",
    "GAT_SANTRUMPA",
    "GAT_VARDAS_K",
    "NR",
    "KORPUSO_NR",
    "PASTO_KODAS",
    "AOB_NUO",
    "N_KOORD",
    "E_KOORD",
];

/// Locations per county and class, each from a different settlement.
const TAKE: [(&str, usize); 3] = [("street", 4), ("letter", 3), ("village", 3)];

#[derive(Clone, Debug)]
struct Address {
    id: String,
    county: String,
    /// The municipality's genitive name as the register gives it ("Zarasų rajono").
    municipality: String,
    settlement_code: String,
    settlement_type: String,
    /// The settlement's genitive name and type abbreviation ("Vaškių", "k.").
    settlement_gen: String,
    settlement_abbr: String,
    /// The settlement's nominative name ("Vaškiai").
    settlement: String,
    /// The street's genitive name and type abbreviation ("Taikos", "g."); empty where the
    /// settlement has no streets and numbers its houses itself.
    street: String,
    street_abbr: String,
    number: String,
    postcode: String,
    point: [f64; 2],
    record: Value,
}

impl Address {
    fn class(&self) -> &'static str {
        if self.number.ends_with(|c: char| c.is_alphabetic()) {
            "letter"
        } else if self.street.is_empty() {
            "village"
        } else {
            "street"
        }
    }

    fn road(&self) -> String {
        format!("{} {}", self.street, self.street_abbr)
    }

    /// "Taikos g. 4", or "Vaškių k. 12" in a settlement without streets.
    fn house(&self) -> String {
        if self.street.is_empty() {
            format!("{} {}", self.settlement_full(), self.number)
        } else {
            format!("{} {}", self.road(), self.number)
        }
    }

    /// "Vaškių k.", "Dusetų m.".
    fn settlement_full(&self) -> String {
        format!("{} {}", self.settlement_gen, self.settlement_abbr)
    }

    /// "Zarasų r. sav.", "Kauno m. sav.", "Birštono sav.".
    fn municipality_short(&self) -> String {
        let name = self.municipality.as_str();
        match (name.strip_suffix(" rajono"), name.strip_suffix(" miesto")) {
            (Some(stem), _) => format!("{stem} r. sav."),
            (_, Some(stem)) => format!("{stem} m. sav."),
            _ => format!("{name} sav."),
        }
    }

    /// The municipality's centre, a town written alone before the postcode.
    fn is_centre(&self) -> bool {
        let stem = self
            .municipality
            .strip_suffix(" rajono")
            .or_else(|| self.municipality.strip_suffix(" miesto"))
            .unwrap_or(&self.municipality);
        self.settlement_type == "miestas" && self.settlement_gen == stem
    }

    /// Lietuvos paštas's form: "Gėlių g. 12, LT-20115 Ukmergė" in a municipality's centre;
    /// elsewhere the settlement (a town in the nominative, a village with its type) and the
    /// municipality after the postcode: "Taikos g. 4, Dusetos, LT-32305 Zarasų r. sav.",
    /// "Vaškių k. 12, LT-96361 Šilutės r. sav.".
    fn postal(&self) -> String {
        if self.is_centre() {
            return format!(
                "{}, {} {}, Lithuania",
                self.house(),
                self.postcode,
                self.settlement
            );
        }
        let mut parts = vec![self.house()];
        if !self.street.is_empty() {
            parts.push(match self.settlement_type.as_str() {
                "miestas" | "miestelis" => self.settlement.clone(),
                _ => self.settlement_full(),
            });
        }
        parts.push(format!("{} {}", self.postcode, self.municipality_short()));
        parts.push("Lithuania".into());
        parts.join(", ")
    }

    /// The register's components in its order, municipality first and without postcode:
    /// "Zarasų r. sav., Dusetų m., Taikos g. 4", "Šilutės r. sav., Vaškių k. 12".
    fn register(&self) -> String {
        let mut parts = vec![self.municipality_short()];
        if !self.street.is_empty() {
            parts.push(self.settlement_full());
        }
        parts.push(self.house());
        parts.push("Lithuania".into());
        parts.join(", ")
    }

    fn places(&self) -> Vec<String> {
        super::places([
            self.settlement.as_str(),
            &self.settlement_full(),
            &format!("{} savivaldybė", self.municipality),
            &self.municipality_short(),
        ])
    }
}

/// A house number: digits without leading zero and at most one capital letter ("12", "12A").
fn is_number(number: &str) -> bool {
    let digits = number.trim_end_matches(|c: char| c.is_uppercase());
    number.starts_with(|c: char| ('1'..='9').contains(&c))
        && digits.bytes().all(|b| b.is_ascii_digit())
        && number[digits.len()..].chars().count() <= 1
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty Adresų registras CSV"
    );
    ensure!(
        line.trim_end().split(',').eq(COLUMNS),
        "{} is not the Adresų registras extract tests/registry/adr-csv.sql writes",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "Adresų registras CSV row {n}: column count"
        );
        let field = |name: &str| fields[COLUMNS.iter().position(|c| *c == name).unwrap()].trim();
        let coordinate = |name: &str| {
            field(name)
                .parse::<f64>()
                .context(format!("Adresų registras {name}"))
        };
        let (lat, lon) = (coordinate("N_KOORD")?, coordinate("E_KOORD")?);
        ensure!(
            (53.8..56.5).contains(&lat) && (20.9..26.9).contains(&lon),
            "Adresų registras CSV row {n}: needs WGS84 latitude N_KOORD, longitude E_KOORD"
        );
        let (id, number, postcode) = (field("AOB_KODAS"), field("NR"), field("PASTO_KODAS"));
        if id.is_empty()
            || !id.bytes().all(|b| b.is_ascii_digit())
            || !is_number(number)
            || !field("KORPUSO_NR").is_empty()
            || postcode.len() != 8
            || !postcode.starts_with("LT-")
            || !postcode[3..].bytes().all(|b| b.is_ascii_digit())
            || field("GAT_KODAS").is_empty() != field("GAT_VARDAS_K").is_empty()
            || [
                "ADM_VARDAS_K",
                "SAV_VARDAS_K",
                "GYV_TIPAS",
                "GYV_SANTRUMPA",
                "GYV_VARDAS_K",
                "GYV_VARDAS",
            ]
            .iter()
            .any(|c| field(c).is_empty())
        {
            continue;
        }
        eligible += 1;
        visit(Address {
            id: id.into(),
            county: field("ADM_VARDAS_K").into(),
            municipality: field("SAV_VARDAS_K").into(),
            settlement_code: field("GYV_KODAS").into(),
            settlement_type: field("GYV_TIPAS").into(),
            settlement_gen: field("GYV_VARDAS_K").into(),
            settlement_abbr: field("GYV_SANTRUMPA").into(),
            settlement: field("GYV_VARDAS").into(),
            street: field("GAT_VARDAS_K").into(),
            street_abbr: field("GAT_SANTRUMPA").into(),
            number: number.into(),
            postcode: postcode.into(),
            point: [lat, lon],
            record: json!(
                COLUMNS
                    .iter()
                    .map(|c| (c.to_string(), json!(field(c))))
                    .collect::<serde_json::Map<_, _>>()
            ),
        })?;
    }
    eprintln!("Adresų registras: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate AOB_KODAS {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "Adresų registras CSV lacks requested AOB_KODAS (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("Adresų registras file name")?;
    let mut pools: BTreeMap<(String, &'static str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let pool = pools.entry((a.county.clone(), a.class())).or_default();
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        // ponytail: settlement diversity draws on the 200 smallest hashes per county and class; enlarge the pool if it cannot supply TAKE settlements.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((county, class), pool) in pools {
        let take = TAKE.iter().find(|(c, _)| *c == class).unwrap().1;
        let mut settlements = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| settlements.insert(&a.settlement_code))
            .take(take)
            .collect();
        eprintln!(
            "Adresų registras {county} {class}: {} locations",
            selected.len()
        );
        for a in selected {
            let query = a.postal();
            let base = json!({"kind":"address", "source":"adr", "ref":a.id, "cohort":format!("lt_{class}"), "queries":[query], "label":query, "tags":["registry-sample", "lt", "adr", release, format!("county:{county} apskr."), format!("municipality:{}", a.municipality_short()), class]});
            let mut register = base.clone();
            register["cohort"] = json!("lt_register");
            register["queries"] = json!([a.register()]);
            register["tags"]
                .as_array_mut()
                .unwrap()
                .push(json!("register_form"));
            lines.extend([base, register]);
        }
    }
    ensure!(!lines.is_empty(), "no eligible Adresų registras addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "adr",
    command: "sample-lt",
    country: "lt",
    input: "adr",
    license: "CC-BY-4.0",
    location: "adr",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("N_KOORD, E_KOORD: WGS84 latitude, longitude of the address point as the register publishes them (adr_gra_adresai_LT.json)"),
    );
    metadata.insert(
        "attribution".into(),
        json!("VĮ Registrų centras, Adresų registras; data date in source.release"),
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
                components.insert("road", a.road());
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
        let path = std::env::temp_dir().join(format!("adr-test-{}.csv", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let row = |id: &str, municipality: &str, settlement: &str, street: &str, number: &str| {
            format!(
                "{id},Utenos,43,{municipality},{settlement},{street},{number},2005-02-08,55.75,26.25\n"
            )
        };
        let rows = [
            row(
                "1",
                "Zarasų rajono",
                "100,miestas,m.,Dusetų,Dusetos",
                "7,g.,Taikos",
                "4,\"\",LT-32305",
            ),
            row(
                "2",
                "Zarasų rajono",
                "101,miestas,m.,Zarasų,Zarasai",
                "8,pr.,Vytauto",
                "12A,\"\",LT-32100",
            ),
            row(
                "3",
                "Zarasų rajono",
                "102,kaimas,k.,Vaškių,Vaškiai",
                "\"\",\"\",\"\"",
                "12,\"\",LT-32361",
            ),
            row(
                "4",
                "Zarasų rajono",
                "103,kaimas,k.,Pelkelės,Pelkelė",
                "9,g.,Pavasario",
                "15,\"\",LT-32231",
            ),
            row(
                "5",
                "Kauno miesto",
                "104,miestas,m.,Kauno,Kaunas",
                "10,al.,Laisvės",
                "1,\"\",LT-44001",
            ),
            row(
                "6",
                "Birštono",
                "105,miestelis,mstl.,Nemajūnų,Nemajūnai",
                "11,g.,Kalno",
                "2,\"\",LT-59201",
            ),
            row(
                "7",
                "Kauno miesto",
                "104,miestas,m.,Kauno,Kaunas",
                "10,al.,Laisvės",
                "3,2,LT-44001",
            ),
            row(
                "8",
                "Kauno miesto",
                "104,miestas,m.,Kauno,Kaunas",
                "10,al.,Laisvės",
                "05,\"\",LT-44001",
            ),
            row(
                "9",
                "Kauno miesto",
                "104,miestas,m.,Kauno,Kaunas",
                "10,al.,Laisvės",
                "5AB,\"\",LT-44001",
            ),
            row(
                "10",
                "Kauno miesto",
                "104,miestas,m.,Kauno,Kaunas",
                "10,al.,Laisvės",
                "6,\"\",\"\"",
            ),
        ];
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
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
                "Taikos g. 4, Dusetos, LT-32305 Zarasų r. sav., Lithuania",
                "Zarasų r. sav., Dusetų m., Taikos g. 4, Lithuania",
                "Vytauto pr. 12A, LT-32100 Zarasai, Lithuania",
                "Zarasų r. sav., Zarasų m., Vytauto pr. 12A, Lithuania",
                "Vaškių k. 12, LT-32361 Zarasų r. sav., Lithuania",
                "Zarasų r. sav., Vaškių k. 12, Lithuania",
                "Pavasario g. 15, Pelkelės k., LT-32231 Zarasų r. sav., Lithuania",
                "Zarasų r. sav., Pelkelės k., Pavasario g. 15, Lithuania",
                "Laisvės al. 1, LT-44001 Kaunas, Lithuania",
                "Kauno m. sav., Kauno m., Laisvės al. 1, Lithuania",
                "Kalno g. 2, Nemajūnai, LT-59201 Birštono sav., Lithuania",
                "Birštono sav., Nemajūnų mstl., Kalno g. 2, Lithuania",
            ])
        );
        let mut reversed = rows.clone();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["2".into(), "3".into()])).unwrap();
        assert_eq!(
            found["3"].places(),
            [
                "Vaškiai",
                "Vaškių k.",
                "Zarasų rajono savivaldybė",
                "Zarasų r. sav."
            ]
        );
        assert_eq!(found["2"].class(), "letter");
        assert_eq!(found["3"].class(), "village");
        for id in ["7", "8", "9", "10"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        write(&[&rows[0].replace("55.75,26.25", "26.25,55.75")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("WGS84")
        );
        std::fs::remove_file(path).unwrap();
    }
}
