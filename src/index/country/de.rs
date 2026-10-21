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
