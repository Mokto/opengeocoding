use super::*;
use crate::{Case, Want, case, town};

pub const PLACES: &[Place] = &[
    place!(
        "qa",
        "country",
        "",
        "QA",
        "",
        3_000_000,
        (51.0, 25.0),
        "",
        "Qatar",
        &[("en", "Qatar")]
    ),
    place!(
        "qa-duneward",
        "region",
        "",
        "QA",
        "QA-DA",
        100_000,
        (51.2, 25.2),
        "qa",
        "Duneward",
        &[("en", "Duneward")]
    ),
    place!(
        "qa-zone",
        "county",
        "",
        "QA",
        "QA-DA",
        10_000,
        (51.21, 25.21),
        "qa-duneward",
        "47",
        &[("en", "47")]
    ),
    place!(
        "qa-sands",
        "county",
        "",
        "QA",
        "QA-DA",
        1_000,
        (51.3, 25.3),
        "qa-duneward",
        "Sands 47",
        &[("en", "Sands 47")]
    ),
];

pub fn cases() -> Vec<Case> {
    vec![
        case("47", Want::Abstain),
        case("Unknown Road 47", Want::Abstain),
        case("Unknown Site, Unknown Hamlet (47)", Want::Abstain),
        case("47, Duneward, France", Want::Abstain),
        case("47, Qatar", town("qa-zone", true)),
        case("47, Duneward, Qatar", town("qa-zone", true)),
        case("47, Duneward", town("qa-zone", true)),
    ]
}
