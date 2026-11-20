use super::ca;
use crate::matching::{country_named, normalize_place, normalize_postcode};
use regex::Regex;
use std::sync::LazyLock;

fn zip_code(text: &str) -> Option<&str> {
    let (zip, extension) = text.split_once('-').unwrap_or((text, ""));
    let digits = |s: &str, len| s.len() == len && s.bytes().all(|b| b.is_ascii_digit());
    (digits(zip, 5) && (text == zip || digits(extension, 4))).then_some(zip)
}

fn before_unit(text: &str) -> Option<&str> {
    static UNIT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i-u)(?:^|[ -])(?:(?:apt|apartment|unit|ste|suite|studio|lot|rm|room|bldg|build|trlr|spc)\.?[ -]+|#\s*)[a-z0-9-]{1,6}(?:(?: and | & )[a-z0-9-]{1,6})?$")
            .unwrap()
    });
    UNIT.find(text)
        .map(|unit| text[..unit.start()].trim_end_matches([' ', '-']))
}

fn bare_unit(text: &str) -> bool {
    static UNIT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i-u)^(?:[0-9]{1,4}[a-z]?|[a-z])$").unwrap());
    UNIT.is_match(text)
}

fn civic_number(text: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    digits(text)
        || text
            .split_once('-')
            .is_some_and(|(a, b)| digits(a) && digits(b))
}

pub(crate) struct Building<'a> {
    pub query: String,
    pub number: &'a str,
    pub road: String,
    pub region: String,
    pub postcode: String,
    pub country: &'static str,
    pub town: &'a str,
}

impl Building<'_> {
    pub(crate) fn road_aliases(&self) -> Vec<String> {
        let region = self.region.split_once('-').map_or("", |(_, code)| code);
        road_aliases(&self.road, self.country, region)
    }

    pub(crate) fn rewrite(&self, road: &str) -> String {
        let (_, tail) = self.query.split_once(", ").expect("structured address");
        format!("{} {road}, {tail}", self.number)
    }
}

pub(crate) fn same_road_in_postcode(written: &str, indexed: &str) -> bool {
    use crate::index::canon::canon;
    let written = normalize_place(written);
    let indexed = normalize_place(indexed);
    let key = |road: &str| {
        road.split_whitespace()
            .map(|w| canon(w).unwrap_or(w))
            .collect::<String>()
    };
    key(&written) == key(&indexed)
}

fn road_aliases(road: &str, country: &str, region: &str) -> Vec<String> {
    let words = road.to_ascii_lowercase().replace('-', " ");
    if let Some((prefix, route)) = words.rsplit_once(' ')
        && (1..=4).contains(&route.len())
        && route.bytes().all(|b| b.is_ascii_digit())
    {
        if country == "us" {
            if prefix.eq_ignore_ascii_case(region) {
                return vec![
                    format!("State Route {route}"),
                    format!("State Highway {route}"),
                ];
            }
            if matches!(prefix, "co rd" | "county rd" | "county road" | "cr") {
                return vec![format!("CR {route}"), format!("County Road {route}")];
            }
        } else if matches!(prefix, "sideroad" | "side road" | "side rd") {
            return vec![format!("{route} Siderd"), format!("{route} Sideroad")];
        }
    }
    if country == "ca"
        && let Some((prefix, name)) = road.split_once(' ')
        && prefix.eq_ignore_ascii_case("chem.")
    {
        return vec![format!("Chemin {name}")];
    }
    vec![]
}

/// A street and town followed by a North American region code and postal code. Keep both
/// constraints: a bare state code can look like a country, and a detached ZIP+4 like a house.
pub(crate) fn building(query: &str) -> Option<Building<'_>> {
    if !query.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let parts: Vec<_> = query.split(',').take(10).map(str::trim).collect();
    if parts.len() == 10 {
        return None;
    }
    let [street, tail @ ..] = parts.as_slice() else {
        return None;
    };
    let (number, _) = street.split_once(char::is_whitespace)?;
    if !civic_number(number) {
        return None;
    }
    let (last, rest) = tail.split_last()?;
    let explicit = if last.chars().any(|c| c.is_ascii_digit()) {
        None
    } else {
        Some(country_named(last))
    };
    let tail = if explicit.is_some() { rest } else { tail };
    let (last, rest) = tail.split_last()?;
    let (region, postcode, rest, joined) = match last.split_once(char::is_whitespace) {
        Some((region, postcode))
            if region.len() == 2 && region.bytes().all(|b| b.is_ascii_alphabetic()) =>
        {
            (region, postcode.trim(), rest, true)
        }
        _ => {
            let (region, rest) = rest.split_last()?;
            (*region, *last, rest, false)
        }
    };
    if region.len() != 2 || !region.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let postcode = postcode
        .split_once(char::is_whitespace)
        .filter(|(part, _)| part.eq_ignore_ascii_case(region))
        .map_or(postcode, |(_, code)| code.trim());
    let (country, label, postcode) = if ca::province_and_postcode(&format!("{region} {postcode}")) {
        let compact = normalize_postcode(postcode).to_ascii_uppercase();
        (
            "ca",
            "Canada",
            format!("{} {}", &compact[..3], &compact[3..]),
        )
    } else {
        ("us", "United States", zip_code(postcode)?.to_owned())
    };
    if explicit.as_deref().is_some_and(|stated| stated != country) {
        return None;
    }
    let building = before_unit(street);
    if joined && rest.len() == 1 && building.is_none() && !last.contains('-') {
        return None;
    }
    let rest = if joined && rest.last().is_some_and(|p| p.eq_ignore_ascii_case(region)) {
        &rest[..rest.len() - 1]
    } else {
        rest
    };
    let town = *rest.last()?;
    let street = match rest {
        [_] => building.unwrap_or(street),
        [unit, _] if before_unit(unit) == Some("") => building.unwrap_or(street),
        [unit, _] if building.is_none() && bare_unit(unit) => street,
        [repeated_town, repeated_region, unit, _]
            if normalize_place(repeated_town) == normalize_place(town)
                && repeated_region.eq_ignore_ascii_case(region)
                && before_unit(unit) == Some("") =>
        {
            building.unwrap_or(street)
        }
        _ => return None,
    };
    let (_, road) = street.split_once(char::is_whitespace)?;
    if !town.chars().any(char::is_alphabetic) || !road.chars().any(char::is_alphabetic) {
        return None;
    }
    let region = region.to_ascii_uppercase();
    let rewritten = format!("{street}, {town}, {region} {postcode}, {label}");
    (rewritten != query).then(|| Building {
        query: rewritten,
        number,
        road: road.trim().to_owned(),
        region: format!("{}-{region}", country.to_ascii_uppercase()),
        postcode,
        country,
        town,
    })
}

#[cfg(test)]
mod tests {
    use super::building;

    #[test]
    fn normalizes_only_addresses_with_a_compatible_country() {
        for query in [
            "14 Meadow Rd, Suite 27, Bayport, IL, 60001-4321, Canada",
            "7 Pine Ridge Rd, Unit 27, Delhi, ON, N0B1A0, Australia",
            "Virtual Office, Delhi, ON, N0B1A0",
        ] {
            assert!(building(query).is_none(), "{query}");
        }
    }
}
