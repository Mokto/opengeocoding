use crate::{
    index::{Index, Parsed, parse::field},
    matching::normalize_place,
};

fn administrative_name(text: &str) -> Option<(&'static str, String)> {
    if !matches!(
        text.trim_start().as_bytes().first(),
        Some(b'T' | b't' | b'P' | b'p' | b'Q' | b'q' | b'H' | b'h' | b'X' | b'x')
    ) {
        return None;
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    let first = *words.first()?;
    let (key, count) = if words.len() >= 3 && normalize_place(&words[..2].join(" ")) == "thanh pho"
    {
        ("city", 2)
    } else {
        match normalize_place(first).as_str() {
            "tp" if first.ends_with('.') => ("city", 1),
            "p" if first.ends_with('.') => ("suburb", 1),
            "q" if first.ends_with('.') => ("city_district", 1),
            "phuong" | "xa" => ("suburb", 1),
            "quan" | "huyen" => ("city_district", 1),
            "tinh" => ("state", 1),
            _ => return None,
        }
    };
    (words.len() > count).then(|| (key, words[count..].join(" ")))
}

pub(crate) fn aligned(
    index: &Index,
    query: &str,
    whole: &[(String, String)],
) -> anyhow::Result<Option<Parsed>> {
    let parts: Vec<_> = query.split(',').map(str::trim).collect();
    let administrative: Vec<_> = parts
        .iter()
        .filter_map(|p| administrative_name(p))
        .collect();
    if administrative
        .iter()
        .all(|(key, _)| !matches!(*key, "city" | "state"))
    {
        return Ok(None);
    }
    let countries: Vec<_> = parts
        .iter()
        .filter_map(|p| index.country(p))
        .chain(index.country(field(whole, "country")))
        .collect();
    if countries.iter().any(|cc| cc != "vn") {
        return Ok(None);
    }
    let known = administrative.iter().any(|(key, name)| {
        name.chars().any(char::is_alphabetic)
            && index
                .places_exact(name, Some("vn"), None, &|p| match *key {
                    "city" => {
                        p.subtype.as_ref() == "locality"
                            && (matches!(p.class.as_ref(), "city" | "town")
                                || p.population >= 10_000)
                    }
                    "state" => matches!(p.subtype.as_ref(), "region" | "macroregion"),
                    _ => false,
                })
                .1
    });
    if countries.is_empty() && !known {
        return Ok(None);
    }
    let number = field(whole, "house_number");
    let city = field(whole, "city");
    if !number.is_empty()
        && !city.is_empty()
        && index
            .streets(
                field(whole, "road"),
                Some(number),
                Some(city),
                None,
                Some("vn"),
                true,
            )
            .iter()
            .any(|hit| {
                matches!(
                    hit,
                    crate::index::Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            })
    {
        return Ok(None);
    }
    let rest = parts
        .iter()
        .filter(|p| administrative_name(p).is_none() && index.country(p).is_none())
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    let mut parsed = index.parser.parse(&rest)?.to_vec();
    parsed.retain(|(key, _)| key != "country" && !administrative.iter().any(|(k, _)| *k == key));
    if field(&parsed, "city").is_empty()
        && !administrative.iter().any(|(key, _)| *key == "city")
        && let Some((_, name)) = administrative.iter().find(|(key, _)| *key != "state")
    {
        parsed.push(("city".into(), name.to_lowercase()));
    }
    parsed.extend(
        administrative
            .into_iter()
            .map(|(key, name)| (key.into(), name.to_lowercase())),
    );
    parsed.push(("country".into(), "Vietnam".into()));
    Ok(Some(parsed.into()))
}
