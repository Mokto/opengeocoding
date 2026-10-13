//! Model-backed libpostal parsing and searches against a built index, over the non-held-out
//! rows of `tests/addresses.tsv`. Needs libpostal's models and `INDEX_DIR`; without the
//! index it runs nothing. Queries whose country the index lacks still parse, but miss.
//! `BENCH_COUNTRIES` (comma-separated ISO codes) picks the rows; set it to the countries
//! your index covers.
use divan::{Bencher, black_box};
use opengeocoding::index::{Index, Parser};
use std::{path::PathBuf, sync::Arc, sync::LazyLock};

const COUNTRIES: &str = "dk,nl,fr,ch,no,es";

struct Fixture {
    parser: Arc<Parser>,
    index: Index,
    queries: Vec<(String, String)>,
}

static FIXTURE: LazyLock<Fixture> = LazyLock::new(|| {
    let dir = PathBuf::from(std::env::var_os("INDEX_DIR").expect("INDEX_DIR"));
    let countries = std::env::var("BENCH_COUNTRIES").unwrap_or_else(|_| COUNTRIES.into());
    let parser = Arc::new(Parser::setup().expect("libpostal models"));
    let index = Index::open(&dir, parser.clone()).expect("index");
    let queries = include_str!("../tests/addresses.tsv")
        .lines()
        .skip(1)
        .filter_map(|l| {
            let c: Vec<_> = l.split('\t').collect();
            (c[6] == "0" && countries.split(',').any(|x| x == c[3]))
                .then(|| (c[1].to_string(), c[8].to_string()))
        })
        .collect();
    Fixture {
        parser,
        index,
        queries,
    }
});

fn main() {
    if std::env::var_os("INDEX_DIR").is_none() {
        eprintln!("INDEX_DIR is not set: skipping the model-backed benchmarks");
        return;
    }
    divan::main();
}

fn queries(targets: &[&str]) -> Vec<String> {
    FIXTURE
        .queries
        .iter()
        .filter(|(t, _)| targets.contains(&t.as_str()))
        .map(|(_, q)| q.clone())
        .collect()
}

#[divan::bench]
fn parse_all(bencher: Bencher) {
    let all = queries(&[
        "address", "street", "postcode", "city", "region", "country", "unparsed",
    ]);
    bencher.with_inputs(|| &*FIXTURE).bench_local_refs(|f| {
        all.iter()
            .map(|q| f.parser.parse(black_box(q)).unwrap().len())
            .sum::<usize>()
    });
}

fn search(bencher: Bencher, targets: &[&str]) {
    let qs = queries(targets);
    bencher.with_inputs(|| &*FIXTURE).bench_local_refs(|f| {
        qs.iter()
            .map(|q| f.index.search(black_box(q)).len())
            .sum::<usize>()
    });
}

#[divan::bench]
fn search_addresses(bencher: Bencher) {
    search(bencher, &["address"]);
}

#[divan::bench]
fn search_streets(bencher: Bencher) {
    search(bencher, &["street"]);
}

#[divan::bench]
fn search_postcodes(bencher: Bencher) {
    search(bencher, &["postcode"]);
}

#[divan::bench]
fn search_places(bencher: Bencher) {
    search(bencher, &["city", "region", "country"]);
}
