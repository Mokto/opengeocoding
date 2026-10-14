//! `Index::suggest` against a real index: expectations from public facts (a place's coordinates
//! and country), never from this service's answers. `INDEX_DIR` is a `build_index` output; the
//! tests that need it are ignored, as they need a world index. No libpostal models are read.
use opengeocoding::index::{Index, Parser, Suggestion};
use std::{path::Path, sync::Arc, time::Instant};

const GOLD: &str = include_str!("suggest.gold.tsv");

fn open() -> Index {
    let dir = std::env::var("INDEX_DIR").expect("INDEX_DIR names a build_index output");
    Index::open(Path::new(&dir), Arc::new(Parser::unloaded(0))).unwrap()
}

fn km(lat: f64, lon: f64, s: &Suggestion) -> f64 {
    let (dx, dy) = ((s.x - lon) * lat.to_radians().cos(), s.y - lat);
    dx.hypot(dy) * 111.0
}

struct Case<'a> {
    query: &'a str,
    country: &'a str,
    rank: usize,
    kind: &'a str,
    cc: &'a str,
    lat: f64,
    lon: f64,
    km: f64,
}

fn cases() -> Vec<Case<'static>> {
    GOLD.lines()
        .skip(1)
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            assert_eq!(c.len(), 8, "{l}");
            Case {
                query: c[0],
                country: c[1],
                rank: c[2].parse().unwrap(),
                kind: c[3],
                cc: c[4],
                lat: c[5].parse().unwrap(),
                lon: c[6].parse().unwrap(),
                km: c[7].parse().unwrap(),
            }
        })
        .collect()
}

#[test]
fn gold_file_is_well_formed() {
    let cases = cases();
    assert!(cases.len() >= 20);
    for c in cases {
        assert!((-90.0..=90.0).contains(&c.lat) && (-180.0..=180.0).contains(&c.lon));
        assert!(c.rank >= 1 && c.km > 0.0 && c.cc.len() == 2, "{}", c.query);
    }
}

/// Every case must have a suggestion at its country, kind and place within `rank`.
#[test]
#[ignore = "needs INDEX_DIR"]
fn gold() {
    let index = open();
    let mut failed = vec![];
    for c in cases() {
        let found = index.suggest(c.query, 5, Some(c.country).filter(|c| !c.is_empty()));
        let hit = found.iter().take(c.rank).any(|s| {
            s.country_code == c.cc
                && (c.kind.is_empty() || s.kind == c.kind)
                && km(c.lat, c.lon, s) <= c.km
        });
        if !hit {
            let got: Vec<_> = found
                .iter()
                .map(|s| {
                    format!(
                        "{} {} ({:.0} km)",
                        s.kind,
                        s.display_name,
                        km(c.lat, c.lon, s)
                    )
                })
                .collect();
            failed.push(format!(
                "{:?}: wanted a {} in {} near {},{}; got {got:?}",
                c.query, c.kind, c.cc, c.lat, c.lon
            ));
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {} failed:\n{}",
        failed.len(),
        cases().len(),
        failed.join("\n")
    );
}

/// Every prefix of public place and address strings, as typed, timed over the world index.
#[test]
#[ignore = "needs INDEX_DIR"]
fn latency() {
    const TYPED: &[&str] = &[
        "London",
        "Paris",
        "New York",
        "San Francisco",
        "Springfield, Illinois",
        "København",
        "Munich, Germany",
        "São Paulo",
        "Mexico City",
        "Sydney",
        "Mumbai",
        "Istanbul",
        "Cairo",
        "東京",
        "Москва",
        "10 Downing Street, London",
        "350 5th Avenue, New York",
        "1600 Pennsylvania Avenue NW, Washington",
        "Unter den Linden 77, Berlin",
        "Rådhuspladsen 1, København",
        "Main Street, Springfield",
        "Rue de Rivoli 12, Paris",
        "Bahnhofstrasse 10, Zürich",
        "Gran Via 28, Madrid",
        "Via del Corso 10, Roma",
        "SW1A 2AA",
        "75001",
        "10001",
        "1012 JS",
    ];
    let index = open();
    // as the service does after startup
    let started = Instant::now();
    index.warm_suggest();
    eprintln!("warm-up: {:?}", started.elapsed());
    // the first pass reads what the warm-up did not (three-letter prefixes, the ancestors of
    // towns); the second finds it kept
    for pass in ["first", "repeat"] {
        let mut micros = vec![];
        for text in TYPED {
            let chars: Vec<char> = text.chars().collect();
            for end in 2..=chars.len() {
                let typed: String = chars[..end].iter().collect();
                let t = Instant::now();
                index.suggest(&typed, 5, None);
                micros.push((t.elapsed().as_micros(), typed));
            }
        }
        report(pass, micros);
    }
}

fn report(pass: &str, mut micros: Vec<(u128, String)>) {
    micros.sort();
    for (us, typed) in micros.iter().rev().take(25) {
        eprintln!("  {us:>8} µs  {typed}");
    }
    let at = |q: f64| micros[((micros.len() - 1) as f64 * q) as usize].0;
    eprintln!(
        "{pass}: {} typed prefixes: p50 {} µs, p90 {} µs, p99 {} µs, max {} µs ({:?})",
        micros.len(),
        at(0.5),
        at(0.9),
        at(0.99),
        micros.last().unwrap().0,
        micros.last().unwrap().1
    );
    if pass == "repeat" {
        assert!(at(0.99) < 20_000, "p99 {} µs", at(0.99));
    }
}
