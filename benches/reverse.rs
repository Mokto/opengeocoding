//! `Index::reverse_search` against a built index, from points derived from the index itself so
//! they need no libpostal models: houses (the nearest to a locality's point, moved about 15 m),
//! localities with no house within 500 m (a locality's point moved about 2 km) and a world grid's
//! points that have neither. Needs `INDEX_DIR`; without it, or when the index has no `reverse.bin`
//! (benches/fixture/README.md), the house benchmarks run nothing.
use divan::{Bencher, black_box};
use opengeocoding::index::{Index, Parser};
use std::{path::PathBuf, sync::Arc, sync::LazyLock};

const RADIUS_M: f64 = 500.0;
const POINTS: usize = 200;

struct Fixture {
    index: Index,
    houses: Vec<(f64, f64)>,
    localities: Vec<(f64, f64)>,
    empty: Vec<(f64, f64)>,
}

fn layer(index: &Index, (lat, lon): (f64, f64)) -> Option<&'static str> {
    index
        .reverse_search(lat, lon, RADIUS_M, 1)
        .first()
        .map(|r| r.0)
}

static FIXTURE: LazyLock<Fixture> = LazyLock::new(|| {
    let dir = PathBuf::from(std::env::var_os("INDEX_DIR").expect("INDEX_DIR"));
    let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).expect("index");
    let places = std::fs::read_to_string(dir.join("places.tsv")).expect("places.tsv");
    let centres: Vec<(f64, f64)> = places
        .lines()
        .filter(|l| l.starts_with("locality\t"))
        .filter_map(|l| {
            let c: Vec<_> = l.split('\t').collect();
            Some((c.get(7)?.parse().ok()?, c.get(6)?.parse().ok()?))
        })
        .step_by(97)
        .collect();
    // The nearest house within 2 km of a locality's point, moved about 15 m.
    let houses: Vec<(f64, f64)> = centres
        .iter()
        .filter_map(|&(lat, lon)| {
            let found = index.reverse_search(lat, lon, 2000.0, 1);
            let (layer, house, _) = found.first()?;
            let at = |k: &str| house[k].as_str()?.parse::<f64>().ok();
            (*layer == "address").then_some((at("lat")? + 0.0001, at("lon")?))
        })
        .filter(|&p| layer(&index, p) == Some("address"))
        .take(POINTS)
        .collect();
    let localities: Vec<(f64, f64)> = centres
        .iter()
        .map(|&(lat, lon)| (lat + 0.02, lon))
        .filter(|&p| layer(&index, p) == Some("locality"))
        .take(POINTS)
        .collect();
    let empty: Vec<(f64, f64)> = (0..40)
        .flat_map(|i| (0..40).map(move |j| (-60.0 + i as f64 * 3.5, -180.0 + j as f64 * 9.0)))
        .filter(|&p| layer(&index, p).is_none())
        .take(POINTS)
        .collect();

    Fixture {
        index,
        houses,
        localities,
        empty,
    }
});

fn main() {
    if std::env::var_os("INDEX_DIR").is_none() {
        eprintln!("INDEX_DIR is not set: skipping the index benchmarks");
        return;
    }
    divan::main();
}

// Each point is looked up once first, as in resolve.rs, so first-touch page faults are not in
// the measurement: this times a warm service.
fn reverse(bencher: Bencher, points: &[(f64, f64)], size: usize) {
    if points.is_empty() {
        eprintln!("no such points in this index: skipping");
        return;
    }
    for &(lat, lon) in points {
        FIXTURE.index.reverse_search(lat, lon, RADIUS_M, size);
    }
    bencher.with_inputs(|| &*FIXTURE).bench_local_refs(|f| {
        points
            .iter()
            .map(|&(lat, lon)| {
                f.index
                    .reverse_search(black_box(lat), black_box(lon), RADIUS_M, size)
                    .len()
            })
            .sum::<usize>()
    });
}

#[divan::bench]
fn reverse_house(bencher: Bencher) {
    reverse(bencher, &FIXTURE.houses, 1);
}

#[divan::bench]
fn reverse_ten_houses(bencher: Bencher) {
    reverse(bencher, &FIXTURE.houses, 10);
}

#[divan::bench]
fn reverse_locality(bencher: Bencher) {
    reverse(bencher, &FIXTURE.localities, 1);
}

#[divan::bench]
fn reverse_nothing_in_range(bencher: Bencher) {
    reverse(bencher, &FIXTURE.empty, 1);
}
