use super::*;
use crate::*;

pub const LISTINGS: &[(&str, f64, f64)] = &[
    ("솔빛시 새봄면 새로12길 812", 140.0002, 35.),
    ("새벽리 산8-2", 140.3001, 35.),
    ("새로12길 13-34", 140.0009, 35.),
    ("없는길 813", 140., 35.),
    ("없는리 815", 140.3, 35.),
    ("새벽리 814", 2.34, 48.86),
    ("새로12길 816 3층", 140., 35.),
    ("새로12길 817-", 140., 35.),
    ("나래도 솔빛시 새벽리 818", 140.3001, 35.),
];

#[test]
fn listed_native_addresses_are_complete_not_units_or_romanizations() {
    use opengeocoding::index::country::kr::listed_lot;
    for (q, street, number) in [
        ("가상면 새벽리 산 8-2", "새벽리", "산8-2"),
        ("새벽리 산8-2", "새벽리", "산8-2"),
        ("새봄동 19", "새봄동", "19"),
        ("새봄1가 19", "새봄1가", "19"),
        ("새벽리 99999-99999", "새벽리", "99999-99999"),
    ] {
        assert_eq!(
            listed_lot(q, |_| true),
            Some((street, number.into())),
            "{q}"
        );
    }
    let boundary = format!("{}면 새벽리 1", "가".repeat(249));
    assert_eq!(boundary.chars().count(), 256);
    assert!(listed_lot(&boundary, |_| true).is_some());
    assert!(listed_lot(&format!("가{boundary}"), |_| true).is_none());
    assert!(listed_lot("새벽리 산8-2", |_| false).is_none());
    assert!(listed_lot("나래도 새벽리 812", |name| name == "새벽리").is_none());
    for q in [
        "새벽리",
        "새벽리 1-",
        "새벽리 1-2-3",
        "새벽리 0",
        "새벽리 01",
        "새벽리 1-0",
        "새벽리 １２",
        "새벽리 1-123456",
        "새벽리 12,000",
        "새벽리 123456",
        "새벽리 12 3층",
        "새벽리 12 (빌딩)",
        "새로12길 산12",
        "새로12길 812",
        "1100로 12",
        "새벽리 산 산8-2",
        "12 새로12길",
        "Saero-gil 12",
        "South Korea, 새로12길 12",
        "없는건물 새로12길 12",
    ] {
        assert!(listed_lot(q, |_| true).is_none(), "{q}");
    }
}

pub fn check_listed_houses(poi: &Index) {
    for (tail, number, point) in [
        ("새벽리 산8-2", "산8-2", (140.3001, 35.)),
        ("새봄면 새로12길 13-34", "13-34", (140., 35.)),
    ] {
        let q = format!("가람도 솔빛시 {tail}, South Korea");
        let found = poi.suggest(&q, 5, None, None, false);
        assert_eq!(found.len(), 1, "{q}");
        assert_eq!(found[0].kind, "address");
        assert!(
            found[0].display_name.starts_with(&format!("{number}, ")),
            "{q}"
        );
        assert!(metres((found[0].x, found[0].y), point) < 5., "{q}");
        assert!(poi.suggest(&q, 5, Some("JP"), None, false).is_empty());
    }
    for tail in [
        "새로12길 812",
        "없는길 813",
        "없는리 815",
        "새벽리 814",
        "새로12길 816",
        "새로12길 817",
        "새벽리 818",
    ] {
        let q = format!("가람도 솔빛시 새봄면 {tail}, South Korea");
        assert!(
            !poi.suggest(&q, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{q}"
        );
    }
    let q = "가람도 솔빛시 새봄면 새로12길, South Korea";
    let before = index().suggest(q, 5, None, None, false);
    let after = poi.suggest(q, 5, None, None, false);
    assert_eq!(before[0].kind, "street");
    assert_eq!(after[0].kind, "street");
    assert!(metres((before[0].x, before[0].y), (after[0].x, after[0].y)) < 0.1);
    let lot = poi.suggest("가람도 솔빛시 새벽리, South Korea", 5, None, None, false);
    assert_eq!(lot[0].kind, "street");
    assert!(metres((lot[0].x, lot[0].y), (140.3001, 35.)) < 5.);
    let reversed = poi.reverse_search(35., 140.3001, 250., 20);
    assert!(
        reversed
            .iter()
            .all(|(_, v, _)| v["address"]["house_number"] != "산8-2")
    );
}

pub const PLACES: &[Place] = &[
    place!(
        "kr",
        "country",
        "",
        "KR",
        "",
        50_000_000,
        (141.0, 34.0),
        "",
        "대한민국",
        &[("en", "South Korea")]
    ),
    place!(
        "kr-garam",
        "region",
        "",
        "KR",
        "KR-41",
        1_000_000,
        (140.0, 35.0),
        "kr",
        "가람도",
        &[("en", "Garam"), ("ko", "경기도")]
    ),
    place!(
        "kr-narae",
        "region",
        "",
        "KR",
        "KR-43",
        1_000_000,
        (142.0, 35.0),
        "kr",
        "나래도",
        &[("en", "Narae"), ("ko", "충청북도")]
    ),
    place!(
        "kr-garam-solbit",
        "county",
        "",
        "KR",
        "KR-41",
        10_000,
        (140.0, 35.0),
        "kr-garam",
        "솔빛시",
        &[("en", "Solbit")]
    ),
    place!(
        "kr-narae-solbit",
        "county",
        "",
        "KR",
        "KR-43",
        10_000,
        (142.0, 35.0),
        "kr-narae",
        "솔빛시",
        &[("en", "Solbit")]
    ),
    place!(
        "kr-garam-saebom",
        "locality",
        "town",
        "KR",
        "KR-41",
        5_000,
        (140.0, 35.0),
        "kr-garam-solbit",
        "새봄면",
        &[("en", "Saebom")]
    ),
    place!(
        "kr-narae-saebom",
        "locality",
        "town",
        "KR",
        "KR-43",
        5_000,
        (142.0, 35.0),
        "kr-narae-solbit",
        "새봄면",
        &[("en", "Saebom")]
    ),
    place!(
        "kr-garam-saebyeok",
        "locality",
        "village",
        "KR",
        "KR-41",
        500,
        (140.3, 35.0),
        "kr-garam-solbit",
        "새벽리",
        &[("en", "Saebyeok")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "새로12길",
        postcode: "12345",
        city: "새봄면",
        cc: "KR",
        lon: 140.0,
        lat: 35.0,
        numbers: &["13-34", "290", "17"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "새로12길",
        postcode: "54321",
        city: "새봄면",
        cc: "KR",
        lon: 142.0,
        lat: 35.0,
        numbers: &["13-34", "999"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "새로12길",
        postcode: "12345",
        city: "새봄면",
        cc: "KR",
        lon: 140.15,
        lat: 35.0,
        numbers: &["17"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "새로12길20번길",
        postcode: "12345",
        city: "새봄면",
        cc: "KR",
        lon: 140.001,
        lat: 35.0,
        numbers: &["99"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "옛리",
        postcode: "12345",
        city: "새봄면",
        cc: "KR",
        lon: 140.002,
        lat: 35.0,
        numbers: &["45-6", "산45-6"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "들길",
        postcode: "12345",
        city: "새벽리",
        cc: "KR",
        lon: 140.3,
        lat: 35.0,
        numbers: &["7"],
    },
];

#[test]
fn country_first_preserves_scope_and_completion_boundaries() {
    for country in ["South Korea", "대한민국"] {
        for (province, lon) in [
            ("가람도", 140.),
            ("경기", 140.),
            ("나래도", 142.),
            ("충북", 142.),
        ] {
            let query = format!("{country}, {province} 솔빛시 새봄면 새로12길 13-34");
            let found = index().suggest(&query, 5, None, None, false);
            assert_eq!(found.len(), 1, "{query}");
            assert_eq!(found[0].kind, "address");
            assert!(found[0].display_name.starts_with("13-34, "));
            assert!(metres((found[0].x, found[0].y), (lon, 35.)) < 5., "{query}");
            assert!(
                index()
                    .suggest(&query, 5, Some("JP"), None, false)
                    .is_empty()
            );
        }
        for (tail, want) in [("새로1", 2), ("새로12길 13-", 1)] {
            let query = format!("{country}, 가람도 솔빛시 새봄면 {tail}");
            assert_eq!(
                index().suggest(&query, 5, None, None, false).len(),
                want,
                "{query}"
            );
            let closed = index().suggest(&format!("{query} "), 5, None, None, false);
            if tail == "새로1" {
                assert!(closed.is_empty(), "{query}");
            } else {
                assert_eq!(closed.len(), 1, "{query}");
                assert_eq!(closed[0].kind, "street", "{query}");
                assert!(closed[0].display_name.starts_with("새로12길, "), "{query}");
            }
        }
        let query = format!("{country}, 가람도 솔빛시 새봄면 들길 7");
        let found = index().suggest(&query, 5, None, None, false);
        assert_eq!(found.len(), 1, "{query}");
        assert!(metres((found[0].x, found[0].y), (140.3, 35.)) < 5.);
    }
    for query in [
        "Japan, 가람도 솔빛시 새봄면 새로12길 13-34",
        "South Korea, 가람도 없는시 새봄면 새로12길 13-34",
        "South Korea, 나래도 솔빛시 새봄면 들길 7",
        "South Korea, 가람도 솔빛시 없는면 들길 7",
        "South Korea, 가람도 솔빛시 새봄면 새로12길 999",
    ] {
        assert!(
            !index()
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

#[test]
fn short_provinces_keep_the_house_and_qualifiers() {
    for (province, number, lon) in [
        ("경기", "13-34", 140.0),
        ("경기", "290", 140.0001),
        ("충북", "13-34", 142.0),
    ] {
        let query = format!("{province} 솔빛시 새봄면 새로12길 {number}, South Korea");
        let found = index().suggest(&query, 5, Some("KR"), None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        assert!(
            found[0].display_name.starts_with(&format!("{number}, ")),
            "{query}"
        );
        assert!(
            metres((found[0].x, found[0].y), (lon, 35.0)) < 5.,
            "{query}"
        );
        assert!(
            index()
                .suggest(&query, 5, Some("JP"), None, false)
                .is_empty()
        );
    }
    for query in [
        "경기 솔빛시 새봄면 들길 7, South Korea",
        "경기 솔빛시 새봄면 새로12길 13-",
    ] {
        let found = index().suggest(query, 5, None, None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
    }
    for query in [
        "경기 솔빛시 새봄면 새로12길 999, South Korea",
        "충북 솔빛시 새봄면 들길 7, South Korea",
        "경기 없는시 새봄면 새로12길 13-34, South Korea",
        "경기 솔빛시 없는면 들길 7, South Korea",
        "경기 솔빛시 새봄면 새로12길 13-34, Japan",
        "경기 솔빛시 새봄면 새로12길 13-, South Korea",
    ] {
        assert!(
            !index()
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}

pub async fn check_short_provinces(resolver: &opengeocoding::resolver::Resolver) {
    for (province, number, lon, code) in [
        ("경기", "13-34", 140.0, "KR-41"),
        ("경기", "290", 140.0001, "KR-41"),
        ("충북", "13-34", 142.0, "KR-43"),
    ] {
        for query in [
            format!("{province} 솔빛시 새봄면 새로12길 {number}, South Korea"),
            format!("South Korea, {province} 솔빛시 새봄면 새로12길 {number}"),
            format!("대한민국, {province} 솔빛시 새봄면 새로12길 {number}"),
        ] {
            let found = resolver.resolve(&query).await.unwrap();
            let detail = found.address_details.as_option().unwrap();
            assert!(found.good_match, "{query}: {found:?}");
            assert_eq!(detail.house_number, number, "{query}");
            assert!(
                detail.region_codes.iter().any(|r| r.code == code),
                "{query}"
            );
            let point = found.geo_coordinates.as_option().unwrap();
            assert!(
                metres((point.lon as f64, point.lat as f64), (lon, 35.0)) < 5.,
                "{query}"
            );
        }
    }
    for query in [
        "Japan, 경기 솔빛시 새봄면 새로12길 13-34",
        "South Korea, 충북 솔빛시 새봄면 들길 7",
        "South Korea, 경기 없는시 새봄면 새로12길 13-34",
        "South Korea, 경기 솔빛시 새봄면 새로12길 999",
        "South Korea, 경기 솔빛시 새봄면 새로12길 17",
        "경기 솔빛시 새봄면 새로12길 999, South Korea",
        "충북 솔빛시 새봄면 들길 7, South Korea",
        "경기 없는시 새봄면 새로12길 13-34, South Korea",
        "경기 솔빛시 새봄면 새로12길 13-34, Japan",
        "경기 솔빛시 새봄면 새로12길 17, South Korea",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
        if query.starts_with("Japan,") {
            assert!(
                found.geo_coordinates.as_option().is_none(),
                "{query}: {found:?}"
            );
        }
        if query.starts_with("South Korea, 충북") {
            assert!(
                found
                    .address_details
                    .as_option()
                    .is_none_or(|d| d.region_codes.iter().all(|r| r.code != "KR-41")),
                "{query}: {found:?}"
            );
        }
    }
}

#[test]
fn korean_houses_keep_their_number_and_administrative_hierarchy() {
    for (province, number, lon, code) in [
        ("가람도", "13-34", 140.0, "KR-41"),
        ("가람도", "290", 140.0001, "KR-41"),
        ("나래도", "13-34", 142.0, "KR-43"),
    ] {
        let query = format!("{province} 솔빛시 새봄면 새로12길 {number}, South Korea");
        let found = index().suggest(&query, 5, Some("KR"), None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        let want = (lon, 35.0);
        assert!(metres((found[0].x, found[0].y), want) < 5., "{query}");
        assert!(
            index()
                .suggest(&query, 5, Some("JP"), None, false)
                .is_empty()
        );
        let reversed = index().reverse_search(want.1, want.0, 5., 1);
        assert_eq!(reversed[0].1["address"]["house_number"], number);
        assert_eq!(reversed[0].1["address"]["ISO3166-2-lvl4"], code);
        assert_eq!(
            index().reverse_region(want.1, want.0)["address"]["ISO3166-2-lvl4"],
            code
        );
    }
    for prefix in [
        "가람도 솔빛시 새봄면 새로1",
        "가람도 솔빛시 새봄면 새로12길 13",
        "가람도 솔빛시 새봄면 새로12길 13-",
    ] {
        let found = index().suggest(prefix, 1, None, None, false);
        assert_eq!(found.len(), 1, "{prefix}");
        assert!(
            metres((found[0].x, found[0].y), (140.0, 35.0)) < 200.,
            "{prefix}"
        );
        if prefix.ends_with(['3', '-']) {
            assert_eq!(found[0].kind, "address", "{prefix}");
        }
    }
    for (number, lon) in [
        ("45-6", 140.002),
        ("산45-6", 140.0021),
        ("산 45-6", 140.0021),
    ] {
        let query = format!("가람도 솔빛시 새봄면 옛리 {number}, 대한민국");
        let found = index().suggest(&query, 5, None, None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        assert!(metres((found[0].x, found[0].y), (lon, 35.)) < 5., "{query}");
    }
    // 새벽리 sits straight under its 시, as the data often puts a 리 outside its stated 면.
    for query in [
        "가람도 솔빛시 새봄면 들길 7, South Korea",
        "가람도 솔빛시 새봄면 들길 7",
    ] {
        let found = index().suggest(query, 5, None, None, false);
        assert_eq!((found.len(), found[0].kind), (1, "address"), "{query}");
        assert!(
            metres((found[0].x, found[0].y), (140.3, 35.)) < 5.,
            "{query}"
        );
    }
    for query in [
        "나래도 솔빛시 새봄면 들길 7, South Korea",
        "가람도 솔빛시 없는면 들길 7, South Korea",
        "가람도 솔빛시 새봄면 새로12길 999, South Korea",
        "가람도 솔빛시 새봄면 새로12길 99, South Korea",
        "가람도 솔빛시 새봄면 새로12길 13-34, Japan",
        "가람도 없는시 새봄면 새로12길 13-34, South Korea",
    ] {
        assert!(
            !index()
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
}
