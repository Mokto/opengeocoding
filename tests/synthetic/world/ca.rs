use super::*;
use crate::*;
use opengeocoding::index::schema;

pub const PLACES: &[Place] = &[
    place!(
        "ca",
        "country",
        "",
        "CA",
        "",
        38_000_000,
        (-106.0, 56.0),
        "",
        "Canada",
        &[("en", "Canada")],
    ),
    place!(
        "ca-on",
        "region",
        "",
        "CA",
        "CA-ON",
        14_000_000,
        (-85.0, 50.0),
        "ca",
        "Ontario",
        &[("en", "Ontario")],
    ),
    place!(
        "ca-on-delhi",
        "locality",
        "town",
        "CA",
        "CA-ON",
        4_000,
        (-80.4953, 42.8532),
        "ca-on",
        "Delhi",
        &[("en", "Delhi")],
    ),
    place!(
        "ca-on-hamilton",
        "locality",
        "city",
        "CA",
        "CA-ON",
        570_000,
        (-79.87, 43.26),
        "ca-on",
        "Hamilton",
        &[("en", "Hamilton")],
    ),
    place!(
        "ca-qc",
        "region",
        "",
        "CA",
        "CA-QC",
        8_000_000,
        (-71.0, 53.0),
        "ca",
        "Quebec",
        &[("en", "Quebec")],
    ),
    place!(
        "ca-qc-harbour-on",
        "locality",
        "town",
        "CA",
        "CA-QC",
        4_000,
        (-71.2, 46.8),
        "ca-qc",
        "Harbour ON",
        &[("en", "Harbour ON")],
    ),
    // New Brunswick: a civil parish's area holds every house; its city and hamlets are points
    // under it, and its parent is a local government other than the city's
    place!(
        "ca-nb",
        "region",
        "",
        "CA",
        "CA-NB",
        800_000,
        (-66.0, 46.5),
        "ca",
        "New Brunswick",
        &[("en", "New Brunswick")],
    ),
    place!(
        "ca-nb-tidemouth-city",
        "county",
        "",
        "CA",
        "CA-NB",
        80_000,
        (-65.05, 46.0),
        "ca-nb",
        "City of Tidemouth",
        &[("en", "City of Tidemouth")],
    ),
    place!(
        "ca-nb-hillside",
        "county",
        "",
        "CA",
        "CA-NB",
        5_000,
        (-64.9, 46.0),
        "ca-nb",
        "Hillside Rural Community",
        &[("en", "Hillside Rural Community")],
    ),
    place!(
        "ca-nb-tidemouth-parish",
        "locality",
        "",
        "CA",
        "CA-NB",
        0,
        (-65.0, 46.0),
        "ca-nb-hillside",
        "Tidemouth District",
        &[("en", "Tidemouth District")],
    ),
    place!(
        "ca-nb-tidemouth",
        "locality",
        "city",
        "CA",
        "CA-NB",
        80_000,
        (-65.06, 46.0),
        "ca-nb-tidemouth-parish",
        "Tidemouth",
        &[("en", "Tidemouth")],
    ),
    place!(
        "ca-nb-cove",
        "locality",
        "hamlet",
        "CA",
        "CA-NB",
        300,
        (-64.9, 46.0),
        "ca-nb-tidemouth-parish",
        "Cove",
        &[("en", "Cove")],
    ),
    place!(
        "ca-nb-gap",
        "locality",
        "hamlet",
        "CA",
        "CA-NB",
        100,
        (-64.99, 46.0),
        "ca-nb-tidemouth-parish",
        "Gap",
        &[("en", "Gap")],
    ),
    // two cities that are points only, each under its own government: a house of one nearer the
    // other's point is filed under the other
    place!(
        "ca-nb-port-royal-city",
        "county",
        "",
        "CA",
        "CA-NB",
        20_000,
        (-65.0, 46.5),
        "ca-nb",
        "City of Port Royal",
        &[("en", "City of Port Royal")],
    ),
    place!(
        "ca-nb-pearl-city",
        "county",
        "",
        "CA",
        "CA-NB",
        20_000,
        (-64.94, 46.5),
        "ca-nb",
        "City of Pearl",
        &[("en", "City of Pearl")],
    ),
    place!(
        "ca-nb-port-royal",
        "locality",
        "city",
        "CA",
        "CA-NB",
        20_000,
        (-65.025, 46.5),
        "ca-nb-port-royal-city",
        "Port Royal",
        &[("en", "Port Royal")],
    ),
    place!(
        "ca-nb-pearl",
        "locality",
        "city",
        "CA",
        "CA-NB",
        20_000,
        (-64.95, 46.5),
        "ca-nb-pearl-city",
        "Pearl",
        &[("en", "Pearl")],
    ),
    // a town under the province alone, its point outside both governments' polygons
    place!(
        "ca-nb-bayside",
        "locality",
        "town",
        "CA",
        "CA-NB",
        3_000,
        (-64.98, 46.535),
        "ca-nb",
        "Bayside",
        &[("en", "Bayside")],
    ),
    // a town under the government, its point just outside its polygon
    place!(
        "ca-nb-harbourview",
        "locality",
        "town",
        "CA",
        "CA-NB",
        3_000,
        (-65.01, 46.535),
        "ca-nb-port-royal-city",
        "Harbourview",
        &[("en", "Harbourview")],
    ),
];

pub const POINT_ONLY: &[&str] = &[
    "ca-nb-harbourview",
    "ca-nb-bayside",
    "ca-nb-tidemouth",
    "ca-nb-cove",
    "ca-nb-gap",
    "ca-nb-port-royal",
    "ca-nb-pearl",
];

pub const AREAS: &[(&str, &str, bool, bool, f64, f64, f64)] = &[
    (
        "ca-nb-tidemouth-city",
        "county",
        true,
        true,
        -65.05,
        46.0,
        0.05,
    ),
    ("ca-nb-hillside", "county", true, true, -64.9, 46.0, 0.05),
    (
        "ca-nb-port-royal-city",
        "county",
        true,
        true,
        -65.0,
        46.5,
        0.03,
    ),
    ("ca-nb-pearl-city", "county", true, true, -64.94, 46.5, 0.03),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Maple Scope Road",
        postcode: "N4B 1A1",
        city: "Delhi",
        cc: "CA",
        lon: -80.49,
        lat: 42.8532,
        numbers: &["12", "12a", "12 1/2", "20-24", "24"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Birch Completion Road",
        postcode: "N4B 1A1",
        city: "Delhi",
        cc: "CA",
        lon: -80.49,
        lat: 42.8532,
        numbers: &["12", "12a"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Birch Completion Road",
        postcode: "G1A 0A2",
        city: "Harbour ON",
        cc: "CA",
        lon: -71.2,
        lat: 46.8,
        numbers: &["12"],
    },
    // a street the source names without the direction an address writes ("Thistle Bend Dr E")
    Street {
        class: "residential",
        length: 0.0,
        name: "Thistle Bend Drive",
        postcode: "N4B 1A3",
        city: "Delhi",
        cc: "CA",
        lon: -80.497,
        lat: 42.856,
        numbers: &["31", "716.0 E"],
    },
    // a source writing the postcode compact, under a street libpostal reads the number into
    Street {
        class: "residential",
        length: 0.0,
        name: "Avenue W South",
        postcode: "N4B1A6",
        city: "Delhi",
        cc: "CA",
        lon: -80.484,
        lat: 42.854,
        numbers: &["422.0 A"],
    },
    // in the city's local government, nearer the hamlet across its edge than the city
    Street {
        class: "residential",
        length: 0.0,
        name: "Harbour Road",
        postcode: "E1C 6T8",
        city: "Tidemouth",
        cc: "CA",
        lon: -65.005,
        lat: 46.0,
        numbers: &["113"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cove Road",
        postcode: "E1C 6T9",
        city: "Cove",
        cc: "CA",
        lon: -64.905,
        lat: 46.0,
        numbers: &["7"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Shore Lane",
        postcode: "E1C 6T3",
        city: "Harbourview",
        cc: "CA",
        lon: -65.012,
        lat: 46.528,
        numbers: &["4"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Bay Road",
        postcode: "E1C 6T4",
        city: "Bayside",
        cc: "CA",
        lon: -64.985,
        lat: 46.528,
        numbers: &["9"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Lady Lane",
        postcode: "E1C 6T6",
        city: "Port Royal",
        cc: "CA",
        lon: -64.975,
        lat: 46.5,
        numbers: &["51"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Pearl Street",
        postcode: "E1C 6T5",
        city: "Pearl",
        cc: "CA",
        lon: -64.945,
        lat: 46.5,
        numbers: &["3"],
    },
    // in the parish, between the local governments' polygons
    Street {
        class: "residential",
        length: 0.0,
        name: "Between Lane",
        postcode: "E1C 6T7",
        city: "Tidemouth",
        cc: "CA",
        lon: -64.975,
        lat: 46.0,
        numbers: &["5"],
    },
    // a possessive the source keeps and Canada Post's mailing form drops, and the other way
    Street {
        class: "residential",
        length: 0.0,
        name: "Robert's Road",
        postcode: "N4B 1A4",
        city: "Delhi",
        cc: "CA",
        lon: -80.493,
        lat: 42.859,
        numbers: &["57"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Dicks Square",
        postcode: "N4B 1A5",
        city: "Delhi",
        cc: "CA",
        lon: -80.488,
        lat: 42.851,
        numbers: &["10.0 B"],
    },
    // Overture's Canadian rows: the direction abbreviated, a suffixed civic number as a float
    Street {
        class: "residential",
        length: 0.0,
        name: "6e Rue O",
        postcode: "G1A 0A3",
        city: "Harbour ON",
        cc: "CA",
        lon: -71.205,
        lat: 46.8,
        numbers: &["870", "55.0 A", "29.0 1/2"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case("Delhi, Ontario", town("ca-on-delhi", true)),
        case("Delhi, ON, Canada", town("ca-on-delhi", true)),
        case("Delhi, Canada", town("ca-on-delhi", true)),
        case(
            "12 Maple Scope Road, Delhi, ON",
            at_house("Maple Scope Road", "12"),
        ),
        case(
            "12 Maple Scope Road, Delhi, Phone",
            at_house("Maple Scope Road", "12"),
        ),
        // a province code no region of New Zealand has: not New Zealand's street and town
        case("19 East Orchard Road, Harbourton, QC", Want::Abstain),
        case("19 East Orchard Road, Harbourton, qc", Want::Abstain),
        case("19 East Orchard Road, Harbourton, ON", Want::Abstain),
        case("19 East Orchard Road, Harbourton, QC, Phone", Want::Abstain),
    ]
}

#[test]
fn quebec_directions_and_float_civic_numbers_match_as_written() {
    let index = index();
    let road = street("6e Rue O");
    for (query, number) in [
        ("870 6e Rue Ouest, Harbour ON, Canada", "870"),
        ("870 6e Rue O, Harbour ON, Canada", "870"),
        ("55A 6e Rue Ouest, Harbour ON, Canada", "55.0 A"),
    ] {
        let found = index.suggest(query, 1, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(
            metres((found[0].x, found[0].y), house(road, number)) < 2.,
            "{query}"
        );
    }
    for query in [
        "55B 6e Rue O, Harbour ON, Canada",
        "870 6e Rue Est, Harbour ON, Canada",
    ] {
        assert!(
            !index
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

#[test]
fn canadian_unit_suggestions_identify_buildings_not_apartments_or_range_endpoints() {
    let index = index();
    for number in ["12", "12a"] {
        let query = format!("{number} Maple Scope Road, Delhi, ON N4B 1A1, Canada");
        let mut base = index.suggest(&query, 1, Some("ca"), None, false);
        assert_eq!(base.len(), 1, "building control: {query}");
        let base = base.remove(0);
        assert_eq!(base.kind, "address", "{query}");
        for unit in ["100", "A", "BSMT", "UPPR", "APT"] {
            let query = format!("{unit}-{number} Maple Scope Road, Delhi, ON N4B 1A1, Canada");
            for landmarks in [false, true] {
                let found = index.suggest(&query, 1, None, None, landmarks);
                assert_eq!(found.len(), 1, "{query}");
                let got = &found[0];
                assert_eq!((got.kind, got.country_code.as_str()), ("address", "ca"));
                assert_eq!(
                    (&got.id, &got.retrieval_id, got.x, got.y),
                    (&base.id, &base.retrieval_id, base.x, base.y),
                    "{query}"
                );
                assert_eq!(got.display_name, base.display_name);
                assert!(got.unverified_suffix.is_empty());
                assert!(metres((got.x, got.y), house(street("Maple Scope Road"), number)) < 2.);
            }
            assert!(index.suggest(&query, 1, Some("us"), None, false).is_empty());
            assert!(index.suggest(&query, 0, None, None, false).is_empty());
        }
    }
    for query in [
        "12 1/2 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        "Maple Scope Road 12 1/2, Delhi, ON N4B 1A1, Canada",
        "100-12 1/2 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
    ] {
        let found = index.suggest(query, 5, Some("ca"), None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(
            metres(
                (found[0].x, found[0].y),
                house(street("Maple Scope Road"), "12 1/2")
            ) < 2.,
            "{query}"
        );
    }
    // a fraction the street lacks is not its whole number's house
    for query in [
        "12 3/4 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        "24 1/2 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
    ] {
        assert!(
            !index
                .suggest(query, 5, Some("ca"), None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
    let literal = index.suggest(
        "20-24 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        1,
        None,
        None,
        false,
    );
    assert_eq!(literal[0].kind, "address");
    assert!(
        metres(
            (literal[0].x, literal[0].y),
            house(street("Maple Scope Road"), "20-24")
        ) < 2.
    );
    for query in [
        "1-999 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        "1-12 Maple Scope Road, Delhi, ON M5T 0A8, Canada",
        "1-12 Maple Scope Road, Delhi, ON N4B 1A1, Australia",
        "1-12 Maple Scope Road, Delhi, ON N4B 1A1, Poland",
        "1-12 Maple Scope Road, Missingville, ON N4B 1A1, Canada",
        "1-12 Missing Road, Delhi, ON N4B 1A1, Canada",
    ] {
        assert!(
            !index
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

/// A bare number written as a part of its own after the street (OSM's `addr:district`, or a unit)
/// garbles libpostal's road; the building with the civic number as written answers. A possessive
/// written the other way than the source's is found only with the house's postcode, and a postcode
/// written spaced confirms a house whose source writes it compact.
pub async fn check_search(resolver: &opengeocoding::resolver::Resolver) {
    let query = "422A AVENUE W S, DELHI, ON N4B 1A6, Canada";
    let got = resolver.resolve(query).await.unwrap();
    assert!(got.good_match, "{query}: {got:?}");
    let at = got.geo_coordinates.as_option().unwrap();
    assert!(
        metres(
            (at.lon as f64, at.lat as f64),
            house(street("Avenue W South"), "422.0 A")
        ) < 5.0,
        "{query}: {got:?}"
    );
    for query in [
        "422B AVENUE W S, DELHI, ON N4B 1A6, Canada",
        "422 AVENUE W S, DELHI, ON N4B 1A6, Canada",
        "422A AVENUE W S, DELHI, ON N4B 1A5, Canada",
    ] {
        let got = resolver.resolve(query).await.unwrap();
        let house = got
            .address_details
            .as_option()
            .map_or("", |d| d.house_number.as_str());
        assert!(!got.good_match && house.is_empty(), "{query}: {got:?}");
    }
    for (query, road, number) in POSSESSIVE {
        let got = resolver.resolve(query).await.unwrap();
        assert!(got.good_match, "{query}: {got:?}");
        let at = got.geo_coordinates.as_option().unwrap();
        assert!(
            metres((at.lon as f64, at.lat as f64), house(street(road), number)) < 5.0,
            "{query}: {got:?}"
        );
    }
    for query in NOT_POSSESSIVE {
        let got = resolver.resolve(query).await.unwrap();
        let house = got
            .address_details
            .as_option()
            .map_or("", |d| d.house_number.as_str());
        assert!(!got.good_match && house.is_empty(), "{query}: {got:?}");
    }
    let road = street("Maple Scope Road");
    for (query, number) in [
        ("12 Maple Scope Road, 2, Delhi, ON, Canada", "12"),
        ("Maple Scope Road 12, 2, Delhi, Ontario, Canada", "12"),
        ("12a Maple Scope Road, 4, Delhi, Canada", "12a"),
        ("12 1/2 Maple Scope Road, 2, Delhi, Canada", "12 1/2"),
        ("20-24 Maple Scope Road, 2, Delhi, Canada", "20-24"),
    ] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(got.good_match, "{query}: {got:?}");
        let details = got.address_details.as_option().unwrap();
        assert_eq!(details.country_code, "ca", "{query}");
        assert_eq!(
            schema::number_key(&details.house_number),
            schema::number_key(number),
            "{query}: {got:?}"
        );
        let at = got.geo_coordinates.as_option().unwrap();
        assert!(
            metres((at.lon as f64, at.lat as f64), house(road, number)) < 5.0,
            "{query}: {got:?}"
        );
    }
    for (query, number) in [
        (
            "12 Maple Scope Road, Delhi, Subd. B, ON N4B 1A1, Canada",
            "12",
        ),
        (
            "12a Maple Scope Road, Delhi, Part 1, ON N4B 1A1, Canada",
            "12a",
        ),
    ] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(got.good_match, "{query}: {got:?}");
        let at = got.geo_coordinates.as_option().unwrap();
        assert!(
            metres((at.lon as f64, at.lat as f64), house(road, number)) < 5.0,
            "{query}: {got:?}"
        );
    }
    let road = street("Thistle Bend Drive");
    for (query, number) in [
        ("31 Thistle Bend Dr E, Delhi, ON N4B 1A3, Canada", "31"),
        ("31 North Thistle Bend Dr, Delhi, ON N4B 1A3, Canada", "31"),
        (
            "716E Thistle Bend Dr E, Delhi, ON N4B 1A3, Canada",
            "716.0 E",
        ),
    ] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(got.good_match, "{query}: {got:?}");
        let at = got.geo_coordinates.as_option().unwrap();
        assert!(
            metres((at.lon as f64, at.lat as f64), house(road, number)) < 5.0,
            "{query}: {got:?}"
        );
    }
    for query in [
        // the direction left out needs the house's own postcode, and its number and suffix
        "31 Thistle Bend Dr E, Delhi, ON N4B 9Z9, Canada",
        "31 Thistle Bend Dr E, Delhi, ON, Canada",
        "33 Thistle Bend Dr E, Delhi, ON N4B 1A3, Canada",
        "716 Thistle Bend Dr E, Delhi, ON N4B 1A3, Canada",
        // the source's street holds the opposite direction
        "870 6e Rue Est, Harbour ON, QC G1A 0A3, Canada",
        // a subdivision designator leaves the house's number, town and postcode to match
        "13 Maple Scope Road, Delhi, Subd. B, ON N4B 1A1, Canada",
        "12b Maple Scope Road, Delhi, Subd. B, ON N4B 1A1, Canada",
        "12 Maple Scope Road, Harbour ON, Subd. B, QC, Canada",
        "12 Maple Scope Road, Delhi, Subd. B, ON M5T 0A8, Canada",
        // a suffix or number the street lacks: never its plain or other-suffixed neighbour
        "12b Maple Scope Road, 2, Delhi, Canada",
        "13 Maple Scope Road, 2, Delhi, Canada",
        // a bare CA is also California
        "12 Maple Scope Road, 2, Delhi, ON, CA",
        // the stated postcode, province and town, or country, are another place's
        "12 Maple Scope Road, 2, Delhi, ON M5T 0A8, Canada",
        "12 Maple Scope Road, 2, Harbour ON, QC, Canada",
        "12 Maple Scope Road, 2, Delhi, ON, Australia",
    ] {
        let got = resolver.resolve(query).await.unwrap();
        let house = got
            .address_details
            .as_option()
            .map_or("", |d| d.house_number.as_str());
        assert!(!got.good_match && house.is_empty(), "{query}: {got:?}");
    }
}

#[test]
fn a_bare_number_part_suggests_the_building_with_the_civic_number_as_written() {
    let index = index();
    let road = street("Maple Scope Road");
    for (query, number) in [
        ("12 Maple Scope Road, 2, Delhi, ON N4B 1A1, Canada", "12"),
        ("12a Maple Scope Road, 4, Delhi, ON N4B 1A1, Canada", "12a"),
        (
            "20-24 Maple Scope Road, 2, Delhi, ON N4B 1A1, Canada",
            "20-24",
        ),
    ] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert_eq!(found[0].country_code, "ca");
        assert!(
            metres((found[0].x, found[0].y), house(road, number)) < 2.,
            "{query}"
        );
        assert!(index.suggest(query, 5, Some("us"), None, false).is_empty());
    }
    for query in [
        "12b Maple Scope Road, 2, Delhi, ON N4B 1A1, Canada",
        "13 Maple Scope Road, 2, Delhi, ON N4B 1A1, Canada",
        "12 Maple Scope Road, 2, Delhi, ON N4B 1A1, CA",
        "12 Maple Scope Road, 2, Delhi, ON M5T 0A8, Canada",
        "12 Maple Scope Road, 2, Delhi, ON N4B 1A1, Australia",
    ] {
        assert!(
            !index
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

#[test]
fn a_street_direction_the_source_leaves_out_is_suggested_only_with_the_house_postcode() {
    let index = index();
    let road = street("Thistle Bend Drive");
    for (query, number) in [
        ("31 Thistle Bend Dr E, Delhi, ON N4B 1A3, Canada", "31"),
        ("31 North Thistle Bend Dr, Delhi, ON N4B 1A3, Canada", "31"),
        (
            "716E Thistle Bend Dr E, Delhi, ON N4B 1A3, Canada",
            "716.0 E",
        ),
    ] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(
            metres((found[0].x, found[0].y), house(road, number)) < 2.,
            "{query}"
        );
        assert!(index.suggest(query, 5, Some("us"), None, false).is_empty());
    }
    for query in [
        "31 Thistle Bend Dr E, Delhi, ON N4B 9Z9, Canada",
        "31 Thistle Bend Dr E, Delhi, ON, Canada",
        "33 Thistle Bend Dr E, Delhi, ON N4B 1A3, Canada",
        "716 Thistle Bend Dr E, Delhi, ON N4B 1A3, Canada",
        "870 6e Rue Est, Harbour ON, QC G1A 0A3, Canada",
    ] {
        assert!(
            !index
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

#[test]
fn a_subdivision_designator_part_suggests_the_house_the_name_before_it_qualifies() {
    let index = index();
    let road = street("Maple Scope Road");
    for (query, number) in [
        (
            "12 Maple Scope Road, Delhi, Subd. B, ON N4B 1A1, Canada",
            "12",
        ),
        (
            "12a Maple Scope Road, Delhi, Part 1, ON N4B 1A1, Canada",
            "12a",
        ),
    ] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(
            metres((found[0].x, found[0].y), house(road, number)) < 2.,
            "{query}"
        );
    }
    for query in [
        "13 Maple Scope Road, Delhi, Subd. B, ON N4B 1A1, Canada",
        "12b Maple Scope Road, Delhi, Subd. B, ON N4B 1A1, Canada",
        "12 Maple Scope Road, Delhi, Subd. B, ON M5T 0A8, Canada",
        "12 Maple Scope Road, Delhi, Subd. B, ON N4B 1A1, CA",
        "12 Maple Scope Road, Delhi, Subd. B, ON N4B 1A1, Australia",
    ] {
        assert!(
            !index
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

const POSSESSIVE: [(&str, &str, &str); 3] = [
    (
        "57 Roberts Rd, Delhi, ON N4B 1A4, Canada",
        "Robert's Road",
        "57",
    ),
    (
        "57 ROBERT’S ROAD, DELHI, ON N4B 1A4, Canada",
        "Robert's Road",
        "57",
    ),
    (
        "10B Dick's Sq, Delhi, ON N4B 1A5, Canada",
        "Dicks Square",
        "10.0 B",
    ),
];

const NOT_POSSESSIVE: [&str; 4] = [
    // another postcode, a number or a suffix the street lacks
    "57 Roberts Rd, Delhi, ON N4B 1A5, Canada",
    "59 Roberts Rd, Delhi, ON N4B 1A4, Canada",
    "10 Dick's Sq, Delhi, ON N4B 1A5, Canada",
    "10C Dick's Sq, Delhi, ON N4B 1A5, Canada",
];

#[test]
fn a_possessive_written_the_other_way_is_suggested_only_with_the_house_postcode() {
    let index = index();
    for (query, road, number) in POSSESSIVE {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(
            metres((found[0].x, found[0].y), house(street(road), number)) < 2.,
            "{query}"
        );
        assert!(index.suggest(query, 5, Some("us"), None, false).is_empty());
    }
    for query in NOT_POSSESSIVE {
        assert!(
            !index
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

#[test]
fn a_canadian_house_names_the_local_government_holding_it_and_its_nearest_settlement() {
    for (lat, lon, number, key, settlement, county) in [
        (
            46.5,
            -64.975,
            "51",
            "city",
            "Port Royal",
            "City of Port Royal",
        ),
        (
            46.528,
            -64.985,
            "9",
            "city",
            "Port Royal",
            "City of Port Royal",
        ),
        (
            46.528,
            -65.012,
            "4",
            "town",
            "Harbourview",
            "City of Port Royal",
        ),
        (46.5, -64.945, "3", "city", "Pearl", "City of Pearl"),
        (
            46.0,
            -65.005,
            "113",
            "city",
            "Tidemouth",
            "City of Tidemouth",
        ),
        (
            46.0,
            -64.905,
            "7",
            "village",
            "Cove",
            "Hillside Rural Community",
        ),
        // no local government's polygon holds it: the parish and its parent stay
        (
            46.0,
            -64.975,
            "5",
            "village",
            "Tidemouth District",
            "Hillside Rural Community",
        ),
    ] {
        let found = index().reverse_search(lat, lon, 2.0, 1);
        let (layer, hit, _) = &found[0];
        assert_eq!(*layer, "address", "{number}");
        let address = &hit["address"];
        assert_eq!(address["house_number"], number);
        assert_eq!(address[key], settlement, "{number}: {address}");
        assert_eq!(address["county"], county, "{number}: {address}");
        let named = ["city", "town", "village"]
            .iter()
            .filter(|k| address.get(**k).is_some())
            .count();
        assert_eq!(named, 1, "{number}: {address}");
        assert_eq!(address["ISO3166-2-lvl4"], "CA-NB");
    }
}

#[test]
fn a_place_the_house_is_not_filed_under_is_passed_over_for_its_full_postcode() {
    let index = index();
    let road = street("Maple Scope Road");
    for query in [
        "12 Maple Scope Road, Hamilton, ON N4B 1A1, Canada",
        "12 Maple Scope Road, Harbour ON, Hamilton, ON N4B 1A1, Canada",
    ] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(
            metres((found[0].x, found[0].y), house(road, "12")) < 2.,
            "{query}"
        );
    }
    for query in [
        // another postcode, a number or suffix the street lacks, a bare CA, another country
        "12 Maple Scope Road, Hamilton, ON N4B 1A2, Canada",
        "13 Maple Scope Road, Hamilton, ON N4B 1A1, Canada",
        "12b Maple Scope Road, Hamilton, ON N4B 1A1, Canada",
        "12 Maple Scope Road, Hamilton, ON N4B 1A1, CA",
        "12 Maple Scope Road, Hamilton, ON N4B 1A1, Australia",
        // a place no data names, or another province
        "12 Maple Scope Road, Missingville, ON N4B 1A1, Canada",
        "12 Maple Scope Road, Hamilton, Quebec, N4B 1A1, Canada",
    ] {
        assert!(
            !index
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}
