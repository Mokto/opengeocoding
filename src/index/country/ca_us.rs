use super::ca;
use crate::matching::{country_named, normalize_postcode};
use regex::Regex;
use std::sync::LazyLock;

fn zip_code(text: &str) -> Option<&str> {
    let (zip, extension) = text.split_once('-').unwrap_or((text, ""));
    let digits = |s: &str, len| s.len() == len && s.bytes().all(|b| b.is_ascii_digit());
    (digits(zip, 5) && (text == zip || digits(extension, 4))).then_some(zip)
}

fn before_unit(text: &str) -> Option<&str> {
    static UNIT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i-u)(?:^| )(?:(?:apt|apartment|unit|ste|suite|lot|rm|room|bldg|trlr|spc|#)\.? |#)[a-z0-9-]{1,6}$")
            .unwrap()
    });
    UNIT.find(text).map(|unit| text[..unit.start()].trim_end())
}

pub(crate) struct Building<'a> {
    pub query: String,
    pub number: &'a str,
    pub road: String,
    pub region: String,
    pub postcode: String,
    pub country: &'static str,
}

/// A street and town followed by a North American region code and postal code. Keep both
/// constraints: a bare state code can look like a country, and a detached ZIP+4 like a house.
pub(crate) fn building(query: &str) -> Option<Building<'_>> {
    if !query.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let parts: Vec<_> = query.split(',').take(7).map(str::trim).collect();
    let [street, tail @ ..] = parts.as_slice() else {
        return None;
    };
    let (number, _) = street.split_once(char::is_whitespace)?;
    if !number.bytes().all(|b| b.is_ascii_digit()) {
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
    let street = match rest {
        [_] => building.unwrap_or(street),
        [unit, _] if building.is_none() && before_unit(unit) == Some("") => street,
        _ => return None,
    };
    let town = rest.last()?.trim();
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
