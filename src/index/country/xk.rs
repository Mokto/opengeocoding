use crate::matching::{expected_country, parsed_country};

/// libpostal knows no Kosovo: a "Kosovo" written last reads as a road or a town and takes the
/// parts before it along ("Voskopoja 55, Bob, Kosovo" gives road "bob kosovo"), and it reads
/// few of its Albanian street names as roads ("46 Tumat Dardane" is a house name). An address
/// written as a numbered street, towns or a postcode, and the country is read here instead:
/// the number is the street part's last word with a digit, or else its first ("12 Qershori 81"
/// is number 81 of 12 Qershori), a part of five digits the postcode, the last town the city
/// and those before it suburbs. None for any other shape.
pub(crate) fn parsed(q: &str) -> Option<Vec<(String, String)>> {
    let parts: Vec<&str> = q.split(',').map(str::trim).collect();
    let (country, rest) = parts.split_last()?;
    let (street, places) = rest.split_first()?;
    if expected_country(&parsed_country(country)) != "xk" {
        return None;
    }
    let words: Vec<&str> = street.split_whitespace().collect();
    let numbered = |w: &&str| w.chars().any(|c| c.is_ascii_digit());
    let at = match words[..] {
        [_, _, ..] if numbered(&words[words.len() - 1]) => words.len() - 1,
        [first, _, ..] if numbered(&first) => 0,
        _ => return None,
    };
    let road = [&words[..at], &words[at + 1..]].concat().join(" ");
    let mut out = vec![
        ("house_number".to_string(), words[at].to_lowercase()),
        ("road".to_string(), road.to_lowercase()),
    ];
    let (codes, towns): (Vec<&str>, Vec<&str>) = places.iter().partition(|p| numbered(p));
    match codes[..] {
        [] => {}
        [code] if code.len() == 5 && code.bytes().all(|b| b.is_ascii_digit()) => {
            out.push(("postcode".into(), code.into()));
        }
        _ => return None,
    }
    if towns.iter().any(|t| t.is_empty()) {
        return None;
    }
    if let Some((city, suburbs)) = towns.split_last() {
        out.extend(suburbs.iter().map(|s| ("suburb".into(), s.to_lowercase())));
        out.push(("city".into(), city.to_lowercase()));
    }
    out.push(("country".into(), country.to_lowercase()));
    Some(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn kosovo_addresses_read_as_written() {
        let parsed =
            |q| super::parsed(q).map(|p| p.into_iter().map(|(_, v)| v).collect::<Vec<_>>());
        assert_eq!(
            parsed("12 Qershori 81, Pjetërshticë, Shtime, 72000, Kosovo").unwrap(),
            [
                "81",
                "12 qershori",
                "72000",
                "pjetërshticë",
                "shtime",
                "kosovo"
            ]
        );
        assert_eq!(
            parsed("r-104 Mehmet Haxha, Klina, Kosova").unwrap(),
            ["r-104", "mehmet haxha", "klina", "kosova"]
        );
        for q in [
            "Prizren, 20000, Kosovo",
            "Voskopoja 55, Bob, Albania",
            "983 Gryka e Llapit, 11000addr:street=Gryka e Llapit, Kosovo",
            "Voskopoja 55, , Kosovo",
        ] {
            assert_eq!(parsed(q), None, "{q}");
        }
    }
}
