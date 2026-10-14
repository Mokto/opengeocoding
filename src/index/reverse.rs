//! Streets by place, so a coordinate finds the houses near it. `reverse.bin` is a sorted file of
//! 10-byte records: the 48-bit Morton code of a house's position (24 bits of longitude and of
//! latitude, as `schema::pack` stores them) and the packed address of its street's document. A
//! street has one record per grid cell that holds a house of it, ordered by cell, so a lookup is a
//! binary search for the cell. The houses stay in the number FSTs, which a street's id prefixes.
use anyhow::{Context, Result, ensure};
use rayon::prelude::*;
use std::{
    cmp::Reverse as Rev,
    collections::BinaryHeap,
    fs,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};
use tantivy::{
    DocSet, Term,
    schema::{Field, IndexRecordOption},
};

pub const FILE: &str = "reverse.bin";
pub const SPILL: &str = ".reverse-spill";
const RECORD: usize = 10;
/// A cell is 2^CELL_BITS steps of either coordinate: about 76 m by 153 m at the equator, and 76 m
/// by 76 m at 60° north.
const CELL_BITS: u32 = 6;
const CELL_SHIFT: u32 = 2 * CELL_BITS;
const STEPS: f64 = (1u64 << 24) as f64;
const METERS_PER_DEGREE: f64 = 111_195.0;
/// Metres per cell on the latitude axis, and on the longitude axis at the equator.
pub const CELL_LAT_M: f64 = 180.0 / STEPS * (1 << CELL_BITS) as f64 * METERS_PER_DEGREE;
pub const CELL_LON_M: f64 = 2.0 * CELL_LAT_M;
const CELLS: i64 = 1 << (24 - CELL_BITS);

fn spread(v: u32) -> u64 {
    let mut v = v as u64;
    v = (v | v << 16) & 0x0000_ffff_0000_ffff;
    v = (v | v << 8) & 0x00ff_00ff_00ff_00ff;
    v = (v | v << 4) & 0x0f0f_0f0f_0f0f_0f0f;
    v = (v | v << 2) & 0x3333_3333_3333_3333;
    (v | v << 1) & 0x5555_5555_5555_5555
}

fn morton(x: u32, y: u32) -> u64 {
    spread(x) | spread(y) << 1
}

/// The record key of a house from its `schema::pack`ed halves.
pub fn key(x: u32, y: u32) -> u64 {
    morton(x & 0xff_ffff, y >> 8)
}

fn cell_of_key(key: u64) -> u64 {
    key >> CELL_SHIFT
}

/// The grid cell holding a point, as (column, row).
pub fn cell_at(lon: f64, lat: f64) -> (i64, i64) {
    let q = |v: f64, span: f64| ((v / span * STEPS) as i64).clamp(0, (1 << 24) - 1) >> CELL_BITS;
    (q(lon + 180.0, 360.0), q(lat + 90.0, 180.0))
}

/// A street document's address in the final index. 4 bits of segment and 28 of document.
pub fn pack_doc(segment: u32, doc: u32) -> Result<u32> {
    ensure!(
        segment < 16 && doc < 1 << 28,
        "document {segment}:{doc} does not fit reverse.bin"
    );
    Ok(segment << 28 | doc)
}

pub fn unpack_doc(packed: u32) -> (u32, u32) {
    (packed >> 28, packed & 0x0fff_ffff)
}

/// One street's houses: a record for each cell they occupy, at the first house in it.
pub fn street_postings(keys: &mut Vec<u64>, sid: u32, out: &mut Vec<(u64, u32)>) {
    keys.sort_unstable();
    keys.dedup_by_key(|k| cell_of_key(*k));
    out.extend(keys.drain(..).map(|k| (k, sid)));
}

fn encode(key: u64, id: u32) -> [u8; RECORD] {
    let mut r = [0; RECORD];
    r[..6].copy_from_slice(&key.to_be_bytes()[2..]);
    r[6..].copy_from_slice(&id.to_be_bytes());
    r
}

fn decode(r: &[u8]) -> (u64, u32) {
    let key = u64::from_be_bytes([0, 0, r[0], r[1], r[2], r[3], r[4], r[5]]);
    (key, u32::from_be_bytes([r[6], r[7], r[8], r[9]]))
}

/// One number shard's postings, ordered by cell then street, in the spill directory.
pub fn write_spill(dir: &Path, shard: usize, mut posts: Vec<(u64, u32)>) {
    fs::create_dir_all(dir).unwrap();
    posts.par_sort_unstable_by_key(|&(k, sid)| (cell_of_key(k), sid, k));
    let mut w = BufWriter::new(fs::File::create(dir.join(format!("{shard:02}"))).unwrap());
    for (k, sid) in posts {
        w.write_all(&encode(k, sid)).unwrap();
    }
    w.flush().unwrap();
}

/// Merges the spilled shards into `reverse.bin` as `out`, replacing each street id with its
/// document's address. Streets with no document are dropped. Returns the records written.
pub fn merge(spill: &Path, out: &Path, doc_of: impl Fn(u32) -> Option<u32>) -> Result<u64> {
    let mut readers: Vec<_> = fs::read_dir(spill)?
        .map(|e| {
            Ok(BufReader::with_capacity(
                1 << 20,
                fs::File::open(e?.path())?,
            ))
        })
        .collect::<Result<_>>()?;
    let next = |r: &mut BufReader<fs::File>| -> Result<Option<(u64, u32)>> {
        let mut rec = [0; RECORD];
        match r.read_exact(&mut rec) {
            Ok(()) => Ok(Some(decode(&rec))),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e.into()),
        }
    };
    let mut heap = BinaryHeap::new();
    for (i, r) in readers.iter_mut().enumerate() {
        if let Some((k, sid)) = next(r)? {
            heap.push(Rev((cell_of_key(k), sid, k, i)));
        }
    }
    let mut w = BufWriter::with_capacity(1 << 22, fs::File::create(out)?);
    let mut n = 0;
    while let Some(Rev((_, sid, k, i))) = heap.pop() {
        if let Some(doc) = doc_of(sid) {
            w.write_all(&encode(k, doc))?;
            n += 1;
        }
        if let Some((k, sid)) = next(&mut readers[i])? {
            heap.push(Rev((cell_of_key(k), sid, k, i)));
        }
    }
    w.flush()?;
    Ok(n)
}

/// Writes `reverse.bin` once the index is final: its street ids become document addresses, which
/// only the merged segments fix.
pub fn build(dir: &Path, index: &tantivy::Index, kind: Field) -> Result<u64> {
    let searcher = index.reader()?.searcher();
    let street = Term::from_field_text(kind, "s");
    let mut docs: Vec<u32> = vec![];
    for (ord, segment) in searcher.segment_readers().iter().enumerate() {
        let ids = segment.fast_fields().u64("id")?;
        let Some(mut p) = segment
            .inverted_index(kind)?
            .read_postings(&street, IndexRecordOption::Basic)?
        else {
            continue;
        };
        while p.doc() != tantivy::TERMINATED {
            let sid = ids.first(p.doc()).context("street without id")? as usize;
            if sid >= docs.len() {
                docs.resize(sid + 1, u32::MAX);
            }
            docs[sid] = pack_doc(ord as u32, p.doc())?;
            p.advance();
        }
    }
    let spill = dir.join(SPILL);
    let n = merge(&spill, &dir.join(FILE), |sid| {
        docs.get(sid as usize).copied().filter(|&d| d != u32::MAX)
    })?;
    fs::remove_dir_all(spill)?;
    Ok(n)
}

/// The mapped `reverse.bin`.
pub struct Reverse(tantivy::directory::OwnedBytes);

impl Reverse {
    pub fn new(bytes: tantivy::directory::OwnedBytes) -> Result<Reverse> {
        ensure!(
            bytes.len().is_multiple_of(RECORD),
            "reverse.bin is truncated"
        );
        Ok(Reverse(bytes))
    }

    /// Appends the street documents with a house in cell (`cx`, `cy`).
    pub fn docs(&self, cx: i64, cy: i64, out: &mut Vec<u32>) {
        if !(0..CELLS).contains(&cx) || !(0..CELLS).contains(&cy) {
            return;
        }
        let want = morton(cx as u32, cy as u32);
        let bytes = self.0.as_slice();
        let record = |i: usize| decode(&bytes[i * RECORD..(i + 1) * RECORD]);
        let (mut lo, mut hi) = (0, bytes.len() / RECORD);
        while lo < hi {
            let mid = (lo + hi) / 2;
            if cell_of_key(record(mid).0) < want {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        while lo < bytes.len() / RECORD {
            let (k, doc) = record(lo);
            if cell_of_key(k) != want {
                break;
            }
            out.push(doc);
            lo += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_follow_cells() {
        for (lon, lat) in [
            (12.5683, 55.6761),
            (-73.9857, 40.7484),
            (151.2, -33.87),
            (0.0, 0.0),
        ] {
            // whatever postcode slot the packed longitude carries
            let (x, y) = super::super::schema::pack(lon, lat, 7);
            let (cx, cy) = cell_at(lon, lat);
            assert_eq!(
                cell_of_key(key(x, y)),
                morton(cx as u32, cy as u32),
                "{lon} {lat}"
            );
        }
        assert_eq!(cell_at(-180.0, -90.0), (0, 0));
        assert_eq!(cell_at(180.0, 90.0), (CELLS - 1, CELLS - 1));
        assert_eq!(cell_at(10.0, 50.0).0 + 1, cell_at(10.0 + 0.0014, 50.0).0);
    }

    #[test]
    fn records_merge_by_cell_and_find_by_cell() {
        let dir = std::env::temp_dir().join(format!("reverse-merge-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let spill = dir.join(SPILL);
        let at = |lon, lat| {
            let (x, y) = super::super::schema::pack(lon, lat, 0);
            key(x, y)
        };
        let (a, b) = (at(10.0, 50.0), at(10.0002, 50.0));
        let far = at(11.0, 51.0);
        // street 1 has two houses in one cell, street 2 one house there and one far away
        let mut posts = vec![];
        street_postings(&mut vec![b, a], 1, &mut posts);
        write_spill(&spill, 0, posts);
        let mut posts = vec![];
        street_postings(&mut vec![far, a], 2, &mut posts);
        street_postings(&mut vec![], 4, &mut posts);
        write_spill(&spill, 1, posts);
        let n = merge(&spill, &dir.join(FILE), |sid| (sid != 3).then(|| sid * 10)).unwrap();
        assert_eq!(n, 3);
        let reverse = Reverse::new(tantivy::directory::OwnedBytes::new(
            fs::read(dir.join(FILE)).unwrap(),
        ))
        .unwrap();
        let (cx, cy) = cell_at(10.0, 50.0);
        let mut docs = vec![];
        reverse.docs(cx, cy, &mut docs);
        assert_eq!(docs, [10, 20]);
        docs.clear();
        let (fx, fy) = cell_at(11.0, 51.0);
        reverse.docs(fx, fy, &mut docs);
        assert_eq!(docs, [20]);
        docs.clear();
        reverse.docs(cx + 5, cy, &mut docs);
        reverse.docs(-1, 0, &mut docs);
        assert!(docs.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }
}
