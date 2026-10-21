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

#[cfg(test)]
mod tests {
    use super::*;

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
}
