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

const URL: &str = "https://dataset.address-br.digital.go.jp/";
const EXPORT: &str = "https://data.openaddresses.io/cache/uploads/jeffdefacto/2026-10-01-tkes5/jp_abr_residential.csv.zip";
const SCRIPT: &str = "https://github.com/openaddresses/openaddresses/blob/5172acb847e0127cab772a75f92b43444cac5db0/scripts/jp/abr_residential.py";
const COLUMNS: [&str; 9] = [
    "id", "pref", "city", "street", "koaza", "number", "postcode", "lon", "lat",
];

struct Residence {
    record: [String; 9],
    point: [f64; 2],
}

impl Residence {
    fn id(&self) -> &str {
        &self.record[0]
    }

    fn prefecture(&self) -> &str {
        &self.id()[..2]
    }

    fn class(&self) -> &'static str {
        if self.record[5].split('-').count() == 3 {
            "three_part"
        } else {
            "two_part"
        }
    }

    fn address(&self) -> String {
        self.record[1..6].concat()
    }
}

fn for_each(path: &Path, mut visit: impl FnMut(Residence) -> Result<()>) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut line = String::new();
    ensure!(super::ban::record(&mut reader, &mut line)?, "empty ABR CSV");
    ensure!(
        super::nad::split(line.trim_end(), ',')
            .iter()
            .map(|s| s.as_ref())
            .eq(COLUMNS),
        "{} is not the OpenAddresses ABR residential export",
        path.display()
    );
    let (mut count, mut eligible) = (0usize, 0usize);
    while super::ban::record(&mut reader, &mut line)? {
        count += 1;
        if count.is_multiple_of(5_000_000) {
            eprintln!("ABR: {count} rows read");
        }
        let fields = super::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == COLUMNS.len(),
            "ABR row {count}: column count"
        );
        let id = fields[0].as_ref();
        ensure!(
            id.len() >= 18
                && id.bytes().all(|c| c.is_ascii_digit())
                && id[..2].parse::<u8>().is_ok_and(|p| (1..=47).contains(&p)),
            "ABR row {count}: invalid residence id or prefecture"
        );
        let numbers: Vec<_> = fields[5].split('-').collect();
        if fields[1..4].iter().any(|s| s.trim().is_empty())
            || !(2..=3).contains(&numbers.len())
            || numbers.iter().any(|n| {
                !(1..=4).contains(&n.len())
                    || n.starts_with('0')
                    || !n.bytes().all(|c| c.is_ascii_digit())
            })
            || (!fields[6].is_empty()
                && (fields[6].len() != 7 || !fields[6].bytes().all(|c| c.is_ascii_digit())))
            || fields[7].is_empty()
            || fields[8].is_empty()
        {
            continue;
        }
        let point = [
            fields[8].parse::<f64>().context("ABR latitude")?,
            fields[7].parse::<f64>().context("ABR longitude")?,
        ];
        ensure!(
            (20. ..46.).contains(&point[0]) && (122. ..154.).contains(&point[1]),
            "ABR row {count}: needs Japanese latitude/longitude, not swapped axes"
        );
        eligible += 1;
        visit(Residence {
            record: std::array::from_fn(|i| fields[i].trim().to_string()),
            point,
        })?;
    }
    eprintln!("ABR: {count} rows, {eligible} eligible residences");
    Ok(())
}

fn lookup(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Residence>> {
    let mut found = BTreeMap::new();
    for_each(path, |r| {
        if refs.contains(r.id()) {
            ensure!(!found.contains_key(r.id()), "duplicate ABR id {}", r.id());
            found.insert(r.id().to_string(), r);
        }
        Ok(())
    })?;
    ensure!(
        found.len() == refs.len(),
        "ABR lacks requested eligible residence ids"
    );
    let addresses: BTreeMap<_, _> = found.values().map(|r| (r.address(), r.id())).collect();
    ensure!(
        addresses.len() == found.len(),
        "duplicate sampled ABR address"
    );
    for_each(path, |r| {
        if let Some(id) = addresses.get(&r.address()) {
            ensure!(
                *id == r.id(),
                "ambiguous ABR address for {id} and {}",
                r.id()
            );
        }
        Ok(())
    })?;
    Ok(found)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<(String, &'static str), BTreeMap<String, Residence>>::new();
    for_each(path, |r| {
        let pool = pools.entry((r.prefecture().into(), r.class())).or_default();
        let hash = sha256_hex(format!("{seed}\0{}", r.id()).as_bytes());
        // ponytail: municipality diversity uses 200 candidates per prefecture and class; enlarge the pool if it cannot supply the target.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash <= *k) {
            ensure!(!pool.contains_key(&hash), "duplicate ABR id {}", r.id());
            pool.insert(hash, r);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
        Ok(())
    })?;
    let mut lines = vec![];
    for ((prefecture, class), pool) in pools {
        let mut municipalities = BTreeSet::new();
        let take = if class == "two_part" { 4 } else { 2 };
        for r in pool
            .values()
            .filter(|r| municipalities.insert(&r.id()[..6]))
            .take(take)
        {
            let address = r.address();
            for (form, query) in [
                ("country_suffix", format!("{address}, Japan")),
                ("country_prefix", format!("日本{address}")),
            ] {
                lines.push(json!({"kind":"address", "source":"abr-residences", "ref":r.id(), "cohort":format!("jp_abr_{class}"), "queries":[query], "label":address, "extra_regions":[format!("JP-{prefecture}")], "tags":["registry-sample", "jp", "abr", "residence-not-block", format!("prefecture:{prefecture}"), format!("municipality:{}", &r.id()[..6]), class, form]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible ABR residences");
    let refs = lines
        .iter()
        .map(|s| s["ref"].as_str().unwrap().to_string())
        .collect();
    lookup(path, &refs)?;
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "abr-residences",
    command: "sample-jp-abr",
    country: "jp",
    input: "abr",
    license: "CC-BY-4.0",
    location: "abr-residence",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let mut metadata = super::provenance(path)?;
    metadata.insert("export".into(), json!(EXPORT));
    metadata.insert("preparation_script".into(), json!(SCRIPT));
    metadata.insert(
        "terms".into(),
        json!("https://www.digital.go.jp/policies/base_registry_address_tos"),
    );
    metadata.insert("coordinates".into(), json!("ABR residence-position rep_lat/rep_lon, exported as JGD2011 (EPSG:6668); used as WGS84 without transformation at 150 m tolerance; the export omits per-record CRS, scale and source codes"));
    metadata.insert("attribution".into(), json!("デジタル庁 アドレス・ベース・レジストリ (Digital Agency, Address Base Registry); joined by OpenAddresses, sampled and reformatted by OpenGeocoding"));
    lookup(path, refs)?
        .into_iter()
        .map(|(id, r)| {
            let mut provenance = metadata.clone();
            provenance.insert(
                "record".into(),
                json!(
                    COLUMNS
                        .iter()
                        .zip(&r.record)
                        .map(|(k, v)| (k.to_string(), json!(v)))
                        .collect::<serde_json::Map<_, _>>()
                ),
            );
            let mut components = indexmap::IndexMap::from([("house_number", r.record[5].clone())]);
            if !r.record[6].is_empty() {
                let pc = &r.record[6];
                components.insert("postcode", format!("{}-{}", &pc[..3], &pc[3..]));
            }
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(r.point.map(|c| (c * 1e6).round() / 1e6)),
                    components,
                    radius_floor_km: 0.15,
                    provenance,
                    number_aliases: None,
                    places: vec![],
                },
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residences_keep_complete_numbers_source_spelling_and_shared_locations() {
        let path = std::env::temp_dir().join(format!("abr-test-{}.csv", std::process::id()));
        let header = format!("{}\n", COLUMNS.join(","));
        let rows = [
            "1310160001001001001,東京都,千代田区,永田町二丁目,,1-6,1000014,139.745,35.67\n",
            "1310240001001001001,東京都,中央区,新川１丁目,,2-3-4,,139.78,35.67\n",
            "0110110001001001001,北海道,札幌市中央区,旭ケ丘１丁目,,1-2,0640941,141.32,43.04\n",
            "1310320001001001001,東京都,港区,赤坂一丁目,,5,,139.74,35.67\n",
            "1310320001001001002,東京都,港区,赤坂一丁目,,1-0,,139.74,35.67\n",
            "1310320001001001003,東京都,港区,赤坂一丁目,,1-A,,139.74,35.67\n",
        ];
        let write =
            |rows: &[&str]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 6);
        assert!(
            first
                .iter()
                .any(|s| s["queries"][0] == "日本東京都中央区新川１丁目2-3-4")
        );
        assert!(first.iter().any(|s| s["extra_regions"] == json!(["JP-01"])));
        let mut reversed = rows.to_vec();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let refs = first
            .iter()
            .map(|s| s["ref"].as_str().unwrap().to_string())
            .collect();
        let entries = prepare(&path, &refs).unwrap();
        let residence = &entries["1310240001001001001"];
        assert_eq!(residence.components["house_number"], "2-3-4");
        assert!(!residence.components.contains_key("postcode"));
        assert_eq!(
            entries["1310160001001001001"].components["postcode"],
            "100-0014"
        );
        assert_eq!(
            first
                .iter()
                .filter(|s| s["ref"] == "1310240001001001001")
                .count(),
            2
        );
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        write(&[rows[0], rows[0]]);
        assert!(sample(&path, "test").is_err());
        assert!(prepare(&path, &BTreeSet::from(["1310160001001001001".into()])).is_err());
        let alias = rows[0].replace("1310160001001001001", "1310160001001001002");
        write(&[rows[0], &alias]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("ambiguous")
        );
        assert!(
            prepare(&path, &BTreeSet::from(["1310160001001001001".into()]))
                .err()
                .unwrap()
                .to_string()
                .contains("ambiguous")
        );
        for bad in ["35.67,139.745", "139.745,NaN", "139.745,inf"] {
            let row = rows[0].replace("139.745,35.67", bad);
            write(&[&row]);
            assert!(sample(&path, "test").is_err());
        }
        let row = rows[0].replacen("131016", "481016", 1);
        write(&[&row]);
        assert!(sample(&path, "test").is_err());
        std::fs::write(&path, "id,pref,city,street,koaza,number,postcode,lat,lon\n").unwrap();
        assert!(sample(&path, "test").is_err());
        let diverse: Vec<_> = (0..220)
            .map(|n| {
                format!(
                    "{:06}0001001001001,東京都,架空市{n},架空町１丁目,,1-2,,139.74,35.67\n",
                    131001 + n
                )
            })
            .collect();
        let refs: Vec<_> = diverse.iter().map(String::as_str).collect();
        write(&refs);
        let capped = sample(&path, "test").unwrap();
        assert_eq!(capped.len(), 8);
        let mut ids: Vec<_> = diverse
            .iter()
            .map(|r| r.split(',').next().unwrap())
            .collect();
        ids.sort_by_key(|id| sha256_hex(format!("test\0{id}").as_bytes()));
        assert_eq!(
            capped
                .iter()
                .map(|s| s["ref"].as_str().unwrap())
                .collect::<BTreeSet<_>>(),
            ids[..4].iter().copied().collect()
        );
        write(&refs.into_iter().rev().collect::<Vec<_>>());
        assert_eq!(capped, sample(&path, "test").unwrap());
        std::fs::remove_file(path).unwrap();
    }
}
