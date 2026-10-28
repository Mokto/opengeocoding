use super::*;
use crate::{Case, at_house, case, named};

/// El Oasis only in Chihuahua, a street Piedras Negras in Coahuila has twice, as the map names
/// it ("Calle Godard", without houses) and as INEGI's 2015 establishment directory does
/// ("GODARD"), and a lettered house.
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

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.003,
        name: "Calle Godard",
        postcode: "26000",
        city: "Piedras Negras",
        cc: "MX",
        lon: -45.0,
        lat: 45.0,
        numbers: &[],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "GODARD",
        postcode: "26000",
        city: "Piedras Negras",
        cc: "MX",
        lon: -45.0005,
        lat: 45.0,
        numbers: &["171", "173"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Calle Matamoros",
        postcode: "26000",
        city: "Piedras Negras",
        cc: "MX",
        lon: -45.0,
        lat: 45.002,
        numbers: &["11", "11A"],
    },
];

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
        // the house is on the road the directory names without its type
        case(
            "CALLE GODARD 173, Piedras Negras, Coahuila, Mexico",
            at_house("GODARD", "173"),
        ),
        // INEGI's standard order: the settlement is not the road, the postcode not the number
        case(
            "CALLE GODARD 173, COLONIA CENTRO, 26000 Piedras Negras, Piedras Negras, Coahuila de Zaragoza, Mexico",
            at_house("GODARD", "173"),
        ),
        case(
            "CALLE MATAMOROS 11-A, Piedras Negras, Coahuila, Mexico",
            at_house("Calle Matamoros", "11A"),
        ),
    ]
}
