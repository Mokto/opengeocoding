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

const URL: &str = "https://data.public.lu/en/datasets/adresses-georeferencees-bd-adresses/";

struct Address {
    id: String,
    geoportail: String,
    street_id: String,
    street: String,
    number: String,
    localite: String,
    postcode: String,
    commune: String,
    lau2: String,
    class: &'static str,
    point: [f64; 2],
}

fn house_class(number: &str) -> Option<&'static str> {
    let (low, high) = match number.split_once('-') {
        None => (number, None),
        Some((low, high)) if !high.contains('-') => (low, Some(high)),
        Some(_) => return None,
    };
    let low_suffix = letter_suffix(low)?;
    if let Some(high) = high {
        letter_suffix(high)?;
        return (!low_suffix).then_some("range");
    }
    Some(if low_suffix { "suffix" } else { "plain" })
}

fn precedes(row: &Address, old: &Address) -> bool {
    match row.geoportail.cmp(&old.geoportail) {
        std::cmp::Ordering::Equal => row.point[0]
            .total_cmp(&old.point[0])
            .then(row.point[1].total_cmp(&old.point[1]))
            .is_lt(),
        order => order.is_lt(),
    }
}

fn letter_suffix(number: &str) -> Option<bool> {
    let bytes = number.as_bytes();
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits == 0 || !bytes[digits..].iter().all(|b| b.is_ascii_alphabetic()) {
        None
    } else {
        Some(digits < bytes.len())
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Address>> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        super::ban::record(&mut reader, &mut line)?,
        "empty BD-Adresses CSV"
    );
    let columns: BTreeMap<_, _> =
        super::nad::split(line.trim_start_matches('\u{feff}').trim_end(), ';')
            .into_iter()
            .enumerate()
            .map(|(i, s)| (s.to_string(), i))
            .collect();
    for name in [
        "rue",
        "numero",
        "localite",
        "code_postal",
        "id_caclr_rue",
        "id_caclr_bat",
        "lat_wgs84",
        "lon_wgs84",
        "id_geoportail",
        "commune",
        "lau2",
    ] {
        ensure!(columns.contains_key(name), "BD-Adresses CSV missing {name}");
    }
    let mut addresses = BTreeMap::<String, Address>::new();
    let mut count = 0usize;
    let mut empty_numbers = 0usize;
    let mut numbered = 0usize;
    while super::ban::record(&mut reader, &mut line)? {
        count += 1;
        let fields = super::nad::split(line.trim_end_matches(['\r', '\n']), ';');
        ensure!(
            fields.len() == columns.len(),
            "BD-Adresses row {count}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        if field("numero").is_empty() {
            empty_numbers += 1;
            continue;
        }
        numbered += 1;
        let class = house_class(field("numero")).with_context(|| {
            format!(
                "BD-Adresses row {count}: house number {:?}",
                field("numero")
            )
        })?;
        let postcode = field("code_postal");
        ensure!(
            postcode.len() == 4 && postcode.bytes().all(|b| b.is_ascii_digit()),
            "BD-Adresses row {count}: postcode"
        );
        for name in [
            "rue",
            "localite",
            "id_caclr_rue",
            "id_caclr_bat",
            "id_geoportail",
            "commune",
            "lau2",
        ] {
            ensure!(
                !field(name).is_empty(),
                "BD-Adresses row {count}: empty {name}"
            );
        }
        let lat = field("lat_wgs84")
            .parse::<f64>()
            .with_context(|| format!("BD-Adresses row {count}: lat_wgs84"))?;
        let lon = field("lon_wgs84")
            .parse::<f64>()
            .with_context(|| format!("BD-Adresses row {count}: lon_wgs84"))?;
        ensure!(
            (49.0..=51.0).contains(&lat) && (5.0..=7.0).contains(&lon),
            "BD-Adresses row {count}: WGS84 point {lat},{lon} is outside Luxembourg; LUREF columns are not coordinates"
        );
        let id = field("id_caclr_bat").to_string();
        let row = Address {
            id: id.clone(),
            geoportail: field("id_geoportail").into(),
            street_id: field("id_caclr_rue").into(),
            street: field("rue").into(),
            number: field("numero").into(),
            localite: field("localite").into(),
            postcode: postcode.into(),
            commune: field("commune").into(),
            lau2: field("lau2").into(),
            class,
            point: [lat, lon],
        };
        if addresses.get(&id).is_none_or(|old| precedes(&row, old)) {
            addresses.insert(id, row);
        }
    }
    eprintln!(
        "BD-Adresses: {count} rows, {empty_numbers} empty numbers, {} duplicate building rows collapsed, {} eligible buildings",
        numbered - addresses.len(),
        addresses.len()
    );
    Ok(addresses)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<_, Vec<Address>>::new();
    for address in load(path)?.into_values() {
        pools
            .entry((address.commune.clone(), address.class))
            .or_default()
            .push(address);
    }
    let mut lines = vec![];
    for ((commune, class), mut pool) in pools {
        pool.sort_by_key(|a| sha256_hex(format!("{seed}\0{}", a.id).as_bytes()));
        let address = pool.into_iter().next().expect("commune class pool");
        let query = format!(
            "{} {}, {} {}, Luxembourg",
            address.street, address.number, address.postcode, address.localite
        );
        lines.push(json!({
            "kind": "address",
            "source": "act-csv",
            "ref": address.id,
            "cohort": format!("lu_{class}"),
            "queries": [query.clone()],
            "label": query,
            "tags": ["registry-sample", "lu", class, commune],
        }));
    }
    ensure!(!lines.is_empty(), "no eligible BD-Adresses buildings");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "act-csv",
    command: "sample-lu",
    country: "lu",
    input: "act",
    license: "CC0-1.0",
    location: "act-building",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    let rows = load(path)?;
    refs.iter()
        .map(|id| {
            let address = rows
                .get(id)
                .with_context(|| format!("BD-Adresses lacks building {id}"))?;
            let mut provenance = metadata.clone();
            provenance.extend(serde_json::from_value::<serde_json::Map<String, Value>>(
                json!({
                    "id_caclr_bat": address.id,
                    "id_caclr_rue": address.street_id,
                    "id_geoportail": address.geoportail,
                    "street": address.street,
                    "house_number": address.number,
                    "localite": address.localite,
                    "commune": address.commune,
                    "lau2": address.lau2,
                    "coordinate_system": "EPSG:4326",
                    "location_key": "id_caclr_bat; duplicate rows keep the smallest id_geoportail",
                }),
            )?);
            Ok((
                id.clone(),
                Entry {
                    point: Point::Wgs84(address.point),
                    components: indexmap::IndexMap::from([
                        ("house_number", address.number.clone()),
                        ("postcode", address.postcode.clone()),
                    ]),
                    radius_floor_km: 0.15,
                    provenance,
                    number_aliases: None,
                    places: super::places([address.localite.as_str(), &address.commune]),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_building_per_commune_class_keeps_the_smallest_geoportail() {
        let path = std::env::temp_dir().join(format!("act-test-{}.csv", std::process::id()));
        let header = "\u{feff}rue;numero;localite;code_postal;id_caclr_rue;id_caclr_bat;lat_wgs84;lon_wgs84;id_geoportail;commune;lau2\n";
        let rows = [
            "Rue A;12;Luxembourg;1111;1;200;49.60;6.10;B;Luxembourg;0304\n",
            "Rue A;12;Luxembourg;1111;1;200;49.61;6.11;A;Strassen;0309\n",
            "Rue B;12A;Luxembourg;1111;2;201;49.62;6.12;C;Luxembourg;0304\n",
            "Rue C;2-4;Luxembourg;1111;3;202;49.63;6.13;D;Luxembourg;0304\n",
            "Rue D;3-3A;Dalheim;1112;4;203;49.54;6.25;E;Dalheim;0101\n",
            "Rue E;;Esch;1113;5;204;49.50;5.98;F;Esch-sur-Alzette;1004\n",
            "Rue F;5;Esch;1113;6;300;49.50;5.98;G;Esch-sur-Alzette;1004\n",
            "Rue G;9;Esch;1113;7;301;49.51;5.99;H;Esch-sur-Alzette;1004\n",
        ];
        let write = |order: &[usize]| {
            let mut text = header.to_string();
            for i in order {
                text.push_str(rows[*i]);
            }
            std::fs::write(&path, text).unwrap();
        };
        write(&[0, 1, 2, 3, 4, 5, 6, 7]);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 5);
        let esch_hash = |id: &str| sha256_hex(format!("test\0{id}").as_bytes());
        let esch = if esch_hash("300") < esch_hash("301") {
            "300"
        } else {
            "301"
        };
        let refs: BTreeSet<_> = first.iter().map(|s| s["ref"].as_str().unwrap()).collect();
        assert_eq!(
            refs,
            ["200", "201", "202", "203", esch].into_iter().collect()
        );
        assert!(
            first
                .iter()
                .any(|s| s["queries"][0] == "Rue D 3-3A, 1112 Dalheim, Luxembourg")
        );
        assert_eq!(
            (
                house_class("12"),
                house_class("12BIS"),
                house_class("22-26"),
                house_class("3-3A"),
                house_class("12A-14"),
            ),
            (
                Some("plain"),
                Some("suffix"),
                Some("range"),
                Some("range"),
                None,
            )
        );
        write(&[7, 6, 5, 4, 3, 2, 1, 0]);
        assert_eq!(first, sample(&path, "test").unwrap());
        let entries = prepare(&path, &BTreeSet::from(["200".into()])).unwrap();
        let entry = entries.values().next().unwrap();
        assert!(matches!(entry.point, Point::Wgs84([49.61, 6.11])));
        assert_eq!(entry.components["house_number"], "12");
        assert_eq!(entry.provenance["commune"], "Strassen");
        assert_eq!(entry.provenance["id_geoportail"], "A");
        assert!(prepare(&path, &BTreeSet::from(["204".into()])).is_err());
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
