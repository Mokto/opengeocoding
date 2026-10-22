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

pub const URL: &str = "https://service.pdok.nl/kadaster/bag/atom/bag.xml";

#[derive(Clone, Debug)]
pub struct Address {
    pub id: String,
    pub number: String,
    pub street: String,
    pub postcode: String,
    city: String,
    municipality: String,
    suffix: bool,
    pub point: [f64; 2],
}

impl Address {
    fn query(&self) -> String {
        format!(
            "{} {}, {} {}, Netherlands",
            self.street, self.number, self.postcode, self.city
        )
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Address) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        crate::registry::ban::record(&mut reader, &mut line)?,
        "empty BAG CSV"
    );
    let columns: BTreeMap<_, _> = crate::registry::nad::split(line.trim_end(), ',')
        .iter()
        .enumerate()
        .map(|(i, s)| (s.to_string(), i))
        .collect();
    for name in [
        "id",
        "street",
        "number",
        "letter",
        "addition",
        "postcode",
        "city",
        "municipality",
        "lat",
        "lon",
    ] {
        ensure!(columns.contains_key(name), "BAG CSV missing {name}");
    }
    let mut n = 0usize;
    while crate::registry::ban::record(&mut reader, &mut line)? {
        n += 1;
        if n.is_multiple_of(1_000_000) {
            eprintln!("BAG: {n} rows read");
        }
        let fields = crate::registry::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == columns.len(),
            "BAG CSV row {n}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        let lat: f64 = field("lat").parse().context("BAG latitude")?;
        let lon: f64 = field("lon").parse().context("BAG longitude")?;
        ensure!(
            (50. ..=54.).contains(&lat) && (3. ..=8.).contains(&lon),
            "BAG CSV row {n}: needs European Netherlands EPSG:4326 coordinates"
        );
        let id = field("id");
        let postcode = field("postcode");
        let municipality = field("municipality");
        if municipality.len() != 4
            || !municipality.bytes().all(|b| b.is_ascii_digit())
            || id.len() != 16
            || !id.bytes().all(|b| b.is_ascii_digit())
            || postcode.len() != 6
            || !postcode.as_bytes()[..4].iter().all(u8::is_ascii_digit)
            || !postcode.as_bytes()[4..].iter().all(u8::is_ascii_uppercase)
            || !field("number").parse::<u32>().is_ok_and(|n| n > 0)
            || field("letter").len() > 1
            || !field("letter").bytes().all(|b| b.is_ascii_alphabetic())
            || ["street", "city"].iter().any(|name| field(name).is_empty())
        {
            continue;
        }
        let mut number = format!("{}{}", field("number"), field("letter"));
        if !field("addition").is_empty() {
            number.push('-');
            number.push_str(field("addition"));
        }
        visit(Address {
            id: id.into(),
            number,
            street: field("street").into(),
            postcode: postcode.into(),
            city: field("city").into(),
            municipality: municipality.into(),
            suffix: !field("letter").is_empty() || !field("addition").is_empty(),
            point: [lat, lon],
        })?;
    }
    Ok(())
}

pub fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let mut found = BTreeMap::new();
    for_each(path, |a| {
        if ids.contains(&a.id) {
            ensure!(!found.contains_key(&a.id), "duplicate BAG id {}", a.id);
            found.insert(a.id.clone(), a);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == ids.len(),
        "BAG CSV lacks requested address ids"
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
            query.trim_end_matches(", Netherlands").into(),
            Some(true),
        ),
        (
            "no_postcode",
            format!("{} {}, {}, Netherlands", a.street, a.number, a.city),
            Some(true),
        ),
        ("uppercase", query.to_uppercase(), Some(true)),
        ("lowercase", query.to_lowercase(), Some(true)),
        ("no_commas", query.replace(',', ""), Some(true)),
        (
            "reordered",
            format!(
                "Netherlands, {} {}, {} {}",
                a.postcode, a.city, a.street, a.number
            ),
            Some(true),
        ),
        (
            "spaced_postcode",
            query.replace(
                &a.postcode,
                &format!("{} {}", &a.postcode[..4], &a.postcode[4..]),
            ),
            Some(true),
        ),
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
        .context("BAG file name")?;
    let mut pools: BTreeMap<(String, bool), BTreeMap<String, Address>> = BTreeMap::new();
    for_each(path, |a| {
        let pool = pools.entry((a.municipality.clone(), a.suffix)).or_default();
        let hash = key(seed, &a.id);
        // ponytail: diversity is limited to 200 candidates per municipality and suffix class; enlarge the pool if it cannot supply two streets.
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
    let mut streets = BTreeSet::new();
    let mut selected = BTreeMap::<String, Vec<Address>>::new();
    for ((municipality, _), pool) in pools {
        if let Some(a) = pool.into_values().find(|a| {
            streets.insert((
                municipality.clone(),
                a.city.to_lowercase(),
                a.street.to_lowercase(),
            ))
        }) {
            selected.entry(municipality).or_default().push(a);
        }
    }
    for (municipality, mut selected) in selected {
        selected.sort_by_key(|a| key(seed, &a.id));
        eprintln!("BAG {municipality}: {} locations", selected.len());
        for (i, a) in selected.iter().enumerate() {
            let base = json!({"kind":"address", "source":"bag-csv", "ref":a.id, "cohort":if a.suffix {"nl_suffix"} else {"nl_house"}, "queries":[a.query()], "label":a.query(), "tags":["registry-sample", "nl", "bag", release, format!("municipality:{municipality}")]});
            lines.push(base.clone());
            if i == 0
                && let Some(other) = selected.get(1)
            {
                let (kind, query, good_match) = variants(a, other)
                    .into_iter()
                    .min_by_key(|(kind, _, _)| {
                        (
                            counts.get(kind).copied().unwrap_or(0),
                            key(seed, &format!("{}:{kind}", a.id)),
                        )
                    })
                    .context("BAG variants")?;
                *counts.entry(kind).or_default() += 1;
                let mut variant = base;
                variant["cohort"] = json!("nl_degraded");
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
    ensure!(!lines.is_empty(), "no eligible BAG addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "bag-csv",
    command: "sample-nl",
    country: "nl",
    input: "bag",
    license: "CC0-1.0",
    location: "bag",
    url: |id| format!("{URL}#{id}"),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = {
        let mut metadata = super::provenance(path)?;
        metadata.insert("coordinate_transform".into(), json!({"from":"EPSG:28992", "to":"EPSG:4326", "tool":"DuckDB spatial / PROJ", "always_xy":true}));
        metadata
    };
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
                    places: super::places([row.city.as_str()]),
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_suffixes_and_coordinates_survive_an_order_independent_draw() {
        let path = std::env::temp_dir().join(format!("bag-test-{}.csv", std::process::id()));
        let header = "id,street,number,letter,addition,postcode,city,municipality,lat,lon\n";
        let rows = [
            "0001200000000001,First street,12,A,1,1234AB,Town,0001,52.1,5.2\n",
            "0001200000000002,Second street,3,,2,1235CD,Village,0002,52.2,5.3\n",
            "0001200000000003,\"A, street\",4,,,1236EF,Town,0001,52.3,5.4\n",
            "0001200000000004,Other street,5,,,1237GH,Town,0002,52.4,5.5\n",
            "0001200000000005,Bad postcode,6,,,invalid,Town,0002,52.4,5.5\n",
        ];
        let write =
            |rows: &[&str]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 6);
        let mut reversed = rows;
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let ids = BTreeSet::from(["0001200000000001".into(), "0001200000000002".into()]);
        let found = lookup(&path, &ids).unwrap();
        let a = &found["0001200000000001"];
        assert_eq!(a.number, "12A-1");
        assert_eq!(a.point, [52.1, 5.2]);
        assert_eq!(a.postcode, "1234AB");
        assert!(
            variants(a, &found["0001200000000002"])
                .iter()
                .any(|(k, q, g)| *k == "spaced_postcode"
                    && q.contains("1234 AB")
                    && *g == Some(true))
        );
        assert!(lookup(&path, &BTreeSet::from(["missing".into()])).is_err());
        write(&[rows[0], rows[0]]);
        assert!(
            lookup(&path, &ids)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
        write(&["0001200000000001,Street,1,,,1234AB,Town,0001,463000,155000\n"]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("EPSG:4326")
        );
        std::fs::remove_file(path).unwrap();
    }
}
