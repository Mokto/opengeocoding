use crate::index::*;
use regex::Regex;
use std::sync::LazyLock;

/// `query` with "S.A.R." (or "SAR", "Special Administrative Region") dropped after Hong Kong or
/// Macau, which names no place in the data: "Kowloon, Hong Kong S.A.R." is "Kowloon, Hong Kong".
/// None when no part has it.
pub fn without_sar(query: &str) -> Option<String> {
    static SAR: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\b(hong\s*kong|macau|macao)(\s*,)?\s+(s\.?\s?a\.?\s?r\.?|special\s+administrative(\s+region)?)(\s+(of\s+)?(the\s+)?(people's\s+republic\s+of\s+)?china)?\s*(,|$)",
        )
        .unwrap()
    });
    SAR.is_match(query).then(|| {
        SAR.replace_all(query, "$1$9")
            .trim_end_matches([' ', ','])
            .to_owned()
    })
}

/// Hong Kong is written where the town goes ("27 Ko Fong Street, Hong Kong") while the data
/// files its houses under towns of their own (Kwai Chung).
pub fn territory_as_town(cc: &str) -> bool {
    cc == "hk"
}

/// `text` without the territory that starts a Hong Kong address written in Chinese, and the
/// region after it: "香港新界葵青區…" is "葵青區…". 香港仔 (Aberdeen) and 九龍城
/// (Kowloon City) are places, not the territory or region.
pub(crate) fn without_territory(text: &str) -> &str {
    let rest = ["中國香港", "中国香港", "香港"]
        .iter()
        .find_map(|t| text.strip_prefix(t).filter(|r| !r.starts_with('仔')))
        .unwrap_or(text);
    ["新界", "九龍", "九龙"]
        .iter()
        .find_map(|r| rest.strip_prefix(r).filter(|r| !r.starts_with('城')))
        .unwrap_or(rest)
}

/// Whether a place named `area` lies near a house of the territory. Its areas (Tai Wai, Kwai
/// Chung) have no boundaries in the data, which files a house under whichever locality its point
/// falls in, so the area named and the house's locality often differ.
// ponytail: a fixed 3 km radius; per-area extents would be the upgrade.
pub fn area_near(index: &Index, area: &str, cc: &str, lat: f64, lon: f64) -> bool {
    index.place_near(area, Some(cc), lat as f32, lon as f32, 3.0)
}

/// "185 Ngau Liu": a number and the street or village it is on, which libpostal reads as a
/// house or a town when the name has no street type.
pub(crate) fn numbered(head: &str) -> Option<Vec<(String, String)>> {
    static NUMBERED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^([1-9][0-9]{0,3}[A-Za-z]?)\s+([A-Za-z][A-Za-z .'-]+)$").unwrap()
    });
    let c = NUMBERED.captures(head.trim())?;
    Some(vec![
        ("house_number".into(), c[1].to_lowercase()),
        ("road".into(), c[2].trim().to_lowercase()),
    ])
}

/// "27 Ko Fong Street, Kwai Chung, Kwai Tsing District, New Territories, Hong Kong" read with
/// its commas as the boundaries: libpostal runs the road on into the next part ("ko fong street
/// kwai", city "chung"). The last place is the town, a "… District" its district; the regions
/// (Kowloon, New Territories, Hong Kong Island) name no place of the data and are dropped.
pub(crate) fn aligned(head: &[(String, String)], places: &[&str]) -> Vec<(String, String)> {
    let mut aligned = head.to_vec();
    let mut town = None;
    for place in places {
        let place = place.trim().to_lowercase().replace(" & ", " and ");
        // OpenStreetMap writes both scripts: "新界 New Territories"
        let latin = place.trim_start_matches(|c: char| !c.is_ascii()).trim();
        let region = |p: &str| {
            matches!(
                p,
                "kowloon"
                    | "new territories"
                    | "hong kong"
                    | "hong kong island"
                    | "n.t."
                    | "nt"
                    | "九龍"
                    | "新界"
                    | "香港"
            )
        };
        if place.is_empty() || region(&place) || region(latin) {
            continue;
        }
        if place.ends_with(" district") {
            aligned.push(("state_district".into(), place));
        } else {
            town = Some(place);
        }
    }
    aligned.extend(town.map(|t| ("city".to_string(), t)));
    aligned
}

/// (district, road, number) of a Hong Kong address in Chinese that starts with the territory
/// or one of its regions: "香港新界葵青區葵涌高芳街27號" -> (葵青區, 葵涌高芳街, 27). The road
/// may still start with a place (葵涌, Kwai Chung) that nothing ends.
pub(crate) fn chinese(q: &str) -> Option<(Option<&'static str>, String, String)> {
    const DISTRICTS: [&str; 18] = [
        "中西區",
        "灣仔區",
        "東區",
        "南區",
        "油尖旺區",
        "深水埗區",
        "九龍城區",
        "黃大仙區",
        "觀塘區",
        "葵青區",
        "荃灣區",
        "屯門區",
        "元朗區",
        "北區",
        "大埔區",
        "沙田區",
        "西貢區",
        "離島區",
    ];
    static NUMBERED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.+?)([1-9][0-9]{0,3}[A-Z]?)號$").unwrap());
    let part = q.split([',', '，']).find(|p| p.contains('號'))?;
    let text: String = part
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' | 'Ａ'..='Ｚ' => char::from_u32(c as u32 - 0xFEE0).unwrap(),
            c => c.to_ascii_uppercase(),
        })
        .collect();
    let rest = without_territory(&text);
    if rest.len() == text.len() {
        return None;
    }
    let district = DISTRICTS.into_iter().find(|d| rest.starts_with(d));
    let rest = &rest[district.map_or(0, str::len)..];
    let c = NUMBERED.captures(rest)?;
    Some((district, c[1].to_string(), c[2].to_string()))
}

impl Index {
    /// Whether no other house far off has this house's number on a street that shares one of its
    /// names: "8 Yat Tung Street" is two OpenStreetMap houses 280 m apart on one estate, and
    /// villages share names. Only a house alone is one the territory or an area near it names.
    pub(crate) fn hk_house_alone(&self, hit: &Hit) -> bool {
        let Hit::Street {
            name,
            number: Some(number),
            x,
            y,
            ..
        } = hit
        else {
            return true;
        };
        // OpenStreetMap names Hong Kong's streets in both scripts at once ("逸東街 Yat Tung
        // Street"), the register in one each.
        let mut names: Vec<&str> = vec![];
        for n in name.split(SEP) {
            names.push(n.trim());
            if let Some(at) = n.find(|c: char| c.is_ascii_alphabetic())
                && at > 0
            {
                names.extend([n[..at].trim(), n[at..].trim()]);
            }
        }
        names.retain(|n| !n.is_empty());
        names.dedup();
        names.iter().all(|alias| {
            let key = fold(alias);
            self.streets(alias, Some(number), None, None, Some("hk"), false)
                .iter()
                .all(|h| match h {
                    Hit::Street {
                        name,
                        number: Some(_),
                        x: x1,
                        y: y1,
                        ..
                    } if name.split(SEP).any(|n| fold(n) == key) => {
                        ((x1 - x) * y.to_radians().cos()).hypot(y1 - y) * 111.2 < 0.15
                    }
                    _ => true,
                })
        })
    }

    /// A Hong Kong house the town written does not hold: anywhere in the territory when the town
    /// is the territory itself, else near the area named ([`area_near`]), and only a house
    /// alone. None when `hits` already has a house or the query is not of Hong Kong.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn hk_house_anywhere(
        &self,
        hits: &[Hit],
        road: &str,
        number: Option<&str>,
        city: Option<&str>,
        postcode: Option<&str>,
        cc: Option<&str>,
        exact: bool,
    ) -> Option<Vec<Hit>> {
        let housed = |h: &&Hit| {
            matches!(
                h,
                Hit::Street {
                    number: Some(_),
                    ..
                }
            )
        };
        let territory = match cc {
            Some(c) => c.to_owned(),
            None => city.and_then(|c| self.country(c))?,
        };
        if !territory_as_town(&territory) || hits.iter().any(|h| housed(&h)) {
            return None;
        }
        let area = city.filter(|c| self.country(c).is_none());
        let anywhere: Vec<Hit> = self
            .streets_in(road, number, None, None, postcode, Some(&territory), exact)
            .into_iter()
            .filter(|h| match (h, area) {
                (Hit::Street { x, y, .. }, Some(area)) => area_near(self, area, &territory, *y, *x),
                _ => true,
            })
            .collect();
        match anywhere.iter().find(housed) {
            Some(house) => self.hk_house_alone(house).then_some(anywhere),
            None => hits.is_empty().then_some(anywhere),
        }
    }

    /// The house a Hong Kong address in Chinese names ("香港新界葵青區葵涌高芳街27號"): libpostal
    /// splits the territory, region and district into a country, a state and a postcode. A place
    /// may stand before the road with nothing to end it (葵涌, Kwai Chung): the road is then the
    /// longest ending of the text that names a street holding the house, in the district written.
    pub(crate) fn hk_chinese(&self, q: &str) -> Option<Hit> {
        let (district, road, number) = chinese(q)?;
        let in_district = |place: u32| {
            district.is_none_or(|d| {
                self.chain(place)
                    .iter()
                    .any(|&p| self.places[p as usize].named(name_key(d)))
            })
        };
        // The data often files a house under another district than its own (Java Road under
        // Kwun Tong): without the district, a road and number found in one spot only.
        for scoped in [true, false] {
            let town = district.filter(|_| scoped);
            let shortest = if scoped { 2 } else { 3 };
            for (at, _) in road.char_indices() {
                let rest = &road[at..];
                if rest.chars().count() < shortest {
                    break;
                }
                let housed: Vec<Hit> = self
                    .streets(rest, Some(&number), town, None, Some("hk"), false)
                    .into_iter()
                    .filter(|h| {
                        matches!(h, Hit::Street { number: Some(_), name, place, .. }
                            if fold(name) == fold(rest) && (!scoped || in_district(*place)))
                    })
                    .collect();
                if let Some(first) = housed.first()
                    && (scoped || self.hk_house_alone(first))
                {
                    return Some(first.clone());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_chinese_addresses_after_the_territory_and_region() {
        assert_eq!(
            chinese("香港新界葵青區葵涌高芳街２７號"),
            Some((Some("葵青區"), "葵涌高芳街".into(), "27".into()))
        );
        assert_eq!(
            chinese("香港九龍九龍城區馬頭涌道101A號"),
            Some((Some("九龍城區"), "馬頭涌道".into(), "101A".into()))
        );
        assert_eq!(
            chinese("香港新界沙田區沙田 大圍田心街28號"),
            Some((Some("沙田區"), "沙田大圍田心街".into(), "28".into()))
        );
        assert_eq!(
            chinese("香港東鎮圍56號"),
            Some((None, "東鎮圍".into(), "56".into()))
        );
        // Aberdeen and Kowloon City are places, not the territory or a region
        assert_eq!(without_territory("香港仔大道1號"), "香港仔大道1號");
        assert_eq!(without_territory("九龍城道31號"), "九龍城道31號");
        for unstated in ["高芳街27號", "上海市虹口区中山北一路295号", "香港高芳街"]
        {
            assert_eq!(chinese(unstated), None, "{unstated}");
        }
    }

    #[test]
    fn aligns_english_addresses_on_their_commas() {
        let head = numbered("185 NGAU LIU").unwrap();
        assert_eq!(
            head,
            [
                ("house_number".to_string(), "185".to_string()),
                ("road".to_string(), "ngau liu".to_string())
            ]
        );
        assert_eq!(numbered("Flat A, 5/F"), None);
        assert_eq!(numbered("12 高芳街"), None);
        let aligned = aligned(
            &head,
            &[
                " TUEN MUN SOUTH",
                " CENTRAL & WESTERN DISTRICT",
                " 屯門 Tuen Mun",
                " 新界 New Territories",
                " KOWLOON",
            ],
        );
        assert_eq!(
            aligned[2..],
            [
                (
                    "state_district".to_string(),
                    "central and western district".to_string()
                ),
                ("city".to_string(), "屯門 tuen mun".to_string()),
            ]
        );
    }

    #[test]
    fn drops_the_title_of_a_special_administrative_region() {
        for (query, expected) in [
            ("hong kong s.a.r.", Some("hong kong")),
            ("Kowloon, Hong Kong S.A.R.", Some("Kowloon, Hong Kong")),
            (
                "hong kong, central and western, hong kong s.a.r.",
                Some("hong kong, central and western, hong kong"),
            ),
            ("macau s.a.r.", Some("macau")),
            ("Macao SAR, China", Some("Macao, China")),
            ("hong kong special administrative ", Some("hong kong")),
            ("Hong Kong SAR, Wan Chai", Some("Hong Kong, Wan Chai")),
            ("hong kong", None),
            ("sar, hong kong", None),
            ("Sarajevo", None),
        ] {
            assert_eq!(without_sar(query).as_deref(), expected, "{query}");
        }
    }
}
