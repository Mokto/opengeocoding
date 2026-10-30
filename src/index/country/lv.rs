/// "Valmieras nov.", "Valmieras novads": the municipality as the index's regions name it.
fn municipality(part: &str) -> Option<String> {
    let p = part.to_lowercase();
    let stem = p
        .strip_suffix(" nov.")
        .or_else(|| p.strip_suffix(" novads"))?;
    Some(format!("{stem} novads"))
}

/// "Vecates pag.", "Vecates pagasts": the parish written out, as the index names it.
fn parish(part: &str) -> Option<String> {
    let p = part.to_lowercase();
    let stem = p
        .strip_suffix(" pag.")
        .or_else(|| p.strip_suffix(" pagasts"))?;
    Some(format!("{stem} pagasts"))
}

/// "LV-4211" (or "LV4211"), as the index writes Latvian postcodes.
fn postcode(part: &str) -> Option<String> {
    let digits = part
        .strip_prefix("LV")
        .or_else(|| part.strip_prefix("lv"))?
        .trim_start_matches('-');
    (digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| format!("lv-{digits}"))
}

/// "Puķu iela 19", "Zolitūdes iela 71D": the street and a number without leading zero, with at
/// most one letter.
fn house(part: &str) -> Option<(String, String)> {
    let (road, number) = part.rsplit_once(' ')?;
    let digits = number.trim_end_matches(|c: char| c.is_alphabetic());
    (digits.starts_with(|c: char| ('1'..='9').contains(&c))
        && digits.bytes().all(|b| b.is_ascii_digit())
        && number.chars().count() - digits.len() <= 1
        && road.contains(char::is_alphabetic))
    .then(|| (road.to_lowercase(), number.to_lowercase()))
}

/// A house's name, quoted as the register writes it (`"Riņņi"`, `„Riņņi"`) or bare without
/// digits ("Riņņi").
fn name(part: &str) -> Option<String> {
    let quoted = part
        .strip_prefix(['"', '„', '“'])
        .and_then(|p| p.strip_suffix(['"', '”', '“']));
    let name = quoted.unwrap_or(part).trim();
    (!name.is_empty()
        && name.contains(char::is_alphabetic)
        && (quoted.is_some() || !name.contains(|c: char| c.is_ascii_digit())))
    .then(|| name.to_lowercase())
}

/// A Latvian address written part by part as the State Address Register and the post write it
/// ("Uzvaras iela 8, Uzvara, Gailīšu pag., Bauskas nov., LV-3931, Latvia"), or with the types
/// spelt out and without postcode ("…, Gailīšu pagasts, Bauskas novads, Latvia"), which libpostal
/// reads into fragments (city "pagasts", a house "\"riņņi\" vecates pag. valmieras"). Most rural
/// houses have a name, not a street and number (`"Riņņi", Vecates pag., Valmieras nov.`); the
/// index numbers them with the name on the village or parish they are in, as the register's
/// address names it. The house, the village or town (or else the parish) as the town, the
/// postcode, and the municipality as the state. `None` unless every part has its place.
pub(crate) fn aligned(q: &str) -> Option<Vec<(String, String)>> {
    let mut parts: Vec<&str> = q.split(',').map(str::trim).collect();
    let country = parts
        .last()
        .map(|p| p.to_lowercase())
        .filter(|p| matches!(p.as_str(), "latvia" | "latvija"));
    if country.is_some() {
        parts.pop();
    } else if !parts.iter().any(|p| postcode(p).is_some()) {
        return None;
    }
    let [head, ref places @ ..] = parts[..] else {
        return None;
    };
    let (mut settlement, mut civil, mut state, mut code) = (None, None, None, None);
    for place in places {
        if let Some(c) = postcode(place) {
            code.replace(c).is_none().then_some(())?;
        } else if let Some(m) = municipality(place) {
            state.replace(m).is_none().then_some(())?;
        } else if let Some(p) = parish(place) {
            (state.is_none() && civil.replace(p).is_none()).then_some(())?;
        } else if settlement.is_none()
            && civil.is_none()
            && state.is_none()
            && code.is_none()
            && !place.contains(|c: char| c.is_ascii_digit())
        {
            settlement = Some(place.to_lowercase());
        } else {
            return None;
        }
    }
    let town = settlement.or(civil)?;
    let (road, number) = match house(head) {
        Some(house) if !head.starts_with('"') => house,
        _ => (town.clone(), name(head)?),
    };
    let mut parsed = vec![
        ("road".to_string(), road),
        ("house_number".to_string(), number),
        ("city".to_string(), town),
    ];
    parsed.extend(code.map(|c| ("postcode".to_string(), c)));
    parsed.extend(state.map(|s| ("state".to_string(), s)));
    parsed.push((
        "country".to_string(),
        country.unwrap_or_else(|| "latvia".into()),
    ));
    Some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(q: &str) -> Vec<String> {
        aligned(q)
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect()
    }

    #[test]
    fn register_and_spelt_addresses_are_read_part_by_part() {
        assert_eq!(
            read("Uzvaras iela 8, Uzvara, Gailīšu pag., Bauskas nov., LV-3931, Latvia"),
            [
                "road=uzvaras iela",
                "house_number=8",
                "city=uzvara",
                "postcode=lv-3931",
                "state=bauskas novads",
                "country=latvia"
            ]
        );
        assert_eq!(
            read("\"Riņņi\", Vecates pag., Valmieras nov., LV-4211, Latvia"),
            [
                "road=vecates pagasts",
                "house_number=riņņi",
                "city=vecates pagasts",
                "postcode=lv-4211",
                "state=valmieras novads",
                "country=latvia"
            ]
        );
        assert_eq!(
            read("Silvas, Anna, Annas pagasts, Alūksnes novads, Latvia"),
            [
                "road=anna",
                "house_number=silvas",
                "city=anna",
                "state=alūksnes novads",
                "country=latvia"
            ]
        );
        assert_eq!(
            read("\"Silmači 1\", Skrīveru pag., Aizkraukles nov., LV-5125"),
            [
                "road=skrīveru pagasts",
                "house_number=silmači 1",
                "city=skrīveru pagasts",
                "postcode=lv-5125",
                "state=aizkraukles novads",
                "country=latvia"
            ]
        );
        assert_eq!(
            read("Matīsa iela 58A, Rīga, LV-1009, Latvia"),
            [
                "road=matīsa iela",
                "house_number=58a",
                "city=rīga",
                "postcode=lv-1009",
                "country=latvia"
            ]
        );
        for no in [
            "Matīsa iela 58A, Rīga",
            "Rīga, Latvia",
            "Vecates pag., Valmieras nov., Latvia",
            "Matīsa iela 58A, Rīga, 12, Latvia",
            "Riņņi, Valmieras nov., Vecates pag., Latvia",
            "Gedimino pr. 9, Vilnius, Lithuania",
        ] {
            assert!(aligned(no).is_none(), "{no}");
        }
    }
}
