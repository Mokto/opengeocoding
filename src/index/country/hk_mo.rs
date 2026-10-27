use regex::Regex;
use std::sync::LazyLock;

/// `query` with "S.A.R." (or "SAR", "Special Administrative Region") dropped after Hong Kong or
/// Macau, which names no place in the data: "Kowloon, Hong Kong S.A.R." is "Kowloon, Hong Kong".
/// None when no part has it.
pub fn without_sar(query: &str) -> Option<String> {
    static SAR: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"(?i)\b(hong\s*kong|macau|macao)(\s*,)?\s+(s\.?\s?a\.?\s?r\.?|special\s+administrative(\s+region)?)(\s+(of\s+)?(the\s+)?(people's\s+republic\s+of\s+)?china)?\s*(,|$)",
        )
        .unwrap()
    });
    SAR.is_match(query).then(|| {
        SAR.replace_all(query, "$1$9")
            .trim_end_matches([' ', ','])
            .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::without_sar;

    #[test]
    fn drops_the_title_of_a_special_administrative_region() {
        for (query, expected) in [
            ("hong kong s.a.r.", Some("hong kong")),
            ("Kowloon, Hong Kong S.A.R.", Some("Kowloon, Hong Kong")),
            (
                "hong kong, central and western, hong kong s.a.r.",
                Some("hong kong, central and western, hong kong"),
            ),
            ("macau s.a.r.", Some("macau")),
            ("Macao SAR, China", Some("Macao, China")),
            ("hong kong special administrative ", Some("hong kong")),
            ("Hong Kong SAR, Wan Chai", Some("Hong Kong, Wan Chai")),
            ("hong kong", None),
            ("sar, hong kong", None),
            ("Sarajevo", None),
        ] {
            assert_eq!(without_sar(query).as_deref(), expected, "{query}");
        }
    }
}
