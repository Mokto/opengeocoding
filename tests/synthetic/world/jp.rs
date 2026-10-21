use super::*;

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
];
