use super::*;
use crate::{Case, Want, at_house, case, index, metres, street};

pub const PLACES: &[Place] = &[
    place!(
        "lv",
        "country",
        "",
        "LV",
        "",
        1_900_000,
        (120.0, -45.0),
        "",
        "Latvija",
        &[("lv", "Latvija"), ("en", "Latvia")]
    ),
    place!(
        "lv-liepava-region",
        "region",
        "",
        "LV",
        "LV-001",
        20_000,
        (120.0, -45.2),
        "lv",
        "Liepavas novads",
        &[("lv", "Liepavas novads")]
    ),
    place!(
        "lv-liepava",
        "locality",
        "town",
        "LV",
        "LV-001",
        5_000,
        (120.0, -45.0),
        "lv-liepava-region",
        "Liepava",
        &[("lv", "Liepava")]
    ),
];

pub const STREETS: &[Street] = &[Street {
    class: "residential",
    length: 0.0,
    name: "Birzu iela",
    postcode: "LV-1234",
    city: "Liepava",
    cc: "LV",
    lon: 120.0,
    lat: -45.0,
    numbers: &["12", "12 k-3", "12A", "12A k-37"],
}];

const HOUSES: &[(&str, &str)] = &[
    (
        "Birzu iela 12 k-3, Liepava, Liepavas nov., LV-1234, Latvia",
        "12 k-3",
    ),
    (
        "12 k-3 Birzu iela, Liepavas novads, Liepava, LV-1234, Latvia",
        "12 k-3",
    ),
    (
        "Birzu iela 12A k-37, Liepavas novads, Liepava, LV-1234, Latvia",
        "12A k-37",
    ),
    (
        "12A k-37 Birzu iela, Liepava, Liepavas novads, Latvia",
        "12A k-37",
    ),
    (
        "Birzu iela 12, Liepavas novads, Liepava, LV-1234, Latvia",
        "12",
    ),
];

pub fn cases() -> Vec<Case> {
    HOUSES
        .iter()
        .map(|&(query, n)| case(query, at_house("Birzu iela", n)))
        .chain([
            case(
                "Liepava Municipality, Latvia",
                Want::Named("lv", "", &["LV-001"], true),
            ),
            case(
                "LIEPAVA MUNICIPALITY, LATVIJA",
                Want::Named("lv", "", &["LV-001"], true),
            ),
        ])
        .collect()
}

#[test]
fn latvian_buildings_keep_the_whole_number_in_either_address_order() {
    for &(query, number) in HOUSES {
        let found = index().suggest(query, 5, Some("LV"), None, false);
        let first = found
            .first()
            .unwrap_or_else(|| panic!("{query}: no suggestion"));
        assert_eq!(first.kind, "address", "{query}");
        let answer = index().suggested_house(first).unwrap();
        assert_eq!(
            answer["address"]["house_number"],
            number.to_lowercase(),
            "{query}"
        );
        assert!(
            metres((first.x, first.y), house(street("Birzu iela"), number)) < 5.0,
            "{query}"
        );
    }
    for query in [
        "Birzu iela 12 k-99, Liepava, Liepavas novads, Latvia",
        "Birzu iela 12A k-99, Liepava, Liepavas novads, Latvia",
        "Birzu iela 12 k-3, Liepava, Ozolu novads, Latvia",
    ] {
        let found = index().suggest(query, 5, Some("LV"), None, false);
        assert!(found.iter().all(|s| s.kind != "address"), "{query}");
    }
}

#[test]
fn an_english_municipality_is_the_region_not_its_town() {
    for query in [
        "Liepava Municipality",
        "Liepava Municipality, Latvia",
        "LIEPAVA MUNICIPALITY, LATVIJA",
    ] {
        let found = index().suggest(query, 5, None, None, false);
        let first = found
            .first()
            .unwrap_or_else(|| panic!("{query}: no suggestion"));
        assert_eq!(first.kind, "region", "{query}");
        assert_eq!(first.country_code, "lv", "{query}");
        assert!(metres((first.x, first.y), (120.0, -45.2)) < 5.0, "{query}");
    }
    for (query, cc) in [
        ("Liepava Municipality, Lithuania", None),
        ("Liepava Municipality, Latvia", Some("LT")),
        ("Liepava Municipality, Lithuania, Latvia", None),
    ] {
        let found = index().suggest(query, 5, cc, None, false);
        assert!(
            found
                .iter()
                .all(|s| s.country_code != "lv" || s.kind != "region"),
            "{query}"
        );
    }
}

pub async fn check_search(resolver: &crate::Resolver) {
    for &(query, number) in HOUSES {
        let got = resolver.resolve(query).await.unwrap();
        assert!(got.good_match, "{query}: {got:?}");
        assert_eq!(
            got.address_details.as_option().unwrap().house_number,
            number.to_lowercase(),
            "{query}"
        );
    }
    for query in [
        "Birzu iela 12 k-99, Liepava, Liepavas novads, Latvia",
        "Birzu iela 12A k-99, Liepava, Liepavas novads, Latvia",
        "Birzu iela 12 k-3, Liepava, Ozolu novads, Latvia",
        "Birzu iela 12 k-3 k-4, Liepava, Liepavas novads, Latvia",
        "Liepava Municipality, Lithuania",
    ] {
        let got = resolver.resolve(query).await.unwrap();
        assert!(!got.good_match, "{query}: {got:?}");
    }
}
