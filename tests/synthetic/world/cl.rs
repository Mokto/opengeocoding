use super::*;
use crate::{Case, named};

/// Arica, in Arica y Parinacota since it was split from Tarapacá, whose border is about 55 km
/// south of the town; Magallanes is far from both. The regions' areas are in `AREAS`.
pub const PLACES: &[Place] = &[
    place!(
        "cl",
        "country",
        "",
        "CL",
        "",
        19_000_000,
        (-71.0, -35.0),
        "",
        "Chile",
        &[("en", "Chile")],
    ),
    place!(
        "cl-ap",
        "region",
        "",
        "CL",
        "CL-AP",
        250_000,
        (-69.7, -18.5),
        "cl",
        "Región de Arica y Parinacota",
        &[("en", "Arica y Parinacota Region")],
    ),
    place!(
        "cl-ta",
        "region",
        "",
        "CL",
        "CL-TA",
        380_000,
        (-69.7, -19.8),
        "cl",
        "Región de Tarapacá",
        &[("en", "Tarapacá Region")],
    ),
    place!(
        "cl-ma",
        "region",
        "",
        "CL",
        "CL-MA",
        170_000,
        (-71.0, -53.0),
        "cl",
        "Región de Magallanes",
        &[("en", "Magallanes Region")],
    ),
    place!(
        "cl-ap-arica",
        "locality",
        "city",
        "CL",
        "CL-AP",
        202_000,
        (-70.0, -18.8),
        "cl-ap",
        "Arica",
        &[("en", "Arica")],
    ),
];

pub fn cases() -> Vec<Case> {
    vec![
        // the region it was in, next to its own
        named(
            "Arica, de Tarapaca, Chile",
            "cl",
            "Arica",
            &["CL-AP"],
            false,
        ),
        // a region far from the town is not one it was in
        named("Arica, Magallanes, Chile", "cl", "", &["CL-MA"], false),
    ]
}
