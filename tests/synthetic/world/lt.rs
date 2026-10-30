use super::*;
use crate::{Case, case};

/// One county and two of its municipalities, each with a village Užbaliai whose houses are
/// numbered on the village's own name, as Lithuanian villages without streets are; and a street
/// the data names with its given name's initial.
pub const PLACES: &[Place] = &[
    place!(
        "lt",
        "country",
        "",
        "LT",
        "",
        2_800_000,
        (140.5, -50.0),
        "",
        "Lietuva",
        &[("lt", "Lietuva"), ("en", "Lithuania")]
    ),
    place!(
        "lt-ta",
        "region",
        "",
        "LT",
        "LT-TA",
        100_000,
        (140.5, -50.0),
        "lt",
        "Tauragės apskritis",
        &[("lt", "Tauragės apskritis"), ("en", "Tauragė County")]
    ),
    place!(
        "lt-pagegiai",
        "county",
        "",
        "LT",
        "LT-TA",
        7_000,
        (140.0, -50.0),
        "lt-ta",
        "Pagėgių savivaldybė",
        &[("lt", "Pagėgių savivaldybė")]
    ),
    place!(
        "lt-jurbarkas",
        "county",
        "",
        "LT",
        "LT-TA",
        25_000,
        (141.0, -50.0),
        "lt-ta",
        "Jurbarko rajono savivaldybė",
        &[
            ("lt", "Jurbarko rajono savivaldybė"),
            ("en", "Jurbarkas District Municipality")
        ]
    ),
    place!(
        "lt-pagegiai-uzbaliai",
        "locality",
        "village",
        "LT",
        "LT-TA",
        50,
        (140.0, -50.0),
        "lt-pagegiai",
        "Užbaliai",
        &[("lt", "Užbaliai")]
    ),
    place!(
        "lt-jurbarkas-uzbaliai",
        "locality",
        "village",
        "LT",
        "LT-TA",
        80,
        (141.0, -50.0),
        "lt-jurbarkas",
        "Užbaliai",
        &[("lt", "Užbaliai")]
    ),
    place!(
        "lt-paezerys",
        "locality",
        "village",
        "LT",
        "LT-TA",
        300,
        (140.0, -49.6),
        "lt-pagegiai",
        "Paežerys",
        &[("lt", "Paežerys")]
    ),
];

const fn street(
    name: &'static str,
    postcode: &'static str,
    city: &'static str,
    (lon, lat): (f64, f64),
) -> Street {
    Street {
        class: "residential",
        length: 0.0,
        name,
        postcode,
        city,
        cc: "LT",
        lon,
        lat,
        numbers: &["1", "2", "2B"],
    }
}

const PAGEGIAI_UZBALIAI: Street = street("Užbalių k.", "LT-99240", "Užbalių k.", (140.0, -50.0));
const JURBARKAS_UZBALIAI: Street = street("Užbalių k.", "LT-74216", "Užbalių k.", (141.0, -50.0));
const ROZMANO: Street = street("V. Rozmano g.", "LT-99241", "Paežerio k.", (140.0, -49.6));

pub const STREETS: &[Street] = &[PAGEGIAI_UZBALIAI, JURBARKAS_UZBALIAI, ROZMANO];

fn at(street: &Street, number: &str) -> crate::Want {
    crate::Want::At(house(street, number), 25.0, true)
}

pub fn cases() -> Vec<Case> {
    vec![
        // the register's order, municipality first: the village's house in that municipality
        case(
            "Pagėgių sav., Užbalių k. 1, Lithuania",
            at(&PAGEGIAI_UZBALIAI, "1"),
        ),
        case(
            "Jurbarko r. sav., Užbalių k. 1, Lithuania",
            at(&JURBARKAS_UZBALIAI, "1"),
        ),
        // Lietuvos paštas's order, the postcode before the municipality
        case(
            "Užbalių k. 2, LT-74216 Jurbarko r. sav., Lithuania",
            at(&JURBARKAS_UZBALIAI, "2"),
        ),
        // the register spells out the given name the data writes as an initial
        case(
            "Vlado Rozmano g. 2B, Paežerio k., LT-99241 Pagėgių sav., Lithuania",
            at(&ROZMANO, "2B"),
        ),
        case(
            "Pagėgių sav., Paežerio k., Vlado Rozmano g. 2, Lithuania",
            at(&ROZMANO, "2"),
        ),
    ]
}
