use crate::index::{Hit, Index, NONE, name_key};

fn numbered_head(query: &str) -> Option<(String, String)> {
    let head = query.split(',').next()?.trim();
    let (number, rest) = head.split_once(char::is_whitespace)?;
    let digits = number.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0
        || number.len() - digits > 1
        || !number[digits..].bytes().all(|b| b.is_ascii_alphabetic())
    {
        return None;
    }
    let rest = rest.trim();
    let (number, road) = match rest.split_once(char::is_whitespace) {
        Some((suffix, road))
            if matches!(
                suffix.to_ascii_lowercase().as_str(),
                "bis" | "ter" | "a" | "b" | "c" | "d" | "e"
            ) =>
        {
            (format!("{number} {suffix}"), road.trim())
        }
        _ => (number.to_string(), rest),
    };
    road.starts_with(char::is_alphabetic)
        .then(|| (number.to_lowercase(), road.to_lowercase()))
}

pub(crate) fn align(query: &str, parsed: &mut Vec<(String, String)>) {
    if crate::matching::expected_country(&parsed.iter().cloned().collect()) != "pf"
        || parsed.iter().any(|(key, _)| {
            matches!(
                key.as_str(),
                "unit" | "level" | "entrance" | "staircase" | "po_box"
            )
        })
    {
        return;
    }
    let Some((head, tail)) = query.split_once(',') else {
        return;
    };
    if parsed
        .iter()
        .any(|(key, value)| key == "postcode" && head.trim_start().starts_with(value))
    {
        return;
    }
    let Some((number, road)) = numbered_head(head) else {
        return;
    };
    let (head, tail) = (head.to_lowercase(), tail.to_lowercase());
    parsed.retain(|(key, value)| {
        !matches!(key.as_str(), "house" | "house_number" | "road" | "unit")
            && (!head.contains(value) || tail.contains(value))
    });
    parsed.insert(0, ("road".into(), road));
    parsed.insert(0, ("house_number".into(), number));
}

pub(crate) fn preserve_house_postcode(cc: Option<&str>) -> bool {
    cc == Some("pf")
}

pub(crate) fn commune_named(index: &Index, hit: &Hit, name: &str) -> bool {
    let Hit::Street {
        place,
        number: Some(_),
        ..
    } = hit
    else {
        return false;
    };
    *place != NONE
        && index.places[*place as usize].cc.as_ref() == "pf"
        && index.chain(*place).into_iter().any(|id| {
            let place = &index.places[id as usize];
            matches!(place.subtype.as_ref(), "county" | "localadmin") && place.named(name_key(name))
        })
}
