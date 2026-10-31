use std::sync::LazyLock;

/// "16 Papaya bij Westpunt, Westpunt, Curacao", "Palm Beach 21, Noord, Aruba", "A20 Kaminda San
/// Sebastian, …": an Aruban or Curaçaoan street and house number, number first or last, then the
/// town or a postcode. libpostal, which has seen few of these, reads the whole part as a venue
/// ("house"), as one road with the number in it, or the street as a suburb, and runs "bij" (near)
/// into a `near` field. The number (digits with an optional letter, "211-T", "70 m", or letters
/// and digits, "BK3") with a shop unit after it ("211-T lok 6"), the rest as the road, a place as
/// the town and a digits-only one as the postcode ("00000", a placeholder, as none). `None` when
/// libpostal already read that number and road, or unless every part has its place.
pub(crate) fn aligned(q: &str, whole: &[(String, String)]) -> Option<Vec<(String, String)>> {
    static FIRST: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"(?i)^(\d+(?:[- ]?[a-z])?|[a-z]{1,2}\d+)(\s+lok\s+\d+[a-z]?)?\s+(\p{L}.*)$",
        )
        .unwrap()
    });
    static LAST: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"(?i)^(\p{L}.*?)\s+(\d+(?:[- ]?[a-z])?|[a-z]{1,2}\d+)(\s+lok\s+\d+[a-z]?)?$",
        )
        .unwrap()
    });
    let mut parts: Vec<&str> = q.split(',').map(str::trim).collect();
    let country = parts.pop()?.to_lowercase();
    if !matches!(country.as_str(), "aruba" | "curacao" | "curaçao") {
        return None;
    }
    let [head, ref places @ ..] = parts[..] else {
        return None;
    };
    let (number, unit, road) = match FIRST.captures(head) {
        Some(c) => (c.get(1), c.get(2), c.get(3)),
        None => LAST
            .captures(head)
            .map(|c| (c.get(2), c.get(3), c.get(1)))?,
    };
    let lower = |m: Option<regex::Match>| {
        m.map_or(String::new(), |m| {
            m.as_str()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
    };
    // OSM numbers a shop "211-T lok 6" as a whole: there is no house 211-T to find
    let number = [lower(number), lower(unit)]
        .join(" ")
        .trim_end()
        .to_string();
    let road = lower(road);
    let field = |key: &str| whole.iter().filter(|(k, _)| k == key).collect::<Vec<_>>();
    // "Landhuis Groot Kwartier Groot Kwartierweg 12": libpostal tells a venue from its street
    let venue = |r: &str| {
        field("house")
            .iter()
            .any(|(_, h)| format!("{h} {r}") == road)
    };
    if matches!(&field("house_number")[..], [(_, n)] if *n == number)
        && matches!(&field("road")[..], [(_, r)] if *r == road || venue(r))
    {
        return None;
    }
    let towns = places
        .iter()
        .filter(|p| !p.bytes().all(|b| b.is_ascii_digit()));
    if places.len() > 2 || towns.count() > 1 || places.iter().any(|p| p.is_empty()) {
        return None;
    }
    let mut parsed = vec![
        ("house_number".to_string(), number),
        ("road".to_string(), road),
    ];
    for place in places {
        if place.bytes().all(|b| b.is_ascii_digit()) {
            if place.bytes().any(|b| b != b'0') {
                parsed.push(("postcode".into(), place.to_string()));
            }
        } else if place.contains(|c: char| c.is_ascii_digit()) {
            return None;
        } else {
            parsed.push(("city".into(), place.to_lowercase()));
        }
    }
    parsed.push(("country".into(), country));
    Some(parsed)
}

/// "211-T lok 6 Caya G.F. Betico Croes", "Caya G.F. Betico Croes 211-T lok 6": where the words
/// of a typed street start a shop's number and unit, which OpenStreetMap keys as one number.
pub(crate) fn shop_at(words: &[&str]) -> Option<usize> {
    let n = words.len();
    if n <= 3 {
        return None;
    }
    [0, n - 3].into_iter().find(|&at| {
        words[at].starts_with(|c: char| c.is_ascii_digit())
            && words[at + 1].eq_ignore_ascii_case("lok")
            && words[at + 2].starts_with(|c: char| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::{aligned, shop_at};

    fn pairs(p: &[(&str, &str)]) -> Vec<(String, String)> {
        p.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn splits_the_number_off_a_misread_street() {
        let venue = pairs(&[("house", "16 papaya bij westpunt"), ("city", "westpunt")]);
        assert_eq!(
            aligned("16 Papaya bij Westpunt, Westpunt, Curacao", &venue),
            Some(pairs(&[
                ("house_number", "16"),
                ("road", "papaya bij westpunt"),
                ("city", "westpunt"),
                ("country", "curacao"),
            ]))
        );
        let road = pairs(&[("road", "kaminda san sebastian a20")]);
        assert_eq!(
            aligned(
                "Kaminda San Sebastian A20, Sint Willibrordus, Curacao",
                &road
            )
            .unwrap()[..2],
            pairs(&[("house_number", "a20"), ("road", "kaminda san sebastian")])[..]
        );
        let unit = pairs(&[("road", "211-t lok"), ("house_number", "6")]);
        assert_eq!(
            aligned("211-T lok 6 Caya G.F. Betico Croes, 00000, Aruba", &unit),
            Some(pairs(&[
                ("house_number", "211-t lok 6"),
                ("road", "caya g.f. betico croes"),
                ("country", "aruba"),
            ]))
        );
    }

    #[test]
    fn keeps_a_good_parse_and_other_countries() {
        let good = pairs(&[("house_number", "24"), ("road", "weg naar westpunt")]);
        assert_eq!(
            aligned("24 Weg naar Westpunt, Westpunt, Curacao", &good),
            None
        );
        let venue = pairs(&[("house", "canashito"), ("house_number", "17")]);
        assert_eq!(
            aligned("Canashito 17, Oranjestad, Netherlands", &venue),
            None
        );
        let spaced = pairs(&[("house", "macuarima"), ("house_number", "70")]);
        assert_eq!(
            aligned("Macuarima 70 m, 00000, Aruba", &spaced).unwrap()[..2],
            pairs(&[("house_number", "70 m"), ("road", "macuarima")])[..]
        );
    }

    #[test]
    fn finds_a_shop_number_at_either_end() {
        let words = |t: &'static str| t.split(' ').collect::<Vec<_>>();
        assert_eq!(shop_at(&words("211-T lok 6 Caya Betico Croes")), Some(0));
        assert_eq!(shop_at(&words("Caya Betico Croes 211-T lok 6")), Some(3));
        assert_eq!(shop_at(&words("Caya Betico Croes 211")), None);
        assert_eq!(shop_at(&words("211 lok 6")), None);
    }
}
