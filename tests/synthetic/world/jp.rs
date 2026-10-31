use super::*;
use crate::{Case, at_house, case, town};

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
];

pub fn cases() -> Vec<Case> {
    [
        "日本大分県中津市沖代町一丁目5番9号",
        "大分県中津市沖代町一丁目5番9号, Japan",
    ]
    .into_iter()
    .map(|q| case(q, at_house("沖代町一丁目", "5-9")))
    .chain([
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
