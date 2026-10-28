use super::*;
use crate::*;

// Hong Kong's districts hold areas (Kwai Chung, Tai Wai) that the data files houses under; the
// territory itself is written where the town goes, and its regions (Kowloon, New Territories)
// name no place.
pub const PLACES: &[Place] = &[
    place!(
        "hk",
        "dependency",
        "",
        "HK",
        "",
        7_000_000,
        (175.0, 15.0),
        "",
        "香港",
        &[("en", "Hong Kong")],
    ),
    place!(
        "hk-kwai-tsing",
        "county",
        "",
        "HK",
        "",
        500_000,
        (175.0, 15.0),
        "hk",
        "葵青區",
        &[("en", "Kwai Tsing District")],
    ),
    place!(
        "hk-kwai-chung",
        "locality",
        "town",
        "HK",
        "",
        300_000,
        (175.0, 15.0),
        "hk-kwai-tsing",
        "葵涌",
        &[("en", "Kwai Chung")],
    ),
    place!(
        "hk-central-western",
        "county",
        "",
        "HK",
        "",
        250_000,
        (175.2, 15.0),
        "hk",
        "中西區",
        &[("en", "Central and Western District")],
    ),
    place!(
        "hk-sha-tin",
        "county",
        "",
        "HK",
        "",
        650_000,
        (175.1, 15.0),
        "hk",
        "沙田區",
        &[("en", "Sha Tin District")],
    ),
    place!(
        "hk-tai-wai",
        "locality",
        "town",
        "HK",
        "",
        100_000,
        (175.1, 15.0),
        "hk-sha-tin",
        "大圍",
        &[("en", "Tai Wai")],
    ),
    place!(
        "hk-sha-tin-town",
        "locality",
        "town",
        "HK",
        "",
        200_000,
        (175.125, 15.0),
        "hk-sha-tin",
        "沙田",
        &[("en", "Sha Tin")],
    ),
    place!(
        "hk-sai-kung",
        "locality",
        "town",
        "HK",
        "",
        40_000,
        (175.3, 15.0),
        "hk",
        "西貢",
        &[("en", "Sai Kung")],
    ),
];

pub const STREETS: &[Street] = &[
    // filed under Kwai Chung, in English and in Chinese
    Street {
        class: "residential",
        length: 0.0,
        name: "KO FONG STREET",
        postcode: "",
        city: "Kwai Chung",
        cc: "HK",
        lon: 175.005,
        lat: 15.0,
        numbers: &["27"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "高芳街",
        postcode: "",
        city: "Kwai Chung",
        cc: "HK",
        lon: 175.005,
        lat: 15.0,
        numbers: &["27"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "ELGIN STREET",
        postcode: "",
        city: "",
        cc: "HK",
        lon: 175.2,
        lat: 15.0,
        numbers: &["29"],
    },
    // a village whose houses are numbered on its name
    Street {
        class: "residential",
        length: 0.0,
        name: "NGAU LIU",
        postcode: "",
        city: "Sai Kung",
        cc: "HK",
        lon: 175.305,
        lat: 15.0,
        numbers: &["185"],
    },
    // written as in Tai Wai, 2 km off, but nearer Sha Tin, which the data files it under
    Street {
        class: "residential",
        length: 0.0,
        name: "TIN SAM STREET",
        postcode: "",
        city: "",
        cc: "HK",
        lon: 175.12,
        lat: 15.0,
        numbers: &["28"],
    },
];

pub fn cases() -> Vec<Case> {
    let ko_fong = at_house("KO FONG STREET", "27");
    vec![
        case("27 KO FONG STREET, HONG KONG", ko_fong),
        case(
            "27 Ko Fong Street, Kwai Chung, Kwai Tsing District, New Territories, Hong Kong",
            at_house("KO FONG STREET", "27"),
        ),
        case("香港新界葵青區葵涌高芳街27號", at_house("高芳街", "27")),
        case("香港高芳街27號", at_house("高芳街", "27")),
        case(
            "29 ELGIN STREET, CENTRAL & WESTERN DISTRICT, HONG KONG",
            at_house("ELGIN STREET", "29"),
        ),
        case(
            "185 NGAU LIU, SAI KUNG, SAI KUNG DISTRICT, NEW TERRITORIES, HONG KONG",
            at_house("NGAU LIU", "185"),
        ),
        case(
            "28 TIN SAM STREET, TAI WAI, SHA TIN DISTRICT, NEW TERRITORIES, HONG KONG",
            at_house("TIN SAM STREET", "28"),
        ),
        // Tai Wai is 11 km from Ko Fong Street: the area does not hold that house
        case(
            "27 KO FONG STREET, TAI WAI, HONG KONG",
            town("hk-tai-wai", false),
        ),
    ]
}
