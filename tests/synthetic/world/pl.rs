use super::*;
use crate::{Case, Want, at_house, case, street};

pub const REVERSE_PLACES: &[Place] = &[
    place!(
        "pl-reverse-west-region",
        "region",
        "",
        "PL",
        "PL-04",
        100000,
        (20.980000, 52.000000),
        "pl",
        "Test West",
        &[]
    ),
    place!(
        "pl-reverse-east-region",
        "region",
        "",
        "PL",
        "PL-08",
        100000,
        (21.020000, 52.000000),
        "pl",
        "Test East",
        &[]
    ),
    place!(
        "pl-reverse-west",
        "locality",
        "village",
        "PL",
        "PL-04",
        1000,
        (20.990000, 52.000000),
        "pl-reverse-west-region",
        "Brookmere",
        &[]
    ),
    place!(
        "pl-reverse-east",
        "locality",
        "village",
        "PL",
        "PL-04",
        1000,
        (21.010000, 52.000000),
        "pl-reverse-east-region",
        "Pineward",
        &[]
    ),
    place!(
        "pl-reverse-closer",
        "locality",
        "hamlet",
        "PL",
        "PL-04",
        10,
        (20.999800, 52.000000),
        "pl-reverse-west-region",
        "Falsebrook",
        &[]
    ),
    place!(
        "pl-reverse-namesake",
        "locality",
        "village",
        "PL",
        "PL-08",
        1000000,
        (21.030000, 52.000000),
        "pl-reverse-east-region",
        "Brookmere",
        &[]
    ),
];

pub const REVERSE_POINT_ONLY: &[&str] = &[
    "pl-reverse-west",
    "pl-reverse-east",
    "pl-reverse-closer",
    "pl-reverse-namesake",
];

pub const REVERSE_AREAS: &[(&str, &str, bool, bool, f64, f64, f64)] = &[
    (
        "pl-reverse-west-region",
        "region",
        true,
        true,
        20.980000,
        52.0,
        0.02,
    ),
    (
        "pl-reverse-east-region",
        "region",
        true,
        true,
        21.020000,
        52.0,
        0.02,
    ),
    (
        "pl-reverse-west",
        "locality",
        true,
        true,
        20.990000,
        52.0,
        0.01,
    ),
    (
        "pl-reverse-east",
        "locality",
        true,
        true,
        21.010000,
        52.0,
        0.01,
    ),
];

pub const REVERSE_STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Wiejska West",
        postcode: "86-001",
        city: "Dispatch",
        cc: "PL",
        lon: 20.999900,
        lat: 52.0,
        numbers: &["17B/19"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Wiejska East",
        postcode: "86-001",
        city: "Brookmere",
        cc: "PL",
        lon: 21.000100,
        lat: 52.0,
        numbers: &["23A/25"],
    },
];

#[test]
fn pl_reverse_house_locality_is_not_the_nearest_or_postal_name() {
    let west = house(crate::street("Wiejska West"), "17B/19");
    let nearest = crate::index().reverse(west.1, west.0);
    assert_eq!(nearest["address"]["village"], "Falsebrook");
    let found = crate::index().reverse_search(west.1, west.0, 2.0, 1);
    assert_eq!(found.len(), 1);
    let (layer, value, _) = &found[0];
    assert_eq!(*layer, "address");
    assert_eq!(value["address"]["village"], "Brookmere");
    assert_eq!(value["address"]["postal_town"], "Dispatch");
    assert_eq!(value["address"]["country_code"], "pl");
    assert_eq!(value["address"]["ISO3166-2-lvl4"], "PL-04");
    assert_eq!(value["address"]["house_number"], "17b/19");
    assert_eq!(value["address"]["road"], "Wiejska West");
    assert_eq!(value["address"]["postcode"], "86001");
    let at = (
        value["lon"].as_str().unwrap().parse().unwrap(),
        value["lat"].as_str().unwrap().parse().unwrap(),
    );
    assert!(crate::metres(at, west) < 0.2);
    assert!(
        !value["display_name"]
            .as_str()
            .unwrap()
            .contains("Falsebrook")
    );
}

#[test]
fn pl_reverse_offset_crosses_the_boundary_without_renaming_either_house() {
    let west = house(crate::street("Wiejska West"), "17B/19");
    let east = house(crate::street("Wiejska East"), "23A/25");
    let offset = west.0 + 25.0 / (111_195.0 * west.1.to_radians().cos());
    let found = crate::index().reverse_search(west.1, offset, 100.0, 1);
    let at_east = crate::index().reverse_search(east.1, east.0, 2.0, 1);
    assert_eq!(found[0].1, at_east[0].1);
    let value = &found[0].1;
    assert_eq!(value["address"]["village"], "Pineward");
    assert_eq!(value["address"]["postal_town"], "Brookmere");
    assert_eq!(value["address"]["ISO3166-2-lvl4"], "PL-08");
    assert_eq!(value["address"]["country_code"], "pl");
    assert_eq!(value["address"]["house_number"], "23a/25");
    assert_eq!(value["address"]["road"], "Wiejska East");
    assert_eq!(value["address"]["postcode"], "86001");
    let at = (
        value["lon"].as_str().unwrap().parse().unwrap(),
        value["lat"].as_str().unwrap().parse().unwrap(),
    );
    assert!(crate::metres(at, east) < 0.2);
}

pub const MEMBERSHIP_PLACES: &[Place] = &[
    place!(
        "pl-source-birch",
        "locality",
        "village",
        "PL",
        "PL-04",
        100,
        (20.991, 52.0),
        "pl-reverse-west",
        "Birchfield",
        &[]
    ),
    place!(
        "pl-source-birch-other-region",
        "locality",
        "village",
        "PL",
        "PL-08",
        1000000,
        (21.031, 52.0),
        "pl-reverse-east",
        "Birchfield",
        &[]
    ),
    place!(
        "pl-source-twin-a",
        "locality",
        "village",
        "PL",
        "PL-04",
        100,
        (20.992, 52.0),
        "pl-reverse-west",
        "Twinfield",
        &[]
    ),
    place!(
        "pl-source-twin-b",
        "locality",
        "village",
        "PL",
        "PL-04",
        200,
        (20.993, 52.0),
        "pl-reverse-west",
        "Twinfield",
        &[]
    ),
    place!(
        "pl-source-else",
        "locality",
        "village",
        "PL",
        "PL-04",
        100,
        (20.994, 52.0),
        "pl-reverse-west",
        "Elsefield",
        &[]
    ),
];
pub const MEMBERSHIP_POINT_ONLY: &[&str] = &[
    "pl-source-birch",
    "pl-source-birch-other-region",
    "pl-source-twin-a",
    "pl-source-twin-b",
    "pl-source-else",
];
pub const MEMBERSHIP_STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Missing Level",
        postcode: "86-001",
        city: "Birchfield",
        cc: "PL",
        lon: 20.9999,
        lat: 51.996,
        numbers: &["44B/46"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Inconsistent Province",
        postcode: "86-001",
        city: "Birchfield",
        cc: "PL",
        lon: 20.9999,
        lat: 51.995,
        numbers: &["48B/50"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Recovery",
        postcode: "86-001",
        city: "Birchfield",
        cc: "PL",
        lon: 20.9999,
        lat: 52.003,
        numbers: &["12B/14"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Existing",
        postcode: "86-001",
        city: "Brookmere",
        cc: "PL",
        lon: 20.9999,
        lat: 52.004,
        numbers: &["16B/18"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Postal Only",
        postcode: "86-001",
        city: "Birchfield",
        cc: "PL",
        lon: 20.9999,
        lat: 52.005,
        numbers: &["20B/22"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Wrong Region",
        postcode: "86-001",
        city: "Birchfield",
        cc: "PL",
        lon: 20.9999,
        lat: 52.006,
        numbers: &["24B/26"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Ambiguous",
        postcode: "86-001",
        city: "Twinfield",
        cc: "PL",
        lon: 20.9999,
        lat: 52.007,
        numbers: &["28B/30"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Conflict",
        postcode: "86-001",
        city: "Birchfield",
        cc: "PL",
        lon: 20.9999,
        lat: 52.008,
        numbers: &["32B/34"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Conflict",
        postcode: "86-001",
        city: "Elsefield",
        cc: "PL",
        lon: 20.9999,
        lat: 52.008,
        numbers: &["32B/34"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Unsupported",
        postcode: "86-001",
        city: "Birchfield",
        cc: "PL",
        lon: 20.9999,
        lat: 52.009,
        numbers: &["36B/38"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Averaged",
        postcode: "86-001",
        city: "Birchfield",
        cc: "PL",
        lon: 20.99986,
        lat: 51.997,
        numbers: &["40B/42"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "PRG Averaged",
        postcode: "86-001",
        city: "Birchfield",
        cc: "PL",
        lon: 20.9999,
        lat: 51.997,
        numbers: &["40B/42"],
    },
];

pub fn levels(s: &Street) -> Vec<&str> {
    if !s.name.starts_with("PRG ") {
        return vec![s.city];
    }
    if s.name == "PRG Missing Level" {
        return vec!["Test County", s.city];
    }
    let province = if matches!(s.name, "PRG Wrong Region" | "PRG Inconsistent Province") {
        "lubuskie"
    } else {
        "kujawsko-pomorskie"
    };
    if s.name == "PRG Postal Only" {
        vec!["Test County", province]
    } else {
        vec!["Test County", province, s.city]
    }
}

pub fn postal_name(s: &Street) -> &str {
    if s.name.starts_with("PRG ") && s.name != "PRG Postal Only" {
        "Dispatch"
    } else {
        s.city
    }
}

pub fn source<'a>(s: &Street, id: &'a str) -> Option<[&'a str; 7]> {
    if s.cc != "PL" || !s.name.starts_with("PRG ") {
        return None;
    }
    Some([
        if s.name == "PRG Unsupported" {
            "postal_provider"
        } else {
            "open_addresses"
        },
        if s.name == "PRG Wrong Region" {
            "pl/lubuskie"
        } else {
            "pl/kujawsko-pomorskie"
        },
        "authored-capture",
        "",
        "LicenseRef-Public-Domain-PL-Geodesy-Law",
        id,
        "2026-01-01T00:00:00Z",
    ])
}

#[test]
fn polish_reverse_uses_the_selected_houses_geographic_source_not_its_postcode_or_nearest_point() {
    for (road, number, want) in [
        ("PRG Recovery", "12B/14", "Birchfield"),
        ("PRG Existing", "16B/18", "Brookmere"),
        ("PRG Postal Only", "20B/22", "Brookmere"),
        ("PRG Wrong Region", "24B/26", "Brookmere"),
        ("PRG Ambiguous", "28B/30", "Brookmere"),
        ("PRG Conflict", "32B/34", "Brookmere"),
        ("PRG Unsupported", "36B/38", "Brookmere"),
        ("PRG Missing Level", "44B/46", "Brookmere"),
        ("PRG Inconsistent Province", "48B/50", "Brookmere"),
    ] {
        let point = house(crate::street(road), number);
        let found = crate::index().reverse_search(point.1, point.0, 2.0, 1);
        assert_eq!(found.len(), 1, "{road}");
        let value = &found[0].1;
        assert_eq!(value["address"]["village"], want, "{road}: {value}");
        assert_eq!(
            value["address"]["house_number"],
            number.to_lowercase(),
            "{road}"
        );
        assert_eq!(value["address"]["road"], road, "{road}");
        assert_eq!(value["address"]["postcode"], "86001", "{road}");
        assert_eq!(value["address"]["country_code"], "pl", "{road}");
        assert_eq!(value["address"]["ISO3166-2-lvl4"], "PL-04", "{road}");
        let at = (
            value["lon"].as_str().unwrap().parse().unwrap(),
            value["lat"].as_str().unwrap().parse().unwrap(),
        );
        assert!(crate::metres(at, point) < 0.2, "{road}");
        if road == "PRG Recovery" {
            assert_eq!(value["address"]["postal_town"], "Dispatch");
            assert!(
                !value["display_name"]
                    .as_str()
                    .unwrap()
                    .contains("Brookmere")
            );
            let nearest = crate::index().reverse(point.1, point.0);
            assert_eq!(nearest["address"]["village"], "Falsebrook");
        }
    }
    let source = house(crate::street("PRG Averaged"), "40B/42");
    let found = crate::index().reverse_search(source.1, source.0, 10.0, 1);
    assert_eq!(found[0].1["address"]["village"], "Brookmere");
    assert_eq!(found[0].1["address"]["house_number"], "40b/42");
}

pub const PLACES: &[Place] = &[
    place!(
        "pl",
        "country",
        "",
        "PL",
        "",
        5_000_000,
        (19.0, 52.0),
        "",
        "Poland",
        &[("en", "Poland")]
    ),
    place!(
        "pl-damnica",
        "locality",
        "village",
        "PL",
        "",
        2_000,
        (17.3, 54.5),
        "pl",
        "Damnica",
        &[("pl", "Damnica")]
    ),
    // a city whose streets the data files under its districts, themselves localities
    place!(
        "pl-stolica",
        "locality",
        "city",
        "PL",
        "",
        1_800_000,
        (18.0, 53.0),
        "pl",
        "Stolica",
        &[("pl", "Stolica")]
    ),
    place!(
        "pl-centrum",
        "locality",
        "",
        "PL",
        "",
        100_000,
        (18.01, 53.0),
        "pl-stolica",
        "Centrum",
        &[("pl", "Centrum")]
    ),
];

pub const STREETS: &[Street] = &[
    // a village whose houses are numbered on its name: libpostal reads "Świtały 12" as a venue
    Street {
        class: "residential",
        length: 0.0,
        name: "Świtały",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.33,
        lat: 54.5,
        numbers: &["12"],
    },
    // written "ul. M. Kopernika": with "ul." (ulica) and the given name's initial
    Street {
        class: "residential",
        length: 0.0,
        name: "Mikołaja Kopernika",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.39,
        lat: 54.5,
        numbers: &["11"],
    },
    Street {
        class: "residential",
        length: 0.01,
        name: "ulica Szeroka",
        postcode: "",
        city: "Centrum",
        cc: "PL",
        lon: 18.01,
        lat: 53.0,
        numbers: &["7"],
    },
    // libpostal leaves "20BS" in the road
    Street {
        class: "residential",
        length: 0.0,
        name: "Rakowice Małe",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.36,
        lat: 54.5,
        numbers: &["20bs"],
    },
    // the postcode's centre is nearer the longer name that also holds house 10
    Street {
        class: "residential",
        length: 0.0,
        name: "Wybudowanie pod Jaszcz",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.3,
        lat: 54.5,
        numbers: &["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Jaszcz",
        postcode: "76-231",
        city: "Damnica",
        cc: "PL",
        lon: 17.3,
        lat: 54.52,
        numbers: &["10"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case("Świtały 12, 76-231, Poland", at_house("Świtały", "12")),
        case("Jaszcz 10, 76-231, Poland", at_house("Jaszcz", "10")),
        case(
            "ul. M. Kopernika 11, 76-231 Damnica, Poland",
            at_house("Mikołaja Kopernika", "11"),
        ),
        case(
            "ul. Mikołaja Kopernika 11, 76-231 Damnica",
            at_house("Mikołaja Kopernika", "11"),
        ),
        case("Szeroka 7, Stolica, Poland", at_house("ulica Szeroka", "7")),
        case(
            "Szeroka, Stolica, Poland",
            Want::At(house(street("ulica Szeroka"), "7"), 1200.0, true),
        ),
        case(
            "Rakowice Małe 20BS, 76-231, Poland",
            at_house("Rakowice Małe", "20bs"),
        ),
    ]
}
