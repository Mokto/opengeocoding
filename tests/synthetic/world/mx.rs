use super::*;
use crate::{Case, at_house, case, named};

/// El Oasis only in Chihuahua, and streets of Piedras Negras, in Coahuila: one the map names
/// "Calle Godard" (without houses) and the 2015 establishment directory "GODARD", a lettered
/// house, postcode 25901 beside a house 901 on Calle 25, and an "Avenida Corregidora" another
/// source calls "Calle Corregidora".
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
    // INEGI's establishment directory names streets without their type
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
    Street {
        class: "residential",
        length: 0.0,
        name: "Calle 25",
        postcode: "26000",
        city: "Piedras Negras",
        cc: "MX",
        lon: -45.001,
        lat: 45.002,
        numbers: &["901"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Avenida Corregidora",
        postcode: "26000",
        city: "Piedras Negras",
        cc: "MX",
        lon: -45.004,
        lat: 45.001,
        numbers: &["222"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Calle Allende",
        postcode: "25901",
        city: "Piedras Negras",
        cc: "MX",
        lon: -45.003,
        lat: 45.003,
        numbers: &["1"],
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
        // INEGI's standard order: the settlement is not the road, the postcode not the number
        case(
            "CALLE MATAMOROS 11, COLONIA ZONA CENTRO, 26000 Piedras Negras, Piedras Negras, Coahuila, Mexico",
            at_house("Calle Matamoros", "11"),
        ),
        // the house is on the road the directory names without its type
        case(
            "CALLE GODARD 173, Piedras Negras, Coahuila, Mexico",
            at_house("GODARD", "173"),
        ),
        // "25 901" is no postcode, though 25901 is one
        case(
            "Calle 25 901, Piedras Negras, Mexico",
            at_house("Calle 25", "901"),
        ),
        case(
            "CALLE MATAMOROS 11-A, Piedras Negras, Coahuila, Mexico",
            at_house("Calle Matamoros", "11A"),
        ),
        // one street under another main type
        case(
            "CALLE CORREGIDORA 222, Piedras Negras, Coahuila, Mexico",
            at_house("Avenida Corregidora", "222"),
        ),
    ]
}
