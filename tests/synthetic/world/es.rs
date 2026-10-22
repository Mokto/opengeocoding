use super::*;
use crate::*;

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
    place!(
        "es-burgos",
        "county",
        "",
        "ES",
        "",
        350_000,
        (50.03, -19.95),
        "es",
        "Burgos",
        &[("en", "Burgos")]
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
        "es-burgos",
        "Villaseca",
        &[("en", "Villaseca")]
    ),
    // One of Villaseca's villages, at its second Calle Mayor 3; Overture has it as a point.
    place!(
        "es-villaseca-pinedo",
        "locality",
        "village",
        "ES",
        "",
        80,
        (50.06, -19.95),
        "es-villaseca",
        "Pinedo",
        &[("en", "Pinedo")]
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
    // CartoCiudad writes house 12B as number 12 and extension B. 30 has only 30 A.
    Street {
        class: "residential",
        length: 0.0,
        name: "CALLE REAL",
        postcode: "09120",
        city: "Villaseca",
        cc: "ES",
        lon: 50.003,
        lat: -19.952,
        numbers: &["12", "14", "16", "18", "20", "12 B", "30 A"],
    },
    // A street of Pinedo, which the houses file under Villaseca.
    Street {
        class: "residential",
        length: 0.0,
        name: "CALLE ERAS",
        postcode: "09128",
        city: "Villaseca",
        cc: "ES",
        lon: 50.061,
        lat: -19.951,
        numbers: &["5"],
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
        case(
            "Calle Real 12B, Villaseca, Spain",
            at_house("CALLE REAL", "12 B"),
        ),
        // house 12 keeps its own point, not one moved toward 12B
        case(
            "Calle Real 12, Villaseca, Spain",
            at_house("CALLE REAL", "12"),
        ),
        // with no house 30 of its own, 30 is the building of 30 A
        case(
            "Calle Real 30, Villaseca, Spain",
            at_house("CALLE REAL", "30 A"),
        ),
        // the cadastre names the village before the municipality; Pinedo's Calle Mayor 3
        case(
            "Calle Mayor 3, Pinedo, Villaseca, Burgos, Spain",
            mayor("09128"),
        ),
    ]
}

#[test]
fn spanish_houses_are_suggested_with_their_extension_and_village() {
    let near = |typed: &str, want: (f64, f64)| {
        index()
            .suggest(typed, 5, None, None, false)
            .iter()
            .any(|s| s.kind == "address" && metres((s.x, s.y), want) < 5.0)
    };
    let real = street("CALLE REAL");
    assert!(near(
        "Calle Real 12B, Villaseca, Spain",
        house(real, "12 B")
    ));
    // the village holds no streets of its own: they are the municipality's
    assert!(near(
        "Calle Eras 5, Pinedo, Villaseca, Burgos, Spain",
        house(street("CALLE ERAS"), "5")
    ));
    // but not another village's house
    assert!(
        index()
            .suggest(
                "Calle Real 14, Pinedo, Villaseca, Burgos, Spain",
                5,
                None,
                None,
                false
            )
            .iter()
            .all(|s| s.kind != "address")
    );
}
