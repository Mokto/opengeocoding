/// "VIA PRAGA 3, POSSIDENTE, Avigliano (PZ), Italy": a frazione written between the street and
/// its municipality, which libpostal reads into the road ("via praga 3 possidente"), takes for
/// the road instead of the street ("cala gonone" for "VIA TIRSO 2, CALA GONONE"), fuses with the
/// municipality ("felegara medesano"), takes for the town with the municipality left as the
/// province, or drops. The query without it, unless the parse keeps it a place of its own and
/// the municipality the town.
pub(crate) fn without_frazione(query: &str, parsed: &[(String, String)]) -> Option<String> {
    let (trimmed, frazione, town) = frazione_parts(query)?;
    let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let apart = parsed
        .iter()
        .any(|(k, v)| !matches!(k.as_str(), "road" | "city") && words(v) == frazione);
    let kept = parsed.iter().any(|(k, v)| k == "city" && words(v) == town);
    (!(apart && kept)).then_some(trimmed)
}

/// The query without its second part, when it reads as an Italian house, a frazione and its
/// municipality: with the frazione and the municipality's name, folded to lower case.
pub(crate) fn frazione_parts(query: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = query.split(',').map(str::trim).collect();
    let [head, frazione, municipality, ..] = parts[..] else {
        return None;
    };
    let province = |p: &str| {
        p.strip_suffix(')')
            .and_then(|p| p.rsplit_once('('))
            .is_some_and(|(_, code)| {
                code.len() == 2 && code.bytes().all(|b| b.is_ascii_alphabetic())
            })
    };
    let italy = parts
        .last()
        .is_some_and(|c| c.eq_ignore_ascii_case("italy") || c.eq_ignore_ascii_case("italia"));
    if !(italy || province(municipality))
        || parts.len() < 3 + usize::from(italy)
        || !head.chars().any(|c| c.is_ascii_digit())
        || frazione.is_empty()
        || frazione.chars().any(|c| c.is_ascii_digit())
    {
        return None;
    }
    let words = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let town = words(&municipality.to_lowercase());
    let town = town
        .rsplit_once(" (")
        .map_or(town.as_str(), |(town, _)| town);
    // "20121 Milano (MI)" names Milano; a bare postcode ("Irgoli, 08020") leaves the town before it
    let town = town
        .split_once(' ')
        .filter(|(cap, _)| cap.len() == 5 && cap.bytes().all(|b| b.is_ascii_digit()))
        .map_or(town, |(_, town)| town);
    if !town.chars().any(char::is_alphabetic) || town.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut kept = parts.clone();
    kept.remove(1);
    Some((
        kept.join(", "),
        words(&frazione.to_lowercase()),
        town.to_string(),
    ))
}

/// The province a two-letter sigla ("PN") names where the index has no ISO 3166-2 code for it:
/// Aosta's is the region's, and Friuli-Venezia Giulia's former provinces and Sassari have none.
pub(crate) fn uncoded_province(sigla: &str) -> Option<&'static str> {
    Some(match sigla {
        "ao" => "Aosta Valley",
        "go" => "Gorizia",
        "pn" => "Pordenone",
        "ts" => "Trieste",
        "ud" => "Udine",
        "ss" => "Sassari",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(fields: &[(&str, &str)]) -> Vec<(String, String)> {
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn a_frazione_the_parse_runs_across_is_dropped() {
        let road = parsed(&[
            ("road", "via praga 3 possidente"),
            ("city", "avigliano"),
            ("state_district", "pz"),
            ("country", "italy"),
        ]);
        assert_eq!(
            without_frazione("VIA PRAGA 3, POSSIDENTE, Avigliano (PZ), Italy", &road).as_deref(),
            Some("VIA PRAGA 3, Avigliano (PZ), Italy")
        );
        let fused = parsed(&[
            ("road", "via donatori del sangue"),
            ("house_number", "38"),
            ("city", "felegara medesano"),
        ]);
        assert_eq!(
            without_frazione(
                "VIA DONATORI DEL SANGUE 38, FELEGARA, Medesano (PR)",
                &fused
            )
            .as_deref(),
            Some("VIA DONATORI DEL SANGUE 38, Medesano (PR)")
        );
        assert_eq!(
            without_frazione(
                "VIA PRAGA 3, POSSIDENTE, 85021 Avigliano (PZ), Italy",
                &road
            )
            .as_deref(),
            Some("VIA PRAGA 3, 85021 Avigliano (PZ), Italy")
        );
        let as_road = parsed(&[
            ("road", "cala gonone"),
            ("house_number", "2"),
            ("city", "dorgali"),
        ]);
        assert_eq!(
            without_frazione("VIA TIRSO 2, CALA GONONE, Dorgali (NU), Italy", &as_road).as_deref(),
            Some("VIA TIRSO 2, Dorgali (NU), Italy")
        );
        let dropped = parsed(&[
            ("road", "vocabolo greppalto"),
            ("house_number", "127"),
            ("city", "città di castello"),
        ]);
        assert!(
            without_frazione(
                "VOCABOLO GREPPALTO 127, SAN LEO BASTIA, Città di Castello (PG), Italy",
                &dropped
            )
            .is_some()
        );
        let as_town = parsed(&[
            ("road", "via gioiosa"),
            ("house_number", "1/c"),
            ("city", "caramagna ligure"),
            ("state_district", "imperia im"),
        ]);
        assert_eq!(
            without_frazione(
                "VIA GIOIOSA 1/C, CARAMAGNA LIGURE, Imperia (IM), Italy",
                &as_town
            )
            .as_deref(),
            Some("VIA GIOIOSA 1/C, Imperia (IM), Italy")
        );
        let apart = parsed(&[
            ("road", "via galante"),
            ("house_number", "38"),
            ("suburb", "marruci"),
            ("city", "pizzoli"),
        ]);
        assert_eq!(
            without_frazione("VIA GALANTE 38, MARRUCI, Pizzoli (AQ), Italy", &apart),
            None
        );
        // not Italian, no house, a postcode part, or too few parts
        for q in [
            "VIA PRAGA 3, POSSIDENTE, Avigliano, Spain",
            "VIA PRAGA, POSSIDENTE, Avigliano (PZ), Italy",
            "VIA PRAGA 3, 85021 POSSIDENTE, Avigliano (PZ), Italy",
            "VIA PRAGA 3, POSSIDENTE, Italy",
            "Via Nuoro 21, Irgoli, 08020, Italy",
        ] {
            assert_eq!(without_frazione(q, &road), None, "{q}");
        }
    }
}
