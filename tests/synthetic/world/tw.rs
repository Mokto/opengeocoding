use super::*;
use crate::*;

pub const PLACES: &[Place] = &[
    place!(
        "tw",
        "country",
        "",
        "TW",
        "",
        20_000_000,
        (125.0, 20.0),
        "",
        "臺灣",
        &[("en", "Taiwan")]
    ),
    place!(
        "tw-qingshan",
        "region",
        "",
        "TW",
        "TW-TPE",
        1_000_000,
        (123.0, 25.0),
        "tw",
        "青山市",
        &[("en", "Qingshan")]
    ),
    place!(
        "tw-guanghua",
        "locality",
        "city",
        "TW",
        "TW-TPE",
        50_000,
        (123.0, 25.0),
        "tw-qingshan",
        "光華區",
        &[("en", "Guanghua District")]
    ),
    place!(
        "tw-yunlin",
        "region",
        "",
        "TW",
        "TW-YUN",
        600_000,
        (123.5, 24.5),
        "tw",
        "雲林縣",
        &[("en", "Yunlin County")]
    ),
    place!(
        "tw-huwei",
        "locality",
        "town",
        "TW",
        "TW-YUN",
        70_000,
        (123.5, 24.5),
        "tw-yunlin",
        "虎尾鎮",
        &[("en", "Huwei")]
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "明光路3段389巷6弄",
        postcode: "",
        city: "光華區",
        cc: "TW",
        lon: 123.0,
        lat: 25.0,
        numbers: &["11", "11之2"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "市民大道五段",
        postcode: "",
        city: "光華區",
        cc: "TW",
        lon: 123.01,
        lat: 25.0,
        numbers: &["131", "7"],
    },
    // 511 is also a postcode here: a house number next to its road must stay the house.
    Street {
        class: "residential",
        length: 0.0,
        name: "林森路二段",
        postcode: "",
        city: "虎尾鎮",
        cc: "TW",
        lon: 123.5,
        lat: 24.5,
        numbers: &["511"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "中正路",
        postcode: "511",
        city: "虎尾鎮",
        cc: "TW",
        lon: 123.51,
        lat: 24.5,
        numbers: &["8"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "市民大道五段13巷",
        postcode: "",
        city: "光華區",
        cc: "TW",
        lon: 123.02,
        lat: 25.0,
        numbers: &["131", "999"],
    },
];

#[test]
fn taiwanese_suggestions_keep_building_identity_and_unverified_floors() {
    for (road, number, stored) in [
        ("明光路三段389巷6弄", "11", "明光路3段389巷6弄"),
        ("明光路3段389巷6弄", "11之2", "明光路3段389巷6弄"),
        ("市民大道五段", "131", "市民大道五段"),
        ("市民大道5段", "7", "市民大道五段"),
    ] {
        let building = format!("青山市光華區{road}{number}號");
        let plain = index().suggest(&building, 1, None, None, false);
        assert_eq!(plain.len(), 1, "{building}");
        assert_eq!(plain[0].kind, "address", "{building}");
        assert!(plain[0].display_name.starts_with(&format!("{number}, ")));
        assert!(metres((plain[0].x, plain[0].y), house(street(stored), number)) < 5.);
        for floor in ["", "三樓", "7樓", "地下二樓"] {
            for country in ["", ", Taiwan", "，臺灣"] {
                let query = format!("{building}{floor}{country}");
                let fullwidth: String = query
                    .chars()
                    .map(|c| {
                        if c.is_ascii_digit() {
                            char::from_u32(c as u32 + 0xfee0).unwrap()
                        } else {
                            c
                        }
                    })
                    .collect();
                for text in [&query, &fullwidth] {
                    for cc in [None, Some("TW")] {
                        let found = index().suggest(text, 1, cc, None, false);
                        assert_eq!(found.len(), 1, "{text}");
                        let hit = &found[0];
                        assert_eq!(hit.kind, "address", "{text}");
                        assert_eq!(hit.country_code, "tw", "{text}");
                        assert_eq!(hit.retrieval_id, plain[0].retrieval_id, "{text}");
                        assert_eq!((hit.x, hit.y), (plain[0].x, plain[0].y), "{text}");
                        let suffix = text
                            .split('號')
                            .nth(1)
                            .unwrap()
                            .split([',', '，'])
                            .next()
                            .unwrap();
                        assert_eq!(hit.unverified_suffix, suffix, "{text}");
                        if !suffix.is_empty() {
                            assert!(hit.display_name.contains(suffix), "{text}");
                        }
                        let retrieved = index().retrieve_suggestion(&hit.retrieval_id).unwrap();
                        assert_eq!(retrieved.id, plain[0].id, "{text}");
                        assert_eq!((retrieved.x, retrieved.y), (hit.x, hit.y), "{text}");
                        assert!(retrieved.unverified_suffix.is_empty(), "{text}");
                    }
                }
            }
        }
    }
    for query in [
        "青山市光華區明光路三段389巷6弄999號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11之9號三樓, Taiwan",
        "青山市光華區明光路四段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段388巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷5弄11號三樓, Taiwan",
        "青山市光華區市民大道五段999號三樓, Taiwan",
        "碧海市光華區明光路三段389巷6弄11號三樓, Taiwan",
        "青山市未知區明光路三段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11號三樓, France",
        "青山市光華區明光路三段389巷6弄11號三樓, Alpha, Taiwan",
        "青山市光華區明光路三段389巷6弄11號三樓, Missingland",
        "青山市光華區明光路三段389巷6弄11號三樓商店, Taiwan",
        "999青山市光華區明光路三段389巷6弄11號三樓, Taiwan",
        "９９９青山市光華區明光路三段389巷6弄11號三樓, Taiwan",
        "中國青山市光華區明光路三段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄12345678901號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11-12號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄11至12號三樓, Taiwan",
        "青山市光華區明先路三段389巷6弄11號三樓, Taiwan",
        "青山市光華區明光路三段389巷6弄",
    ] {
        assert!(
            !index()
                .suggest(query, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{query}"
        );
    }
    let query = "青山市光華區明光路三段389巷6弄11號三樓, Taiwan";
    assert!(
        index()
            .suggest(query, 5, Some("JP"), None, false)
            .is_empty()
    );
    assert!(index().suggest(query, 0, None, None, false).is_empty());
}

// OSM still files Taiwan's addresses under 台灣省, a province no place of the index is.
#[test]
fn taiwanese_suggestions_skip_taiwan_province() {
    for province in ["台灣省", "臺灣省", "Taiwan Province"] {
        for text in [
            format!("511 林森路二段, 虎尾鎮, 雲林縣, {province}, Taiwan"),
            format!("林森路二段 511, 虎尾鎮, 雲林縣, {province}, Taiwan"),
        ] {
            let found = index().suggest(&text, 5, None, None, false);
            let hit = found.first().unwrap_or_else(|| panic!("{text}"));
            assert_eq!(hit.kind, "address", "{text}");
            assert_eq!(hit.country_code, "tw", "{text}");
            assert!(
                metres((hit.x, hit.y), house(street("林森路二段"), "511")) < 5.,
                "{text}"
            );
            assert!(
                index()
                    .suggest(&text, 5, Some("JP"), None, false)
                    .is_empty()
            );
        }
    }
}
