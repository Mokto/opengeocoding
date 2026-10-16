//! Streets by place, so a coordinate finds the houses near it. `reverse.bin` is a sorted file of
//! 10-byte records: the 48-bit Morton code of a house's position (24 bits of longitude and of
//! latitude, as `schema::pack` stores them) and the packed address of its street's document. A
//! street has one record per grid cell that holds a house of it, ordered by cell, so a lookup is a
//! binary search for the cell. The houses stay in the number FSTs, which a street's id prefixes.
//!
//! `streets.bin` holds the streets' lines the same way, for the streets nearest a point that has
//! no house near it: 12-byte records of a straight edge, filed by the Morton code of its near end,
//! with the far end's offset and the street's document. Street cells are larger (`STREET_BITS`),
//! and an edge is cut short enough to reach at most half a cell beyond its near end's.
use anyhow::{Context, Result, ensure};
use rayon::prelude::*;
use std::{
    cmp::Reverse as Rev,
    collections::BinaryHeap,
    fs,
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
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

pub const STREETS: &str = "streets.bin";
pub const STREETS_SPILL: &str = ".streets-spill";
const STREET_RECORD: usize = 12;
/// A street cell is 2^STREET_BITS steps: about 305 m by 611 m at the equator.
const STREET_BITS: u32 = 8;
const STREET_SHIFT: u32 = 2 * STREET_BITS;
/// The farthest an edge's far end lies from its near end, in steps of either coordinate.
const MAX_STEP: i64 = i8::MAX as i64;
/// The most of a street cell's side an edge reaches beyond its near end's cell: once `r` rings of
/// cells are read, every edge within `r - STREET_SLACK` cells of the point has been.
pub const STREET_SLACK: f64 = MAX_STEP as f64 / (1 << STREET_BITS) as f64;
pub const STREET_CELL_LAT_M: f64 = CELL_LAT_M * (1 << (STREET_BITS - CELL_BITS)) as f64;
pub const STREET_CELL_LON_M: f64 = 2.0 * STREET_CELL_LAT_M;
const STREET_CELLS: i64 = 1 << (24 - STREET_BITS);
/// Metres per latitude step; a longitude step is 2 cos(latitude) of them.
pub const STEP_LAT_M: f64 = 180.0 / STEPS * METERS_PER_DEGREE;
/// How far a simplified line may stray from the source line, in latitude steps: about 5 m.
const SIMPLIFY_STEPS: f64 = 4.0;
const STREET_SPILLS: usize = 64;

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

fn compact(v: u64) -> u32 {
    let mut v = v & 0x5555_5555_5555_5555;
    v = (v | v >> 1) & 0x3333_3333_3333_3333;
    v = (v | v >> 2) & 0x0f0f_0f0f_0f0f_0f0f;
    v = (v | v >> 4) & 0x00ff_00ff_00ff_00ff;
    v = (v | v >> 8) & 0x0000_ffff_0000_ffff;
    (v | v >> 16) as u32
}

/// The record key of a house from its `schema::pack`ed halves.
pub fn key(x: u32, y: u32) -> u64 {
    morton(x & 0xff_ffff, y >> 8)
}

fn cell_of_key(key: u64) -> u64 {
    key >> CELL_SHIFT
}

/// A point in steps of 2^-24 of either coordinate's range, as `schema::pack` stores it.
pub fn steps(lon: f64, lat: f64) -> (f64, f64) {
    ((lon + 180.0) / 360.0 * STEPS, (lat + 90.0) / 180.0 * STEPS)
}

/// The inverse of `steps`.
pub fn degrees(x: f64, y: f64) -> (f64, f64) {
    (x / STEPS * 360.0 - 180.0, y / STEPS * 180.0 - 90.0)
}

fn grid(lon: f64, lat: f64) -> (i64, i64) {
    let (x, y) = steps(lon, lat);
    let q = |v: f64| (v as i64).clamp(0, (1 << 24) - 1);
    (q(x), q(y))
}

/// The grid cell holding a point, as (column, row).
pub fn cell_at(lon: f64, lat: f64) -> (i64, i64) {
    let (x, y) = grid(lon, lat);
    (x >> CELL_BITS, y >> CELL_BITS)
}

/// The street cell holding a point, as (column, row).
pub fn street_cell_at(lon: f64, lat: f64) -> (i64, i64) {
    let (x, y) = grid(lon, lat);
    (x >> STREET_BITS, y >> STREET_BITS)
}

/// The distance from `p` to the segment from `a` to `b`, and the point of it nearest `p`.
pub fn nearest(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> (f64, (f64, f64)) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = dx * dx + dy * dy;
    let t = if len > 0.0 {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let q = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - q.0).hypot(p.1 - q.1), q)
}

/// Appends the edges of a line of (lon, lat) points as (Morton code of the near end, offset of
/// the far end): simplified to within about 5 m, then cut to fit `MAX_STEP`. A line across the
/// antimeridian is left out.
pub fn street_edges(line: &[[f64; 2]], out: &mut Vec<(u64, (i8, i8))>) {
    let mut pts: Vec<(i64, i64)> = line.iter().map(|p| grid(p[0], p[1])).collect();
    pts.dedup();
    if pts.len() < 2 || pts.windows(2).any(|w| (w[0].0 - w[1].0).abs() > 1 << 22) {
        return;
    }
    let sx = 2.0 * line[0][1].to_radians().cos();
    let at = |i: usize| (pts[i].0 as f64 * sx, pts[i].1 as f64);
    let mut keep = vec![false; pts.len()];
    (keep[0], keep[pts.len() - 1]) = (true, true);
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((i, j)) = stack.pop() {
        let (mut far, mut split) = (SIMPLIFY_STEPS, None);
        for k in i + 1..j {
            let d = nearest(at(k), at(i), at(j)).0;
            if d > far {
                (far, split) = (d, Some(k));
            }
        }
        if let Some(k) = split {
            keep[k] = true;
            stack.extend([(i, k), (k, j)]);
        }
    }
    let kept: Vec<(i64, i64)> = pts
        .iter()
        .zip(&keep)
        .filter(|p| *p.1)
        .map(|p| *p.0)
        .collect();
    for w in kept.windows(2) {
        let (p, d) = (w[0], (w[1].0 - w[0].0, w[1].1 - w[0].1));
        let n = (d.0.abs().max(d.1.abs()) + MAX_STEP - 1) / MAX_STEP;
        let at = |i: i64| (p.0 + d.0 * i / n, p.1 + d.1 * i / n);
        for i in 0..n {
            let (a, b) = (at(i), at(i + 1));
            let off = ((b.0 - a.0) as i8, (b.1 - a.1) as i8);
            out.push((morton(a.0 as u32, a.1 as u32), off));
        }
    }
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

/// Merges files of sorted records into one sequence, calling `emit` on each record in order.
fn merge_sorted<T: Ord>(
    dir: &Path,
    size: usize,
    decode: impl Fn(&[u8]) -> T,
    mut emit: impl FnMut(T) -> Result<()>,
) -> Result<()> {
    let mut readers: Vec<_> = fs::read_dir(dir)?
        .map(|e| {
            Ok(BufReader::with_capacity(
                1 << 20,
                fs::File::open(e?.path())?,
            ))
        })
        .collect::<Result<_>>()?;
    let mut rec = vec![0; size];
    let mut next = |r: &mut BufReader<fs::File>| -> Result<Option<T>> {
        match r.read_exact(&mut rec) {
            Ok(()) => Ok(Some(decode(&rec))),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(e) => Err(e.into()),
        }
    };
    let mut heap = BinaryHeap::new();
    for (i, r) in readers.iter_mut().enumerate() {
        if let Some(t) = next(r)? {
            heap.push(Rev((t, i)));
        }
    }
    while let Some(Rev((t, i))) = heap.pop() {
        emit(t)?;
        if let Some(t) = next(&mut readers[i])? {
            heap.push(Rev((t, i)));
        }
    }
    Ok(())
}

/// Merges the spilled shards into `reverse.bin` as `out`, replacing each street id with its
/// document's address. Streets with no document are dropped. Returns the records written.
pub fn merge(spill: &Path, out: &Path, doc_of: impl Fn(u32) -> Option<u32>) -> Result<u64> {
    let mut w = BufWriter::with_capacity(1 << 22, fs::File::create(out)?);
    let mut n = 0;
    let decode = |r: &[u8]| {
        let (k, sid) = decode(r);
        (cell_of_key(k), sid, k)
    };
    merge_sorted(spill, RECORD, decode, |(_, sid, k)| {
        if let Some(doc) = doc_of(sid) {
            w.write_all(&encode(k, doc))?;
            n += 1;
        }
        Ok(())
    })?;
    w.flush()?;
    Ok(n)
}

fn encode_edge(key: u64, (dx, dy): (i8, i8), id: u32) -> [u8; STREET_RECORD] {
    let mut r = [0; STREET_RECORD];
    r[..6].copy_from_slice(&key.to_be_bytes()[2..]);
    r[6..8].copy_from_slice(&[dx as u8, dy as u8]);
    r[8..].copy_from_slice(&id.to_be_bytes());
    r
}

fn decode_edge(r: &[u8]) -> (u64, (i8, i8), u32) {
    let key = u64::from_be_bytes([0, 0, r[0], r[1], r[2], r[3], r[4], r[5]]);
    (
        key,
        (r[6] as i8, r[7] as i8),
        u32::from_be_bytes([r[8], r[9], r[10], r[11]]),
    )
}

fn street_cell(key: u64) -> u64 {
    key >> STREET_SHIFT
}

/// Spills spread cells by a hash, so that none holds a dense region's share of the world.
fn spill_of(key: u64) -> usize {
    (street_cell(key).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 58) as usize % STREET_SPILLS
}

/// Street edges as the builder reads them, each with its street's id of the moment, which
/// `build` replaces with the street's document.
pub struct StreetSpill(Vec<Mutex<BufWriter<fs::File>>>);

impl StreetSpill {
    pub fn new(dir: &Path) -> StreetSpill {
        fs::create_dir_all(dir).unwrap();
        StreetSpill(
            (0..STREET_SPILLS)
                .map(|s| {
                    let f = fs::File::create(dir.join(format!("{s:02}"))).unwrap();
                    Mutex::new(BufWriter::with_capacity(1 << 20, f))
                })
                .collect(),
        )
    }

    /// Writes and clears one task's edges.
    pub fn add(&self, edges: &mut Vec<(u64, (i8, i8), u32)>) {
        edges.sort_unstable_by_key(|e| spill_of(e.0));
        let mut buf = vec![];
        for run in edges.chunk_by(|a, b| spill_of(a.0) == spill_of(b.0)) {
            buf.clear();
            for &(k, d, id) in run {
                buf.extend(encode_edge(k, d, id));
            }
            self.0[spill_of(run[0].0)]
                .lock()
                .unwrap()
                .write_all(&buf)
                .unwrap();
        }
        edges.clear();
    }

    pub fn finish(self) {
        for f in self.0 {
            f.into_inner().unwrap().flush().unwrap();
        }
    }
}

/// Writes `streets.bin` from the spill, with each street's id replaced by `doc_of`. Edges of
/// streets with no document are dropped. Returns the records written.
fn build_streets(dir: &Path, doc_of: impl Fn(u32) -> Option<u32> + Sync) -> Result<u64> {
    let spill = dir.join(STREETS_SPILL);
    let files: Vec<PathBuf> = fs::read_dir(&spill)?
        .map(|e| Ok(e?.path()))
        .collect::<Result<_>>()?;
    files.par_iter().try_for_each(|path| -> Result<()> {
        let bytes = fs::read(path)?;
        let mut recs: Vec<(u64, u32, u64, (i8, i8))> = bytes
            .as_chunks::<STREET_RECORD>()
            .0
            .iter()
            .filter_map(|r| {
                let (k, d, id) = decode_edge(r);
                Some((street_cell(k), doc_of(id)?, k, d))
            })
            .collect();
        drop(bytes);
        recs.par_sort_unstable();
        recs.dedup();
        let mut w = BufWriter::with_capacity(1 << 20, fs::File::create(path)?);
        for (_, doc, k, d) in recs {
            w.write_all(&encode_edge(k, d, doc))?;
        }
        Ok(w.flush()?)
    })?;
    let mut w = BufWriter::with_capacity(1 << 22, fs::File::create(dir.join(STREETS))?);
    let mut n = 0;
    let decode = |r: &[u8]| {
        let (k, d, doc) = decode_edge(r);
        (street_cell(k), doc, k, d)
    };
    merge_sorted(&spill, STREET_RECORD, decode, |(_, doc, k, d)| {
        n += 1;
        Ok(w.write_all(&encode_edge(k, d, doc))?)
    })?;
    w.flush()?;
    fs::remove_dir_all(spill)?;
    Ok(n)
}

/// Writes `reverse.bin` once the index is final: its street ids become document addresses, which
/// only the merged segments fix. With a street spill, `streets.bin` too, its ids made street ids
/// by `street_of`. Returns the records of each.
pub fn build(
    dir: &Path,
    index: &tantivy::Index,
    kind: Field,
    street_of: impl Fn(u32) -> Option<u32> + Sync,
) -> Result<(u64, u64)> {
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
    let doc_of = |sid: u32| docs.get(sid as usize).copied().filter(|&d| d != u32::MAX);
    let houses = merge(&spill, &dir.join(FILE), doc_of)?;
    fs::remove_dir_all(spill)?;
    let streets = if dir.join(STREETS_SPILL).exists() {
        build_streets(dir, |id| street_of(id).and_then(doc_of))?
    } else {
        0
    };
    Ok((houses, streets))
}

/// The records of cell `want` in a file of `size`-byte records sorted by a leading 6-byte Morton
/// code, whose cell is that code shifted right by `shift`.
fn cell_records(bytes: &[u8], size: usize, shift: u32, want: u64) -> impl Iterator<Item = &[u8]> {
    let cell =
        move |r: &[u8]| u64::from_be_bytes([0, 0, r[0], r[1], r[2], r[3], r[4], r[5]]) >> shift;
    let (mut lo, mut hi) = (0, bytes.len() / size);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if cell(&bytes[mid * size..]) < want {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    bytes[lo * size..]
        .chunks_exact(size)
        .take_while(move |r| cell(r) == want)
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
        out.extend(cell_records(&self.0, RECORD, CELL_SHIFT, want).map(|r| decode(r).1));
    }
}

pub type Point = (f64, f64);

/// The mapped `streets.bin`.
pub struct Streets(tantivy::directory::OwnedBytes);

impl Streets {
    pub fn new(bytes: tantivy::directory::OwnedBytes) -> Result<Streets> {
        ensure!(
            bytes.len().is_multiple_of(STREET_RECORD),
            "streets.bin is truncated"
        );
        Ok(Streets(bytes))
    }

    /// Appends the edges filed in street cell (`cx`, `cy`): both ends in steps (see `steps`) and
    /// the street's document.
    pub fn edges(&self, cx: i64, cy: i64, out: &mut Vec<(Point, Point, u32)>) {
        if !(0..STREET_CELLS).contains(&cx) || !(0..STREET_CELLS).contains(&cy) {
            return;
        }
        let want = morton(cx as u32, cy as u32);
        out.extend(
            cell_records(&self.0, STREET_RECORD, STREET_SHIFT, want).map(|r| {
                let (k, (dx, dy), doc) = decode_edge(r);
                // a step's centre, as the ends were rounded down to their step
                let (x, y) = (compact(k) as f64 + 0.5, compact(k >> 1) as f64 + 0.5);
                ((x, y), (x + dx as f64, y + dy as f64), doc)
            }),
        );
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

    #[test]
    fn street_lines_are_simplified_and_cut_to_fit_one_cell() {
        let ends = |e: &(u64, (i8, i8))| {
            let (x, y) = (compact(e.0) as i64, compact(e.0 >> 1) as i64);
            ((x, y), (x + e.1.0 as i64, y + e.1.1 as i64))
        };
        // 1 km east at 50° north, with a vertex every 10 m and one 2 m off the line
        let mut line: Vec<[f64; 2]> = (0..=100)
            .map(|i| [10.0 + i as f64 * 0.00014, 50.0])
            .collect();
        line[50][1] += 0.00002;
        let mut edges = vec![];
        street_edges(&line, &mut edges);
        let (a, b) = (grid(10.0, 50.0), grid(10.014, 50.0));
        assert_eq!(edges.len() as i64, (b.0 - a.0 + MAX_STEP - 1) / MAX_STEP);
        assert_eq!(ends(&edges[0]).0, a);
        assert_eq!(ends(edges.last().unwrap()).1, b);
        for w in edges.windows(2) {
            assert_eq!(ends(&w[0]).1, ends(&w[1]).0);
        }
        assert!(
            edges
                .iter()
                .all(|e| e.1.0.abs() as i64 <= MAX_STEP && e.1.1 == 0)
        );
        // a corner 50 m off the line stays
        let mut corner = vec![];
        street_edges(
            &[[10.0, 50.0], [10.0005, 50.00045], [10.001, 50.0]],
            &mut corner,
        );
        assert_eq!(corner.len(), 2);
        // across the antimeridian, or a single point: nothing
        let mut none = vec![];
        street_edges(&[[179.9, 0.0], [-179.9, 0.0]], &mut none);
        street_edges(&[[1.0, 1.0], [1.0, 1.0]], &mut none);
        assert!(none.is_empty());
    }

    #[test]
    fn street_edges_are_filed_by_cell_with_their_documents() {
        let dir = std::env::temp_dir().join(format!("streets-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let spill = StreetSpill::new(&dir.join(STREETS_SPILL));
        let mut edges = vec![];
        street_edges(&[[10.0, 50.0], [10.0001, 50.0001]], &mut edges);
        let mut far = vec![];
        street_edges(&[[-70.0, -30.0], [-70.0001, -30.0]], &mut far);
        let mut recs: Vec<_> = edges.iter().map(|&(k, d)| (k, d, 1)).collect();
        recs.extend(far.iter().map(|&(k, d)| (k, d, 2)));
        recs.extend(edges.iter().map(|&(k, d)| (k, d, 3)));
        spill.add(&mut recs);
        assert!(recs.is_empty());
        spill.finish();
        // id 3 has no document
        let n = build_streets(&dir, |id| (id != 3).then_some(id * 10)).unwrap();
        assert_eq!(n, 2);
        assert!(!dir.join(STREETS_SPILL).exists());
        let streets = Streets::new(tantivy::directory::OwnedBytes::new(
            fs::read(dir.join(STREETS)).unwrap(),
        ))
        .unwrap();
        let mut found = vec![];
        let (cx, cy) = street_cell_at(10.0, 50.0);
        streets.edges(cx, cy, &mut found);
        assert_eq!(found.len(), 1);
        let ((ax, ay), (bx, by), doc) = found[0];
        assert_eq!(doc, 10);
        let (a, b) = (degrees(ax, ay), degrees(bx, by));
        assert!(
            (a.0 - 10.0).abs() < 1e-4 && (a.1 - 50.0).abs() < 1e-4,
            "{a:?}"
        );
        assert!(
            (b.0 - 10.0001).abs() < 1e-4 && (b.1 - 50.0001).abs() < 1e-4,
            "{b:?}"
        );
        found.clear();
        let (fx, fy) = street_cell_at(-70.0, -30.0);
        streets.edges(fx, fy, &mut found);
        assert_eq!(found.iter().map(|e| e.2).collect::<Vec<_>>(), [20]);
        found.clear();
        streets.edges(cx + 1, cy, &mut found);
        streets.edges(-1, 0, &mut found);
        assert!(found.is_empty());
        let (d, q) = nearest((0.0, 1.0), (-1.0, 0.0), (1.0, 0.0));
        assert_eq!((d, q), (1.0, (0.0, 0.0)));
        assert_eq!(nearest((5.0, 0.0), (-1.0, 0.0), (1.0, 0.0)).0, 4.0);
        fs::remove_dir_all(&dir).unwrap();
    }
}
