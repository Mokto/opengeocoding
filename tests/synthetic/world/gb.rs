use super::*;
use crate::*;

pub const PLACES: &[Place] = &[
    place!(
        "gb",
        "country",
        "",
        "GB",
        "",
        68_000_000,
        (30.0, 10.0),
        "",
        "United Kingdom",
        &[("en", "United Kingdom")]
    ),
    place!(
        "gb-beacon",
        "locality",
        "town",
        "GB",
        "",
        20_000,
        (30.0, 10.0),
        "gb",
        "Beacon",
        &[
            ("en", "Beacon"),
            ("ja", "桜森市"),
            ("zh", "松林市"),
            ("ko", "솔빛도시"),
            ("tr", "İncikent"),
        ]
    ),
    place!(
        "gb-bramble",
        "locality",
        "town",
        "GB",
        "",
        20_000,
        (31.0, 11.0),
        "gb",
        "6th of Bramble",
        &[("en", "Sixth of Bramble"), ("fr", "Bramble6")]
    ),
    place!(
        "gb-orchard",
        "microhood",
        "",
        "GB",
        "",
        500,
        (30.02, 10.02),
        "gb-beacon",
        "Orchard",
        &[("en", "Orchard")],
    ),
    // a part of Beacon, whose houses are filed under Beacon
    place!(
        "gb-quayside",
        "neighborhood",
        "",
        "GB",
        "",
        6_000,
        (30.1, 10.1),
        "gb-beacon",
        "Quayside",
        &[("en", "Quayside")],
    ),
    place!(
        "gb-harbor-region",
        "region",
        "",
        "GB",
        "GB-WLS",
        100_000,
        (35.0, 12.0),
        "gb",
        "Harbor City",
        &[("en", "Harbor City")]
    ),
    place!(
        "gb-harbor-city",
        "locality",
        "city",
        "GB",
        "GB-WLS",
        100_000,
        (35.0, 12.01),
        "gb-harbor-region",
        "Harbor City",
        &[("en", "Harbor City")]
    ),
    place!(
        "gb-dockward",
        "locality",
        "village",
        "GB",
        "GB-WLS",
        100,
        (35.0, 12.0),
        "gb-harbor-region",
        "Dockward",
        &[("en", "Dockward")]
    ),
    place!(
        "gb-mailshire",
        "region",
        "",
        "GB",
        "GB-ENG",
        10_000,
        (32.0, 12.0),
        "gb",
        "Mailshire",
        &[("en", "Mailshire")]
    ),
    place!(
        "gb-posthaven-county",
        "county",
        "",
        "GB",
        "GB-ENG",
        1_000,
        (32.0, 12.0),
        "gb-mailshire",
        "Posthaven",
        &[("en", "Posthaven")]
    ),
    place!(
        "gb-oakmere",
        "locality",
        "village",
        "GB",
        "GB-ENG",
        100,
        (32.0, 12.0),
        "gb-posthaven-county",
        "Oakmere",
        &[("en", "Oakmere")]
    ),
    place!(
        "gb-posthaven",
        "locality",
        "town",
        "GB",
        "GB-ENG",
        1_000,
        (32.6, 12.0),
        "gb-posthaven-county",
        "Posthaven",
        &[("en", "Posthaven")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Quay Road",
        postcode: "",
        city: "",
        cc: "GB",
        lon: 35.001,
        lat: 12.0,
        numbers: &["61"],
    },
    POSTAL_STREET,
    Street {
        postcode: "ZZ1 1AB",
        city: "Farhaven",
        lon: 32.002,
        numbers: &["32"],
        ..POSTAL_STREET
    },
    Street {
        postcode: "ZZ1 1AC",
        city: "Amber Quay",
        lon: 32.003,
        numbers: &["33"],
        ..POSTAL_STREET
    },
    Street {
        postcode: "ZZ1 1AC",
        city: "Birch Reach",
        lon: 32.004,
        numbers: &["34"],
        ..POSTAL_STREET
    },
    Street {
        postcode: "",
        lon: 32.005,
        numbers: &["35"],
        ..POSTAL_STREET
    },
    Street {
        postcode: "",
        city: "Farhaven",
        lon: 32.006,
        numbers: &["36"],
        ..POSTAL_STREET
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "High Street",
        postcode: "AA1 1AA",
        city: "Beacon",
        cc: "GB",
        lon: 30.01,
        lat: 10.0,
        numbers: &["1", "2", "3"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![case("Orchard, United Kingdom", town("gb-orchard", true))]
}

/// AA1 1AA is filed under Beacon, whose part Quayside is not in its places' chain.
#[test]
fn a_whole_postcode_is_suggested_with_a_district_it_is_not_filed_under() {
    let index = index();
    for query in ["AA1 1AA, Quayside", "AA1 1AA, Quayside, United Kingdom"] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("postcode"), "{query}");
        assert_eq!(found[0].country_code, "gb", "{query}");
    }
    // a place in another country than the code's
    assert!(
        index
            .suggest("75460, Beacon", 5, None, None, false)
            .iter()
            .all(|s| s.kind != "postcode")
    );
}

/// A postcode and a local authority the data has only as a point near it, not in its chain.
#[test]
fn a_postcode_is_answered_with_an_area_near_it() {
    let index = index();
    let found = |q: &str| {
        index
            .gb_postcode_in_area(q)
            .map(|(hit, exact)| (index.render(&hit, true)["display_name"].clone(), exact))
    };
    let (name, exact) = found("AA1 1AA, Quayside, United Kingdom").unwrap();
    assert!(name.as_str().unwrap().starts_with("AA1 1AA, "), "{name}");
    assert!(exact);
    // Posthaven is a town 300 km away
    assert_eq!(found("AA1 1AA, Posthaven"), None);
    // a code the data lacks, at its sector ZZ1 1, is no match
    let (name, exact) = found("ZZ1 1ZZ, Oakmere").unwrap();
    assert!(name.as_str().unwrap().starts_with("ZZ1 1ZZ, "), "{name}");
    assert!(!exact);
    assert_eq!(found("ZZ1 2ZZ, Oakmere"), None);
}
