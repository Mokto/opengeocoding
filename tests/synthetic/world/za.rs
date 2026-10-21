use super::*;
use crate::{Case, at_house, case};

pub const PLACES: &[Place] = &[
    place!(
        "za",
        "country",
        "",
        "ZA",
        "",
        50_000_000,
        (25.0, -30.0),
        "",
        "South Africa",
        &[("en", "South Africa")]
    ),
    place!(
        "za-veldhaven",
        "locality",
        "town",
        "ZA",
        "",
        20_000,
        (25.0, -30.0),
        "za",
        "Veldhaven",
        &[("en", "Veldhaven")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Acacia Street",
        postcode: "9991",
        city: "Veldhaven",
        cc: "ZA",
        lon: 25.01,
        lat: -30.0,
        numbers: &["7"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Literal Straat",
        postcode: "9991",
        city: "Veldhaven",
        cc: "ZA",
        lon: 25.02,
        lat: -30.0,
        numbers: &["8"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Literal Street",
        postcode: "9991",
        city: "Veldhaven",
        cc: "ZA",
        lon: 25.03,
        lat: -30.0,
        numbers: &["8"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "7 Acacia Straat, 9991 Veldhaven, South Africa",
            at_house("Acacia Street", "7"),
        ),
        case(
            "8 Literal Straat, 9991 Veldhaven, South Africa",
            at_house("Literal Straat", "8"),
        ),
    ]
}
