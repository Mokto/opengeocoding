use super::*;
use crate::*;

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
    let known_failure = "index: completion does not join a spaced fractional civic number";
    for query in [
        "12 1/2 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
        "100-12 1/2 Maple Scope Road, Delhi, ON N4B 1A1, Canada",
    ] {
        assert!(
            index.suggest(query, 5, Some("ca"), None, false).is_empty(),
            "fixed, remove its known_failure ({known_failure}): {query}"
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
