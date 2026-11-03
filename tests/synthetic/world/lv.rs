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
    place!(
        "lv-liepu",
        "region",
        "",
        "LV",
        "LV-002",
        10_000,
        (121.0, -45.0),
        "lv",
        "Liepu novads",
        &[("lv", "Liepu novads")]
    ),
    place!(
        "lv-ozolu",
        "region",
        "",
        "LV",
        "LV-003",
        10_000,
        (122.0, -45.0),
        "lv",
        "Ozolu novads",
        &[("lv", "Ozolu novads")]
    ),
    place!(
        "lv-liepu-parish",
        "locality",
        "village",
        "LV",
        "LV-002",
        200,
        (121.0, -45.0),
        "lv-liepu",
        "Liepu pagasts",
        &[("lv", "Liepu pagasts")]
    ),
    place!(
        "lv-ozolu-parish",
        "locality",
        "village",
        "LV",
        "LV-003",
        200,
        (122.0, -45.0),
        "lv-ozolu",
        "Ozolu pagasts",
        &[("lv", "Ozolu pagasts")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Birzu iela",
        postcode: "LV-1234",
        city: "Liepava",
        cc: "LV",
        lon: 120.0,
        lat: -45.0,
        numbers: &["12", "12 k-3", "12A", "12A k-37"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Liepu pag.",
        postcode: "LV-1234",
        city: "Liepu pagasts",
        cc: "LV",
        lon: 121.0,
        lat: -45.0,
        numbers: &["Silvas"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Ozolu pagasts",
        postcode: "LV-1235",
        city: "Ozolu pagasts",
        cc: "LV",
        lon: 122.0,
        lat: -45.0,
        numbers: &["Silvas"],
    },
];

const QUERIES: &[(&str, &str)] = &[
    (
        "\"Silvas\", Liepu pag., Liepu nov., LV-1234, Latvia",
        "Liepu pag.",
    ),
    ("Silvas, Liepu pagasts, Liepu novads, Latvia", "Liepu pag."),
    (
        "\"Silvas\", Ozolu pag., Ozolu nov., LV-1235, Latvia",
        "Ozolu pagasts",
    ),
    (
        "Silvas, Ozolu pagasts, Ozolu novads, Latvia",
        "Ozolu pagasts",
    ),
];

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
        .chain(
            QUERIES
                .iter()
                .map(|&(q, road)| case(q, at_house(road, "Silvas"))),
        )
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

#[test]
fn parish_spellings_find_the_same_named_house() {
    for &(query, road) in QUERIES {
        let found = index().suggest(query, 5, Some("LV"), None, false);
        let first = found
            .first()
            .unwrap_or_else(|| panic!("{query}: no suggestion"));
        assert_eq!(first.kind, "address", "{query}");
        assert!(
            metres((first.x, first.y), house(street(road), "Silvas")) < 5.0,
            "{query}"
        );
    }
    for query in [
        "Silvas, Liepu pagasts, Ozolu novads, Latvia",
        "Silvas, Ozolu pagasts, Liepu novads, Latvia",
        "Missing, Liepu pagasts, Liepu novads, Latvia",
    ] {
        let found = index().suggest(query, 5, Some("LV"), None, false);
        assert!(found.iter().all(|s| s.kind != "address"), "{query}");
    }
}

pub async fn check_conflicts(resolver: &crate::Resolver) {
    for query in [
        "Silvas, Liepu pagasts, Ozolu novads, Latvia",
        "Silvas, Ozolu pagasts, Liepu novads, Latvia",
        "Missing, Liepu pagasts, Liepu novads, Latvia",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
    }
}
