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
        Regex::new(r"^([1-9][0-9]{0,3}[A-Za-z]?)\s+([0-9]*[A-Za-z][A-Za-z0-9 .'-]*)$").unwrap()
    });
    let c = NUMBERED.captures(head.trim())?;
    Some(vec![
        ("house_number".into(), c[1].to_lowercase()),
        ("road".into(), c[2].trim().to_lowercase()),
    ])
}

/// "14 北潭凹 Pak Tam Au" or "北潭凹 Pak Tam Au 14": a number and a road named in both scripts,
/// as OpenStreetMap names Hong Kong's.
fn bilingual(head: &str) -> Option<Vec<(String, String)>> {
    static FIRST: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^([1-9][0-9]{0,3}[A-Za-z]?)\s+(\S*\p{Han}\S*\s+[A-Za-z].*)$").unwrap()
    });
    static LAST: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(\S*\p{Han}\S*\s+[A-Za-z].*?)\s+([1-9][0-9]{0,3}[A-Za-z]?)$").unwrap()
    });
    let head = head.trim();
    let (number, road) = match FIRST.captures(head) {
        Some(c) => (c[1].to_string(), c[2].to_string()),
        None => {
            let c = LAST.captures(head)?;
            (c[2].to_string(), c[1].to_string())
        }
    };
    Some(vec![
        ("house_number".into(), number.to_lowercase()),
        ("road".into(), road.to_lowercase()),
    ])
}

fn shop(head: &str) -> Option<Vec<(String, String)>> {
    static SHOP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\p{Han}+(?:商場|街市)[1-9][0-9]{0,3}[A-Za-z]?號[鋪舖]").unwrap()
    });
    let head = head.trim();
    let found = SHOP.find(head)?;
    let road = if found.start() == 0 {
        head[found.end()..].strip_prefix(char::is_whitespace)?
    } else if found.end() == head.len() {
        head[..found.start()].strip_suffix(char::is_whitespace)?
    } else {
        return None;
    };
    if road.trim().is_empty() {
        return None;
    }
    // The source stores the mall and shop as one opaque number, never the street's civic number.
    Some(vec![
        ("house_number".into(), found.as_str().to_lowercase()),
        ("road".into(), road.trim().to_lowercase()),
    ])
}

fn latin_place(part: &str) -> &str {
    match part.find(|c: char| c.is_ascii_alphabetic()) {
        Some(at) if !part.is_ascii() => &part[at..],
        _ => part,
    }
}

/// Whether a lower-cased part names the territory or one of its regions, which no place of the
/// data is named for. OpenStreetMap writes both scripts: "新界 new territories".
fn region(part: &str) -> bool {
    let latin = part.trim_start_matches(|c: char| !c.is_ascii()).trim();
    [part, latin].iter().any(|p| {
        matches!(
            *p,
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
    })
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
        if place.is_empty() || region(&place) {
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

/// Chinese text without its spaces, in ASCII digits and capitals.
fn compact(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' | 'Ａ'..='Ｚ' => char::from_u32(c as u32 - 0xFEE0).unwrap(),
            c => c.to_ascii_uppercase(),
        })
        .collect()
}

/// A Hong Kong road being typed in Chinese as the suggestions can read it: without the
/// territory, region and district before it, and with a space before its number, which the
/// term dictionary otherwise holds as one word with the road ("香港中西區伊利近街2" is "伊利近街
/// 2", "高芳街27號" is "高芳街 27"). None for a road alone, which needs nothing.
pub(crate) fn typed_chinese(text: &str) -> Option<String> {
    static TYPED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\p{Han}+?)([1-9][0-9]{0,3}[A-Z]?)?號?$").unwrap());
    let text = compact(text);
    let rest = without_territory(&text);
    let stated = rest.len() < text.len();
    let rest = match DISTRICTS
        .into_iter()
        .find(|d| stated && rest.starts_with(d))
    {
        Some(d) => &rest[d.len()..],
        None => rest,
    };
    let c = TYPED.captures(rest)?;
    match c.get(2) {
        Some(number) => Some(format!("{} {}", &c[1], number.as_str())),
        None => stated.then(|| c[1].to_string()),
    }
}

/// (district, road, number) of a Hong Kong address in Chinese that starts with the territory
/// or one of its regions: "香港新界葵青區葵涌高芳街27號" -> (葵青區, 葵涌高芳街, 27). The road
/// may still start with a place (葵涌, Kwai Chung) that nothing ends.
pub(crate) fn chinese(q: &str) -> Option<(Option<&'static str>, String, String)> {
    static NUMBERED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(.+?)([1-9][0-9]{0,3}[A-Z]?)號$").unwrap());
    let part = q.split([',', '，']).find(|p| p.contains('號'))?;
    let text = compact(part);
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
    pub(crate) fn hk_comma_house(&self, q: &str, parsed: &Parsed) -> Option<Parsed> {
        if q.is_ascii() {
            return None;
        }
        let parts: Vec<_> = q.split(',').map(str::trim).collect();
        let [head, rest @ .., country] = parts.as_slice() else {
            return None;
        };
        if self.country(country).as_deref() != Some("hk")
            || parts
                .iter()
                .skip(1)
                .any(|p| self.country(p).is_some_and(|cc| cc != "hk"))
            || parsed
                .iter()
                .any(|(k, v)| k == "country" && self.country(v).is_some_and(|cc| cc != "hk"))
        {
            return None;
        }
        let head = bilingual(head).or_else(|| shop(head))?;
        if self.hk_typed_houses(q, Some("hk")).is_empty() {
            return None;
        }
        let mut repaired = aligned(&head, rest);
        repaired.push(("country".into(), country.to_lowercase()));
        Some(repaired.into())
    }

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

    /// The houses a whole Hong Kong address names, read as SearchAddress reads it but without
    /// libpostal: "香港新界葵青區葵涌高芳街27號", or "27 KO FONG STREET, KWAI CHUNG, KWAI TSING
    /// DISTRICT, NEW TERRITORIES, HONG KONG". Those in or near the area written, or in the
    /// district; else one only when no namesake far off has its number
    /// ([`Index::hk_house_alone`]).
    pub(crate) fn hk_typed_houses(&self, text: &str, cc: Option<&str>) -> Vec<Hit> {
        if let Some(hit) = self.hk_chinese(text) {
            return vec![hit];
        }
        let parts: Vec<&str> = text.split(',').map(str::trim).collect();
        // the flat, floor and building come first: "Flat A, 5/F, 29 Elgin Street, Central"
        let Some((at, head)) = parts.iter().enumerate().find_map(|(i, p)| {
            Some((i, numbered(p).or_else(|| bilingual(p)).or_else(|| shop(p))?))
        }) else {
            return vec![];
        };
        // "西貢 Sai Kung": the place by its English name
        let places: Vec<&str> = parts[at + 1..].iter().map(|p| latin_place(p)).collect();
        let Some(last) = places.last() else {
            return vec![];
        };
        let local = |p: &Place| !matches!(&*p.subtype, "country" | "dependency");
        let territory = cc.is_some_and(|c| c.eq_ignore_ascii_case("hk"))
            || region(&last.to_lowercase())
            || self.country(last).is_some_and(|c| territory_as_town(&c))
            || {
                let named = self.places(last, None, None, &local);
                !named.is_empty()
                    && named.iter().all(|h| {
                        matches!(h, Hit::Place(p) if territory_as_town(&self.places[*p as usize].cc))
                    })
            };
        if !territory {
            return vec![];
        }
        let aligned = aligned(&head, &places);
        let get = |k: &str| {
            aligned
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        let (Some(road), Some(number)) = (get("road"), get("house_number")) else {
            return vec![];
        };
        let (city, district) = (get("city"), get("state_district"));
        let mut hits = self.streets_in(road, Some(number), city, None, None, Some("hk"), false);
        if let Some(found) =
            self.hk_house_anywhere(&hits, road, Some(number), city, None, Some("hk"), false)
        {
            hits = found;
        }
        hits.retain(|h| {
            matches!(
                h,
                Hit::Street {
                    number: Some(_),
                    ..
                }
            )
        });
        let stated = |h: &Hit| match (h, city, district) {
            (Hit::Street { x, y, .. }, Some(city), _) => {
                self.house_locality_named(h, city)
                    || self.house_town_named(h, city)
                    || area_near(self, city, "hk", *y, *x)
            }
            (Hit::Street { place, .. }, None, Some(district)) => self
                .chain(*place)
                .iter()
                .any(|&p| self.places[p as usize].named(name_key(district))),
            _ => false,
        };
        let (named, other): (Vec<Hit>, Vec<Hit>) = hits.into_iter().partition(stated);
        if !named.is_empty() {
            return named;
        }
        other
            .into_iter()
            .next()
            .filter(|h| self.hk_house_alone(h))
            .into_iter()
            .collect()
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
            for (at, _) in road.char_indices() {
                let rest = &road[at..];
                // a two-character ending ("曲街") of a longer road is too common a name to
                // stand without the district
                if rest.chars().count() < if scoped || at == 0 { 2 } else { 3 } {
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
    fn spaces_the_number_of_a_road_typed_in_chinese() {
        for (typed, expected) in [
            ("香港中西區伊利近街2", Some("伊利近街 2")),
            ("香港中西區伊利近街", Some("伊利近街")),
            ("九龍彌敦道132號", Some("彌敦道 132")),
            ("高芳街２７號", Some("高芳街 27")),
            ("高芳街 27", Some("高芳街 27")),
            // a road alone needs nothing; 東區 starts a road unless the territory comes first
            ("伊利近街", None),
            ("東區走廊1", Some("東區走廊 1")),
            ("27 Ko Fong Street", None),
            ("芝公園4-2-8", None),
        ] {
            assert_eq!(typed_chinese(typed).as_deref(), expected, "{typed}");
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
        for head in ["14 北潭凹 Pak Tam Au", "北潭凹 Pak Tam Au 14"] {
            assert_eq!(
                bilingual(head).unwrap(),
                [
                    ("house_number".to_string(), "14".to_string()),
                    ("road".to_string(), "北潭凹 pak tam au".to_string())
                ]
            );
        }
        assert_eq!(bilingual("14 Pak Tam Au"), None);
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
    fn keeps_the_mall_in_a_shop_identifier() {
        for head in [
            "青松商場42B號鋪 翠湖路 Jade Lake Road",
            "翠湖路 Jade Lake Road 青松商場42B號鋪",
        ] {
            assert_eq!(
                shop(head).unwrap(),
                [
                    ("house_number".into(), "青松商場42b號鋪".into()),
                    ("road".into(), "翠湖路 jade lake road".into()),
                ]
            );
        }
        for head in [
            "42號鋪 翠湖路",
            "青松商場42號鋪",
            "翠湖路 青松商場42號鋪 後門",
        ] {
            assert_eq!(shop(head), None, "{head}");
        }
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
