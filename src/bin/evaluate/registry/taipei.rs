use super::{Entry, Point, Registry};
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

const URL: &str = "https://data.gov.tw/dataset/155472";
const DISTRICTS: &str = include_str!("../../../../tests/registry/taipei-districts.xml");
static NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9]+(?:之[0-9]+)?)號((?:地下)?[一二三四五六七八九十百0-9]+樓.*)?$").unwrap()
});

pub(super) fn digits(text: &str) -> String {
    text.chars()
        .map(|c| {
            if ('０'..='９').contains(&c) {
                char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap()
            } else {
                c
            }
        })
        .collect()
}

#[derive(Clone)]
struct Address {
    id: String,
    district: String,
    street: String,
    area: String,
    lane: String,
    alley: String,
    number: String,
    raw_number: String,
    floor: Option<(String, [f64; 2])>,
    point: [f64; 2],
    ambiguous: bool,
}

impl Address {
    fn category(&self) -> &'static str {
        if self.number.contains('之') {
            "suffix"
        } else if !self.alley.is_empty() {
            "alley"
        } else if !self.lane.is_empty() {
            "lane"
        } else if self.street.contains('段') {
            "section"
        } else {
            "plain"
        }
    }
    fn district_name(&self) -> Option<&'static str> {
        match self.district.as_str() {
            "63000010" => Some("松山區"),
            "63000020" => Some("信義區"),
            "63000030" => Some("大安區"),
            "63000040" => Some("中山區"),
            "63000050" => Some("中正區"),
            "63000060" => Some("大同區"),
            "63000070" => Some("萬華區"),
            "63000080" => Some("文山區"),
            "63000090" => Some("南港區"),
            "63000100" => Some("內湖區"),
            "63000110" => Some("士林區"),
            "63000120" => Some("北投區"),
            _ => None,
        }
    }
    fn query(&self, number: &str) -> String {
        format!(
            "臺北市{}{}{}{}{}{number}, Taiwan",
            self.district_name().unwrap(),
            self.street,
            self.area,
            self.lane,
            self.alley
        )
    }
}

fn load(path: &Path) -> Result<BTreeMap<String, Address>> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut line = String::new();
    ensure!(
        super::ban::record(&mut reader, &mut line)?,
        "empty Taipei CSV"
    );
    let columns: BTreeMap<_, _> = super::nad::split(line.trim_end(), ',')
        .into_iter()
        .enumerate()
        .map(|(i, s)| (s.to_string(), i))
        .collect();
    for name in [
        "省市縣市代碼",
        "鄉鎮市區代碼",
        "街路段",
        "地區",
        "巷",
        "弄",
        "號",
        "橫座標",
        "縱座標",
        "lat",
        "lon",
    ] {
        ensure!(columns.contains_key(name), "Taipei CSV missing {name}");
    }
    let mut rows = BTreeMap::<String, Address>::new();
    let mut count = 0usize;
    while super::ban::record(&mut reader, &mut line)? {
        count += 1;
        let fields = super::nad::split(line.trim_end_matches(['\r', '\n']), ',');
        ensure!(
            fields.len() == columns.len(),
            "Taipei row {count}: column count"
        );
        let field = |name: &str| fields[columns[name]].trim();
        ensure!(field("省市縣市代碼") == "63000", "not a Taipei record");
        let number = digits(field("號"));
        let Some(c) = NUMBER.captures(&number) else {
            continue;
        };
        if field("街路段").is_empty() && field("地區").is_empty() {
            continue;
        }
        if field("lat").is_empty() || field("lon").is_empty() {
            continue;
        }
        let point = [
            field("lat").parse::<f64>().context("Taipei latitude")?,
            field("lon").parse::<f64>().context("Taipei longitude")?,
        ];
        ensure!(
            (24.9..25.3).contains(&point[0]) && (121.4..121.7).contains(&point[1]),
            "Taipei needs local WGS84 coordinates"
        );
        let id = sha256_hex(
            format!(
                "{}\0{}\0{}\0{}\0{}\0{}",
                field("鄉鎮市區代碼"),
                digits(field("街路段")),
                digits(field("地區")),
                digits(field("巷")),
                digits(field("弄")),
                &c[1]
            )
            .as_bytes(),
        );
        let is_floor = c.get(2).is_some();
        let a = Address {
            id: id.clone(),
            district: field("鄉鎮市區代碼").into(),
            street: field("街路段").into(),
            area: field("地區").into(),
            lane: field("巷").into(),
            alley: field("弄").into(),
            number: c[1].into(),
            raw_number: format!("{}號", field("號").split('號').next().unwrap()),
            floor: is_floor.then(|| (field("號").to_owned(), point)),
            point,
            ambiguous: false,
        };
        ensure!(a.district_name().is_some(), "unknown Taipei district");
        if let Some(old) = rows.get_mut(&id) {
            old.ambiguous |= old.point != point;
            if (&a.street, &a.area, &a.lane, &a.alley, &a.raw_number)
                < (
                    &old.street,
                    &old.area,
                    &old.lane,
                    &old.alley,
                    &old.raw_number,
                )
            {
                old.street = a.street;
                old.area = a.area;
                old.lane = a.lane;
                old.alley = a.alley;
                old.raw_number = a.raw_number;
            }
            if let Some(floor) = a.floor
                && old.floor.as_ref().is_none_or(|f| floor.0 < f.0)
            {
                old.floor = Some(floor);
            }
        } else {
            rows.insert(id, a);
        }
    }
    rows.retain(|_, a| !a.ambiguous);
    eprintln!(
        "Taipei: {count} rows, {} unambiguous eligible building keys",
        rows.len()
    );
    Ok(rows)
}

pub fn sample(path: &Path, seed: &str) -> Result<Vec<Value>> {
    let mut pools = BTreeMap::<_, Vec<Address>>::new();
    for a in load(path)?.into_values() {
        pools
            .entry((a.district.clone(), a.category()))
            .or_default()
            .push(a);
    }
    let mut lines = vec![];
    for ((district, category), mut pool) in pools {
        pool.sort_by_key(|a| sha256_hex(format!("{seed}\0{}", a.id).as_bytes()));
        let mut streets = BTreeSet::new();
        for a in pool
            .into_iter()
            .filter(|a| {
                streets.insert((
                    a.street.clone(),
                    a.area.clone(),
                    a.lane.clone(),
                    a.alley.clone(),
                ))
            })
            .take(4)
        {
            let mut queries = vec![("native", a.query(&a.raw_number))];
            let ascii = digits(&queries[0].1);
            if ascii != queries[0].1 {
                queries.push(("ascii_digits", ascii));
            }
            if let Some((floor, point)) = &a.floor
                && *point == a.point
            {
                queries.push(("floor", a.query(floor)));
            }
            for (form, query) in queries {
                lines.push(json!({"kind":"address", "source":"taipei-addresses", "ref":a.id, "cohort":format!("tw_{category}"), "queries":[query], "label":a.query(&a.raw_number), "tags":["registry-sample", "municipal-sample", "tw", district, category, form]}));
            }
        }
    }
    ensure!(!lines.is_empty(), "no eligible Taipei addresses");
    Ok(lines)
}

pub const REGISTRY: Registry = Registry {
    source: "taipei-addresses",
    command: "sample-tw",
    country: "tw",
    input: "taipei",
    license: "Taiwan-Open-Government-Data-License-1.0",
    location: "taipei-building",
    url: |_| URL.into(),
    sample,
    prepare,
};

fn prepare(path: &Path, refs: &BTreeSet<String>) -> Result<BTreeMap<String, Entry>> {
    let metadata = super::provenance(path)?;
    let rows = load(path)?;
    refs.iter().map(|id| {
        let a = rows.get(id).with_context(|| format!("Taipei lacks building {id}"))?;
        let mut provenance = metadata.clone();
        provenance.insert("district_names".into(), json!({
            "name": "代碼服務－鄉鎮市區清單(戶政)",
            "agency": "內政部國土測繪中心",
            "url": "https://api.nlsc.gov.tw/other/ListTown1/A",
            "catalogue": "https://data.gov.tw/dataset/102011",
            "license": REGISTRY.license,
            "retrieved": "2026-10-31",
            "release": "taipei-districts.xml",
            "sha256": sha256_hex(DISTRICTS.as_bytes()),
            "code_field": "towncode",
        }));
        provenance.extend(serde_json::from_value::<serde_json::Map<String, Value>>(json!({"district_code":a.district,"street":a.street,"area":a.area,"lane":a.lane,"alley":a.alley,"house_number":a.raw_number,"floor_address":a.floor.as_ref().map(|f| &f.0),"coordinate_transform":"EPSG:3826 to EPSG:4326, DuckDB spatial / PROJ, always_xy=true","location_key":"SHA256(district, street, area, lane, alley, house number), ASCII digits, NUL separated"}))?);
        Ok((id.clone(), Entry { point: Point::Wgs84(a.point), components: indexmap::IndexMap::from([("house_number", a.number.clone())]), radius_floor_km: 0.15, provenance, number_aliases: None, places: vec![a.district_name().context("unknown Taipei district")?.into()] }))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_district_codes_are_exact_and_supply_reverse_places() {
        let path =
            std::env::temp_dir().join(format!("taipei-districts-{}.csv", std::process::id()));
        let header = "省市縣市代碼,鄉鎮市區代碼,街路段,地區,巷,弄,號,橫座標,縱座標,lat,lon\n";
        let row =
            |code: &str| format!("63000,{code},民生東路三段,,,,１號,306800,2772200,25.05,121.56\n");
        let pattern =
            Regex::new(r"(?s)<towncode>([^<]+)</towncode>.*?<townname>([^<]+)</townname>").unwrap();
        let districts: BTreeMap<_, _> = pattern
            .captures_iter(DISTRICTS)
            .map(|c| (c[1].to_string(), c[2].to_string()))
            .collect();
        assert_eq!(districts.len(), 12);
        assert_ne!(districts["63000010"], districts["63000100"]);
        std::fs::write(
            &path,
            format!(
                "{header}{}",
                districts.keys().map(|c| row(c)).collect::<String>()
            ),
        )
        .unwrap();
        let rows = load(&path).unwrap();
        let prepared = prepare(&path, &rows.keys().cloned().collect()).unwrap();
        assert_eq!(prepared.len(), 12);
        for (id, a) in rows {
            let expected = &districts[&a.district];
            assert_eq!(a.district_name(), Some(expected.as_str()));
            assert_eq!(prepared[&id].places, [expected.clone()]);
            assert_eq!(
                prepared[&id].provenance["district_names"]["code_field"],
                "towncode"
            );
        }
        for code in ["63000130", "063000010", "6300010", "6300100", "A01"] {
            std::fs::write(&path, format!("{header}{}", row(code))).unwrap();
            assert!(
                load(&path)
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("unknown Taipei district")
            );
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn sections_lanes_alleys_and_floors_keep_one_location() {
        let path = std::env::temp_dir().join(format!("taipei-test-{}.csv", std::process::id()));
        let header = "省市縣市代碼,鄉鎮市區代碼,街路段,地區,巷,弄,號,橫座標,縱座標,lat,lon\n";
        let a = "63000,63000010,民生東路三段,,９５巷,２弄,１之２號,306800,2772200,25.05,121.56\n";
        let b =
            "63000,63000010,民生東路三段,,９５巷,２弄,１之２號二樓,306800,2772200,25.05,121.56\n";
        std::fs::write(&path, format!("{header}{a}{b}")).unwrap();
        let first = sample(&path, "test").unwrap();
        assert_eq!(first.len(), 3);
        assert!(first.iter().all(|s| s["ref"] == first[0]["ref"]));
        assert!(
            first[1]["queries"][0]
                .as_str()
                .unwrap()
                .contains("95巷2弄1之2號")
        );
        std::fs::write(&path, format!("{header}{b}{a}")).unwrap();
        assert_eq!(first, sample(&path, "test").unwrap());
        assert_eq!(digits("四段１２號"), "四段12號");
        assert!(!NUMBER.is_match("1號之2"));
        let refs = BTreeSet::from([first[0]["ref"].as_str().unwrap().into()]);
        assert_eq!(
            prepare(&path, &refs)
                .unwrap()
                .values()
                .next()
                .unwrap()
                .components["house_number"],
            "1之2"
        );
        assert!(prepare(&path, &BTreeSet::from(["missing".into()])).is_err());
        let ascii = digits(a);
        std::fs::write(&path, format!("{header}{a}{ascii}{b}")).unwrap();
        let canonical = sample(&path, "test").unwrap();
        std::fs::write(&path, format!("{header}{b}{ascii}{a}")).unwrap();
        assert_eq!(canonical, sample(&path, "test").unwrap());
        let displaced = b.replace("25.05", "25.06");
        std::fs::write(&path, format!("{header}{a}{displaced}")).unwrap();
        assert!(sample(&path, "test").is_err());
        std::fs::remove_file(path).unwrap();
    }
}
