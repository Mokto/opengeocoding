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

#[cfg(test)]
mod tests {
    use super::*;

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
