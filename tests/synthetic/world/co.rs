use super::*;

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
];
