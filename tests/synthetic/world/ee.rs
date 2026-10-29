use super::*;
use crate::{Case, at_house, case, index, metres, street, town};

pub const PLACES: &[Place] = &[
    place!(
        "ee",
        "country",
        "",
        "EE",
        "",
        1_300_000,
        (25.0, 58.7),
        "",
        "Estonia",
        &[("en", "Estonia")]
    ),
    place!(
        "ee-kivilinn",
        "locality",
        "town",
        "EE",
        "",
        90_000,
        (26.72, 58.4),
        "ee",
        "Kivilinn",
        &[("et", "Kivilinn")]
    ),
    place!(
        "ee-kivi-kula",
        "locality",
        "village",
        "EE",
        "",
        200,
        (26.9, 58.5),
        "ee",
        "Kivi küla",
        &[("et", "Kivi küla")]
    ),
];

const fn kivilinn(name: &'static str, lon: f64, numbers: &'static [&'static str]) -> Street {
    Street {
        class: "residential",
        length: 0.0,
        name,
        postcode: "",
        city: "Kivilinn",
        cc: "EE",
        lon,
        lat: 58.4,
        numbers,
    }
}

pub const STREETS: &[Street] = &[
    // addresses write the street type short: tn, mnt, pst
    kivilinn("Vinkli tänav", 26.70, &["8"]),
    kivilinn("Tartu maantee", 26.71, &["25"]),
    kivilinn("Rävala puiestee", 26.72, &["2"]),
    // a street the data names with the short type, beside one with the long
    kivilinn("Kalda mnt", 26.73, &["4"]),
    kivilinn("Kalda maantee", 26.74, &["4"]),
    // the register writes "A. Lauteri tn"
    kivilinn("Ants Lauteri tänav", 26.75, &["7c"]),
    // a farm named, not numbered, in its village: the builder files it as the name numbered on
    // a street of the village's name
    Street {
        class: "residential",
        length: 0.0,
        name: "Kivi küla",
        postcode: "",
        city: "Kivi küla",
        cc: "EE",
        lon: 26.9,
        lat: 58.5,
        numbers: &["Antsu"],
    },
];

const ABBREVIATED: &[(&str, &str, &str)] = &[
    ("Vinkli tn 8, Kivilinn, Estonia", "Vinkli tänav", "8"),
    ("Vinkli tn. 8, Kivilinn, Estonia", "Vinkli tänav", "8"),
    ("Tartu mnt 25, Kivilinn, Estonia", "Tartu maantee", "25"),
    ("Rävala pst 2, Kivilinn, Estonia", "Rävala puiestee", "2"),
    ("Kalda mnt 4, Kivilinn, Estonia", "Kalda mnt", "4"),
    ("Kalda maantee 4, Kivilinn, Estonia", "Kalda maantee", "4"),
];

/// The register's own order (county first), a farm's name in its village, and a street named
/// with initials.
const REGISTER: &[(&str, &str, &str)] = &[
    (
        "Tartu maakond, Kivilinn, Vinkli tn 8, Estonia",
        "Vinkli tänav",
        "8",
    ),
    (
        "A. Lauteri tn 7c, Kivilinn, Estonia",
        "Ants Lauteri tänav",
        "7c",
    ),
    (
        "Tartu maakond, Kivilinn, A. Lauteri tn 7c, Estonia",
        "Ants Lauteri tänav",
        "7c",
    ),
    (
        "Antsu, Kivi küla, Kivi vald, Tartu maakond, Estonia",
        "Kivi küla",
        "Antsu",
    ),
    (
        "Tartu maakond, Kivi vald, Kivi küla, Antsu, Estonia",
        "Kivi küla",
        "Antsu",
    ),
];

pub fn cases() -> Vec<Case> {
    ABBREVIATED
        .iter()
        .chain(REGISTER)
        .map(|&(query, road, number)| case(query, at_house(road, number)))
        .chain([
            // another initial names no street of the town
            case(
                "K. Lauteri tn 7c, Kivilinn, Estonia",
                town("ee-kivilinn", false),
            ),
        ])
        .collect()
}

#[test]
fn estonian_short_street_types_suggest_the_house() {
    for &(typed, road, number) in ABBREVIATED {
        let found = index().suggest(typed, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        assert!(
            metres((found[0].x, found[0].y), house(street(road), number)) < 5.0,
            "{typed}"
        );
    }
}

#[test]
fn estonian_register_forms_suggest_the_house() {
    for (typed, road, number) in [
        (
            "Antsu, Kivi küla, Kivi vald, Tartu maakond, Estonia",
            "Kivi küla",
            "Antsu",
        ),
        (
            "Tartu maakond, Kivi vald, Kivi küla, Antsu, Estonia",
            "Kivi küla",
            "Antsu",
        ),
        // a postcode the data lacks
        ("Vinkli tn 8, 51000 Kivilinn, Estonia", "Vinkli tänav", "8"),
    ] {
        let found = index().suggest(typed, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{typed}");
        assert!(
            metres((found[0].x, found[0].y), house(street(road), number)) < 5.0,
            "{typed}"
        );
    }
}
