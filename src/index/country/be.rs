/// "Linkebeekstraat 34 bus 1", "Rue Agimont 23 boîte rez": a Belgian flat's box after the house
/// number, which the houses are not keyed with. The road part's words without it; not a road
/// after its number ("44126 Box Place").
pub(crate) fn without_box(words: &mut Vec<String>) {
    if let [_, .., number, marker, unit] = words.as_slice()
        && number.chars().any(|c| c.is_ascii_digit())
        && matches!(
            marker.to_lowercase().as_str(),
            "bus" | "boîte" | "boite" | "box"
        )
        && (unit.chars().any(|c| c.is_ascii_digit()) || unit.chars().count() <= 3)
    {
        words.truncate(words.len() - 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_box_after_a_number_goes() {
        for (typed, kept) in [
            ("Linkebeekstraat 34 bus 1", "Linkebeekstraat 34"),
            ("Rue des Puits l'Eau 22 boîte 31", "Rue des Puits l'Eau 22"),
            ("Tilmontstraat 6 bus 0M/H", "Tilmontstraat 6"),
            ("Rue Agimont 23 boîte rez", "Rue Agimont 23"),
            ("Busstraat 4 bus", "Busstraat 4 bus"),
            ("Rue du Bus 12", "Rue du Bus 12"),
            ("44126 Box Place", "44126 Box Place"),
            ("Main Road 12 Box Hill", "Main Road 12 Box Hill"),
        ] {
            let mut words = typed.split(' ').map(String::from).collect();
            without_box(&mut words);
            assert_eq!(words.join(" "), kept);
        }
    }
}
