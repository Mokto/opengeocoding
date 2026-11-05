use super::*;
use crate::*;

fn ring(x0: f64, y0: f64, x1: f64, y1: f64) -> [[i32; 2]; 5] {
    let mut previous = [0i32; 2];
    [[x0, y0], [x1, y0], [x1, y1], [x0, y1], [x0, y0]].map(|point| {
        let point = point.map(|v| (v * 1e7).round() as i32);
        let delta = [point[0] - previous[0], point[1] - previous[1]];
        previous = point;
        delta
    })
}

pub fn write_municipalities(path: &std::path::Path) {
    use std::io::Write;
    let records = serde_json::json!([
        [
            "cousub/4900100001",
            "US-UT",
            "Elmwood township",
            [
                ring(-108.02, 35.98, -108.0, 36.02),
                ring(-108.015, 36.004, -108.013, 36.008),
                ring(-108.018, 36.004, -108.016, 36.008),
                ring(-108.0195, 35.992, -108.0185, 35.994)
            ]
        ],
        [
            "cousub/4900100002",
            "US-UT",
            "Brookmere town",
            [ring(-108.0195, 35.992, -108.0185, 35.994)]
        ],
        [
            "place/0400001",
            "US-AZ",
            "Juniper city",
            [ring(-108.0, 35.98, -107.96, 36.02)]
        ],
        [
            "place/4900001",
            "US-UT",
            "Overlapping borough",
            [ring(-108.011, 36.005, -108.009, 36.007)]
        ],
        [
            "place/4900099",
            "US-UT",
            "Incorporated Conflict",
            [ring(-108.0015, 35.992, -108.0005, 35.994)]
        ],
        [
            "place/0400002",
            "US-AZ",
            "Contradictory city",
            [ring(-108.018, 36.004, -108.016, 36.008)]
        ]
    ]);
    let mut gzip = flate2::write::GzEncoder::new(
        std::fs::File::create(path).unwrap(),
        flate2::Compression::default(),
    );
    gzip.write_all(records.to_string().as_bytes()).unwrap();
    gzip.finish().unwrap();
}

pub fn write_cdps(path: &std::path::Path) {
    use std::io::Write;
    let records = serde_json::json!([
        [
            "cdp/4900107",
            "US-UT",
            "Meadow Alias",
            [ring(-108.0055, 35.992, -108.0045, 35.994)]
        ],
        [
            "cdp/4900108",
            "US-UT",
            "Statistical Newvale",
            [ring(-108.0194, 35.9921, -108.0186, 35.9939)]
        ],
        [
            "cdp/4900101",
            "US-UT",
            "Meadowford",
            [
                ring(-108.018, 35.989, -108.006, 35.997),
                ring(-108.017, 35.992, -108.015, 35.994),
                ring(-108.012, 35.994, -108.010, 35.996)
            ]
        ],
        [
            "cdp/4900102",
            "US-UT",
            "Overlap",
            [ring(-108.009, 35.992, -108.007, 35.994)]
        ],
        [
            "cdp/0400101",
            "US-AZ",
            "Wrong State",
            [ring(-108.004, 35.992, -108.002, 35.994)]
        ],
        [
            "cdp/4900103",
            "US-UT",
            "Brookmere",
            [ring(-108.012, 35.994, -108.010, 35.996)]
        ],
        [
            "cdp/4900104",
            "US-UT",
            "Conflict",
            [ring(-108.0015, 35.992, -108.0005, 35.994)]
        ],
        [
            "cdp/4900105",
            "US-UT",
            "Westfield",
            [ring(-108.013, 35.987, -108.012, 35.9885)]
        ],
        [
            "cdp/4900106",
            "US-UT",
            "Eastfield",
            [ring(-108.012, 35.987, -108.0105, 35.9885)]
        ]
    ]);
    let mut gzip = flate2::write::GzEncoder::new(
        std::fs::File::create(path).unwrap(),
        flate2::Compression::default(),
    );
    gzip.write_all(records.to_string().as_bytes()).unwrap();
    gzip.finish().unwrap();
}

pub const REVERSE_PLACES: &[Place] = &[
    place!(
        "us-reverse-west-region",
        "region",
        "",
        "US",
        "US-UT",
        100000,
        (-108.020000, 36.000000),
        "us",
        "Brookmere",
        &[]
    ),
    place!(
        "us-reverse-east-region",
        "region",
        "",
        "US",
        "US-AZ",
        100000,
        (-107.980000, 36.000000),
        "us",
        "Test East",
        &[]
    ),
    place!(
        "us-reverse-west",
        "locality",
        "village",
        "US",
        "US-UT",
        1000,
        (-108.010000, 36.000000),
        "us-reverse-west-region",
        "Brookmere",
        &[("fr", "Meadow Alias")]
    ),
    place!(
        "us-reverse-east",
        "locality",
        "village",
        "US",
        "US-UT",
        1000,
        (-107.990000, 36.000000),
        "us-reverse-east-region",
        "Pineward",
        &[]
    ),
    place!(
        "us-reverse-closer",
        "locality",
        "hamlet",
        "US",
        "US-UT",
        10,
        (-108.000200, 36.000000),
        "us-reverse-west-region",
        "Falsebrook",
        &[]
    ),
    place!(
        "us-reverse-namesake",
        "locality",
        "village",
        "US",
        "US-AZ",
        1000000,
        (-107.970000, 36.000000),
        "us-reverse-east-region",
        "Brookmere",
        &[]
    ),
    place!(
        "us-existing-government",
        "localadmin",
        "",
        "US",
        "US-UT",
        1000,
        (-108.011, 36.016),
        "us-reverse-west-region",
        "Existing Municipality",
        &[]
    ),
    place!(
        "us-government-child",
        "locality",
        "village",
        "US",
        "US-UT",
        1000,
        (-108.011, 36.016),
        "us-existing-government",
        "Woods End",
        &[]
    ),
];

pub const REVERSE_POINT_ONLY: &[&str] = &[
    "us-reverse-west",
    "us-reverse-east",
    "us-reverse-closer",
    "us-reverse-namesake",
    "us-existing-government",
    "us-government-child",
];

pub const REVERSE_AREAS: &[(&str, &str, bool, bool, f64, f64, f64)] = &[
    (
        "us-existing-government",
        "localadmin",
        true,
        true,
        -108.011,
        36.016,
        0.0008,
    ),
    (
        "us-government-child",
        "locality",
        true,
        true,
        -108.011,
        36.016,
        0.0006,
    ),
    (
        "us-reverse-west-region",
        "region",
        true,
        true,
        -108.020000,
        36.0,
        0.02,
    ),
    (
        "us-reverse-east-region",
        "region",
        true,
        true,
        -107.980000,
        36.0,
        0.02,
    ),
    (
        "us-reverse-west",
        "locality",
        true,
        true,
        -108.010000,
        36.0,
        0.01,
    ),
    (
        "us-reverse-east",
        "locality",
        true,
        true,
        -107.990000,
        36.0,
        0.01,
    ),
];

pub const REVERSE_STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp Alias",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.005,
        lat: 35.993,
        numbers: &["171Q-173"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp Government Name",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.019,
        lat: 35.993,
        numbers: &["181R-183"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp Inside",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.012,
        lat: 35.991,
        numbers: &["81F-83"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp Hole",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.016,
        lat: 35.993,
        numbers: &["91G-93"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp Overlap",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.008,
        lat: 35.993,
        numbers: &["101H/103"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp Wrong State",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.003,
        lat: 35.993,
        numbers: &["111J-113"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp Encoding Edge",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.017,
        lat: 35.993,
        numbers: &["121K-123"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp Already",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.011,
        lat: 35.995,
        numbers: &["131L-133"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp Government Conflict",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.001,
        lat: 35.993,
        numbers: &["141M-143"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp West",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.01205,
        lat: 35.988,
        numbers: &["151N-153"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cdp East",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.011,
        lat: 35.988,
        numbers: &["161P-163"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Civic Hole",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.014,
        lat: 36.006,
        numbers: &["31B-33"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Civic Overlap",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.010,
        lat: 36.006,
        numbers: &["41A/43"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Civic Wrong State",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.017,
        lat: 36.006,
        numbers: &["51C-53"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Civic Encoding Edge",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.015,
        lat: 36.006,
        numbers: &["61D-63"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Civic Existing",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.011,
        lat: 36.016,
        numbers: &["71E-73"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Civic West",
        postcode: "84001",
        city: "Dispatch",
        cc: "US",
        lon: -108.000100,
        lat: 36.0,
        numbers: &["17B-19"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Civic East",
        postcode: "84001",
        city: "Brookmere",
        cc: "US",
        lon: -107.999900,
        lat: 36.0,
        numbers: &["23A-25"],
    },
];

#[test]
fn us_reverse_house_locality_is_not_the_nearest_or_postal_name() {
    let west = house(crate::street("Civic West"), "17B-19");
    let nearest = crate::index().reverse(west.1, west.0);
    assert_eq!(nearest["address"]["village"], "Falsebrook");
    let found = crate::index().reverse_search(west.1, west.0, 2.0, 1);
    assert_eq!(found.len(), 1);
    let (layer, value, _) = &found[0];
    assert_eq!(*layer, "address");
    assert_eq!(value["address"]["village"], "Brookmere");
    assert_eq!(value["address"]["postal_town"], "Dispatch");
    assert_eq!(value["address"]["country_code"], "us");
    assert_eq!(value["address"]["ISO3166-2-lvl4"], "US-UT");
    assert_eq!(value["address"]["house_number"], "17b-19");
    assert_eq!(value["address"]["road"], "Civic West");
    assert_eq!(value["address"]["postcode"], "84001");
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
fn us_reverse_offset_crosses_the_boundary_without_renaming_either_house() {
    let west = house(crate::street("Civic West"), "17B-19");
    let east = house(crate::street("Civic East"), "23A-25");
    let offset = west.0 + 25.0 / (111_195.0 * west.1.to_radians().cos());
    let found = crate::index().reverse_search(west.1, offset, 100.0, 1);
    let at_east = crate::index().reverse_search(east.1, east.0, 2.0, 1);
    assert_eq!(found[0].1, at_east[0].1);
    let value = &found[0].1;
    assert_eq!(value["address"]["village"], "Pineward");
    assert_eq!(value["address"]["postal_town"], "Brookmere");
    assert_eq!(value["address"]["ISO3166-2-lvl4"], "US-AZ");
    assert_eq!(value["address"]["country_code"], "us");
    assert_eq!(value["address"]["house_number"], "23a-25");
    assert_eq!(value["address"]["road"], "Civic East");
    assert_eq!(value["address"]["postcode"], "84001");
    let at = (
        value["lon"].as_str().unwrap().parse().unwrap(),
        value["lat"].as_str().unwrap().parse().unwrap(),
    );
    assert!(crate::metres(at, east) < 0.2);
}

#[test]
fn us_reverse_keeps_old_indexes_readable_and_preserves_every_existing_field() {
    let dir = std::env::temp_dir().join(format!("us-legacy-index-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(crate::dir()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if entry.file_type().unwrap().is_file()
            && name != "us-municipalities.bin"
            && name != "us-municipalities.json"
        {
            std::fs::copy(entry.path(), dir.join(name)).unwrap();
        }
    }
    let old = opengeocoding::index::Index::open(
        &dir,
        std::sync::Arc::new(opengeocoding::index::Parser::unloaded(0)),
    )
    .unwrap();
    let point = house(crate::street("Civic West"), "17B-19");
    let before = old.reverse_search(point.1, point.0, 2.0, 1);
    let mut after = crate::index().reverse_search(point.1, point.0, 2.0, 1);
    assert_eq!(
        after[0].1["address"]
            .as_object_mut()
            .unwrap()
            .remove("municipality"),
        Some(serde_json::json!("Elmwood township"))
    );
    assert_eq!(before, after);
    drop(old);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn us_reverse_uses_typed_government_polygons_without_replacing_localities() {
    for (road, number, village, municipality, region) in [
        (
            "Civic West",
            "17B-19",
            "Brookmere",
            Some("Elmwood township"),
            "US-UT",
        ),
        (
            "Civic East",
            "23A-25",
            "Pineward",
            Some("Juniper city"),
            "US-AZ",
        ),
        ("Civic Hole", "31B-33", "Brookmere", None, "US-UT"),
        ("Civic Overlap", "41A/43", "Brookmere", None, "US-UT"),
        ("Civic Wrong State", "51C-53", "Brookmere", None, "US-UT"),
        ("Civic Encoding Edge", "61D-63", "Brookmere", None, "US-UT"),
        (
            "Civic Existing",
            "71E-73",
            "Woods End",
            Some("Existing Municipality"),
            "US-UT",
        ),
    ] {
        let point = house(crate::street(road), number);
        let found = crate::index().reverse_search(point.1, point.0, 2.0, 1);
        assert_eq!(found.len(), 1, "{road}");
        assert_eq!(found[0].0, "address");
        let value = &found[0].1;
        assert_eq!(
            value["address"]["municipality"].as_str(),
            municipality,
            "{road}: {value}"
        );
        assert_eq!(value["address"]["village"], village, "{road}: {value}");
        assert_eq!(value["address"]["country_code"], "us");
        assert_eq!(value["address"]["ISO3166-2-lvl4"], region);
        assert_eq!(value["address"]["house_number"], number.to_lowercase());
        assert_eq!(value["address"]["road"], road);
        assert_eq!(value["address"]["postcode"], "84001");
        let at = (
            value["lon"].as_str().unwrap().parse().unwrap(),
            value["lat"].as_str().unwrap().parse().unwrap(),
        );
        assert!(crate::metres(at, point) < 0.2);
        assert!(!value["display_name"].as_str().unwrap().contains("township"));
    }
}

#[test]
fn us_cdps_correct_only_geographic_locality_and_keep_legacy_indexes_readable() {
    let dir = std::env::temp_dir().join(format!("us-cdp-legacy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(crate::dir()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if entry.file_type().unwrap().is_file() && name != "us-cdps.bin" && name != "us-cdps.json" {
            std::fs::copy(entry.path(), dir.join(name)).unwrap();
        }
    }
    let parser = || std::sync::Arc::new(opengeocoding::index::Parser::unloaded(0));
    let old = opengeocoding::index::Index::open(&dir, parser()).unwrap();
    for (road, number, expected) in [
        ("Cdp Inside", "81F-83", Some("Meadowford")),
        ("Cdp Alias", "171Q-173", None),
        ("Cdp Government Name", "181R-183", None),
        ("Cdp Hole", "91G-93", None),
        ("Cdp Overlap", "101H/103", None),
        ("Cdp Wrong State", "111J-113", None),
        ("Cdp Encoding Edge", "121K-123", None),
        ("Cdp Already", "131L-133", None),
        ("Cdp Government Conflict", "141M-143", None),
        ("Cdp West", "151N-153", Some("Westfield")),
        ("Cdp East", "161P-163", Some("Eastfield")),
    ] {
        let point = house(crate::street(road), number);
        let before = old.reverse_search(point.1, point.0, 2.0, 1);
        let mut after = crate::index().reverse_search(point.1, point.0, 2.0, 1);
        assert_eq!(before.len(), 1, "{road}");
        assert_eq!(after.len(), 1, "{road}");
        if let Some(expected) = expected {
            if road == "Cdp Inside" {
                assert_eq!(before[0].1["address"]["state"], "Brookmere");
                assert!(
                    after[0].1["display_name"]
                        .as_str()
                        .unwrap()
                        .split(", ")
                        .any(|name| name == "Brookmere")
                );
            }
            assert_eq!(after[0].1["address"]["village"], expected, "{road}");
            assert!(
                after[0].1["display_name"]
                    .as_str()
                    .unwrap()
                    .contains(expected)
            );
            for key in ["city", "town", "village"] {
                after[0].1["address"].as_object_mut().unwrap().remove(key);
                if let Some(value) = before[0].1["address"].get(key) {
                    after[0].1["address"]
                        .as_object_mut()
                        .unwrap()
                        .insert(key.into(), value.clone());
                }
            }
            after[0].1["display_name"] = before[0].1["display_name"].clone();
        }
        assert_eq!(before, after, "{road}");
    }
    drop(old);
    std::fs::copy(crate::dir().join("us-cdps.json"), dir.join("us-cdps.json")).unwrap();
    assert!(opengeocoding::index::Index::open(&dir, parser()).is_err());
    std::fs::copy(crate::dir().join("us-cdps.bin"), dir.join("us-cdps.bin")).unwrap();
    drop(opengeocoding::index::Index::open(&dir, parser()).unwrap());
    let mut labels: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("us-cdps.json")).unwrap()).unwrap();
    labels[0][0] = "place/4900101".into();
    std::fs::write(
        dir.join("us-cdps.json"),
        serde_json::to_vec(&labels).unwrap(),
    )
    .unwrap();
    assert!(opengeocoding::index::Index::open(&dir, parser()).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn us_cdp_membership_follows_the_selected_house_not_the_offset_query() {
    let west = house(crate::street("Cdp West"), "151N-153");
    let query = west.0 + 25.0 / (111_195.0 * west.1.to_radians().cos());
    assert!(query > -108.012);
    let at_house = crate::index().reverse_search(west.1, west.0, 2.0, 1);
    let offset = crate::index().reverse_search(west.1, query, 100.0, 1);
    assert_eq!(offset[0].1, at_house[0].1);
    assert_eq!(offset[0].1["address"]["village"], "Westfield");
    assert_eq!(offset[0].1["address"]["municipality"], "Elmwood township");
}

pub const PLACES: &[Place] = &[
    place!(
        "us-tx-alderhaven",
        "locality",
        "town",
        "US",
        "US-TX",
        1_000,
        (-96.0, 34.0),
        "us-tx",
        "Ashford",
        &[("en", "Ashford")]
    ),
    place!(
        "us-tx-north-alderhaven",
        "macrohood",
        "",
        "US",
        "US-TX",
        0,
        (-96.0, 34.02),
        "us-tx-alderhaven",
        "North Ashford",
        &[("en", "North Ashford")]
    ),
    place!(
        "us-ga-alderhaven",
        "locality",
        "city",
        "US",
        "US-GA",
        100_000,
        (-82.0, 33.0),
        "us-ga",
        "Ashford",
        &[("en", "Ashford")]
    ),
    place!(
        "us-ga-north-alderhaven",
        "locality",
        "hamlet",
        "US",
        "US-GA",
        0,
        (-82.0, 33.02),
        "us-ga-alderhaven",
        "North Ashford",
        &[("en", "North Ashford")]
    ),
    place!(
        "us",
        "country",
        "",
        "US",
        "",
        330_000_000,
        (-100.0, 40.0),
        "",
        "United States",
        &[("en", "United States")],
    ),
    place!(
        "us-tx",
        "region",
        "",
        "US",
        "US-TX",
        29_000_000,
        (-97.0, 31.5),
        "us",
        "Texas",
        &[("en", "Texas")],
    ),
    place!(
        "us-tx-paris",
        "locality",
        "city",
        "US",
        "US-TX",
        25_000,
        (-95.5555, 33.6609),
        "us-tx",
        "Paris",
        &[("en", "Paris")],
    ),
    place!(
        "us-ga",
        "region",
        "",
        "US",
        "US-GA",
        1_000_000,
        (-83.0, 32.0),
        "us",
        "Georgia",
        &[("en", "Georgia")]
    ),
    place!(
        "us-ga-arbor",
        "locality",
        "town",
        "US",
        "US-GA",
        10_000,
        (-83.0, 32.0),
        "us-ga",
        "Arbor",
        &[("en", "Arbor")]
    ),
    place!(
        "us-il",
        "region",
        "",
        "US",
        "US-IL",
        1_000_000,
        (-89.6, 40.3),
        "us",
        "Illinois",
        &[("en", "Illinois")]
    ),
    place!(
        "us-il-bayport",
        "locality",
        "town",
        "US",
        "US-IN",
        20_000,
        (-89.6, 40.3),
        "us-il",
        "Bayport",
        &[("en", "Bayport")]
    ),
    // a city beside Paris, Texas, at the east end of Long Road, also a point only
    place!(
        "us-tx-reno",
        "locality",
        "city",
        "US",
        "US-TX",
        30_000,
        (-95.575, 33.703),
        "us-tx",
        "Reno",
        &[("en", "Reno")],
    ),
    // Guernsey, Wyoming: a town named like the crown dependency (`gg.rs`).
    place!(
        "us-wy",
        "region",
        "",
        "US",
        "US-WY",
        580_000,
        (-107.5, 43.0),
        "us",
        "Wyoming",
        &[("en", "Wyoming")],
    ),
    place!(
        "us-wy-guernsey",
        "locality",
        "town",
        "US",
        "US-WY",
        1_147,
        (-104.74, 42.27),
        "us-wy",
        "Guernsey",
        &[("en", "Guernsey")],
    ),
    // A town named like the made-up territory of `wf.rs`, which no reference data names.
    place!(
        "us-wy-coral-isles",
        "locality",
        "town",
        "US",
        "US-WY",
        2_000,
        (-105.5, 41.3),
        "us-wy",
        "Coral Isles",
        &[("en", "Coral Isles")],
    ),
    // A county named for a town that lies elsewhere in its state.
    place!(
        "us-mo",
        "region",
        "",
        "US",
        "US-MO",
        6_100_000,
        (-92.5, 38.5),
        "us",
        "Missouri",
        &[("en", "Missouri")],
    ),
    place!(
        "us-mo-jackson-county",
        "county",
        "",
        "US",
        "US-MO",
        717_000,
        (-94.35, 39.0),
        "us-mo",
        "Jackson County",
        &[("en", "Jackson County")],
    ),
    place!(
        "us-mo-jackson",
        "locality",
        "town",
        "US",
        "US-MO",
        15_000,
        (-89.66, 37.38),
        "us-mo",
        "Jackson",
        &[("en", "Jackson")],
    ),
    place!(
        "us-tx-dallas",
        "locality",
        "city",
        "US",
        "US-TX",
        1_300_000,
        (-96.8, 32.78),
        "us-tx",
        "Dallas",
        &[("en", "Dallas")],
    ),
    place!(
        "us-wy-laramie",
        "locality",
        "town",
        "US",
        "US-WY",
        30_000,
        (-106.3, 42.85),
        "us-wy",
        "Laramie",
        &[("en", "Laramie")],
    ),
];

pub const STREETS: &[Street] = &[
    Street {
        class: "residential",
        length: 0.0,
        name: "Laurel Mtn Road",
        postcode: "75480",
        city: "Ashford",
        cc: "US",
        lon: -96.05,
        lat: 34.02,
        numbers: &["1", "3"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Laurel Mountain Road",
        postcode: "75480",
        city: "Ashford",
        cc: "US",
        lon: -95.95,
        lat: 34.02,
        numbers: &["100", "102"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Birch Mountain Highway",
        postcode: "75480",
        city: "Ashford",
        cc: "US",
        lon: -96.0,
        lat: 34.02,
        numbers: &["100", "102"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cedar Avenue",
        postcode: "31001",
        city: "North Ashford",
        cc: "US",
        lon: -82.0,
        lat: 33.02,
        numbers: &["104", "106"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Township Lane",
        postcode: "60001",
        city: "Postalton",
        cc: "US",
        lon: -89.6,
        lat: 40.3004,
        numbers: &["15"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Cedar Road",
        postcode: "31000",
        city: "Arbor",
        cc: "US",
        lon: -83.0,
        lat: 32.0,
        numbers: &["16"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Meadow Road",
        postcode: "60001",
        city: "Bayport",
        cc: "US",
        lon: -89.6,
        lat: 40.3,
        numbers: &["14"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Main Street",
        postcode: "75460",
        city: "Paris",
        cc: "US",
        lon: -95.556,
        lat: 33.66,
        numbers: &["100", "102", "104"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Canal Street",
        postcode: "75460",
        city: "Paris",
        cc: "US",
        lon: -95.55,
        lat: 33.65,
        numbers: &["1"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Canal Street",
        postcode: "75461",
        city: "Paris",
        cc: "US",
        lon: -95.58,
        lat: 33.65,
        numbers: &["2"],
    },
    // houses at the west end of a 2.8 km street
    Street {
        class: "residential",
        length: 0.03,
        name: "Long Road",
        postcode: "75462",
        city: "Paris",
        cc: "US",
        lon: -95.6,
        lat: 33.7,
        numbers: &["1", "3"],
    },
    // One segment of Levee Road holds 75465 (number 172) and 75466 (number 170); most of 75465 is
    // Gin Road's, so its centre is 6.7 km from them, like a town-wide Brazilian CEP.
    Street {
        class: "residential",
        length: 0.0,
        name: "Levee Road",
        postcode: "75466",
        city: "Paris",
        cc: "US",
        lon: -95.7,
        lat: 33.5,
        numbers: &["170"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Levee Road",
        postcode: "75465",
        city: "Paris",
        cc: "US",
        lon: -95.69985,
        lat: 33.5,
        numbers: &["172"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Gin Road",
        postcode: "75465",
        city: "Paris",
        cc: "US",
        lon: -95.62,
        lat: 33.5,
        numbers: &["1", "2", "3", "4", "5", "6", "7", "8", "9"],
    },
    // a street named for its town, which holds no house 120
    Street {
        class: "residential",
        length: 0.0,
        name: "Laramie Boulevard",
        postcode: "82601",
        city: "Laramie",
        cc: "US",
        lon: -106.3,
        lat: 42.85,
        numbers: &["100", "102"],
    },
];

pub fn cases() -> Vec<Case> {
    vec![
        case(
            "102 Laurel Mtn Road, Ashford, United States",
            Want::At(house(street("Laurel Mtn Road"), "1"), 300.0, false),
        ),
        case(
            "102 Laurel Mountain Road, Ashford, United States",
            at_house("Laurel Mountain Road", "102"),
        ),
        case(
            "2760 Birch Mtn Highway, North Ashford, United States",
            town("us-tx-north-alderhaven", false),
        ),
        case(
            "2760 Birch Mountain Highway, North Ashford, United States",
            town("us-tx-north-alderhaven", false),
        ),
        case(
            "102 Birch Mtn Highway, Ashford, United States",
            at_house("Birch Mountain Highway", "102"),
        ),
        case(
            "104 Cedar Avenue, North Ashford, United States",
            at_house("Cedar Avenue", "104"),
        ),
        case(
            "North Ashford, Georgia, United States",
            town("us-ga-north-alderhaven", true),
        ),
        named("Jackson County, Missouri", "us", "", &["US-MO"], true),
        // a metro label the caller does not drop reaches its first town, never confidently
        named(
            "Dallas-Fort Worth Metroplex",
            "us",
            "Dallas",
            &["US-TX"],
            false,
        ),
        case("Paris, Texas", town("us-tx-paris", true)),
        case("Paris, TX, USA", town("us-tx-paris", true)),
        case("us-tx-paris", town("us-tx-paris", true)),
        case(
            "14 Meadow Road, 60001 Bayport, Illinois, United States",
            at_house("Meadow Road", "14"),
        ),
        case(
            "102 Main Street, Paris, Texas",
            at_house("Main Street", "102"),
        ),
        // France has this street, Texas has not: the town in Texas, never the French street
        case("Rue de Rivoli, Paris, Texas", town("us-tx-paris", false)),
        // the street in the town it is named for, not Wyoming: no town stated, never a good match
        case(
            "120 Laramie Blvd, WY",
            Want::At(house(street("Laramie Boulevard"), "100"), 300.0, false),
        ),
    ]
}

/// Township Lane's house is filed under Bayport; only its addresses name Postalton.
#[test]
fn us_addresses_are_suggested_with_a_state_zip_unit_or_out_of_order() {
    let index = index();
    let want = house(street("Township Lane"), "15");
    for query in [
        "15 Township Lane, Postalton, IL 60001",
        "15 Township Lane, Postalton, IL",
        "15 Township Lane, Postalton, IL 60001-1234",
        "15 Township Lane, Postalton Township, IL 60001",
        "15 Township Lane, Cook, Postalton Township, IL 60001",
        "15 Township Lane Apt 4, Postalton, IL 60001",
        "15 Township Lane, Apt 4, Postalton, IL 60001",
        "15 Township Lane #4, Postalton, IL 60001",
        "15 Township Lane Postalton IL 60001",
        "United States, IL 60001, Postalton, 15 Township Lane",
        // a ZIP or a town the house does not have
        "15 Township Lane, Postalton, IL 60009",
        "15 Township Lane, Arbor, IL 60001",
    ] {
        let found = index.suggest(query, 5, None, None, false);
        assert_eq!(found.first().map(|s| s.kind), Some("address"), "{query}");
        assert_eq!(found[0].country_code, "us", "{query}");
        assert!(metres((found[0].x, found[0].y), want) < 5., "{query}");
        let unit = if query.contains("4,") { "4" } else { "" };
        assert!(found[0].unverified_suffix.ends_with(unit), "{query}");
    }
    // another state holds no such house
    let query = "15 Township Lane Apt 4, Postalton, GA 60001";
    assert!(
        !index
            .suggest(query, 5, None, None, false)
            .iter()
            .any(|s| s.kind == "address" && metres((s.x, s.y), want) < 5.),
        "{query}"
    );
    // the street alone, by the town its addresses name
    let found = index.suggest("Township Lane, Postalton", 5, None, None, false);
    assert_eq!(found.first().map(|s| s.kind), Some("street"));
    let paris = index.suggest("Paris, TX 75460", 5, None, None, false);
    let want = place("us-tx-paris");
    assert!(
        metres((paris[0].x, paris[0].y), (want.lon, want.lat)) < 5.,
        "Paris, TX 75460"
    );
}
