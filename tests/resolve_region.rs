//! `Index::reverse_region` against a real index: each point's region is public geography (an
//! ISO 3166-2 code), never this service's answer. `INDEX_DIR` is a `build_index` output; the test
//! is ignored, as it needs a world index. No libpostal models are read.
use opengeocoding::index::{Index, Parser};
use std::{path::Path, sync::Arc};

/// (lat, lon, ISO 3166-2 code, why the case is known to fail). A known failure must keep failing
/// until its label is removed.
const POINTS: &[(f64, f64, &str, &str)] = &[
    // either side of State Line Road, Kansas City
    (39.0406, -94.5746, "US-MO", ""),
    (39.0400, -94.5732, "US-MO", ""),
    (39.1142, -94.6275, "US-KS", ""),
    // Zgorzelec and Görlitz, either side of the Neisse
    (51.1519, 15.0087, "PL-02", ""),
    (51.1533, 14.9872, "DE-SN", ""),
    // places whose localities' parents stop short of a region
    (31.2180, 121.4478, "CN-SH", ""),
    (38.9072, -77.0369, "US-DC", ""),
    // 108 km from the nearest locality
    (52.4761, -71.8259, "CA-QC", ""),
    (
        35.1747,
        33.3639,
        "CY-01",
        "the index has no Cypriot divisions",
    ),
];

#[test]
#[ignore = "needs INDEX_DIR, a world index"]
fn each_point_names_the_region_that_contains_it() {
    let dir = std::env::var("INDEX_DIR").expect("INDEX_DIR names a build_index output");
    let index = Index::open(Path::new(&dir), Arc::new(Parser::unloaded(0))).unwrap();
    let mut wrong = vec![];
    for &(lat, lon, want, known_failure) in POINTS {
        let found = index.reverse_region(lat, lon);
        let got = found["address"]["ISO3166-2-lvl4"].as_str();
        match (got == Some(want), known_failure.is_empty()) {
            (false, true) => wrong.push(format!("{lat},{lon}: wanted {want}, got {got:?}")),
            (true, false) => wrong.push(format!("{lat},{lon}: now passes; drop its known failure")),
            _ => {}
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
