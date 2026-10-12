use geolocation::{
    matching::*,
    proto::geolocation::*,
    regions::{format_regions, python_repr},
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
fn nominatim_response_and_region_code_filtering() {
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
