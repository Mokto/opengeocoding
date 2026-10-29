/// Estonian street types as addresses abbreviate them, and as the map names them.
const TYPES: &[(&str, &str)] = &[("tn", "tänav"), ("mnt", "maantee"), ("pst", "puiestee")];

/// `text` with its abbreviated Estonian street types written out: "Vinkli tn 8, Tartu" is on
/// Vinkli tänav, "Tartu mnt" is Tartu maantee. The type follows the name, so the first word of a
/// part, or one after a number, is part of a name.
pub(crate) fn full_road(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split(' ').collect();
    let mut changed = false;
    let full: Vec<String> = words
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let bare = w.trim_end_matches([',', '.']);
            let named = i > 0
                && words[i - 1].chars().any(char::is_alphabetic)
                && !words[i - 1].ends_with(',');
            match TYPES
                .iter()
                .find(|(short, _)| bare.eq_ignore_ascii_case(short))
            {
                Some((_, long)) if named => {
                    changed = true;
                    format!("{long}{}", w[bare.len()..].trim_start_matches('.'))
                }
                _ => w.to_string(),
            }
        })
        .collect();
    changed.then(|| full.join(" "))
}

/// The road libpostal read from [`full_road`]'s text, with the street type as `typed`: a street
/// may be named with the short type, so the written-out one is only tried after it.
pub(crate) fn typed_road(parsed: &mut [(String, String)], typed: &str) {
    let typed = typed.to_lowercase();
    let typed: Vec<&str> = typed.split(|c: char| !c.is_alphanumeric()).collect();
    for (_, road) in parsed.iter_mut().filter(|(k, _)| k == "road") {
        *road = road
            .split(' ')
            .map(|w| {
                TYPES
                    .iter()
                    .find(|(short, long)| {
                        w == *long && typed.contains(short) && !typed.contains(long)
                    })
                    .map_or(w, |(short, _)| short)
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
}

/// "Harju maakond, Rae vald, Rae küla, Pae tee 3, Estonia": the register's full address (ADS
/// täisaadress) runs from the county to the house, which libpostal reads as one road. The parts
/// house first, the country kept last.
pub(crate) fn county_first(q: &str) -> Option<String> {
    let (first, _) = q.split_once(',')?;
    if !first.trim().to_lowercase().ends_with(" maakond") {
        return None;
    }
    let mut parts: Vec<&str> = q.split(',').map(str::trim).collect();
    let country = parts
        .last()
        .filter(|p| p.eq_ignore_ascii_case("estonia") || p.eq_ignore_ascii_case("eesti"))
        .copied();
    if country.is_some() {
        parts.pop();
    } else if parts
        .last()
        .is_some_and(|p| !crate::matching::country_named(p).is_empty())
    {
        return None;
    }
    (parts.len() >= 3).then(|| {
        parts.reverse();
        parts.extend(country);
        parts.join(", ")
    })
}

/// "Antsu, Ruhnu küla, Ruhnu vald, 93001 Saare maakond": a farm or other property named, not
/// numbered, in its village, which libpostal reads as a house with the village. The data files
/// such an address as the name numbered on a street of the village's name. The name and village.
pub(crate) fn named_house(q: &str) -> Option<(&str, &str)> {
    let mut parts = q.split(',').map(str::trim);
    let (name, village) = (parts.next()?, parts.next()?);
    let settlement = village
        .split(" / ")
        .next()
        .unwrap_or(village)
        .rsplit(' ')
        .next()
        .is_some_and(|w| w.ends_with("küla") || matches!(w, "alevik" | "alev"));
    (settlement
        && !name.is_empty()
        && !name.contains(|c: char| c.is_ascii_digit())
        && name.split(' ').count() <= 3)
        .then_some((name, village))
}

/// An Estonian address written part by part as Omniva and the register write it ("Pikk tn 55,
/// Kärdla linn, Hiiumaa vald, 92411 Hiiu maakond, Estonia"), which libpostal reads into
/// fragments ("city": "vald", "state_district": "linn"): the house part as libpostal reads it
/// alone (or [`named_house`]'s name numbered on its village), the first settlement as the town,
/// a further one (the municipality) as a district, a Tallinn district as the suburb, and the
/// county as the state. `None` unless every part has its place type, so other addresses keep
/// libpostal's reading.
pub(crate) fn aligned(
    q: &str,
    whole: &[(String, String)],
    libpostal: impl Fn(&str) -> anyhow::Result<Vec<(String, String)>>,
) -> anyhow::Result<Option<Vec<(String, String)>>> {
    let stated = crate::matching::expected_country(&whole.iter().cloned().collect());
    if !matches!(stated.as_str(), "ee" | "") {
        return Ok(None);
    }
    let mut parts: Vec<&str> = q.split(',').map(str::trim).collect();
    let country = parts
        .last()
        .filter(|p| p.eq_ignore_ascii_case("estonia") || p.eq_ignore_ascii_case("eesti"))
        .map(|p| p.to_lowercase());
    if country.is_some() {
        parts.pop();
    }
    let [head, ref places @ ..] = parts[..] else {
        return Ok(None);
    };
    // "Rannaküla" is a village too, and "Hara küla / Harga" one with its Swedish name
    let typed = |p: &str| {
        let p = p.split(" / ").next().unwrap_or(p);
        p.ends_with("küla")
            || p.rsplit_once(' ').is_some_and(|(_, w)| {
                matches!(
                    w,
                    "alevik" | "alev" | "linn" | "vald" | "linnaosa" | "maakond"
                )
            })
    };
    let coded = |p: &str| {
        p.split_once(' ')
            .is_some_and(|(code, _)| code.len() == 5 && code.bytes().all(|b| b.is_ascii_digit()))
    };
    // "Karjamaa tn 1a, 80020 Pärnu, Estonia", which libpostal reads as house "1a 80020"
    let postal_town = country.is_some() && places.len() == 1 && coded(places[0]);
    if !postal_town && !places.iter().any(|p| typed(p) && !p.ends_with(" maakond")) {
        return Ok(None);
    }
    let mut parsed = match named_house(q) {
        Some((name, village)) => vec![
            ("road".to_string(), village.to_lowercase()),
            ("house_number".to_string(), name.to_lowercase()),
        ],
        None => {
            let read = libpostal(head)?;
            let house = read.iter().any(|(k, _)| k == "road")
                && read
                    .iter()
                    .all(|(k, _)| matches!(k.as_str(), "road" | "house_number" | "unit"));
            if !house {
                return Ok(None);
            }
            read
        }
    };
    let mut town = false;
    for (i, place) in places.iter().enumerate() {
        let (postcode, name) = match place.split_once(' ') {
            Some((code, name)) if coded(place) => (Some(code), name),
            _ => (None, *place),
        };
        let last = i + 1 == places.len();
        let key = if name.ends_with(" maakond") {
            if !last {
                return Ok(None);
            }
            "state"
        } else if name.ends_with(" linnaosa") {
            "suburb"
        } else if !typed(name) && !(last && postcode.is_some()) {
            return Ok(None);
        } else if !town {
            town = true;
            "city"
        } else {
            "state_district"
        };
        parsed.extend(postcode.map(|c| ("postcode".to_string(), c.to_string())));
        parsed.push((key.to_string(), name.to_lowercase()));
    }
    parsed.extend(country.map(|c| ("country".to_string(), c)));
    Ok(Some(parsed))
}

/// A street named after a person with the initials the register writes ("A. Lauteri tn", "F. R.
/// Kreutzwaldi tn"), which the map spells out ("Ants Lauteri tänav"): the name without them, for
/// the street lookup to complete, and the number of initials.
fn initials(road: &str) -> Option<(&str, usize)> {
    let (mut rest, mut n) = (road.trim(), 0);
    while let Some((initial, after)) = rest.split_once(' ')
        && initial.ends_with('.')
        && initial.chars().count() == 2
        && initial.starts_with(char::is_alphabetic)
        && after.contains(' ')
    {
        (rest, n) = (after.trim_start(), n + 1);
    }
    (n > 0).then_some((rest, n))
}

/// [`initials`]'s road without them, for the lookup.
pub(crate) fn without_initials(road: &str) -> Option<String> {
    initials(road).map(|(rest, _)| rest.to_string())
}

/// "A. Lauteri tn" is "Ants Lauteri tänav": after the initials, the words are the found road's
/// last ones (compared as `key` writes them), and each initial begins one of the words before.
pub(crate) fn initials_agree(typed: &str, found: &str, key: impl Fn(&str) -> String) -> bool {
    let Some((rest, n)) = initials(typed) else {
        return false;
    };
    // "R. du Bosquet" is a Portuguese Rua: only an Estonian street type marks the initials
    let estonian = rest.rsplit(' ').next().is_some_and(|w| {
        let w = w.to_lowercase();
        TYPES.iter().any(|(short, long)| w == *short || w == *long)
            || matches!(w.as_str(), "tee" | "põik" | "allee" | "plats" | "väljak")
    });
    if !estonian {
        return false;
    }
    let rest = full_road(rest).unwrap_or_else(|| rest.to_string());
    let (rest, found_key) = (
        key(&rest),
        key(&full_road(found).unwrap_or_else(|| found.into())),
    );
    let found_words: Vec<&str> = found_key.split(' ').collect();
    let rest_words = rest.split(' ').count();
    if found_words.len() != rest_words + n || found_words[n..].join(" ") != rest {
        return false;
    }
    let typed_initials = key(&typed.split(' ').take(n).collect::<Vec<_>>().join(" "));
    typed_initials
        .split(' ')
        .zip(&found_words[..n])
        .all(|(i, w)| i.chars().count() == 1 && w.starts_with(i))
}

/// The municipality an Estonian address states ([`aligned`]'s "rae vald" beside its "harju
/// maakond") is not the answer's: "Oru tn 10, Vao küla, Väike-Maarja vald" is not Vao küla in
/// Järva vald, whose county the index renders (in English, "Saku Parish" for Saku vald).
pub(crate) fn municipality_conflict(
    address: &crate::matching::ParsedAddress,
    details: Option<&crate::proto::opengeocoding::AddressDetails>,
) -> bool {
    use crate::matching::{names_agree, part};
    let stated = part(address, "state_district");
    let typed = stated.ends_with(" vald") || stated.ends_with(" linn");
    typed
        && part(address, "state").ends_with(" maakond")
        && details.is_some_and(|d| {
            d.country_code == "ee"
                && !d.county.is_empty()
                && ![&d.county, &d.city, &d.town, &d.municipality]
                    .iter()
                    .map(|name| name.strip_suffix(" Parish").unwrap_or(name))
                    .any(|name| !name.is_empty() && names_agree(stated, name))
        })
}

/// "21055 Narva" without its postcode: the index knows only some of Omniva's postcodes, and one
/// it lacks is no place name to complete.
pub(crate) fn without_postcode(part: String) -> String {
    match part.split_once(' ') {
        Some((code, town)) if code.len() == 5 && code.bytes().all(|b| b.is_ascii_digit()) => {
            town.trim().to_string()
        }
        _ => part,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_name_the_spelt_out_street() {
        use crate::matching::roads_agree;
        assert_eq!(
            without_initials("F. R. Kreutzwaldi tn").as_deref(),
            Some("Kreutzwaldi tn")
        );
        assert!(roads_agree("l. tolstoi tn", "Lev Tolstoi tänav"));
        assert!(roads_agree(
            "F. R. Kreutzwaldi tn",
            "Friedrich Reinhold Kreutzwaldi tänav"
        ));
        assert!(roads_agree("a. lauteri tn", "Ants Lauteri tänav"));
        for (typed, found) in [
            ("k. tolstoi tn", "Lev Tolstoi tänav"),
            ("l. tolstoi tn", "Tolstoi tänav"),
            ("f. kreutzwaldi tn", "Friedrich Reinhold Kreutzwaldi tänav"),
            ("l. tolstoi tn", "Lev Tolstoi põik"),
        ] {
            assert!(!roads_agree(typed, found), "{typed}");
        }
        assert!(!roads_agree("R. du Bosquet", "Rue du Bosquet"));
        assert_eq!(without_initials("Pärnu mnt"), None);
        assert_eq!(without_initials("A. tn"), None);
    }

    #[test]
    fn reads_county_first_and_named_addresses() {
        assert_eq!(
            county_first("Rapla maakond, Rapla vald, Oela küla, Suislepa tee 9, Estonia")
                .as_deref(),
            Some("Suislepa tee 9, Oela küla, Rapla vald, Rapla maakond, Estonia")
        );
        assert_eq!(
            county_first("Harju maakond, Tallinn, Pärnu mnt 15").as_deref(),
            Some("Pärnu mnt 15, Tallinn, Harju maakond")
        );
        for kept in [
            "Pärnu mnt 15, 10141 Tallinn, Harju maakond, Estonia",
            "Harju maakond, Estonia",
            "Harju maakond, Tallinn, Pärnu mnt 15, Latvia",
        ] {
            assert_eq!(county_first(kept), None, "{kept}");
        }
        assert_eq!(
            named_house("Antsu, Ruhnu küla, Ruhnu vald, 93001 Saare maakond, Estonia"),
            Some(("Antsu", "Ruhnu küla"))
        );
        assert_eq!(
            named_house("Põlluääre, Vatsküla, Saaremaa vald"),
            Some(("Põlluääre", "Vatsküla"))
        );
        for kept in [
            "Pae tee 3, Rae küla, Rae vald",
            "Antsu, Ruhnu vald",
            "Ruhnu küla",
        ] {
            assert_eq!(named_house(kept), None, "{kept}");
        }
    }

    #[test]
    fn expands_the_street_type_after_a_name() {
        assert_eq!(full_road("vinkli tn").as_deref(), Some("vinkli tänav"));
        assert_eq!(full_road("Vinkli tn. 8").as_deref(), Some("Vinkli tänav 8"));
        assert_eq!(
            full_road("12 Ülikooli Tn, Tartu, Estonia").as_deref(),
            Some("12 Ülikooli tänav, Tartu, Estonia")
        );
        assert_eq!(
            full_road("Tartu mnt 25, Tallinn").as_deref(),
            Some("Tartu maantee 25, Tallinn")
        );
        assert_eq!(full_road("Rävala pst").as_deref(), Some("Rävala puiestee"));
        for kept in ["Tn 4", "4 tn", "Vinkli tänav", "Vinkli 8, Tn", "Kotnik 8"] {
            assert_eq!(full_road(kept), None, "{kept}");
        }
    }

    #[test]
    fn keeps_the_typed_street_type() {
        let mut parsed = vec![
            ("road".to_string(), "vinkli tänav".to_string()),
            ("city".to_string(), "tartu".to_string()),
        ];
        typed_road(&mut parsed, "Vinkli tn. 8, Tartu");
        assert_eq!(parsed[0].1, "vinkli tn");
        let mut parsed = vec![("road".to_string(), "vinkli tänav".to_string())];
        typed_road(&mut parsed, "Vinkli tänav 8, Tartu");
        assert_eq!(parsed[0].1, "vinkli tänav");
    }
}
