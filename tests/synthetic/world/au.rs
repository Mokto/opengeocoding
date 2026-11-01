use super::*;
use crate::*;

pub const POINT_ONLY: &[&str] = &["au-close-village", "au-unbounded-metro"];

// Both copies must stay in one 0.1° street cell, or they are separate indexed houses.
pub const FEATURES: &[Feature] = &[
    Feature {
        shape: Shape::Node,
        lon: 150.00507,
        lat: -22.5,
        tags: &[
            ("addr:country", "AU"),
            ("addr:street", "Boundary Walk"),
            ("addr:housenumber", "12"),
            ("addr:postcode", "4511"),
            ("addr:city", "Posthaven"),
        ],
    },
    Feature {
        shape: Shape::Node,
        lon: 150.0032,
        lat: -22.5,
        tags: &[
            ("addr:country", "AU"),
            ("addr:street", "Conflict Walk"),
            ("addr:housenumber", "33"),
            ("addr:postcode", "4513"),
            ("addr:city", "Farhaven"),
        ],
    },
];

pub const PLACES: &[Place] = &[
    place!(
        "au-unbounded-metro",
        "locality",
        "city",
        "AU",
        "AU-QLD",
        100_000,
        (152.0, -22.5),
        "au-qld",
        "Unbounded",
        &[]
    ),
    place!(
        "au-unbounded-central",
        "macrohood",
        "",
        "AU",
        "AU-QLD",
        0,
        (152.0, -22.5),
        "au-qld",
        "Unbounded Central",
        &[]
    ),
    place!(
        "au-reconciled-metro",
        "locality",
        "city",
        "AU",
        "AU-SA",
        100_000,
        (151.0, -22.5),
        "au-sa",
        "Shorebay",
        &[]
    ),
    place!(
        "au-reconciled-suburb",
        "macrohood",
        "",
        "AU",
        "AU-SA",
        0,
        (151.0, -22.5),
        "au-reconciled-metro",
        "Harbour Quay",
        &[]
    ),
    place!(
        "au-coast",
        "macrohood",
        "",
        "AU",
        "AU-QLD",
        0,
        (150.0, -22.5),
        "au-ardmore",
        "Copper Coast",
        &[]
    ),
    place!(
        "au-fern-quay",
        "macrohood",
        "",
        "AU",
        "AU-QLD",
        0,
        (149.99, -22.5),
        "au-coast",
        "Fern Quay",
        &[]
    ),
    place!(
        "au-wattle-quay",
        "macrohood",
        "",
        "AU",
        "AU-QLD",
        0,
        (150.01, -22.5),
        "au-coast",
        "Wattle Quay",
        &[]
    ),
    place!(
        "au-close-village",
        "locality",
        "village",
        "AU",
        "AU-QLD",
        100,
        (150.0002, -22.5),
        "au-qld",
        "Nearbank",
        &[]
    ),
    place!(
        "au-foreign-suburb",
        "macrohood",
        "",
        "NZ",
        "NZ-AUK",
        0,
        (150.00495, -22.5),
        "nz-harbourton",
        "Foreign Quay",
        &[]
    ),
    place!(
        "au-other-region-suburb",
        "macrohood",
        "",
        "AU",
        "AU-SA",
        0,
        (150.00495, -22.5),
        "au-sa",
        "Other Quay",
        &[]
    ),
    place!(
        "au-overlap-a",
        "macrohood",
        "",
        "AU",
        "AU-QLD",
        0,
        (150.05, -22.5),
        "au-coast",
        "First Quay",
        &[]
    ),
    place!(
        "au-overlap-b",
        "macrohood",
        "",
        "AU",
        "AU-QLD",
        0,
        (150.05, -22.5),
        "au-coast",
        "Second Quay",
        &[]
    ),
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
        &[("en", "Ardmore"), ("alt_name", "Greater Ardmore")]
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
    place!(
        "au-ardmore-municipality",
        "county",
        "",
        "AU",
        "AU-QLD",
        0,
        (149.8, -22.5),
        "au-qld",
        "City of Ardmore",
        &[("en", "Ardmore")]
    ),
    place!(
        "au-fenbank",
        "county",
        "",
        "AU",
        "AU-QLD",
        0,
        (150.4, -22.5),
        "au-qld",
        "City of Fenbank",
        &[("en", "Fenbank")]
    ),
    place!(
        "au-silverbank",
        "locality",
        "town",
        "AU",
        "AU-QLD",
        10_000,
        (150.25, -22.5),
        "au-ardmore",
        "Oak Point",
        &[("en", "Oak Point")]
    ),
    place!(
        "au-westhaven",
        "locality",
        "",
        "AU",
        "AU-QLD",
        5_000,
        (149.6, -22.5),
        "au-ardmore",
        "Westhaven",
        &[("en", "Westhaven")]
    ),
];

pub const AREAS: &[(&str, &str, bool, bool, f64, f64, f64)] = &[
    ("au-qld", "region", true, true, 152.0, -22.5, 0.1),
    (
        "au-unbounded-central",
        "macrohood",
        true,
        true,
        152.0,
        -22.5,
        0.01,
    ),
    ("au-qld", "region", true, true, 151.0, -22.5, 0.1),
    (
        "au-reconciled-suburb",
        "macrohood",
        true,
        true,
        151.0,
        -22.5,
        0.01,
    ),
    ("au-qld", "region", true, true, 150.0, -22.5, 0.1),
    ("au-coast", "macrohood", true, true, 150.0, -22.5, 0.06),
    (
        "au-fern-quay",
        "macrohood",
        true,
        true,
        149.99,
        -22.5,
        0.015,
    ),
    (
        "au-wattle-quay",
        "macrohood",
        true,
        true,
        150.01,
        -22.5,
        0.005,
    ),
    (
        "au-foreign-suburb",
        "macrohood",
        true,
        true,
        150.00495,
        -22.5,
        0.0001,
    ),
    (
        "au-other-region-suburb",
        "macrohood",
        true,
        true,
        150.00495,
        -22.5,
        0.0001,
    ),
    (
        "au-overlap-a",
        "macrohood",
        true,
        true,
        150.05,
        -22.5,
        0.005,
    ),
    (
        "au-overlap-b",
        "macrohood",
        true,
        true,
        150.05,
        -22.5,
        0.005,
    ),
    (
        "au-ardmore-municipality",
        "county",
        true,
        true,
        149.8,
        -22.5,
        0.4,
    ),
    ("au-fenbank", "county", true, true, 150.4, -22.5, 0.2),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Conflict Walk",
        postcode: "4513",
        city: "Posthaven",
        cc: "AU",
        lon: 150.003,
        lat: -22.5,
        numbers: &["33"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Repeat Walk",
        postcode: "4511",
        city: "Posthaven",
        cc: "AU",
        lon: 150.0061,
        lat: -22.5,
        numbers: &["31"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Repeat Walk",
        postcode: "4511",
        city: "Posthaven",
        cc: "AU",
        lon: 150.0065,
        lat: -22.5,
        numbers: &["31"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Remnant Walk",
        postcode: "4514",
        city: "Posthaven",
        cc: "AU",
        lon: 152.0,
        lat: -22.5,
        numbers: &["19"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Reconciled Walk",
        postcode: "4513",
        city: "Posthaven",
        cc: "AU",
        lon: 151.0,
        lat: -22.5,
        numbers: &["18"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Boundary Walk",
        postcode: "4511",
        city: "Posthaven",
        cc: "AU",
        lon: 150.00495,
        lat: -22.5,
        numbers: &["12"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Boundary Walk",
        postcode: "4511",
        city: "Posthaven",
        cc: "AU",
        lon: 150.00505,
        lat: -22.5,
        numbers: &["14"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Overlap Walk",
        postcode: "4511",
        city: "Posthaven",
        cc: "AU",
        lon: 150.05,
        lat: -22.5,
        numbers: &["16"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Regent Street",
        postcode: "4512",
        city: "Oak Point",
        cc: "AU",
        lon: 150.25,
        lat: -22.5,
        numbers: &["119", "121"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Harbour Crescent",
        postcode: "4501",
        city: "Westhaven",
        cc: "AU",
        lon: 149.6,
        lat: -22.5,
        numbers: &["119", "121"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Coastal Road",
        postcode: "4512",
        city: "Ardmore",
        cc: "AU",
        lon: 150.26,
        lat: -22.5,
        numbers: &["119", "121"],
    },
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
        case(
            "12 Boundary Walk, Posthaven 4511, Australia",
            at_house("Boundary Walk", "12"),
        ),
        case("121 Regent Street, Ardmore", town("au-ardmore", false)),
        case(
            "121 Regent Street, Ardmore, Australia",
            town("au-ardmore", false),
        ),
        case(
            "121 Regent Street, Ardmore QLD 4599, Australia",
            town("au-ardmore", false),
        ),
        case(
            "121 Regent Street, Ardmore QLD 4512, Australia",
            at_house("Regent Street", "121"),
        ),
        case(
            "121 Regent Street, Oak Point, Australia",
            at_house("Regent Street", "121"),
        ),
        case(
            "121 Regent Street, Oak Point, Ardmore, Australia",
            at_house("Regent Street", "121"),
        ),
        case(
            "121 Regent Street, Greater Ardmore, Australia",
            at_house("Regent Street", "121"),
        ),
        case(
            "121 Harbour Crescent, Ardmore, Australia",
            at_house("Harbour Crescent", "121"),
        ),
        case(
            "121 Coastal Road, Ardmore, Australia",
            at_house("Coastal Road", "121"),
        ),
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

#[test]
fn australian_suburbs_follow_reconciled_region_parents() {
    let found = index().reverse_search(-22.5, 151.0, 2.0, 1);
    let address = &found[0].1["address"];
    assert_eq!(address["house_number"], "18");
    assert_eq!(address["suburb"], "Harbour Quay");
    assert_eq!(address["city"], "Shorebay");
    assert_eq!(address["ISO3166-2-lvl4"], "AU-QLD");
}

#[test]
fn australian_reverse_houses_keep_their_containing_suburb_and_metro() {
    let lat: f64 = -22.5;
    for (lon, number, suburb) in [
        (150.00495, "12", "Fern Quay"),
        (150.00505, "14", "Wattle Quay"),
        (150.0063, "31", "Wattle Quay"),
    ] {
        let found = index().reverse_search(lat, lon, 2.0, 1);
        assert_eq!(found.len(), 1);
        let (layer, hit, _) = &found[0];
        assert_eq!(*layer, "address", "{lon}: {number}");
        assert_eq!(hit["address"]["house_number"], number);
        assert_eq!(hit["address"]["suburb"], suburb);
        assert_eq!(hit["address"]["city"], "Ardmore");
        assert_eq!(hit["address"]["postal_town"], "Posthaven");
        assert_eq!(hit["address"]["country_code"], "au");
        assert_eq!(hit["address"]["ISO3166-2-lvl4"], "AU-QLD");
        let point = (
            hit["lon"].as_str().unwrap().parse().unwrap(),
            hit["lat"].as_str().unwrap().parse().unwrap(),
        );
        assert!(metres(point, (lon, lat)) < 1.0);
    }
    let conflict = index().reverse_search(lat, 150.003, 2.0, 1);
    let address = &conflict[0].1["address"];
    assert_eq!(address["house_number"], "33");
    assert_eq!(address["city"], "Ardmore");
    assert_eq!(address["suburb"], "Fern Quay");
    assert!(address.get("postal_town").is_none());

    let offset_lon = 150.00495 + 25.0 / (111_195.0 * lat.to_radians().cos());
    let offset = index().reverse_search(lat, offset_lon, 30.0, 1);
    assert_eq!(offset[0].1["address"]["house_number"], "14");
    assert_eq!(offset[0].1["address"]["suburb"], "Wattle Quay");

    let unbounded = index().reverse_search(lat, 152.0, 2.0, 1);
    assert_eq!(unbounded[0].1["address"]["house_number"], "19");
    assert_eq!(unbounded[0].1["address"]["city"], "Unbounded");
    assert!(unbounded[0].1["address"].get("suburb").is_none());

    let overlap = index().reverse_search(lat, 150.05, 2.0, 1);
    assert_eq!(overlap[0].1["address"]["house_number"], "16");
    assert_eq!(overlap[0].1["address"]["city"], "Ardmore");
    assert!(overlap[0].1["address"].get("suburb").is_none());
    let control = index().reverse_search(-21.2, 149.2, 2.0, 1);
    assert_eq!(control[0].1["address"]["house_number"], "206");
    assert_eq!(control[0].1["address"]["town"], "Wattleford");
}
