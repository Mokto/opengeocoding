//! `Index::reverse_region` against a real index: each point's region is public geography (an
//! ISO 3166-2 code), never this service's answer. `INDEX_DIR` is a `build_index` output; the test
//! is ignored, as it needs a world index. No libpostal models are read.
use opengeocoding::index::{Index, Parser, area};
use std::{path::Path, sync::Arc};

/// (lat, lon, ISO 3166-2 codes, why the case is known to fail), one code per level (4, then 6).
/// A known failure must keep failing until its label is removed.
const POINTS: &[(f64, f64, &[&str], &str)] = &[
    // either side of State Line Road, Kansas City
    (39.0406, -94.5746, &["US-MO"], ""),
    (39.0400, -94.5732, &["US-MO"], ""),
    (39.1142, -94.6275, &["US-KS"], ""),
    // Zgorzelec and Görlitz, either side of the Neisse
    (51.1519, 15.0087, &["PL-02"], ""),
    (51.1533, 14.9872, &["DE-SN"], ""),
    // places whose localities' parents stop short of a region
    (31.2180, 121.4478, &["CN-SH"], ""),
    (38.9072, -77.0369, &["US-DC"], ""),
    // 108 km from the nearest locality
    (52.4761, -71.8259, &["CA-QC"], ""),
    (
        35.1747,
        33.3639,
        &["CY-01"],
        "the index has no Cypriot divisions",
    ),
    // Baarle-Hertog (Wikidata Q244959), a Belgian enclave in the Netherlands
    (51.438333, 4.931667, &["BE-VLG", "BE-VAN"], ""),
    // Wikidata items' points whose nearest locality or house the index files in another region,
    // or in none: Oshikango (Q2033319), on the Angolan border; Caracas (Q1533); Valletta
    // (Q20924973); Sabana Yegua (Q23762304); Belait (Q40395)
    (-17.4, 15.883333, &["NA-OW"], ""),
    (10.506111, -66.914444, &["VE-A"], ""),
    (35.89833, 14.5125, &["MT-60"], ""),
    (18.716667, -71.016667, &["DO-02"], ""),
    (4.416667, 114.583333, &["BN-BE"], ""),
    // a region and the subdivision below it
    (48.0734, 7.2376, &["FR-GES", "FR-68"], ""),
    (48.8552, 2.3275, &["FR-IDF", "FR-75C"], ""),
    (45.4798, 9.1998, &["IT-25", "IT-MI"], ""),
    (43.2630, -2.9350, &["ES-PV", "ES-BI"], ""),
    (49.9632, 5.4398, &["BE-WAL", "BE-WLX"], ""),
    (49.8162, 18.1977, &["CZ-80", "CZ-806"], ""),
];

#[test]
#[ignore = "needs INDEX_DIR, a world index"]
fn each_point_names_the_regions_that_contain_it() {
    let index = open();
    let mut wrong = vec![];
    for &(lat, lon, want, known_failure) in POINTS {
        let address = &index.reverse_region(lat, lon)["address"];
        let got: Vec<_> = ["ISO3166-2-lvl4", "ISO3166-2-lvl6"]
            .iter()
            .filter_map(|key| address[*key].as_str())
            .collect();
        match (got == want, known_failure.is_empty()) {
            (false, true) => wrong.push(format!("{lat},{lon}: wanted {want:?}, got {got:?}")),
            (true, false) => wrong.push(format!("{lat},{lon}: now passes; drop its known failure")),
            _ => {}
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Times `reverse_region`, and the polygon lookup alone, on a grid of 10,000 points over western
/// and central Europe, land and sea, after one untimed pass. Prints the percentiles; asserts
/// nothing about speed.
#[test]
#[ignore = "needs INDEX_DIR, a world index"]
fn region_lookups_are_timed() {
    let index = open();
    let points: Vec<(f64, f64)> = (0..100)
        .flat_map(|i| (0..100).map(move |j| (36.0 + i as f64 * 0.24, -9.0 + j as f64 * 0.33)))
        .collect();
    let answered = points
        .iter()
        .filter(|&&(lat, lon)| index.reverse_region(lat, lon).get("address").is_some())
        .count();
    println!(
        "{} points, {answered} answered; reverse_region {}",
        points.len(),
        timed(&points, |lat, lon| {
            std::hint::black_box(index.reverse_region(lat, lon));
        })
    );
    let dir = std::env::var("INDEX_DIR").unwrap();
    if let Ok(bytes) = std::fs::read(Path::new(&dir).join(area::FILE)) {
        let areas = area::Areas::new(tantivy::directory::OwnedBytes::new(bytes)).unwrap();
        let held = points
            .iter()
            .filter(|&&(lat, lon)| !areas.containing(lat, lon).is_empty())
            .count();
        println!(
            "{held} in a polygon; polygon lookup {}",
            timed(&points, |lat, lon| {
                std::hint::black_box(areas.containing(lat, lon));
            })
        );
    }
}

fn timed(points: &[(f64, f64)], f: impl Fn(f64, f64)) -> String {
    let mut took: Vec<f64> = points
        .iter()
        .map(|&(lat, lon)| {
            let t = std::time::Instant::now();
            f(lat, lon);
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    took.sort_by(f64::total_cmp);
    let at = |q: f64| took[((took.len() - 1) as f64 * q) as usize];
    format!(
        "p50 {:.1} µs, p90 {:.1} µs, p99 {:.1} µs, max {:.0} µs",
        at(0.5),
        at(0.9),
        at(0.99),
        at(1.0)
    )
}

/// Every row `ListRegions` serves is what a search for `<cc>, <code>` returns: the place's own
/// name, at the code's level.
#[test]
#[ignore = "needs INDEX_DIR, a world index"]
fn every_listed_code_finds_its_own_region() {
    let index = open();
    // Wallis and Futuna's three districts: the search finds the territory, not a district
    const KNOWN: [&str; 3] = ["WF-AL", "WF-SG", "WF-UV"];
    let mut wrong = vec![];
    for (code, place) in index.region_places() {
        let (cc, _) = code.split_once('-').unwrap();
        let found = index.search(&format!("{}, {code}", cc.to_lowercase()));
        let rendered = found.first().map(|hit| index.render(hit, true));
        let carries = |key: &str| {
            rendered
                .as_ref()
                .is_some_and(|r| r["address"][key].as_str() == Some(code))
        };
        let finds = ["ISO3166-2-lvl4", "ISO3166-2-lvl5", "ISO3166-2-lvl6"]
            .iter()
            .any(|key| carries(key));
        if finds == KNOWN.contains(&code) {
            wrong.push(format!("{code} (place {place}, finds it: {finds})"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of them: {}",
        wrong.len(),
        wrong.join(", ")
    );
}

fn open() -> Index {
    let dir = std::env::var("INDEX_DIR").expect("INDEX_DIR names a build_index output");
    Index::open(Path::new(&dir), Arc::new(Parser::unloaded(0))).unwrap()
}
