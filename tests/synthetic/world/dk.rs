use super::*;
use crate::{Case, at_house, case};

pub const PLACES: &[Place] = &[
    place!(
        "dk",
        "country",
        "",
        "DK",
        "",
        5_000_000,
        (10.0, 56.0),
        "",
        "Denmark",
        &[("en", "Denmark")]
    ),
    place!(
        "dk-north",
        "region",
        "",
        "DK",
        "DK-81",
        600_000,
        (10.0, 57.0),
        "dk",
        "Region Nordjylland",
        &[("en", "North Denmark Region")]
    ),
    place!(
        "dk-havnholm",
        "locality",
        "town",
        "DK",
        "",
        20_000,
        (10.4, 56.4),
        "dk",
        "Havnholm",
        &[("en", "Havnholm")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Kajgade",
        postcode: "8600",
        city: "Postholm",
        cc: "DK",
        lon: 10.45,
        lat: 56.4,
        numbers: &["5"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Skibsgade",
        postcode: "8500",
        city: "Havnholm",
        cc: "DK",
        lon: 10.4,
        lat: 56.4,
        numbers: &["9"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        // a Danish floor and door beside the house still resolve the building
        case(
            "Skibsgade 9, 8500 Havnholm, Denmark",
            at_house("Skibsgade", "9"),
        ),
        case(
            "Skibsgade 9, st. th, 8500 Havnholm, Denmark",
            at_house("Skibsgade", "9"),
        ),
        case(
            "Skibsgade 9, 1. tv, 8500 Havnholm, Denmark",
            at_house("Skibsgade", "9"),
        ),
        case(
            "Skibsgade 9, 2. 3, 8500 Havnholm, Denmark",
            at_house("Skibsgade", "9"),
        ),
    ]
}
