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

pub const STREETS: &[Street] = &[
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
    let flat_block = index().search("大分県中津市海風一丁目6番, Japan");
    assert_eq!(flat_block.len(), 1);
    assert_eq!(
        index().render(&flat_block[0], true)["address"]["house_number"],
        "1-6"
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
