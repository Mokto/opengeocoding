use crate::index::*;
use regex::Regex;

fn province(part: &str) -> bool {
    matches!(
        part.trim().to_lowercase().as_str(),
        "台灣省" | "臺灣省" | "台湾省" | "taiwan province"
    )
}

/// `query` without a part that is Taiwan Province (台灣省), a province in name only: no place of
/// the index has it, and the parser can take it for the town. None when no part is.
pub fn without_taiwan_province(query: &str) -> Option<String> {
    let parts: Vec<&str> = query.split(',').map(str::trim).collect();
    let kept: Vec<&str> = parts.iter().filter(|p| !province(p)).copied().collect();
    (kept.len() < parts.len() && !kept.is_empty()).then(|| kept.join(", "))
}

pub fn taiwan(country: &str) -> bool {
    matches!(country, "taiwan" | "臺灣" | "台灣" | "台湾")
}

/// "35號 溪海路153巷, 大園區, 桃園市, Taiwan", OpenStreetMap's way of writing a Taiwanese
/// address: the number and its road (or village) in either order, then the township or district,
/// its county or city, 台灣省 and a postcode, each its own part. libpostal reads across the
/// commas: it folds the road into a `house`, cuts it to one character ("霧" of 霧臺鄉), takes
/// 大園區 for a floor and 台灣省 for the town, and splits the 建 off "建51之21號". The parts say
/// what each is. None for any other shape.
pub(crate) fn aligned(head: &str, places: &[&str]) -> Option<Vec<(String, String)>> {
    static NUMBER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^建?[0-9]+(?:之[0-9]+)?[號号]?$").unwrap());
    let cjk = |w: &str| {
        w.chars().any(|c| c as u32 >= 0x3040)
            && w.chars().all(|c| c as u32 >= 0x3040 || c.is_ascii_digit())
    };
    let pair = |kind: &str, value: &str| (kind.to_string(), value.to_string());
    let words: Vec<&str> = head.split_whitespace().collect();
    let [first, second] = words[..] else {
        return None;
    };
    let mut parsed = match (NUMBER.is_match(first), NUMBER.is_match(second)) {
        (true, false) if cjk(second) => vec![pair("house_number", first), pair("road", second)],
        (false, true) if cjk(first) => vec![pair("road", first), pair("house_number", second)],
        _ => return None,
    };
    let mut towns = vec![];
    let mut postcode = None;
    for part in places.iter().map(|p| p.trim()) {
        if province(part) {
            continue;
        }
        if postcode.is_none()
            && (3..=6).contains(&part.len())
            && part.bytes().all(|b| b.is_ascii_digit())
        {
            postcode = Some(part);
        } else if cjk(part)
            && !part.contains(|c: char| c.is_ascii_digit())
            && part.ends_with(['鄉', '郷', '鎮', '市', '區', '縣'])
        {
            towns.push(part);
        } else {
            return None;
        }
    }
    match towns[..] {
        [town] if town.ends_with('縣') => parsed.push(pair("state_district", town)),
        [town] => parsed.push(pair("city", town)),
        [town, above] if above.ends_with('縣') => {
            parsed.extend([pair("city", town), pair("state_district", above)])
        }
        // the data files some of a municipality's houses under no district
        [town, above] => parsed.extend([pair("suburb", town), pair("state", above)]),
        _ => return None,
    }
    parsed.extend(postcode.map(|p| pair("postcode", p)));
    Some(parsed)
}

impl Index {
    /// "386號 佳崙, 太麻里鄉, 台東縣, Taiwan": a road, or a village name the houses are numbered
    /// on, that runs across a township line. The stated township's street (at `at`) lacks the
    /// number, which a namesake street within 5 km in the same county (`region`, its ISO 3166-2
    /// code) holds: that house, when it is the only one. The township contradicts it, so the
    /// caller does not answer it confidently.
    pub fn house_across_township(
        &self,
        road: &str,
        number: &str,
        (lat, lon): (f64, f64),
        region: &str,
    ) -> Option<Value> {
        let road_key = fold(road);
        let houses: Vec<Value> = self
            .houses_numbered(
                &[schema::number_key(number)],
                "",
                road,
                true,
                Some("tw"),
                (lat, lon),
                |name| fold(name) == road_key,
            )
            .into_iter()
            .filter(|h| h["address"]["ISO3166-2-lvl4"] == region)
            .collect();
        let point = |h: &Value| -> Option<(f64, f64)> {
            Some((
                h["lat"].as_str()?.parse().ok()?,
                h["lon"].as_str()?.parse().ok()?,
            ))
        };
        let first = point(houses.first()?)?;
        let near_first = |h: &Value| {
            point(h).is_some_and(|(y, x)| {
                ((x - first.1) * y.to_radians().cos()).hypot(y - first.0) * 111_195.0 < 200.0
            })
        };
        houses.iter().all(near_first).then(|| houses[0].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osm_addresses_align_by_their_commas() {
        let read = |head: &str, places: &[&str]| {
            aligned(head, places).map(|a| {
                a.into_iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
        };
        assert_eq!(
            read("溪海路153巷 35號", &["大園區", "桃園市"]).as_deref(),
            Some("road=溪海路153巷 house_number=35號 suburb=大園區 state=桃園市")
        );
        assert_eq!(
            read("建51之21號 番婆街", &["福興鄉", "彰化縣", "台灣省"]).as_deref(),
            Some("house_number=建51之21號 road=番婆街 city=福興鄉 state_district=彰化縣")
        );
        assert_eq!(
            read("118號 鎮樂", &["卑南郷", "台東縣", "台灣省", "954104"]).as_deref(),
            Some("house_number=118號 road=鎮樂 city=卑南郷 state_district=台東縣 postcode=954104")
        );
        assert_eq!(
            read("員山路一段510巷1弄 9", &["員山鄉", "宜蘭縣", "台灣省"]).as_deref(),
            Some("road=員山路一段510巷1弄 house_number=9 city=員山鄉 state_district=宜蘭縣")
        );
        for (head, places) in [
            ("溪海路153巷35號", &["大園區", "桃園市"][..]),
            ("35號 溪海路153巷", &["Dayuan", "桃園市"]),
            ("35 Xihai Road", &["大園區", "桃園市"]),
            ("35號 36號", &["大園區", "桃園市"]),
            ("35號 溪海路", &["台灣省"]),
            ("35號 溪海路", &["大園區", "中壢區", "桃園市"]),
        ] {
            assert_eq!(read(head, places), None, "{head}");
        }
    }
}
