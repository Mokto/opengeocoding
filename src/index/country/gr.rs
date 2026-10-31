/// Greece's municipalities (Acharnes, Marousi) are counties in the data, with no town of their
/// own: one is the town an address states.
pub fn municipality_is_town(country_code: &str) -> bool {
    country_code.eq_ignore_ascii_case("gr")
}
