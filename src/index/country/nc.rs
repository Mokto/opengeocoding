fn territory_postcode(pc: &str) -> bool {
    pc.len() == 5 && pc.starts_with("988") && pc.bytes().all(|b| b.is_ascii_digit())
}

pub fn permits_house_postcode(
    source: [u8; 2],
    place: Option<[u8; 2]>,
    postcode: Option<&str>,
) -> bool {
    (source != *b"NC" && place != Some(*b"NC"))
        || postcode
            .map(str::trim)
            .filter(|pc| !pc.is_empty())
            .is_none_or(territory_postcode)
}

/// "Route de Koé Prolongée 447 bis, Dumbéa": libpostal keeps a civic number with a trailing
/// "bis", "ter" or "quater" in the road, or reads the suffix as a second road. The first part is
/// the road and the whole number, as libpostal reads the number-first order. A number after a
/// numbered-route type ("Route Territoriale 7 bis") is the route's.
pub(crate) fn align(query: &str, parsed: &mut Vec<(String, String)>) {
    let country = crate::matching::expected_country(&parsed.iter().cloned().collect());
    let territory = parsed
        .iter()
        .any(|(k, v)| k == "postcode" && territory_postcode(v));
    if country != "nc" && !(territory && matches!(country.as_str(), "" | "fr")) {
        return;
    }
    let Some((head, tail)) = query.split_once(',') else {
        return;
    };
    let (head, tail) = (head.trim().to_lowercase(), tail.to_lowercase());
    let words: Vec<&str> = head.split_whitespace().collect();
    let [road @ .., number, suffix] = words.as_slice() else {
        return;
    };
    if road.is_empty()
        || !road[0].starts_with(char::is_alphabetic)
        || !number.bytes().all(|b| b.is_ascii_digit())
        || !matches!(*suffix, "bis" | "ter" | "quater")
        || matches!(
            road[road.len() - 1],
            "territoriale" | "provinciale" | "municipale" | "rt" | "rp" | "rm"
        )
    {
        return;
    }
    let number = format!("{number} {suffix}");
    if parsed.iter().any(|(k, v)| {
        k == "house_number" && v.split_whitespace().collect::<String>() == number.replace(' ', "")
    }) {
        return;
    }
    parsed.retain(|(_, value)| !head.contains(value.as_str()) || tail.contains(value.as_str()));
    parsed.insert(0, ("road".into(), road.join(" ")));
    parsed.insert(0, ("house_number".into(), number));
}
