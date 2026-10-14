//! `Index::suggest` against a built index, over the non-held-out rows of `tests/addresses.tsv`
//! as they are typed: each query cut at half, three quarters and all of its length. Needs
//! `INDEX_DIR` (the fixture in benches/fixture) but no libpostal models, which suggestions never
//! read; without the index it runs nothing.
use divan::{Bencher, black_box};
use opengeocoding::index::{Index, Parser};
use std::{path::PathBuf, sync::Arc, sync::LazyLock};

struct Fixture {
    index: Index,
    rows: Vec<(String, String)>,
}

static FIXTURE: LazyLock<Fixture> = LazyLock::new(|| {
    let dir = PathBuf::from(std::env::var_os("INDEX_DIR").expect("INDEX_DIR"));
    let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).expect("index");
    let rows = include_str!("../tests/addresses.tsv")
        .lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<_> = l.split('\t').collect();
            (c[6] == "0").then(|| (c[1].to_string(), c[8].to_string()))
        })
        .collect();
    Fixture { index, rows }
});

fn main() {
    if std::env::var_os("INDEX_DIR").is_none() {
        eprintln!("INDEX_DIR is not set: skipping the index benchmarks");
        return;
    }
    divan::main();
}

fn typed(targets: &[&str]) -> Vec<String> {
    let mut out = vec![];
    for (target, query) in &FIXTURE.rows {
        if !targets.contains(&target.as_str()) {
            continue;
        }
        let chars: Vec<char> = query.chars().collect();
        for part in [2, 3, 4] {
            let end = (chars.len() * part).div_ceil(4);
            if end >= 3 {
                out.push(chars[..end].iter().collect());
            }
        }
    }
    out
}

// Each prefix is suggested once first, as in resolve.rs, so first-touch page faults and the
// term lists kept for short prefixes are not in the measurement: this times a warm service.
fn suggest(bencher: Bencher, targets: &[&str]) {
    let qs = typed(targets);
    for q in &qs {
        FIXTURE.index.suggest(q, 5, None);
    }
    bencher.with_inputs(|| &*FIXTURE).bench_local_refs(|f| {
        qs.iter()
            .map(|q| f.index.suggest(black_box(q), 5, None).len())
            .sum::<usize>()
    });
}

#[divan::bench]
fn suggest_addresses(bencher: Bencher) {
    suggest(bencher, &["address"]);
}

#[divan::bench]
fn suggest_streets(bencher: Bencher) {
    suggest(bencher, &["street"]);
}

#[divan::bench]
fn suggest_postcodes(bencher: Bencher) {
    suggest(bencher, &["postcode"]);
}

#[divan::bench]
fn suggest_places(bencher: Bencher) {
    suggest(bencher, &["city", "region", "country"]);
}
