use super::canon::Canon;
use rayon::prelude::*;
use std::{
    fs,
    io::{BufReader, BufWriter, Read, Write},
    path::PathBuf,
    sync::Mutex,
};
use tantivy::{
    Index, IndexWriter, TantivyDocument,
    schema::{
        FAST, Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions,
    },
    tokenizer::{AsciiFoldingFilter, LowerCaser, RemoveLongFilter, SimpleTokenizer, TextAnalyzer},
};

/// Query-time normalization must use the same analyzer.
pub fn analyzer() -> TextAnalyzer {
    TextAnalyzer::builder(SimpleTokenizer::default())
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
    pub ctok: Field,
    pub name: Field,
    pub id: Field,
    pub importance: Field,
    /// Street and postcode docs: their place id, or u32::MAX.
    pub place: Field,
    pub x: Field,
    pub y: Field,
}

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
        ctok: b.add_text_field("ctok", STRING),
        name: b.add_text_field("name", STORED),
        id: b.add_u64_field("id", FAST),
        importance: b.add_u64_field("importance", FAST),
        place: b.add_u64_field("place", FAST),
        x: b.add_f64_field("x", FAST),
        y: b.add_f64_field("y", FAST),
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
    // A single merged segment measured slower to query than the tens the writer produces.
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

pub struct LongNum {
    pub sid: u32,
    pub num: String,
    pub x: u32,
    pub y: u32,
}

/// An FST value: longitude in 24 bits (about 2 m), the house's postcode as a 1-based index into
/// its street's stored `pcode` list (0 = none) in 8 bits, then latitude in 32 bits.
pub fn pack(lon: f64, lat: f64, postcode: u8) -> (u32, u32) {
    let x = ((lon + 180.0) / 360.0 * (1u64 << 24) as f64).clamp(0.0, ((1 << 24) - 1) as f64) as u32;
    let y = ((lat + 90.0) / 180.0 * (1u64 << 32) as f64).clamp(0.0, u32::MAX as f64) as u32;
    (x | (postcode as u32) << 24, y)
}

pub fn with_postcode(x: u32, postcode: u8) -> u32 {
    (x & 0xff_ffff) | (postcode as u32) << 24
}

pub fn postcode_slot(x: u32) -> u8 {
    (x >> 24) as u8
}

/// (longitude, latitude, postcode slot) of an FST value.
pub fn unpack(v: u64) -> (f64, f64, u8) {
    let (x, y) = ((v >> 32) as u32, v as u32);
    let lon = (x & 0xff_ffff) as f64 / (1u64 << 24) as f64 * 360.0 - 180.0;
    let lat = y as f64 / (1u64 << 32) as f64 * 180.0 - 90.0;
    (lon, lat, postcode_slot(x))
}

impl Num {
    pub fn new(sid: u32, num: &str, x: u32, y: u32) -> Result<Num, LongNum> {
        if num.len() > INLINE || num.as_bytes().contains(&0) {
            return Err(LongNum {
                sid,
                num: num.to_string(),
                x,
                y,
            });
        }
        let mut b = [0u8; INLINE];
        b[..num.len()].copy_from_slice(num.as_bytes());
        Ok(Num { sid, num: b, x, y })
    }
    fn bytes(&self) -> &[u8] {
        &self.num[..self.num.iter().position(|&c| c == 0).unwrap_or(INLINE)]
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
    long: Vec<Mutex<Vec<LongNum>>>,
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
        let long = (0..shards).map(|_| Mutex::default()).collect();
        NumberSpill { dir, files, long }
    }

    pub fn shards(&self) -> usize {
        self.files.len()
    }

    /// Bytes spilled so far, i.e. the in-memory size of the inline numbers.
    pub fn bytes(&self) -> u64 {
        self.files
            .iter()
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
            self.long[s].lock().unwrap().extend(b.long);
        }
    }

    /// One FST per shard, `parallel` shards at a time to bound memory. Key = sid (u32 BE) ++
    /// number; value = x and y as f32 bits. Duplicate keys keep the first.
    pub fn write_fsts(self, out: &str, parallel: usize) -> u64 {
        let n_shards = self.files.len();
        for f in self.files {
            f.into_inner().unwrap().flush().unwrap();
        }
        let mut long: Vec<Option<Vec<LongNum>>> = self
            .long
            .into_iter()
            .map(|l| Some(l.into_inner().unwrap()))
            .collect();
        let mut n = 0;
        for group in (0..n_shards).collect::<Vec<_>>().chunks(parallel) {
            let jobs: Vec<(usize, Vec<LongNum>)> = group
                .iter()
                .map(|&s| (s, long[s].take().unwrap()))
                .collect();
            n += jobs
                .into_par_iter()
                .map(|(s, long)| self::shard(out, s, &self.dir, long))
                .sum::<u64>();
        }
        fs::remove_dir_all(&self.dir).unwrap();
        fs::write(format!("{out}/numbers.shards"), n_shards.to_string()).unwrap();
        n
    }
}

fn shard(out: &str, s: usize, dir: &std::path::Path, long: Vec<LongNum>) -> u64 {
    let path = dir.join(format!("{s:02}"));
    let len = fs::metadata(&path).unwrap().len() as usize;
    let mut short = Vec::with_capacity(len / RECORD);
    let mut r = BufReader::with_capacity(1 << 20, fs::File::open(&path).unwrap());
    let mut rec = [0u8; RECORD];
    for _ in 0..len / RECORD {
        r.read_exact(&mut rec).unwrap();
        short.push(Num::decode(&rec));
    }
    drop(r);
    fs::remove_file(&path).unwrap();
    short.par_sort_unstable_by_key(|p| (p.sid, p.num));
    let mut long = long;
    long.sort_unstable_by(|p, q| (p.sid, p.num.as_bytes()).cmp(&(q.sid, q.num.as_bytes())));
    let (a, b) = (short, long);
    let w = BufWriter::with_capacity(
        1 << 22,
        fs::File::create(format!("{out}/numbers-{s:02}.fst")).unwrap(),
    );
    let mut fst = tantivy_fst::MapBuilder::new(w).unwrap();
    let (mut i, mut j, mut n) = (0, 0, 0u64);
    let mut last: Vec<u8> = vec![];
    let mut key = Vec::with_capacity(64);
    while i < a.len() || j < b.len() {
        let take_a = j == b.len()
            || (i < a.len() && (a[i].sid, a[i].bytes()) <= (b[j].sid, b[j].num.as_bytes()));
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
        if key <= last {
            continue;
        }
        fst.insert(&key, ((x as u64) << 32) | y as u64).unwrap();
        n += 1;
        std::mem::swap(&mut key, &mut last);
    }
    fst.finish().unwrap();
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

    #[test]
    fn numbers_merge_inline_and_long_in_byte_order() {
        let dir = std::env::temp_dir().join(format!("numbers-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let out = dir.to_str().unwrap();
        let spill = NumberSpill::new(dir.join("spill"), 2);
        let mut nums = [
            (7, "12"),
            (7, "1"),
            (7, "12"),
            (7, "1a"),
            (2, "9"),
            (7, "1-very-long-number"),
        ]
        .into_iter();
        // two tasks
        for task in [nums.by_ref().take(3).collect::<Vec<_>>(), nums.collect()] {
            let mut buckets: Vec<Bucket> = (0..2).map(|_| Bucket::default()).collect();
            for (sid, num) in task {
                let b = &mut buckets[sid as usize % 2];
                let (x, y) = pack(sid as f64, num.len() as f64, 3);
                match Num::new(sid, num, x, y) {
                    Ok(n) => b.short.push(n),
                    Err(l) => b.long.push(l),
                }
            }
            spill.add(buckets);
        }
        assert_eq!(spill.write_fsts(out, 1), 5, "duplicate (7, 12) dropped");
        assert!(!dir.join("spill").exists());
        let shard = |s: usize| {
            tantivy_fst::Map::from_bytes(fs::read(format!("{out}/numbers-{s:02}.fst")).unwrap())
                .unwrap()
        };
        let key = |sid: u32, n: &str| [&sid.to_be_bytes()[..], n.as_bytes()].concat();
        assert!(shard(0).get(key(2, "9")).is_some());
        let s1 = shard(1);
        for n in ["1", "1-very-long-number", "1a", "12"] {
            assert!(s1.get(key(7, n)).is_some(), "{n}");
        }
        let v = s1.get(key(7, "1a")).unwrap();
        let (lon, lat, pc) = unpack(v);
        assert!(
            (lon - 7.0).abs() < 1e-4 && (lat - 2.0).abs() < 1e-6 && pc == 3,
            "{lon} {lat} {pc}"
        );
        let (lon, lat, pc) = unpack({
            let (x, y) = pack(-179.99999, 89.99999, 255);
            (x as u64) << 32 | y as u64
        });
        assert!((lon + 179.99999).abs() < 3e-5 && (lat - 89.99999).abs() < 1e-6 && pc == 255);
        fs::remove_dir_all(&dir).unwrap();
    }
}
