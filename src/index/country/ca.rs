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
