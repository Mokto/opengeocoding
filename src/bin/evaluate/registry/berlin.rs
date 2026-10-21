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

const URL: &str = "https://daten.berlin.de/datensaetze/hauskoordinaten-als-teildatenbestand-adressen-berlin-796582aa";
const PER_CLASS: usize = 8;

#[derive(Clone, Debug)]
struct Address {
    id: String,
    quality: String,
    bezirk: String,
    ortsteil: String,
    street_key: String,
    street: String,
    number: String,
    addition: String,
    postcode: String,
    point: [f64; 2],
}

impl Address {
    fn house(&self) -> String {
        format!("{}{}", self.number, self.addition)
    }
    fn class(&self) -> &'static str {
        if self.addition.is_empty() {
            "plain"
        } else {
            "letter"
        }
    }
    fn query(&self) -> String {
        format!(
            "{} {}, {} Berlin, Germany",
            self.street,
            self.house(),
            self.postcode
        )
    }
}

fn eligible(id: &str, quality: &str, number: &str, addition: &str, postcode: &str) -> bool {
    id.len() == 16
        && id.starts_with("DEBE")
        && id.bytes().all(|b| b.is_ascii_alphanumeric())
        && matches!(quality, "A" | "B")
        && number.parse::<u32>().is_ok_and(|n| n > 0)
        && (addition.is_empty()
            || (addition.len() == 1 && addition.bytes().all(|b| b.is_ascii_alphabetic())))
        && postcode.len() == 5
        && postcode.starts_with('1')
        && postcode.as_bytes()[1].is_ascii_digit()
        && (b'0'..=b'4').contains(&postcode.as_bytes()[1])
        && postcode[2..].bytes().all(|b| b.is_ascii_digit())
}

fn load(path: &Path) -> Result<Vec<Address>> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty Berlin house-coordinate CSV"
    );
    let columns: BTreeMap<_, _> = crate::registry::nad::split(line.trim_end(), ',')
        .iter()
        .enumerate()
        .map(|(i, s)| (s.to_string(), i))
        .collect();
    for name in [
        "id",
        "quality",
        "bezirk",
        "ortsteil",
        "street_key",
        "street",
        "number",
        "addition",
        "postcode",
        "lat",
        "lon",
    ] {
        ensure!(columns.contains_key(name), "Berlin CSV missing {name}");
    }
    let mut ids = BTreeSet::new();
    let mut rows = Vec::new();
    let mut n = 0usize;
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == columns.len(),
            "Berlin CSV row {n}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        let id = field("id");
        ensure!(
            !id.is_empty() && ids.insert(id.to_string()),
            "empty or duplicate Berlin house id"
        );
        let lat: f64 = field("lat").parse().context("Berlin latitude")?;
        let lon: f64 = field("lon").parse().context("Berlin longitude")?;
        ensure!(
            (52.30..=52.70).contains(&lat) && (13.05..=13.80).contains(&lon),
            "Berlin CSV row {n}: needs Berlin EPSG:4326 coordinates"
        );
        let (quality, number, addition, postcode) = (
            field("quality"),
            field("number"),
            field("addition"),
            field("postcode"),
        );
        if !eligible(id, quality, number, addition, postcode)
            || field("bezirk").is_empty()
            || field("street_key").is_empty()
            || field("street").is_empty()
        {
            continue;
        }
        rows.push(Address {
            id: id.into(),
            quality: quality.into(),
            bezirk: field("bezirk").into(),
            ortsteil: field("ortsteil").into(),
            street_key: field("street_key").into(),
            street: field("street").into(),
            number: number.into(),
            addition: addition.into(),
            postcode: postcode.into(),
            point: [lat, lon],
        });
    }
    Ok(rows)
}

fn key(seed: &str, id: &str) -> String {
    sha256_hex(format!("{seed}\0{id}").as_bytes())
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("Berlin file name")?;
    let mut pools: BTreeMap<(String, &'static str), Vec<Address>> = BTreeMap::new();
    for address in load(path)? {
        pools
            .entry((address.bezirk.clone(), address.class()))
            .or_default()
            .push(address);
    }
    let mut lines = vec![];
    for ((bezirk, class), mut pool) in pools {
        pool.sort_by_key(|a| key(seed, &a.id));
        let mut streets = BTreeSet::new();
        for address in pool
            .into_iter()
            .filter(|a| streets.insert(a.street_key.clone()))
            .take(PER_CLASS)
        {
            lines.push(json!({
                "kind": "address",
                "source": "berlin-hko",
                "ref": address.id,
                "cohort": format!("de_{class}"),
                "queries": [address.query()],
                "label": address.query(),
                "tags": [
                    "registry-sample", "de", "berlin", class, release,
                    format!("quality:{}", address.quality),
                    format!("bezirk:{bezirk}"),
                ],
            }));
        }
    }
    ensure!(!lines.is_empty(), "no eligible Berlin house coordinates");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "berlin-hko",
    command: "sample-de",
    country: "de",
    input: "berlin",
    license: "DL-DE-ZERO-2.0",
    location: "berlin-hko",
    url: |id| format!("{URL}#{id}"),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert(
        "coordinate_transform".into(),
        json!({
            "from": "EPSG:25833",
            "to": "EPSG:4326",
            "tool": "DuckDB spatial / PROJ",
            "always_xy": true
        }),
    );
    let rows = load(path)?;
    let by_id: BTreeMap<_, _> = rows.into_iter().map(|a| (a.id.clone(), a)).collect();
    refs.iter()
        .map(|id| {
            let address = by_id
                .get(id)
                .with_context(|| format!("Berlin house coordinates lack {id}"))?;
            let mut provenance = metadata.clone();
            provenance.extend(serde_json::from_value::<serde_json::Map<String, Value>>(
                json!({
                    "oid": address.id,
                    "quality": address.quality,
                    "bezirk": address.bezirk,
                    "ortsteil": address.ortsteil,
                    "street_key": address.street_key,
                    "addition": address.addition,
                }),
            )?);
            Ok((
                id.clone(),
                Entry {
                    point: Point::Wgs84(address.point),
                    components: indexmap::IndexMap::from([
                        ("house_number", address.house()),
                        ("road", address.street.clone()),
                        ("postcode", address.postcode.clone()),
                    ]),
                    radius_floor_km: 0.15,
                    provenance,
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
    fn boroughs_letters_and_coordinates_survive_an_order_independent_draw() {
        let path = std::env::temp_dir().join(format!("berlin-hko-{}.csv", std::process::id()));
        let header =
            "id,quality,bezirk,ortsteil,street_key,street,number,addition,postcode,lat,lon\n";
        let mut owned = vec![
            "DEBE00000000000A,A,Mitte,Mitte,10001,Letter Street,12,A,10115,52.52,13.40\n"
                .to_string(),
            "DEBE00000000000B,B,Pankow,Pankow,20001,\"A, street\",4,,13187,52.57,13.41\n"
                .to_string(),
            "DEBE00000000000C,A,Mitte,Mitte,10002,No postcode,1,,not-a-postcode,52.52,13.40\n"
                .to_string(),
            "DEBE00000000000D,C,Mitte,Mitte,10003,Internal,2,,10115,52.52,13.40\n".to_string(),
            "DEBE00000000000E,A,Mitte,Mitte,10004,Zero,0,,10115,52.52,13.40\n".to_string(),
            "DEBE000000000010,A,Mitte,Mitte,10005,Range,3,14,10115,52.52,13.40\n".to_string(),
        ];
        for n in 1..=9 {
            owned.push(format!(
                "DEBE0000000000{n:02},A,Mitte,Mitte,{n:05},Street {n},{n},,10115,52.52,13.40\n"
            ));
        }
        let write = |rows: &[String]| {
            std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        };
        write(&owned);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 10);
        assert!(first.iter().any(|s| s["ref"] == "DEBE00000000000A"
            && s["queries"][0] == "Letter Street 12A, 10115 Berlin, Germany"
            && s["cohort"] == "de_letter"));
        assert!(
            first
                .iter()
                .any(|s| s["queries"][0] == "A, street 4, 13187 Berlin, Germany")
        );
        assert!(first.iter().all(|s| {
            ![
                "DEBE00000000000C",
                "DEBE00000000000D",
                "DEBE00000000000E",
                "DEBE000000000010",
            ]
            .contains(&s["ref"].as_str().unwrap())
        }));
        let mut reversed = owned.clone();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let found = prepare(&path, &BTreeSet::from(["DEBE00000000000A".into()])).unwrap();
        assert_eq!(found["DEBE00000000000A"].components["house_number"], "12A");
        assert!(matches!(
            found["DEBE00000000000A"].point,
            Point::Wgs84([52.52, 13.40])
        ));
        assert!(prepare(&path, &BTreeSet::from(["DEBE000000000099".into()])).is_err());
        write(&[
            "DEBE00000000000A,A,Mitte,Mitte,10001,Street,1,,10115,52.52,13.40\n".into(),
            "DEBE00000000000A,A,Mitte,Mitte,10001,Street,1,,10115,52.52,13.40\n".into(),
        ]);
        assert!(
            load(&path)
                .err()
                .is_some_and(|e| e.to_string().contains("duplicate"))
        );
        write(&["DEBE00000000000A,A,Mitte,Mitte,10001,Street,1,,10115,48.1,11.5\n".into()]);
        assert!(
            load(&path)
                .err()
                .is_some_and(|e| e.to_string().contains("EPSG:4326"))
        );
        std::fs::remove_file(path).unwrap();
    }
}
