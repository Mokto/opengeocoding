use super::*;

pub const PLACES: &[Place] = &[
    place!(
        "li",
        "country",
        "",
        "LI",
        "",
        40000,
        (61.6, 0.0),
        "",
        "Liechtenstein",
        &[("en", "Liechtenstein")],
    ),
    place!(
        "li-vaduz",
        "locality",
        "",
        "LI",
        "",
        5000,
        (61.65, 0.0),
        "li",
        "Vaduz",
        &[("en", "Vaduz")],
    ),
    place!(
        "li-westufer",
        "locality",
        "town",
        "LI",
        "",
        20000,
        (79.999, 0.0),
        "li",
        "Westufer",
        &[]
    ),
];
