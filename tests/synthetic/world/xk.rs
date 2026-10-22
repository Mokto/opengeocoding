use super::*;
use crate::{Case, at_house, case, town};

pub const PLACES: &[Place] = &[
    place!(
        "xk",
        "country",
        "",
        "XK",
        "",
        1_700_000,
        (21.0, 42.5),
        "",
        "Kosova",
        &[("sq", "Kosova"), ("en", "Kosovo")]
    ),
    place!(
        "xk-lugishte",
        "locality",
        "town",
        "XK",
        "",
        7_000,
        (21.0, 42.5),
        "xk",
        "Lugishtë",
        &[("sq", "Lugishtë")]
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Dëshmorët e Lirisë",
    postcode: "21000",
    city: "Lugishtë",
    cc: "XK",
    lon: 21.001,
    lat: 42.5,
    numbers: &["147"],
}];

pub fn cases() -> Vec<Case> {
    // libpostal knows no Kosovo: written last, it reads as a road with the parts before it.
    vec![
        case(
            "147 Dëshmorët e Lirisë, Lugishtë, 21000, Kosovo",
            at_house("Dëshmorët e Lirisë", "147"),
        ),
        case(
            "Dëshmorët e Lirisë 147, Lugishtë, 21000, Kosovo",
            at_house("Dëshmorët e Lirisë", "147"),
        ),
        case(
            "Dëshmorët e Lirisë 147, Lugishtë, Kosovo",
            at_house("Dëshmorët e Lirisë", "147"),
        ),
        case("Lugishtë, Kosovo", town("xk-lugishte", true)),
    ]
}
