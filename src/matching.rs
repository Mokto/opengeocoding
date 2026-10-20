use crate::{
    index::{
        canon::{canon, is_canonical},
        schema::fold,
    },
    proto::opengeocoding::{AddressDetails, FullAddress},
};
use indexmap::IndexMap;
use regex::Regex;
use serde::Deserialize;
use std::{collections::HashMap, sync::LazyLock};

pub type ParsedAddress = IndexMap<String, String>;

#[derive(Deserialize)]
struct Reference {
    countries: HashMap<String, String>,
    country_names: HashMap<String, String>,
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

/// A country code's CLDR English name ("es" → "Spain").
pub fn country_name(code: &str) -> Option<&'static str> {
    REFERENCE.country_names.get(code).map(String::as_str)
}

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
    "and",
    "borough",
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
    "township",
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
    // "Mt. Gretna" is "Mount Gretna": the index's street-type canonicalization, applied to both.
    let compact = |words: &[&str]| {
        words
            .iter()
            .map(|w| canon(w).unwrap_or(w))
            .collect::<String>()
            .replace("aa", "a")
    };
    (compact(&words), compact(&words[start..end.max(start)]))
}

/// Whether two names denote the same place, ignoring qualifiers, spacing and script.
pub fn names_agree(expected: &str, actual: &str) -> bool {
    agrees_with(expected)(actual)
}

/// `names_agree` with `expected` folded once, for comparing it with many names.
pub fn agrees_with(expected: &str) -> impl Fn(&str) -> bool {
    let (expected, expected_core) = place_keys(expected);
    move |actual| {
        let (actual, actual_core) = place_keys(actual);
        !expected.is_empty()
            && (expected == actual || (!expected_core.is_empty() && expected_core == actual_core))
    }
}

pub fn expected_country(address: &ParsedAddress) -> String {
    let stated = clean(part(address, "country"), false);
    // "Kingdom of Bahrain", "Republic of Korea": the form of state is not the name
    const FORMS: &[&str] = &[
        "kingdom of ",
        "republic of ",
        "state of ",
        "sultanate of ",
        "emirate of ",
        "commonwealth of ",
        "the ",
    ];
    std::iter::once(stated.as_str())
        .chain(FORMS.iter().filter_map(|f| stated.strip_prefix(f)))
        .find_map(|name| REFERENCE.countries.get(name))
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

/// Repairs what libpostal gets wrong in a way that costs the whole lookup: a country-plate
/// prefix on a postcode ("SE-262 73", "L-1855"), a postcode glued to the town ("St Julians
/// STJ1000"), a locality abbreviation in front of the town ("г. Воронеж"), and a road that is
/// only a street type ("Dr." in "Dr. Nikiski", which would otherwise match any drive there).
pub fn tidy(address: &mut ParsedAddress) {
    static PLATE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[A-Za-z]{1,2}\s?-\s?(\d)").unwrap());
    static GLUED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[a-z]{1,3}\d{2,}[a-z]*$").unwrap());
    static PLATE_ONLY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[A-Za-z]{1,2}\s?-\s?\d{3,5}$").unwrap());
    // Only plates the data does not keep: Latvian postcodes are stored as "LV-1002".
    const PLATE_DROPPED: &[&str] = &[
        "se", "dk", "no", "fi", "de", "d-", "d ", "at", "a-", "a ", "ch", "lu", "l-", "l ", "be",
        "b-", "b ", "nl", "fr", "f-", "f ", "es", "it", "i-", "pl", "cz", "sk", "hu", "ro", "bg",
        "hr", "si", "pt", "gr", "ie",
    ];
    const LOCALITY: &[&str] = &["г.", "г", "гор.", "пос.", "пгт", "смт", "м."];
    const STREET_TYPES: &[&str] = &[
        "dr",
        "st",
        "rd",
        "ave",
        "blvd",
        "ln",
        "ct",
        "pl",
        "hwy",
        "ул",
        "улица",
        "вул",
        "вулиця",
        "пер",
        "просп",
        "пр-т",
        "наб",
        "ш",
    ];
    // "Dublin 4": a postal district numbered after its town, which libpostal files as a district
    // or a postcode of its own and leaves the street with no town.
    static DISTRICT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(\p{L}[\p{L} .'-]{2,}?[\p{L}])\s+\d{1,2}[a-z]?$").unwrap());
    if part(address, "city").is_empty() {
        for key in ["city_district", "postcode"] {
            if let Some(town) = address
                .get(key)
                .and_then(|v| DISTRICT.captures(v))
                .map(|c| c[1].to_owned())
            {
                address.shift_remove(key);
                address.insert("city".into(), town);
                break;
            }
        }
    }
    // "L-1855" taken for the town
    if part(address, "postcode").is_empty()
        && let Some(city) = address
            .get("city")
            .filter(|c| PLATE_ONLY.is_match(c))
            .cloned()
    {
        address.shift_remove("city");
        address.insert("postcode".into(), city);
    }
    if let Some(code) = address.get_mut("postcode")
        && PLATE_DROPPED
            .iter()
            .any(|p| code.to_lowercase().starts_with(p))
    {
        *code = PLATE.replace(code, "$1").into_owned();
    }
    if let Some(city) = address.get("city").cloned() {
        let mut words: Vec<&str> = city.split(whitespace).filter(|w| !w.is_empty()).collect();
        if words.len() > 1 && LOCALITY.contains(&words[0]) {
            words.remove(0);
        }
        if words.len() > 1
            && part(address, "postcode").is_empty()
            && words.last().is_some_and(|w| GLUED.is_match(w))
        {
            let glued = words.pop().unwrap_or_default().to_owned();
            address.insert("postcode".into(), glued);
        }
        address.insert("city".into(), words.join(" "));
    }
    let only_type = |road: &str| {
        let words: Vec<String> = road
            .split(whitespace)
            .map(|w| w.trim_matches('.').to_lowercase())
            .filter(|w| !w.is_empty())
            .collect();
        !words.is_empty() && words.iter().all(|w| STREET_TYPES.contains(&w.as_str()))
    };
    if only_type(part(address, "road")) {
        address.shift_remove("road");
    }
}

pub fn recover_postcode(query: &str, address: &mut ParsedAddress) {
    let country = expected_country(address);
    let postcode = part(address, "postcode");
    if postcode.is_empty() || postcode_shape(&country, postcode) != Some(false) {
        return;
    }
    let mut codes = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .filter(|p| postcode_shape(&country, p) == Some(true));
    if let Some(code) = codes.next()
        && codes.next().is_none()
    {
        address.insert("postcode".into(), code.into());
    }
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
    expected.is_empty()
        || details.is_some_and(|d| {
            let actual = d.country_code.to_lowercase();
            // a territory named as the country is answered under its sovereign
            actual == expected || crate::index::sovereign(expected) == Some(actual.as_str())
        })
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

/// The query's first part names the candidate and a later part names its region: "Kriel,
/// Mpumalanga, South Africa" for the place Kriel in the province Mpumalanga, which libpostal
/// reads as a suburb Kriel in a town Mpumalanga. A name and its region agreeing is evidence
/// enough for the place whichever fields the parser chose.
pub fn named_in_region(query: &str, details: Option<&AddressDetails>) -> bool {
    let Some(d) = details else {
        return false;
    };
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .filter(|p| !p.is_empty())
        .collect();
    let Some((first, rest)) = parts.split_first() else {
        return false;
    };
    let agrees = agrees_with(first);
    let named = [&d.city, &d.town, &d.village, &d.municipality, &d.suburb]
        .iter()
        .any(|n| !n.is_empty() && agrees(n));
    // With a region stated, only the place in that region is named; with a lone country
    // ("Skopje, North Macedonia") the country is the evidence.
    let in_country = |part: &str| {
        names_agree(part, &d.country)
            || (!d.country_code.is_empty()
                && expected_country(&parsed_country(part)) == d.country_code.to_lowercase())
    };
    let in_region = |part: &str| {
        [
            &d.state,
            &d.province,
            &d.region,
            &d.county,
            &d.state_district,
        ]
        .iter()
        .any(|r| !r.is_empty() && (names_agree(part, r) || admin_name_agrees(part, r)))
    };
    named
        && rest
            .iter()
            .any(|part| in_region(part) || (rest.len() == 1 && in_country(part)))
}

/// "San Juan" is "San Juan Province": the region's name without its administrative word.
fn admin_name_agrees(stated: &str, region: &str) -> bool {
    const WORDS: &[&str] = &[
        "province",
        "department",
        "county",
        "state",
        "prefecture",
        "governorate",
        "canton",
        "oblast",
        "voivodeship",
        "territory",
    ];
    let name = normalize_place(region);
    let words: Vec<&str> = name.split(' ').collect();
    match words.split_last() {
        Some((last, rest)) if !rest.is_empty() && WORDS.contains(last) => {
            normalize_place(stated) == rest.join(" ")
        }
        _ => false,
    }
}

/// The stated town is the candidate's own city, town, village or municipality, not a suburb it
/// is filed under.
pub fn town_named(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    let expected = part(address, "city");
    let agrees = agrees_with(expected);
    !expected.is_empty()
        && details.is_some_and(|d| {
            [&d.city, &d.town, &d.village, &d.municipality]
                .iter()
                .filter(|s| !s.is_empty())
                .any(|s| agrees(s))
        })
}

pub fn city_agrees(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    let expected = part(address, "city");
    if expected.is_empty() {
        return false;
    }
    let Some(d) = details else {
        return false;
    };
    let agrees = agrees_with(expected);
    // A suburb the index files under its town ("Blairgowrie" in Johannesburg) is its own name
    // when no road or house narrows it; with one, the suburb only says where the road is. A
    // fragment of a few letters ("C" of "Fraser Valley C") names no suburb.
    let own_area = d.road.is_empty()
        && d.house_number.is_empty()
        && normalize_place(expected).chars().count() >= 3;
    [&d.city, &d.town, &d.village, &d.municipality]
        .into_iter()
        .chain(
            own_area
                .then_some([&d.suburb, &d.city_district, &d.neighbourhood])
                .into_iter()
                .flatten(),
        )
        .filter(|s| !s.is_empty())
        .any(|s| agrees(s))
}

/// The stated postcode is the candidate's and the stated town names a place above the
/// candidate's own locality. The index files a house under its borough or locality: "10719
/// Berlin" is in Charlottenburg-Wilmersdorf of the state Berlin, and "1165 København K" in the
/// suburb København. Without the postcode this would accept any house in a same-named region.
pub fn postcode_place_agrees(
    address: &ParsedAddress,
    postcode: &str,
    details: Option<&AddressDetails>,
) -> bool {
    let expected = part(address, "city");
    let Some(d) = details else {
        return false;
    };
    if expected.is_empty()
        || postcode.is_empty()
        || d.postcode.is_empty()
        || normalize_postcode(postcode) != normalize_postcode(&d.postcode)
    {
        return false;
    }
    // Danish postal districts ("København K", "Frederiksberg C") end in one letter.
    let words: Vec<_> = expected.split(whitespace).collect();
    let undistricted = match words.split_last() {
        Some((last, rest)) if !rest.is_empty() && last.chars().count() == 1 => rest.join(" "),
        _ => expected.to_owned(),
    };
    [
        &d.state,
        &d.county,
        &d.state_district,
        &d.suburb,
        &d.city_district,
        &d.borough,
        &d.district,
    ]
    .iter()
    .filter(|s| !s.is_empty())
    .any(|s| names_agree(expected, s) || names_agree(&undistricted, s))
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
        && (roads_agree(road, &d.road)
            || road_expansions(road, &d.country_code)
                .iter()
                .any(|expanded| {
                    road_key(expanded) == road_key(&d.road)
                        && country_matches(&expected_country(address), Some(d))
                }))
        && {
            let key = |n: &str| {
                let n = if d.country_code.eq_ignore_ascii_case("co") {
                    let n = n.trim_start_matches(['#', ' ']);
                    colombian_plate(n).unwrap_or_else(|| n.to_owned())
                } else {
                    n.to_owned()
                };
                crate::index::schema::number_key(&n.split(whitespace).collect::<String>())
            };
            key(house) == key(&d.house_number)
        }
}

/// "Avenida Calle 43 S" as Bogotá's register writes it, "AC 43 S", and back: an avenue
/// that is also a numbered calle or carrera (AK).
fn bogota_road(road: &str) -> Option<String> {
    static LONG: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*(?:avenida|av\.?)\s+(calle|cl|carrera|kr|cra)\s+(.+)$").unwrap()
    });
    static SHORT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^\s*(ac|ak)\s+(.+)$").unwrap());
    if let Some(c) = LONG.captures(road) {
        let kind = match c[1].to_lowercase().as_str() {
            "calle" | "cl" => "AC",
            _ => "AK",
        };
        return Some(format!("{kind} {}", &c[2]));
    }
    SHORT.captures(road).map(|c| {
        let kind = if c[1].eq_ignore_ascii_case("ac") {
            "Calle"
        } else {
            "Carrera"
        };
        format!("Avenida {kind} {}", &c[2])
    })
}

/// A Colombian plate ("# 86B-18", "54D-35 S": the house 18 m along from the crossing road 86B)
/// as Bogotá's register keys it, "86B 18" and "54D 35 S". None when it is not one.
pub(crate) fn colombian_plate(n: &str) -> Option<String> {
    static PLATE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^\s*(?:#|no\.?)?\s*(\d+\s?[a-z]?(?:\s?bis)?)\s*-\s*(\d+)(\s+[a-z]+)?\s*$")
            .unwrap()
    });
    let c = PLATE.captures(n)?;
    Some(format!(
        "{} {}{}",
        c[1].replace(' ', ""),
        &c[2],
        c.get(3).map_or("", |m| m.as_str())
    ))
}

pub(crate) fn road_expansions(road: &str, cc: &str) -> Vec<String> {
    if cc.eq_ignore_ascii_case("pt") {
        return ctt_road(road);
    }
    if cc.eq_ignore_ascii_case("co") {
        return bogota_road(road).into_iter().collect();
    }
    if cc.eq_ignore_ascii_case("es") {
        // Spain's address registry leaves most articles and prepositions out of a road's name:
        // "Calle del Castillo" is its "CALLE CASTILLO".
        let road = normalize_place(road);
        let all: Vec<&str> = road.split(' ').collect();
        let mut words: Vec<&str> = all
            .iter()
            .copied()
            .filter(|w| !SPANISH_PARTICLES.contains(w))
            .collect();
        let kind = |w: &str| canon(w).is_some() || is_canonical(w);
        // and Basque names put the type last ("Belakasoro kalea" is its "CALLE BELAKASORO")
        if words.len() > 1 && kind(words[words.len() - 1]) && !kind(words[0]) {
            words.rotate_right(1);
        }
        return (words.iter().any(|w| !kind(w)) && words != all)
            .then(|| words.join(" "))
            .into_iter()
            .collect();
    }
    if cc.eq_ignore_ascii_case("za") {
        let road = normalize_place(road);
        return [(" straat", " street"), (" street", " straat")]
            .iter()
            .filter_map(|(from, to)| road.strip_suffix(from).map(|stem| format!("{stem}{to}")))
            .collect();
    }
    let german = ["ch", "de", "at"]
        .iter()
        .any(|c| cc.eq_ignore_ascii_case(c));
    if german {
        let road = normalize_place(road);
        let (prefix, word) = road.rsplit_once(' ').unwrap_or(("", &road));
        if !word.ends_with("strasse") {
            let mut found = vec![];
            // Correct the long suffix before the analyzer shortens an intact compound to …str.
            for width in 6..=8 {
                let Some(at) = word.len().checked_sub(width).filter(|&at| at > 2) else {
                    continue;
                };
                let Some((stem, suffix)) = word.split_at_checked(at) else {
                    continue;
                };
                if suffix.starts_with('s')
                    && stem.chars().all(char::is_alphabetic)
                    && crate::index::misspelt_word(suffix, "strasse")
                {
                    found.push(format!("{prefix} {stem}strasse").trim_start().to_owned());
                }
            }
            if !found.is_empty() {
                return found;
            }
        }
    }
    let (short, full): (&str, &[&str]) = match cc {
        "fr" | "FR" => ("r", &["rue"]),
        "no" | "NO" => ("vn", &["veien", "vegen"]),
        "nl" | "NL" => ("ln", &["laan"]),
        _ if german => ("str", &["strasse"]),
        _ => return vec![],
    };
    if short == "r" {
        return normalize_place(road)
            .strip_prefix("r ")
            .map(|rest| vec![format!("rue {rest}")])
            .unwrap_or_default();
    }
    // Full names like Solhavn and Koeln are not compound abbreviations.
    let marker = format!("{short}.");
    let road = road.to_lowercase();
    let Some((stem, rest)) = road.split_once(&marker) else {
        return vec![];
    };
    if !stem.ends_with(char::is_alphabetic) || rest.contains(&marker) {
        return vec![];
    }
    let stem = normalize_place(stem);
    let rest = normalize_place(rest);
    if !stem.rsplit(' ').next().is_some_and(|word| {
        word.chars().count() >= (if german { 2 } else { 3 })
            && word.chars().all(char::is_alphabetic)
    }) {
        return vec![];
    }
    full.iter()
        .map(|suffix| format!("{stem}{suffix} {rest}").trim_end().to_owned())
        .collect()
}

/// A Portuguese road as the national address register (CTT) writes it, which is how the index
/// stores most Portuguese houses: "Rua dos Farias" is "R FARIAS", "Bairro de Santa Bárbara" is
/// "BR S BARBARA", "Rua Tenente Valadim" is "R TEN VALADIM". The register abbreviates the
/// leading street type, most titles and Santa/Santo, and usually drops the particle after the
/// type. Only the abbreviations it uses for the large majority of its names are applied ("Marquês" is also a surname, "Monsenhor" mostly stays whole).
fn ctt_road(road: &str) -> Vec<String> {
    const TYPES: &[(&str, &str)] = &[
        ("praceta", "pct"),
        ("bairro", "br"),
        ("caminho", "cam"),
        ("urbanizacao", "urb"),
        ("sitio", "sit"),
        ("praca", "pc"),
        ("beco", "bc"),
        ("quinta", "qta"),
        ("calcada", "cc"),
        ("calcadinha", "ccnh"),
        ("monte", "mte"),
        ("casal", "csl"),
        ("vereda", "vr"),
        ("canada", "can"),
        ("quelha", "qlha"),
        ("rotunda", "rot"),
        ("escadinhas", "escnh"),
        ("escadas", "esc"),
        ("parque", "pq"),
        ("azinhaga", "az"),
        ("ladeira", "lad"),
        ("loteamento", "loteam"),
        ("jardim", "jrd"),
        ("conjunto", "conj"),
        ("zona", "zn"),
        ("terreiro", "terr"),
        ("rampa", "ramp"),
        ("aldeamento", "aldt"),
        ("herdade", "hrd"),
        ("viela", "vla"),
        ("patio", "pto"),
    ];
    const TITLES: &[(&str, &str)] = &[
        ("tenente", "ten"),
        ("comendador", "comend"),
        ("comandante", "cmdt"),
        ("almirante", "alm"),
        ("conselheiro", "cons"),
        ("visconde", "visc"),
        ("brigadeiro", "brig"),
        ("sargento", "sarg"),
        ("infante", "inf"),
        ("frei", "fr"),
        ("coronel", "cor"),
        ("arquitecto", "arq"),
        ("arquiteto", "arq"),
        ("duque", "dq"),
        ("embaixador", "emb"),
        ("doutora", "dra"),
        ("dom", "d"),
        ("dona", "d"),
        ("nossa", "nsa"),
        ("santa", "s"),
        ("santo", "s"),
    ];
    const PARTICLES: &[&str] = &["da", "do", "das", "dos", "de"];
    let road = normalize_place(road);
    let short = |table: &[(&str, &'static str)], w: &str| {
        table.iter().find(|(full, _)| *full == w).map(|(_, s)| *s)
    };
    // the street type, after a house number typed first ("12 rua dos farias")
    let at = road
        .split(' ')
        .position(|w| !w.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(0);
    let form = road
        .split(' ')
        .enumerate()
        .filter(|&(i, w)| i != at + 1 || !PARTICLES.contains(&w))
        .map(|(i, w)| {
            (i == at)
                .then(|| short(TYPES, w))
                .flatten()
                .or_else(|| short(TITLES, w))
                .unwrap_or(w)
        })
        .collect::<Vec<_>>()
        .join(" ");
    // a bare type ("Rua da") names no street
    if form.contains(' ') && form != road {
        vec![form]
    } else {
        vec![]
    }
}

/// "2955-243": no other country writes its postcodes so, which tells a query that names no country
/// to read its road as Portugal's register does. Only the written shape counts: "50667" could be
/// German or French, and a country-scoped spelling needs the country to be certain.
pub(crate) fn portuguese_postcode(postcode: &str) -> bool {
    let b = postcode.trim().as_bytes();
    b.len() == 8 && b[4] == b'-' && b[..4].iter().chain(&b[5..]).all(u8::is_ascii_digit)
}

/// Spanish, Catalan and Galician articles and prepositions.
const SPANISH_PARTICLES: &[&str] = &[
    "de", "del", "la", "las", "los", "el", "dels", "les", "els", "da", "do", "das", "dos",
];

/// A Polish road as the index may name it: without "ul." (ulica), which most sources leave
/// out, and without its given names' initials ("ul. M. Kopernika" is Mikołaja Kopernika's
/// street). Without a stated country, only a road written with "ul." is taken to be Polish.
pub(crate) fn bare_road(road: &str, cc: Option<&str>) -> Option<String> {
    let initial = |w: &str| {
        w.strip_suffix('.')
            .is_some_and(|l| l.chars().count() == 1 && l.chars().all(char::is_alphabetic))
    };
    let mut words: Vec<&str> = road.split_whitespace().collect();
    let ulica = words
        .first()
        .is_some_and(|w| matches!(w.to_lowercase().as_str(), "ul." | "ul" | "ulica"));
    if !cc.map_or(ulica, |c| c.eq_ignore_ascii_case("pl")) {
        return None;
    }
    let before = words.len();
    if ulica {
        words.remove(0);
    }
    words.retain(|w| !initial(w));
    (words.len() < before
        && words
            .iter()
            .any(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3))
    .then(|| words.join(" "))
}

/// One road, or one with a single word misspelt as the index corrects it: "West 54th Srteet" is
/// the candidate's "West 54th Street" when the index found it by that correction.
pub(crate) fn roads_agree(typed: &str, found: &str) -> bool {
    if (!typed.is_ascii() || !found.is_ascii())
        && (typed.contains(['路', '街', '道', '段', '巷', '弄'])
            || found.contains(['路', '街', '道', '段', '巷', '弄']))
    {
        let typed = fold(typed);
        return !typed.is_empty() && typed == fold(found);
    }
    let (typed, found) = (road_key(typed), road_key(found));
    if typed == found {
        return true;
    }
    let (a, b): (Vec<&str>, Vec<&str>) = (typed.split(' ').collect(), found.split(' ').collect());
    let differing: Vec<_> = a.iter().zip(&b).filter(|(x, y)| x != y).collect();
    a.len() == b.len() && matches!(&differing[..], [(x, y)] if crate::index::misspelt_word(x, y))
}

/// A road name with its type and direction words canonicalized ("S 5th Street" and "South 5th
/// Street" are one road).
fn road_key(road: &str) -> String {
    let road = normalize_place(road);
    let mut words: Vec<&str> = road.split(' ').map(|w| canon(w).unwrap_or(w)).collect();
    // Polish "ulica" ("ul."): some sources name a street with it, most without
    if words.len() > 1 && words[0] == "ul" {
        words.remove(0);
    }
    words.join(" ")
}

/// The stated house, road and postcode all equal the candidate's. A town the input names
/// wrongly or not at all (a trailing "IDX", a merged municipality's old village) can't outweigh
/// that.
pub fn house_road_postcode(
    address: &ParsedAddress,
    postcode: &str,
    details: Option<&AddressDetails>,
) -> bool {
    road_agrees(address, details)
        && details.is_some_and(|d| {
            !postcode.is_empty()
                && !d.postcode.is_empty()
                && normalize_postcode(postcode) == normalize_postcode(&d.postcode)
        })
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

/// British, Canadian, Dutch, Irish and Portuguese postcodes in full, which name a few buildings
/// rather than an area.
static UNIT_POSTCODE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([A-Z]{1,2}\d[A-Z\d]?\d[A-Z]{2}|[A-Z]\d[A-Z]\d[A-Z]\d|\d{4}[A-Z]{2}|[A-Z]\d{2}[0-9A-Z]{4}|\d{7})$")
        .unwrap()
});

pub fn choose_candidate(
    address: &ParsedAddress,
    country: &str,
    postcode: &str,
    candidates: &[FullAddress],
) -> Option<(usize, &'static str)> {
    // A region that only repeats the town ("Delhi" is a town and a state, "Tokyo" a city and a
    // prefecture) says nothing of the country: it would let a same-named place abroad outweigh
    // the country stated ("Tokyo, United States" is not Japan's).
    let city = part(address, "city");
    let repeated: Vec<&str> = ["state", "state_district", "country_region"]
        .into_iter()
        .filter(|key| {
            !city.is_empty()
                && !part(address, key).is_empty()
                && names_agree(city, part(address, key))
        })
        .collect();
    let mut alone;
    let address = if repeated.is_empty() {
        address
    } else {
        alone = address.clone();
        for key in repeated {
            alone.shift_remove(key);
        }
        &alone
    };
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
                    // another country's place of that name, in the region stated (or none)
                    hits.iter()
                        .copied()
                        .find(|&i| {
                            state_consistent(address, candidates[i].address_details.as_option())
                        })
                        .map(|i| (i, tier))
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
    // "1 Plaza La Fuente, Muro de Aguas, 26587": an area postcode several villages share is
    // centred on one of them; for an address not found, the stated village or town (not a city,
    // which holds several postcodes), with no postcode of its own to contradict it and near that
    // centre, agrees with both. A unit postcode ("SW8 1JQ") is finer than any village.
    let area_postcode = !part(address, "house_number").is_empty()
        && !UNIT_POSTCODE.is_match(&normalize_postcode(postcode).to_uppercase());
    let centres: Vec<(f32, f32)> = candidates
        .iter()
        .filter(|_| area_postcode)
        .filter(|item| postcode_agrees(postcode, item.address_details.as_option()))
        .filter_map(|item| item.geo_coordinates.as_option().map(|g| (g.lat, g.lon)))
        .collect();
    let city_near_postcode = candidates
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            let details = item.address_details.as_option();
            details.is_some_and(|d| d.postcode.is_empty() && d.city.is_empty())
                && city_agrees(address, details)
                && item.geo_coordinates.as_option().is_some_and(|g| {
                    centres.iter().any(|&(lat, lon)| {
                        ((g.lon - lon) * lat.to_radians().cos()).hypot(g.lat - lat) * 111.2 < 10.0
                    })
                })
        })
        .map(|(i, _)| i)
        .collect();
    if let Some(hit) = prefer(city_near_postcode, "postcode_city") {
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
    // "Multan, 66000, Punjab": a postcode in another region does not outweigh the stated
    // town there, when that town is a candidate. The postcode still wins when it is in the
    // stated region, or when nothing in that region was found.
    let region_stated = ["state", "state_district", "country_region"]
        .iter()
        .any(|key| !part(address, key).is_empty());
    if region_stated && !postcode.is_empty() {
        let in_region: Vec<usize> = candidates
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                let details = item.address_details.as_option();
                city_agrees(address, details) && state_consistent(address, details)
            })
            .map(|(i, _)| i)
            .collect();
        let elsewhere = candidates.iter().any(|item| {
            let details = item.address_details.as_option();
            postcode_agrees(postcode, details) && !state_consistent(address, details)
        });
        if elsewhere && let Some(hit) = prefer(in_region, "city_region") {
            return Some(hit);
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

/// House number, road and town all agree with the parsed address. Beside an exact house and
/// road, a town that is the candidate's with more words ("Bad Soden-Salmünster" for the village
/// Bad Soden it merged) counts too.
pub fn house_road_city(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    road_agrees(address, details)
        && (city_agrees(address, details) || city_related(address, details))
}

/// One of the town names is the other's first words.
fn city_related(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    let expected = normalize_place(part(address, "city"));
    let Some(d) = details else {
        return false;
    };
    let within = |long: &str, short: &str| {
        short.len() >= 5
            && long
                .strip_prefix(short)
                .is_some_and(|rest| rest.starts_with(' '))
    };
    !expected.is_empty()
        && [&d.city, &d.town, &d.village, &d.municipality]
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| normalize_place(s))
            .any(|actual| within(&expected, &actual) || within(&actual, &expected))
}

/// A Canadian unit/civic query as (building query, civic number), after literal house lookup fails.
pub fn canadian_building(query: &str) -> Option<(&str, &str)> {
    static UNIT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^[a-z0-9]+-([1-9][0-9]*[a-z]?(?:\s+[0-9]+/[0-9]+)?)\s+[^,\s]").unwrap()
    });
    let query = query.trim_matches(whitespace);
    let captures = UNIT.captures(query)?;
    if query.chars().count() > 256 {
        return None;
    }
    let mut canadian = false;
    for part in query.split(',').skip(1).map(str::trim) {
        let country = expected_country(&parsed_country(part));
        if country == "ca" {
            canadian |= !part.eq_ignore_ascii_case("ca");
        } else if !country.is_empty() {
            return None;
        }
    }
    let number = captures.get(1)?;
    canadian.then(|| (&query[number.start()..], number.as_str()))
}

/// "82A AV MERCIER, ..." as "82 A AV MERCIER, ..." and the number "82A": the parser reads a
/// suffixed number glued to it as part of the road, and spaced as the house.
pub fn spaced_house_suffix(query: &str) -> Option<(String, String)> {
    static GLUED: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^\s*([0-9]+)([A-Za-z])\s").unwrap());
    let c = GLUED.captures(query)?;
    let rest = &query[c.get(2)?.end()..];
    Some((
        format!("{} {}{rest}", &c[1], &c[2]),
        format!("{}{}", &c[1], &c[2]),
    ))
}

/// A Canadian address with its street's direction left out ("31 Thatcher Dr E" as "31 Thatcher
/// Dr", "162 North Klondike Hwy" as "162 Klondike Hwy"), with its house number and compact
/// postcode: some sources name the street without it. Only the postcode, which in Canada places a
/// block face, may confirm the house found.
pub fn without_street_direction(query: &str) -> Option<(String, String, String)> {
    static POSTCODE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b([A-Za-z][0-9][A-Za-z]) ?([0-9][A-Za-z][0-9])\b").unwrap());
    let (head, tail) = query.split_once(',')?;
    let postcode = POSTCODE.captures(tail)?;
    let postcode = format!("{}{}", &postcode[1], &postcode[2]).to_uppercase();
    let words: Vec<&str> = head.split_whitespace().collect();
    let number = *words.first()?;
    let direction = |w: &str| {
        let w = w.trim_end_matches('.').to_lowercase();
        let w = canon(&w).unwrap_or(w.as_str());
        matches!(w, "n" | "s" | "e" | "w" | "o" | "ne" | "nw" | "se" | "sw")
    };
    if !number.starts_with(|c: char| c.is_ascii_digit()) || words.len() < 4 {
        return None;
    }
    let at = if direction(words[words.len() - 1]) {
        words.len() - 1
    } else if direction(words[1]) {
        1
    } else {
        return None;
    };
    let kept: Vec<&str> = (words.iter().enumerate())
        .filter(|(i, _)| *i != at)
        .map(|(_, w)| *w)
        .collect();
    Some((
        format!("{},{tail}", kept.join(" ")),
        number.to_owned(),
        postcode,
    ))
}

/// `query` without the whitespace-separated words equal to `word` (case and dots aside).
pub fn without_word(query: &str, word: &str) -> String {
    let key = word.trim_matches('.').to_lowercase();
    query
        .split(whitespace)
        .filter(|t| t.trim_matches(['.', ',']).to_lowercase() != key)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `query` with each parenthesised remark ("Waldbronn (Reichenbach)", "Runkel-Dehrn (Germany)")
/// turned into a comma-separated part, or dropped when `keep` is false; None without one. The
/// parser files the remark inside the neighbouring field and then nothing matches.
pub fn without_parentheses(query: &str, keep: bool) -> Option<String> {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\(([^()]*)\)").unwrap());
    RE.is_match(query).then(|| {
        RE.replace_all(query, |c: &regex::Captures| {
            if keep {
                format!(", {}", c[1].trim())
            } else {
                String::new()
            }
        })
        .into_owned()
    })
}

/// `query` without a Danish unit written as a comma part of its own: floor and door ("Mogensgade
/// 9, st. th, 8500 Grenaa, Denmark"), a floor alone ("1.") or a door alone ("1", "3b"). It names no
/// place, yet libpostal swallows the house and postcode into the road and reads "th" as Thailand.
/// The building's house is the answer; the unit is not modelled. Only floor and door with a
/// th/tv/mf door stand without Denmark stated, and a part naming another country keeps the
/// query as written. None without such a part after a house.
pub fn without_danish_unit(query: &str) -> Option<String> {
    static UNIT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)^(?:(?:st|kl|\d{1,2})\.(?:\s*([a-zæøå]{1,5}|\d{1,4}[a-z]?))?|\d{1,3}[a-z]?|th|tv|mf)$",
        )
        .unwrap()
    });
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .collect();
    let at = (1..parts.len())
        .find(|&i| parts[i - 1].chars().any(|c| c.is_ascii_digit()) && UNIT.is_match(parts[i]))?;
    // the unit itself is left out: "1. tv" reads as Tuvalu and "st. 1" as São Tomé
    let countries: Vec<String> = parts
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != at)
        .map(|(_, p)| expected_country(&parsed_country(p)))
        .filter(|c| !c.is_empty())
        .collect();
    let side = UNIT
        .captures(parts[at])?
        .get(1)
        .is_some_and(|door| matches!(door.as_str().to_lowercase().as_str(), "th" | "tv" | "mf"));
    let danish = countries.iter().any(|c| c == "dk");
    (countries.iter().all(|c| c == "dk") && (danish || side)).then(|| {
        let mut kept = parts;
        kept.remove(at);
        kept.join(", ")
    })
}

/// A Danish address's supplementary place ("Pittevej 6, Aidt, 8881 Thorsø, Denmark") and the query
/// without it: between the house and a four-digit postcode part, with Denmark stated. libpostal
/// then reads the postcode as the house and the road as the town. None without such a part.
pub fn without_danish_place(query: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .collect();
    let postcoded = |p: &str| {
        let (code, town) = p.split_once(' ').unwrap_or((p, ""));
        code.len() == 4 && code.bytes().all(|b| b.is_ascii_digit()) && !town.is_empty()
    };
    let at = (1..parts.len().saturating_sub(1)).find(|&i| {
        parts[i - 1].chars().any(|c| c.is_ascii_digit())
            && postcoded(parts[i + 1])
            && !parts[i].chars().any(|c| c.is_ascii_digit())
            && (1..=4).contains(&parts[i].split_whitespace().count())
    })?;
    let countries: Vec<String> = parts
        .iter()
        .enumerate()
        .filter(|&(i, _)| i != at)
        .map(|(_, p)| expected_country(&parsed_country(p)))
        .filter(|c| !c.is_empty())
        .collect();
    (!countries.is_empty() && countries.iter().all(|c| c == "dk")).then(|| {
        let mut kept = parts.clone();
        kept.remove(at);
        (kept.join(", "), parts[at].to_owned())
    })
}

/// `query` without the unit of an Australian "unit/number" house ("71/206 Phillip Street,
/// Mount Pleasant QLD 4740"), which libpostal keeps as one house number the index may not
/// hold. The building's house answers then, as for "Unit 71, 206 Phillip Street". Australia
/// must be stated, or a part must end in a state code and postcode ("VIC 3065"); Czech,
/// Polish and other slashed numbers mean something else. None without such a number.
pub fn without_australian_unit(query: &str) -> Option<String> {
    static UNIT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[A-Za-z]?\d{1,5}[A-Za-z]?/(\d{1,6}[A-Za-z]?(?:-\d{1,6}[A-Za-z]?)?\s+\S.*)$")
            .unwrap()
    });
    static STATE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\b(?:act|nsw|nt|qld|sa|tas|vic|wa)\s+\d{4}$").unwrap());
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .collect();
    let (at, unit) = parts
        .iter()
        .enumerate()
        .find_map(|(i, p)| Some((i, UNIT.captures(p)?)))?;
    let countries: Vec<String> = parts
        .iter()
        .map(|p| expected_country(&parsed_country(p)))
        .filter(|c| !c.is_empty())
        .collect();
    let australian = countries.iter().any(|c| c == "au") || parts.iter().any(|p| STATE.is_match(p));
    if !australian || countries.iter().any(|c| c != "au") {
        return None;
    }
    let mut kept = parts.clone();
    kept[at] = unit.get(1)?.as_str();
    Some(kept.join(", "))
}

/// `query` with a leading region code moved behind the place it was fused with
/// ("NJ, WILLIAMSTOWN, United States", which libpostal reads as one state). A country
/// code ("CA, Toronto, Canada") and a postcode ("PA, 19560") are left alone. None when
/// the query does not start with such a code.
pub fn place_before_region_code(query: &str) -> Option<String> {
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .filter(|p| !p.is_empty())
        .collect();
    let (code, place) = (parts.first().copied()?, parts.get(1).copied()?);
    if code.len() != 2
        || !code.chars().all(|c| c.is_ascii_alphabetic())
        || place.chars().any(|c| c.is_ascii_digit())
        || place.chars().count() <= 2
        || !expected_country(&parsed_country(code)).is_empty()
    {
        return None;
    }
    let mut out = vec![place, code];
    out.extend_from_slice(&parts[2..]);
    Some(out.join(", "))
}

/// `query` with a comma put before a country named at its end without one ("Dublin 2 Ireland"),
/// which libpostal reads as a house and a town; None when no country ends it that way.
pub fn comma_before_country(query: &str) -> Option<String> {
    const ADJECTIVES: &[&str] = &["new", "south", "north", "west", "east", "republic", "of"];
    let (head, tail) = query.rsplit_once(',').unwrap_or(("", query));
    let words: Vec<&str> = tail.split(whitespace).filter(|w| !w.is_empty()).collect();
    (1..words.len().min(4)).rev().find_map(|n| {
        let (rest, country) = words.split_at(words.len() - n);
        let country = country.join(" ");
        let named = !country.chars().any(char::is_numeric)
            && !expected_country(&parsed_country(&country)).is_empty()
            && !ADJECTIVES.contains(&rest[rest.len() - 1].to_lowercase().as_str());
        let head = if head.is_empty() {
            String::new()
        } else {
            format!("{head},")
        };
        named.then(|| {
            format!("{head} {}, {country}", rest.join(" "))
                .trim()
                .to_owned()
        })
    })
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

/// `query` without the comma parts that only repeat another: the same text ("Kyiv, Kyiv,
/// Ukraine"), the same text without its postcode ("Madrid, Madrid 28007"), or a country by its
/// code beside its name ("Delhi, IN, India"). The parser reads such a repeat as one fused word
/// or as a region that contradicts the country (IN is Indiana), and nothing then matches. None
/// when no part repeats.
pub fn without_redundant_parts(query: &str) -> Option<String> {
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .filter(|p| !p.is_empty())
        .collect();
    let last = parts.len().checked_sub(1)?;
    // as written, not transliterated: "Варна, Varna" is left to the parser as before
    let written = |text: &str| {
        text.to_lowercase()
            .split(|c: char| c.is_whitespace() || c.is_ascii_punctuation())
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let keys: Vec<String> = parts.iter().map(|p| written(p)).collect();
    let countries: Vec<String> = parts
        .iter()
        .map(|p| expected_country(&parsed_country(p)))
        .collect();
    let unnumbered: Vec<String> = parts
        .iter()
        .map(|p| {
            let words: Vec<_> = p
                .split(whitespace)
                .filter(|w| !w.chars().any(char::is_numeric))
                .collect();
            written(&words.join(" "))
        })
        .collect();
    let redundant = |i: usize| {
        keys[i].chars().any(char::is_alphabetic)
            && (0..parts.len()).filter(|&j| j != i).any(|j| {
                let ends_country = |k: usize| k == last && !countries[k].is_empty();
                if keys[i] == keys[j] {
                    // the later copy goes, unless it is the country that ends the address
                    if i < j {
                        ends_country(j)
                    } else {
                        !ends_country(i)
                    }
                } else if !countries[i].is_empty() && countries[i] == countries[j] {
                    keys[i].len() <= 3 && keys[i].len() < keys[j].len()
                } else {
                    unnumbered[j] == keys[i] && keys[j] != keys[i]
                }
            })
    };
    let kept: Vec<&str> = (0..parts.len())
        .filter(|&i| !redundant(i))
        .map(|i| parts[i])
        .collect();
    (kept.len() < parts.len()).then(|| kept.join(", "))
}

/// `query` with a part that is a region's ISO 3166-2 code, written as "B.C.S" or "BCS" after the
/// first part, replaced by the name `region` gives for the code (uppercase, without the
/// country): libpostal takes such a code for a house or a town, and a code that only one
/// region has is as good as its name. None when no part is one. Two-letter codes are left to the
/// parser ("VA" is a state or a country), and so is a lowercase word without dots ("Vic").
pub fn with_region_names(query: &str, region: impl Fn(&str) -> Option<String>) -> Option<String> {
    let mut changed = false;
    let parts: Vec<String> = query
        .split(',')
        .enumerate()
        .map(|(i, part)| {
            let part = part.trim();
            let letters: String = part.chars().filter(|c| c.is_ascii_alphabetic()).collect();
            let shape = part.chars().all(|c| c.is_ascii_alphabetic() || c == '.')
                && letters.len() == 3
                && (part.contains('.') || letters.chars().all(|c| c.is_ascii_uppercase()));
            match shape.then(|| region(&letters.to_uppercase())).flatten() {
                Some(name) if i > 0 => {
                    changed = true;
                    name
                }
                _ => part.to_string(),
            }
        })
        .collect();
    changed.then(|| parts.join(", "))
}

/// The query is one country named more than once ("Luxembourg, Luxembourg", "Chile, Chile"):
/// the town of that name where the country has one, else the country.
pub fn is_repeated_country(query: &str) -> bool {
    let parts: Vec<&str> = query
        .split(',')
        .map(|p| p.trim_matches(whitespace))
        .filter(|p| !p.is_empty())
        .collect();
    parts.len() > 1
        && parts
            .iter()
            .all(|p| normalize_place(p) == normalize_place(parts[0]))
        && !expected_country(&parsed_country(parts[0])).is_empty()
}

/// The candidate's road is only a stated state's or town's name with a street type before or
/// after it ("Calle Baja California Sur" for "Baja California Sur"): the query named a place
/// there, not a road.
pub fn road_named_for_place(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    let Some(d) = details.filter(|d| !d.road.is_empty()) else {
        return false;
    };
    let name: Vec<String> = normalize_place(&d.road)
        .split(' ')
        .filter(|w| !(canon(w).is_some() || is_canonical(w)))
        .map(str::to_owned)
        .collect();
    let name = name.join(" ");
    name.len() >= 4
        && ["city", "state", "state_district", "country_region"]
            .iter()
            .any(|key| normalize_place(part(address, key)) == name)
}

/// A word or two of the query that is a postcode in the shape of `found_cc` and not of
/// `stated_cc` ("5757 BD" is a Dutch postcode, not a Belgian one). Read from the text because the
/// parser files postcodes it doesn't know under a house number or a road.
pub fn postcode_shaped_for(query: &str, stated_cc: &str, found_cc: &str) -> bool {
    query.split(',').any(|part| {
        let words: Vec<&str> = part.split(whitespace).filter(|w| !w.is_empty()).collect();
        let pairs = words.windows(2).map(|w| w.join(" "));
        words
            .iter()
            .map(|w| w.to_string())
            .chain(pairs)
            .any(|text| {
                postcode_shape(found_cc, &text) == Some(true)
                    && postcode_shape(stated_cc, &text) == Some(false)
            })
    })
}

/// The part of a multi-part query that names a country when the parser labelled it something
/// else ("Manama. P.O. Box 54482, Kingdom of Bahrain" gives house "kingdom of bahrain"). A part
/// the parser took for a region or town is left alone: "Georgia" after a town is a US state.
pub fn country_in_tail(query: &str, address: &ParsedAddress) -> Option<String> {
    let parts: Vec<_> = query
        .split(',')
        .map(|s| s.trim_matches(whitespace))
        .filter(|s| !s.is_empty())
        .collect();
    // A two-letter part is a state ("VA", "IN", "CA") more often than a country code, and no
    // country is written with digits.
    let last = *parts.last().filter(|l| {
        parts.len() >= 2 && l.chars().count() > 2 && !l.chars().any(|c| c.is_numeric())
    })?;
    let labelled = |label: &str| normalize_place(part(address, label)) == normalize_place(last);
    let places = [
        "city",
        "city_district",
        "suburb",
        "state",
        "state_district",
        "country_region",
    ];
    if places.iter().any(|label| labelled(label)) {
        return None;
    }
    let candidate = parsed_country(last);
    (!expected_country(&candidate).is_empty()).then(|| last.to_owned())
}

fn parsed_country(text: &str) -> ParsedAddress {
    ParsedAddress::from_iter([("country".to_owned(), text.to_owned())])
}

/// How many of the query's content words the candidate's texts account for, and how many there
/// are: a word is accounted for when it, or it joined to a neighbour ("M3J 3G8" is "M3J3G8"),
/// is one of the texts' words. Single letters and street-type words say nothing about a place.
pub fn coverage<'a>(query: &str, texts: impl IntoIterator<Item = &'a str>) -> (usize, usize) {
    // a candidate's English and local renderings repeat its names, and folding them dominates
    let mut distinct = std::collections::HashSet::new();
    let known: std::collections::HashSet<String> = texts
        .into_iter()
        .filter(|t| distinct.insert(*t))
        .flat_map(|t| fold(t).split(' ').map(str::to_owned).collect::<Vec<_>>())
        .collect();
    let words: Vec<String> = fold(query)
        .split(' ')
        .map(str::to_owned)
        .filter(|w| !w.is_empty())
        .collect();
    let content = |w: &String| {
        w.chars().any(|c| c.is_ascii_digit()) || (w.chars().count() >= 3 && !is_street_word(w))
    };
    let mut explained = 0;
    let mut total = 0;
    let mut seen = std::collections::HashSet::new();
    for (i, word) in words.iter().enumerate().filter(|(_, w)| content(w)) {
        if !seen.insert(word) {
            continue;
        }
        total += 1;
        let joined_next = words.get(i + 1).map(|n| format!("{word}{n}"));
        let joined_prev = i.checked_sub(1).map(|p| format!("{}{word}", words[p]));
        if known.contains(word)
            || joined_next.is_some_and(|j| known.contains(&j))
            || joined_prev.is_some_and(|j| known.contains(&j))
        {
            explained += 1;
        }
    }
    (explained, total)
}

/// Words that type a street or an administrative area in some language, and so say nothing of
/// which one: "Kota Bandung" would otherwise account for the "Kota" of "Kota Jkt Utara".
fn is_street_word(word: &str) -> bool {
    const TYPES: &[&str] = &[
        "kota",
        "kab",
        "kabupaten",
        "kec",
        "kecamatan",
        "kel",
        "kelurahan",
        "desa",
        "jalan",
        "provinsi",
        "city",
        "town",
        "village",
        "district",
        "county",
        "province",
        "region",
        "state",
        "municipality",
    ];
    crate::index::canon::is_canonical(word) || TYPES.contains(&word)
}

/// A postcode written as the country writes it: "1012LG" is "1012 LG", "h9r5z5" "H9R 5Z5". An
/// unrecognised shape is returned as it came.
pub fn format_postcode(country_code: &str, postcode: &str) -> String {
    static SHAPES: LazyLock<Vec<(&str, Regex, &str)>> = LazyLock::new(|| {
        [
            ("gb", r"^([A-Z]{1,2}\d[A-Z\d]?)(\d[A-Z]{2})$", "$1 $2"),
            ("nl", r"^(\d{4})([A-Z]{2})$", "$1 $2"),
            ("ca", r"^([A-Z]\d[A-Z])(\d[A-Z]\d)$", "$1 $2"),
            ("ie", r"^([A-Z]\d{2}|D6W)([0-9A-Z]{4})$", "$1 $2"),
            ("pl", r"^(\d{2})(\d{3})$", "$1-$2"),
            ("pt", r"^(\d{4})(\d{3})$", "$1-$2"),
            ("br", r"^(\d{5})(\d{3})$", "$1-$2"),
            ("jp", r"^(\d{3})(\d{4})$", "$1-$2"),
            ("us", r"^(\d{5})(\d{4})$", "$1-$2"),
            ("se", r"^(\d{3})(\d{2})$", "$1 $2"),
            ("cz", r"^(\d{3})(\d{2})$", "$1 $2"),
            ("sk", r"^(\d{3})(\d{2})$", "$1 $2"),
            ("gr", r"^(\d{3})(\d{2})$", "$1 $2"),
        ]
        .into_iter()
        .map(|(cc, re, to)| (cc, Regex::new(re).unwrap(), to))
        .collect()
    });
    let country_code = country_code.to_lowercase();
    let compact: String = postcode
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase();
    // a different alphabet or punctuation ("LV-1050") is not ours to rewrite
    if compact.is_empty()
        || postcode
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && !" -".contains(c))
    {
        return postcode.to_owned();
    }
    SHAPES
        .iter()
        .find(|(cc, re, _)| *cc == country_code && re.is_match(&compact))
        .map(|(_, re, to)| re.replace(&compact, *to).into_owned())
        // an outcode or routing key ("rh14") still has the case the country writes
        .or_else(|| {
            (["gb", "ca", "nl", "ie"].contains(&country_code.as_str())
                && postcode_shape(&country_code, &compact) == Some(true))
            .then(|| postcode.trim().to_uppercase())
        })
        .unwrap_or_else(|| postcode.to_owned())
}

/// Whether `postcode` could be one of the country's: a US ZIP is no Canadian postcode. A country
/// whose shape isn't listed fits anything.
pub fn postcode_fits(country_code: &str, postcode: &str) -> bool {
    postcode_shape(country_code, postcode).is_none_or(|fits| fits)
}

/// Whether `postcode` fits the country's shape, or None when the shape isn't known.
pub fn postcode_shape(country_code: &str, postcode: &str) -> Option<bool> {
    static SHAPES: LazyLock<Vec<(&str, Regex)>> = LazyLock::new(|| {
        [
            ("gb", r"^[A-Z]{1,2}\d[A-Z\d]?(\d[A-Z]{2})?$"),
            ("nl", r"^\d{4}([A-Z]{2})?$"),
            ("ca", r"^[A-Z]\d[A-Z](\d[A-Z]\d)?$"),
            ("ie", r"^[A-Z]\d{2}([0-9A-Z]{4})?$"),
            ("pt", r"^\d{4}(\d{3})?$"),
            ("br", r"^\d{5}(\d{3})?$"),
            ("jp", r"^\d{3}(\d{4})?$"),
            ("us", r"^\d{5}(\d{4})?$"),
            ("at", r"^\d{4}$"),
            ("hu", r"^\d{4}$"),
            ("be", r"^\d{4}$"),
            ("ch", r"^\d{4}$"),
            ("dk", r"^\d{4}$"),
            ("no", r"^\d{4}$"),
            ("de", r"^\d{5}$"),
            ("es", r"^\d{5}$"),
            ("fi", r"^\d{5}$"),
            ("fr", r"^\d{5}$"),
            ("it", r"^\d{5}$"),
            ("pl", r"^\d{5}$"),
            ("se", r"^\d{5}$"),
        ]
        .into_iter()
        .map(|(cc, re)| (cc, Regex::new(re).unwrap()))
        .collect()
    });
    let country_code = country_code.to_lowercase();
    let compact = postcode
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase();
    SHAPES
        .iter()
        .find(|(cc, _)| *cc == country_code)
        .map(|(_, re)| re.is_match(&compact))
}

/// The stated region is the candidate's, by name or by ISO 3166-2 code ("PA" for Pennsylvania),
/// or none is stated.
pub fn state_consistent(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    let stated: Vec<_> = ["state", "state_district", "country_region"]
        .iter()
        .map(|key| part(address, key))
        .filter(|s| !s.is_empty())
        .collect();
    if stated.is_empty() {
        return true;
    }
    region_agrees(address, details)
        || details.is_some_and(|d| {
            d.region_codes
                .iter()
                .filter(|c| c.admin_level < 6)
                .any(|c| {
                    c.code
                        .rsplit('-')
                        .next()
                        .is_some_and(|suffix| stated.iter().any(|s| s.eq_ignore_ascii_case(suffix)))
                })
        })
}

/// The stated state is a two-letter code ("MA") that the candidate's own ISO 3166-2 codes
/// contradict: "Dorchester, MA" is not Dorchester in Dorset or in Wisconsin. A state named in
/// full, or a candidate with no codes, says nothing.
pub fn state_code_conflict(address: &ParsedAddress, details: Option<&AddressDetails>) -> bool {
    let stated = part(address, "state");
    stated.len() == 2
        && stated.chars().all(|c| c.is_ascii_alphabetic())
        && details.is_some_and(|d| {
            // a subdivision below a region (ES-MA, Málaga) is no state code
            let mut codes = d
                .region_codes
                .iter()
                .filter(|c| c.admin_level < 6)
                .peekable();
            codes.peek().is_some()
                && !codes.any(|c| {
                    c.code
                        .rsplit('-')
                        .next()
                        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(stated))
                })
                && !region_agrees(address, Some(d))
        })
}

/// A town the data holds no postcodes for ("Mt. Gretna, PA 17064") answers a postcode it can't
/// confirm when nothing contradicts it: the data doesn't know the postcode at all (`holds_postcode`
/// takes the country code), it has the shape of the town's country, and the stated region is
/// the town's.
pub fn unpostcoded_candidate(
    address: &ParsedAddress,
    country: &str,
    postcode: &str,
    candidates: &[FullAddress],
    holds_postcode: impl Fn(&str) -> bool,
) -> Option<usize> {
    candidates.iter().position(|item| {
        let details = item.address_details.as_option();
        !postcode.is_empty()
            && !details.is_some_and(|d| holds_postcode(&d.country_code))
            && country_matches(country, details)
            && details.is_some_and(|d| {
                d.postcode.is_empty() && postcode_shape(&d.country_code, postcode) == Some(true)
            })
            && city_agrees(address, details)
            && state_consistent(address, details)
    })
}

/// Whether the parser's `po_box` text is one: libpostal also files "CC Baarle-Nassau" there.
pub fn is_po_box(text: &str) -> bool {
    const MARKERS: &[&str] = &[
        "box",
        "po",
        "pobox",
        "postbus",
        "postfach",
        "apartado",
        "apdo",
        "casilla",
        "casella",
        "bp",
        "cp",
        "caixa",
        "boite",
        "boîte",
        "postboks",
        "postlåda",
    ];
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| MARKERS.contains(&w))
}
