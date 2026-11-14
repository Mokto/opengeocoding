use super::*;
use crate::{Case, case, named, town};

pub const PLACES: &[Place] = &[
    place!(
        "in",
        "country",
        "",
        "IN",
        "",
        1_400_000_000,
        (78.0, 22.0),
        "",
        "India",
        &[("en", "India")],
    ),
    place!(
        "in-dl",
        "region",
        "",
        "IN",
        "IN-DL",
        17_000_000,
        (77.1, 28.65),
        "in",
        "Delhi",
        &[("en", "Delhi")],
    ),
    place!(
        "in-dl-delhi",
        "locality",
        "city",
        "IN",
        "IN-DL",
        11_000_000,
        (77.209, 28.6139),
        "in-dl",
        "Delhi",
        &[("en", "Delhi")],
    ),
    place!(
        "in-rj",
        "region",
        "",
        "IN",
        "IN-RJ",
        68_000_000,
        (73.0, 26.3),
        "in",
        "Rajasthan",
        &[("en", "Rajasthan")],
    ),
    place!(
        "in-rj-jodhpur",
        "locality",
        "city",
        "IN",
        "IN-RJ",
        1_100_000,
        (73.02, 26.29),
        "in-rj",
        "Jodhpur",
        &[("en", "Jodhpur")],
    ),
    // Tamil Nadu's towns around Chennai lie in its metropolitan area, a locality of no class.
    place!(
        "in-tn",
        "region",
        "",
        "IN",
        "IN-TN",
        72_000_000,
        (78.5, 11.0),
        "in",
        "Tamil Nadu",
        &[("en", "Tamil Nadu")],
    ),
    place!(
        "in-tn-chennai-ma",
        "locality",
        "",
        "IN",
        "IN-TN",
        12_395_000,
        (80.1, 12.95),
        "in-tn",
        "Chennai Metropolitan Area",
        &[("en", "Chennai Metropolitan Area")],
    ),
    place!(
        "in-tn-chennai",
        "locality",
        "city",
        "IN",
        "IN-TN",
        4_681_087,
        (80.27, 13.08),
        "in-tn-chennai-ma",
        "Chennai",
        &[("en", "Chennai")],
    ),
    place!(
        "in-tn-chengalpattu",
        "locality",
        "town",
        "IN",
        "IN-TN",
        0,
        (79.98, 12.8),
        "in-tn-chennai-ma",
        "Chengalpattu",
        &[("en", "Chengalpattu")],
    ),
    place!(
        "in-tn-tiruvallur",
        "locality",
        "town",
        "IN",
        "IN-TN",
        0,
        (79.95, 13.05),
        "in-tn-chennai-ma",
        "Tiruvallur",
        &[("en", "Tiruvallur")],
    ),
    // Overture has Thrissur only as its district; GeoNames has the city (`write_geonames`).
    place!(
        "in-kl",
        "region",
        "",
        "IN",
        "IN-KL",
        33_000_000,
        (76.3, 10.0),
        "in",
        "Kerala",
        &[("en", "Kerala")],
    ),
    place!(
        "in-kl-thrissur",
        "county",
        "",
        "IN",
        "IN-KL",
        0,
        (76.21, 10.52),
        "in-kl",
        "Thrissur",
        &[("en", "Thrissur")],
    ),
    place!(
        "in-wb",
        "region",
        "",
        "IN",
        "IN-WB",
        91_000_000,
        (87.9, 23.4),
        "in",
        "West Bengal",
        &[("en", "West Bengal")],
    ),
    place!(
        "in-wb-north-24-parganas",
        "county",
        "",
        "IN",
        "IN-WB",
        0,
        (88.6, 22.8),
        "in-wb",
        "North 24 Parganas",
        &[("en", "North 24 Parganas")],
    ),
    place!(
        "in-ap",
        "region",
        "",
        "IN",
        "IN-AP",
        49_000_000,
        (80.5, 16.0),
        "in",
        "Andhra Pradesh",
        &[("en", "Andhra Pradesh")],
    ),
    place!(
        "in-ap-east-godavari",
        "county",
        "",
        "IN",
        "IN-AP",
        0,
        (82.0, 17.3),
        "in-ap",
        "East Godavari",
        &[("en", "East Godavari")],
    ),
];

pub const STREETS: &[Street] = &[
    // libpostal reads "Shastri Nagar D-112" as one road: the house is found without its parse.
    Street {
        class: "residential",
        length: 0.0,
        name: "Shastri Nagar",
        postcode: "342003",
        city: "Jodhpur",
        cc: "IN",
        lon: 73.02,
        lat: 26.29,
        numbers: &["D-112"],
    },
    // A number of several words that libpostal splits, and a street named like its town.
    Street {
        class: "residential",
        length: 0.0,
        name: "Shiv Vihar",
        postcode: "342011",
        city: "Jodhpur",
        cc: "IN",
        lon: 73.03,
        lat: 26.29,
        numbers: &["Plot No.14"],
    },
    Street {
        class: "residential",
        length: 0.0,
        name: "Jodhpur",
        postcode: "342003",
        city: "Jodhpur",
        cc: "IN",
        lon: 73.021,
        lat: 26.29,
        numbers: &["Plot No.15"],
    },
];

pub fn write_geosadak(path: &Path) -> String {
    let rows = [
        (1, "Neelapara", 76.31, 10.01, 100),
        (2, "Neelapara", 76.311, 10.011, 100),
        (3, "Delhi", 76.32, 10.02, 100),
        (4, "Samanagar", 76.32, 10.02, 100),
        (5, "Samanagar", 80.1, 13.1, 100),
        (6, "Duplicate East", 76.32, 10.02, 100),
        (6, "Duplicate West", 76.33, 10.03, 100),
        (7, "Badpoint", -1.79769e308, -1.79769e308, 100),
        (8, "Swappedpoint", 10.0, 76.0, 100),
        (9, "Empty Pop", 76.32, 10.02, 0),
        (10, "Foreign", 8.0, 55.0, 100),
        (11, "Outside Region", 75.0, 8.5, 100),
        (12, "123", 76.32, 10.02, 100),
        (13, "", 76.32, 10.02, 100),
        (14, "Nallur Road Colony", 76.32, 10.02, 100),
    ];
    let points: Vec<Vec<u8>> = rows
        .iter()
        .map(|&(_, _, x, y, _)| {
            let mut point = vec![1];
            point.extend(1u32.to_le_bytes());
            point.extend(f64::to_le_bytes(x));
            point.extend(f64::to_le_bytes(y));
            point
        })
        .collect();
    let columns: Vec<(&str, ArrayRef)> = vec![
        (
            "HAB_ID",
            Arc::new(Int32Array::from_iter_values(rows.iter().map(|r| r.0))),
        ),
        (
            "HAB_NAME",
            Arc::new(StringArray::from_iter_values(rows.iter().map(|r| r.1))),
        ),
        (
            "TOT_POPULA",
            Arc::new(Int32Array::from_iter_values(rows.iter().map(|r| r.4))),
        ),
        (
            "geometry",
            Arc::new(arrow_array::BinaryArray::from_iter_values(
                points.iter().map(Vec::as_slice),
            )),
        ),
    ];
    let batch = RecordBatch::try_from_iter(columns).unwrap();
    let mut writer =
        ArrowWriter::try_new(File::create(path).unwrap(), batch.schema(), None).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    ring::digest::digest(&ring::digest::SHA256, &std::fs::read(path).unwrap())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn geosadak_only_fills_unambiguous_gaps_and_never_names_reverse_points() {
    let index = crate::index();
    let suggestions = index.suggest("Neelapa", 5, Some("IN"), None, false);
    assert_eq!(suggestions.len(), 1);
    assert!((suggestions[0].x - 76.31).abs() < 0.00001);
    assert!((suggestions[0].y - 10.01).abs() < 0.00001);
    assert!(index.locality_named("Neelapara", "in", false).is_none());
    assert!(index.locality_named("Neelapara", "in", true).is_some());
    assert!(
        index
            .suggest("Neelapa", 5, Some("FR"), None, false)
            .is_empty()
    );
    let reverse = index.reverse(10.01, 76.31).to_string();
    assert!(!reverse.contains("Neelapara") && !reverse.contains("Nallur Road Colony"));
    let rows = std::fs::read_to_string(crate::dir().join("places.tsv")).unwrap();
    let added: Vec<_> = rows
        .lines()
        .filter(|line| line.split('\t').nth(1) == Some("pmgsy_habitation"))
        .collect();
    assert_eq!(added.len(), 2, "{added:?}");
    assert!(
        added
            .iter()
            .all(|line| line.split('\t').nth(3) == Some("IN-KL"))
    );
    for name in [
        "Samanagar",
        "Duplicate East",
        "Duplicate West",
        "Badpoint",
        "Swappedpoint",
        "Empty Pop",
        "Outside Region",
    ] {
        assert!(
            !added
                .iter()
                .any(|line| line.split('\t').nth(13) == Some(name)),
            "{name}"
        );
    }
    assert!(crate::dir().join("GEOSADAK-NOTICE.txt").is_file());
}

pub fn cases() -> Vec<Case> {
    vec![
        case("Delhi", town("in-dl-delhi", true)),
        case("Delhi, India", town("in-dl-delhi", true)),
        // the town, not the metropolitan area it is in
        named(
            "Chengalpattu, Tamil Nadu, India",
            "in",
            "Chengalpattu",
            &["IN-TN"],
            true,
        ),
        named(
            "Tiruvallur, Tamil Nadu, India",
            "in",
            "Tiruvallur",
            &["IN-TN"],
            true,
        ),
        named(
            "Chennai, Tamil Nadu, India",
            "in",
            "Chennai",
            &["IN-TN"],
            true,
        ),
        named(
            "Thrissur, Kerala, India",
            "in",
            "Thrissur",
            &["IN-KL"],
            true,
        ),
        // a district has no town, and one the parse can't place still answers in its state
        named("East Godavari, Andhra Pradesh", "in", "", &["IN-AP"], true),
        named(
            "North 24 Parganas, West Bengal, India",
            "in",
            "",
            &["IN-WB"],
            false,
        ),
    ]
}
