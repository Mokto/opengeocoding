const TYPES: &[&str] = &[
    "r", "rua", "av", "avenida", "tv", "trav", "travessa", "al", "alameda", "estrada", "estr",
    "rodovia", "rod", "largo", "lg", "praca", "praça", "pc",
];

/// `text` without its road's street type ("Av. Waldemar Falcão, 121" for "Rua Waldemar Falcão"):
/// the types of a street are often mistaken or swapped ("Avenida" for "Rua"), and a lookup on the
/// name finds the street. None when the road has no type or nothing but it.
pub fn without_street_type(text: &str) -> Option<String> {
    let (road, rest) = text.split_once(',').unwrap_or((text, ""));
    let words: Vec<&str> = road.split_whitespace().collect();
    let at = words
        .iter()
        .position(|w| !w.starts_with(|c: char| c.is_ascii_digit()))?;
    let kind = words[at].trim_end_matches('.').to_lowercase();
    if !TYPES.contains(&kind.as_str()) || words.len() <= at + 1 {
        return None;
    }
    let mut kept = words;
    kept.remove(at);
    let rest = if rest.is_empty() {
        String::new()
    } else {
        format!(",{rest}")
    };
    Some(format!("{}{rest}", kept.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_street_type_leaves_the_road() {
        for (typed, name) in [
            ("Av. Waldemar Falcão, 121", Some("Waldemar Falcão, 121")),
            ("Avenida Waldemar Falcão 121", Some("Waldemar Falcão 121")),
            (
                "121 Rua Waldemar Falcão, Salvador",
                Some("121 Waldemar Falcão, Salvador"),
            ),
            ("Waldemar Falcão, 121", None),
            ("Rua, 121", None),
            ("Rua", None),
        ] {
            assert_eq!(without_street_type(typed).as_deref(), name, "{typed}");
        }
    }
}
