use super::*;

pub const PLACES: &[Place] = &[
    place!(
        "eg",
        "country",
        "",
        "EG",
        "",
        110_000_000,
        (30.0, 27.0),
        "",
        "مصر",
        &[("ar", "مصر"), ("en", "Egypt")],
    ),
    place!(
        "eg-ast",
        "region",
        "",
        "EG",
        "EG-AST",
        4_400_000,
        (31.55, 27.21),
        "eg",
        "أسيوط",
        &[("ar", "أسيوط"), ("en", "Asyut")],
    ),
    place!(
        "eg-ast-manfalut",
        "locality",
        "city",
        "EG",
        "EG-AST",
        80_000,
        (30.97, 27.31),
        "eg-ast",
        "منفلوط",
        &[("ar", "منفلوط"), ("en", "Manfalut")],
    ),
];

pub const STREETS: &[Street] = &[
    // libpostal reads a number written before an Arabic street as part of the road.
    Street {
        class: "residential",
        length: 0.0,
        name: "شارع الكنيسه",
        postcode: "71621",
        city: "منفلوط",
        cc: "EG",
        lon: 30.97,
        lat: 27.31,
        numbers: &["5"],
    },
];
