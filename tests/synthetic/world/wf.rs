use super::*;

pub const PLACES: &[Place] = &[
    // A territory with an ISO 3166-2 code of its own (it borrows FR-WF's item) above a region
    // that has another code.
    place!(
        "wf",
        "dependency",
        "",
        "WF",
        "",
        10_000,
        (-150.0, -20.0),
        "",
        "Coral Isles",
        &[("en", "Coral Isles")],
    ),
    place!(
        "wf-reefward",
        "region",
        "",
        "WF",
        "WF-RF",
        8_000,
        (-150.0, -20.0),
        "wf",
        "Reefward",
        &[("en", "Reefward")],
    ),
    place!(
        "wf-lagoonport",
        "locality",
        "town",
        "WF",
        "WF-RF",
        1_000,
        (-150.1, -20.0),
        "wf-reefward",
        "Lagoonport",
        &[("en", "Lagoonport")],
    ),
];
