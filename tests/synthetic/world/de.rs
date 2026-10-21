use super::*;
use crate::{Case, Want, at_house, case, street, town};

pub const PLACES: &[Place] = &[
    place!(
        "de",
        "country",
        "",
        "DE",
        "",
        83_000_000,
        (10.5, 51.0),
        "",
        "Germany",
        &[("en", "Germany")],
    ),
    place!(
        "de-nw",
        "region",
        "",
        "DE",
        "DE-NW",
        18_000_000,
        (7.5, 51.5),
        "de",
        "Nordrhein-Westfalen",
        &[("en", "North Rhine-Westphalia")],
    ),
    place!(
        "de-koeln",
        "locality",
        "city",
        "DE",
        "DE-NW",
        1_000_000,
        (6.9603, 50.9375),
        "de-nw",
        "Köln",
        &[("en", "Cologne"), ("de", "Köln"), ("es", "Colonia")],
    ),
    // a village Overture has as a point inside Cologne's polygon, with none of its own
    place!(
        "de-koeln-feldheim",
        "locality",
        "village",
        "DE",
        "DE-NW",
        800,
        (6.86, 50.903),
        "de-koeln",
        "Feldheim",
        &[("en", "Feldheim")],
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Ulstrasse",
        postcode: "50667",
        city: "Köln",
        cc: "DE",
        lon: 6.954,
        lat: 50.94,
        numbers: &["25", "26"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Birkenstrasse",
        postcode: "50667",
        city: "Köln",
        cc: "DE",
        lon: 6.957,
        lat: 50.94,
        numbers: &["24"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Hohe Straße",
        postcode: "50667",
        city: "Köln",
        cc: "DE",
        lon: 6.95,
        lat: 50.94,
        numbers: &["1", "2", "3"],
    },
    // a street with no houses
    Street {
        class: "residential",
        length: 0.02,
        name: "Feldweg",
        postcode: "",
        city: "Köln",
        cc: "DE",
        lon: 6.85,
        lat: 50.9,
        numbers: &[],
    },
    // a path, which is no street to answer a point with
    Street {
        class: "footway",
        length: 0.01,
        name: "Uferpfad",
        postcode: "",
        city: "Köln",
        cc: "DE",
        lon: 6.9,
        lat: 50.97,
        numbers: &[],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "25 Ulstr., 50667 Köln, Germany",
            at_house("Ulstrasse", "25"),
        ),
        case(
            "24 Birkenstarsse, 50667 Köln, Germany",
            at_house("Birkenstrasse", "24"),
        ),
        case(
            "24 Birkensrasse, 50667 Köln, Germany",
            at_house("Birkenstrasse", "24"),
        ),
        case("Hohe Strasse 2, Köln", at_house("Hohe Straße", "2")),
        case(
            "2 Hohe Straße, Colonia, Germany",
            at_house("Hohe Straße", "2"),
        ),
        case("Cologne, Germany", town("de-koeln", true)),
        case("Köln, Germany", town("de-koeln", true)),
        Case {
            query: "Koeln",
            want: town("de-koeln", true),
            known_failure: "index: search does not fold the oe digraph for ö, as suggest does",
        },
        // With no country, a postcode and a town near it in one country place the query there,
        // when the postcode's place has another name (Feldheim is in Köln's 50667). France has
        // 50667 too, in Nordville.
        case(
            "Ulstrasse 99, 50667 Feldheim",
            Want::At(house(street("Ulstrasse"), "25"), 300.0, false),
        ),
        case(
            "50667 Feldheim",
            Want::At(house(street("Ulstrasse"), "25"), 1000.0, true),
        ),
        case(
            "Rue Haute 9, 50667 Feldheim",
            Want::At(house(street("Ulstrasse"), "25"), 1000.0, false),
        ),
        case("Paris, Germany", Want::Abstain),
    ]
}
