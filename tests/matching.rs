use opengeocoding::{
    matching::*,
    proto::opengeocoding::*,
    regions::{Regions, format_regions, python_repr},
    resolver::format_response,
};
use serde_json::json;

fn parsed(values: &[(&str, &str)]) -> ParsedAddress {
    values
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn candidate(country: &str, city: &str, state: &str, postcode: &str) -> FullAddress {
    FullAddress {
        address_details: AddressDetails {
            country_code: country.into(),
            city: city.into(),
            state: state.into(),
            postcode: postcode.into(),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    }
}

#[test]
fn multilingual_normalization() {
    for (input, expected) in [
        ("München", "munchen"),
        ("São-Paulo", "sao paulo"),
        ("Straße", "strasse"),
        ("Łódź", "lodz"),
        ("København", "kobenhavn"),
        ("Αθήνα", "athena"),
        ("Москва", "moskva"),
        ("東京", "dong jing"),
        ("  Paris\u{1c}France ", "paris france"),
    ] {
        assert_eq!(normalize_place(input), expected, "{input}");
    }
}

#[test]
fn public_country_names_and_regions() {
    for (name, code) in [
        ("France", "fr"),
        ("Deutschland", "de"),
        ("USA", "us"),
        ("日本", "jp"),
        ("Danmark", "dk"),
    ] {
        assert_eq!(
            expected_country(&parsed(&[("country", name)])),
            code,
            "{name}"
        );
    }
    assert_eq!(implied_country(&parsed(&[("state", "Texas")])), "us");
    assert_eq!(implied_country(&parsed(&[("state", "Ontario")])), "ca");
    assert_eq!(implied_country(&parsed(&[("state", "Georgia")])), "");
}

#[test]
fn postcode_precision() {
    for (expected, actual, compatible) in [
        ("E14 5HQ", "E14", true),
        ("E14 5HQ", "E14 9ZZ", true),
        ("E14 5HQ", "E15 9ZZ", false),
        ("08019", "08019-1234", true),
        ("08019", "08930", false),
        ("D02 AF30", "D02", true),
        ("D02 AF30", "D02 AB12", false),
        ("D02 AF30", "D04 AF30", false),
        ("", "08019", false),
        ("08019", "", false),
        ("1234 AB", "1234", true),
        ("K1A 0B1", "K1A", true),
    ] {
        assert_eq!(postcodes_compatible(expected, actual), compatible);
        assert_eq!(postcodes_compatible(actual, expected), compatible);
    }
}

#[test]
fn eircode_control_separators() {
    for separator in [' ', '\u{1c}', '\u{1d}', '\u{1e}', '\u{1f}'] {
        let code = format!("D02{separator}AF30");
        assert_eq!(eircode(&format!("Dublin {code} Ireland")), code);
    }
}

#[test]
fn stated_country_cannot_override_corroborated_region() {
    let address = parsed(&[("country", "France"), ("state", "Texas"), ("city", "Paris")]);
    let candidates = vec![
        candidate("fr", "Paris", "Île-de-France", ""),
        candidate("us", "Paris", "Texas", ""),
    ];
    assert_eq!(
        choose_candidate(&address, "fr", "", &candidates),
        Some((1, "city_region"))
    );
}

#[test]
fn country_conflict_without_region_agreement_is_rejected() {
    let address = parsed(&[("country", "France"), ("state", "Texas"), ("city", "Paris")]);
    assert_eq!(
        choose_candidate(&address, "fr", "", &[candidate("fr", "Paris", "", "")]),
        None
    );
}

#[test]
fn country_postcode_conflict_is_rejected() {
    let address = parsed(&[("country", "France"), ("city", "Paris")]);
    assert_eq!(
        choose_candidate(
            &address,
            "fr",
            "75001",
            &[candidate("us", "Paris", "Texas", "75460")]
        ),
        None
    );
}

#[test]
fn mistyped_postcode_is_overruled_by_house_road_and_town() {
    let address = parsed(&[
        ("house_number", "312"),
        ("road", "Lowell Street"),
        ("city", "Lexington"),
        ("postcode", "02423"),
    ]);
    let mut house = candidate("us", "Lexington", "Massachusetts", "02421");
    let details = house.address_details.get_or_insert_default();
    details.road = "Lowell Street".into();
    details.house_number = "312".into();
    assert_eq!(
        choose_candidate(&address, "", "02423", std::slice::from_ref(&house)).map(|(i, _)| i),
        Some(0)
    );
    // a candidate holding the postcode still comes first
    let other = candidate("us", "Lexington", "Massachusetts", "02423");
    assert_eq!(
        choose_candidate(&address, "", "02423", &[house.clone(), other]).map(|(i, _)| i),
        Some(1)
    );
    // another house number on the road is no match
    house.address_details.get_or_insert_default().house_number = "310".into();
    assert_eq!(choose_candidate(&address, "", "02423", &[house]), None);
}

#[test]
fn postcode_missing_is_eligible_for_backfill() {
    assert!(postcode_matches("75001", Some(&AddressDetails::default())));
    assert!(!postcode_agrees("75001", Some(&AddressDetails::default())));
}

#[test]
fn fallback_query_order_is_postcode_first() {
    let address = parsed(&[
        ("country", "France"),
        ("city", "Paris"),
        ("postcode", "75001"),
        ("road", "Rue A"),
    ]);
    assert_eq!(
        create_query(&address, &["postcode", "city", "country"]),
        "75001, Paris, France"
    );
    assert_eq!(
        without_country("Paris, France, 75001", &address),
        "Paris, 75001"
    );
    assert_eq!(without_country("Paris, 75001", &address), "");
}

#[test]
fn index_response_and_region_code_filtering() {
    let result = format_response(&json!({
        "display_name": "Paris, France", "lat": "48.8566", "lon": "2.3522",
        "address": { "city": "Paris", "country_code": "fr", "unknown_key": "ignored",
            "ISO3166-2-lvl4": "FR-IDF", "ISO3166-2-lvl12": "FR-IGNORED", "ISO3166-2-lvl6": "RS-KM" }
    }))
    .unwrap();
    assert_eq!(result.address_details.city, "Paris");
    assert_eq!(result.address_details.region_codes.len(), 1);
    assert_eq!(result.address_details.region_codes[0].code, "FR-IDF");
}

#[test]
fn region_hierarchy_and_python_representation() {
    let rows = vec![
        Region {
            country_code: "dk".into(),
            country_name: "Denmark".into(),
            admin_code: "84".into(),
            admin_level: 4,
            local_name: "Hovedstaden".into(),
            en_name: "Capital Region".into(),
            ..Default::default()
        },
        Region {
            country_code: "dk".into(),
            country_name: "Denmark".into(),
            admin_code: "84_101".into(),
            admin_level: 6,
            local_name: "København".into(),
            en_name: "Copenhagen".into(),
            parent_admin_code: "DK-84".into(),
            ..Default::default()
        },
    ];
    let tree = format_regions(&rows).unwrap();
    let representation = python_repr(&tree).unwrap();
    assert!(representation.contains("Denmark"));
    assert!(representation.contains("København"));
    assert_eq!(
        python_repr(&json!({"key": "line\n\u{1c}"})).unwrap(),
        "{'key': 'line\\n\\x1c'}"
    );
}

#[test]
fn regions_come_from_codes_and_serve_one_stable_map() {
    let region = |country: &str, code: &str, level: u32, name: &str| {
        (
            format!("{}-{code}", country.to_uppercase()),
            Region {
                country_code: country.into(),
                country_name: country.to_uppercase(),
                admin_code: code.into(),
                admin_level: level,
                local_name: name.into(),
                en_name: name.into(),
                ..Default::default()
            },
        )
    };
    let code = |code: &str, admin_level: u32| ISO3166_2 {
        code: code.into(),
        admin_level,
        ..Default::default()
    };
    let rows = vec![
        region("dk", "84", 4, "Hovedstaden"),
        region("ie", "D", 4, "County Dublin"),
        region("dk", "82", 4, "Midtjylland"),
    ];
    let regions = Regions::from_rows(rows.clone()).unwrap();
    assert_eq!(regions.len(), 3);
    let found = regions.reformat(&[code("DK-84", 4), code("DK-XX", 5), code("dk-82", 3)]);
    // lowest admin level first; a code with no region is left out; case does not matter
    assert_eq!(
        found
            .iter()
            .map(|r| r.local_name.as_str())
            .collect::<Vec<_>>(),
        ["Midtjylland", "Hovedstaden"]
    );
    // the map does not depend on the order the index listed the regions in
    let reversed = Regions::from_rows(rows.into_iter().rev().collect()).unwrap();
    assert_eq!(regions.map(), reversed.map());
    assert_eq!(regions.list(), reversed.list());
    assert_eq!(
        regions
            .list()
            .iter()
            .map(|r| (r.country_code.as_str(), r.admin_code.as_str()))
            .collect::<Vec<_>>(),
        [("dk", "82"), ("dk", "84"), ("ie", "D")]
    );
    assert!(
        regions
            .map()
            .starts_with("{'dk': {'country_name': 'DK', '82': ")
    );
}

#[test]
fn place_names_agree_across_qualifiers_spelling_and_script() {
    for (expected, actual, agree) in [
        ("Valga", "Valga city", true),
        ("Jakarta", "Special Capital Region of Jakarta", true),
        ("Hyderabad", "Hyderabad City Taluka", true),
        ("Cambridge", "Town of Cambridge", true),
        ("Århus", "Aarhus", true),
        ("Hanoi", "Hà Nội", true),
        ("Baarle-Nassau", "Baarle Nassau", true),
        ("大阪", "大阪市", true),
        ("京都", "京都府", true),
        ("上海", "上海市", true),
        ("大阪", "大坂", false),
        ("上海", "Shanghai", false),
        ("Capital Region", "Region", false),
        ("Paris", "Parish", false),
        ("Kansas", "Arkansas", false),
    ] {
        assert_eq!(
            names_agree(expected, actual),
            agree,
            "{expected} vs {actual}"
        );
    }
}

#[test]
fn province_corroborates_region() {
    let address = parsed(&[("state", "Ontario"), ("city", "London")]);
    let details = AddressDetails {
        city: "London".into(),
        province: "Ontario".into(),
        country_code: "ca".into(),
        ..Default::default()
    };
    assert!(region_agrees(&address, Some(&details)));
}

#[test]
fn postcode_and_a_place_above_the_locality_corroborate_the_town() {
    let at = |state: &str, suburb: &str, postcode: &str| FullAddress {
        address_details: AddressDetails {
            country_code: "de".into(),
            city: "Mitte".into(),
            state: state.into(),
            suburb: suburb.into(),
            postcode: postcode.into(),
            ..Default::default()
        }
        .into(),
        ..Default::default()
    };
    let berlin = parsed(&[
        ("road", "unter den linden"),
        ("postcode", "10117"),
        ("city", "berlin"),
    ]);
    let house = at("Berlin", "", "10117");
    assert!(!city_agrees(&berlin, house.address_details.as_option()));
    assert!(postcode_place_agrees(
        &berlin,
        "10117",
        house.address_details.as_option()
    ));
    // the postcode must be the stated one: a same-named state alone is no evidence
    assert!(!postcode_place_agrees(
        &berlin,
        "10117",
        at("Berlin", "", "12043").address_details.as_option()
    ));
    assert!(!postcode_place_agrees(
        &berlin,
        "",
        house.address_details.as_option()
    ));
    assert!(!postcode_place_agrees(
        &berlin,
        "10117",
        at("Bayern", "", "10117").address_details.as_option()
    ));
    // a Danish postal district letter is not part of the town
    let copenhagen = parsed(&[("postcode", "1165"), ("city", "københavn k")]);
    let k = at("Region Hovedstaden", "København", "1165");
    assert!(postcode_place_agrees(
        &copenhagen,
        "1165",
        k.address_details.as_option()
    ));
    assert!(!postcode_place_agrees(
        &parsed(&[("city", "københavn k")]),
        "1165",
        at("", "Aarhus", "1165").address_details.as_option()
    ));
}
