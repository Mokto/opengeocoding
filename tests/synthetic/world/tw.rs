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
    place!(
        "tw-shuilin",
        "locality",
        "town",
        "TW",
        "TW-YUN",
        25_000,
        (123.5, 24.0),
        "tw-yunlin",
        "水林鄉",
        &[("en", "Shuilin")]
    ),
    place!(
        "tw-taoyuan",
        "region",
        "",
        "TW",
        "TW-TAO",
        2_000_000,
        (124.0, 23.0),
        "tw",
        "桃園市",
        &[("en", "Taoyuan City")]
    ),
    place!(
        "tw-dayuan",
        "locality",
        "town",
        "TW",
        "TW-TAO",
        90_000,
        (124.0, 23.0),
        "tw-taoyuan",
        "大園區",
        &[("en", "Dayuan District")]
    ),
    place!(
        "tw-pingtung",
        "region",
        "",
        "TW",
        "TW-PIF",
        800_000,
        (124.5, 23.0),
        "tw",
        "屏東縣",
        &[("en", "Pingtung County")]
    ),
    place!(
        "tw-wutai",
        "locality",
        "village",
        "TW",
        "TW-PIF",
        3_000,
        (124.5, 23.0),
        "tw-pingtung",
        "霧臺鄉",
        &[("en", "Wutai")]
    ),
    place!(
        "tw-changhua",
        "region",
        "",
        "TW",
        "TW-CHA",
        1_200_000,
        (125.0, 23.0),
        "tw",
        "彰化縣",
        &[("en", "Changhua County")]
    ),
    place!(
        "tw-fuxing",
        "locality",
        "town",
        "TW",
        "TW-CHA",
        45_000,
        (125.0, 23.0),
        "tw-changhua",
        "福興鄉",
        &[("en", "Fuxing")]
    ),
    place!(
        "tw-hualien",
        "region",
        "",
        "TW",
        "TW-HUA",
        300_000,
        (125.5, 23.0),
        "tw",
        "花蓮縣",
        &[("en", "Hualien County")]
    ),
    place!(
        "tw-ruisui",
        "locality",
        "town",
        "TW",
        "TW-HUA",
        11_000,
        (125.5, 23.0),
        "tw-hualien",
        "瑞穗鄉",
        &[("en", "Ruisui")]
    ),
    place!(
        "tw-taitung",
        "region",
        "",
        "TW",
        "TW-TTT",
        200_000,
        (126.2, 23.0),
        "tw",
        "台東縣",
        &[("en", "Taitung County")]
    ),
    place!(
        "tw-taimali",
        "locality",
        "town",
        "TW",
        "TW-TTT",
        10_000,
        (126.0, 23.0),
        "tw-taitung",
        "太麻里鄉",
        &[("en", "Taimali")]
    ),
    place!(
        "tw-jinfeng",
        "locality",
        "village",
        "TW",
        "TW-TTT",
        3_500,
        (126.4, 23.0),
        "tw-taitung",
        "金峰鄉",
        &[("en", "Jinfeng")]
    ),
];

const fn osm_street(
    name: &'static str,
    city: &'static str,
    lon: f64,
    lat: f64,
    numbers: &'static [&'static str],
) -> Street {
    Street {
        class: "residential",
        length: 0.0,
        name,
        postcode: "",
        city,
        cc: "TW",
        lon,
        lat,
        numbers,
    }
}

/// 番婆街 建51之21, an OpenStreetMap address node: the register's rows (`STREETS`) give a
/// number's 之 part as its unit, which the builder keeps off a number with a prefix.
pub const PREFIXED_HOUSE: (f64, f64) = (125.0003, 23.0);

// A village name OpenStreetMap numbers houses on, on both sides of a township line 2 km apart.
const TAIMALI_JIALUN: Street = osm_street("佳崙", "太麻里鄉", 126.19, 23.0, &["12"]);
const JINFENG_JIALUN: Street = osm_street("佳崙", "金峰鄉", 126.21, 23.0, &["386"]);

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
    osm_street("溪海路153巷", "大園區", 124.0, 23.0, &["33", "35"]),
    osm_street("東川巷", "霧臺鄉", 124.5, 23.0, &["49", "49之1"]),
    osm_street("番婆街", "福興鄉", 125.0, 23.0, &["51"]),
    osm_street("萬興", "水林鄉", 123.5, 24.0, &["48之17"]),
    osm_street("瑞北", "瑞穗鄉", 125.5, 23.0, &["156"]),
    TAIMALI_JIALUN,
    JINFENG_JIALUN,
];

/// OpenStreetMap's Taiwanese addresses, each part apart: libpostal folds the road into a house,
/// cuts 霧臺鄉 to 霧, reads 大園區 as a floor and 台灣省 as the town, and splits 建 off a number.
pub fn cases() -> Vec<Case> {
    let mut cases = vec![];
    for (road, number, places) in [
        ("溪海路153巷", "35號", "大園區, 桃園市"),
        ("東川巷", "49之1號", "霧臺鄉, 屏東縣, 台灣省"),
        ("萬興", "48之17", "水林鄉, 雲林縣, 台灣省"),
        ("瑞北", "156號", "瑞穗鄉, 花蓮縣, 台灣省"),
    ] {
        let stored = number.trim_end_matches('號');
        for query in [
            format!("{number} {road}, {places}, Taiwan"),
            format!("{road} {number}, {places}, Taiwan"),
        ] {
            cases.push(case(query.leak(), at_house(road, stored)));
        }
    }
    for query in [
        "建51之21號 番婆街, 福興鄉, 彰化縣, 台灣省, Taiwan",
        "番婆街 建51之21號, 福興鄉, 彰化縣, 台灣省, Taiwan",
    ] {
        cases.push(case(query, Want::At(PREFIXED_HOUSE, 25.0, true)));
    }
    // the township the query names holds the street but not the house, which the next one does
    let across = Want::At(house(&JINFENG_JIALUN, "386"), 25.0, false);
    cases.push(case("386號 佳崙, 太麻里鄉, 台東縣, 台灣省, Taiwan", across));
    cases
}

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

// Overture writes Taiwan's house 11之2 as number 11 with unit 之2號: it is its own house, not 11.
#[test]
fn taiwanese_sub_numbers_from_units_stay_their_own_houses() {
    let road = street("明光路3段389巷6弄");
    for number in ["11", "11之2"] {
        let at = house(road, number);
        let found = index().reverse_search(at.1, at.0, 50.0, 1);
        assert_eq!(found[0].1["address"]["house_number"], number, "{number}");
        assert!(found[0].2 < 5.0, "{number}: {}", found[0].2);
    }
}

#[test]
fn taiwanese_suggestions_keep_a_numbers_prefix() {
    for text in [
        "建51之21號 番婆街, 福興鄉, 彰化縣, 台灣省, Taiwan",
        "番婆街 建51之21號, 福興鄉, 彰化縣, 台灣省, Taiwan",
    ] {
        let found = index().suggest(text, 5, None, None, false);
        let hit = found.first().unwrap_or_else(|| panic!("{text}"));
        assert_eq!(hit.kind, "address", "{text}");
        assert!(hit.display_name.starts_with("建51之21, "), "{text}");
        assert!(metres((hit.x, hit.y), PREFIXED_HOUSE) < 5.);
    }
}
