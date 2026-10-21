use crate::index::suggest::*;
use crate::index::*;

/// (administrative units, road, number) of a Chinese road address, the units most general
/// first: "上海市虹口区中山北一路295号" -> (["上海市", "虹口区"], "中山北一路", "295").
pub(crate) fn chinese(q: &str) -> Option<(Vec<String>, String, String)> {
    static NUMBER: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
                r"([0-9]+(?:之[0-9]+)?)(?:[至-][0-9]+)?[号號](?:(?:地下)?[一二三四五六七八九十百0-9]+[楼樓])?$",
            )
            .unwrap()
    });
    let part = q.split([',', '，']).find(|p| p.contains(['号', '號']))?;
    let t: String = part
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            _ => c,
        })
        .collect();
    let m = NUMBER.captures(&t)?;
    let before = &t[..m.get(0)?.start()];
    let before = before.trim_start_matches(|c: char| c.is_ascii_digit());
    let before = ["中国", "中國", "臺灣", "台灣", "台湾"]
        .iter()
        .fold(before, |b, p| b.strip_prefix(p).unwrap_or(b));
    if before.is_empty()
        || !before
            .chars()
            .all(|c| c as u32 >= 0x3040 || c.is_ascii_digit())
    {
        return None;
    }
    let mut units = vec![];
    let mut start = 0;
    for (i, c) in before.char_indices() {
        if i > start && "省市区區县縣镇鎮乡鄉".contains(c) {
            units.push(before[start..i + c.len_utf8()].to_string());
            start = i + c.len_utf8();
        }
    }
    let road = &before[start..];
    if road.chars().count() < 2 {
        return None;
    }
    Some((units, road.to_string(), m[1].to_string()))
}

/// "中正路63巷 22號, 甲仙區, 高雄市": libpostal reads a Taiwanese or Chinese number written
/// after its road, apart from it, as part of the road, or after a village's name as a unit. A
/// word of digits ending in 號/号 ("number") is a house number, nothing else.
pub(crate) fn number_from_cjk_road(parsed: &mut Vec<(String, String)>) {
    static NUMBER: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[0-9０-９]+(?:之[0-9０-９]+)?[號号]$").unwrap());
    if parsed.iter().any(|(k, _)| k == "house_number") {
        return;
    }
    // "佳崙 386號": a number after a village's name reads as a unit
    if let Some(unit) = parsed
        .iter_mut()
        .find(|(k, v)| k == "unit" && NUMBER.is_match(v))
    {
        unit.0 = "house_number".into();
        return;
    }
    let Some(at) = parsed.iter().position(|(k, _)| k == "road") else {
        return;
    };
    let road = &parsed[at].1;
    let split = match (road.split_once(' '), road.rsplit_once(' ')) {
        (_, Some((rest, number))) if NUMBER.is_match(number) => Some((rest, number)),
        (Some((number, rest)), _) if NUMBER.is_match(number) => Some((rest, number)),
        _ => None,
    };
    if let Some((rest, number)) = split {
        let (rest, number) = (rest.to_string(), number.to_string());
        parsed[at].1 = rest;
        parsed.insert(at + 1, ("house_number".into(), number));
    }
}

impl Index {
    pub(crate) fn chinese_streets(
        &self,
        units: &[String],
        road: &str,
        number: &str,
        city: Option<&str>,
        cc: Option<&str>,
    ) -> Vec<Hit> {
        let towns = units.iter().rev().map(String::as_str).chain(city);
        let mut street = None;
        for town in towns {
            let hits: Vec<_> = self
                .streets(road, Some(number), Some(town), None, cc, false)
                .into_iter()
                .filter(|hit| {
                    let Hit::Street { place, .. } = hit else {
                        return false;
                    };
                    let chain = self.chain(*place);
                    units.iter().all(|unit| {
                        chain
                            .iter()
                            .any(|&p| self.places[p as usize].named(name_key(unit)))
                    })
                })
                .collect();
            if let Some(h) = hits.iter().find(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            }) {
                return vec![h.clone()];
            }
            if street.is_none() {
                street = hits.into_iter().next();
            }
        }
        street.into_iter().collect()
    }

    pub(crate) fn chinese_suggest(&self, text: &str, cc: Option<&str>) -> Option<Vec<Suggestion>> {
        if !text.contains('號') {
            return None;
        }
        let (units, road, number) = chinese(text)?;
        if !matches!(units.as_slice(), [state, city] if state.ends_with('市') && city.ends_with('區'))
        {
            return None;
        }
        let parts: Vec<_> = text.split([',', '，']).map(str::trim).collect();
        if !(parts.len() == 1
            || parts.len() == 2 && self.country(parts[1]).as_deref() == Some("tw"))
        {
            return None;
        }
        let (building, suffix) = parts[0].rsplit_once('號')?;
        if schema::number_key(building) != format!("{}{road}{number}", units.concat()) {
            return None;
        }
        let road_key = fold(&road);
        if road_key.is_empty()
            || number.len() > MAX_NUMBER
            || cc.is_some_and(|c| !c.eq_ignore_ascii_case("tw"))
        {
            return Some(vec![]);
        }
        let suffix = suffix.trim();
        Some(
            self.chinese_streets(&units, &road, &number, None, Some("tw"))
                .into_iter()
                .filter_map(|hit| {
                    let Hit::Street {
                        doc,
                        sid,
                        place,
                        name,
                        number: Some(found_number),
                        x,
                        y,
                        ..
                    } = hit
                    else {
                        return None;
                    };
                    if fold(&name) != road_key || schema::number_key(&found_number) != number {
                        return None;
                    }
                    let mut found =
                        self.street_suggestion(doc, sid, place, &name, Some(&found_number), (x, y));
                    if !suffix.is_empty() {
                        if let Some((at, _)) = found.display_name.match_indices(", ").nth(1) {
                            found.display_name.insert_str(at, &format!(", {suffix}"));
                        }
                        found.unverified_suffix = suffix.into();
                    }
                    Some(found)
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_road_addresses() {
        assert_eq!(
            chinese("上海市虹口区中山北一路295号, Hongkou 200083, China"),
            Some((
                vec!["上海市".into(), "虹口区".into()],
                "中山北一路".into(),
                "295".into()
            ))
        );
        assert_eq!(
            chinese("浙江省海宁市学林街60至62号"),
            Some((
                vec!["浙江省".into(), "海宁市".into()],
                "学林街".into(),
                "60".into()
            ))
        );
        assert_eq!(
            chinese("学院路18号, Beijing 100083, China"),
            Some((vec![], "学院路".into(), "18".into()))
        );
        for query in [
            "臺北市松山區南京東路五段３８９巷６弄１１號, Taiwan",
            "臺北市松山區南京東路五段389巷6弄11號, Taiwan",
            "臺北市松山區南京東路五段３８９巷６弄１１號三樓, Taiwan",
        ] {
            assert_eq!(
                chinese(query),
                Some((
                    vec!["臺北市".into(), "松山區".into()],
                    "南京東路五段389巷6弄".into(),
                    "11".into()
                ))
            );
        }
        assert_eq!(
            chinese("臺北市松山區市民大道五段１３１號七樓, Taiwan"),
            Some((
                vec!["臺北市".into(), "松山區".into()],
                "市民大道五段".into(),
                "131".into()
            ))
        );
        assert_eq!(
            chinese("臺北市松山區南京東路三段２６９巷６之４號二樓, Taiwan")
                .unwrap()
                .2,
            "6之4"
        );
        assert_eq!(chinese("臺北市松山區南京東路11號未知商店"), None);
        assert_eq!(chinese("10 Downing Street"), None);
    }
}
