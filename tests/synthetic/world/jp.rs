use super::*;
use crate::{Case, at_house, case, index, metres, street, town};

pub const PLACES: &[Place] = &[
    place!(
        "jp",
        "country",
        "",
        "JP",
        "",
        100_000_000,
        (146.0, 41.0),
        "",
        "日本",
        &[("en", "Japan")]
    ),
    place!(
        "jp-aoba",
        "region",
        "",
        "JP",
        "JP-04",
        1_000_000,
        (145.0, 40.0),
        "jp",
        "島根県",
        &[("en", "Shimane Prefecture")]
    ),
    place!(
        "jp-aoba-city",
        "locality",
        "city",
        "JP",
        "JP-04",
        50_000,
        (145.0, 40.0),
        "jp-aoba",
        "松江市",
        &[("en", "Matsue")]
    ),
    place!(
        "jp-oita",
        "region",
        "",
        "JP",
        "JP-44",
        1_000_000,
        (145.5, 41.0),
        "jp",
        "大分県",
        &[("en", "Oita Prefecture")]
    ),
    place!(
        "jp-nakatsu",
        "locality",
        "city",
        "JP",
        "JP-44",
        50_000,
        (145.5, 41.0),
        "jp-oita",
        "中津市",
        &[("en", "Nakatsu")]
    ),
    place!(
        "jp-asanagi",
        "locality",
        "city",
        "JP",
        "JP-44",
        50_000,
        (145.5, 43.0),
        "jp-oita",
        "朝凪市",
        &[("en", "Asanagi")]
    ),
    place!(
        "jp-harukaze",
        "macrohood",
        "",
        "JP",
        "JP-44",
        10_000,
        (145.51, 43.0),
        "jp-asanagi",
        "晴風区",
        &[("en", "Harukaze Ward")]
    ),
    place!(
        "jp-asamura",
        "macrohood",
        "",
        "JP",
        "JP-44",
        10_000,
        (145.515, 43.0),
        "jp-asanagi",
        "朝村区",
        &[("en", "Asamura Ward")]
    ),
    place!(
        "jp-aonami",
        "macrohood",
        "",
        "JP",
        "JP-44",
        10_000,
        (145.52, 43.0),
        "jp-asanagi",
        "青波区",
        &[("en", "Aonami Ward")]
    ),
    place!(
        "jp-yunagi",
        "locality",
        "",
        "JP",
        "JP-44",
        10_000,
        (146.0, 43.0),
        "jp-asanagi",
        "夕凪区",
        &[("en", "Yunagi Ward")]
    ),
    place!(
        "jp-hoshi-county",
        "county",
        "",
        "JP",
        "JP-44",
        20_000,
        (145.5, 42.0),
        "jp-oita",
        "星雲郡",
        &[("en", "Hoshi County")]
    ),
    place!(
        "jp-natsumi",
        "locality",
        "town",
        "JP",
        "JP-44",
        10_000,
        (145.5, 42.0),
        "jp-hoshi-county",
        "夏見町",
        &[("en", "Natsumi")]
    ),
    place!(
        "jp-asagumo",
        "region",
        "",
        "JP",
        "JP-05",
        1_000_000,
        (146.0, 38.0),
        "jp",
        "朝雲県",
        &[("en", "Asagumo Prefecture")]
    ),
    place!(
        "jp-shadow-nakatsu",
        "locality",
        "city",
        "JP",
        "JP-05",
        2_000_000,
        (146.0, 38.0),
        "jp-asagumo",
        "中津市",
        &[("en", "Shadow Nakatsu")]
    ),
    place!(
        "jp-tsu",
        "locality",
        "city",
        "JP",
        "JP-05",
        50_000,
        (146.0, 38.0),
        "jp-asagumo",
        "津市",
        &[("en", "Tsu")]
    ),
];

const WARD_STREET: Street = Street {
    class: "residential",
    length: 0.0,
    name: "風見二丁目",
    postcode: "",
    city: "朝凪市晴風区",
    cc: "JP",
    lon: 145.51,
    lat: 43.0,
    numbers: &["7-8-9"],
};

const MLIT_STREET: Street = Street {
    name: "真珠一丁目",
    city: "中津市",
    lon: 145.5,
    lat: 41.0,
    numbers: &["5-12"],
    ..WARD_STREET
};

pub fn sources(s: &Street) -> &'static [(&'static str, &'static str, &'static str)] {
    match s.name {
        "真珠一丁目" if s.lon == 145.5 => &[("open_addresses", "jp/oita", "911134")],
        "真珠一丁目" | "遠霞五丁目" => &[("open_addresses", "jp/countrywide", "911134")],
        "凪通二丁目" | "裂野三丁目" | "砂丘一丁目" | "朝露一丁目" | "孤潮七丁目" => {
            &[("open_addresses", "jp/oita", "911134")]
        }
        "次波四丁目" => &[("open_addresses", "jp/oita", "999999")],
        "遠波五丁目" => &[("other_provider", "jp/oita", "911134")],
        "混波六丁目" => &[
            ("open_addresses", "jp/oita", "911134"),
            ("open_addresses", "jp/countrywide", "911134"),
        ],
        _ => &[],
    }
}

pub const STREETS: &[Street] = &[
    MLIT_STREET,
    Street {
        name: "孤潮七丁目",
        numbers: &["16-12"],
        lon: 145.62,
        ..MLIT_STREET
    },
    Street {
        numbers: &["5-12", "5-2-3"],
        postcode: "123-4567",
        lon: 145.52,
        ..MLIT_STREET
    },
    Street {
        name: "凪通二丁目",
        numbers: &["7-12"],
        lon: 145.51,
        ..MLIT_STREET
    },
    Street {
        name: "裂野三丁目",
        numbers: &["8-99"],
        ..MLIT_STREET
    },
    Street {
        name: "次波四丁目",
        numbers: &["9", "11-12"],
        lon: 145.55,
        ..MLIT_STREET
    },
    Street {
        name: "遠波五丁目",
        numbers: &["10-12"],
        lon: 145.54,
        ..MLIT_STREET
    },
    Street {
        name: "遠霞五丁目",
        numbers: &["12-13"],
        lon: 145.54,
        ..MLIT_STREET
    },
    Street {
        name: "混波六丁目",
        numbers: &["13-12"],
        lon: 145.56,
        ..MLIT_STREET
    },
    Street {
        name: "砂丘一丁目",
        numbers: &["14-12"],
        lon: 145.51,
        ..MLIT_STREET
    },
    Street {
        name: "砂丘一丁目",
        numbers: &["14-12"],
        lon: 145.57,
        ..MLIT_STREET
    },
    Street {
        name: "朝露一丁目",
        numbers: &["15-12"],
        lon: 146.0,
        lat: 38.0,
        ..MLIT_STREET
    },
    WARD_STREET,
    Street {
        name: "朱明二丁目",
        city: "朝凪市朝村区",
        numbers: &["14-15"],
        ..WARD_STREET
    },
    Street {
        city: "朝凪市青波区",
        lon: 145.62,
        ..WARD_STREET
    },
    Street {
        name: "霞通一丁目",
        numbers: &["3-4"],
        ..WARD_STREET
    },
    Street {
        name: "霞通一丁目",
        city: "朝凪市青波区",
        numbers: &["3-5"],
        lon: 145.51015,
        ..WARD_STREET
    },
    Street {
        name: "浜道三丁目",
        numbers: &["6-7"],
        postcode: "223-4455",
        ..WARD_STREET
    },
    Street {
        name: "浜道三丁目",
        city: "朝凪市青波区",
        numbers: &["6-8"],
        postcode: "223-4456",
        lon: 145.51015,
        ..WARD_STREET
    },
    Street {
        name: "霧道一丁目",
        numbers: &["8-9"],
        postcode: "223-4457",
        ..WARD_STREET
    },
    Street {
        name: "霧道一丁目",
        city: "朝凪市青波区",
        numbers: &["8-10"],
        postcode: "223-4457",
        lon: 145.51015,
        ..WARD_STREET
    },
    Street {
        name: "重波一丁目",
        numbers: &["11-12"],
        ..WARD_STREET
    },
    Street {
        name: "重波一丁目",
        city: "朝凪市青波区",
        numbers: &["11-12"],
        lon: 145.516,
        ..WARD_STREET
    },
    Street {
        name: "波路一丁目",
        city: "朝凪市",
        numbers: &["4-5"],
        ..WARD_STREET
    },
    Street {
        name: "二重一丁目",
        numbers: &["13-14"],
        ..WARD_STREET
    },
    Street {
        name: "二重一丁目",
        numbers: &["13-14"],
        lon: 145.55,
        ..WARD_STREET
    },
    Street {
        name: "朝影一丁目",
        city: "朝凪市夕凪区",
        numbers: &["10-11"],
        lon: 146.0,
        ..WARD_STREET
    },
    Street {
        name: "朝影一丁目",
        city: "朝凪市夕凪区",
        numbers: &["10-11"],
        ..WARD_STREET
    },
    Street {
        name: "夕映一丁目",
        numbers: &["9-10"],
        lon: 146.0,
        ..WARD_STREET
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "朝日町伊勢宮",
        postcode: "",
        city: "松江市",
        cc: "JP",
        lon: 145.0,
        lat: 40.0,
        numbers: &["472-2"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Koyodai",
        postcode: "",
        city: "松江市",
        cc: "JP",
        lon: 145.01,
        lat: 40.0,
        numbers: &["258-2"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "沖代町一丁目",
        postcode: "",
        city: "中津市",
        cc: "JP",
        lon: 145.5,
        lat: 41.0,
        numbers: &["5-9"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "海風一丁目",
        postcode: "123-4567",
        city: "中津市",
        cc: "JP",
        lon: 145.51,
        lat: 41.0,
        numbers: &["2-3", "8-1", "9-1", "10-1", "2-3-4", "2-3101"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "桜９条東三丁目",
        postcode: "",
        city: "中津市",
        cc: "JP",
        lon: 145.52,
        lat: 41.0,
        numbers: &["7-8-9"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "海風",
        postcode: "",
        city: "中津市",
        cc: "JP",
        lon: 145.53,
        lat: 41.0,
        numbers: &["1-6"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "日和二丁目",
        postcode: "",
        city: "夏見町",
        cc: "JP",
        lon: 145.5,
        lat: 42.0,
        numbers: &["6-7-8"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "沖代町一丁目",
        postcode: "999-9999",
        city: "中津市",
        cc: "JP",
        lon: 146.0,
        lat: 38.0,
        numbers: &["5-9"],
    },
];

pub fn cases() -> Vec<Case> {
    [
        "日本大分県中津市沖代町一丁目5番9号",
        "大分県中津市沖代町一丁目5番9号, Japan",
    ]
    .into_iter()
    .map(|q| case(q, at_house("沖代町一丁目", "5-9")))
    .chain([
        case(
            "大分県中津市真珠一丁目5番, Japan",
            crate::Want::At((145.5, 41.0), 25., true),
        ),
        case(
            "日本大分県中津市凪通二丁目7番",
            crate::Want::At((145.51, 41.0), 25., true),
        ),
        case(
            "日本大分県中津市次波四丁目9番",
            crate::Want::At((145.55, 41.0), 25., true),
        ),
        case(
            "日本大分県中津市真珠一丁目5-12",
            crate::Want::At((145.52, 41.0), 25., true),
        ),
        case(
            "大分県中津市凪通二丁目7-12, Japan",
            town("jp-nakatsu", false),
        ),
        case(
            "大分県朝凪市朝村区朱明二丁目14-15, Japan",
            at_house("朱明二丁目", "14-15"),
        ),
        case(
            "大分県朝凪市晴風区霞通一丁目3-5, Japan",
            town("jp-asanagi", false),
        ),
        case(
            "大分県朝凪市晴風区霧道一丁目8-9, Japan",
            town("jp-asanagi", false),
        ),
        case(
            "大分県朝凪市晴風区二重一丁目13-14, Japan",
            town("jp-asanagi", false),
        ),
        case(
            "大分県朝凪市晴風区波路一丁目4-5, Japan",
            town("jp-asanagi", false),
        ),
        case(
            "大分県朝凪市晴風区夕映一丁目9-10, Japan",
            town("jp-asanagi", false),
        ),
        case(
            "大分県朝凪市夕凪区朝影一丁目10-11, Japan",
            at_house("朝影一丁目", "10-11"),
        ),
        case(
            "大分県朝凪市晴風区風見二丁目7-8-9, Japan",
            at_house("風見二丁目", "7-8-9"),
        ),
        case(
            "日本大分県朝凪市青波区風見二丁目7-8-9",
            crate::Want::At((145.62, 43.0), 25., true),
        ),
        case(
            "日本大分県朝凪市晴風区霞通一丁目3-4",
            at_house("霞通一丁目", "3-4"),
        ),
        case(
            "日本大分県朝凪市青波区霞通一丁目3-5",
            crate::Want::At((145.51015, 43.0), 25., true),
        ),
        case(
            "日本大分県朝凪市晴風区浜道三丁目6-7",
            at_house("浜道三丁目", "6-7"),
        ),
        case(
            "日本大分県朝凪市青波区浜道三丁目6-8",
            crate::Want::At((145.51015, 43.0), 25., true),
        ),
        case(
            "大分県中津市海風一丁目2-3-4, Japan",
            at_house("海風一丁目", "2-3-4"),
        ),
        case(
            "日本大分県中津市海風1-2-3-4",
            at_house("海風一丁目", "2-3-4"),
        ),
        case(
            "日本大分県中津市桜９条東三丁目7-8-9",
            at_house("桜９条東三丁目", "7-8-9"),
        ),
        case(
            "〒123-4567 大分県中津市海風一丁目2番3-4号, Japan",
            at_house("海風一丁目", "2-3-4"),
        ),
        case(
            "大分県中津市海風一丁目2-3-999, Japan",
            town("jp-nakatsu", false),
        ),
        case(
            "大分県中津市海風一丁目2-3-4, Nakatsu 123-4567, Japan",
            at_house("海風一丁目", "2-3-4"),
        ),
        case(
            "大分県星雲郡夏見町日和二丁目6-7-8, Japan",
            at_house("日和二丁目", "6-7-8"),
        ),
        case("日本大分県中津市沖代町一丁目5番", town("jp-nakatsu", false)),
        case(
            "大分県中津市沖代町一丁目5番, Japan",
            town("jp-nakatsu", false),
        ),
        case(
            "日本大分県中津市沖代町一丁目5番999号",
            town("jp-nakatsu", false),
        ),
        case(
            "大分県中津市沖代町一丁目5番999号, Japan",
            town("jp-nakatsu", false),
        ),
    ])
    .collect()
}

#[test]
fn mlit_areas_are_not_residences_and_keep_selection_coordinates() {
    for (road, number, lon) in [
        ("真珠一丁目", "5", 145.5),
        ("凪通二丁目", "7", 145.51),
        ("次波四丁目", "9", 145.55),
    ] {
        for q in [
            format!("日本大分県中津市{road}{number}番"),
            format!("大分県中津市{road}{number}番, Japan"),
        ] {
            let found = index().suggest(&q, 5, Some("JP"), None, false);
            assert_eq!(found.len(), 1, "{q}");
            let s = &found[0];
            assert_eq!(s.kind, "street");
            assert!(s.id.starts_with('b'));
            assert!(metres((s.x, s.y), (lon, 41.)) < 5.);
            let retrieved = index().retrieve_suggestion(&s.retrieval_id).unwrap();
            assert_eq!(
                (retrieved.id, retrieved.kind, retrieved.x, retrieved.y),
                (s.id.clone(), "street", s.x, s.y)
            );
            let hits = index().search(&q);
            assert_eq!(hits.len(), 1);
            let value = index().render(&hits[0], true);
            assert!(value["address"].get("house_number").is_none());
            assert_eq!(value["address"]["road"], format!("{road}{number}番"));
        }
    }
    for q in [
        "大分県中津市凪通二丁目8番, Japan",
        "大分県中津市裂野三丁目8番, Japan",
        "大分県中津市砂丘一丁目14番, Japan",
        "大分県中津市混波六丁目13番, Japan",
        "朝雲県中津市朝露一丁目15番, Japan",
        "大分県中津市朝露一丁目15番, Japan",
        "大分県中津市真珠一丁目5番, France",
    ] {
        assert!(
            index()
                .suggest(q, 5, Some("JP"), None, false)
                .iter()
                .all(|s| s.kind != "address" && !s.id.starts_with('b')),
            "{q}"
        );
    }
    for (road, number, lon) in [
        ("真珠一丁目", "5-12", 145.52),
        ("遠波五丁目", "10-12", 145.54),
        ("遠霞五丁目", "12-13", 145.54),
    ] {
        let q = format!("大分県中津市{road}{number}, Japan");
        let found = index().suggest(&q, 5, Some("JP"), None, false);
        assert_eq!(found.len(), 1, "{q}");
        assert_eq!(found[0].kind, "address");
        assert!(metres((found[0].x, found[0].y), (lon, 41.)) < 5.);
    }
    assert!(
        index()
            .reverse_search(41., 145.62, 100., 5)
            .iter()
            .all(|(_, v, _)| v["address"].get("house_number").is_none())
    );
}

#[test]
fn ward_qualified_houses_need_their_own_source_ward() {
    let mut number_streets = vec![];
    let mut namesakes = vec![];
    for (ward, road, number, lon) in [
        ("朝村区", "朱明二丁目", "14-15", 145.51),
        ("晴風区", "風見二丁目", "7-8-9", 145.51),
        ("青波区", "風見二丁目", "7-8-9", 145.62),
        ("晴風区", "霞通一丁目", "3-4", 145.51),
        ("青波区", "霞通一丁目", "3-5", 145.51015),
        ("晴風区", "浜道三丁目", "6-7", 145.51),
        ("青波区", "浜道三丁目", "6-8", 145.51015),
    ] {
        for query in [
            format!("日本大分県朝凪市{ward}{road}{number}"),
            format!("大分県朝凪市{ward}{road}{number}, Japan"),
        ] {
            let found = index().suggest(&query, 5, Some("JP"), None, false);
            assert_eq!(found.len(), 1, "{query}");
            assert_eq!(
                (found[0].kind, found[0].country_code.as_str()),
                ("address", "jp")
            );
            assert!(metres((found[0].x, found[0].y), (lon, 43.)) < 5., "{query}");
            let hit = index().suggested_house(&found[0]).unwrap();
            assert_eq!(hit["address"]["house_number"], number);
            assert_eq!(hit["address"]["city"], "Asanagi");
            assert_eq!(hit["address"]["postal_town"], format!("朝凪市{ward}"));
            assert_eq!(hit["address"]["ISO3166-2-lvl4"], "JP-44");
            if road == "風見二丁目" {
                namesakes.push(found[0].id.split(':').next().unwrap().to_string());
            }
            if road == "霞通一丁目" {
                number_streets.push(found[0].id.split(':').next().unwrap().to_string());
            }
        }
    }
    assert!(number_streets.iter().all(|id| id == &number_streets[0]));
    assert_ne!(namesakes[0], namesakes[2]);
    let preferred = index().suggest(
        "大分県朝凪市夕凪区朝影一丁目10-11, Japan",
        5,
        None,
        None,
        false,
    );
    assert_eq!(preferred.len(), 1);
    assert!(metres((preferred[0].x, preferred[0].y), (146., 43.)) < 5.);
    for text in [
        "大分県朝凪市晴風区霞通一丁目3-5, Japan",
        "大分県朝凪市青波区霞通一丁目3-4, Japan",
        "大分県朝凪市晴風区浜道三丁目6-8, Japan",
        "大分県朝凪市青波区浜道三丁目6-7, Japan",
        "大分県朝凪市晴風区霧道一丁目8-9, Japan",
        "大分県朝凪市青波区霧道一丁目8-10, Japan",
        "大分県朝凪市晴風区波路一丁目4-5, Japan",
        "大分県朝凪市晴風区二重一丁目13-14, Japan",
        "大分県朝凪市晴風区重波一丁目11-12, Japan",
        "大分県朝凪市青波区重波一丁目11-12, Japan",
        "大分県朝凪市晴風区夕映一丁目9-10, Japan",
        "〒223-4456 大分県朝凪市晴風区浜道三丁目6-7, Japan",
        "大分県朝凪市晴風区風見二丁目7-8-9, France",
        "朝雲県朝凪市晴風区風見二丁目7-8-9, Japan",
    ] {
        assert!(
            !index()
                .suggest(text, 5, None, None, false)
                .iter()
                .any(|s| s.kind == "address"),
            "{text}"
        );
    }
}

#[test]
fn native_houses_keep_residence_numbers_numeric_streets_and_prefecture_scope() {
    for (query, road, number) in [
        ("日本大分県中津市沖代町一丁目5番9号", "沖代町一丁目", "5-9"),
        ("大分県中津市海風一丁目2-3-4, Japan", "海風一丁目", "2-3-4"),
        ("日本大分県中津市海風1-2-3-4", "海風一丁目", "2-3-4"),
        (
            "大分県中津市海風一丁目2-3101, Japan",
            "海風一丁目",
            "2-3101",
        ),
        (
            "大分県中津市海風一丁目2-3-4, Nakatsu 123-4567, Japan",
            "海風一丁目",
            "2-3-4",
        ),
        (
            "日本大分県中津市桜９条東三丁目7-8-9",
            "桜９条東三丁目",
            "7-8-9",
        ),
        (
            "〒123-4567 大分県中津市海風一丁目2番3-4号, Japan",
            "海風一丁目",
            "2-3-4",
        ),
    ] {
        let expected = house(street(road), number);
        for cc in [None, Some("JP")] {
            let found = index().suggest(query, 5, cc, None, false);
            assert_eq!(found.len(), 1, "{query}");
            assert_eq!(
                (found[0].kind, found[0].country_code.as_str()),
                ("address", "jp")
            );
            assert!(metres((found[0].x, found[0].y), expected) < 5., "{query}");
            let retrieved = index().suggested_house(&found[0]).unwrap();
            assert_eq!(retrieved["address"]["house_number"], number, "{query}");
            assert_eq!(retrieved["address"]["ISO3166-2-lvl4"], "JP-44", "{query}");
        }
        assert!(
            index()
                .suggest(query, 5, Some("FR"), None, false)
                .is_empty()
        );
    }
    let flat_house = index().search("大分県中津市海風1-6, Japan");
    assert_eq!(flat_house.len(), 1);
    assert_eq!(
        index().render(&flat_house[0], true)["address"]["house_number"],
        "1-6"
    );
    assert!(
        index()
            .search("大分県中津市海風一丁目6番, Japan")
            .is_empty()
    );
    let postal = index().suggest(
        "日本　〒１２３−４５６７ 大分県中津市海風一丁目2番3-4号",
        5,
        None,
        None,
        false,
    );
    assert_eq!(postal.len(), 1);
    assert_eq!(
        index().suggested_house(&postal[0]).unwrap()["address"]["house_number"],
        "2-3-4"
    );
    let county = index().suggest(
        "大分県星雲郡夏見町日和二丁目6-7-8, Japan",
        5,
        None,
        None,
        false,
    );
    assert_eq!(county.len(), 1);
    assert!(
        metres(
            (county[0].x, county[0].y),
            house(street("日和二丁目"), "6-7-8")
        ) < 5.
    );
    for query in [
        "大分県中津市海風一丁目2-3-999, Japan",
        "大分県中津市海風一丁目2-3 101, Japan",
        "〒999-9999 大分県中津市海風一丁目2-3-4, Japan",
        "日本　〒９９９ー９９９９ 大分県中津市海風一丁目2-3-4",
        "大分県中津市沖代町一丁目5-9, China",
        "大分県中津市海風一丁目2-3-4, Shadow Nakatsu 123-4567, Japan",
        "大分県中津市海風一丁目2-3-4, Nakatsu 999-9999, Japan",
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
