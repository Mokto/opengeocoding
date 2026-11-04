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

/// A Canadian address with parts between its street and its full postcode (`18 Sydney WAY,
/// Crossroads, PE C1B 4M4, Canada`), as (the street, postcode and what follows it, the parts
/// left out, the civic number, the compact postcode). The register's community is often not the
/// place the index files the house under (`Mermaid`), and a municipality's name can hold commas
/// (`Stanley Bridge, Hope River, Bayview, Cavendish and North Rustico`); the postcode, which
/// places a block face, stays to confirm the house. The province beside the postcode goes too:
/// the postcode's own place can be another than the house's, and the postcode names the
/// province.
pub fn without_place_parts(query: &str) -> Option<(String, Vec<String>, String, String)> {
    static POSTCODE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\b([A-Za-z][0-9][A-Za-z]) ?([0-9][A-Za-z][0-9])\b").unwrap());
    if query.chars().count() > 256 || !states_canada(query) {
        return None;
    }
    let parts: Vec<&str> = query.split(',').map(str::trim).collect();
    let at = parts.iter().position(|p| POSTCODE.is_match(p))?;
    if at < 2 {
        return None;
    }
    let number = civic_number(parts[0])?;
    let postcode = POSTCODE.captures(parts[at])?;
    let written = postcode.get(0)?.as_str();
    let postcode = format!("{}{}", &postcode[1], &postcode[2]).to_uppercase();
    let kept: Vec<&str> = [parts[0], written]
        .into_iter()
        .chain(parts[at + 1..].iter().copied())
        .collect();
    let dropped = parts[1..at].iter().map(|p| p.to_string()).collect();
    Some((kept.join(", "), dropped, number, postcode))
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

/// A Canadian house filed under a place that is not its town names instead the local government
/// whose polygon holds it and that government's settlement nearest the house (a town's or city's
/// distance divided by `TOWN_REACH`) within 10 km. Two such places: New Brunswick's localities
/// with an area, its civil parishes, which hold every house while its towns and cities are points
/// under them, and whose parent is one of the local governments they span, not always the
/// house's (`Moncton Parish` under `Maple Hills Rural Community`, where Moncton's houses lie in
/// `City of Moncton`); and a town under another government than the house's, which the builder
/// gives a house where no locality's area holds it and that town's point is the nearest (Mount
/// Pearl's, for houses of St. John's).
pub(in crate::index) fn reverse_locality(
    index: &crate::index::Index,
    hit: &crate::index::Hit,
    address: &mut serde_json::Map<String, serde_json::Value>,
    display: &mut Vec<String>,
) {
    use crate::index::nearby::TOWN_REACH;
    let crate::index::Hit::Street {
        place,
        x,
        y,
        number: Some(_),
        ..
    } = hit
    else {
        return;
    };
    let pl = |p: u32| &index.places[p as usize];
    let Some(locality) = index.places.get(*place as usize) else {
        return;
    };
    if locality.cc.as_ref() != "ca" || locality.subtype.as_ref() != "locality" {
        return;
    }
    let Some(government) = index
        .area_at(*y, *x)
        .0
        .filter(|&c| pl(c).subtype.as_ref() == "county" && pl(c).cc.as_ref() == "ca")
    else {
        return;
    };
    let chain = index.chain(*place);
    let parish = locality.class.is_empty() && index.region_code(&chain) == Some("CA-NB");
    let counties: Vec<u32> = (chain.iter().copied())
        .filter(|&p| pl(p).subtype.as_ref() == "county")
        .collect();
    // a town under no county whose own point lies outside the government holding the house
    // (Torbay's, for houses of St. John's); one under that government (North Rustico, beside
    // its resort municipality) is the house's
    let outside = counties.is_empty()
        && !locality.class.is_empty()
        && index.area_at(locality.y as f64, locality.x as f64).0 != Some(government);
    let elsewhere = outside || counties.iter().any(|&p| p != government);
    if !parish && !elsewhere {
        return;
    }
    let settlement = |p: u32| {
        matches!(pl(p).class.as_ref(), "city" | "town" | "village" | "hamlet")
            && pl(p).cc.as_ref() == "ca"
            && index.area_at(pl(p).y as f64, pl(p).x as f64).0 == Some(government)
    };
    let reach = |p: u32| {
        let reach = match pl(p).class.as_ref() {
            "city" | "town" => TOWN_REACH,
            _ => 1.0,
        };
        ((pl(p).x as f64, pl(p).y as f64), reach)
    };
    let Some((town, _)) = index
        .localities
        .nearest_by_reach(*y, *x, 10_000.0, TOWN_REACH, reach, settlement)
    else {
        return;
    };
    let old: Vec<_> = ["city", "town", "village", "county"]
        .iter()
        .filter_map(|key| address.remove(*key))
        .collect();
    let mut at = 0;
    display.retain(|name| {
        at += 1;
        at <= 2 || !old.iter().any(|value| value.as_str() == Some(name))
    });
    let (town, government) = (pl(town), pl(government));
    let at = 2.min(display.len());
    display.insert(at, town.name(true).to_string());
    display.insert(at + 1, government.name(true).to_string());
    address.insert(town.key().into(), town.name(true).into());
    address.insert("county".into(), government.name(true).into());
}
