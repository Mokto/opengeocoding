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

const REGISTRY_STREET: Street = Street {
    class: "residential",
    length: 0.0,
    name: "CALLE CASTILLO",
    postcode: "09120",
    city: "Villaseca",
    cc: "ES",
    lon: 50.002,
    lat: -19.951,
    numbers: &["2"],
};

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
    REGISTRY_STREET,
    Street {
        name: "AV GALICIA",
        numbers: &["40"],
        lon: 50.04,
        ..REGISTRY_STREET
    },
    Street {
        name: "AV CANDELAS",
        numbers: &["63"],
        lon: 50.041,
        ..REGISTRY_STREET
    },
    Street {
        name: "CM PINEDES",
        numbers: &["3", "3B"],
        lon: 50.01,
        ..REGISTRY_STREET
    },
    Street {
        name: "CAMI DE SES FONTS",
        numbers: &["4"],
        lon: 50.046,
        ..REGISTRY_STREET
    },
    Street {
        name: "CM FONTS",
        numbers: &["4"],
        lon: 50.047,
        ..REGISTRY_STREET
    },
    Street {
        name: "PLAZA CONSTITUCION",
        numbers: &["10"],
        lon: 50.004,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE SAGRAT COR",
        numbers: &["68"],
        lon: 50.005,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE DEL PUENTE",
        numbers: &["4"],
        lon: 50.006,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE PUENTE",
        numbers: &["4"],
        lon: 50.007,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE DEL OLMO",
        numbers: &["8"],
        lon: 50.008,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE OLMO",
        numbers: &["4"],
        lon: 50.009,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE TORRES ALTAS",
        numbers: &["6"],
        lon: 50.02,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE TORRES BAJAS",
        numbers: &["6"],
        lon: 50.021,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR A",
        numbers: &["6", "7", "8", "9"],
        lon: 50.03000,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR B",
        numbers: &["1", "2", "3"],
        lon: 50.03005,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR C",
        numbers: &["1", "2", "3"],
        lon: 50.03010,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR D",
        numbers: &["1", "2", "3"],
        lon: 50.03015,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR E",
        numbers: &["1", "2", "3"],
        lon: 50.03020,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR F",
        numbers: &["1", "2", "3"],
        lon: 50.03025,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR G",
        numbers: &["1", "2", "3"],
        lon: 50.03030,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR H",
        numbers: &["1", "2", "3"],
        lon: 50.03035,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR I",
        numbers: &["1", "2", "3"],
        lon: 50.03040,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR J",
        numbers: &["1", "2", "3"],
        lon: 50.03045,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR K",
        numbers: &["1", "2", "3"],
        lon: 50.03050,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR L",
        numbers: &["1", "2", "3"],
        lon: 50.03055,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR M",
        numbers: &["1", "2", "3"],
        lon: 50.03060,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR N",
        numbers: &["1", "2", "3"],
        lon: 50.03065,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR O",
        numbers: &["1", "2", "3"],
        lon: 50.03070,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR P",
        numbers: &["1", "2", "3"],
        lon: 50.03075,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR Q",
        numbers: &["1", "2", "3"],
        lon: 50.03080,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR R",
        numbers: &["1", "2", "3"],
        lon: 50.03085,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR S",
        numbers: &["1", "2", "3"],
        lon: 50.03090,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR T",
        numbers: &["1", "2", "3"],
        lon: 50.03095,
        ..REGISTRY_STREET
    },
    Street {
        name: "CALLE MIRADOR U",
        numbers: &["6"],
        lon: 50.03100,
        ..REGISTRY_STREET
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
        case(
            "3 Camí de ses Pinedes, Villaseca, 09120, Spain",
            at_house("CM PINEDES", "3"),
        ),
        case(
            "Camí de ses Pinedes 3, Villaseca, 09120, Spain",
            at_house("CM PINEDES", "3"),
        ),
        case(
            "Camino de ses Pinedes 3, Villaseca, 09120, Spain",
            at_house("CM PINEDES", "3"),
        ),
        case(
            "Camí de ses Pinedes 3B, Villaseca, 09120, Spain",
            at_house("CM PINEDES", "3B"),
        ),
        case(
            "Camí de ses Fonts 4, Villaseca, 09120, Spain",
            at_house("CAMI DE SES FONTS", "4"),
        ),
        case(
            "40 Avenida de Galicia, Villaseca, 09120, Spain",
            at_house("AV GALICIA", "40"),
        ),
        case(
            "63 Avenida de las Candelas, Villaseca, 09120, Spain",
            at_house("AV CANDELAS", "63"),
        ),
        case(
            "Avenida de Galicia 40, Villaseca, 09120, Spain",
            at_house("AV GALICIA", "40"),
        ),
        case(
            "Avenida de las Candelas 63, Villaseca, 09120, Spain",
            at_house("AV CANDELAS", "63"),
        ),
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
fn spanish_registry_road_names_complete_only_unambiguous_houses() {
    for (road, number, stored) in [
        ("Calle del Castillo", "2", "CALLE CASTILLO"),
        ("Plaza de la Constitución", "10", "PLAZA CONSTITUCION"),
        ("Carrer del Sagrat Cor", "68", "CALLE SAGRAT COR"),
    ] {
        for first in [format!("{number} {road}"), format!("{road} {number}")] {
            let text = format!("{first}, Villaseca, 09120, Spain");
            let found = index().suggest(&text, 5, None, None, false);
            let houses: Vec<_> = found.iter().filter(|s| s.kind == "address").collect();
            assert_eq!(houses.len(), 1, "{text}");
            assert!(
                metres((houses[0].x, houses[0].y), house(street(stored), number)) < 5.0,
                "{text}"
            );
        }
    }
    let literal = index().suggest("Calle del Puente 4, Villaseca, Spain", 5, None, None, false);
    assert!(
        metres(
            (literal[0].x, literal[0].y),
            house(street("CALLE DEL PUENTE"), "4")
        ) < 5.0
    );
    let torres = index().suggest("Calle Torres 6, Villaseca, Spain", 5, None, None, false);
    assert_eq!(
        torres.iter().filter(|s| s.kind == "address").count(),
        2,
        "{:?}",
        torres
            .iter()
            .map(|s| (&s.display_name, s.x, s.y))
            .collect::<Vec<_>>()
    );
    for (text, cc) in [
        ("Calle del Castillo 2, Villaseca, 09128, Spain", None),
        ("Calle del Castillo 2, Pinedo, Spain", None),
        ("Calle del Castillo 2B, Villaseca, Spain", None),
        ("Calle del Castillo 2", Some("ES")),
        ("Calle del Castillo 2, Villaseca, Spain", Some("FR")),
        ("Calle del Olmo 4, Villaseca, Spain", None),
        ("Calle de Torres 6, Villaseca, Spain", None),
    ] {
        for limit in [1, 5] {
            assert!(
                index()
                    .suggest(text, limit, cc, None, false)
                    .iter()
                    .all(|s| s.kind != "address"),
                "{text} (limit {limit}): {:?}",
                index()
                    .suggest(text, limit, cc, None, false)
                    .iter()
                    .map(|s| (&s.display_name, s.x, s.y))
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn spanish_registry_paths_keep_complete_numbers_and_qualifiers() {
    let village = place("es-villaseca-pinedo");
    assert!(metres(house(street("CM PINEDES"), "3"), (village.lon, village.lat)) > 3000.0);
    for text in [
        "3 Camí de ses Pinedes, Villaseca, 09120, Spain",
        "Camí de ses Pinedes 3, Villaseca, 09120, Spain",
        "Camino de ses Pinedes 3, Villaseca, 09120, Spain",
        "Cno. de ses Pinedes 3, Villaseca, 09120, Spain",
        "Camí de ses Pinedes 3B, Villaseca, 09120, Spain",
    ] {
        let number = if text.contains("3B") { "3B" } else { "3" };
        let found = index().suggest(text, 5, None, None, false);
        assert_eq!(found.len(), 1, "{text}");
        assert_eq!(found[0].kind, "address", "{text}");
        assert!(
            metres(
                (found[0].x, found[0].y),
                house(street("CM PINEDES"), number)
            ) < 5.0,
            "{text}"
        );
    }
    let literal = index().suggest(
        "Camí de ses Fonts 4, Villaseca, 09120, Spain",
        5,
        None,
        None,
        false,
    );
    assert!(
        metres(
            (literal[0].x, literal[0].y),
            house(street("CAMI DE SES FONTS"), "4")
        ) < 5.0
    );
    for (text, cc) in [
        ("Camí de ses Pinedes 3, Villaseca, 09128, Spain", None),
        ("Camí de ses Pinedes 3, Pinedo, Spain", None),
        ("Camí de ses Pinedes 3, Villaseca, Spain", Some("FR")),
        ("Camí de ses Pinedes 3C, Villaseca, Spain", None),
        ("Camí de ses Pinedes 3/4, Villaseca, Spain", None),
        ("Camí de ses Pinedes 3-4, Villaseca, Spain", None),
        ("Camí de ses Pinedes 3", Some("ES")),
    ] {
        let found = index().suggest(text, 5, cc, None, false);
        assert!(found.iter().all(|s| s.kind != "address"), "{text}");
    }
}

#[test]
fn spanish_road_repairs_reject_uniqueness_from_a_truncated_lookup() {
    let houses = |text: &str, limit| {
        index()
            .suggest(text, limit, None, None, false)
            .into_iter()
            .filter(|s| s.kind == "address")
            .count()
    };
    assert_eq!(houses("Calle Mirador 6, Villaseca, Spain", 1), 1);
    assert_eq!(houses("Calle Mirador 6, Villaseca, Spain", 25), 2);
    for limit in [1, 5] {
        assert_eq!(houses("Calle del Mirador 6, Villaseca, Spain", limit), 0);
    }
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
