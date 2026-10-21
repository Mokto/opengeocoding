use super::*;
use crate::*;

pub const PLACES: &[Place] = &[
    place!(
        "pt",
        "country",
        "",
        "PT",
        "",
        10_000_000,
        (-8.0, 39.5),
        "",
        "Portugal",
        &[("en", "Portugal")]
    ),
    place!(
        "pt-pinhal",
        "locality",
        "town",
        "PT",
        "",
        20_000,
        (-8.0, 39.5),
        "pt",
        "Pinhal",
        &[("en", "Pinhal")]
    ),
];

pub const STREETS: &[Street] = &[
    // Portugal's address register writes "R FARIAS" for the map's "Rua dos Farias"
    Street {
        class: "residential",
        length: 0.002,
        name: "Rua dos Farias",
        postcode: "",
        city: "Pinhal",
        cc: "PT",
        lon: -8.01,
        lat: 39.5,
        numbers: &[],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "R FARIAS",
        postcode: "2955-243",
        city: "Pinhal",
        cc: "PT",
        lon: -8.0095,
        lat: 39.5,
        numbers: &["10", "12"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "AV NORUEGA",
        postcode: "2955-244",
        city: "Pinhal",
        cc: "PT",
        lon: -8.005,
        lat: 39.5,
        numbers: &["6"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "12 Rua dos Farias, 2955-243 Pinhal, Portugal",
            at_house("R FARIAS", "12"),
        ),
        case(
            "Rua dos Farias 12, Pinhal, Portugal",
            at_house("R FARIAS", "12"),
        ),
        case(
            "Rua dos Farias, 12, 2955-243 Pinhal",
            at_house("R FARIAS", "12"),
        ),
        case(
            "6 Avenida da Noruega, Pinhal, Portugal",
            at_house("AV NORUEGA", "6"),
        ),
        case(
            "99 Rua dos Farias, Pinhal, Portugal",
            Want::At((-8.009, 39.5), 300.0, false),
        ),
    ]
}

#[test]
fn portuguese_houses_are_suggested_on_the_registers_abbreviated_street() {
    let want = house(street("R FARIAS"), "12");
    for typed in [
        "12 Rua dos Farias, Pinhal, Portugal",
        "Rua dos Farias 12, 2955-243 Pinhal, Portugal",
        // the forms Portuguese addresses are written in, often without the country
        "Rua dos Farias 12, 2955-243 Pinhal",
        "Rua dos Farias, 12, 2955-243 Pinhal",
        "Rua dos Farias, 12, 2955-243",
        "Rua dos Farias nº 12, 2955-243 Pinhal",
        "Rua dos Farias 12 2955-243 Pinhal",
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(found[0].kind, "address", "{typed}");
        assert!(metres((found[0].x, found[0].y), want) < 5.0, "{typed}");
    }
    // another number is not borrowed, nor the house from another country's same-named road
    for (typed, cc) in [
        ("99 Rua dos Farias, Pinhal, Portugal", None),
        ("12 Rua dos Farias, Pinhal", Some("ES")),
    ] {
        assert!(
            index()
                .suggest(typed, 5, cc, None, false)
                .iter()
                .all(|s| s.kind != "address"),
            "{typed}"
        );
    }
}
