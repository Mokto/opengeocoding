use anyhow::{Context, Result, ensure};
use flate2::read::MultiGzDecoder;
use opengeocoding::eval::sha256_hex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

#[derive(Clone, Debug)]
pub struct Address {
    pub id: String,
    pub number: String,
    pub street: String,
    pub postcode: String,
    pub city: String,
    pub commune: String,
    pub point: [f64; 2],
    placement: String,
    certified: bool,
}

impl Address {
    fn department(&self) -> &str {
        &self.commune[..if self.commune.starts_with("97") { 3 } else { 2 }]
    }

    fn query(&self) -> String {
        format!(
            "{} {}, {} {}, France",
            self.number, self.street, self.postcode, self.city
        )
    }

    pub fn radius_km(&self) -> f64 {
        if self.placement == "parcelle" {
            0.3
        } else {
            0.15
        }
    }
}

pub(super) fn record(reader: &mut impl BufRead, line: &mut String) -> Result<bool> {
    line.clear();
    loop {
        let read = reader.read_line(line)?;
        ensure!(line.len() <= 1_048_576, "CSV record exceeds 1 MiB");
        let quoted = line.bytes().filter(|b| *b == b'"').count() % 2 == 1;
        ensure!(read != 0 || !quoted, "unterminated CSV quote");
        if !quoted {
            return Ok(!line.is_empty());
        }
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::new(MultiGzDecoder::new(File::open(path)?));
    let mut line = String::new();
    ensure!(record(&mut reader, &mut line)?, "empty BAN CSV");
    let columns: BTreeMap<String, usize> = crate::nad::split(line.trim_end(), ';')
        .iter()
        .enumerate()
        .map(|(i, s)| (s.to_string(), i))
        .collect();
    for name in [
        "id",
        "numero",
        "rep",
        "nom_voie",
        "code_postal",
        "code_insee",
        "nom_commune",
        "lat",
        "lon",
        "type_position",
        "certification_commune",
    ] {
        ensure!(columns.contains_key(name), "BAN CSV missing {name}");
    }
    let mut n = 0usize;
    while record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(5_000_000) {
            eprintln!("BAN: {n} rows read");
        }
        let fields = crate::nad::split(line.trim_end_matches(['\r', '\n']), ';');
        ensure!(
            fields.len() == columns.len(),
            "BAN CSV row {n}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        let commune = field("code_insee");
        let postcode = field("code_postal");
        let Ok(lat) = field("lat").parse::<f64>() else {
            continue;
        };
        let Ok(lon) = field("lon").parse::<f64>() else {
            continue;
        };
        if !(-90. ..=90.).contains(&lat)
            || !(-180. ..=180.).contains(&lon)
            || commune.len() != 5
            || !commune.is_ascii()
            || postcode.len() != 5
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || ["id", "numero", "nom_voie", "nom_commune"]
                .iter()
                .any(|name| field(name).is_empty())
        {
            continue;
        }
        let number = [field("numero"), field("rep")]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        visit(Address {
            id: field("id").into(),
            number,
            street: field("nom_voie").into(),
            postcode: postcode.into(),
            city: field("nom_commune").into(),
            commune: commune.into(),
            point: [lat, lon],
            placement: field("type_position").into(),
            certified: field("certification_commune") == "1",
        })?;
    }
    Ok(())
}

pub fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate BAN id {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "BAN CSV lacks {} requested ids",
        ids.len() - found.len()
    );
    Ok(found)
}

fn key(seed: &str, id: &str) -> String {
    sha256_hex(format!("{seed}\0{id}").as_bytes())
}

pub(super) fn typo(text: &str) -> Option<String> {
    let word = text
        .split_whitespace()
        .filter(|w| w.chars().all(char::is_alphabetic))
        .max_by_key(|w| w.chars().count())?;
    let mut chars: Vec<_> = word.chars().collect();
    let at = (1..chars.len().saturating_sub(1)).find(|&i| chars[i] != chars[i + 1])?;
    if chars.len() < 5 {
        return None;
    }
    chars.swap(at, at + 1);
    Some(text.replacen(word, &chars.into_iter().collect::<String>(), 1))
}

fn variants(a: &Address, other: &Address) -> Vec<(&'static str, String, Option<bool>)> {
    let query = a.query();
    let mut out = vec![
        (
            "no_country",
            query.trim_end_matches(", France").into(),
            Some(true),
        ),
        (
            "no_postcode",
            format!("{} {}, {}, France", a.number, a.street, a.city),
            Some(true),
        ),
        ("uppercase", query.to_uppercase(), Some(true)),
        ("lowercase", query.to_lowercase(), Some(true)),
        ("no_commas", query.replace(',', ""), Some(true)),
        (
            "reordered",
            format!(
                "France, {} {}, {} {}",
                a.postcode, a.city, a.number, a.street
            ),
            Some(true),
        ),
    ];
    if let Some(street) = typo(&a.street) {
        let mut copy = a.clone();
        copy.street = street;
        out.push(("typo_street", copy.query(), None));
    }
    if let Some(city) = typo(&a.city) {
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
        .context("BAN file name")?;
    let mut pools: BTreeMap<String, BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let department = a.department();
        if !(matches!(
            department,
            "2A" | "2B" | "971" | "972" | "973" | "974" | "976"
        ) || department
            .parse::<u8>()
            .is_ok_and(|n| (1..=95).contains(&n) && n != 20))
            || !a.certified
            || !["entrée", "bâtiment", "parcelle"].contains(&a.placement.as_str())
        {
            return Ok(());
        }
        // ponytail: diversity is limited to 200 candidates per department; increase the pool if it cannot fill five communes.
        let pool = pools.entry(a.department().into()).or_default();
        let hash = key(seed, &a.id);
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    let mut counts = BTreeMap::<&str, usize>::new();
    for (department, pool) in pools {
        let mut communes = BTreeSet::new();
        let mut streets = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| {
                if communes.contains(&a.commune) || streets.contains(&a.street) {
                    return false;
                }
                communes.insert(a.commune.clone());
                streets.insert(a.street.clone());
                true
            })
            .take(5)
            .collect();
        eprintln!("BAN {department}: {} locations", selected.len());
        for (i, a) in selected.iter().enumerate() {
            let tags = vec![
                "registry-sample".to_string(),
                "fr".into(),
                "ban".into(),
                release.into(),
                format!("department:{department}"),
                format!("placement:{}", a.placement),
                "commune-certified".into(),
            ];
            let base = json!({"kind":"address", "source":"ban-csv", "ref":a.id, "cohort":"fr_house", "queries":[a.query()], "label":a.query(), "tags":tags});
            lines.push(base.clone());
            if i % 2 == 0 && selected.len() > 1 {
                let other = selected[(i + 1) % selected.len()];
                let choices = variants(a, other);
                let (kind, query, good_match) = choices
                    .into_iter()
                    .min_by_key(|(kind, _, _)| {
                        (
                            counts.get(kind).copied().unwrap_or(0),
                            key(seed, &format!("{}:{kind}", a.id)),
                        )
                    })
                    .context("BAN variants")?;
                *counts.entry(kind).or_default() += 1;
                let mut variant = base;
                variant["cohort"] = json!("fr_degraded");
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
    ensure!(!lines.is_empty(), "no eligible BAN addresses");
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_sampling_and_lookup_are_reproducible() {
        let path = std::env::temp_dir().join(format!("ban-test-{}.csv.gz", std::process::id()));
        let header = "id;numero;rep;nom_voie;code_postal;code_insee;nom_commune;lat;lon;type_position;certification_commune\n";
        let rows = [
            "a;12;bis;\"Rue des; Écoles\";01000;01001;Village;46;5;entrée;1\n",
            "b;8;;Chemin du Moulin;01200;01002;Autreville;46.1;5.1;parcelle;1\n",
            "c;9;;Rue absente;01200;01003;Autreville;46.1;5.1;entrée;0\n",
            "d;10;;Rue invalide;01200;01004;Autreville;NaN;5.1;entrée;1\n",
            "e;11;;Rue de Monaco;98000;99138;Monaco;43.7;7.4;entrée;1\n",
            "f;11;;Rue hors département;97500;97501;Miquelon;46.8;-56.3;entrée;1\n",
        ];
        let write = |rows: &[&str]| {
            use std::io::Write;
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
        let mut reversed = rows;
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        assert_eq!(
            first.iter().filter(|s| s["cohort"] == "fr_house").count(),
            2
        );
        let found = lookup(&path, &BTreeSet::from(["a".into(), "b".into()])).unwrap();
        assert_eq!(found["a"].number, "12 bis");
        assert_eq!(found["a"].street, "Rue des; Écoles");
        assert_eq!(found["b"].radius_km(), 0.3);
        assert!(lookup(&path, &BTreeSet::from(["missing".into()])).is_err());
        assert_eq!(typo("Églantines").unwrap(), "Élgantines");
        let mut line = String::new();
        assert!(record(&mut &b"a;\"b\nc\";d\n"[..], &mut line).unwrap());
        assert_eq!(crate::nad::split(line.trim_end(), ';'), ["a", "b\nc", "d"]);
        assert!(record(&mut &b"a;\"unterminated"[..], &mut line).is_err());
        assert!(
            variants(&found["a"], &found["b"])
                .iter()
                .any(|(k, _, g)| *k == "wrong_postcode" && g.is_none())
        );
        std::fs::remove_file(path).unwrap();
    }
}
