use crate::index::{canon, parse::field};

/// "CALLE REAL 7, Pinedo, Valdegovía, Araba/Álava, Spain": the cadastre's form names the
/// settlement (the address area) before the municipality the house is filed under. libpostal
/// fuses the two into one town ("pinedo valdegovía") or takes the settlement for the road.
/// The street part parsed on its own, the settlement as a suburb, the municipality as the town.
pub(crate) fn area_aligned(
    head: &[(String, String)],
    area: &str,
    town: &str,
    province: &str,
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let (area, town, province) = (
        area.trim().to_lowercase(),
        town.trim().to_lowercase(),
        province.trim().to_lowercase(),
    );
    let numbered = |s: &str| s.chars().any(|c| c.is_ascii_digit());
    if area.is_empty()
        || town.is_empty()
        || province.is_empty()
        || [&area, &town, &province].into_iter().any(|p| numbered(p))
        || field(head, "road").is_empty()
        || field(head, "house_number").is_empty()
        || !head
            .iter()
            .all(|(k, _)| matches!(k.as_str(), "house_number" | "road"))
        || (field(whole, "city") == town && field(whole, "road") == field(head, "road"))
    {
        return None;
    }
    let province_key = whole
        .iter()
        .find(|(k, v)| *v == province && matches!(k.as_str(), "state" | "state_district"))
        .map_or("state_district", |(k, _)| k.as_str());
    let mut aligned = head.to_vec();
    aligned.push(("suburb".into(), area));
    aligned.push(("city".into(), town));
    aligned.push((province_key.into(), province));
    Some(aligned)
}

/// The settlement a Spanish address names besides its municipality, which picks between the
/// municipality's same-named streets and rules out another village's house.
// ponytail: Spain only, where the cadastre names the settlement; elsewhere a stated suburb
// (a London borough) can be larger than the 1.5 km a house may lie from its point.
pub(crate) fn suburb<'a>(cc: Option<&str>, suburb: Option<&'a str>) -> Option<&'a str> {
    suburb.filter(|_| cc == Some("es"))
}

/// The country as libpostal labels it in a Spanish address.
pub(crate) fn spain(country: &str) -> bool {
    matches!(
        country,
        "spain" | "españa" | "espana" | "espanya" | "espainia"
    )
}

pub(crate) fn number_from_avenida(query: &str, parsed: &mut Vec<(String, String)>) {
    if parsed.iter().any(|(k, _)| k == "house_number") {
        return;
    }
    let mut roads = parsed.iter().enumerate().filter(|(_, (k, _))| k == "road");
    let Some((at, (_, first))) = roads.next() else {
        return;
    };
    let Some((number, road)) = first.split_once(' ') else {
        return;
    };
    if roads.next().is_some()
        || number.is_empty()
        || !number.bytes().all(|b| b.is_ascii_digit())
        || !road
            .strip_prefix("avenida ")
            .is_some_and(|name| name.chars().any(char::is_alphabetic))
        || query.split(',').next().unwrap_or("").trim().to_lowercase() != *first
        || crate::matching::country_named(
            parsed
                .iter()
                .rfind(|(k, _)| k == "country")
                .map_or("", |(_, v)| v.as_str()),
        ) != "es"
    {
        return;
    }
    let (number, road) = (number.to_owned(), road.to_owned());
    parsed[at].1 = road;
    parsed.insert(at, ("house_number".into(), number));
}

pub(crate) fn registry_road(road: &str) -> Option<String> {
    const PARTICLES: &[&str] = &[
        "de", "del", "la", "las", "los", "el", "dels", "les", "els", "ses", "da", "do", "das",
        "dos",
    ];
    let road = crate::matching::normalize_place(road);
    let all: Vec<&str> = road.split(' ').collect();
    let mut words: Vec<&str> = all
        .iter()
        .copied()
        .filter(|w| !PARTICLES.contains(w))
        .collect();
    let kind = |w: &str| canon::canon(w).is_some() || canon::is_canonical(w);
    let path = |w: &str| matches!(w, "cami" | "camino" | "cmno" | "cno" | "cm");
    let typed = |w: &str| kind(w) || path(w);
    if words.len() > 1 && typed(words[words.len() - 1]) && !typed(words[0]) {
        words.rotate_right(1);
    }
    if let Some(first) = words.first_mut()
        && path(first)
    {
        *first = "cm";
    }
    (words
        .iter()
        .enumerate()
        .any(|(i, w)| !kind(w) && !(i == 0 && path(w)))
        && words != all)
        .then(|| words.join(" "))
}

pub(crate) fn registry_address(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 2 {
        return None;
    }
    for at in [0, words.len() - 1] {
        let number = words[at];
        if !number.starts_with(|c: char| c.is_ascii_digit())
            || !number.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            continue;
        }
        let road = words[..at]
            .iter()
            .chain(&words[at + 1..])
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        if road.chars().any(|c| c.is_ascii_digit()) {
            return None;
        }
        let road = registry_road(&road)?;
        return Some(if at == 0 {
            format!("{number} {road}")
        } else {
            format!("{road} {number}")
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(parts: &[(&str, &str)]) -> Vec<(String, String)> {
        parts
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_leading_avenida_number_keeps_every_other_component() {
        let mut parsed = pairs(&[
            ("road", "40 avenida de galicia"),
            ("suburb", "centro"),
            ("city", "zamora"),
            ("state", "castilla y león"),
            ("postcode", "49024"),
            ("country", "spain"),
        ]);
        let qualifiers = parsed[1..].to_vec();
        number_from_avenida("40 Avenida de Galicia, Zamora, 49024, Spain", &mut parsed);
        assert_eq!(
            &parsed[..2],
            &pairs(&[("house_number", "40"), ("road", "avenida de galicia")])
        );
        assert_eq!(parsed[2..], qualifiers);
    }

    #[test]
    fn the_avenida_country_check_keeps_aliases_and_last_label_semantics() {
        for countries in [
            &["spain"][..],
            &["españa"],
            &["ES"],
            &["france", "spain"],
            &["spain", "france"],
        ] {
            let mut parsed = pairs(&[("road", "40 avenida de galicia")]);
            parsed.extend(countries.iter().map(|c| ("country".into(), (*c).into())));
            let expected = crate::matching::expected_country(&parsed.iter().cloned().collect());
            number_from_avenida("40 Avenida de Galicia, Spain", &mut parsed);
            assert_eq!(
                parsed.iter().any(|(k, _)| k == "house_number"),
                expected == "es"
            );
        }
    }

    #[test]
    fn only_an_unparsed_spanish_civic_number_is_split_from_avenida() {
        for (query, road, country, extra) in [
            (
                "40 Avenida de Galicia, France",
                "40 avenida de galicia",
                "france",
                None,
            ),
            ("40 Avenida de Galicia", "40 avenida de galicia", "", None),
            (
                "40 Avenida de Galicia, Spain",
                "40 avenida de galicia",
                "spain",
                Some(("house_number", "99")),
            ),
            (
                "40 Avenida de Galicia, Spain",
                "40 avenida de galicia",
                "spain",
                Some(("road", "zamora")),
            ),
            (
                "40 Avenida de Galicia, Spain",
                "40 avenida de galicia zamora",
                "spain",
                None,
            ),
            (
                "Avenida 9 de Octubre, Spain",
                "avenida 9 de octubre",
                "spain",
                None,
            ),
            (
                "40B Avenida de Galicia, Spain",
                "40b avenida de galicia",
                "spain",
                None,
            ),
            (
                "40-42 Avenida de Galicia, Spain",
                "40-42 avenida de galicia",
                "spain",
                None,
            ),
            (
                "40 1/2 Avenida de Galicia, Spain",
                "40 1/2 avenida de galicia",
                "spain",
                None,
            ),
            ("40 Avenida, Spain", "40 avenida", "spain", None),
        ] {
            let mut parsed = pairs(&[("road", road), ("country", country)]);
            parsed.extend(extra.map(|(k, v)| (k.into(), v.into())));
            let before = parsed.clone();
            number_from_avenida(query, &mut parsed);
            assert_eq!(parsed, before, "{query}");
        }
    }

    #[test]
    fn registry_paths_use_the_cadastral_type_without_rewriting_a_name() {
        for road in [
            "Camí de ses Pinedes",
            "Camino de ses Pinedes",
            "Cmno de ses Pinedes",
            "Cno de ses Pinedes",
            "Pinedes camí",
        ] {
            assert_eq!(registry_road(road), Some("cm pinedes".into()), "{road}");
        }
        assert_eq!(registry_road("Calle del Cami"), Some("calle cami".into()));
        assert_eq!(registry_road("Calle del CM"), Some("calle cm".into()));
        assert_eq!(
            registry_road("Camí de Sescorts"),
            Some("cm sescorts".into())
        );
        assert_eq!(
            registry_address("3B Camí de ses Pinedes"),
            Some("3B cm pinedes".into())
        );
        assert_eq!(
            registry_address("Camí de ses Pinedes 3B"),
            Some("cm pinedes 3B".into())
        );
        for road in ["Camí", "Camino", "Camí de ses", "CM de ses"] {
            assert_eq!(registry_road(road), None, "{road}");
        }
    }

    #[test]
    fn registry_addresses_preserve_complete_numbers_and_require_a_road_name() {
        assert_eq!(
            registry_address("2B Calle del Castillo"),
            Some("2B calle castillo".into())
        );
        assert_eq!(
            registry_address("Calle del Castillo 2B"),
            Some("calle castillo 2B".into())
        );
        for text in [
            "2 Calle del",
            "Calle del 2",
            "2/3 Calle del Castillo",
            "Calle del Castillo 2 1/2",
            "Calle 11 de Mayo 2",
            "Castillo",
        ] {
            assert_eq!(registry_address(text), None, "{text}");
        }
    }

    #[test]
    fn the_area_is_a_suburb_of_the_municipality() {
        let head = pairs(&[("road", "calle real"), ("house_number", "7")]);
        let whole = pairs(&[
            ("road", "pinedo"),
            ("house_number", "7"),
            ("city", "valdegovía/gaubea"),
            ("state_district", "araba/álava"),
        ]);
        assert_eq!(
            area_aligned(
                &head,
                " Pinedo",
                " Valdegovía/Gaubea",
                " Araba/Álava",
                &whole
            ),
            Some(pairs(&[
                ("road", "calle real"),
                ("house_number", "7"),
                ("suburb", "pinedo"),
                ("city", "valdegovía/gaubea"),
                ("state_district", "araba/álava"),
            ]))
        );
        let fused = pairs(&[
            ("road", "calle recon"),
            ("house_number", "20"),
            ("city", "antoñana campezo"),
            ("state", "araba/álava"),
        ]);
        assert_eq!(
            area_aligned(
                &pairs(&[("road", "calle recon"), ("house_number", "20")]),
                "Antoñana",
                "Campezo",
                "Araba/Álava",
                &fused
            )
            .map(|a| a[3..].to_vec()),
            Some(pairs(&[("city", "campezo"), ("state", "araba/álava")]))
        );
    }

    #[test]
    fn a_right_parse_or_an_unparsed_street_is_kept() {
        let head = pairs(&[("road", "calle real"), ("house_number", "7")]);
        let right = pairs(&[
            ("road", "calle real"),
            ("house_number", "7"),
            ("suburb", "pinedo"),
            ("city", "valdegovía"),
        ]);
        assert_eq!(
            area_aligned(&head, "Pinedo", "Valdegovía", "Araba/Álava", &right),
            None
        );
        let unnumbered = pairs(&[("road", "calle real")]);
        assert_eq!(
            area_aligned(&unnumbered, "Pinedo", "Valdegovía", "Araba/Álava", &right),
            None
        );
        assert_eq!(
            area_aligned(&head, "Sector 11", "Valdegovía", "Araba/Álava", &[]),
            None
        );
    }
}
