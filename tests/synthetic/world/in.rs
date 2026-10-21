use super::*;
use crate::{Case, case, town};

pub const PLACES: &[Place] = &[
    place!(
        "in",
        "country",
        "",
        "IN",
        "",
        1_400_000_000,
        (78.0, 22.0),
        "",
        "India",
        &[("en", "India")],
    ),
    place!(
        "in-dl",
        "region",
        "",
        "IN",
        "IN-DL",
        17_000_000,
        (77.1, 28.65),
        "in",
        "Delhi",
        &[("en", "Delhi")],
    ),
    place!(
        "in-dl-delhi",
        "locality",
        "city",
        "IN",
        "IN-DL",
        11_000_000,
        (77.209, 28.6139),
        "in-dl",
        "Delhi",
        &[("en", "Delhi")],
    ),
    place!(
        "in-rj",
        "region",
        "",
        "IN",
        "IN-RJ",
        68_000_000,
        (73.0, 26.3),
        "in",
        "Rajasthan",
        &[("en", "Rajasthan")],
    ),
    place!(
        "in-rj-jodhpur",
        "locality",
        "city",
        "IN",
        "IN-RJ",
        1_100_000,
        (73.02, 26.29),
        "in-rj",
        "Jodhpur",
        &[("en", "Jodhpur")],
    ),
];

pub const STREETS: &[Street] = &[
    // libpostal reads "Shastri Nagar D-112" as one road: the house is found without its parse.
    Street {
        class: "residential",
        length: 0.0,
        name: "Shastri Nagar",
        postcode: "342003",
        city: "Jodhpur",
        cc: "IN",
        lon: 73.02,
        lat: 26.29,
        numbers: &["D-112"],
    },
    // A number of several words that libpostal splits, and a street named like its town.
    Street {
        class: "residential",
        length: 0.0,
        name: "Shiv Vihar",
        postcode: "342011",
        city: "Jodhpur",
        cc: "IN",
        lon: 73.03,
        lat: 26.29,
        numbers: &["Plot No.14"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Jodhpur",
        postcode: "342003",
        city: "Jodhpur",
        cc: "IN",
        lon: 73.021,
        lat: 26.29,
        numbers: &["Plot No.15"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case("Delhi", town("in-dl-delhi", true)),
        case("Delhi, India", town("in-dl-delhi", true)),
    ]
}
