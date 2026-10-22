use super::*;
use crate::*;

pub const PLACES: &[Place] = &[
    // Colombia's county is the municipality: Bogotá's localities sit partly under the city and
    // partly beside it under the district, like San Cristóbal.
    place!(
        "co-dc",
        "region",
        "",
        "CO",
        "CO-DC",
        0,
        (-60.0, 30.3),
        "",
        "Bogotá, Distrito Capital",
        &[("en", "Capital District"), ("es", "Bogotá")]
    ),
    place!(
        "co-bogota-dc",
        "county",
        "",
        "CO",
        "CO-DC",
        0,
        (-60.0, 30.3),
        "co-dc",
        "Bogotá, D.c.",
        &[("en", "Bogotá, D.c.")]
    ),
    place!(
        "co-bogota",
        "locality",
        "city",
        "CO",
        "CO-DC",
        7_000_000,
        (-60.0, 30.0),
        "co-bogota-dc",
        "Bogotá",
        &[("en", "Bogotá")]
    ),
    place!(
        "co-san-cristobal",
        "locality",
        "",
        "CO",
        "CO-DC",
        400_000,
        (-60.0, 30.6),
        "co-bogota-dc",
        "San Cristóbal",
        &[("en", "San Cristóbal")]
    ),
];

pub const STREETS: &[Street] = &[
    // Bogotá's register: "AC" for Avenida Calle, plates keyed "86B 18" for "# 86B-18"
    Street {
        class: "residential",
        length: 0.0,
        name: "AC 43 S",
        postcode: "",
        city: "Bogotá",
        cc: "CO",
        lon: -60.0,
        lat: 30.0,
        numbers: &["86B 18", "54D 35 S"],
    },
    // One CL 22BIS S in the city without the house, another in San Cristóbal with it; the
    // register names no postal city
    Street {
        class: "residential",
        length: 0.0,
        name: "CL 22BIS S",
        postcode: "",
        city: "",
        cc: "CO",
        lon: -60.05,
        lat: 30.05,
        numbers: &["70 12"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "CL 22BIS S",
        postcode: "",
        city: "",
        cc: "CO",
        lon: -60.0,
        lat: 30.6,
        numbers: &["9C 18 E"],
    },
    // OpenStreetMap keys a plate as written; the register form of it elsewhere on the street
    // must not win over the house as typed
    Street {
        class: "residential",
        length: 0.0,
        name: "Calle 20",
        postcode: "",
        city: "Bogotá",
        cc: "CO",
        lon: -59.95,
        lat: 30.0,
        numbers: &["11-23", "11 23"],
    },
];

#[test]
fn bogota_plates_are_suggested_as_the_registers_houses() {
    let ac = street("AC 43 S");
    let san_cristobal = STREETS
        .iter()
        .find(|s| s.numbers.contains(&"9C 18 E"))
        .unwrap();
    for (typed, cc, want) in [
        (
            "AC 43 S # 86B-18, Bogotá, Colombia",
            None,
            house(ac, "86B 18"),
        ),
        (
            "Avenida Calle 43 S # 86B-18, Bogotá, Colombia",
            None,
            house(ac, "86B 18"),
        ),
        (
            "Avenida Calle 43 S 86B-18, Bogotá",
            None,
            house(ac, "86B 18"),
        ),
        (
            "AC 43 S # 54D-35 S, Bogotá",
            Some("CO"),
            house(ac, "54D 35 S"),
        ),
        (
            "CL 22BIS S # 9C-18 E, Bogotá, Colombia",
            None,
            house(san_cristobal, "9C 18 E"),
        ),
        (
            "Calle 20 11-23, Bogotá, Colombia",
            None,
            house(street("Calle 20"), "11-23"),
        ),
        (
            "Calle 20 # 11-23, Bogotá",
            Some("CO"),
            house(street("Calle 20"), "11-23"),
        ),
        // typed so far: the plate's distance not finished
        ("AC 43 S # 86B-1", Some("CO"), house(ac, "86B 18")),
    ] {
        let found = index().suggest(typed, 5, cc, None, false);
        assert!(!found.is_empty(), "{typed}");
        assert_eq!(
            found[0].kind, "address",
            "{typed}: {}",
            found[0].display_name
        );
        assert!(
            metres((found[0].x, found[0].y), want) < 5.0,
            "{typed}: {}",
            found[0].display_name
        );
    }
    // another plate is not borrowed
    for typed in [
        "AC 43 S # 86B-19, Bogotá, Colombia",
        "AC 44 S # 86B-18, Bogotá, Colombia",
    ] {
        assert!(
            index()
                .suggest(typed, 5, None, None, false)
                .iter()
                .all(|s| s.kind != "address"),
            "{typed}"
        );
    }
}
