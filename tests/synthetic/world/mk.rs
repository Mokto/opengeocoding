use super::*;
use crate::{Case, Want, at_house, case};

pub const PLACES: &[Place] = &[
    place!(
        "mk",
        "country",
        "",
        "MK",
        "",
        1_800_000,
        (21.7, 41.6),
        "",
        "Северна Македонија",
        &[("mk", "Северна Македонија"), ("en", "North Macedonia")]
    ),
    place!(
        "mk-radovish",
        "locality",
        "town",
        "MK",
        "",
        16_000,
        (22.46, 41.63),
        "mk",
        "Радовиш",
        &[("mk", "Радовиш"), ("en", "Radovish")]
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Св. Спасо Радовишки",
    postcode: "2420",
    city: "Радовиш",
    cc: "MK",
    lon: 22.461,
    lat: 41.63,
    numbers: &["32"],
}];

pub fn cases() -> Vec<Case> {
    // libpostal reads the number and street as one house name, and the postcode before the
    // country as the house number: only the completion lookup finds the house.
    vec![
        case(
            "32 Св. Спасо Радовишки, Радовиш, 2420, North Macedonia",
            at_house("Св. Спасо Радовишки", "32"),
        ),
        case(
            "Св. Спасо Радовишки 32, Радовиш, 2420, North Macedonia",
            at_house("Св. Спасо Радовишки", "32"),
        ),
        // a part that names no place may be another address's
        case(
            "32 Св. Спасо Радовишки, Кривец, Радовиш, 2420, North Macedonia",
            Want::Abstain,
        ),
    ]
}
