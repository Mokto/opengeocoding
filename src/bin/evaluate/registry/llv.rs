use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

pub const URL: &str = "https://service.geo.llv.li/download/getfile.php?theme=officialAdress";

const COLUMNS: [&str; 20] = [
    "ADR_EGAID",
    "STR_ESID",
    "BDG_EGID",
    "ADR_EDID",
    "STN_NAME",
    "ADR_NUMBER",
    "COM_FOSNR",
    "ZIP_ZIP4",
    "ZIP_ZIPA",
    "ZIP_NAME",
    "STATUS",
    "OFFICIAL",
    "RELIABLE",
    "EAST",
    "NORTH",
    "WGS84_N",
    "WGS84_E",
    "MODIFIED",
    "AV_STRNR",
    "FL_GEID",
];

/// The municipality numbers (as in swisstopo's locality directory) of Liechtenstein's eleven
/// municipalities, their names and ISO 3166-2 codes.
const MUNICIPALITIES: [(&str, &str, &str); 11] = [
    ("7001", "Vaduz", "LI-11"),
    ("7002", "Triesen", "LI-09"),
    ("7003", "Balzers", "LI-01"),
    ("7004", "Triesenberg", "LI-10"),
    ("7005", "Schaan", "LI-07"),
    ("7006", "Planken", "LI-05"),
    ("7007", "Eschen", "LI-02"),
    ("7008", "Mauren", "LI-04"),
    ("7009", "Gamprin", "LI-03"),
    ("7010", "Ruggell", "LI-06"),
    ("7011", "Schellenberg", "LI-08"),
];

/// Locations drawn per municipality: plain numbers and numbers with a letter.
const PER_CLASS: [(&str, usize); 2] = [("street", 8), ("letter", 4)];

#[derive(Clone, Debug)]
struct Address {
    /// ADR_EGAID, the federal building-address identifier.
    id: String,
    street: String,
    number: String,
    postcode: String,
    /// ZIP_NAME, the postal locality ("Nendeln" for a house in Eschen).
    town: String,
    municipality: &'static str,
    iso: &'static str,
    point: [f64; 2],
    record: Value,
}

impl Address {
    fn class(&self) -> &'static str {
        if self.number.ends_with(|c: char| c.is_ascii_alphabetic()) {
            "letter"
        } else {
            "street"
        }
    }

    fn query(&self) -> String {
        format!(
            "{} {}, {} {}, Liechtenstein",
            self.street, self.number, self.postcode, self.town
        )
    }
}

/// "12", "42b": one to four digits without a leading zero and at most one lowercase letter.
fn is_number(number: &str) -> bool {
    let digits = number.trim_end_matches(|c: char| c.is_ascii_lowercase());
    number.len() - digits.len() <= 1
        && (1..=4).contains(&digits.len())
        && !digits.starts_with('0')
        && digits.bytes().all(|b| b.is_ascii_digit())
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let bytes = std::fs::read(path)?;
    // The file is ISO-8859-1 (its shapefile's .cpg says 1252); C1 bytes would decode differently.
    ensure!(
        !bytes.iter().any(|b| (0x80..0xa0).contains(b)),
        "{}: bytes 0x80-0x9F, not ISO-8859-1",
        path.display()
    );
    let text: String = bytes.iter().map(|&b| b as char).collect();
    let mut lines = text.lines();
    ensure!(
        lines.next().is_some_and(|h| h.split(';').eq(COLUMNS)),
        "{} is not the Liechtenstein address directory's dataset.csv",
        path.display()
    );
    let (mut n, mut candidates, mut per_text) = (0usize, vec![], HashMap::new());
    for line in lines {
        n += 1;
        let fields = crate::registry::nad::split(line, ';');
        ensure!(
            fields.len() == COLUMNS.len(),
            "Liechtenstein address CSV row {n}: column count"
        );
        let field = |name: &str| fields[COLUMNS.iter().position(|c| *c == name).unwrap()].trim();
        let coordinate = |name: &str| {
            field(name)
                .parse::<f64>()
                .context(format!("Liechtenstein address {name}"))
        };
        let (lat, lon) = (coordinate("WGS84_N")?, coordinate("WGS84_E")?);
        ensure!(
            (47.0..47.3).contains(&lat) && (9.4..9.7).contains(&lon),
            "Liechtenstein address CSV row {n}: needs WGS84 latitude WGS84_N, longitude WGS84_E"
        );
        let &(_, municipality, iso) = MUNICIPALITIES
            .iter()
            .find(|(code, _, _)| *code == field("COM_FOSNR"))
            .with_context(|| format!("unknown municipality {}", field("COM_FOSNR")))?;
        let (id, street, number) = (field("ADR_EGAID"), field("STN_NAME"), field("ADR_NUMBER"));
        let (postcode, town) = (field("ZIP_ZIP4"), field("ZIP_NAME"));
        if [field("STATUS"), field("OFFICIAL"), field("RELIABLE")] != ["real", "TRUE", "TRUE"]
            || !id.parse::<u64>().is_ok_and(|n| n > 0)
            || street.is_empty()
            || street.contains(',')
            || !is_number(number)
            || postcode.len() != 4
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || town.is_empty()
        {
            continue;
        }
        *per_text
            .entry((street.to_string(), number.to_string(), postcode.to_string()))
            .or_insert(0) += 1;
        candidates.push(Address {
            id: id.into(),
            street: street.into(),
            number: number.into(),
            postcode: postcode.into(),
            town: town.into(),
            municipality,
            iso,
            point: [lat, lon],
            record: json!(
                COLUMNS
                    .iter()
                    .map(|c| (c.to_string(), json!(field(c))))
                    .collect::<serde_json::Map<_, _>>()
            ),
        });
    }
    let mut eligible = 0usize;
    for a in candidates {
        // no other address written the same way in its postcode
        if per_text[&(a.street.clone(), a.number.clone(), a.postcode.clone())] == 1 {
            eligible += 1;
            visit(a)?;
        }
    }
    eprintln!("Liechtenstein addresses: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate ADR_EGAID {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "Liechtenstein address CSV lacks requested ADR_EGAID (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("Liechtenstein address file name")?;
    let mut pools: BTreeMap<(&str, &str), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        pools.entry((a.iso, a.class())).or_default().insert(hash, a);
        Ok(())
    })?;
    let mut lines = vec![];
    for ((_, class), pool) in pools {
        let take = PER_CLASS.iter().find(|(c, _)| *c == class).unwrap().1;
        let mut streets = BTreeSet::new();
        for a in pool
            .values()
            .filter(|a| streets.insert(a.street.clone()))
            .take(take)
        {
            let query = a.query();
            lines.push(json!({"kind":"address", "source":"llv", "ref":a.id, "cohort":format!("li_{class}"), "queries":[query], "label":query, "tags":["registry-sample", "li", "llv", release, format!("municipality:{}", a.municipality), class], "extra_regions":[a.iso]}));
        }
    }
    eprintln!("Liechtenstein: {} locations", lines.len());
    ensure!(!lines.is_empty(), "no eligible Liechtenstein addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "llv",
    command: "sample-li",
    country: "li",
    input: "llv",
    license: "LLV-ABI-OGD-2",
    location: "llv",
    url: |id| format!("{URL}#{id}"),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("WGS84_N, WGS84_E: WGS84 latitude, longitude of the address point as the directory publishes them, with EAST, NORTH in LV95 (EPSG:2056)"),
    );
    metadata.insert(
        "attribution".into(),
        json!("Liechtensteinische Landesverwaltung (Offizielles Verzeichnis der Gebäudeadressen); terms: Nutzungsbedingungen ABI/OGD-Version 2 vom 5.2.2021, tests/registry/li.NOTICE; data date in source.release"),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(id, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), a.record.clone());
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(a.point),
                    components: indexmap::IndexMap::from([
                        ("house_number", a.number.clone()),
                        ("road", a.street.clone()),
                        ("postcode", a.postcode.clone()),
                    ]),
                    radius_floor_km: 0.,
                    provenance,
                    number_aliases: None,
                    places: super::places([a.town.as_str(), a.municipality]),
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
        let path = std::env::temp_dir().join(format!("llv-test-{}.csv", std::process::id()));
        let header = format!("{}\r\n", COLUMNS.join(";"));
        let row = |id: &str, street: &str, number: &str, municipality: &str, status: &str| {
            format!(
                "{id};1;1;1;{street};{number};{municipality};9485;0;Nendeln;{status};TRUE;TRUE;2759174.1;1229001.8;47.19279;9.539265;2019-02-21T00:00:00;1;1\r\n"
            )
        };
        let rows = [
            row("1", "Churer Strasse", "102", "7007", "real"),
            row("2", "Oberwiesen", "42b", "7007", "real"),
            row("3", "Oberwiesen", "9", "7007", "planned"),
            row("4", "Oberwiesen", "09", "7007", "real"),
            row("5", "Oberwiesen", "27B1", "7007", "real"),
            row("6", "Feld", "3", "7007", "real"),
            row("7", "Feld", "3", "7007", "real"),
            row("8", "Äule", "1", "7003", "real"),
        ];
        let write = |rows: &[String]| {
            let text = format!("{header}{}", rows.concat());
            std::fs::write(&path, text.chars().map(|c| c as u8).collect::<Vec<_>>()).unwrap()
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
                "Churer Strasse 102, 9485 Nendeln, Liechtenstein",
                "Oberwiesen 42b, 9485 Nendeln, Liechtenstein",
                "Äule 1, 9485 Nendeln, Liechtenstein",
            ])
        );
        let regions =
            |q: &str| first.iter().find(|s| s["queries"][0] == q).unwrap()["extra_regions"].clone();
        assert_eq!(
            regions("Äule 1, 9485 Nendeln, Liechtenstein"),
            json!(["LI-01"])
        );
        let mut reversed = rows.to_vec();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["2".into()])).unwrap();
        assert_eq!(found["2"].class(), "letter");
        assert_eq!(found["2"].municipality, "Eschen");
        // planned, a leading zero, other numbering, and two addresses written alike
        for id in ["3", "4", "5", "6"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        write(&[rows[0].replace("47.19279;9.539265", "9.539265;47.19279")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("WGS84")
        );
        write(&[rows[0].replace(";7007;", ";7012;")]);
        assert!(sample(&path, "test").is_err());
        std::fs::write(&path, [header.as_bytes(), &[0x80]].concat()).unwrap();
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
