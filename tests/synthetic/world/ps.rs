use super::*;
use crate::*;

/// Overture files the West Bank as a country of its own (XW), which answers name as Palestine.
pub const PLACES: &[Place] = &[
    place!(
        "xw",
        "country",
        "",
        "XW",
        "",
        3_000_000,
        (35.3, 32.3),
        "",
        "West Bank",
        &[("en", "West Bank")]
    ),
    place!(
        "xw-ein-zahra",
        "locality",
        "town",
        "XW",
        "",
        9_000,
        (35.3, 32.3),
        "xw",
        "Ein Zahra",
        &[("en", "Ein Zahra")]
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Bayader Street",
    postcode: "",
    city: "Ein Zahra",
    cc: "XW",
    lon: 35.301,
    lat: 32.3,
    numbers: &["4"],
}];

/// GeoNames' name, which libpostal doesn't know as a country.
pub fn cases() -> Vec<Case> {
    vec![case(
        "4 Bayader Street, Ein Zahra, Palestinian Territory",
        at_house("Bayader Street", "4"),
    )]
}

#[test]
fn palestinian_houses_are_suggested_under_palestine() {
    for typed in [
        "Bayader Street 4, Ein Zahra",
        "Bayader Street 4, Ein Zahra, Palestine",
        "4 Bayader Street, Ein Zahra, Palestinian Territory",
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        let first = found.first().unwrap_or_else(|| panic!("{typed}: nothing"));
        assert_eq!(
            (first.kind, first.country_code.as_str()),
            ("address", "ps"),
            "{typed}"
        );
        assert!(
            metres((first.x, first.y), house(street("Bayader Street"), "4")) < 5.0,
            "{typed}"
        );
    }
    let town = index().suggest("Ein Zahra, Palestine", 5, None, None, false);
    assert_eq!(
        town.first().map(|s| (s.kind, s.country_code.as_str())),
        Some(("town", "ps"))
    );
}
