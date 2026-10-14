use crate::{
    index::{canon::canon, schema::fold},
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
    let (expected, expected_core) = place_keys(expected);
    let (actual, actual_core) = place_keys(actual);
    !expected.is_empty()
        && (expected == actual || (!expected_core.is_empty() && expected_core == actual_core))
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
        && road_key(road) == road_key(&d.road)
        && house.split(whitespace).collect::<String>().to_lowercase()
            == d.house_number
                .split(whitespace)
                .collect::<String>()
                .to_lowercase()
}

/// A road name with its type and direction words canonicalized ("S 5th Street" and "South 5th
/// Street" are one road).
fn road_key(road: &str) -> String {
    normalize_place(road)
        .split(' ')
        .map(|w| canon(w).unwrap_or(w))
        .collect::<Vec<_>>()
        .join(" ")
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
    let known: std::collections::HashSet<String> = texts
        .into_iter()
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
            d.region_codes.iter().any(|c| {
                c.code
                    .rsplit('-')
                    .next()
                    .is_some_and(|suffix| stated.iter().any(|s| s.eq_ignore_ascii_case(suffix)))
            })
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
