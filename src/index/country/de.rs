/// "Straße 47 7A" is house 7A of the numbered street Straße 47. The street's own number is
/// otherwise read as the house ("47, Thaler Straße").
pub(crate) fn numbered_street(query: &str, parsed: &mut Vec<(String, String)>) {
    let head = query.split(',').next().unwrap_or(query).trim();
    let mut words = head.split_whitespace();
    let (Some(kind), Some(street_no), Some(house), None) =
        (words.next(), words.next(), words.next(), words.next())
    else {
        return;
    };
    if !kind.eq_ignore_ascii_case("straße") && !kind.eq_ignore_ascii_case("strasse") {
        return;
    }
    if !street_no.bytes().all(|b| b.is_ascii_digit()) || !house_token(house) {
        return;
    }
    let road = format!("{} {street_no}", kind.to_lowercase());
    let house = house.to_lowercase();
    let mut saw_road = false;
    let mut saw_house = false;
    for (k, v) in parsed.iter_mut() {
        if k == "road" {
            *v = road.clone();
            saw_road = true;
        } else if k == "house_number" {
            *v = house.clone();
            saw_house = true;
        }
    }
    if !saw_road {
        parsed.insert(0, ("road".into(), road));
    }
    if !saw_house {
        parsed.push(("house_number".into(), house));
    }
}

/// "Hamburg-Barmbek", "Limburg-Staffel": a German town and its district joined by a hyphen, as the
/// town. Elsewhere the part before a hyphen is often another town ("Mbanza" of "Mbanza-Ngungu").
pub fn town_without_district(town: &str) -> Option<String> {
    let (head, tail) = town.split_once('-')?;
    (head.chars().filter(|c| c.is_alphabetic()).count() >= 3
        && head.chars().all(char::is_alphabetic)
        && tail.chars().next().is_some_and(char::is_alphabetic))
    .then(|| head.to_owned())
}

/// "Kehl am Rhein", "Neustadt an der Weinstraße": a German town's official long name, which the
/// index may hold as the town alone ("Kehl").
pub fn town_without_qualifier(name: &str) -> Option<String> {
    let words: Vec<&str> = name.split_whitespace().collect();
    let at = (1..words.len().saturating_sub(1)).find(|&i| {
        matches!(
            words[i].to_lowercase().as_str(),
            "am" | "an" | "im" | "in" | "ob" | "bei" | "vor"
        )
    })?;
    let town = words[..at].join(" ");
    (town.chars().filter(|c| c.is_alphabetic()).count() >= 3).then_some(town)
}

/// A house token after the street's number: digits, or digits and one letter.
fn house_token(house: &str) -> bool {
    let bytes = house.as_bytes();
    let digits = bytes.iter().take_while(|c| c.is_ascii_digit()).count();
    digits > 0
        && bytes.len() - digits <= 1
        && bytes[digits..].iter().all(|c| c.is_ascii_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn towns_with_districts() {
        assert_eq!(
            town_without_district("Hamburg-Barmbek").as_deref(),
            Some("Hamburg")
        );
        assert_eq!(town_without_district("Hamburg"), None);
        assert_eq!(town_without_district("Bad Homburg-Gonzenheim"), None);
    }

    #[test]
    fn long_town_names() {
        assert_eq!(
            town_without_qualifier("Kehl am Rhein").as_deref(),
            Some("Kehl")
        );
        assert_eq!(
            town_without_qualifier("Neustadt an der Weinstraße").as_deref(),
            Some("Neustadt")
        );
        assert_eq!(town_without_qualifier("Kehl"), None);
        assert_eq!(town_without_qualifier("Am Markt"), None);
    }

    fn marked(query: &str, parts: &[(&str, &str)]) -> Vec<String> {
        let mut parsed: Vec<(String, String)> = parts
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        numbered_street(query, &mut parsed);
        parsed.iter().map(|(k, v)| format!("{k}={v}")).collect()
    }

    #[test]
    fn a_numbered_street_keeps_its_number() {
        assert_eq!(
            marked(
                "Straße 47 7A, 13129 Berlin, Germany",
                &[("house_number", "47"), ("road", "thaler straße")]
            ),
            ["house_number=7a", "road=straße 47"]
        );
        assert_eq!(
            marked(
                "Strasse 12 3, Berlin",
                &[("road", "strasse"), ("house_number", "12")]
            ),
            ["road=strasse 12", "house_number=3"]
        );
        assert_eq!(
            marked(
                "Hamburger Straße 29, 12623 Berlin, Germany",
                &[("road", "hamburger straße"), ("house_number", "29")]
            ),
            ["road=hamburger straße", "house_number=29"]
        );
    }
}
