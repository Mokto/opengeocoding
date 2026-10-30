use super::*;
use crate::{Case, Want, case};

/// One parish with two towns that both have an "Avinguda Carlemany 8"; only Engordany's house
/// has the parish's postcode, which Les Escaldes's street also holds for another house.
pub const PLACES: &[Place] = &[
    place!(
        "ad",
        "country",
        "",
        "AD",
        "",
        80_000,
        (100.0, 70.0),
        "",
        "Andorra",
        &[("en", "Andorra")]
    ),
    place!(
        "ad-08",
        "region",
        "",
        "AD",
        "AD-08",
        14_000,
        (100.0, 70.0),
        "ad",
        "Escaldes-Engordany",
        &[("ca", "Escaldes-Engordany")]
    ),
    place!(
        "ad-les-escaldes",
        "locality",
        "town",
        "AD",
        "AD-08",
        10_000,
        (99.7, 70.0),
        "ad-08",
        "les Escaldes",
        &[("ca", "les Escaldes")]
    ),
    place!(
        "ad-engordany",
        "locality",
        "town",
        "AD",
        "AD-08",
        2_000,
        (100.3, 70.0),
        "ad-08",
        "Engordany",
        &[("ca", "Engordany")]
    ),
];

const CARLEMANY: Street = Street {
    class: "primary",
    length: 0.0,
    name: "Avinguda Carlemany",
    postcode: "AD700",
    city: "les Escaldes",
    cc: "AD",
    lon: 99.701,
    lat: 70.0,
    numbers: &["2", "4", "6"],
};

pub const STREETS: &[Street] = &[
    CARLEMANY,
    Street {
        postcode: "",
        lon: 99.702,
        numbers: &["8"],
        ..CARLEMANY
    },
    Street {
        city: "Engordany",
        lon: 100.301,
        numbers: &["8"],
        ..CARLEMANY
    },
];

pub fn cases() -> Vec<Case> {
    // the town's house of that number has no postcode; the other town's has the one written
    vec![case(
        "8 Avinguda Carlemany, Escaldes-Engordany, AD700, Andorra",
        Want::At((100.301, 70.0), 25.0, true),
    )]
}
