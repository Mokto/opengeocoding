use super::{Entry, Point, Registry, taipei::digits};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
    sync::LazyLock,
};

const URL: &str = "https://data.tainan.gov.tw/Resource/af44f904-2f4c-49b2-aaf8-1a64dce09bd4";
const DISTRICTS: [&str; 37] = [
    "新營區",
    "鹽水區",
    "白河區",
    "柳營區",
    "後壁區",
    "東山區",
    "麻豆區",
    "下營區",
    "六甲區",
    "官田區",
    "大內區",
    "佳里區",
    "學甲區",
    "西港區",
    "七股區",
    "將軍區",
    "北門區",
    "新化區",
    "善化區",
    "新市區",
    "安定區",
    "山上區",
    "玉井區",
    "楠西區",
    "南化區",
    "左鎮區",
    "仁德區",
    "歸仁區",
    "關廟區",
    "龍崎區",
    "永康區",
    "東區",
    "南區",
    "北區",
    "安南區",
    "安平區",
    "中西區",
];
static NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[1-9][0-9]{0,4}(?:之[1-9][0-9]{0,3})?號$").unwrap());

#[derive(Clone)]
struct Address {
    id: String,
    district: String,
    district_name: &'static str,
    road: [String; 4],
    raw_number: String,
    number: String,
    point: [f64; 2],
    record: Value,
    ambiguous: bool,
}

impl Address {
    fn category(&self) -> &'static str {
        if self.number.contains('之') {
            "suffix"
        } else if !self.road[3].is_empty() {
            "alley"
        } else if !self.road[2].is_empty() {
            "lane"
        } else if self.road[0].contains('段') {
            "section"
        } else if self.road[0].is_empty() {
            "area"
        } else {
            "plain"
        }
    }

    fn query(&self) -> String {
        format!(
            "臺南市{}{}{}, Taiwan",
            self.district_name,
            self.road.concat(),
            self.raw_number
        )
    }
}

fn district_name(code: &str) -> Option<&'static str> {
    // This export uses 6700100 for 新營區, not the official table's eight-digit 67000010.
    let n = code.strip_prefix("670")?.strip_suffix("00")?;
    (n.len() == 2 && n.bytes().all(|b| b.is_ascii_digit()))
        .then(|| n.parse::<usize>().ok()?.checked_sub(1))
        .flatten()
        .and_then(|i| DISTRICTS.get(i).copied())
}

fn load(path: &Path) -> Result<BTreeMap<String, Address>> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        super::ban::record(&mut reader, &mut line)?,
        "empty Tainan CSV"
    );
    let columns: BTreeMap<_, _> =
        super::nad::split(line.trim_end().trim_start_matches('\u{feff}'), ',')
            .into_iter()
            .enumerate()
            .map(|(i, s)| (s.to_string(), i))
            .collect();
    for name in [
        "縣市別代碼",
        "地址-行政區域代碼",
        "村里",
        "鄰",
        "街路段",
        "地區",
        "巷",
        "弄",
        "號",
        "橫座標[E 坐標]",
        "縱座標[N 坐標]",
        "lat",
        "lon",
    ] {
        ensure!(columns.contains_key(name), "Tainan CSV missing {name}");
    }
    let (mut count, mut eligible) = (0usize, 0usize);
    let mut rows = BTreeMap::<String, Address>::new();
    while super::ban::record(&mut reader, &mut line)? {
        count += 1;
        let fields = super::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == columns.len(),
            "Tainan row {count}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        ensure!(field("縣市別代碼") == "67000", "not a Tainan record");
        let district = field("地址-行政區域代碼");
        let district_name = district_name(district).context("unknown Tainan district code")?;
        let number = digits(field("號"));
        if !NUMBER.is_match(&number) || field("街路段").is_empty() && field("地區").is_empty()
        {
            continue;
        }
        let easting = field("橫座標[E 坐標]")
            .parse::<f64>()
            .context("Tainan easting")?;
        let northing = field("縱座標[N 坐標]")
            .parse::<f64>()
            .context("Tainan northing")?;
        if !(150_000. ..220_000.).contains(&easting)
            || !(2_530_000. ..2_600_000.).contains(&northing)
            || field("lat").is_empty()
            || field("lon").is_empty()
        {
            continue;
        }
        let point = [
            field("lat").parse::<f64>().context("Tainan latitude")?,
            field("lon").parse::<f64>().context("Tainan longitude")?,
        ];
        ensure!(
            (22.8..23.5).contains(&point[0]) && (120.0..120.7).contains(&point[1]),
            "Tainan needs local WGS84 coordinates from tests/registry/tainan-csv.sql"
        );
        eligible += 1;
        let road = ["街路段", "地區", "巷", "弄"].map(|c| field(c).to_string());
        let id = sha256_hex(
            format!(
                "{district}\0{}\0{}\0{}\0{}\0{number}",
                digits(&road[0]),
                digits(&road[1]),
                digits(&road[2]),
                digits(&road[3])
            )
            .as_bytes(),
        );
        let a = Address {
            id: id.clone(),
            district: district.into(),
            district_name,
            road,
            raw_number: field("號").into(),
            number: number.trim_end_matches('號').into(),
            point,
            record: json!(
                columns
                    .keys()
                    .map(|c| (c.clone(), json!(field(c))))
                    .collect::<serde_json::Map<_, _>>()
            ),
            ambiguous: false,
        };
        if let Some(old) = rows.get_mut(&id) {
            let ambiguous = old.ambiguous || old.point != a.point;
            if a.record
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k, v.as_str()))
                .cmp(
                    old.record
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(k, v)| (k, v.as_str())),
                )
                .is_lt()
            {
                *old = a;
            }
            old.ambiguous = ambiguous;
        } else {
            rows.insert(id, a);
        }
    }
    let groups = rows.len();
    rows.retain(|_, a| !a.ambiguous);
    eprintln!(
        "Tainan: {count} rows, {eligible} eligible rows, {groups} building keys, {} unambiguous",
        rows.len()
    );
    Ok(rows)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut picks = BTreeMap::<_, (String, Address)>::new();
    for a in load(path)?.into_values() {
        let hash = sha256_hex(format!("{seed}\0{}", a.id).as_bytes());
        let key = (a.district.clone(), a.category());
        if picks.get(&key).is_none_or(|(best, _)| hash < *best) {
            picks.insert(key, (hash, a));
        }
    }
    let mut lines = vec![];
    for ((district, category), (_, a)) in picks {
        let query = a.query();
        let ascii = digits(&query);
        let mut queries = vec![("native", query)];
        if ascii != queries[0].1 {
            queries.push(("ascii_digits", ascii));
        }
        for (form, query) in queries {
            lines.push(json!({"kind":"address", "source":"tainan-addresses", "ref":a.id, "cohort":format!("tw_tainan_{category}"), "queries":[query], "label":a.query(), "extra_regions":["TW-TNN"], "tags":["registry-sample", "municipal-sample", "tw", "tainan", district, category, form]}));
        }
    }
    ensure!(!lines.is_empty(), "no eligible Tainan addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "tainan-addresses",
    command: "sample-tainan",
    country: "tw",
    input: "tainan",
    license: "Taiwan-Open-Government-Data-License-1.0",
    location: "tainan-building",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    let rows = load(path)?;
    refs.iter().map(|id| {
        let a = rows.get(id).with_context(|| format!("Tainan lacks building {id}"))?;
        let mut provenance = metadata.clone();
        provenance.insert("record".into(), a.record.clone());
        provenance.insert("coordinate_transform".into(), json!("EPSG:3826 to EPSG:4326, DuckDB spatial / PROJ, always_xy=true"));
        provenance.insert("attribution".into(), json!("臺南市政府民政局, 2025 (民國114年), 臺南市門牌坐標資料; Open Government Data License v1, https://data.gov.tw/license"));
        provenance.insert("location_key".into(), json!("SHA256(district, street, area, lane, alley, house number including 號), ASCII digits, NUL separated"));
        Ok((id.clone(), Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.number.clone())]), radius_floor_km: 0.3, provenance, number_aliases: None, places: vec![] }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_numbers_strata_duplicates_and_axes() {
        let path = std::env::temp_dir().join(format!("tainan-test-{}.csv", std::process::id()));
        let header = "縣市別代碼,地址-行政區域代碼,村里,鄰,街路段,地區,巷,弄,號,橫座標[E 坐標],縱座標[N 坐標],lat,lon\n";
        let row = |street: &str, area: &str, lane: &str, alley: &str, n: &str, lat: &str| {
            format!(
                "67000,6700100,三仙里,17,{street},{area},{lane},{alley},{n},179642,2578414,{lat},120.31\n"
            )
        };
        let rows = [
            row("三民路", "", "", "", "９７之３號", "23.3"),
            row("三民路", "", "", "", "９７號之３", "23.3"),
            row("三民路", "", "", "", "９７號二樓", "23.3"),
            row("民治路", "", "", "", "３６號", "23.3"),
            row("民治路", "", "", "", "３８號", "23.3"),
            row("民生路一段", "", "", "", "１號", "23.3"),
            row("三民路", "", "１巷", "", "２號", "23.3"),
            row("三民路", "", "１巷", "２弄", "３號", "23.3"),
            row("", "五間厝", "", "", "４號", "23.3"),
            row("民生路", "", "", "", "８號", "23.3"),
            row("民生路", "", "", "", "８號", "23.4"),
            row("民生路", "", "", "", "０號", "23.3"),
        ];
        let write =
            |rows: &[String]| std::fs::write(&path, format!("{header}{}", rows.concat())).unwrap();
        write(&rows);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 12);
        let rows_by_id = load(&path).unwrap();
        let expected_plain = rows_by_id
            .values()
            .filter(|a| a.category() == "plain")
            .min_by_key(|a| sha256_hex(format!("test\0{}", a.id).as_bytes()))
            .unwrap();
        assert!(
            first
                .iter()
                .filter(|s| s["cohort"] == "tw_tainan_plain")
                .all(|s| s["ref"] == expected_plain.id)
        );
        assert_eq!(
            first
                .iter()
                .map(|s| s["ref"].as_str().unwrap())
                .collect::<BTreeSet<_>>()
                .len(),
            6
        );
        assert!(
            first
                .iter()
                .all(|s| s["extra_regions"] == json!(["TW-TNN"]))
        );
        assert!(
            first
                .iter()
                .any(|s| s["queries"][0] == "臺南市新營區三民路97之3號, Taiwan")
        );
        assert!(
            !first
                .iter()
                .any(|s| s["queries"][0].as_str().unwrap().contains("號之"))
        );
        let mut reversed = rows.clone();
        reversed.reverse();
        write(&reversed);
        assert_eq!(first, sample(&path, "test").unwrap());
        let refs: BTreeSet<_> = first
            .iter()
            .map(|s| s["ref"].as_str().unwrap().to_string())
            .collect();
        let prepared = prepare(&path, &refs).unwrap();
        assert!(
            prepared
                .values()
                .any(|a| a.components["house_number"] == "97之3")
        );
        assert!(prepared.values().all(|a| a.radius_floor_km == 0.3));
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        let ascii = digits(&rows[0]);
        write(&[rows[0].clone(), ascii.clone()]);
        let mixed = sample(&path, "test").unwrap();
        write(&[ascii, rows[0].clone()]);
        assert_eq!(mixed, sample(&path, "test").unwrap());
        assert_eq!(district_name("6700100"), Some("新營區"));
        assert_eq!(district_name("6703700"), Some("中西區"));
        assert_eq!(district_name("67000010"), None);
        assert_eq!(district_name("6703800"), None);
        assert_eq!(district_name("6700000"), None);
        write(&[rows[0].replace("179642,2578414", "3422483,9441669")]);
        assert!(sample(&path, "test").is_err());
        write(&[rows[0].replace("23.3,120.31", "120.31,23.3")]);
        assert!(
            sample(&path, "test")
                .unwrap_err()
                .to_string()
                .contains("WGS84")
        );
        std::fs::remove_file(path).unwrap();
    }
}
