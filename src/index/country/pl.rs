use crate::index::*;

pub const LOCALITIES_FILE: &str = "pl-localities.fst";

pub fn source_region(dataset: &str) -> Option<&'static str> {
    Some(match dataset {
        "pl/dolnoslaskie" => "PL-02",
        "pl/kujawsko-pomorskie" => "PL-04",
        "pl/lubelskie" => "PL-06",
        "pl/lubuskie" => "PL-08",
        "pl/lodzkie" => "PL-10",
        "pl/malopolskie" => "PL-12",
        "pl/mazowieckie" => "PL-14",
        "pl/opolskie" => "PL-16",
        "pl/podkarpackie" => "PL-18",
        "pl/podlaskie" => "PL-20",
        "pl/pomorskie" => "PL-22",
        "pl/slaskie" => "PL-24",
        "pl/swietokrzyskie" => "PL-26",
        "pl/warminsko-mazurskie" => "PL-28",
        "pl/wielkopolskie" => "PL-30",
        "pl/zachodniopomorskie" => "PL-32",
        _ => return None,
    })
}

pub(in crate::index) fn reverse_locality(
    index: &Index,
    hit: &Hit,
    address: &mut serde_json::Map<String, serde_json::Value>,
    display: &mut Vec<String>,
) {
    let Hit::Street {
        sid,
        place,
        x,
        y,
        number: Some(number),
        ..
    } = hit
    else {
        return;
    };
    if address
        .get("country_code")
        .and_then(serde_json::Value::as_str)
        != Some("pl")
    {
        return;
    }
    let Some(localities) = &index.polish_localities else {
        return;
    };
    let key = schema::house_point_key(*sid, number, *x, *y);
    let Some(locality) = localities.get(key).and_then(|p| u32::try_from(p).ok()) else {
        return;
    };
    let Some(town) = index.places.get(locality as usize) else {
        return;
    };
    let chain = index.chain(locality);
    if town.cc.as_ref() != "pl"
        || town.subtype.as_ref() != "locality"
        || !chain.contains(place)
        || index.chain(*place).contains(&locality)
        || index.region_code(&chain)
            != address
                .get("ISO3166-2-lvl4")
                .and_then(serde_json::Value::as_str)
    {
        return;
    }
    let old: Vec<_> = ["city", "town", "village"]
        .iter()
        .filter_map(|key| address.remove(*key))
        .collect();
    for value in old {
        if let Some(at) = display
            .iter()
            .enumerate()
            .skip(2)
            .find_map(|(at, name)| (value.as_str() == Some(name)).then_some(at))
        {
            display.remove(at);
        }
    }
    display.insert(2.min(display.len()), town.name(true).to_string());
    address.insert(town.key().into(), town.name(true).into());
}

/// A Polish road as the index may name it: without "ul." (ulica), which most sources leave
/// out, and without its given names' initials ("ul. M. Kopernika" is Mikołaja Kopernika's
/// street). Without a stated country, only a road written with "ul." is taken to be Polish.
pub(crate) fn bare_road(road: &str, cc: Option<&str>) -> Option<String> {
    let initial = |w: &str| {
        w.strip_suffix('.')
            .is_some_and(|l| l.chars().count() == 1 && l.chars().all(char::is_alphabetic))
    };
    let mut words: Vec<&str> = road.split_whitespace().collect();
    let ulica = words
        .first()
        .is_some_and(|w| matches!(w.to_lowercase().as_str(), "ul." | "ul" | "ulica"));
    if !cc.map_or(ulica, |c| c.eq_ignore_ascii_case("pl")) {
        return None;
    }
    let before = words.len();
    if ulica {
        words.remove(0);
    }
    words.retain(|w| !initial(w));
    (words.len() < before
        && words
            .iter()
            .any(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3))
    .then(|| words.join(" "))
}
