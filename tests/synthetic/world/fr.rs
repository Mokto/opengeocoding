use super::*;
use crate::{Case, Want, at_house, case, named, street, town};

pub const PLACES: &[Place] = &[
    place!(
        "fr",
        "country",
        "",
        "FR",
        "",
        68_000_000,
        (2.0, 46.0),
        "",
        "France",
        &[("en", "France")],
    ),
    place!(
        "fr-idf",
        "region",
        "",
        "FR",
        "FR-IDF",
        12_000_000,
        (2.5, 48.7),
        "fr",
        "Île-de-France",
        &[("en", "Ile-de-France")],
    ),
    place!(
        "fr-paris",
        "locality",
        "city",
        "FR",
        "FR-IDF",
        2_100_000,
        (2.3522, 48.8566),
        "fr-idf",
        "Paris",
        &[("en", "Paris"), ("fr", "Paris")],
    ),
    // villages that share a word with Paris's airport and a name with its museum, lighter
    // than a town
    place!(
        "fr-thingsmuseum",
        "locality",
        "village",
        "FR",
        "FR-IDF",
        0,
        (2.0, 47.0),
        "fr-idf",
        "Thingsmuseum",
        &[("en", "Thingsmuseum")],
    ),
    place!(
        "fr-nordville",
        "locality",
        "village",
        "FR",
        "FR-IDF",
        0,
        (2.0, 47.5),
        "fr-idf",
        "Nordville",
        &[("en", "Nordville")],
    ),
    place!(
        "fr-haut-rhin",
        "county",
        "",
        "FR",
        "FR-IDF",
        600000,
        (7.3, 48.0),
        "fr-idf",
        "Haut-Rhin",
        &[("en", "Haut-Rhin")],
    ),
    place!(
        "fr-colmar",
        "locality",
        "city",
        "FR",
        "FR-IDF",
        70000,
        (7.35, 48.08),
        "fr-haut-rhin",
        "Colmar",
        &[("en", "Colmar")],
    ),
    // Martinique answers as France, with its ISO 3166-2 code FR-972; its arrondissement is a
    // region of the data without a code.
    place!(
        "fr-mq",
        "dependency",
        "",
        "MQ",
        "",
        360_000,
        (-61.0, 14.65),
        "",
        "Martinique",
        &[("en", "Martinique")],
    ),
    place!(
        "fr-mq-fort-de-france-arrondissement",
        "region",
        "",
        "MQ",
        "",
        160_000,
        (-61.05, 14.62),
        "fr-mq",
        "Fort-de-France",
        &[("en", "Fort-de-France")],
    ),
    place!(
        "fr-mq-fort-de-france",
        "locality",
        "city",
        "MQ",
        "",
        76_000,
        (-61.07, 14.61),
        "fr-mq-fort-de-france-arrondissement",
        "Fort-de-France",
        &[("en", "Fort-de-France")],
    ),
];

pub const STREETS: &[Street] = &[
    // France has Köln's postcode 50667 too, in Nordville
    Street {
        class: "residential",
        length: 0.0,
        name: "Rue Haute",
        postcode: "50667",
        city: "Nordville",
        cc: "FR",
        lon: 2.005,
        lat: 47.5,
        numbers: &["3"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Rue de Rivoli",
        postcode: "75001",
        city: "Paris",
        cc: "FR",
        lon: 2.34,
        lat: 48.86,
        numbers: &["1", "2", "3", "4", "5", "6", "7bis", "8a", "9 C"],
    },
    // "Ch. Frau" is how a Swiss address's code and last word ("…, CH, Frau") read as a road
    Street {
        class: "residential",
        length: 0.0,
        name: "Chemin de la Frau",
        postcode: "50667",
        city: "Nordville",
        cc: "FR",
        lon: 2.01,
        lat: 47.5,
        numbers: &["1"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        named(
            "Fort-de-France, Martinique, France",
            "fr",
            "Fort-de-France",
            &["FR-972"],
            false,
        ),
        case("Paris, France", town("fr-paris", true)),
        case("Missing Road, Paris, France", town("fr-paris", false)),
        case("Île-de-France, Haut-Rhin", town("fr-haut-rhin", true)),
        case(
            "2 R. de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "2"),
        ),
        case(
            "7 bis R. de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "7bis"),
        ),
        case("FR-IDF-PARIS", town("fr-paris", true)),
        // the larger Paris, when nothing says otherwise
        case("Paris", town("fr-paris", true)),
        case(
            "2 Rue de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "2"),
        ),
        case("Rue de Rivoli 2, Paris", at_house("Rue de Rivoli", "2")),
        case(
            "7 bis Rue de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "7bis"),
        ),
        case(
            "8 A Rue de Rivoli, 75001 Paris, France",
            at_house("Rue de Rivoli", "8a"),
        ),
        case("2 Hohe Straße, Colonia, France", Want::Abstain),
        // the street is there, house 99 is not: the street, not a good match
        case(
            "99 Rue de Rivoli, Paris, France",
            Want::At(house(street("Rue de Rivoli"), "3"), 200.0, false),
        ),
        case(
            "Rue Haute 9, 50667 Nordville",
            Want::At(house(street("Rue Haute"), "3"), 300.0, false),
        ),
        case("Ulstrasse 99, 50667 Nordville", town("fr-nordville", false)),
        case("Atlantis, France", town("fr", false)),
        // a stated territory holds only its own places, and a stated country no other's
        case("Colmar, Guadeloupe", Want::Abstain),
        // a country code between commas (Switzerland's) is no word of a French road's name
        case("Kornhaldenstrasse 1, CH, Frau", Want::Abstain),
        case("Colmar, Western Sahara", Want::Abstain),
    ]
}

#[test]
fn a_spaced_suffix_is_kept_in_an_address_written_from_the_country_down() {
    let found = crate::index().suggest(
        "France, 75001 Paris, 7 bis Rue de Rivoli",
        5,
        None,
        None,
        false,
    );
    assert_eq!(found.first().map(|s| s.kind), Some("address"));
    let want = house(street("Rue de Rivoli"), "7bis");
    assert!(crate::metres((found[0].x, found[0].y), want) < 5.0);
}
