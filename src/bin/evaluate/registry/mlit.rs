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

const URL: &str = "https://nlftp.mlit.go.jp/isj/";
const COLUMNS: [&str; 14] = [
    "都道府県名",
    "市区町村名",
    "大字・丁目名",
    "小字・通称名",
    "街区符号・地番",
    "座標系番号",
    "Ｘ座標",
    "Ｙ座標",
    "緯度",
    "経度",
    "住居表示フラグ",
    "代表フラグ",
    "更新前履歴フラグ",
    "更新後履歴フラグ",
];

#[derive(Clone)]
struct Block {
    id: String,
    prefecture: String,
    record: [String; 14],
    point: [f64; 2],
    ambiguous: bool,
}

impl Block {
    fn address(&self) -> String {
        format!("{}番", self.record[..5].concat())
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Block>> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        super::ban::record(&mut reader, &mut line)?,
        "empty MLIT CSV"
    );
    ensure!(
        super::nad::split(line.trim_end(), ',')
            .iter()
            .map(|s| s.as_ref())
            .eq(std::iter::once("pref_code")
                .chain(COLUMNS)
                .chain(["lat", "lon"])),
        "{} is not the block extract tests/registry/mlit-csv.sql writes",
        path.display()
    );
    let (mut n, mut eligible) = (0usize, 0usize);
    let mut blocks = BTreeMap::<String, Block>::new();
    while super::ban::record(&mut reader, &mut line)? {
        n += 1;
        let fields = super::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(fields.len() == 17, "MLIT CSV row {n}: column count");
        let prefecture = fields[0].as_ref();
        ensure!(
            prefecture.len() == 2
                && prefecture
                    .parse::<u8>()
                    .is_ok_and(|p| (1..=47).contains(&p)),
            "MLIT row {n}: unknown prefecture code"
        );
        let record: [String; 14] = std::array::from_fn(|i| fields[i + 1].trim().into());
        let number = &record[4];
        if record[10] != "1"
            || record[11] != "1"
            || record[13] == "3"
            || !(1..=4).contains(&number.len())
            || number.starts_with('0')
            || !number.bytes().all(|b| b.is_ascii_digit())
            || record[..3].iter().any(|s| s.is_empty())
        {
            continue;
        }
        let point = [
            fields[15].parse::<f64>().context("MLIT WGS84 latitude")?,
            fields[16].parse::<f64>().context("MLIT WGS84 longitude")?,
        ];
        ensure!(
            (20. ..46.).contains(&point[0]) && (122. ..154.).contains(&point[1]),
            "MLIT row {n}: needs Japanese WGS84 coordinates, not swapped axes"
        );
        let id = sha256_hex(format!("{prefecture}\0{}", record[..5].join("\0")).as_bytes());
        let a = Block {
            id: id.clone(),
            prefecture: prefecture.into(),
            record,
            point,
            ambiguous: false,
        };
        eligible += 1;
        if let Some(old) = blocks.get_mut(&id) {
            let ambiguous = old.ambiguous || old.point != a.point;
            if a.record < old.record {
                *old = a;
            }
            old.ambiguous = ambiguous;
        } else {
            blocks.insert(id, a);
        }
    }
    let groups = blocks.len();
    blocks.retain(|_, b| !b.ambiguous);
    eprintln!(
        "MLIT: {n} rows, {eligible} eligible, {groups} block keys, {} unambiguous",
        blocks.len()
    );
    Ok(blocks)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<String, BTreeMap<String, Block>>::new();
    for b in load(path)?.into_values() {
        pools
            .entry(b.prefecture.clone())
            .or_default()
            .insert(sha256_hex(format!("{seed}\0{}", b.id).as_bytes()), b);
    }
    let mut lines = vec![];
    for (prefecture, pool) in pools {
        let mut municipalities = BTreeSet::new();
        for b in pool
            .values()
            .filter(|b| municipalities.insert(&b.record[1]))
            .take(4)
        {
            let address = b.address();
            for (form, query) in [
                ("country_suffix", format!("{address}, Japan")),
                ("country_prefix", format!("日本{address}")),
            ] {
                lines.push(json!({"kind":"address", "source":"mlit-blocks", "ref":b.id, "cohort":"jp_block", "level":"street", "radius_km":0.3, "queries":[query], "label":address, "extra_regions":[format!("JP-{prefecture}")], "tags":["registry-sample", "jp", "mlit", "2025", format!("prefecture:{prefecture}"), format!("municipality:{}", b.record[1]), "block-not-house", form]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible MLIT blocks");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "mlit-blocks",
    command: "sample-jp",
    country: "jp",
    input: "mlit",
    license: "MLIT-ISJ-Terms",
    location: "mlit-block",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    let blocks = load(path)?;
    refs.iter().map(|id| {
        let b = blocks.get(id).with_context(|| format!("MLIT lacks unambiguous block {id}"))?;
        let mut provenance = metadata.clone();
        provenance.insert("record".into(), json!(COLUMNS.iter().zip(&b.record).map(|(k,v)| (k.to_string(), json!(v))).collect::<serde_json::Map<_,_>>()));
        provenance.insert("prefecture_code".into(), json!(b.prefecture));
        provenance.insert("upstream".into(), json!(format!("https://nlftp.mlit.go.jp/isj/dls/data/24.0a/{}000-24.0a.zip", b.prefecture)));
        provenance.insert("attribution".into(), json!("街区レベル位置参照情報 国土交通省, 2025年度版; sampled and reformatted by OpenGeocoding; terms: https://nlftp.mlit.go.jp/isj/agreement.html"));
        provenance.insert("coordinate_transform".into(), json!("JGD2000 geographic (EPSG:4612) to WGS84 (EPSG:4326), DuckDB spatial / PROJ, always_xy=true; source point represents a block, not a residence"));
        Ok((id.clone(), Entry {
            point: Point::Wgs84(b.point),
            components: indexmap::IndexMap::new(),
            radius_floor_km: 0.3,
            provenance,
            number_aliases: None,
            places: vec![],
        }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_current_representative_residential_blocks_and_no_house_claim() {
        let path = std::env::temp_dir().join(format!("mlit-test-{}.csv", std::process::id()));
        let header = format!("pref_code,{},lat,lon\n", COLUMNS.join(","));
        let row = |city: &str,
                   town: &str,
                   number: &str,
                   residential: &str,
                   representative: &str,
                   update: &str,
                   lat: &str| {
            format!(
                "13,東京都,{city},{town},,{number},9,-36252.5,-7956.2,35.673207,139.745444,{residential},{representative},0,{update},{lat},139.745444\n"
            )
        };
        let rows = [
            row("千代田区", "永田町一丁目", "6", "1", "1", "0", "35.673207"),
            row("千代田区", "永田町一丁目", "7", "1", "1", "0", "35.674"),
            row("中央区", "新川二丁目", "1", "1", "1", "1", "35.67"),
            row("港区", "赤坂一丁目", "2", "0", "1", "0", "35.67"),
            row("港区", "赤坂一丁目", "3", "1", "0", "0", "35.67"),
            row("港区", "赤坂一丁目", "4", "1", "1", "3", "35.67"),
            row("港区", "赤坂一丁目", "5", "1", "1", "0", "35.67"),
            row("港区", "赤坂一丁目", "5", "1", "1", "0", "35.68"),
            row("港区", "赤坂一丁目", "南2", "1", "1", "0", "35.67"),
        ];
        let write =
            |rows: &[String]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 4);
        assert!(
            first
                .iter()
                .all(|s| s["level"] == "street" && s["extra_regions"] == json!(["JP-13"]))
        );
        assert!(
            first
                .iter()
                .any(|s| s["queries"][0] == "日本東京都中央区新川二丁目1番")
        );
        let expected = load(&path)
            .unwrap()
            .into_values()
            .filter(|b| b.record[1] == "千代田区")
            .min_by_key(|b| sha256_hex(format!("test\0{}", b.id).as_bytes()))
            .unwrap()
            .id;
        assert_eq!(first.iter().filter(|s| s["ref"] == expected).count(), 2);
        let mut reversed = rows.clone();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let refs = first
            .iter()
            .map(|s| s["ref"].as_str().unwrap().to_string())
            .collect();
        let prepared = prepare(&path, &refs).unwrap();
        assert!(
            prepared
                .values()
                .all(|e| e.components.is_empty() && e.radius_floor_km == 0.3)
        );
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        write(&[
            rows[0].clone(),
            rows[0].replace(",0,0,35.673207,", ",2,0,35.673207,"),
        ]);
        let duplicated = sample(&path, "test").unwrap();
        write(&[
            rows[0].replace(",0,0,35.673207,", ",2,0,35.673207,"),
            rows[0].clone(),
        ]);
        assert_eq!(duplicated, sample(&path, "test").unwrap());
        let mut diversified = rows[..3].to_vec();
        for city in ["新宿区", "品川区", "江東区", "渋谷区"] {
            diversified.push(row(city, "一丁目", "1", "1", "1", "0", "35.67"));
        }
        write(&diversified);
        let capped = sample(&path, "test").unwrap();
        assert_eq!(capped.len(), 8);
        assert_eq!(
            capped
                .iter()
                .map(|s| s["ref"].as_str().unwrap())
                .collect::<BTreeSet<_>>()
                .len(),
            4
        );
        write(&[rows[0].replace(",35.673207,139.745444\n", ",139.745444,35.673207\n")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("WGS84")
        );
        write(&[rows[0].replacen("13,", "48,", 1)]);
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
