use super::*;
use crate::{Case, Want, at_house, case, street};

pub const PLACES: &[Place] = &[
    place!(
        "pl",
        "country",
        "",
        "PL",
        "",
        5_000_000,
        (19.0, 52.0),
        "",
        "Poland",
        &[("en", "Poland")]
    ),
    place!(
        "pl-damnica",
        "locality",
        "village",
        "PL",
        "",
        2_000,
        (17.3, 54.5),
        "pl",
        "Damnica",
        &[("pl", "Damnica")]
    ),
    // a city whose streets the data files under its districts, themselves localities
    place!(
        "pl-stolica",
        "locality",
        "city",
        "PL",
        "",
        1_800_000,
        (18.0, 53.0),
        "pl",
        "Stolica",
        &[("pl", "Stolica")]
    ),
    place!(
        "pl-centrum",
        "locality",
        "",
        "PL",
        "",
        100_000,
        (18.01, 53.0),
        "pl-stolica",
        "Centrum",
        &[("pl", "Centrum")]
    ),
];

pub const STREETS: &[Street] = &[
    // a village whose houses are numbered on its name: libpostal reads "Świtały 12" as a venue
    Street {
        class: "residential",
        length: 0.0,
        name: "Świtały",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.33,
        lat: 54.5,
        numbers: &["12"],
    },
    // written "ul. M. Kopernika": with "ul." (ulica) and the given name's initial
    Street {
        class: "residential",
        length: 0.0,
        name: "Mikołaja Kopernika",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.39,
        lat: 54.5,
        numbers: &["11"],
    },
    Street {
        class: "residential",
        length: 0.01,
        name: "ulica Szeroka",
        postcode: "",
        city: "Centrum",
        cc: "PL",
        lon: 18.01,
        lat: 53.0,
        numbers: &["7"],
    },
    // libpostal leaves "20BS" in the road
    Street {
        class: "residential",
        length: 0.0,
        name: "Rakowice Małe",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.36,
        lat: 54.5,
        numbers: &["20bs"],
    },
    // the postcode's centre is nearer the longer name that also holds house 10
    Street {
        class: "residential",
        length: 0.0,
        name: "Wybudowanie pod Jaszcz",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.3,
        lat: 54.5,
        numbers: &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Jaszcz",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.3,
        lat: 54.52,
        numbers: &["10"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case("Świtały 12, 76-231, Poland", at_house("Świtały", "12")),
        case("Jaszcz 10, 76-231, Poland", at_house("Jaszcz", "10")),
        case(
            "ul. M. Kopernika 11, 76-231 Damnica, Poland",
            at_house("Mikołaja Kopernika", "11"),
        ),
        case(
            "ul. Mikołaja Kopernika 11, 76-231 Damnica",
            at_house("Mikołaja Kopernika", "11"),
        ),
        case("Szeroka 7, Stolica, Poland", at_house("ulica Szeroka", "7")),
        case(
            "Szeroka, Stolica, Poland",
            Want::At(house(street("ulica Szeroka"), "7"), 1200.0, true),
        ),
        case(
            "Rakowice Małe 20BS, 76-231, Poland",
            at_house("Rakowice Małe", "20bs"),
        ),
    ]
}
