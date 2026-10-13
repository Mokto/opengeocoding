use crate::proto::geolocation::{AddressDetails, FullAddress};
use indexmap::IndexMap;
use regex::Regex;
use serde::Deserialize;
use std::{collections::HashMap, sync::LazyLock};

pub type ParsedAddress = IndexMap<String, String>;

#[derive(Deserialize)]
struct Reference {
    countries: HashMap<String, String>,
    #[serde(default)]
    overrides: HashMap<String, FullAddress>,
    region_countries: HashMap<String, String>,
    alpha_ranges: Vec<[u32; 2]>,
    printable_ranges: Vec<[u32; 2]>,
}

static REFERENCE: LazyLock<Reference> = LazyLock::new(|| {
    serde_json::from_str(include_str!(concat!(env!("OUT_DIR"), "/reference.json")))
        .expect("generated reference data")
});

pub fn search_override(query: &str) -> Option<FullAddress> {
    REFERENCE.overrides.get(query).cloned()
}

pub fn printable(ch: char) -> bool {
    let code = ch as u32;
    let index = REFERENCE
        .printable_ranges
        .partition_point(|range| range[1] < code);
    REFERENCE
        .printable_ranges
        .get(index)
        .is_some_and(|range| range[0] <= code)
}

pub fn whitespace(ch: char) -> bool {
    ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch)
}

fn clean(value: &str, numbers: bool) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|&ch| {
            let code = ch as u32;
            let index = REFERENCE
                .alpha_ranges
                .partition_point(|range| range[1] < code);
            "-/ ".contains(ch)
                || whitespace(ch)
                || (numbers && ch.is_ascii_digit())
                || REFERENCE
                    .alpha_ranges
                    .get(index)
                    .is_some_and(|range| range[0] <= code)
        })
        .collect::<String>()
        .split(whitespace)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn transliterate(value: &str) -> String {
    deunicode::deunicode(value)
}

pub fn normalize_place(value: &str) -> String {
    clean(&transliterate(value).replace('-', " "), true)
}

pub fn normalize_postcode(value: &str) -> String {
    value
        .chars()
        .filter(|&ch| ch != '-' && !whitespace(ch))
        .collect::<String>()
        .to_lowercase()
}

/// Words that qualify a place name rather than identify it ("Valga city", "Town of Cambridge",
/// "Special Capital Region of Jakarta").
const QUALIFIERS: &[&str] = &[
    "capital",
    "city",
    "district",
    "greater",
    "metropolitan",
    "municipality",
    "of",
    "region",
    "special",
    "taluka",
    "tehsil",
    "town",
    "village",
];
/// CJK administrative suffixes: 大阪市 is Osaka, 京都府 is Kyoto Prefecture.
const CJK_SUFFIXES: &[char] = &['市', '府', '県', '县', '縣', '省', '郡', '町', '村'];

fn place_keys(value: &str) -> (String, String) {
    let trimmed = value.trim_end_matches(CJK_SUFFIXES);
    // Transliteration conflates distinct Han characters (大阪 Osaka and 大坂 both become "Da Ban").
    if value
        .chars()
        .any(|ch| matches!(ch, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}'))
    {
        let key: String = trimmed.chars().filter(|ch| !whitespace(*ch)).collect();
        return (key.clone(), key);
    }
    let normalized = normalize_place(if trimmed.is_empty() { value } else { trimmed });
    let words: Vec<_> = normalized.split(' ').collect();
    let start = words
        .iter()
        .position(|w| !QUALIFIERS.contains(w))
        .unwrap_or(words.len());
    let end = words
        .iter()
        .rposition(|w| !QUALIFIERS.contains(w))
        .map_or(start, |i| i + 1);
    // Spacing differs between transliterations (Hà Nội/Hanoi, 上海/Shanghai); Danish and Norwegian
    // "aa" is the older spelling of "å" (Aarhus/Århus).
    let compact = |words: &[&str]| words.concat().replace("aa", "a");
    (compact(&words), compact(&words[start..end.max(start)]))
}

/// Whether two names denote the same place, ignoring qualifiers, spacing and script.
pub fn names_agree(expected: &str, actual: &str) -> bool {
    let (expected, expected_core) = place_keys(expected);
    let (actual, actual_core) = place_keys(actual);
    !expected.is_empty()
        && (expected == actual || (!expected_core.is_empty() && expected_core == actual_core))
}

pub fn expected_country(address: &ParsedAddress) -> String {
    REFERENCE
        .countries
        .get(&clean(part(address, "country"), false))
        .cloned()
        .unwrap_or_default()
}

pub fn implied_country(address: &ParsedAddress) -> String {
    ["state", "state_district", "country_region"]
        .iter()
        .find_map(|key| {
            REFERENCE
                .region_countries
                .get(&clean(&transliterate(part(address, key)), false))
                .cloned()
        })
        .unwrap_or_default()
}

pub fn part<'a>(address: &'a ParsedAddress, key: &str) -> &'a str {
    address.get(key).map(String::as_str).unwrap_or("")
}

pub fn create_query(address: &ParsedAddress, order: &[&str]) -> String {
    if order.is_empty() {
        return address.values().cloned().collect::<Vec<_>>().join(", ");
    }
    order
        .iter()
        .map(|key| part(address, key))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

static POSTCODE_SCHEMES: LazyLock<Vec<(Regex, Regex)>> = LazyLock::new(|| {
    [
        (
            r"^([a-z]{1,2}\d[a-z\d]?)(\d[a-z]{2})$",
            r"^[a-z]{1,2}\d[a-z\d]?$",
        ),
        (r"^(\d{4})[a-z]{2}$", r"^\d{4}$"),
        (
            r"^([abceghj-nprstvxy]\d[abceghj-nprstv-z])\d[abceghj-nprstv-z]\d$",
            r"^[abceghj-nprstvxy]\d[abceghj-nprstv-z]$",
        ),
        (r"^(\d{5})\d{4}$", r"^\d{5}$"),
        (
            r"^([ac-fhknprtv-y]\d{2})[0-9ac-fhknprtv-y]{4}$",
            r"^[ac-fhknprtv-y]\d{2}$",
        ),
    ]
    .iter()
    .map(|(full, short)| (Regex::new(full).unwrap(), Regex::new(short).unwrap()))
    .collect()
});

pub fn postcodes_compatible(expected: &str, actual: &str) -> bool {
    let left = normalize_postcode(expected);
    let right = normalize_postcode(actual);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left == right {
        return true;
    }
    POSTCODE_SCHEMES.iter().any(|(full, short)| {
        let area = |value: &str| {
            if let Some(hit) = full.captures(value) {
                hit.get(1).map(|m| m.as_str().to_owned())
            } else if short.is_match(value) {
                Some(value.to_owned())
            } else {
                None
            }
        };
        let left_area = area(&left);
        left_area.is_some() && left_area == area(&right)
    })
}

pub fn eircode(query: &str) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)[ac-fhknprtv-y]\d{2}[\s\x1c-\x1f]?[0-9ac-fhknprtv-y]{4}").unwrap()
    });
    let alphanum = |ch: char| ch.is_ascii_alphanumeric() || "İıſK".contains(ch);
    RE.find_iter(query)
        .find(|hit| {
            !query[..hit.start()]
                .chars()
                .next_back()
                .is_some_and(alphanum)
                && !query[hit.end()..].chars().next().is_some_and(alphanum)
        })
        .map(|hit| hit.as_str().to_owned())
        .unwrap_or_default()
}

pub fn country_matches(expected: &str, details: Option<&AddressDetails>) -> bool {
    expected.is_empty() || details.is_some_and(|d| d.country_code.to_lowercase() == expected)
}

pub fn postcode_agrees(expected: &str, details: Option<&AddressDetails>) -> bool {
    !expected.is_empty()
        && details
            .is_some_and(|d| !d.postcode.is_empty() && postcodes_compatible(expected, &d.postcode))
}

pub fn postcode_matches(expected: &str, details: Option<&AddressDetails>) -> bool {
    expected.is_empty()
        || details
            .is_none_or(|d| d.postcode.is_empty() || postcodes_compatible(expected, &d.postcode))
}

pub fn city_agrees(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    let expected = part(address, "city");
    if expected.is_empty() {
        return false;
    }
    let Some(d) = details else {
        return false;
    };
    [&d.city, &d.town, &d.village, &d.municipality]
        .iter()
        .filter(|s| !s.is_empty())
        .any(|s| names_agree(expected, s))
}

pub fn region_agrees(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    let Some(d) = details else {
        return false;
    };
    ["state", "state_district", "country_region"]
        .iter()
        .map(|key| part(address, key))
        .filter(|s| !s.is_empty())
        .any(|s| {
            let expected = normalize_place(s);
            [
                &d.state,
                &d.province,
                &d.region,
                &d.county,
                &d.state_district,
            ]
            .iter()
            .filter(|s| !s.is_empty())
            .any(|name| normalize_place(name) == expected)
        })
}

pub fn road_agrees(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    let (road, house) = (part(address, "road"), part(address, "house_number"));
    let Some(d) = details else {
        return false;
    };
    !road.is_empty()
        && !house.is_empty()
        && !d.road.is_empty()
        && !d.house_number.is_empty()
        && normalize_place(road) == normalize_place(&d.road)
        && house.split(whitespace).collect::<String>().to_lowercase()
            == d.house_number
                .split(whitespace)
                .collect::<String>()
                .to_lowercase()
}

pub fn same_postcode_country(
    country: &str,
    postcode: &str,
    details: Option<&AddressDetails>,
) -> bool {
    !country.is_empty()
        && !postcode.is_empty()
        && details.is_some_and(|d| {
            !d.postcode.is_empty()
                && country_matches(country, Some(d))
                && normalize_postcode(postcode) == normalize_postcode(&d.postcode)
        })
}

pub fn choose_candidate(
    address: &ParsedAddress,
    country: &str,
    postcode: &str,
    candidates: &[FullAddress],
) -> Option<(usize, &'static str)> {
    let implied = implied_country(address);
    let contradicts = !implied.is_empty() && implied != country;
    let country_ok = |item: &FullAddress| {
        country_matches(country, item.address_details.as_option())
            && (!contradicts || region_agrees(address, item.address_details.as_option()))
    };
    let prefer = |hits: Vec<usize>, tier| {
        hits.iter()
            .copied()
            .find(|&i| country_ok(&candidates[i]))
            .map(|i| (i, "country"))
            .or_else(|| {
                if !contradicts {
                    hits.first().copied().map(|i| (i, tier))
                } else {
                    hits.iter()
                        .copied()
                        .find(|&i| {
                            candidates[i].address_details.as_option().is_some_and(|d| {
                                d.country_code.to_lowercase() == implied
                                    && region_agrees(address, Some(d))
                            })
                        })
                        .map(|i| (i, tier))
                }
            })
    };
    let postcode_city = candidates
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            postcode_agrees(postcode, item.address_details.as_option())
                && city_agrees(address, item.address_details.as_option())
        })
        .map(|(i, _)| i)
        .collect();
    if let Some(hit) = prefer(postcode_city, "postcode_city") {
        return Some(hit);
    }
    if postcode.is_empty() {
        let road_city = candidates
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                road_agrees(address, item.address_details.as_option())
                    && city_agrees(address, item.address_details.as_option())
            })
            .map(|(i, _)| i)
            .collect();
        if let Some(hit) = prefer(road_city, "road_city") {
            return Some(hit);
        }
        if contradicts
            && let Some((i, _)) = candidates.iter().enumerate().find(|(_, item)| {
                item.address_details.as_option().is_some_and(|d| {
                    d.country_code.to_lowercase() == implied
                        && region_agrees(address, Some(d))
                        && (part(address, "city").is_empty() || city_agrees(address, Some(d)))
                })
            })
        {
            return Some((
                i,
                if part(address, "city").is_empty() {
                    "region"
                } else {
                    "city_region"
                },
            ));
        }
    }
    if let Some((i, _)) = candidates.iter().enumerate().find(|(_, item)| {
        country_ok(item)
            && (postcode.is_empty() || postcode_agrees(postcode, item.address_details.as_option()))
    }) {
        return Some((i, "country"));
    }
    if postcode.is_empty()
        && !contradicts
        && let Some((i, _)) = candidates.iter().enumerate().find(|(_, item)| {
            region_agrees(address, item.address_details.as_option())
                && city_agrees(address, item.address_details.as_option())
        })
    {
        return Some((i, "city_region"));
    }
    // A mistyped postcode ("312 Lowell Street, Lexington, MA 02423" for 02421): the house, road
    // and town still agree. Only after every candidate holding the postcode has been refused.
    if !postcode.is_empty() {
        let road_city = candidates
            .iter()
            .enumerate()
            .filter(|(_, item)| house_road_city(address, item.address_details.as_option()))
            .map(|(i, _)| i)
            .collect();
        return prefer(road_city, "road_city");
    }
    None
}

/// House number, road and town all agree with the parsed address.
pub fn house_road_city(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    road_agrees(address, details) && city_agrees(address, details)
}

pub fn without_country(query: &str, address: &ParsedAddress) -> String {
    let country = normalize_place(part(address, "country"));
    if country.is_empty() {
        return String::new();
    }
    let stripped = query
        .split(',')
        .map(|s| s.trim_matches(whitespace))
        .filter(|s| !s.is_empty() && normalize_place(s) != country)
        .collect::<Vec<_>>()
        .join(", ");
    if stripped == query {
        String::new()
    } else {
        stripped
    }
}
