use super::{Entry, Point, Registry};
use anyhow::{Context, Result, ensure};
use opengeocoding::eval::sha256_hex;
use regex::Regex;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::Path,
    sync::LazyLock,
};

pub const URL: &str = "https://www.data.go.kr/data/15021148/standard.do";
pub const RIGHTS: &str = "https://www.data.go.kr/biz/dcat/metadata/15021148.do";
pub const LICENSE: &str = "Korea-Public-Data-Unrestricted";
static ROAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+\S) ([1-9]\d*(?:-\d+)?)$").unwrap());
static LOT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(.+?\S) (산\s*)?([1-9]\d*(?:-\d+)?)$").unwrap());

#[derive(Clone, Deserialize)]
pub struct Address {
    #[serde(rename = "SCHOOL_ID")]
    pub id: String,
    #[serde(rename = "RDNMADR")]
    pub road: String,
    #[serde(rename = "LNMADR")]
    pub lot: String,
    #[serde(rename = "LATITUDE")]
    lat: Option<String>,
    #[serde(rename = "LONGITUDE")]
    lon: Option<String>,
    #[serde(rename = "CDDC_CODE")]
    authority: u32,
    #[serde(rename = "REFERENCE_DATE")]
    pub reference_date: String,
    #[serde(rename = "OPER_STTUS")]
    status: String,
}

impl Address {
    pub fn point(&self) -> Option<[f64; 2]> {
        let lat = self.lat.as_ref()?.parse::<f64>().ok()?;
        let lon = self.lon.as_ref()?.parse::<f64>().ok()?;
        ((32. ..=39.).contains(&lat) && (124. ..=132.).contains(&lon)).then_some([lat, lon])
    }

    pub fn road_number(&self) -> Option<String> {
        Some(ROAD.captures(self.road.trim())?[2].into())
    }

    pub fn road_name(&self) -> Option<String> {
        Some(
            ROAD.captures(self.road.trim())?[1]
                .split_whitespace()
                .last()?
                .into(),
        )
    }

    fn lot_number(&self) -> Option<String> {
        let captures = LOT.captures(self.lot.trim())?;
        Some(format!(
            "{}{}",
            if captures.get(2).is_some() { "산" } else { "" },
            &captures[3]
        ))
    }

    fn category(&self) -> &'static str {
        if self.lot_number().is_some_and(|n| n.starts_with('산')) {
            "mountain_lot"
        } else if self.road_number().is_some_and(|n| n.contains('-')) {
            "road_hyphen"
        } else {
            "plain"
        }
    }

    fn number_aliases(&self) -> Result<Option<BTreeSet<String>>> {
        let numbers = BTreeSet::from([
            self.road_number().context("school road number")?,
            self.lot_number().context("school land-lot number")?,
        ]);
        Ok((numbers.len() > 1).then_some(numbers))
    }
}

fn load(path: &Path) -> Result<Vec<Address>> {
    let mut rows: Vec<Address> = serde_json::from_reader(BufReader::new(File::open(path)?))?;
    let total = rows.len();
    let mut ids = BTreeSet::new();
    for row in &rows {
        ensure!(
            row.id.len() == 10
                && row.id.starts_with('B')
                && row.id[1..].bytes().all(|b| b.is_ascii_digit()),
            "invalid school id {}",
            row.id
        );
        ensure!(ids.insert(row.id.clone()), "duplicate school id {}", row.id);
        ensure!(
            row.authority > 0 && !row.reference_date.is_empty(),
            "school {} lacks authority or reference date",
            row.id
        );
    }
    rows.retain(|a| {
        a.status == "운영"
            && a.point().is_some()
            && a.road_number().is_some()
            && a.lot_number().is_some()
    });
    eprintln!("Korean schools: {} of {total} rows eligible", rows.len());
    Ok(rows)
}

pub fn lookup(path: &Path, ids: &BTreeSet<String>) -> Result<BTreeMap<String, Address>> {
    let found: BTreeMap<_, _> = load(path)?
        .into_iter()
        .filter(|a| ids.contains(&a.id))
        .map(|a| (a.id.clone(), a))
        .collect();
    ensure!(
        found.len() == ids.len(),
        "school snapshot lacks requested eligible ids"
    );
    Ok(found)
}

fn key(seed: &str, id: &str) -> String {
    sha256_hex(format!("{seed}\0{id}").as_bytes())
}

/// Each school's address in three forms: the full one, country first and a short province.
/// Road addresses lead with the English country, lot addresses with the native one; the other
/// combinations share these parse paths and are covered by the synthetic world.
fn queries(address: &str, lot: bool) -> Vec<(&'static str, String)> {
    let address = address.trim();
    let mut found = vec![
        ("", format!("{address}, South Korea")),
        if lot {
            ("_country_first_native", format!("대한민국, {address}"))
        } else {
            ("_country_first", format!("South Korea, {address}"))
        },
    ];
    if let Some((province, rest)) = address.split_once(' ') {
        let short = match province {
            "서울특별시" => "서울",
            "부산광역시" => "부산",
            "대구광역시" => "대구",
            "인천광역시" => "인천",
            "광주광역시" => "광주",
            "대전광역시" => "대전",
            "울산광역시" => "울산",
            "세종특별자치시" => "세종",
            "경기도" => "경기",
            "강원특별자치도" => "강원",
            "충청북도" => "충북",
            "충청남도" => "충남",
            "전북특별자치도" | "전라북도" => "전북",
            "전라남도" => "전남",
            "경상북도" => "경북",
            "경상남도" => "경남",
            "제주특별자치도" => "제주",
            _ => return found,
        };
        found.push(("_short_domestic", format!("{short} {rest}")));
    }
    found
}

/// The index reads these Korean registers (`data/kr-localdata-lots.tsv.gz`), which list most
/// schools with their own coordinates, so a school they hold measures the register, not the geocoder.
static REGISTER: LazyLock<BTreeMap<String, Vec<[f64; 2]>>> = LazyLock::new(|| {
    use std::io::Read;
    let mut text = String::new();
    flate2::read::GzDecoder::new(&include_bytes!("../../../../data/kr-localdata-lots.tsv.gz")[..])
        .read_to_string(&mut text)
        .expect("Korean register data");
    let mut found: BTreeMap<String, Vec<[f64; 2]>> = BTreeMap::new();
    for line in text.lines().skip(1) {
        let mut cells = line.split('\t');
        let (Some(address), Some(lon), Some(lat)) = (cells.next(), cells.next(), cells.next())
        else {
            continue;
        };
        if let (Ok(lon), Ok(lat)) = (lon.parse(), lat.parse()) {
            found.entry(tail(address)).or_default().push([lat, lon]);
        }
    }
    found
});

/// The last two words: the road or lot name and its number.
fn tail(address: &str) -> String {
    let words: Vec<_> = address.split_whitespace().collect();
    words[words.len().saturating_sub(2)..].join(" ")
}

impl Address {
    /// A register row with the same road or lot and number within about 300 m of the school's point.
    /// ponytail: a coarse box (0.003 degrees) and the last two words; it can miss a register row
    /// that writes the address differently, and an unlisted school may still be an input elsewhere
    /// (Overture places, the shop register).
    fn in_register(&self) -> bool {
        let Some([lat, lon]) = self.point() else {
            return false;
        };
        [&self.road, &self.lot].iter().any(|address| {
            REGISTER.get(&tail(address)).is_some_and(|points| {
                points
                    .iter()
                    .any(|p| (p[0] - lat).abs() < 0.003 && (p[1] - lon).abs() < 0.003)
            })
        })
    }
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    draw(path, seed, REGISTRY.source, |_| true)
}

/// The same draw over the schools the Korean registers do not list, leaving out those whose
/// address `tests/registry/kr.seeds.jsonl` already asks, so the two corpora share no query.
pub fn sample_unlisted(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let asked: BTreeSet<String> = include_str!("../../../../tests/registry/kr.seeds.jsonl")
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .flat_map(|seed| seed["queries"].as_array().cloned().unwrap_or_default())
        .filter_map(|query| query.as_str().map(String::from))
        .collect();
    draw(path, seed, UNLISTED_REGISTRY.source, |a| {
        !a.in_register()
            && ![(&a.road, false), (&a.lot, true)]
                .iter()
                .any(|(address, lot)| {
                    queries(address, *lot)
                        .iter()
                        .any(|(_, q)| asked.contains(q))
                })
    })
}

fn draw(
    path: &Path,
    seed: &str,
    source: &str,
    keep: impl Fn(&Address) -> bool,
) -> Result<Vec<Value>> {
    let release = path
        .file_name()
        .and_then(|s| s.to_str())
        .context("school snapshot file name")?;
    let mut pools: BTreeMap<(u32, &'static str), BTreeMap<String, Address>> = BTreeMap::new();
    for a in load(path)?.into_iter().filter(|a| keep(a)) {
        let pool = pools.entry((a.authority, a.category())).or_default();
        let hash = key(seed, &a.id);
        // ponytail: street diversity is limited to 200 candidates per authority and number class; enlarge the pool if it cannot supply five streets.
        if pool.len() < 200 || pool.last_key_value().is_some_and(|(k, _)| hash < *k) {
            pool.insert(hash, a);
            if pool.len() > 200 {
                pool.pop_last();
            }
        }
    }
    let mut lines = vec![];
    let mut roads = BTreeSet::new();
    let mut lots = BTreeSet::new();
    for ((authority, category), pool) in pools {
        let mut streets = BTreeSet::new();
        let selected: Vec<_> = pool
            .values()
            .filter(|a| {
                let prefix = ROAD.captures(a.road.trim()).unwrap()[1].to_string();
                if streets.contains(&prefix)
                    || roads.contains(a.road.trim())
                    || lots.contains(a.lot.trim())
                {
                    return false;
                }
                streets.insert(prefix);
                roads.insert(a.road.trim().to_string());
                lots.insert(a.lot.trim().to_string());
                true
            })
            .take(5)
            .collect();
        eprintln!(
            "Korean schools {authority} {category}: {} locations",
            selected.len()
        );
        for a in selected {
            for (form, address) in [("road", &a.road), ("lot", &a.lot)] {
                for (suffix, query) in queries(address, form == "lot") {
                    lines.push(json!({"kind":"address", "source":source, "ref":a.id, "cohort":format!("kr_{form}{suffix}"), "radius_km":0.25, "queries":[query], "label":a.road, "tags":["registry-sample", "school-sample", "kr", release, category, form, format!("education-authority:{authority}")]}));
                }
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible Korean schools");
    Ok(lines)
}

pub const UNLISTED_REGISTRY: Registry = Registry {
    source: "korean-schools-unlisted",
    command: "sample-kr-unlisted",
    input: "schools-unlisted",
    sample: sample_unlisted,
    ..REGISTRY
};

pub const REGISTRY: Registry = Registry {
    source: "korean-schools",
    command: "sample-kr",
    country: "kr",
    input: "schools",
    license: LICENSE,
    location: "korean-schools",
    url: |id| format!("{URL}#{id}"),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    lookup(path, refs)?
        .into_iter()
        .map(|(id, row)| {
            let number_aliases = row.number_aliases()?;
            let mut metadata = metadata.clone();
            metadata.insert("rights_url".into(), json!(RIGHTS));
            metadata.insert("rights_statement".into(), json!("이용허락범위 제한 없음"));
            metadata.insert("reference_date".into(), json!(row.reference_date));
            metadata.insert("road_address".into(), json!(row.road));
            metadata.insert("land_lot_address".into(), json!(row.lot));
            Ok((
                id,
                Entry {
                    point: Point::Wgs84(row.point().context("school point")?),
                    components: indexmap::IndexMap::from([
                        (
                            "house_number",
                            row.road_number().context("school road number")?,
                        ),
                        ("road", row.road_name().context("school road name")?),
                    ]),
                    radius_floor_km: 0.25,
                    provenance: metadata.clone(),
                    number_aliases,
                    // deunicode spells Hangul unlike the Revised Romanization the index's
                    // names use ("천안시" as "ceonansi", not "Cheonan-si"), so names can't be compared
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
    fn both_registry_address_forms_keep_one_point_and_distinct_number_aliases() {
        let path =
            std::env::temp_dir().join(format!("korean-schools-test-{}.json", std::process::id()));
        let a = json!({"SCHOOL_ID":"B000000001", "RDNMADR":"서울특별시 종로구 새길 12-3", "LNMADR":"서울특별시 종로구 옛동 산 45-6", "LATITUDE":"37.57", "LONGITUDE":"126.98", "CDDC_CODE":7010000, "REFERENCE_DATE":"2026-10-01", "OPER_STTUS":"운영"});
        let mut b = a.clone();
        b["SCHOOL_ID"] = json!("B000000002");
        b["RDNMADR"] = json!("서울특별시 종로구 다른길 7");
        b["LNMADR"] = json!("서울특별시 종로구 다른동 8");
        let write =
            |rows: &[Value]| std::fs::write(&path, serde_json::to_vec(rows).unwrap()).unwrap();
        let mut co_located = b.clone();
        co_located["SCHOOL_ID"] = json!("B000000003");
        co_located["LNMADR"] = json!("서울특별시 종로구 다른동 산8");
        write(&[a.clone(), b.clone(), co_located.clone()]);
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 12);
        assert_eq!(
            first
                .iter()
                .filter(|s| ["kr_road", "kr_lot"].iter().any(|c| s["cohort"] == *c))
                .count(),
            4
        );
        assert_eq!(
            queries("서울특별시 종로구 새길 12-3", false),
            vec![
                ("", "서울특별시 종로구 새길 12-3, South Korea".into()),
                (
                    "_country_first",
                    "South Korea, 서울특별시 종로구 새길 12-3".into()
                ),
                ("_short_domestic", "서울 종로구 새길 12-3".into()),
            ]
        );
        assert_eq!(
            queries("서울특별시 종로구 옛동 45", true)[1],
            (
                "_country_first_native",
                "대한민국, 서울특별시 종로구 옛동 45".into()
            )
        );
        assert_eq!(queries("전남광주통합특별시 가상구 새길 1", false).len(), 2);
        write(&[co_located, b.clone(), a.clone()]);
        assert_eq!(first, sample(&path, "test").unwrap());
        let ids = BTreeSet::from(["B000000001".into()]);
        let found = lookup(&path, &ids).unwrap();
        let row = &found["B000000001"];
        assert_eq!(row.point(), Some([37.57, 126.98]));
        assert_eq!(row.road_number().as_deref(), Some("12-3"));
        assert_eq!(row.lot_number().as_deref(), Some("산45-6"));
        assert_eq!(row.category(), "mountain_lot");
        let mut plain: Address = serde_json::from_value(b.clone()).unwrap();
        assert_eq!(plain.category(), "plain");
        plain.road = row.road.clone();
        assert_eq!(plain.category(), "road_hyphen");
        let expect = crate::registry::with_number_aliases(
            json!({"country":["kr"],"level":"house","max_level":"house","point":row.point(),"radius_km":0.25,"good_match":true,"components":{"house_number":"12-3"}}),
            &row.number_aliases().unwrap(),
        );
        let expect: crate::score::Expect = serde_json::from_value(expect).unwrap();
        for number in ["12-3", "산45-6", "45-6", "999"] {
            let response = json!({"displayName":"x","geoCoordinates":{"lat":37.57,"lon":126.98},"addressDetails":{"countryCode":"kr","road":"새길","houseNumber":number},"goodMatch":true});
            assert_eq!(
                crate::score::score(Some(&expect), &response, true).class == "pass",
                ["12-3", "산45-6"].contains(&number)
            );
        }
        assert!(lookup(&path, &BTreeSet::from(["B999999999".into()])).is_err());
        write(&[a.clone(), a.clone()]);
        assert!(lookup(&path, &ids).is_err());
        let mut bad = a;
        bad["LATITUDE"] = json!("NaN");
        write(&[bad]);
        assert!(sample(&path, "test").is_err());
        b["LNMADR"] = json!("");
        write(&[b]);
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn unlisted_sample_skips_schools_the_register_holds() {
        let (first, lon, lat) = {
            let (address, points) = REGISTER.iter().next().unwrap();
            (address.clone(), points[0][1], points[0][0])
        };
        let listed = json!({"SCHOOL_ID":"B000000001", "RDNMADR":format!("서울특별시 종로구 {first}"), "LNMADR":"서울특별시 종로구 옛동 산 45-6", "LATITUDE":lat.to_string(), "LONGITUDE":lon.to_string(), "CDDC_CODE":7010000, "REFERENCE_DATE":"2026-10-01", "OPER_STTUS":"운영"});
        let mut unlisted = listed.clone();
        unlisted["SCHOOL_ID"] = json!("B000000002");
        unlisted["RDNMADR"] = json!("서울특별시 종로구 새길 12-3");
        unlisted["LATITUDE"] = json!("37.57");
        unlisted["LONGITUDE"] = json!("126.98");
        let rows: Vec<Address> = [listed, unlisted]
            .into_iter()
            .map(|v| serde_json::from_value(v).unwrap())
            .collect();
        assert!(rows[0].in_register());
        assert!(!rows[1].in_register());
    }
}
