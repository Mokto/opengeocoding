use super::*;
use crate::{Case, Want, at_house, case, street};

pub const PLACES: &[Place] = &[
    place!(
        "nl",
        "country",
        "",
        "NL",
        "",
        17_000_000,
        (5.0, 52.0),
        "",
        "Netherlands",
        &[("en", "Netherlands")]
    ),
    place!(
        "nl-polderdam",
        "locality",
        "town",
        "NL",
        "",
        20_000,
        (5.5, 52.3),
        "nl",
        "Polderdam",
        &[("en", "Polderdam")]
    ),
    // 2 km from Weverstraat; Stolica in Poland has a district of this name too
    place!(
        "nl-polderdam-centrum",
        "locality",
        "village",
        "NL",
        "",
        500,
        (5.54, 52.3),
        "nl-polderdam",
        "Centrum",
        &[("nl", "Centrum")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Weverstraat",
        postcode: "1234 NX",
        city: "Polderdam",
        cc: "NL",
        lon: 5.51,
        lat: 52.3,
        numbers: &["2"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Meadowlaan",
        postcode: "1234 TJ",
        city: "Polderdam",
        cc: "NL",
        lon: 5.505,
        lat: 52.3,
        numbers: &["4a"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Willowlaan",
        postcode: "1234 AB",
        city: "Polderdam",
        cc: "NL",
        lon: 5.5,
        lat: 52.3,
        numbers: &["4a"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "Willowlaan 4a, 1234 AB Polderdam, Netherlands",
            at_house("Willowlaan", "4a"),
        ),
        case(
            "Willowln. 4a, 1234 AB Polderdam, Netherlands",
            at_house("Willowlaan", "4a"),
        ),
        case(
            "Meadowlaan 4a, 1234 TJ Polderdam, Netherlands",
            at_house("Meadowlaan", "4a"),
        ),
        case(
            "Meadowln. 4a, 1234 TJ Polderdam, Netherlands",
            at_house("Meadowlaan", "4a"),
        ),
        case(
            "Weverstraat 99, 1234 NX Centrum",
            Want::At(house(street("Weverstraat"), "2"), 300.0, false),
        ),
    ]
}
