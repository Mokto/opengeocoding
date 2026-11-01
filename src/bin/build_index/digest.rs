//! `build_index digest DIR [--dump FILE]`: checksums of an index's logical content, so two builds
//! of the same sources can be compared without their segment layout or document order (which
//! are internal). A reproducible build prints the same line for each part. `--dump` also writes
//! every line, sorted, for `diff`; it keeps them all in memory, so use it on regional indexes.

use anyhow::{Context, Result};
use opengeocoding::index::schema::{self, unpack};
use rayon::prelude::*;
use std::{
    collections::{BTreeMap, HashMap},
    hash::{BuildHasher, BuildHasherDefault, DefaultHasher},
    io::Write,
    path::Path,
};
use tantivy::{
    TantivyDocument,
    schema::{Field, Value},
};
use tantivy_fst::Streamer;

/// A checksum of a multiset of lines: the same lines give the same sums in any order.
#[derive(Default)]
struct Acc {
    n: u64,
    sum: u64,
    xor: u64,
    lines: Vec<String>,
}

impl Acc {
    fn add(&mut self, line: String, keep: bool) {
        let h = BuildHasherDefault::<DefaultHasher>::default().hash_one(&line);
        (self.n, self.sum, self.xor) = (self.n + 1, self.sum.wrapping_add(h), self.xor ^ h);
        if keep {
            self.lines.push(line);
        }
    }

    fn merge(mut self, mut other: Acc) -> Acc {
        (self.n, self.sum, self.xor) = (
            self.n + other.n,
            self.sum.wrapping_add(other.sum),
            self.xor ^ other.xor,
        );
        self.lines.append(&mut other.lines);
        self
    }

    fn show(&self, name: &str) {
        println!("{name}: {} {:016x}{:016x}", self.n, self.sum, self.xor);
    }
}

pub fn digest(args: &[String]) -> Result<()> {
    let dir = Path::new(
        args.get(2)
            .context("usage: build_index digest DIR [--dump FILE]")?,
    );
    let dump = args.windows(2).find(|w| w[0] == "--dump").map(|w| &w[1]);
    let keep = dump.is_some();
    let index = tantivy::Index::open_in_dir(dir)?;
    index.tokenizers().register("fold", schema::analyzer());
    let searcher = index.reader()?.searcher();
    let (_, f) = schema::schema();
    let strings = |d: &TantivyDocument, field: Field| -> Vec<String> {
        d.get_all(field)
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    };

    // street id -> (what identifies the street, its stored postcodes in slot order)
    type Street = (String, Vec<String>);
    let per_segment: Vec<(Acc, Vec<(u64, Street)>)> = searcher
        .segment_readers()
        .par_iter()
        .map(|reader| -> Result<_> {
            let fast = reader.fast_fields();
            let (id, place) = (fast.u64("id")?, fast.u64("place")?);
            let (importance, x, y) = (fast.u64("importance")?, fast.f64("x")?, fast.f64("y")?);
            let store = reader.get_store_reader(64)?;
            let (mut docs, mut streets) = (Acc::default(), vec![]);
            for doc in 0..reader.max_doc() {
                let d: TantivyDocument = store.get(doc)?;
                let (names, pcs) = (strings(&d, f.name), strings(&d, f.pcode));
                let bits = |c: &tantivy::columnar::Column<f64>| {
                    c.first(doc)
                        .map_or("-".to_string(), |v| v.to_bits().to_string())
                };
                let kind = match (place.index.has_value(doc), id.index.has_value(doc)) {
                    (false, _) => "p",
                    (true, true) => "s",
                    (true, false) => "z",
                };
                let place = place.first(doc).map_or(String::new(), |p| p.to_string());
                let who = format!(
                    "{kind}|{}|{place}|{}|{}",
                    names.join("/"),
                    bits(&x),
                    bits(&y)
                );
                docs.add(
                    format!(
                        "{who}|{}|{}|{:?}|{:?}",
                        pcs.join("/"),
                        importance.first(doc).unwrap_or(0),
                        id.first(doc),
                        strings(&d, f.postal_town)
                    ),
                    keep,
                );
                if kind == "s" {
                    streets.push((id.first(doc).unwrap_or(0), (who, pcs)));
                }
            }
            Ok((docs, streets))
        })
        .collect::<Result<_>>()?;
    let mut docs = Acc::default();
    let mut streets: HashMap<u64, Street> = HashMap::new();
    for (acc, found) in per_segment {
        docs = docs.merge(acc);
        streets.extend(found);
    }

    let mut terms = Acc::default();
    for (name, field) in [
        ("names", f.names),
        ("city", f.city),
        ("country", f.country),
        ("ids", f.ids),
        ("pcode", f.pcode),
        ("cnames", f.cnames),
        ("ccity", f.ccity),
        ("small_names", f.small_names),
        ("small_cnames", f.small_cnames),
        ("kind", f.kind),
    ] {
        // a term's document count is the sum over segments, so sum before hashing
        let mut counts: BTreeMap<Vec<u8>, u64> = BTreeMap::new();
        for reader in searcher.segment_readers() {
            let inverted = reader.inverted_index(field)?;
            let mut stream = inverted.terms().stream()?;
            while stream.advance() {
                *counts.entry(stream.key().to_vec()).or_default() += stream.value().doc_freq as u64;
            }
        }
        for (t, n) in counts {
            terms.add(format!("{name}|{}|{n}", String::from_utf8_lossy(&t)), keep);
        }
    }

    let shards: usize = std::fs::read_to_string(dir.join("numbers.shards"))?
        .trim()
        .parse()?;
    let numbers = (0..shards)
        .into_par_iter()
        .map(|s| -> Result<Acc> {
            let map = tantivy_fst::Map::from_bytes(std::fs::read(
                dir.join(format!("numbers-{s:02}.fst")),
            )?)?;
            let mut acc = Acc::default();
            let mut it = map.stream();
            while let Some((key, value)) = it.next() {
                let sid = u32::from_be_bytes(key[..4].try_into()?) as u64;
                let (who, pcs) = streets
                    .get(&sid)
                    .map_or(("?", &[][..]), |(w, p)| (w, &p[..]));
                let slot = unpack(value).2 as usize;
                let pc = slot
                    .checked_sub(1)
                    .and_then(|i| pcs.get(i))
                    .map_or("", String::as_str);
                acc.add(
                    format!(
                        "{who}|{}|{}|{}|{pc}",
                        String::from_utf8_lossy(&key[4..]),
                        (value >> 32) as u32 & 0xff_ffff,
                        value as u32
                    ),
                    keep,
                );
            }
            Ok(acc)
        })
        .try_reduce(Acc::default, |a, b| Ok(a.merge(b)))?;

    let mut landmarks = Acc::default();
    for line in std::fs::read_to_string(dir.join("landmarks.tsv"))
        .unwrap_or_default()
        .lines()
    {
        landmarks.add(line.to_string(), keep);
    }

    docs.show("docs");
    terms.show("terms");
    numbers.show("numbers");
    landmarks.show("landmarks");
    if let Some(path) = dump {
        let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
        for (name, acc) in [
            ("doc", docs),
            ("term", terms),
            ("number", numbers),
            ("landmark", landmarks),
        ] {
            let mut lines = acc.lines;
            lines.sort_unstable();
            for l in lines {
                writeln!(out, "{name} {l}")?;
            }
        }
    }
    Ok(())
}
