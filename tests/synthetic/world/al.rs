use super::*;
use crate::{Case, at_house, case};

pub const PLACES: &[Place] = &[
    place!(
        "al",
        "country",
        "",
        "AL",
        "",
        2_800_000,
        (20.0, 41.0),
        "",
        "Shqipëria",
        &[("sq", "Shqipëria"), ("en", "Albania")]
    ),
    place!(
        "al-malore",
        "locality",
        "town",
        "AL",
        "",
        6_000,
        (20.1, 41.0),
        "al",
        "Malore",
        &[("sq", "Malore")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Rruga Ymer Lulaj",
        postcode: "4401",
        city: "Malore",
        cc: "AL",
        lon: 20.35,
        lat: 41.0,
        numbers: &["7"],
    },
    // A lane numbered after the street, inside Malore's area while the street is outside it.
    Street {
        class: "residential",
        length: 0.0,
        name: "Rruga Ymer Lulaj 2",
        postcode: "4401",
        city: "Malore",
        cc: "AL",
        lon: 20.105,
        lat: 41.0,
        numbers: &["7"],
    },
];

pub fn cases() -> Vec<Case> {
    // libpostal reads "Albania" after a postcode as a road, and the postcode as a house number.
    vec![
        case(
            "Rruga Ymer Lulaj 7, 4401, Albania",
            at_house("Rruga Ymer Lulaj", "7"),
        ),
        case(
            "7 Rruga Ymer Lulaj, 4401, Albania",
            at_house("Rruga Ymer Lulaj", "7"),
        ),
        case(
            "7 Rruga Ymer Lulaj, Malore, 4401, Albania",
            at_house("Rruga Ymer Lulaj", "7"),
        ),
    ]
}
