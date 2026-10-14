//! Model-backed libpostal parsing of the non-held-out rows of `tests/addresses.tsv`. Runs only
//! where CI sets up the fixture and models (`INDEX_DIR`). Index searches are timed end to end by
//! the full-index gold check instead (`gold-speed`).
use divan::{Bencher, black_box};
use opengeocoding::index::Parser;
use std::{sync::Arc, sync::LazyLock};

struct Fixture {
    parser: Arc<Parser>,
    rows: Vec<Row>,
}

struct Row {
    target: String,
    script: String,
    query: String,
}

static FIXTURE: LazyLock<Fixture> = LazyLock::new(|| {
    let parser = Arc::new(Parser::setup_with_cache(0).expect("libpostal models"));
    let rows = include_str!("../tests/addresses.tsv")
        .lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<_> = l.split('\t').collect();
            (c[6] == "0").then(|| Row {
                target: c[1].into(),
                script: c[5].into(),
                query: c[8].into(),
            })
        })
        .collect();
    Fixture { parser, rows }
});

fn main() {
    if std::env::var_os("INDEX_DIR").is_none() {
        eprintln!("INDEX_DIR is not set: skipping the model-backed benchmarks");
        return;
    }
    divan::main();
}

fn queries(keep: impl Fn(&Row) -> bool) -> Vec<String> {
    FIXTURE
        .rows
        .iter()
        .filter(|r| keep(r))
        .map(|r| r.query.clone())
        .collect()
}

// CodSpeed's simulation runs each benchmark once, cold: without this pass the page faults and
// lazy loads of the first touch land in the measurement and are reported as system time.
fn parse(bencher: Bencher, qs: Vec<String>) {
    for q in &qs {
        FIXTURE.parser.parse(q).unwrap();
    }
    bencher.with_inputs(|| &*FIXTURE).bench_local_refs(|f| {
        qs.iter()
            .map(|q| f.parser.parse(black_box(q)).unwrap().len())
            .sum::<usize>()
    });
}

#[divan::bench]
fn parse_addresses(bencher: Bencher) {
    parse(bencher, queries(|r| r.target == "address"));
}

#[divan::bench]
fn parse_streets_and_postcodes(bencher: Bencher) {
    parse(
        bencher,
        queries(|r| matches!(r.target.as_str(), "street" | "postcode")),
    );
}

#[divan::bench]
fn parse_places(bencher: Bencher) {
    parse(
        bencher,
        queries(|r| matches!(r.target.as_str(), "city" | "region" | "country")),
    );
}

#[divan::bench]
fn parse_unparsed(bencher: Bencher) {
    parse(bencher, queries(|r| r.target == "unparsed"));
}

#[divan::bench]
fn parse_non_latin(bencher: Bencher) {
    parse(bencher, queries(|r| r.script == "non_latin"));
}
