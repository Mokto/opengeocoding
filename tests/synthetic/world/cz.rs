use super::*;
use crate::*;

pub const PLACES: &[Place] = &[
    place!(
        "cz",
        "country",
        "",
        "CZ",
        "",
        10_000_000,
        (15.5, 49.8),
        "",
        "Czechia",
        &[("en", "Czechia")]
    ),
    place!(
        "cz-trest",
        "locality",
        "town",
        "CZ",
        "",
        6_000,
        (15.48, 49.29),
        "cz",
        "Třešť",
        &[("cs", "Třešť")]
    ),
];

pub const STREETS: &[Street] = &[
    // Czech houses keyed by descriptive and orientation number together ("1213/5"), by the
    // descriptive number alone, and an evidential number with its mark
    Street {
        class: "residential",
        length: 0.0,
        name: "Roštýnská",
        postcode: "589 01",
        city: "Třešť",
        cc: "CZ",
        lon: 15.48,
        lat: 49.29,
        numbers: &["1194", "1194/1", "1213", "1213/5", "č.ev. 1057"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "Roštýnská 5, 58901 Třešť, Czechia",
            at_house("Roštýnská", "1213/5"),
        ),
        case(
            "Roštýnská č.p. 1213, 58901 Třešť, Czechia",
            at_house("Roštýnská", "1213"),
        ),
        case(
            "Roštýnská č.ev. 1057, 58901 Třešť, Czechia",
            at_house("Roštýnská", "č.ev. 1057"),
        ),
    ]
}

#[test]
fn czech_houses_are_suggested_by_either_number_or_its_mark() {
    for (typed, number) in [
        ("Roštýnská 5, 58901 Třešť, Czechia", "1213/5"),
        ("Roštýnská č.p. 1213, 58901 Třešť, Czechia", "1213"),
        ("Roštýnská č.ev. 1057, 58901 Třešť, Czechia", "č.ev. 1057"),
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(found[0].kind, "address", "{typed}");
        let want = house(street("Roštýnská"), number);
        assert!(metres((found[0].x, found[0].y), want) < 5.0, "{typed}");
        let retrieved = index().retrieve_suggestion(&found[0].retrieval_id);
        assert!(retrieved.is_some(), "{typed}");
    }
    // an orientation number no house has is no house
    assert!(
        index()
            .suggest("Roštýnská 7, 58901 Třešť, Czechia", 5, None, None, false)
            .iter()
            .all(|s| s.kind != "address")
    );
}
