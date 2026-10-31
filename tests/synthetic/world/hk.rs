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
    place!(
        "hk-clear-water-bay",
        "locality",
        "village",
        "HK",
        "",
        1_000,
        (175.31, 15.0),
        "hk",
        "清水灣",
        &[("en", "Clear Water Bay")],
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
    Street {
        class: "residential",
        length: 0.0,
        name: "青松灣 Tsing Chung Wan",
        postcode: "",
        city: "Sai Kung",
        cc: "HK",
        lon: 175.31,
        lat: 15.0,
        numbers: &["72", "94B"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "翠湖路 Jade Lake Road",
        postcode: "",
        city: "Sai Kung",
        cc: "HK",
        lon: 175.315,
        lat: 15.0,
        numbers: &["青松商場42號鋪", "42"],
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
            "72 青松灣 Tsing Chung Wan, 清水灣 Clear Water Bay, Hong Kong",
            at_house("青松灣 Tsing Chung Wan", "72"),
        ),
        case(
            "青松灣 Tsing Chung Wan 72, 清水灣 Clear Water Bay, Hong Kong",
            at_house("青松灣 Tsing Chung Wan", "72"),
        ),
        case(
            "94B 青松灣 Tsing Chung Wan, 清水灣 Clear Water Bay, Hong Kong",
            at_house("青松灣 Tsing Chung Wan", "94B"),
        ),
        case(
            "青松灣 Tsing Chung Wan 94B, 清水灣 Clear Water Bay, Hong Kong",
            at_house("青松灣 Tsing Chung Wan", "94B"),
        ),
        case(
            "青松商場42號鋪 翠湖路 Jade Lake Road, Sai Kung, Hong Kong",
            at_house("翠湖路 Jade Lake Road", "青松商場42號鋪"),
        ),
        case(
            "翠湖路 Jade Lake Road 青松商場42號鋪, Sai Kung, Hong Kong",
            at_house("翠湖路 Jade Lake Road", "青松商場42號鋪"),
        ),
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

#[test]
fn shop_identifiers_are_not_street_house_numbers() {
    let street = street("翠湖路 Jade Lake Road");
    for query in [
        "青松商場42號鋪 翠湖路 Jade Lake Road, Sai Kung, Hong Kong",
        "翠湖路 Jade Lake Road 青松商場42號鋪, Sai Kung, Hong Kong",
    ] {
        let found = index().suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(metres((found[0].x, found[0].y), house(street, "青松商場42號鋪")) < 5.);
        assert!(metres((found[0].x, found[0].y), house(street, "42")) > 10.);
    }
}

pub async fn check_bilingual_safeguards(resolver: &Resolver) {
    for query in [
        "72 青松灣 Tsing Chung Wan, 清水灣 Clear Water Bay, Hong Kong",
        "青松灣 Tsing Chung Wan 72, 清水灣 Clear Water Bay, Hong Kong",
        "94B 青松灣 Tsing Chung Wan, 清水灣 Clear Water Bay, Hong Kong",
        "青松灣 Tsing Chung Wan 94B, 清水灣 Clear Water Bay, Hong Kong",
    ] {
        let parsed = resolver.parse(query).await.unwrap();
        assert_eq!(parsed["country"], "hong kong", "{query}: {parsed:?}");
        assert_eq!(
            parsed["city"], "清水灣 clear water bay",
            "{query}: {parsed:?}"
        );
        assert_eq!(
            parsed["road"], "青松灣 tsing chung wan",
            "{query}: {parsed:?}"
        );
    }
    for query in [
        "青松商場42號鋪 翠湖路 Jade Lake Road, Sai Kung, Hong Kong",
        "翠湖路 Jade Lake Road 青松商場42號鋪, Sai Kung, Hong Kong",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(found.good_match, "{query}: {found:?}");
        assert_eq!(
            found.address_details.as_option().unwrap().house_number,
            "青松商場42號鋪"
        );
    }
    for query in [
        "72 青松灣 Tsing Chung Wan, 清水灣 Clear Water Bay, France",
        "青松灣 Tsing Chung Wan 94B, 清水灣 Clear Water Bay, France",
        "青松商場42號鋪 翠湖路 Jade Lake Road, Sai Kung, France",
        "青松商場43號鋪 翠湖路 Jade Lake Road, Sai Kung, Hong Kong",
    ] {
        let found = resolver.resolve(query).await.unwrap();
        assert!(!found.good_match, "{query}: {found:?}");
    }
}

#[test]
fn hong_kong_suggestions_read_whole_addresses_and_chinese_numbers() {
    for (query, lon) in [
        ("27 KO FONG STREET, HONG KONG", 175.005),
        (
            "72 青松灣 Tsing Chung Wan, 清水灣 Clear Water Bay, Hong Kong",
            175.31,
        ),
        (
            "青松灣 Tsing Chung Wan 94B, 清水灣 Clear Water Bay, Hong Kong",
            175.31,
        ),
        (
            "27 Ko Fong Street, Kwai Chung, Kwai Tsing District, New Territories, Hong Kong",
            175.005,
        ),
        ("Flat A, 5/F, 27 Ko Fong Street, Kwai Chung, N.T.", 175.005),
        (
            "28 TIN SAM STREET, TAI WAI, SHA TIN DISTRICT, NEW TERRITORIES, HONG KONG",
            175.12,
        ),
        (
            "185 NGAU LIU, SAI KUNG, SAI KUNG DISTRICT, NEW TERRITORIES, HONG KONG",
            175.305,
        ),
        ("香港新界葵青區葵涌高芳街27號", 175.005),
        ("高芳街27號", 175.005),
        ("香港新界葵青區高芳街2", 175.005),
    ] {
        let found = index().suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert!(
            metres((found[0].x, found[0].y), (lon, 15.)) < 200.,
            "{query}"
        );
    }
    // Tai Wai is 11 km from Ko Fong Street
    let found = index().suggest(
        "27 KO FONG STREET, TAI WAI, HONG KONG",
        5,
        None,
        None,
        false,
    );
    assert!(
        found.iter().all(|s| s.kind != "address"),
        "{:?}",
        found.first().map(|s| &s.display_name)
    );
}
