use rayon::prelude::*;
use std::{fs, io::BufWriter};
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
        pcode: b.add_text_field("pcode", STRING),
        ctok: b.add_text_field("ctok", STRING),
        name: b.add_text_field("name", STORED),
        id: b.add_u64_field("id", FAST),
        importance: b.add_u64_field("importance", FAST),
        x: b.add_f64_field("x", FAST),
        y: b.add_f64_field("y", FAST),
    };
    (b.build(), f)
}

pub fn create(out: &str) -> (Index, IndexWriter<TantivyDocument>, Fields) {
    let (schema, f) = schema();
    let index = Index::create_in_dir(out, schema).unwrap();
    index.tokenizers().register("fold", analyzer());
    let writer = index.writer_with_num_threads(16, 4_000_000_000).unwrap();
    // A single merged segment measured slower to query than the 16-32 the writer produces.
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
    pub x: f32,
    pub y: f32,
}

pub struct LongNum {
    pub sid: u32,
    pub num: String,
    pub x: f32,
    pub y: f32,
}

impl Num {
    pub fn new(sid: u32, num: &str, x: f32, y: f32) -> Result<Num, LongNum> {
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

/// One FST per bucket, holding the sids with sid % buckets.len() == bucket index, sorted in place
/// and built in parallel. Key = sid (u32 BE) ++ number; value = x and y as f32 bits. Duplicate
/// keys keep the first.
pub fn numbers(out: &str, buckets: Vec<Bucket>) -> u64 {
    let shards = buckets.len();
    let n = buckets
        .into_par_iter()
        .enumerate()
        .map(|(s, mut b)| {
            b.short.sort_unstable_by_key(|p| (p.sid, p.num));
            b.long
                .sort_unstable_by(|p, q| (p.sid, p.num.as_bytes()).cmp(&(q.sid, q.num.as_bytes())));
            let (a, b) = (b.short, b.long);
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
                fst.insert(&key, ((x.to_bits() as u64) << 32) | y.to_bits() as u64)
                    .unwrap();
                n += 1;
                std::mem::swap(&mut key, &mut last);
            }
            fst.finish().unwrap();
            n
        })
        .sum();
    fs::write(format!("{out}/numbers.shards"), shards.to_string()).unwrap();
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_merge_inline_and_long_in_byte_order() {
        let dir = std::env::temp_dir().join(format!("numbers-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut buckets: Vec<Bucket> = (0..2).map(|_| Bucket::default()).collect();
        for (sid, num) in [
            (7, "12"),
            (7, "1"),
            (7, "12"),
            (7, "1a"),
            (2, "9"),
            (7, "1-very-long-number"),
        ] {
            let b = &mut buckets[sid as usize % 2];
            match Num::new(sid, num, sid as f32, num.len() as f32) {
                Ok(n) => b.short.push(n),
                Err(l) => b.long.push(l),
            }
        }
        let out = dir.to_str().unwrap();
        assert_eq!(numbers(out, buckets), 5, "duplicate (7, 12) dropped");
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
        assert_eq!(
            (f32::from_bits((v >> 32) as u32), f32::from_bits(v as u32)),
            (7.0, 2.0)
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
