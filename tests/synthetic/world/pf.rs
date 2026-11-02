use super::*;
use crate::{Case, Resolver, at_house, case};

pub const PLACES: &[Place] = &[
    place!(
        "pf",
        "dependency",
        "",
        "PF",
        "",
        250_000,
        (-145.0, -15.0),
        "",
        "French Polynesia",
        &[("en", "French Polynesia")]
    ),
    place!(
        "pf-rivara",
        "county",
        "",
        "PF",
        "",
        5_000,
        (-145.0, -15.0),
        "pf",
        "Rivara",
        &[]
    ),
    place!(
        "pf-motuvale",
        "locality",
        "village",
        "PF",
        "",
        500,
        (-145.05, -15.0),
        "pf-rivara",
        "Motuvale",
        &[]
    ),
    place!(
        "pf-tamure",
        "county",
        "",
        "PF",
        "",
        4_000,
        (-144.0, -15.0),
        "pf",
        "Tamure",
        &[]
    ),
    place!(
        "pf-other-motuvale",
        "locality",
        "village",
        "PF",
        "",
        1_000,
        (-144.05, -15.0),
        "pf-tamure",
        "Motuvale",
        &[]
    ),
];

const LAGOON: Street = Street {
    class: "residential",
    length: 0.0,
    name: "Arati’a Lagoon",
    postcode: "",
    city: "Motuvale",
    cc: "PF",
    lon: -145.06,
    lat: -15.0,
    numbers: &["31", "6bis"],
};

pub const STREETS: &[Street] = &[
    LAGOON,
    Street {
        name: "Arati’a Tiare 7",
        lon: -145.07,
        numbers: &["5"],
        ..LAGOON
    },
    Street {
        name: "Arati’a Tiare 6",
        lon: -145.08,
        numbers: &["99"],
        ..LAGOON
    },
    Street {
        name: "Purumu Tiare 11a",
        lon: -145.09,
        numbers: &["2"],
        ..LAGOON
    },
    Street {
        name: "Purumu Tiare 11b",
        lon: -145.10,
        numbers: &["8"],
        ..LAGOON
    },
    Street {
        name: "Arati’a Coral",
        postcode: "98712",
        lon: -145.11,
        numbers: &["14"],
        ..LAGOON
    },
    Street {
        name: "Arati’a Outside",
        lon: -144.06,
        numbers: &["19"],
        ..LAGOON
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "31 Arati’a Lagoon, 98735 Rivara, French Polynesia",
            at_house("Arati’a Lagoon", "31"),
        ),
        case(
            "6 bis Arati’a Lagoon, 98735 Rivara, French Polynesia",
            at_house("Arati’a Lagoon", "6bis"),
        ),
        case(
            "5 Arati’a Tiare 7, 98735 Rivara, French Polynesia",
            at_house("Arati’a Tiare 7", "5"),
        ),
        case(
            "2 Purumu Tiare 11a, Rivara, French Polynesia",
            at_house("Purumu Tiare 11a", "2"),
        ),
    ]
}

pub async fn check_search(resolver: &Resolver) {
    let postcode = resolver
        .parse("98735 Rivara, French Polynesia")
        .await
        .unwrap();
    assert_eq!(postcode.get("postcode").map(String::as_str), Some("98735"));
    assert!(!postcode.contains_key("house_number"));
    for (query, number, road) in [
        (
            "31 Arati’a Lagoon, 98735 Rivara, French Polynesia",
            "31",
            "Arati’a Lagoon",
        ),
        (
            "5 Arati’a Tiare 7, 98735 Rivara, French Polynesia",
            "5",
            "Arati’a Tiare 7",
        ),
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(found.good_match, "{query}: {found:?}");
        let details = found.address_details.as_option().unwrap();
        assert_eq!(details.house_number, number, "{query}");
        assert_eq!(details.road, road, "{query}");
        assert_eq!(details.postcode, "98735", "{query}");
        assert_eq!(details.county, "Rivara", "{query}");
    }
    for query in [
        "19 Arati’a Outside, Rivara, French Polynesia",
        "99 Arati’a Tiare 7, Rivara, French Polynesia",
        "99 Arati’a Tiare 8, Rivara, French Polynesia",
        "5 Arati’a Tiare 6, Rivara, French Polynesia",
        "8 Purumu Tiare 11a, Rivara, French Polynesia",
        "999 Arati’a Lagoon, 98735 Rivara, French Polynesia",
        "31 Arati’a Lagoon, 98735 Rivara, Germany",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
        assert!(
            found
                .address_details
                .as_option()
                .is_none_or(|d| d.house_number.is_empty()),
            "{query}: {found:?}"
        );
    }
    let coarse = resolver
        .resolve("999 Arati’a Lagoon, 98712 Motuvale, French Polynesia")
        .await
        .unwrap();
    assert!(coarse.geo_coordinates.as_option().is_some(), "{coarse:?}");
    assert!(!coarse.good_match, "{coarse:?}");
    assert!(
        coarse
            .address_details
            .as_option()
            .unwrap()
            .house_number
            .is_empty(),
        "{coarse:?}"
    );
    let query = "14 Arati’a Coral, 98735 Rivara, French Polynesia";
    let found = resolver.resolve(query).await.unwrap();
    assert!(!found.good_match, "{query}: {found:?}");
    if let Some(details) = found
        .address_details
        .as_option()
        .filter(|d| !d.house_number.is_empty())
    {
        assert_eq!(details.postcode, "98712", "{query}: {found:?}");
    }
}
