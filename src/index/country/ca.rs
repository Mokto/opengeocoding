use crate::index::canon::canon;
use crate::matching::*;
use regex::Regex;
use std::sync::LazyLock;

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
    let number = captures.get(1)?;
    states_canada(query).then(|| (&query[number.start()..], number.as_str()))
}

/// Whether a later comma part names Canada and none names another country. A bare `CA` is
/// also California's code, so it does not count.
fn states_canada(query: &str) -> bool {
    let mut canadian = false;
    for part in query.split(',').skip(1).map(str::trim) {
        let country = expected_country(&parsed_country(part));
        if country == "ca" {
            canadian |= !part.eq_ignore_ascii_case("ca");
        } else if !country.is_empty() {
            return false;
        }
    }
    canadian
}

/// A Canadian address whose second comma part is only a number ("471 Upper Lakeville Road, 2,
/// Upper Lakeville, Nova Scotia, Canada": OSM's `addr:district`, or a unit), as (the query without that part, the civic number). libpostal
/// reads the number into the road or as the house. No Canadian place or postcode is a bare
/// number, so the building answers; the unit or district stays unverified. The civic number is
/// kept as written (`20-24`, `29 1/2`), so the retry is the literal one before any unit reading.
pub fn without_bare_number(query: &str) -> Option<(String, String)> {
    if query.chars().count() > 256 || !states_canada(query) {
        return None;
    }
    let parts: Vec<&str> = query.split(',').map(str::trim).collect();
    let bare = |p: &str| (1..=5).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit());
    let mut bares = (1..parts.len() - 1).filter(|&i| bare(parts[i]));
    let (Some(1), None) = (bares.next(), bares.next()) else {
        return None;
    };
    let number = civic_number(parts[0])?;
    let kept: Vec<&str> = (parts.iter().enumerate())
        .filter(|(i, _)| *i != 1)
        .map(|(_, p)| *p)
        .collect();
    Some((kept.join(", "), number))
}

/// The civic number of a Canadian street part, as written (`20-24`, `29 1/2`): first or last,
/// None when it is unclear which word it is.
fn civic_number(street: &str) -> Option<String> {
    let words: Vec<&str> = street.split_whitespace().collect();
    let civic = |w: &str| {
        w.starts_with(|c: char| c.is_ascii_digit() && c != '0')
            && w.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
    };
    let fraction = |w: &str| {
        w.split_once('/').is_some_and(|(a, b)| {
            [a, b]
                .iter()
                .all(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
        })
    };
    // a spaced letter may be the civic suffix or the street's own name ("55 A Street")
    let letter = |w: &str| w.len() == 1 && w.bytes().all(|c| c.is_ascii_alphabetic());
    // "93 Highway 4548": a number at both ends leaves the civic one unknown
    let (first, last) = (civic(words.first()?), civic(words.last()?));
    let number = match words[..] {
        [n, second, _, ..] if first && !last && fraction(second) => format!("{n} {second}"),
        [n, second, ..] if first && !last && !letter(second) => n.to_owned(),
        [_, .., n] if !first && last => n.to_owned(),
        _ => return None,
    };
    Some(number)
}

/// A Canadian address with Statistics Canada's census-subdivision designator as a comma part of
/// its own after the subdivision's name (`Annapolis, Subd. B`, `East River, Part 1`, as the
/// National Address Register's subdivision names read), as (the query without it, the civic
/// number). No place is named so, and libpostal reads it into the road or the town; the name
/// before it stays to qualify the house.
pub fn without_subdivision_part(query: &str) -> Option<(String, String)> {
    static PART: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)^(?:subd\.?|subdivision|part)\s+[a-z0-9]{1,2}$").unwrap()
    });
    if query.chars().count() > 256 || !states_canada(query) {
        return None;
    }
    let parts: Vec<&str> = query.split(',').map(str::trim).collect();
    let designator = |i: usize| {
        (2..parts.len() - 1).contains(&i)
            && PART.is_match(parts[i])
            && !PART.is_match(parts[i - 1])
            // a name, not a postcode ("B0S 1L0")
            && (parts[i - 1].split_whitespace())
                .any(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3)
    };
    let marked: Vec<usize> = (0..parts.len())
        .filter(|&i| PART.is_match(parts[i]))
        .collect();
    if marked.is_empty() || !marked.iter().all(|&i| designator(i)) {
        return None;
    }
    let number = civic_number(parts[0])?;
    let kept: Vec<&str> = (parts.iter().enumerate())
        .filter(|&(i, _)| !designator(i))
        .map(|(_, p)| *p)
        .collect();
    Some((kept.join(", "), number))
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
    let direction = |w: &str| direction(w).is_some();
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

/// A word's street direction, folded to one form (`Ouest`, `O` and `W` are all "w").
fn direction(word: &str) -> Option<&'static str> {
    let w = word.trim_end_matches('.').to_lowercase();
    match canon(&w).unwrap_or(w.as_str()) {
        "n" => Some("n"),
        "s" => Some("s"),
        "e" => Some("e"),
        "w" | "o" => Some("w"),
        "ne" => Some("ne"),
        "nw" => Some("nw"),
        "se" => Some("se"),
        "sw" => Some("sw"),
        _ => None,
    }
}

/// Whether `road` has a street direction the query's street part does not: `6e Rue O` for
/// `870 6e Rue Est`. A source that leaves the direction out is no such conflict.
pub fn other_street_direction(query: &str, road: &str) -> bool {
    let head = query.split(',').next().unwrap_or("");
    let asked: Vec<_> = head.split_whitespace().filter_map(direction).collect();
    road.split_whitespace()
        .filter_map(direction)
        .any(|d| !asked.contains(&d))
}

/// The forms of a Canadian street part with one possessive written the other way ("57 Roberts
/// Rd S" as "57 Robert's Rd S", "10B Dick's Sq" as "10B Dicks Sq"), with the civic number and the
/// compact postcode, at most four. Canada Post's mailing form drops the apostrophe that some
/// sources keep, and the index reads "Robert's" as two words. Only the stated postcode, which
/// places a block face, may confirm the house found.
pub fn possessive_forms(query: &str) -> Vec<(String, String, String)> {
    static POSTCODE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b([A-Za-z][0-9][A-Za-z]) ?([0-9][A-Za-z][0-9])\b").unwrap());
    let Some((head, tail)) = query.split_once(',') else {
        return vec![];
    };
    let Some(postcode) = POSTCODE.captures(tail) else {
        return vec![];
    };
    let postcode = format!("{}{}", &postcode[1], &postcode[2]).to_uppercase();
    let words: Vec<&str> = head.split_whitespace().collect();
    let Some(&number) = words.first() else {
        return vec![];
    };
    if !number.starts_with(|c: char| c.is_ascii_digit()) || words.len() < 3 {
        return vec![];
    }
    let mut forms = vec![];
    for (i, w) in words.iter().enumerate().skip(1) {
        let other = if let Some(stem) = ["'s", "'S", "’s", "’S"]
            .iter()
            .find_map(|end| w.strip_suffix(end))
        {
            format!("{stem}{}", &w[w.len() - 1..])
        } else if w.chars().count() >= 4
            && w.chars().all(char::is_alphabetic)
            && w.ends_with(['s', 'S'])
            && canon(&w.to_lowercase()).is_none()
        {
            format!("{}'{}", &w[..w.len() - 1], &w[w.len() - 1..])
        } else {
            continue;
        };
        let mut changed = words.clone();
        changed[i] = &other;
        forms.push((
            format!("{},{tail}", changed.join(" ")),
            number.to_owned(),
            postcode.clone(),
        ));
        if forms.len() == 4 {
            break;
        }
    }
    forms
}
