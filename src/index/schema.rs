use super::canon::Canon;
use rayon::prelude::*;
use std::{
    fs,
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed},
    },
};
use tantivy::{
    Index, IndexWriter, TantivyDocument,
    schema::{
        FAST, Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
    },
    tokenizer::{AsciiFoldingFilter, LowerCaser, RemoveLongFilter, TextAnalyzer},
};

/// Query-time normalization must use the same analyzer.
pub fn analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(super::canon::Unmarked::default())
        .filter(RemoveLongFilter::limit(40))
        .filter(LowerCaser)
        .filter(AsciiFoldingFilter)
        .filter(Canon)
        .build()
}

thread_local! {
    static FOLD: std::cell::RefCell<TextAnalyzer> = std::cell::RefCell::new(analyzer());
}

pub fn fold_into(s: &str, out: &mut String) {
    out.clear();
    FOLD.with(|a| {
        let mut a = a.borrow_mut();
        let mut ts = a.token_stream(s);
        while ts.advance() {
            if !out.is_empty() {
                out.push(' ');
            }
            out.push_str(&ts.token().text);
        }
    });
}

/// A `cnames` or `ccity` term: a folded word of a document of `country` (its `country` term).
pub fn country_word(country: &str, word: &str) -> String {
    format!("{country}|{word}")
}

/// The `cnames` (or `ccity`) terms of a document of `country` whose `names` (or `city`) field
/// reads `names`: the words the `fold` analyzer indexes for it, each behind the country.
pub fn country_words(country: &str, names: &str) -> Vec<String> {
    if country.is_empty() {
        return vec![];
    }
    let mut words = vec![];
    FOLD.with(|a| {
        let mut a = a.borrow_mut();
        let mut ts = a.token_stream(names);
        while ts.advance() {
            words.push(country_word(country, &ts.token().text));
        }
    });
    words.sort_unstable();
    words.dedup();
    words
}

pub fn fold(s: &str) -> String {
    let mut out = String::new();
    fold_into(s, &mut out);
    out
}

/// A house number as stored and looked up: lowercase, full-width forms as ASCII ("１２６號" is
/// "126"), without the 号/號 that only says "number".
pub fn number_key(n: &str) -> String {
    n.trim()
        .chars()
        .map(|c| match c {
            '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .trim_end_matches(['号', '號'])
        .to_string()
}

/// Territories Nominatim files under their sovereign's country in an address (Puerto Rico under
/// the United States, Réunion under France), as the services this one replaces answered. The
/// Danish, British and New Zealand territories keep their own.
pub fn sovereign(territory: &str) -> Option<&'static str> {
    const SOVEREIGNS: &[(&str, &str)] = &[
        ("pr", "us"),
        ("gu", "us"),
        ("vi", "us"),
        ("as", "us"),
        ("mp", "us"),
        ("re", "fr"),
        ("gp", "fr"),
        ("mq", "fr"),
        ("gf", "fr"),
        ("yt", "fr"),
        ("nc", "fr"),
        ("pf", "fr"),
        ("pm", "fr"),
        ("wf", "fr"),
        ("bl", "fr"),
        ("mf", "fr"),
        ("sj", "no"),
        ("aw", "nl"),
        ("cw", "nl"),
        ("sx", "nl"),
        ("bq", "nl"),
        ("hk", "cn"),
        ("mo", "cn"),
    ];
    SOVEREIGNS
        .iter()
        .find(|(t, _)| *t == territory)
        .map(|(_, s)| *s)
}

pub fn pc_norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect()
}

pub struct Fields {
    pub kind: Field,
    pub names: Field,
    pub city: Field,
    pub country: Field,
    pub ids: Field,
    pub pcode: Field,
    /// Each word of `names` behind its document's country (`country_words`): a lookup in one
    /// country reads that country's postings of a word, not those of every country, whose
    /// skip lists tantivy walks one block at a time.
    pub cnames: Field,
    /// The same for `city`.
    pub ccity: Field,
    pub name: Field,
    pub id: Field,
    pub importance: Field,
    /// Street and postcode docs: their place id, or u32::MAX.
    pub place: Field,
    pub x: Field,
    pub y: Field,
    pub postal_town: Field,
    pub small_names: Field,
    pub small_cnames: Field,
}

/// The source of a `postal_town` entry naming the town around the house's postal locality (New
/// Zealand's town_city), not the locality itself.
pub const TOWN_CITY: &str = "town_city";

/// The class of a place the builder added from GeoNames below `cities500`'s bar (more than 500
/// people, a capital or a division's seat). Suggestions find it; a search only when the query is
/// its name and country and nothing else answers it; reverse lookups never name it.
pub const SMALL_GEONAMES: &str = "geonames_small";

/// GeoNames' other names and corroborated type-qualified primary names, read only at the last
/// step of a "<town>, <country>" search. Keys are country code + folded name; corroboration
/// keys put a NUL between them so they never become alternate lookup names.
pub const ALTERNATES: &str = "alternates.fst";

pub fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let folded = || {
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("fold")
                .set_index_option(IndexRecordOption::Basic),
        )
    };
    let f = Fields {
        kind: b.add_text_field("kind", STRING),
        names: b.add_text_field("names", folded()),
        city: b.add_text_field("city", folded()),
        country: b.add_text_field("country", STRING),
        ids: b.add_text_field("ids", STRING),
        pcode: b.add_text_field("pcode", STRING | STORED),
        cnames: b.add_text_field("cnames", STRING),
        ccity: b.add_text_field("ccity", STRING),
        name: b.add_text_field("name", STORED),
        id: b.add_u64_field("id", FAST),
        importance: b.add_u64_field("importance", FAST),
        place: b.add_u64_field("place", FAST),
        x: b.add_f64_field("x", FAST),
        y: b.add_f64_field("y", FAST),
        postal_town: b.add_text_field("postal_town", STORED),
        small_names: b.add_text_field("small_names", folded()),
        small_cnames: b.add_text_field("small_cnames", STRING),
    };
    (b.build(), f)
}

pub fn create(
    out: &str,
    threads: usize,
    budget: usize,
) -> (Index, IndexWriter<TantivyDocument>, Fields) {
    let (schema, f) = schema();
    let index = Index::create_in_dir(out, schema).unwrap();
    index.tokenizers().register("fold", analyzer());
    let writer = index.writer_with_num_threads(threads, budget).unwrap();
    // Segments are merged once at the end (build_index's STREET_MERGES), not while indexing.
    writer.set_merge_policy(Box::new(tantivy::merge_policy::NoMergePolicy));
    (index, writer, f)
}

pub const INLINE: usize = 8;

/// One house number. Numbers up to INLINE bytes are stored inline, zero-padded, which sorts the
/// same as the bytes themselves because numbers never contain NUL.
#[derive(Clone, Copy)]
pub struct Num {
    pub sid: u32,
    pub num: [u8; INLINE],
    /// `pack`ed: longitude and postcode slot.
    pub x: u32,
    /// `pack`ed: latitude.
    pub y: u32,
}

/// A house number longer than `INLINE`, or a house whose postcode has no slot on its street
/// (`pc`, an interned id; `NO_PC` otherwise).
pub struct LongNum {
    pub sid: u32,
    pub num: String,
    pub x: u32,
    pub y: u32,
    pub pc: u32,
}

pub const NO_PC: u32 = u32::MAX;

/// Where a house came from, in the two top bits of its packed longitude while the builder holds
/// it (a postcode slot stays under 64): 0 Overture addresses, 1 OpenAddresses, 2 OSM, 3 Overture
/// places and Korea's shop register. When sources give the same house, the better-ranked one is
/// kept.
pub const RANK_SHIFT: u32 = 30;

pub fn with_rank(x: u32, rank: u8) -> u32 {
    x | (rank as u32) << RANK_SHIFT
}

/// Maps a house's street id and postcode, as the builder first numbered them (a slot, or with
/// none an interned id other than `NO_PC`), to the street's final id (which must keep the id's
/// remainder modulo the shard count) and slot.
pub type Resolve<'a> = &'a (dyn Fn(u32, u8, u32) -> (u32, u8) + Sync);

/// A postcode slot while the builder holds it: 6 bits of the packed longitude's top byte, under
/// the source rank.
pub fn held_slot(x: u32) -> u8 {
    ((x >> 24) & 0x3f) as u8
}

fn with_held_slot(x: u32, slot: u8) -> u32 {
    (x & 0xc0ff_ffff) | (slot as u32) << 24
}

/// Copies of a house this close to the first (about 300 m) are one house, averaged.
const NEAR_X: u32 = 186;
const NEAR_Y: u32 = 71_583;

/// A house being written: street, key, packed point, the copies averaged into it, and the first
/// copy from the same source too far away to be the same house.
type Kept = (u32, Vec<u8>, u32, u32, (u64, u64, u64), Option<(u32, u32)>);

/// Per shard, a second house of a street's number from the same source, too far from the first
/// to be one house (two villages of one municipality both have "Calle Mayor 3").
pub fn far_file(s: usize) -> String {
    format!("numbers-far-{s:02}.fst")
}

/// An FST value: the top 24 of the longitude's 28 bits, the house's postcode as a 1-based index
/// into its street's stored `pcode` list (0 = none) in 8 bits, then the latitude's 28 bits and
/// the longitude's last 4. 24 bits alone are 2 m of longitude, which is the gap between two doors
/// and made ReverseGeocode answer a neighbour; 28 are 15 cm, and latitude keeps 7.5 cm.
pub fn pack(lon: f64, lat: f64, postcode: u8) -> (u32, u32) {
    let steps = |v: f64| (v * STEPS).clamp(0.0, STEPS - 1.0) as u32;
    let (lon, lat) = (steps((lon + 180.0) / 360.0), steps((lat + 90.0) / 180.0));
    ((lon >> 4) | (postcode as u32) << 24, lat << 4 | lon & 0xf)
}

const STEPS: f64 = (1u64 << 28) as f64;

/// The longitude and latitude of `pack`ed halves, in steps of 2^-28 of their ranges.
fn steps(x: u32, y: u32) -> (u32, u32) {
    ((x & 0xff_ffff) << 4 | y & 0xf, y >> 4)
}

/// Halves for `steps`, keeping the postcode slot and rank bits of `x`.
fn from_steps(x: u32, (lon, lat): (u32, u32)) -> (u32, u32) {
    ((x & !0xff_ffff) | lon >> 4, lat << 4 | lon & 0xf)
}

pub fn with_postcode(x: u32, postcode: u8) -> u32 {
    (x & 0xff_ffff) | (postcode as u32) << 24
}

pub fn postcode_slot(x: u32) -> u8 {
    (x >> 24) as u8
}

/// (longitude, latitude, postcode slot) of an FST value.
pub fn unpack(v: u64) -> (f64, f64, u8) {
    let x = (v >> 32) as u32;
    let (lon, lat) = steps(x, v as u32);
    (
        lon as f64 / STEPS * 360.0 - 180.0,
        lat as f64 / STEPS * 180.0 - 90.0,
        postcode_slot(x),
    )
}

impl Num {
    pub fn new(sid: u32, num: &str, x: u32, y: u32) -> Result<Num, LongNum> {
        if num.len() > INLINE || num.as_bytes().contains(&0) {
            return Err(LongNum {
                sid,
                num: num.to_string(),
                x,
                y,
                pc: NO_PC,
            });
        }
        let mut b = [0u8; INLINE];
        b[..num.len()].copy_from_slice(num.as_bytes());
        Ok(Num { sid, num: b, x, y })
    }
    fn bytes(&self) -> &[u8] {
        &self.num[..self.num.iter().position(|&c| c == 0).unwrap_or(INLINE)]
    }

    /// This house in the long list, carrying its postcode `pc`.
    pub fn into_long(self, pc: u32) -> LongNum {
        LongNum {
            sid: self.sid,
            num: String::from_utf8(self.bytes().to_vec()).unwrap(),
            x: self.x,
            y: self.y,
            pc,
        }
    }
}

#[derive(Default)]
pub struct Bucket {
    pub short: Vec<Num>,
    pub long: Vec<LongNum>,
}

const RECORD: usize = 4 + INLINE + 4 + 4;

impl Num {
    fn encode(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.sid.to_le_bytes());
        out.extend_from_slice(&self.num);
        out.extend_from_slice(&self.x.to_le_bytes());
        out.extend_from_slice(&self.y.to_le_bytes());
    }
    fn decode(r: &[u8; RECORD]) -> Num {
        let f = |i: usize| r[i..i + 4].try_into().unwrap();
        Num {
            sid: u32::from_le_bytes(f(0)),
            num: r[4..4 + INLINE].try_into().unwrap(),
            x: u32::from_le_bytes(f(4 + INLINE)),
            y: u32::from_le_bytes(f(8 + INLINE)),
        }
    }
}

/// House numbers spilled to one unsorted file per shard while addresses are read, so they're not
/// in memory alongside the polygons and street tables. Shard = sid % shards. Numbers longer than
/// INLINE (rare) stay in memory.
pub struct NumberSpill {
    dir: PathBuf,
    files: Vec<Mutex<BufWriter<fs::File>>>,
    /// Numbers longer than `INLINE`, as sid, x, y, length and bytes: kept in memory they were
    /// most of a gigabyte while every source is read.
    long: Vec<Mutex<BufWriter<fs::File>>>,
}

impl LongNum {
    fn encode(&self, out: &mut Vec<u8>) {
        for v in [self.sid, self.x, self.y, self.pc, self.num.len() as u32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(self.num.as_bytes());
    }

    fn read_all(path: &std::path::Path) -> Vec<LongNum> {
        let bytes = fs::read(path).unwrap();
        let (mut out, mut rest) = (vec![], &bytes[..]);
        let word = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap());
        while !rest.is_empty() {
            let len = word(&rest[16..20]) as usize;
            out.push(LongNum {
                sid: word(&rest[0..4]),
                x: word(&rest[4..8]),
                y: word(&rest[8..12]),
                pc: word(&rest[12..16]),
                num: String::from_utf8(rest[20..20 + len].to_vec()).unwrap(),
            });
            rest = &rest[20 + len..];
        }
        out
    }
}

impl NumberSpill {
    pub fn new(dir: PathBuf, shards: usize) -> NumberSpill {
        fs::create_dir_all(&dir).unwrap();
        let files = (0..shards)
            .map(|s| {
                Mutex::new(BufWriter::with_capacity(
                    1 << 20,
                    fs::File::create(dir.join(format!("{s:02}"))).unwrap(),
                ))
            })
            .collect();
        let long = (0..shards)
            .map(|s| {
                Mutex::new(BufWriter::with_capacity(
                    1 << 16,
                    fs::File::create(dir.join(format!("{s:02}.long"))).unwrap(),
                ))
            })
            .collect();
        NumberSpill { dir, files, long }
    }

    pub fn shards(&self) -> usize {
        self.files.len()
    }

    /// Bytes spilled so far, about the in-memory size of the numbers.
    pub fn bytes(&self) -> u64 {
        self.files
            .iter()
            .chain(&self.long)
            .map(|f| {
                f.lock()
                    .unwrap()
                    .get_ref()
                    .metadata()
                    .map_or(0, |m| m.len())
            })
            .sum()
    }

    /// One task's numbers, already split by shard.
    pub fn add(&self, buckets: Vec<Bucket>) {
        let mut buf = vec![];
        for (s, b) in buckets.into_iter().enumerate() {
            buf.clear();
            b.short.iter().for_each(|n| n.encode(&mut buf));
            self.files[s].lock().unwrap().write_all(&buf).unwrap();
            if !b.long.is_empty() {
                buf.clear();
                b.long.iter().for_each(|n| n.encode(&mut buf));
                self.long[s].lock().unwrap().write_all(&buf).unwrap();
            }
        }
    }

    /// One FST per shard, `parallel` shards at a time to bound memory. Key = sid (u32 BE) ++
    /// number; value = x and y as f32 bits. Of duplicate keys, the one with the smallest
    /// coordinates is kept, with the postcode slot of the first duplicate that has one when it has
    /// none, so the result does not depend on the order the rows arrived in.
    pub fn write_fsts(self, out: &str, parallel: usize, resolve: Resolve) -> u64 {
        let n_shards = self.files.len();
        for f in self.files.into_iter().chain(self.long) {
            f.into_inner().unwrap().flush().unwrap();
        }
        // A worker takes the next shard as soon as it finishes one, so a slow shard never holds
        // back the rest as it did when shards ran in fixed groups.
        let (next, n) = (AtomicUsize::new(0), AtomicU64::new(0));
        std::thread::scope(|scope| {
            for _ in 0..parallel.clamp(1, n_shards) {
                scope.spawn(|| {
                    loop {
                        let s = next.fetch_add(1, Relaxed);
                        if s >= n_shards {
                            break;
                        }
                        let path = self.dir.join(format!("{s:02}.long"));
                        let long = LongNum::read_all(&path);
                        fs::remove_file(&path).unwrap();
                        let done = self::shard(out, s, n_shards, &self.dir, long, resolve);
                        n.fetch_add(done, Relaxed);
                    }
                });
            }
        });
        let n = n.into_inner();
        fs::remove_dir_all(&self.dir).unwrap();
        fs::write(format!("{out}/numbers.shards"), n_shards.to_string()).unwrap();
        n
    }
}

fn shard(
    out: &str,
    s: usize,
    n_shards: usize,
    dir: &std::path::Path,
    mut long: Vec<LongNum>,
    resolve: Resolve,
) -> u64 {
    let path = dir.join(format!("{s:02}"));
    let len = fs::metadata(&path).unwrap().len() as usize;
    let mut short = Vec::with_capacity(len / RECORD);
    let mut r = BufReader::with_capacity(1 << 20, fs::File::open(&path).unwrap());
    let mut rec = [0u8; RECORD];
    for _ in 0..len / RECORD {
        r.read_exact(&mut rec).unwrap();
        let mut n = Num::decode(&rec);
        let (sid, slot) = resolve(n.sid, held_slot(n.x), NO_PC);
        debug_assert_eq!(sid as usize % n_shards, s);
        debug_assert!(slot < 64);
        (n.sid, n.x) = (sid, with_held_slot(n.x, slot));
        short.push(n);
    }
    drop(r);
    fs::remove_file(&path).unwrap();
    for n in &mut long {
        let (sid, slot) = resolve(n.sid, held_slot(n.x), n.pc);
        debug_assert!(slot < 64);
        (n.sid, n.x) = (sid, with_held_slot(n.x, slot));
    }
    // Duplicates sort by source rank, then by everything they hold, so the one kept does not
    // depend on arrival order.
    let order = |x: u32, y: u32| (x >> RANK_SHIFT, x & 0xff_ffff, y, (x >> 24) & 0x3f);
    short.par_sort_unstable_by_key(|p| (p.sid, p.num, order(p.x, p.y)));
    long.sort_unstable_by(|p, q| {
        (p.sid, p.num.as_bytes(), order(p.x, p.y)).cmp(&(q.sid, q.num.as_bytes(), order(q.x, q.y)))
    });
    let (a, b) = (short, long);
    let w = BufWriter::with_capacity(
        1 << 22,
        fs::File::create(format!("{out}/numbers-{s:02}.fst")).unwrap(),
    );
    let mut fst = tantivy_fst::MapBuilder::new(w).unwrap();
    let far_out = BufWriter::new(fs::File::create(format!("{out}/{}", far_file(s))).unwrap());
    let mut far_fst = tantivy_fst::MapBuilder::new(far_out).unwrap();
    let (mut i, mut j, mut n) = (0, 0, 0u64);
    let mut key = Vec::with_capacity(64);
    let (mut posts, mut run, mut run_sid) = (vec![], vec![], 0);
    // The house kept for the current key. A source without postcodes must not shadow another
    // source's copy of the same house, so a later duplicate lends its postcode slot. Copies from
    // the best source within `NEAR` of the first are averaged: a building with units has one
    // point per unit, and the middle of them beats an edge. `(copies, sum x, sum y)` counts them.
    let mut pending: Option<Kept> = None;
    let mut emit = |(sid, key, mut x, mut y, (copies, sx, sy), far): Kept| {
        if copies > 1 {
            (x, y) = from_steps(x, ((sx / copies) as u32, (sy / copies) as u32));
        }
        let x = x & ((1 << RANK_SHIFT) - 1);
        fst.insert(&key, ((x as u64) << 32) | y as u64).unwrap();
        if let Some((fx, fy)) = far {
            let fx = fx & ((1 << RANK_SHIFT) - 1);
            far_fst
                .insert(&key, ((fx as u64) << 32) | fy as u64)
                .unwrap();
        }
        n += 1;
        if sid != run_sid {
            super::reverse::street_postings(&mut run, run_sid, &mut posts);
            run_sid = sid;
        }
        run.push(super::reverse::key(x, y));
    };
    while i < a.len() || j < b.len() {
        // A house goes to the long list when its postcode had no slot, which depends on arrival
        // order, so the two lists merge on all they hold.
        let take_a = j == b.len()
            || (i < a.len()
                && (a[i].sid, a[i].bytes(), order(a[i].x, a[i].y))
                    <= (b[j].sid, b[j].num.as_bytes(), order(b[j].x, b[j].y)));
        let (sid, num, x, y) = if take_a {
            i += 1;
            (a[i - 1].sid, a[i - 1].bytes(), a[i - 1].x, a[i - 1].y)
        } else {
            j += 1;
            (
                b[j - 1].sid,
                b[j - 1].num.as_bytes(),
                b[j - 1].x,
                b[j - 1].y,
            )
        };
        key.clear();
        key.extend_from_slice(&sid.to_be_bytes());
        key.extend_from_slice(num);
        match &mut pending {
            Some((_, kept, px, py, (n, sx, sy), far)) if *kept == key => {
                let slot = |x: u32| (x >> 24) & 0x3f;
                let same_source = x >> RANK_SHIFT == *px >> RANK_SHIFT;
                let close = |(px, py): (u32, u32)| {
                    (x & 0xff_ffff).abs_diff(px & 0xff_ffff) <= NEAR_X && y.abs_diff(py) <= NEAR_Y
                };
                let near = same_source && close((*px, *py));
                if same_source && !near {
                    match far {
                        Some(f) if slot(f.0) == 0 && close(*f) => f.0 |= slot(x) << 24,
                        Some(_) => {}
                        None => *far = Some((x, y)),
                    }
                } else if slot(*px) == 0 {
                    *px |= slot(x) << 24;
                }
                if near {
                    let (lon, lat) = steps(x, y);
                    (*n, *sx, *sy) = (*n + 1, *sx + lon as u64, *sy + lat as u64);
                }
            }
            _ => {
                let first = steps(x, y);
                let first = (first.0 as u64, first.1 as u64);
                if let Some(done) =
                    pending.replace((sid, key.clone(), x, y, (1, first.0, first.1), None))
                {
                    emit(done);
                }
            }
        }
    }
    if let Some(done) = pending.take() {
        emit(done);
    }
    fst.finish().unwrap();
    far_fst.finish().unwrap();
    super::reverse::street_postings(&mut run, run_sid, &mut posts);
    super::reverse::write_spill(
        &std::path::Path::new(out).join(super::reverse::SPILL),
        s,
        posts,
    );
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_canonicalizes_street_abbreviations() {
        for (a, b) in [
            ("S VERMONT AVE", "South Vermont Avenue"),
            ("Av. Paulista", "Avenida Paulista"),
            ("R. Dr. João Pessoa", "Rua Doutor João Pessoa"),
            ("Hauptstr. 5", "Hauptstraße 5"),
            ("Berliner Str.", "Berliner Straße"),
            ("Kerkstraat", "Kerkstr"),
            ("Cl 10 # 5-20", "Calle 10 # 5-20"),
            ("C/ Mayor 3", "Calle Mayor 3"),
            ("Cra 7", "Carrera 7"),
            ("Atatürk Cad.", "Atatürk Caddesi"),
            ("ул. Ленина", "улица Ленина"),
            ("St Mary's Rd", "Saint Mary's Road"),
            ("Drottninggatan 5", "Drottningg. 5"),
            ("Storgata 12", "Storgt. 12"),
            ("Karl Johans gate", "Karl Johans gt"),
            ("Kungsvägen", "Kungsv."),
        ] {
            assert_eq!(fold(a), fold(b), "{a} / {b}");
        }
        assert_eq!(fold("350 Fifth Avenue"), fold("350 5th Ave"));
        assert_eq!(fold("5 AVENUE"), fold("5th Avenue"));
        assert_eq!(fold("21st St"), "21 st");
        assert_eq!(fold("İstanbul"), "istanbul");
        for (a, b) in [
            ("أسيوط", "اسيوط"),
            ("الإسماعيلية", "الاسماعيليه"),
            ("مدينة", "مدينه"),
            ("بنى سويف", "بني سويف"),
            ("آر8", "ار8"),
            ("ٱلقاهرة", "القاهره"),
            ("مُحَمَّد", "محمد"),
            ("شـارع", "شارع"),
        ] {
            assert_eq!(fold(a), b, "{a}");
        }
        assert_eq!(fold("東京都"), "東京 京都");
        assert_eq!(fold("東京"), "東京");
        assert_eq!(fold("서울특별시"), "서울 울특 특별 별시");
        assert_eq!(fold("Via Roma"), "via roma");
        assert_eq!(fold("Strasse"), "str");
        assert_eq!(number_key("１２６號"), "126");
        assert_eq!(number_key("12B"), "12b");
        assert_eq!(fold("濱海路三段"), fold("濱海路3段"));
        assert_eq!(fold("芝公園四丁目"), fold("芝公園4丁目"));
        assert_eq!(fold("浅草５丁目"), fold("浅草5丁目"));
        assert_ne!(fold("浅草２丁目"), fold("浅草5丁目"));
        assert_eq!(fold("二十一丁目"), fold("21丁目"));
    }

    /// street id, number, longitude, postcode slot, source rank
    type Row<'a> = (u32, &'a str, f64, u8, u8);

    #[test]
    fn a_spill_record_stays_20_bytes() {
        // The cluster's scratch volume is sized for 20 bytes per house (500 million for the
        // world); a wider record ran the build out of disk.
        assert_eq!(RECORD, 20);
    }

    /// A street keeps its id and its slots.
    fn same(sid: u32, slot: u8, _: u32) -> (u32, u8) {
        (sid, slot)
    }

    fn write(dir: &std::path::Path, name: &str, tasks: Vec<Vec<Row>>) -> (String, Vec<u8>) {
        let out = dir.join(name);
        fs::create_dir_all(&out).unwrap();
        let out = out.to_str().unwrap().to_string();
        let spill = NumberSpill::new(dir.join(format!("spill-{name}")), 2);
        for task in tasks {
            let mut buckets: Vec<Bucket> = (0..2).map(|_| Bucket::default()).collect();
            for (sid, num, lon, slot, rank) in task {
                let (x, y) = pack(lon, 2.0, slot);
                match Num::new(sid, num, with_rank(x, rank), y) {
                    Ok(n) => buckets[sid as usize % 2].short.push(n),
                    Err(l) => buckets[sid as usize % 2].long.push(l),
                }
            }
            spill.add(buckets);
        }
        spill.write_fsts(&out, 1, &same);
        let bytes = [0, 1]
            .iter()
            .flat_map(|s| fs::read(format!("{out}/numbers-{s:02}.fst")).unwrap())
            .collect();
        (out, bytes)
    }

    fn lookup(out: &str, shard: usize, sid: u32, num: &str) -> Option<(f64, f64, u8)> {
        let map = tantivy_fst::Map::from_bytes(
            fs::read(format!("{out}/numbers-{shard:02}.fst")).unwrap(),
        )
        .unwrap();
        map.get([&sid.to_be_bytes()[..], num.as_bytes()].concat())
            .map(unpack)
    }

    #[test]
    fn numbers_merge_inline_and_long_in_byte_order() {
        let dir = std::env::temp_dir().join(format!("numbers-test-{}", std::process::id()));
        let long = "1-very-long-number";
        let (out, _) = write(
            &dir,
            "a",
            vec![
                vec![
                    (7, "12", 7.0, 3, 0),
                    (7, "1", 7.0, 3, 0),
                    (7, "12", 7.0, 3, 0),
                ],
                vec![
                    (7, "1a", 7.0, 3, 0),
                    (2, "9", 2.0, 3, 0),
                    (7, long, 7.0, 3, 0),
                ],
            ],
        );
        assert!(!dir.join("spill-a").exists());
        assert!(lookup(&out, 0, 2, "9").is_some());
        for n in ["1", long, "1a", "12"] {
            let (lon, lat, slot) = lookup(&out, 1, 7, n).expect(n);
            assert!(
                (lon - 7.0).abs() < 1e-4 && (lat - 2.0).abs() < 1e-6 && slot == 3,
                "{n}: {lon} {lat} {slot}"
            );
        }
        let (x, y) = pack(-179.99999, 89.99999, 255);
        let (lon, lat, slot) = unpack((x as u64) << 32 | y as u64);
        assert!((lon + 179.99999).abs() < 2e-6 && (lat - 89.99999).abs() < 1e-6 && slot == 255);
        // two doors 1 m apart on one parallel stay 1 m apart
        let at = |lon: f64| {
            let (x, y) = pack(lon, 25.0, 0);
            unpack((x as u64) << 32 | y as u64)
        };
        let (west, east) = (at(121.554471), at(121.554471 + 1.0 / 100_900.0));
        assert!(
            ((east.0 - west.0) * 100_900.0 - 1.0).abs() < 0.2,
            "{west:?} {east:?}"
        );
        assert!((west.1 - 25.0).abs() < 1e-6);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn near_copies_of_a_house_are_averaged() {
        let dir = std::env::temp_dir().join(format!("numbers-mean-test-{}", std::process::id()));
        // Two unit points of one building (about 100 m apart), a copy of the same number a
        // degree away and one beside it with a postcode, and an OpenAddresses copy next to the
        // first.
        let copies = [
            (7, 2.0, 1, 0),
            (7, 2.001, 1, 0),
            (7, 3.0, 0, 0),
            (7, 2.0005, 1, 1),
            (7, 3.0005, 2, 0),
        ];
        let rows = |order: [usize; 5]| -> Vec<Vec<Row>> {
            order
                .iter()
                .map(|&i| {
                    let (sid, lon, slot, rank) = copies[i];
                    vec![(sid, "9", lon, slot, rank)]
                })
                .collect()
        };
        let (out, one) = write(&dir, "a", rows([0, 1, 2, 3, 4]));
        let (_, other) = write(&dir, "b", rows([4, 2, 3, 1, 0]));
        assert_eq!(one, other);
        let (lon, _, _) = lookup(&out, 1, 7, "9").unwrap();
        assert!((lon - 2.0005).abs() < 3e-5, "{lon}");
        // the copy a degree away is another house of that number, kept beside it with the
        // postcode of its neighbour
        let far = tantivy_fst::Map::from_bytes(fs::read(format!("{out}/{}", far_file(1))).unwrap())
            .unwrap();
        let (lon, _, slot) = unpack(far.get([&7u32.to_be_bytes()[..], b"9"].concat()).unwrap());
        assert!((lon - 3.0).abs() < 3e-5, "{lon}");
        assert!(slot == 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn duplicate_houses_do_not_depend_on_arrival_order() {
        let dir = std::env::temp_dir().join(format!("numbers-dup-test-{}", std::process::id()));
        let long = "1-very-long-number";
        // Places rank last even when their point sorts before every other copy.
        let copies = [
            (7, 1.5, 4, 0),
            (7, 1.0, 0, 1),
            (7, 1.2, 2, 1),
            (7, 1.0, 3, 2),
            (7, 0.5, 0, 3),
        ];
        let rows = |order: [usize; 5]| -> Vec<Vec<Row>> {
            order
                .iter()
                .map(|&i| {
                    let (sid, lon, slot, rank) = copies[i];
                    vec![(sid, "418", lon, slot, rank), (sid, long, lon, slot, rank)]
                })
                .collect()
        };
        let (out, one) = write(&dir, "a", rows([0, 1, 2, 3, 4]));
        let (_, other) = write(&dir, "b", rows([4, 3, 2, 0, 1]));
        assert_eq!(one, other);
        for n in ["418", long] {
            // the best-ranked source's copy and its postcode, though it is not the westernmost
            let (lon, _, slot) = lookup(&out, 1, 7, n).unwrap();
            assert!((lon - 1.5).abs() < 1e-4 && slot == 4, "{n}: {lon} {slot}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }
}
