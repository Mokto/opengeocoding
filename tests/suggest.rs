//! `Index::suggest` against a real index: expectations from public facts (a place's coordinates
//! and country), never from this service's answers. `INDEX_DIR` is a `build_index` output; the
//! tests that need it are ignored, as they need a world index. No libpostal models are read.
use opengeocoding::index::{Focus, Index, Parser, Suggestion};
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
    /// Every prefix of the query this long or longer must also find the place (any of the 5).
    from: Option<usize>,
    /// The user's position, "lat,lon" or "lat,lon,scale_km" (default 100).
    focus: Option<Focus>,
    /// Why the case is known to fail; it must keep failing until the label is removed.
    known_failure: &'a str,
}

fn cases() -> Vec<Case<'static>> {
    GOLD.lines()
        .skip(1)
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            assert_eq!(c.len(), 11, "{l}");
            Case {
                query: c[0],
                country: c[1],
                rank: c[2].parse().unwrap(),
                kind: c[3],
                cc: c[4],
                lat: c[5].parse().unwrap(),
                lon: c[6].parse().unwrap(),
                km: c[7].parse().unwrap(),
                from: c[8].parse().ok(),
                focus: (!c[9].is_empty()).then(|| {
                    let n: Vec<f64> = c[9].split(',').map(|v| v.parse().unwrap()).collect();
                    Focus {
                        lat: n[0],
                        lon: n[1],
                        scale_km: n.get(2).copied().unwrap_or(100.0),
                    }
                }),
                known_failure: c[10],
            }
        })
        .collect()
}

impl Case<'_> {
    fn suggest(&self, index: &Index, text: &str) -> Vec<Suggestion> {
        let cc = Some(self.country).filter(|c| !c.is_empty());
        index.suggest(text, 5, cc, self.focus.as_ref(), false)
    }

    fn hit(&self, found: &[Suggestion], rank: usize) -> bool {
        found.iter().take(rank).any(|s| {
            s.country_code == self.cc
                && (self.kind.is_empty() || s.kind == self.kind)
                && km(self.lat, self.lon, s) <= self.km
        })
    }

    fn got(&self, found: &[Suggestion]) -> Vec<String> {
        found
            .iter()
            .map(|s| {
                format!(
                    "{} {} ({:.0} km)",
                    s.kind,
                    s.display_name,
                    km(self.lat, self.lon, s)
                )
            })
            .collect()
    }
}

#[test]
fn gold_file_is_well_formed() {
    let cases = cases();
    assert!(cases.len() >= 150);
    let mut seen = std::collections::HashSet::new();
    for c in &cases {
        assert!((-90.0..=90.0).contains(&c.lat) && (-180.0..=180.0).contains(&c.lon));
        assert!(
            (1..=5).contains(&c.rank) && c.km > 0.0 && c.cc.len() == 2,
            "{}",
            c.query
        );
        assert!(c.country.is_empty() || c.country.len() == 2, "{}", c.query);
        assert!(c.country.is_empty() || c.country == c.cc, "{}", c.query);
        assert!(
            c.focus.is_none_or(|f| (-90.0..=90.0).contains(&f.lat)
                && (-180.0..=180.0).contains(&f.lon)
                && f.scale_km > 0.0),
            "{}",
            c.query
        );
        assert!(
            c.from.is_none_or(|f| f <= c.query.chars().count()),
            "{}",
            c.query
        );
        assert!(
            seen.insert((
                c.query,
                c.country,
                c.rank,
                c.focus.map(|f| format!("{},{}", f.lat, f.lon))
            )),
            "duplicate {}",
            c.query
        );
    }
}

impl Case<'_> {
    /// What is wrong with the answers to the query, and to each of its prefixes from `from` on.
    fn problem(&self, index: &Index) -> Option<String> {
        let chars: Vec<char> = self.query.chars().collect();
        for n in self.from.unwrap_or(chars.len())..=chars.len() {
            let text: String = chars[..n].iter().collect();
            let found = self.suggest(index, &text);
            let ids: std::collections::HashSet<_> = found.iter().map(|s| &s.id).collect();
            if found.len() > 5 || ids.len() != found.len() {
                return Some(format!(
                    "{text:?}: too many or repeated: {:?}",
                    self.got(&found)
                ));
            }
            if let Some(s) = found
                .iter()
                .find(|s| !self.country.is_empty() && s.country_code != self.country)
            {
                return Some(format!(
                    "{text:?} in {}: {} is in {}",
                    self.country, s.display_name, s.country_code
                ));
            }
            // a prefix only has to find the place somewhere; the whole query has to rank it
            let rank = if n == chars.len() { self.rank } else { 5 };
            if !self.hit(&found, rank) {
                return Some(format!(
                    "{text:?}: wanted a {} in {} near {},{} within {rank}; got {:?}",
                    self.kind,
                    self.cc,
                    self.lat,
                    self.lon,
                    self.got(&found)
                ));
            }
        }
        None
    }
}

/// Every case must have a suggestion at its country, kind and place within `rank`, and so must
/// every prefix of its query from `from` characters on. Whatever the text, the answer respects
/// the limit and the country filter and repeats no id. A known failure must still fail.
#[test]
#[ignore = "needs INDEX_DIR"]
fn gold() {
    let index = open();
    let all = cases();
    let mut bad = vec![];
    for c in &all {
        match (c.problem(&index), c.known_failure.is_empty()) {
            (Some(p), true) => bad.push(p),
            (None, false) => bad.push(format!(
                "{:?} now passes: remove its known_failure",
                c.query
            )),
            _ => {}
        }
    }
    let known = all.iter().filter(|c| !c.known_failure.is_empty()).count();
    eprintln!("{} cases, {known} known failures", all.len());
    assert!(
        bad.is_empty(),
        "{} problems:\n{}",
        bad.len(),
        bad.join("\n")
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
    // towns); the second finds it kept. Then the same with landmarks, whose first pass finds the
    // places kept.
    for (landmarks, pass) in [
        (false, "first"),
        (false, "repeat"),
        (true, "first with landmarks"),
        (true, "repeat with landmarks"),
    ] {
        let mut micros = vec![];
        for text in TYPED {
            let chars: Vec<char> = text.chars().collect();
            for end in 2..=chars.len() {
                let typed: String = chars[..end].iter().collect();
                let t = Instant::now();
                index.suggest(&typed, 5, None, None, landmarks);
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
    if pass.starts_with("repeat") {
        assert!(at(0.99) < 20_000, "p99 {} µs", at(0.99));
    }
}
