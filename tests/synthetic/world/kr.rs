use super::*;
use crate::*;

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
        &[("en", "Garam")]
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
        &[("en", "Narae")]
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
