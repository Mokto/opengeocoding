use super::*;
use crate::{Case, at_house, case, named};

/// El Oasis only in Chihuahua, and a street of Piedras Negras, in Coahuila, with a lettered house.
pub const PLACES: &[Place] = &[
    place!(
        "mx",
        "country",
        "",
        "MX",
        "",
        126_000_000,
        (-45.0, 45.0),
        "",
        "México",
        &[("en", "Mexico")],
    ),
    place!(
        "mx-coa",
        "region",
        "",
        "MX",
        "MX-COA",
        3_000_000,
        (-45.0, 45.0),
        "mx",
        "Coahuila",
        &[("en", "Coahuila"), ("es", "Coahuila de Zaragoza")],
    ),
    place!(
        "mx-chh",
        "region",
        "",
        "MX",
        "MX-CHH",
        3_700_000,
        (-47.0, 47.0),
        "mx",
        "Chihuahua",
        &[("en", "Chihuahua")],
    ),
    place!(
        "mx-coa-piedras-negras",
        "locality",
        "city",
        "MX",
        "MX-COA",
        160_000,
        (-45.0, 45.0),
        "mx-coa",
        "Piedras Negras",
        &[("en", "Piedras Negras")],
    ),
    place!(
        "mx-chh-el-oasis",
        "locality",
        "village",
        "MX",
        "MX-CHH",
        300,
        (-47.0, 47.0),
        "mx-chh",
        "El Oasis",
        &[("en", "El Oasis")],
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Calle Matamoros",
    postcode: "26000",
    city: "Piedras Negras",
    cc: "MX",
    lon: -45.0,
    lat: 45.002,
    numbers: &["11", "11A"],
}];

pub fn cases() -> Vec<Case> {
    vec![
        // the stated state names another region than the only El Oasis
        named(
            "El Oasis, Coahuila de Zaragoza, Mexico",
            "mx",
            "",
            &["MX-COA"],
            false,
        ),
        // INEGI's standard order: the settlement is not the road, the postcode not the number
        case(
            "CALLE MATAMOROS 11, COLONIA ZONA CENTRO, 26000 Piedras Negras, Piedras Negras, Coahuila, Mexico",
            at_house("Calle Matamoros", "11"),
        ),
        case(
            "CALLE MATAMOROS 11-A, Piedras Negras, Coahuila, Mexico",
            at_house("Calle Matamoros", "11A"),
        ),
    ]
}
