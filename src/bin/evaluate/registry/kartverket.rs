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

pub const URL: &str = "https://nedlasting.geonorge.no/geonorge/Basisdata/MatrikkelenAdresse/CSV/Basisdata_0000_Norge_4258_MatrikkelenAdresse_CSV.zip";

#[derive(Clone, Debug)]
pub struct Address {
    pub id: String,
    pub number: String,
    pub street: String,
    pub postcode: String,
    city: String,
    municipality: String,
    /// The urban settlement (tettsted) and municipality names.
    places: [String; 2],
    pub point: [f64; 2],
}

impl Address {
    fn query(&self) -> String {
        format!(
            "{} {}, {} {}, Norway",
            self.street, self.number, self.postcode, self.city
        )
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty Kartverket CSV"
    );
    let columns: BTreeMap<String, usize> =
        crate::registry::nad::split(line.trim_end().trim_start_matches('\u{feff}'), ';')
            .iter()
            .enumerate()
            .map(|(i, s)| (s.to_string(), i))
            .collect();
    for name in [
        "uuidAdresse",
        "kommunenummer",
        "adressetype",
        "adressenavn",
        "nummer",
        "bokstav",
        "EPSG-kode",
        "Nord",
        "Øst",
        "postnummer",
        "poststed",
        "tettstednavn",
        "kommunenavn",
    ] {
        ensure!(columns.contains_key(name), "Kartverket CSV missing {name}");
    }
    let mut n = 0usize;
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(1_000_000) {
            eprintln!("Kartverket: {n} rows read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ';');
        ensure!(
            fields.len() == columns.len(),
            "Kartverket CSV row {n}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        ensure!(
            field("EPSG-kode") == "4258",
            "Kartverket CSV row {n}: needs EPSG:4258, not {}",
            field("EPSG-kode")
        );
        if field("adressetype") != "vegadresse" {
            continue;
        }
        let municipality = field("kommunenummer");
        let postcode = field("postnummer");
        let Ok(lat) = field("Nord").parse::<f64>() else {
            continue;
        };
        let Ok(lon) = field("Øst").parse::<f64>() else {
            continue;
        };
        if !(-90. ..=90.).contains(&lat)
            || !(-180. ..=180.).contains(&lon)
            || municipality.len() != 4
            || !municipality.bytes().all(|b| b.is_ascii_digit())
            || postcode.len() != 4
            || !postcode.bytes().all(|b| b.is_ascii_digit())
            || !field("nummer").parse::<u32>().is_ok_and(|n| n > 0)
            || !field("bokstav").chars().all(char::is_alphabetic)
            || ["uuidAdresse", "adressenavn", "poststed"]
                .iter()
                .any(|name| field(name).is_empty())
        {
            continue;
        }
        visit(Address {
            id: field("uuidAdresse").into(),
            number: format!("{}{}", field("nummer"), field("bokstav")),
            street: field("adressenavn").into(),
            postcode: postcode.into(),
            city: field("poststed").into(),
            municipality: municipality.into(),
            places: [field("tettstednavn").into(), field("kommunenavn").into()],
            point: [lat, lon],
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
                "duplicate Kartverket UUID {}",
                a.id
            );
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "Kartverket CSV lacks {} requested UUIDs",
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
            query.trim_end_matches(", Norway").into(),
            Some(true),
        ),
        (
            "no_postcode",
            format!("{} {}, {}, Norway", a.street, a.number, a.city),
            Some(true),
        ),
        ("uppercase", query.to_uppercase(), Some(true)),
        ("lowercase", query.to_lowercase(), Some(true)),
        ("no_commas", query.replace(',', ""), Some(true)),
        (
            "reordered",
            format!(
                "Norway, {} {}, {} {}",
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
        .context("Kartverket file name")?;
    let mut pools: BTreeMap<String, BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        if matches!(&a.municipality[..2], "21" | "22") {
            return Ok(());
        }
        // ponytail: diversity is limited to 100 candidates per municipality; increase the pool if it cannot supply two streets.
        let pool = pools.entry(a.municipality.clone()).or_default();
        let hash = key(seed, &a.id);
        if pool.len() < 100 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 100 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    let mut counts = BTreeMap::<&str, usize>::new();
    for (municipality, pool) in pools {
        let mut streets = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| streets.insert(a.street.to_lowercase()))
            .take(2)
            .collect();
        eprintln!("Kartverket {municipality}: {} locations", selected.len());
        for (i, a) in selected.iter().enumerate() {
            let tags = vec![
                "registry-sample".to_string(),
                "no".into(),
                "kartverket".into(),
                release.into(),
                format!("municipality:{municipality}"),
                format!("county:{}", &municipality[..2]),
            ];
            let base = json!({"kind":"address", "source":"kartverket-csv", "ref":a.id, "cohort":"no_house", "queries":[a.query()], "label":a.query(), "tags":tags});
            lines.push(base.clone());
            if i == 0 && selected.len() == 2 {
                let (kind, query, good_match) = variants(a, selected[1])
                    .into_iter()
                    .min_by_key(|(kind, _, _)| {
                        (
                            counts.get(kind).copied().unwrap_or(0),
                            key(seed, &format!("{}:{kind}", a.id)),
                        )
                    })
                    .context("Kartverket variants")?;
                *counts.entry(kind).or_default() += 1;
                let mut variant = base;
                variant["cohort"] = json!("no_degraded");
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
    ensure!(!lines.is_empty(), "no eligible Kartverket addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "kartverket-csv",
    command: "sample-no",
    country: "no",
    input: "kartverket",
    license: "CC-BY-4.0",
    location: "kartverket",
    url: |id| format!("{URL}#{id}"),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    lookup(path, refs)?
        .into_iter()
        .map(|(id, row)| {
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(row.point),
                    components: indexmap::IndexMap::from([
                        ("house_number", row.number),
                        ("road", row.street),
                        ("postcode", row.postcode),
                    ]),
                    radius_floor_km: 0.,
                    provenance: metadata.clone(),
                    number_aliases: None,
                    places: super::places([row.city.as_str(), &row.places[0], &row.places[1]]),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_rows_not_geocoder_answers_define_the_sample() {
        let path = std::env::temp_dir().join(format!("kartverket-test-{}.csv", std::process::id()));
        let header = "uuidAdresse;kommunenummer;adressetype;adressenavn;nummer;bokstav;EPSG-kode;Nord;Øst;postnummer;poststed;tettstednavn;kommunenavn\n";
        let rows = [
            "a;0301;vegadresse;Øvre gate;12;B;4258;59.92;10.78;0123;OSLO;Oslo;Oslo\n",
            "b;0301;vegadresse;\"Nedre; gate\";4;;4258;59.93;10.79;0456;OSLO;Oslo;Oslo\n",
            "c;0301;matrikkeladresse;;4;;4258;59.93;10.79;0456;OSLO;Oslo;Oslo\n",
            "d;0301;vegadresse;Ugyldig;4;;4258;NaN;10.79;0456;OSLO;Oslo;Oslo\n",
            "e;2100;vegadresse;Vei;4;;4258;78;15;9170;LONGYEARBYEN;Longyearbyen;Svalbard\n",
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
        assert_eq!(
            first.iter().filter(|s| s["cohort"] == "no_house").count(),
            2
        );
        let found = lookup(&path, &BTreeSet::from(["a".into(), "b".into()])).unwrap();
        assert_eq!(found["a"].number, "12B");
        assert_eq!(found["a"].postcode, "0123");
        assert_eq!(found["a"].places, ["Oslo", "Oslo"].map(String::from));
        assert_eq!(found["a"].point, [59.92, 10.78]);
        assert!(found["a"].query().starts_with("Øvre gate 12B"));
        assert_eq!(found["b"].street, "Nedre; gate");
        assert!(lookup(&path, &BTreeSet::from(["absent".into()])).is_err());
        assert!(
            variants(&found["a"], &found["b"])
                .iter()
                .any(|(k, q, g)| *k == "ascii" && q.starts_with("Ovre gate") && *g == Some(true))
        );
        write(&["a;0301;vegadresse;Øvre gate;12;B;25833;6640000;263000;0123;OSLO;Oslo;Oslo\n"]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("EPSG:4258")
        );
        std::fs::remove_file(path).unwrap();
    }
}
