use super::*;
use crate::{Case, named};

/// A territory with an ISO 3166-1 code of its own, which its sovereign's ISO 3166-2 code US-PR
/// (Wikidata's Puerto Rico) also names.
pub const PLACES: &[Place] = &[
    place!(
        "pr",
        "dependency",
        "",
        "PR",
        "",
        0,
        (-66.5, 18.2),
        "",
        "Puerto Rico",
        &[("en", "Puerto Rico")],
    ),
    place!(
        "pr-san-juan",
        "locality",
        "city",
        "PR",
        "",
        340_000,
        (-66.1, 18.45),
        "pr",
        "San Juan",
        &[("en", "San Juan")],
    ),
];

pub fn cases() -> Vec<Case> {
    vec![
        // its own country, and no region of the United States
        named("Puerto Rico", "pr", "", &[], true),
        named("San Juan, Puerto Rico", "pr", "San Juan", &[], true),
    ]
}
