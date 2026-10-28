use super::*;
use crate::{Case, at_house, case, town};

/// Libreville's department holds streets outside the town's own area: their houses are filed
/// under the department alone, named for its seat.
pub const PLACES: &[Place] = &[
    place!(
        "ga",
        "country",
        "",
        "GA",
        "",
        2_400_000,
        (11.6, -0.8),
        "",
        "Gabon",
        &[("en", "Gabon")]
    ),
    place!(
        "ga-1",
        "region",
        "",
        "GA",
        "GA-1",
        900_000,
        (9.8, 0.4),
        "ga",
        "Estuaire",
        &[("fr", "Estuaire"), ("en", "Estuaire Province")]
    ),
    place!(
        "ga-1-libreville-dept",
        "county",
        "",
        "GA",
        "GA-1",
        800_000,
        (9.6, 0.4),
        "ga-1",
        "Libreville",
        &[("fr", "Libreville")]
    ),
    place!(
        "ga-1-libreville",
        "locality",
        "city",
        "GA",
        "GA-1",
        700_000,
        (9.45, 0.4),
        "ga-1-libreville-dept",
        "Libreville",
        &[("fr", "Libreville")]
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Rue de la Décolonisation",
    postcode: "",
    city: "",
    cc: "GA",
    lon: 9.75,
    lat: 0.4,
    numbers: &["661"],
}];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "661 Rue de la Décolonisation, Libreville, Gabon",
            at_house("Rue de la Décolonisation", "661"),
        ),
        case(
            "Rue de la Décolonisation 661, Libreville, Estuaire, Gabon",
            at_house("Rue de la Décolonisation", "661"),
        ),
        // another region than the department's
        case(
            "661 Rue de la Décolonisation, Libreville, Ngounié, Gabon",
            town("ga-1-libreville", false),
        ),
    ]
}
