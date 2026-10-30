use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::File,
    io::BufReader,
    path::Path,
};

pub const URL: &str = "https://hms.is/stadfangaskra";

/// The columns `tests/registry/stadfangaskra-csv.sql` writes.
const COLUMNS: [&str; 20] = [
    "HNITNUM",
    "HEINUM",
    "LANDNR",
    "SVFNR",
    "SVEITARFELAG",
    "POSTNR",
    "STADUR",
    "HEITI_NF",
    "HEITI_TGF",
    "HUSNR",
    "BOKST",
    "VIDSK",
    "SERHEITI",
    "TEGHNIT",
    "YFIRFARID",
    "DAGS_LEIDR",
    "VEF_BIRTING",
    "HNIT",
    "N_HNIT_WGS84",
    "E_HNIT_WGS84",
];

#[derive(Clone, Debug)]
struct Address {
    /// HNITNUM, the address point's number (Overture's record id too).
    id: String,
    municipality_code: String,
    /// IS 50V's name of the municipality ("Kópavogsbær").
    municipality: String,
    postcode: String,
    /// Byggðastofnun's place of the postcode ("Kópavogur").
    town: String,
    /// The street, or the farm or place a numberless address names, in the nominative.
    street: String,
    street_dative: String,
    number: String,
    letter: String,
    point: [f64; 2],
    record: Value,
}

impl Address {
    fn class(&self) -> &'static str {
        if self.number.is_empty() {
            "name"
        } else if self.letter.is_empty() {
            "street"
        } else {
            "letter"
        }
    }

    /// "Bæjarlind 8, 201 Kópavogur, Iceland", or with the street in the dative, as Icelanders
    /// write it on letters ("Borgartúni 8A, 105 Reykjavík, Iceland").
    fn query(&self, street: &str) -> String {
        let house = format!("{street} {}{}", self.number, self.letter);
        format!(
            "{}, {} {}, Iceland",
            house.trim_end(),
            self.postcode,
            self.town
        )
    }

    fn places(&self) -> Vec<String> {
        super::places([self.town.as_str(), &self.municipality])
    }
}

/// Words of letters joined by single spaces or hyphens ("Bæjarlind", "Efri-Brú", "Litla Hof").
fn is_name(name: &str) -> bool {
    name.split([' ', '-'])
        .all(|w| !w.is_empty() && w.chars().all(char::is_alphabetic))
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty Staðfangaskrá CSV"
    );
    ensure!(
        line.trim_end().split(',').eq(COLUMNS),
        "{} is not the Staðfangaskrá extract tests/registry/stadfangaskra-csv.sql writes",
        path.display()
    );
    let (mut n, mut candidates) = (0usize, vec![]);
    let (mut per_address, mut per_text) = (HashMap::new(), HashMap::new());
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "Staðfangaskrá CSV row {n}: column count"
        );
        let field = |name: &str| fields[COLUMNS.iter().position(|c| *c == name).unwrap()].as_ref();
        let coordinate = |name: &str| {
            field(name)
                .parse::<f64>()
                .context(format!("Staðfangaskrá {name}"))
        };
        let (lat, lon) = (coordinate("N_HNIT_WGS84")?, coordinate("E_HNIT_WGS84")?);
        ensure!(
            (63.0..66.7).contains(&lat) && (-24.7..-13.3).contains(&lon),
            "Staðfangaskrá CSV row {n}: needs WGS84 latitude N_HNIT_WGS84, longitude E_HNIT_WGS84"
        );
        let (street, number, letter, suffix) = (
            field("HEITI_NF"),
            field("HUSNR"),
            field("BOKST"),
            field("VIDSK"),
        );
        let shown = format!("{street} {number}{letter} {suffix} ({})", field("LANDNR"));
        ensure!(
            shown.trim_start() == field("VEF_BIRTING"),
            "Staðfangaskrá CSV row {n}: VEF_BIRTING is not its components"
        );
        *per_address.entry(field("HEINUM").to_string()).or_insert(0) += 1;
        let text = (
            street.to_string(),
            format!("{number}{letter}"),
            field("POSTNR").to_string(),
        );
        *per_text.entry(text).or_insert(0) += 1;
        let (id, postcode, town) = (field("HNITNUM"), field("POSTNR"), field("STADUR"));
        if id.is_empty()
            || !id.bytes().all(|b| b.is_ascii_digit())
            || !is_name(street)
            || !is_name(field("HEITI_TGF"))
            // ponytail: a numberless address of several words is more often a lot, a car park or
            // a road reserve ("Land Bæjarins", "Bílastæði yfir Fífuhvammsbraut") than a farm;
            // telling them apart needs the property register's land use.
            || number.is_empty() && street.contains(' ')
            || !suffix.is_empty()
            || !(number.is_empty()
                || number.starts_with(|c: char| ('1'..='9').contains(&c))
                    && number.bytes().all(|b| b.is_ascii_digit()))
            || !(letter.is_empty()
                || !number.is_empty()
                    && letter.chars().count() == 1
                    && letter.chars().all(char::is_alphabetic))
            || postcode.len() != 3
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || town.is_empty()
            || matches!(field("YFIRFARID"), "2" | "9")
        {
            continue;
        }
        candidates.push(Address {
            id: id.into(),
            municipality_code: field("SVFNR").into(),
            municipality: field("SVEITARFELAG").into(),
            postcode: postcode.into(),
            town: town.into(),
            street: street.into(),
            street_dative: field("HEITI_TGF").into(),
            number: number.into(),
            letter: letter.into(),
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
        let heinum = a.record["HEINUM"].as_str().unwrap_or_default();
        let text = (
            a.street.clone(),
            format!("{}{}", a.number, a.letter),
            a.postcode.clone(),
        );
        // one point per address, and no other address written the same way in its postcode
        if per_address[heinum] == 1 && per_text[&text] == 1 {
            eligible += 1;
            visit(a)?;
        }
    }
    eprintln!("Staðfangaskrá addresses: {n} rows, {eligible} eligible");
    Ok(())
}

fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate HNITNUM {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "Staðfangaskrá CSV lacks requested HNITNUM (or they are no longer eligible)"
    );
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("Staðfangaskrá file name")?;
    let mut picks: BTreeMap<(String, &'static str), (String, Address)> = BTreeMap::new();
    for_each(path, |a| {
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        let key = (a.municipality_code.clone(), a.class());
        if picks.get(&key).is_none_or(|(best, _)| hash < *best) {
            picks.insert(key, (hash, a));
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((_, class), (_, a)) in picks {
        let query = a.query(&a.street);
        let mut base = json!({"kind":"address", "source":"stadfangaskra", "ref":a.id, "cohort":format!("is_{class}"), "queries":[query], "label":query, "tags":["registry-sample", "is", "stadfangaskra", release, format!("municipality:{}", a.municipality), class]});
        // a farm or place without a number is the street its name gives, not a house of it
        if class == "name" {
            base["level"] = json!("street");
        }
        if a.street_dative != a.street {
            let mut dative = base.clone();
            dative["cohort"] = json!("is_dative");
            dative["queries"] = json!([a.query(&a.street_dative)]);
            dative["tags"]
                .as_array_mut()
                .unwrap()
                .push(json!("dative_form"));
            lines.push(base);
            lines.push(dative);
        } else {
            lines.push(base);
        }
    }
    eprintln!("Staðfangaskrá: {} queries", lines.len());
    ensure!(!lines.is_empty(), "no eligible Staðfangaskrá addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "stadfangaskra",
    command: "sample-is",
    country: "is",
    input: "stadfangaskra",
    license: "IS-Endurnot-Opinberra-Upplysinga AND Byggdastofnun-Postnumer AND CC-BY-4.0",
    location: "stadfangaskra",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("dataset".into(), json!(URL));
    metadata.insert(
        "coordinates".into(),
        json!("N_HNIT_WGS84, E_HNIT_WGS84: WGS84 latitude, longitude of the address point as the register publishes them, with HNIT in ISN93 (EPSG:3057)"),
    );
    metadata.insert(
        "attribution".into(),
        json!("Byggir á upplýsingum frá HMS (Staðfangaskrá); post towns: Byggt á gögnum frá Byggðastofnun (Póstnúmeraskrá); municipalities: Náttúrufræðistofnun, IS 50V (CC BY 4.0); data date in source.release"),
    );
    lookup(path, refs)?
        .into_iter()
        .map(|(id, a)| {
            let mut provenance = metadata.clone();
            provenance.insert("record".into(), a.record.clone());
            let mut components = indexmap::IndexMap::from([("postcode", a.postcode.clone())]);
            if !a.number.is_empty() {
                components.insert("house_number", format!("{}{}", a.number, a.letter));
                components.insert("road", a.street.clone());
            }
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(a.point),
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
        let path =
            std::env::temp_dir().join(format!("stadfangaskra-test-{}.csv", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let row = |id: &str,
                   heinum: &str,
                   street: &str,
                   dative: &str,
                   number: &str,
                   letter: &str,
                   suffix: &str,
                   postcode: &str| {
            let shown = format!("{street} {number}{letter} {suffix} (100)");
            format!(
                "{id},{heinum},100,1000,Kópavogsbær,{postcode},Kópavogur,{street},{dative},{number},{letter},{suffix},\"\",0,0,2020-01-01,{},POINT (1 2),64.1,-21.9\n",
                shown.trim_start()
            )
        };
        let rows = [
            row("1", "11", "Bæjarlind", "Bæjarlind", "8", "", "", "201"),
            row("2", "12", "Borgartún", "Borgartúni", "8", "A", "", "201"),
            row("3", "13", "Höfði", "Höfða", "", "", "", "201"),
            row("4", "14", "Höfði", "Höfða", "", "", "frístundalóð", "202"),
            row(
                "5",
                "15",
                "Land Bæjarins",
                "Landi Bæjarins",
                "",
                "",
                "",
                "201",
            ),
            row("6", "16", "Lind", "Lind", "07", "", "", "201"),
            row("7", "17", "Lind", "Lind", "8", "", "", "201"),
            row("8", "17", "Lind", "Lind", "9", "", "", "201"),
            row("9", "19", "Sel", "Seli", "", "", "", "201"),
            row("10", "20", "Sel", "Seli", "", "", "", "201"),
            row(
                "11",
                "21",
                "Naustabr.12-22 BL",
                "Naustabr.12-22 BL",
                "",
                "",
                "",
                "201",
            ),
        ];
        let write =
            |rows: &[String]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        let queries: BTreeSet<_> = first
            .iter()
            .map(|s| s["queries"][0].as_str().unwrap())
            .collect();
        assert_eq!(
            queries,
            BTreeSet::from([
                "Bæjarlind 8, 201 Kópavogur, Iceland",
                "Borgartún 8A, 201 Kópavogur, Iceland",
                "Borgartúni 8A, 201 Kópavogur, Iceland",
                "Höfði, 201 Kópavogur, Iceland",
                "Höfða, 201 Kópavogur, Iceland",
            ])
        );
        assert!(first.iter().all(|s| {
            s["tags"].as_array().unwrap().contains(&json!("name")) == (s["level"] == "street")
        }));
        let mut reversed = rows.to_vec();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = lookup(&path, &BTreeSet::from(["2".into(), "3".into()])).unwrap();
        assert_eq!(found["2"].places(), ["Kópavogur", "Kópavogsbær"]);
        assert_eq!(found["3"].class(), "name");
        // a lot, a car park or road reserve, a leading zero, two points of one address, two
        // addresses written alike, and a name that is not one
        for id in ["4", "5", "6", "7", "9", "11"] {
            assert!(lookup(&path, &BTreeSet::from([id.into()])).is_err(), "{id}");
        }
        write(&[rows[0].replace("64.1,-21.9", "-21.9,64.1")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("WGS84")
        );
        write(&[rows[0].replace("Bæjarlind 8  (100)", "Bæjarlind 9  (100)")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("VEF_BIRTING")
        );
        std::fs::remove_file(path).unwrap();
    }
}
