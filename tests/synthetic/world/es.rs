use super::*;
use crate::{Case, Want, at_house, case};

pub const PLACES: &[Place] = &[
    place!(
        "es",
        "country",
        "",
        "ES",
        "",
        47_000_000,
        (50.0, -19.95),
        "",
        "Spain",
        &[("en", "Spain"), ("es", "España")]
    ),
    // A municipality whose villages each have a Calle Mayor 3, in one 0.1° cell.
    place!(
        "es-villaseca",
        "locality",
        "town",
        "ES",
        "",
        3_000,
        (50.0, -19.95),
        "es",
        "Villaseca",
        &[("en", "Villaseca")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Calle Mayor",
        postcode: "09120",
        city: "Villaseca",
        cc: "ES",
        lon: 50.001,
        lat: -19.95,
        numbers: &["3"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Calle Mayor",
        postcode: "09128",
        city: "Villaseca",
        cc: "ES",
        lon: 50.06,
        lat: -19.95,
        numbers: &["3"],
    },
    // The registry leaves the article out of "Calle del Castillo".
    Street {
        class: "residential",
        length: 0.0,
        name: "CALLE CASTILLO",
        postcode: "09120",
        city: "Villaseca",
        cc: "ES",
        lon: 50.002,
        lat: -19.951,
        numbers: &["2"],
    },
];

pub fn cases() -> Vec<Case> {
    let mayor = |postcode: &str| {
        let s = STREETS
            .iter()
            .find(|s| s.name == "Calle Mayor" && s.postcode == postcode)
            .unwrap();
        Want::At(house(s, "3"), 25.0, true)
    };
    vec![
        case("Calle Mayor 3, 09120 Villaseca, Spain", mayor("09120")),
        case("Calle Mayor 3, 09128 Villaseca, Spain", mayor("09128")),
        case(
            "Calle del Castillo 2, 09120 Villaseca, Spain",
            at_house("CALLE CASTILLO", "2"),
        ),
    ]
}
