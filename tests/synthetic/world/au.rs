use super::*;
use crate::*;

pub const PLACES: &[Place] = &[
    place!(
        "au",
        "country",
        "",
        "AU",
        "",
        25_000_000,
        (135.0, -25.0),
        "",
        "Australia",
        &[("en", "Australia")]
    ),
    place!(
        "au-qld",
        "region",
        "",
        "AU",
        "AU-QLD",
        5_000_000,
        (145.0, -22.0),
        "au",
        "Queensland",
        &[("en", "Queensland")]
    ),
    // a region whose later word is the country's name
    place!(
        "au-sa",
        "region",
        "",
        "AU",
        "AU-SA",
        1_800_000,
        (135.0, -30.0),
        "au",
        "South Australia",
        &[("en", "South Australia")]
    ),
    place!(
        "au-wattleford",
        "locality",
        "town",
        "AU",
        "",
        20_000,
        (149.2, -21.2),
        "au-qld",
        "Wattleford",
        &[("en", "Wattleford")]
    ),
    // a metropolitan locality whose area holds a town 41 km from its centre, as Brisbane's holds
    // Caboolture
    place!(
        "au-ardmore",
        "locality",
        "city",
        "AU",
        "AU-QLD",
        2_000_000,
        (150.0, -22.5),
        "au-qld",
        "Ardmore",
        &[("en", "Ardmore")]
    ),
    place!(
        "au-burrum-flats",
        "locality",
        "city",
        "AU",
        "AU-QLD",
        30_000,
        (150.4, -22.5),
        "au-ardmore",
        "Burrum Flats",
        &[("en", "Burrum Flats")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Banksia Street",
        postcode: "4740",
        city: "Wattleford",
        cc: "AU",
        lon: 149.2,
        lat: -21.2,
        numbers: &["206", "208", "210", "8/206"],
    },
    // written street before number, which libpostal reads as a venue
    Street {
        class: "residential",
        length: 0.0,
        name: "Panorama Place",
        postcode: "4740",
        city: "Wattleford",
        cc: "AU",
        lon: 149.205,
        lat: -21.2,
        numbers: &["1", "2", "3"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Railway Terrace",
        postcode: "4740",
        city: "Wattleford",
        cc: "AU",
        lon: 149.195,
        lat: -21.2,
        numbers: &["8", "9"],
    },
    // starts with its town's name, so is no village street of that town
    Street {
        class: "residential",
        length: 0.0,
        name: "Wattleford Vista",
        postcode: "4740",
        city: "Wattleford",
        cc: "AU",
        lon: 149.25,
        lat: -21.2,
        numbers: &["1", "2", "3"],
    },
    // its addresses name a suburb that is no place of its own
    Street {
        class: "residential",
        length: 0.0,
        name: "Waratah Avenue",
        postcode: "4740",
        city: "Kingsford",
        cc: "AU",
        lon: 149.21,
        lat: -21.2,
        numbers: &["3"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "King Street",
        postcode: "4510",
        city: "Burrum Flats",
        cc: "AU",
        lon: 150.4,
        lat: -22.5,
        numbers: &["119", "121"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        // a house in a town 41 km from the metropolitan locality stated is no good match there
        case("121 King Street, Ardmore", town("au-ardmore", false)),
        // the town written first, which the parse drops, not France's town of the last word
        case(
            "Burrum Flats, ul. Środkowa 181 Termy Colmar",
            town("au-burrum-flats", false),
        ),
        // an Australian unit written before its building's number still resolves the building
        case(
            "206 Banksia Street, Wattleford QLD 4740, Australia",
            at_house("Banksia Street", "206"),
        ),
        case(
            "71/206 Banksia Street, Wattleford QLD 4740, Australia",
            at_house("Banksia Street", "206"),
        ),
        case(
            "71/206 Banksia St, Wattleford QLD 4740",
            at_house("Banksia Street", "206"),
        ),
        // a unit the source holds as its own house keeps its point
        case(
            "8/206 Banksia Street, Wattleford QLD 4740, Australia",
            at_house("Banksia Street", "8/206"),
        ),
        case(
            "Panorama Place 3, Wattleford, QLD, 4740, Australia",
            at_house("Panorama Place", "3"),
        ),
        case(
            "Railway Terrace 9, Wattleford, QLD, 4740, Australia",
            at_house("Railway Terrace", "9"),
        ),
    ]
}

#[test]
fn australian_houses_are_suggested_with_state_codes_suburbs_and_units() {
    let at = |text: &str| index().suggest(text, 5, None, None, false);
    let near = |s: &opengeocoding::index::Suggestion, street_name, number| {
        metres((s.x, s.y), house(street(street_name), number)) < 5.0
    };
    for (typed, number, suffix, display) in [
        ("206 Banksia Street, 4740, Australia", "206", "", "206, "),
        (
            "206 BANKSIA STREET, WATTLEFORD QLD 4740, Australia",
            "206",
            "",
            "206, ",
        ),
        (
            "206 Banksia Street, Wattleford QLD 4740",
            "206",
            "",
            "206, ",
        ),
        (
            "71/206 Banksia St, Wattleford QLD 4740",
            "206",
            "71",
            "71/206, ",
        ),
        (
            "UNIT 71 206 BANKSIA STREET, WATTLEFORD QLD 4740, Australia",
            "206",
            "UNIT 71",
            "UNIT 71, 206, ",
        ),
        (
            "Unit 71, 206 Banksia Street, Wattleford, QLD",
            "206",
            "Unit 71",
            "Unit 71, 206, ",
        ),
        // a unit the source holds as its own house
        (
            "8/206 Banksia Street, Wattleford QLD 4740",
            "8/206",
            "",
            "8/206, ",
        ),
        ("3 Waratah Avenue, Kingsford", "3", "", "3, "),
        ("3 Waratah Avenue, Kingsford QLD 4740", "3", "", "3, "),
    ] {
        let found = at(typed);
        assert!(!found.is_empty(), "{typed}");
        let s = &found[0];
        assert_eq!(
            (s.kind, s.country_code.as_str()),
            ("address", "au"),
            "{typed}"
        );
        assert!(
            near(
                s,
                if typed.contains("Waratah") {
                    "Waratah Avenue"
                } else {
                    "Banksia Street"
                },
                number
            ),
            "{typed}"
        );
        assert_eq!(s.unverified_suffix, suffix, "{typed}");
        assert!(
            s.display_name.starts_with(display),
            "{typed}: {}",
            s.display_name
        );
    }
    // a slashed number is a unit only in Australia
    for typed in [
        "71/206 Banksia Street, Wattleford",
        "71/206 Banksia Street, Wattleford, United States",
    ] {
        assert!(at(typed).iter().all(|s| s.kind != "address"), "{typed}");
    }
}
