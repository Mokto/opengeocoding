/// A Brazilian CEP is written NNNNN-NNN; some sources keep the dot of its older form (45.160-000).
pub(crate) fn undotted_cep(pc: String) -> String {
    let b = pc.as_bytes();
    let shaped = b.len() == 10
        && b[2] == b'.'
        && b[6] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 2 || i == 6 || c.is_ascii_digit());
    if shaped { pc.replacen('.', "", 1) } else { pc }
}

/// "Avenida Augusto de Lima, 1.715": a house number from 1000 on is written with a thousands
/// dot, which the index does not store. None when no word is such a number.
pub(crate) fn undotted_number(text: &str) -> Option<String> {
    let mut changed = false;
    let words: Vec<String> = text
        .split(' ')
        .map(|word| {
            let (number, tail) = match word.strip_suffix(',') {
                Some(n) => (n, ","),
                None => (word, ""),
            };
            match number.split_once('.') {
                Some((a, b))
                    if (1..=3).contains(&a.len())
                        && b.len() == 3
                        && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit()) =>
                {
                    changed = true;
                    format!("{a}{b}{tail}")
                }
                _ => word.to_owned(),
            }
        })
        .collect();
    changed.then(|| words.join(" "))
}

/// The 27 federative units, as a municipality's "Itupiranga - PA" ends.
const UF: [&str; 27] = [
    "AC", "AL", "AM", "AP", "BA", "CE", "DF", "ES", "GO", "MA", "MG", "MS", "MT", "PA", "PB", "PE",
    "PI", "PR", "RJ", "RN", "RO", "RR", "RS", "SC", "SE", "SP", "TO",
];

/// A CEP as written, NNNNN-NNN or its eight digits.
pub(crate) fn cep(p: &str) -> bool {
    let (b, digits) = (p.as_bytes(), |r: &[u8]| r.iter().all(u8::is_ascii_digit));
    match b.len() {
        8 => digits(b),
        9 => b[5] == b'-' && digits(&b[..5]) && digits(&b[6..]),
        _ => false,
    }
}

/// "Itupiranga - PA" as the houses' postal town names it, "Itupiranga": with a CEP, the state
/// says nothing more.
pub(crate) fn municipality(part: String) -> String {
    match part.rsplit_once(" - ") {
        Some((town, uf)) if UF.contains(&uf.to_uppercase().as_str()) => town.to_owned(),
        _ => part,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thousands_dot_leaves_a_house_number() {
        for (typed, written) in [
            (
                "Avenida Augusto de Lima, 1.715",
                Some("Avenida Augusto de Lima, 1715"),
            ),
            (
                "Rua A, 1.715, Belo Horizonte",
                Some("Rua A, 1715, Belo Horizonte"),
            ),
            ("Rua A, 715", None),
            ("Rua A, 45.160-000", None),
            ("Rua A, 1.7", None),
        ] {
            assert_eq!(undotted_number(typed).as_deref(), written, "{typed}");
        }
    }

    #[test]
    fn ceps_lose_their_old_dot_only() {
        for (stored, shown) in [
            ("45.160-000", "45160-000"),
            ("45160-000", "45160-000"),
            ("1234.5-678", "1234.5-678"),
            ("SW1A 2AA", "SW1A 2AA"),
        ] {
            assert_eq!(undotted_cep(stored.into()), shown);
        }
    }

    #[test]
    fn a_municipality_loses_only_a_states_code() {
        for (typed, kept) in [
            ("Itupiranga - PA", "Itupiranga"),
            ("Itupiranga - pa", "Itupiranga"),
            ("Lincoln - NE", "Lincoln - NE"),
            ("Itupiranga", "Itupiranga"),
        ] {
            assert_eq!(municipality(typed.into()), kept);
        }
        assert!(cep("68580-000") && cep("68580000"));
        assert!(!cep("6858-0000") && !cep("685800") && !cep("68580-0001"));
    }
}
