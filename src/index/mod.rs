//! Address lookup over a `build_index` output directory, loaded into the service process.
//! Responses keep the Nominatim-style JSON candidate format the resolver reads.
pub mod area;
pub mod canon;
mod history;
pub mod landmark;
mod locality;
pub mod reverse;
pub mod schema;

use anyhow::{Context, Result};
use schema::{Fields, fold, pc_norm};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, LazyLock, Mutex},
};
use tantivy::{
    DocAddress, Searcher, Term,
    query::{BooleanQuery, Occur, Query as TQuery, TermQuery},
    schema::{Field, IndexRecordOption},
};

mod korea;
mod suggest;
pub use suggest::{Focus, Suggestion};

const SEP: char = '\u{1f}';
const NONE: u32 = u32::MAX;
const STREET: u64 = 1 << 40;
const HOUSE: u64 = 2 << 40;
const POSTCODE: u64 = 3 << 40;
const STREET_CANDIDATES: usize = 20;

thread_local! {
    /// `Index::matching` results by query, while `Index::memoized` runs on this thread.
    static MEMO: std::cell::RefCell<Option<HashMap<String, Vec<DocAddress>>>> =
        const { std::cell::RefCell::new(None) };
}

struct Place {
    subtype: Box<str>,
    class: Box<str>,
    cc: Box<str>,
    region: Box<str>,
    population: u32,
    x: f32,
    y: f32,
    bbox: [f32; 4],
    parent: u32,
    primary: Box<str>,
    /// `language=name` pairs joined by SEP.
    names: Box<str>,
    /// Where the English name starts in `names` (NONE: none): a country's names run to
    /// thousands of bytes, and every rendered address names its country.
    en: u32,
    /// Sorted `name_key` hashes of all names, so exact-name checks don't re-fold them.
    keys: Box<[u64]>,
    /// A county that is a city in all but name: Overture has Birmingham only as a county with
    /// a same-named neighbourhood inside, and no locality.
    city_like: bool,
    settlement_twin: bool,
}

impl Place {
    fn named(&self, key: u64) -> bool {
        self.keys.binary_search(&key).is_ok()
    }

    fn name(&self, en: bool) -> &str {
        match self.en {
            at if en && at != NONE => self.names[at as usize..].split(SEP).next().unwrap_or(""),
            _ => &self.primary,
        }
    }

    fn english(names: &str) -> u32 {
        let mut at = 0;
        for pair in names.split(SEP) {
            if pair.split_once('=').is_some_and(|(l, _)| l == "en") {
                return (at + "en=".len()) as u32;
            }
            at += pair.len() + SEP.len_utf8();
        }
        NONE
    }

    fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.names.split(SEP).filter_map(|p| p.split_once('='))
    }

    fn all_names(&self) -> impl Iterator<Item = &str> {
        std::iter::once(&*self.primary).chain(self.pairs().map(|(_, n)| n))
    }

    /// Nominatim's address key for this place.
    fn key(&self) -> &'static str {
        match &*self.subtype {
            "country" | "dependency" => "country",
            "region" | "macroregion" => "state",
            "county" if self.city_like => "city",
            "county" => "county",
            "localadmin" => "municipality",
            "locality" => match &*self.class {
                "megacity" | "city" => "city",
                "town" => "town",
                "village" | "hamlet" => "village",
                _ if self.population >= 100_000 => "city",
                _ if self.population >= 10_000 => "town",
                _ => "village",
            },
            "macrohood" | "neighborhood" => "suburb",
            _ => "neighbourhood",
        }
    }

    fn admin_level(&self) -> u32 {
        match &*self.subtype {
            "country" | "dependency" => 2,
            "region" => 4,
            "macroregion" => 5,
            "county" => 6,
            "localadmin" => 8,
            "locality" => 8,
            _ => 10,
        }
    }
}

pub use schema::sovereign;

/// Place number and ISO 3166-2 code of the places that stand for a subdivision.
type OwnCodes = Vec<(u32, String)>;

/// ISO 3166-2 code by Wikidata item, from one list of `data/reference.json`: `iso3166_2`, the
/// subdivisions that Overture's `region` column lacks (Ugandan districts, overseas territories)
/// and that consumers expect, or `iso3166_2_lvl6`, those below a region (French départements).
fn iso_by_item<'a>(reference: &'a Value, list: &str) -> HashMap<&'a str, &'a str> {
    reference[list]
        .as_object()
        .into_iter()
        .flatten()
        .flat_map(|(code, items)| {
            let items = items.as_array().into_iter().flatten();
            items
                .filter_map(Value::as_str)
                .map(move |item| (item, code.as_str()))
        })
        .collect()
}

/// Reads `places.tsv` on all cores (about 5 million lines for the world), hashing each place's
/// names as it goes, and the places whose Wikidata item (the last column) has an ISO 3166-2
/// code in `iso`, by place number.
fn load_places(
    path: &Path,
    iso: &HashMap<&str, &str>,
) -> Result<(Vec<Place>, OwnCodes, history::History)> {
    use rayon::prelude::*;
    let text = fs::read_to_string(path).with_context(|| path.display().to_string())?;
    let lines: Vec<&str> = text.lines().collect();
    let settlements: std::collections::HashSet<(&str, &str, &str)> = lines
        .par_iter()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            if !matches!(fields.next()?, "locality" | "localadmin") {
                return None;
            }
            let country = fields.nth(1)?;
            let region = fields.next()?;
            let item = fields.nth(11)?;
            (!region.is_empty() && !item.is_empty()).then_some((country, region, item))
        })
        .collect();
    let own = lines
        .par_iter()
        .enumerate()
        .filter_map(|(i, l)| {
            let code = iso.get(l.split('\t').nth(15)?)?;
            Some((i as u32, code.to_string()))
        })
        .collect();
    let places: Vec<Place> = lines
        .par_iter()
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            let f = |i: usize| c.get(i).copied().unwrap_or("");
            let num = |i: usize| f(i).parse::<f32>().unwrap_or(f32::NAN);
            let mut place = Place {
                subtype: f(0).into(),
                class: f(1).into(),
                cc: f(2).to_lowercase().into(),
                region: f(3).into(),
                population: f(5).parse().unwrap_or(0),
                x: num(6),
                y: num(7),
                bbox: [num(8), num(9), num(10), num(11)],
                parent: f(12).parse().unwrap_or(NONE),
                primary: f(13).into(),
                names: f(14).into(),
                en: Place::english(f(14)),
                keys: Box::default(),
                city_like: false,
                settlement_twin: matches!(f(0), "county" | "region" | "macroregion")
                    && settlements.contains(&(f(2), f(3), f(15))),
            };
            let mut keys: Vec<u64> = place.all_names().map(name_key).collect();
            keys.sort_unstable();
            keys.dedup();
            place.keys = keys.into();
            place
        })
        .collect();
    let history = history::History::load(&places, &lines)?;
    Ok((places, own, history))
}

struct Columns {
    id: tantivy::columnar::Column<u64>,
    place: tantivy::columnar::Column<u64>,
    x: tantivy::columnar::Column<f64>,
    y: tantivy::columnar::Column<f64>,
    importance: tantivy::columnar::Column<u64>,
    /// The doc ranges of each kind, by `Kind as usize`. The builder writes each kind in long runs
    /// (10 to 20 per segment of a world index), so a lookup seeks its kind's runs and skips the
    /// postings of the others: a place's name words also match millions of streets.
    runs: [Vec<std::ops::Range<u32>>; 3],
    /// Each document's `country` term, numbered in `Index::country_codes` (0: none). A country
    /// term has millions of postings that tantivy's skip list walks one block at a time in every
    /// lookup it intersects; this costs 2 bytes per document instead.
    countries: Box<[u16]>,
}

impl Columns {
    fn open(
        reader: &tantivy::SegmentReader,
        country: Field,
        codes: &mut HashMap<String, u16>,
    ) -> Result<Columns> {
        use tantivy::DocSet;
        let mut countries = vec![0u16; reader.max_doc() as usize];
        let inverted = reader.inverted_index(country)?;
        let mut terms = inverted.terms().stream()?;
        while terms.advance() {
            let next = u16::try_from(codes.len() + 1).context("too many country terms")?;
            let code = *codes
                .entry(String::from_utf8_lossy(terms.key()).into_owned())
                .or_insert(next);
            let mut postings =
                inverted.read_postings_from_terminfo(terms.value(), IndexRecordOption::Basic)?;
            let mut doc = postings.doc();
            while doc != tantivy::TERMINATED {
                countries[doc as usize] = code;
                doc = postings.advance();
            }
        }
        let f = reader.fast_fields();
        let (id, place) = (f.u64("id")?, f.u64("place")?);
        let mut runs: [Vec<std::ops::Range<u32>>; 3] = Default::default();
        for doc in 0..reader.max_doc() {
            let kind = match (place.index.has_value(doc), id.index.has_value(doc)) {
                (false, _) => Kind::Place,
                (true, true) => Kind::Street,
                (true, false) => Kind::Postcode,
            } as usize;
            match runs[kind].last_mut() {
                Some(run) if run.end == doc => run.end += 1,
                _ => runs[kind].push(doc..doc + 1),
            }
        }
        Ok(Columns {
            id,
            place,
            x: f.f64("x")?,
            y: f.f64("y")?,
            importance: f.u64("importance")?,
            runs,
            countries: countries.into(),
        })
    }
}

/// The kind of a document: `kind` is a string field whose terms (about 40 million street
/// documents in a world index) are far longer than any name's, and intersecting with them walks
/// postings the query needs none of. The fast columns tell the kinds apart as well: places have
/// no `place`, streets have both `place` and `id`, postcodes have only `place`.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Place,
    Street,
    Postcode,
}

struct ByImportance<'a> {
    n: usize,
    kind: Kind,
    country: Option<u16>,
    /// Stop at the first match: whether there is one, not which is most important.
    any: bool,
    columns: &'a [Columns],
}

/// What separates documents of equal importance: their id, then their coordinates. Document
/// order in an index depends on how many threads wrote it, so it must not decide which of two
/// equally important places a lookup finds first.
type Tie = (u64, u64, u64);

type Ranked = (u64, std::cmp::Reverse<Tie>, u32);

struct SegmentTop {
    segment: u32,
    n: usize,
    importance: tantivy::columnar::Column<u64>,
    id: tantivy::columnar::Column<u64>,
    x: tantivy::columnar::Column<f64>,
    y: tantivy::columnar::Column<f64>,
    heap: std::collections::BinaryHeap<std::cmp::Reverse<Ranked>>,
}

impl SegmentTop {
    /// Places and streets have an id of their own, so one column read decides; postcode documents
    /// have none and fall back to their coordinates.
    fn tie(&self, doc: u32) -> Tie {
        match self.id.first(doc) {
            Some(id) => (id, 0, 0),
            None => (
                0,
                self.x.first(doc).map_or(0, f64::to_bits),
                self.y.first(doc).map_or(0, f64::to_bits),
            ),
        }
    }
}

impl tantivy::collector::Collector for ByImportance<'_> {
    type Fruit = Vec<DocAddress>;
    type Child = SegmentTop;

    fn for_segment(&self, segment: u32, _: &tantivy::SegmentReader) -> tantivy::Result<SegmentTop> {
        let c = &self.columns[segment as usize];
        Ok(SegmentTop {
            segment,
            n: self.n,
            importance: c.importance.clone(),
            id: c.id.clone(),
            x: c.x.clone(),
            y: c.y.clone(),
            heap: Default::default(),
        })
    }

    fn requires_scoring(&self) -> bool {
        false
    }

    fn collect_segment(
        &self,
        weight: &dyn tantivy::query::Weight,
        segment: u32,
        reader: &tantivy::SegmentReader,
    ) -> tantivy::Result<Vec<(u64, Tie, DocAddress)>> {
        use tantivy::{DocSet, collector::SegmentCollector};
        let mut top = self.for_segment(segment, reader)?;
        let alive = reader.alive_bitset();
        let mut scorer = weight.scorer(reader, 1.0)?;
        let columns = &self.columns[segment as usize];
        for run in &columns.runs[self.kind as usize] {
            let mut doc = scorer.doc();
            if doc < run.start {
                doc = scorer.seek(run.start);
            }
            while doc < run.end {
                if alive.is_none_or(|a| a.is_alive(doc))
                    && self
                        .country
                        .is_none_or(|c| columns.countries[doc as usize] == c)
                {
                    top.collect(doc, 0.0);
                    if self.any {
                        return Ok(top.harvest());
                    }
                }
                doc = scorer.advance();
            }
            if doc == tantivy::TERMINATED {
                break;
            }
        }
        Ok(top.harvest())
    }

    fn merge_fruits(
        &self,
        fruits: Vec<Vec<(u64, Tie, DocAddress)>>,
    ) -> tantivy::Result<Vec<DocAddress>> {
        let mut all: Vec<(u64, Tie, DocAddress)> = fruits.into_iter().flatten().collect();
        all.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        Ok(all.into_iter().take(self.n).map(|(_, _, a)| a).collect())
    }
}

impl tantivy::collector::SegmentCollector for SegmentTop {
    type Fruit = Vec<(u64, Tie, DocAddress)>;

    fn collect(&mut self, doc: u32, _: tantivy::Score) {
        let v = self.importance.first(doc).unwrap_or(0);
        let full = self.heap.len() >= self.n;
        // a less important document needs no tie-break
        if full && self.heap.peek().is_some_and(|m| m.0.0 > v) {
            return;
        }
        let ranked = std::cmp::Reverse((v, std::cmp::Reverse(self.tie(doc)), doc));
        if full {
            if self.heap.peek().is_some_and(|m| *m <= ranked) {
                return;
            }
            self.heap.pop();
        }
        self.heap.push(ranked);
    }

    fn harvest(self) -> Self::Fruit {
        let segment = self.segment;
        self.heap
            .into_iter()
            .map(|std::cmp::Reverse((v, std::cmp::Reverse(tie), doc))| {
                (v, tie, DocAddress::new(segment, doc))
            })
            .collect()
    }
}

pub struct Index {
    searcher: Searcher,
    suggestion_scope: String,
    /// Fast-field readers per segment, opened once: opening one parses its block index.
    columns: Vec<Columns>,
    /// Uppercase `country` terms -> their number in `Columns::countries`.
    country_codes: HashMap<String, u16>,
    f: Fields,
    /// Memory-mapped, so cold parts stay in the page cache instead of private memory.
    numbers: Vec<tantivy_fst::Map<tantivy::directory::OwnedBytes>>,
    /// Absent from hand-built indexes, which then answer reverse lookups with a locality only.
    reverse: Option<reverse::Reverse>,
    /// Administrative polygons for `reverse_region`; absent from hand-built indexes.
    areas: Option<area::Areas>,
    /// Streets' lines, for a point with no house in range; absent like `reverse`.
    streets: Option<reverse::Streets>,
    places: Vec<Place>,
    history: history::History,
    /// Locality points by position, for reverse lookups; built at startup from `places`.
    localities: locality::Localities,
    /// Folded country names and ISO codes -> lowercase country code.
    countries: HashMap<String, String>,
    /// Lowercase country code -> its country place.
    country_places: HashMap<String, u32>,
    /// ISO 3166-2 code -> its region place.
    regions: HashMap<String, u32>,
    /// Places that stand for an ISO 3166-2 subdivision Overture has no region for -> its code.
    own_codes: HashMap<u32, Box<str>>,
    /// ISO 3166-2 code -> the county that stands for a subdivision below a region.
    subdivisions: HashMap<String, u32>,
    /// The same, by place.
    subdivision_codes: HashMap<u32, Box<str>>,
    /// ISO 3166-2 code -> the place made for an Irish province; see `irish_provinces`.
    provinces: HashMap<String, u32>,
    parser: Arc<Parser>,
    /// `terms_near` results: retried road and town splits ask for the same words.
    near: Mutex<HashMap<String, std::sync::Arc<[String]>>>,
    /// `prefix_terms` results, by field and prefix.
    prefixes: Mutex<HashMap<(u32, Box<str>), suggest::PrefixTerms>>,
    /// Folded names of the places a completion most likely wants; see `suggest`.
    place_names: tantivy_fst::Map<Vec<u8>>,
    /// `name_candidates` results, by folded text and whether it is a prefix.
    place_prefixes: Mutex<HashMap<suggest::NamesKey, suggest::Candidates>>,
    suggest: suggest::Caches,
    /// Read only by a suggestion that asks for landmarks.
    landmarks: landmark::Landmarks,
}

#[derive(Clone)]
pub enum Hit {
    Place(u32),
    Street {
        doc: DocAddress,
        sid: u32,
        place: u32,
        name: String,
        x: f64,
        y: f64,
        number: Option<String>,
        postcode: Option<String>,
        /// The requested city, when the street matched it only through its postal city.
        city: Option<String>,
        postal_town: Option<String>,
        /// The country the search was scoped to; a street the data files under no place is
        /// otherwise of no country.
        cc: String,
    },
    Postcode {
        display: String,
        cc: String,
        place: u32,
        x: f64,
        y: f64,
    },
}

/// A street word long enough to correct, and a word rather than a number or code ("3526").
fn fuzzable(t: &str) -> bool {
    t.chars().count() >= 4 && t.chars().all(char::is_alphabetic)
}

/// Matches terms within one insertion, deletion, substitution or adjacent transposition of a word
/// (optimal string alignment over bytes, so a non-ASCII letter's substitution counts twice) that
/// start with its first byte: typed addresses rarely get the first letter wrong, and requiring it
/// lets the walk skip all but one branch of the term dictionary.
struct OneEdit<'a>(&'a [u8]);

/// Longest word `OneEdit` takes: the analyzer drops longer tokens.
const MAX_WORD: usize = 40;

/// (previous DP row, DP row, last byte); distances saturate at 2. The previous row of the start
/// state is all 2s, so it never allows a transposition.
type EditState = ([u8; MAX_WORD + 1], [u8; MAX_WORD + 1], u8);

impl tantivy_fst::Automaton for OneEdit<'_> {
    type State = EditState;

    fn start(&self) -> EditState {
        let mut row = [2; MAX_WORD + 1];
        for (j, d) in row.iter_mut().enumerate().take(self.0.len() + 1) {
            *d = j.min(2) as u8;
        }
        ([2; MAX_WORD + 1], row, 0)
    }

    fn is_match(&self, (_, row, _): &EditState) -> bool {
        row[self.0.len()] <= 1
    }

    fn can_match(&self, (_, row, _): &EditState) -> bool {
        row[..=self.0.len()].iter().any(|&d| d <= 1)
    }

    fn accept(&self, (prev, row, last): &EditState, c: u8) -> EditState {
        let w = self.0;
        // terms hold no zero bytes, so `last` is 0 only at the start
        if *last == 0 && w.first() != Some(&c) {
            return ([2; MAX_WORD + 1], [2; MAX_WORD + 1], c);
        }
        let mut next = [2; MAX_WORD + 1];
        next[0] = (row[0] + 1).min(2);
        for j in 1..=w.len() {
            let mut d = (row[j] + 1)
                .min(next[j - 1] + 1)
                .min(row[j - 1] + u8::from(w[j - 1] != c));
            if j > 1 && c == w[j - 2] && *last == w[j - 1] {
                d = d.min(prev[j - 2] + 1);
            }
            next[j] = d.min(2);
        }
        (*row, next, c)
    }
}

/// A house's own postcode against the stated one. A street typed wrongly and a postcode the
/// house contradicts are two doubts at once, so the respelt street answers only without the second.
fn postcode_not_contradicted(own: Option<&str>, stated: Option<&str>) -> bool {
    match (own.map(pc_norm), stated.map(pc_norm)) {
        (Some(o), Some(s)) if !o.is_empty() && !s.is_empty() => o == s,
        _ => true,
    }
}

/// A typed street word that the index would correct to `indexed`: a name word within one edit
/// and with the same first letter ("whte" for "white"), or a misspelt street type or direction
/// ("srteet" for "st").
pub fn misspelt_word(typed: &str, indexed: &str) -> bool {
    (fuzzable(typed) && typed.chars().next() == indexed.chars().next() && one_edit(typed, indexed))
        || misspelt_kind(typed).iter().any(|k| k == indexed)
}

/// The canonical forms of every street type or direction within an edit of `word`:
/// "southewst" is as near "southeast" as "southwest", and only the street tells which.
fn misspelt_kind(word: &str) -> Vec<String> {
    if canon::is_canonical(word) {
        return vec![];
    }
    let mut kinds: Vec<String> = canon::VARIANTS
        .iter()
        .filter(|(full, _)| one_edit(word, full))
        .map(|(_, short)| short.to_string())
        .collect();
    kinds.sort_unstable();
    kinds.dedup();
    kinds
}

/// Within one insertion, deletion, substitution or transposition of adjacent letters.
fn one_edit(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let (a, b) = (&a[head..], &b[head..]);
    match (a.len(), b.len()) {
        (0, 0) => true,
        (m, n) if m == n => {
            a[1..] == b[1..] || (m >= 2 && a[0] == b[1] && a[1] == b[0] && a[2..] == b[2..])
        }
        (m, n) if m == n + 1 => a[1..] == *b,
        (m, n) if m + 1 == n => *a == b[1..],
        _ => false,
    }
}

fn term(f: Field, t: &str) -> Box<dyn TQuery> {
    Box::new(TermQuery::new(
        Term::from_field_text(f, t),
        IndexRecordOption::Basic,
    ))
}

/// A name as compared for an exact match: folded, without a trailing CJK administrative suffix
/// (東京 names 東京都, 上海 names 上海市, 서울 names 서울특별시).
fn exact_key(name: &str) -> String {
    fold(cjk_stem(name).unwrap_or(name.trim()))
}

/// Hash of `exact_key`, for `Place::keys`.
fn name_key(name: &str) -> u64 {
    use std::hash::BuildHasher;
    std::hash::BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default()
        .hash_one(exact_key(name))
}

fn cjk_stem(name: &str) -> Option<&str> {
    const SUFFIXES: [&str; 18] = [
        "特別市",
        "特别市",
        "特别行政区",
        "自治区",
        "특별시",
        "광역시",
        "都",
        "道",
        "府",
        "県",
        "市",
        "区",
        "町",
        "村",
        "省",
        "시",
        "도",
        "군",
    ];
    let name = name.trim();
    // every suffix ends in a Han or Hangul syllable
    if !name
        .chars()
        .next_back()
        .is_some_and(|c| matches!(c, '\u{4e00}'..='\u{9fff}' | '\u{ac00}'..='\u{d7af}'))
    {
        return None;
    }
    SUFFIXES.iter().find_map(|s| {
        name.strip_suffix(s)
            .filter(|stem| !stem.is_empty() && !stem.is_ascii())
    })
}

/// Nominatim-style keys (`name:fr`, `alt_name`, `short_name:ja`); repeated keys join with ';'.
/// A CJK name with an administrative suffix also gives its stem as a short name (東京都 -> 東京).
fn namedetails(pl: &Place, names: &mut Map<String, Value>) {
    use serde_json::map::Entry;
    let mut add = |k: String, n: &str| match names.entry(k) {
        Entry::Occupied(mut e) => {
            if let Value::String(v) = e.get_mut()
                && !v.split(';').any(|x| x == n)
            {
                v.push(';');
                v.push_str(n);
            }
        }
        Entry::Vacant(e) => {
            e.insert(n.into());
        }
    };
    for (l, n) in pl.pairs() {
        add(
            if l.contains("_name") {
                l.to_string()
            } else {
                format!("name:{l}")
            },
            n,
        );
    }
    for n in pl.all_names() {
        if let Some(stem) = cjk_stem(n) {
            add("short_name".into(), stem);
        }
    }
}

/// ISO 3166-2:IE's four provinces, above the counties that are Overture's Irish regions: code,
/// English and Irish name, and counties.
const IRISH_PROVINCES: [(&str, &str, &str, &[&str]); 4] = [
    (
        "IE-C",
        "Connacht",
        "Connachta",
        &["G", "LM", "MO", "RN", "SO"],
    ),
    (
        "IE-L",
        "Leinster",
        "Laighin",
        &[
            "CW", "D", "KE", "KK", "LS", "LD", "LH", "MH", "OY", "WH", "WX", "WW",
        ],
    ),
    (
        "IE-M",
        "Munster",
        "An Mhumhain",
        &["CE", "CO", "KY", "LK", "TA", "WD"],
    ),
    ("IE-U", "Ulster", "Ulaidh", &["CN", "DL", "MN"]),
];

fn irish_province(county: &str) -> Option<&'static str> {
    let county = county.strip_prefix("IE-")?;
    let province = IRISH_PROVINCES.iter().find(|p| p.3.contains(&county))?;
    Some(province.0)
}

/// A place for each Irish province whose counties the index has, by code: Overture has no
/// provinces. It sits at the mean of its counties' points and is in no other place's chain, so
/// it changes no search; it gives the province a region row and answers `ie, <code>`.
fn irish_provinces(places: &[Place], regions: &HashMap<String, u32>) -> Vec<(String, Place)> {
    IRISH_PROVINCES
        .iter()
        .filter_map(|&(code, en, ga, counties)| {
            let members: Vec<&Place> = counties
                .iter()
                .filter_map(|c| regions.get(&format!("IE-{c}")))
                .map(|&p| &places[p as usize])
                .collect();
            let first = members.first()?;
            let n = members.len() as f32;
            let bbox = members
                .iter()
                .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |b, p| {
                    [
                        b[0].min(p.bbox[0]),
                        b[1].min(p.bbox[1]),
                        b[2].max(p.bbox[2]),
                        b[3].max(p.bbox[3]),
                    ]
                });
            let names: Box<str> = format!("en={en}{SEP}ga={ga}").into();
            let mut keys = vec![name_key(en), name_key(ga)];
            keys.sort_unstable();
            Some((
                code.to_string(),
                Place {
                    subtype: "macroregion".into(),
                    class: "".into(),
                    cc: first.cc.clone(),
                    region: "".into(),
                    population: members.iter().map(|p| p.population).sum(),
                    x: members.iter().map(|p| p.x).sum::<f32>() / n,
                    y: members.iter().map(|p| p.y).sum::<f32>() / n,
                    bbox,
                    parent: first.parent,
                    primary: en.into(),
                    en: Place::english(&names),
                    names,
                    keys: keys.into(),
                    city_like: false,
                    settlement_twin: false,
                },
            ))
        })
        .collect()
}

/// (administrative units, road, number) of a Chinese road address, the units most general
/// first: "上海市虹口区中山北一路295号" -> (["上海市", "虹口区"], "中山北一路", "295").
fn chinese(q: &str) -> Option<(Vec<String>, String, String)> {
    static NUMBER: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
                r"([0-9]+(?:之[0-9]+)?)(?:[至-][0-9]+)?[号號](?:(?:地下)?[一二三四五六七八九十百0-9]+[楼樓])?$",
            )
            .unwrap()
    });
    let part = q.split([',', '，']).find(|p| p.contains(['号', '號']))?;
    let t: String = part
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            _ => c,
        })
        .collect();
    let m = NUMBER.captures(&t)?;
    let before = &t[..m.get(0)?.start()];
    let before = before.trim_start_matches(|c: char| c.is_ascii_digit());
    let before = ["中国", "中國", "臺灣", "台灣", "台湾"]
        .iter()
        .fold(before, |b, p| b.strip_prefix(p).unwrap_or(b));
    if before.is_empty()
        || !before
            .chars()
            .all(|c| c as u32 >= 0x3040 || c.is_ascii_digit())
    {
        return None;
    }
    let mut units = vec![];
    let mut start = 0;
    for (i, c) in before.char_indices() {
        if i > start && "省市区區县縣镇鎮乡鄉".contains(c) {
            units.push(before[start..i + c.len_utf8()].to_string());
            start = i + c.len_utf8();
        }
    }
    let road = &before[start..];
    if road.chars().count() < 2 {
        return None;
    }
    Some((units, road.to_string(), m[1].to_string()))
}

/// Reads 一 to 九十九.
fn kanji_number(t: &str) -> Option<u32> {
    let digit = |c: char| {
        "〇一二三四五六七八九"
            .chars()
            .position(|d| d == c)
            .map(|d| d as u32)
    };
    let chars: Vec<char> = t.chars().collect();
    match chars.as_slice() {
        ['十'] => Some(10),
        [c] => digit(*c),
        ['十', u] => Some(10 + digit(*u)?),
        [t, '十'] => Some(digit(*t)? * 10),
        [t, '十', u] => Some(digit(*t)? * 10 + digit(*u)?),
        _ => None,
    }
}

/// Candidate (municipality, street, number) readings of a Japanese block address, most likely
/// first: "東京都港区芝公園4-2-8" is chōme 4 of 芝公園 in 港区, block 2, house 8, stored as street
/// "芝公園四丁目" and number "2-8"; towns without chōme store "block-house" under the town.
fn japanese(q: &str) -> Vec<(String, String, String)> {
    static POSTCODE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"〒?\d{3}-\d{4}|^日本国?|,?Japan$").unwrap());
    static CHOME: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new("([〇一二三四五六七八九十]+)丁目").unwrap());
    let t: String = q
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            '－' | '−' | '‐' | 'ー' | '―' | '‑' => '-',
            _ => c,
        })
        .collect();
    let t = POSTCODE.replace_all(&t, "");
    let t = CHOME.replace_all(&t, |c: &regex::Captures| {
        kanji_number(&c[1]).map_or(c[0].to_string(), |n| format!("{n}丁目"))
    });
    let Some(at) = t.find(|c: char| c.is_ascii_digit()) else {
        return vec![];
    };
    let (prefix, rest) = t.split_at(at);
    if prefix.is_empty() || !prefix.chars().all(|c| c as u32 >= 0x3040) {
        return vec![];
    }
    let rest = rest
        .replace("丁目", "-")
        .replace("番地", "-")
        .replace(['番', 'の'], "-")
        .replace('号', "");
    let numbers: Vec<u32> = rest
        .split(|c: char| !c.is_ascii_digit() && c != '-')
        .next()
        .unwrap_or("")
        .split('-')
        .filter_map(|n| n.parse().ok())
        .collect();
    if numbers.len() < 2 {
        return vec![];
    }
    let joined = |ns: &[u32]| ns.iter().map(u32::to_string).collect::<Vec<_>>().join("-");
    let mut out = vec![];
    for (i, c) in prefix.char_indices().rev() {
        let end = i + c.len_utf8();
        if !"都道府県市区郡町村".contains(c) || end == prefix.len() {
            continue;
        }
        let (city, town) = prefix.split_at(end);
        // the municipality, without its prefecture
        let city = city
            .rfind(['都', '道', '府', '県'])
            .map(|k| &city[k + 3..])
            .filter(|c| !c.is_empty())
            .unwrap_or(city);
        out.push((
            city.to_string(),
            format!("{town}{}丁目", numbers[0]),
            joined(&numbers[1..numbers.len().min(3)]),
        ));
        out.push((city.to_string(), town.to_string(), joined(&numbers[..2])));
    }
    out
}

/// "Vas County" -> "Vas", "Departamento de Santa Cruz" -> "Santa Cruz": words that classify a
/// region rather than name it, which the data's names mostly leave out.
fn without_admin_word(text: &str) -> Option<String> {
    const WORDS: [&str; 33] = [
        "county",
        "department",
        "departement",
        "departamento",
        "province",
        "provincia",
        "provincie",
        "region",
        "región",
        "région",
        "regione",
        "district",
        "parish",
        "governorate",
        "prefecture",
        "oblast",
        "state",
        "estado",
        "municipality",
        "municipio",
        "kreis",
        "landkreis",
        "kreisfreie",
        "stadt",
        "comté",
        "division",
        "rajon",
        "raion",
        "voivodeship",
        "city",
        // US municipalities as registries write them: "Irvington Township", "Lemoyne Borough",
        // "Christiansburg Town", "City and Borough of Juneau"
        "township",
        "borough",
        "town",
    ];
    const LINKS: [&str; 7] = ["of", "de", "del", "di", "du", "do", "and"];
    let words: Vec<&str> = text.split_whitespace().collect();
    let kept: Vec<&str> = words
        .iter()
        .copied()
        .filter(|w| {
            let w = w
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            !WORDS.contains(&w.as_str())
        })
        .collect();
    let kept: Vec<&str> = kept
        .iter()
        .copied()
        .skip_while(|w| LINKS.contains(&w.to_lowercase().as_str()))
        .collect();
    (!kept.is_empty() && kept.len() < words.len()).then(|| kept.join(" "))
}

/// "hoca kasım köprüsü sk. no" (from "Sk. No:5") -> "hoca kasım köprüsü sk.".
fn without_number_mark(road: &str) -> &str {
    let t = road.trim_end();
    for mark in [" no.", " no", " nº", " n°", " nr.", " nr", " №", " #"] {
        if t.len() > mark.len() && t.to_lowercase().ends_with(mark) {
            return t[..t.len() - mark.len()].trim_end();
        }
    }
    t
}

/// "No:5", "nº 1824", "№ 43", "#12" -> the number itself.
fn house_number(n: &str) -> &str {
    let t = n.trim();
    let lower = t.to_lowercase();
    for prefix in ["no.", "no:", "nº", "n°", "n.", "no", "№", "#", "nr.", "nr"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let rest = rest.trim_start_matches([' ', ':', '.']);
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                return &t[t.len() - rest.len()..];
            }
        }
    }
    t
}

fn compact_house_suffix(n: &str) -> Option<String> {
    let parts: Vec<_> = n.split_whitespace().collect();
    let [number, suffix] = parts.as_slice() else {
        return None;
    };
    (!number.is_empty()
        && number.bytes().all(|c| c.is_ascii_digit())
        && suffix.starts_with(|c: char| c.is_ascii_alphabetic())
        && (suffix[1..].bytes().all(|c| c.is_ascii_digit())
            || matches!(
                suffix.to_ascii_lowercase().as_str(),
                "bis" | "ter" | "quater"
            )))
    .then(|| format!("{number}{suffix}"))
}

fn title(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn stored(doc: &tantivy::TantivyDocument, f: Field) -> String {
    use tantivy::schema::Value as _;
    doc.get_first(f)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

fn postal_town(
    doc: &tantivy::TantivyDocument,
    field: Field,
    number: &str,
    postcode: Option<&str>,
) -> Option<String> {
    use tantivy::schema::Value as _;
    let postcode = postcode.map(pc_norm);
    let number = schema::number_key(number);
    let mut town: Option<String> = None;
    for value in doc.get_all(field) {
        let (pc, house, name, _source): (&str, &str, &str, &str) =
            serde_json::from_str(value.as_str()?).ok()?;
        if if pc.is_empty() {
            house != number
        } else {
            postcode.as_deref() != Some(pc)
        } {
            continue;
        }
        if town
            .as_ref()
            .is_some_and(|t| !crate::matching::names_agree(t, name))
        {
            return None;
        }
        town = Some(name.into());
    }
    town
}

/// The stored name sharing most words with the query, so "Sheikh Mohammed bin Rashid
/// Boulevard" shows in English rather than as the street's Arabic primary name.
fn best_name(doc: &tantivy::TantivyDocument, f: Field, query: &str) -> String {
    use tantivy::schema::Value as _;
    let query = fold(query);
    let want: std::collections::HashSet<&str> = query.split(' ').collect();
    let mut best = (0, String::new());
    for name in doc.get_all(f).filter_map(|v| v.as_str()) {
        let folded = fold(name);
        if folded == query {
            return name.to_string();
        }
        let shared = folded.split(' ').filter(|t| want.contains(*t)).count();
        if best.1.is_empty() || shared > best.0 {
            best = (shared, name.to_string());
        }
    }
    best.1
}

/// Words the analyzer drops for their length (40 bytes: any Cyrillic word of 20 letters), so
/// they are in no index term and a query silently loses them.
fn long_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 40)
        .map(str::to_lowercase)
        .collect()
}

/// Whether some name holds every one of `words`.
fn names_hold<'a>(mut names: impl Iterator<Item = &'a str>, words: &[String]) -> bool {
    names.any(|n| {
        let n = n.to_lowercase();
        words.iter().all(|w| n.contains(w.as_str()))
    })
}

fn must(q: Box<dyn TQuery>) -> (Occur, Box<dyn TQuery>) {
    (Occur::Must, q)
}

/// An index directory opened for reading only: Tantivy takes a lock file to open a reader, which
/// a read-only mount refuses. Nothing writes to a served index, so there is nothing to lock out.
#[derive(Clone, Debug)]
struct ReadOnly(tantivy::directory::MmapDirectory);

impl tantivy::Directory for ReadOnly {
    fn get_file_handle(
        &self,
        path: &Path,
    ) -> Result<Arc<dyn tantivy::directory::FileHandle>, tantivy::directory::error::OpenReadError>
    {
        self.0.get_file_handle(path)
    }

    fn delete(&self, path: &Path) -> Result<(), tantivy::directory::error::DeleteError> {
        Err(tantivy::directory::error::DeleteError::IoError {
            io_error: Arc::new(std::io::ErrorKind::ReadOnlyFilesystem.into()),
            filepath: path.to_path_buf(),
        })
    }

    fn exists(&self, path: &Path) -> Result<bool, tantivy::directory::error::OpenReadError> {
        self.0.exists(path)
    }

    fn open_write(
        &self,
        path: &Path,
    ) -> Result<tantivy::directory::WritePtr, tantivy::directory::error::OpenWriteError> {
        Err(tantivy::directory::error::OpenWriteError::IoError {
            io_error: Arc::new(std::io::ErrorKind::ReadOnlyFilesystem.into()),
            filepath: path.to_path_buf(),
        })
    }

    fn atomic_read(
        &self,
        path: &Path,
    ) -> Result<Vec<u8>, tantivy::directory::error::OpenReadError> {
        self.0.atomic_read(path)
    }

    fn atomic_write(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
        Err(std::io::ErrorKind::ReadOnlyFilesystem.into())
    }

    fn sync_directory(&self) -> std::io::Result<()> {
        Ok(())
    }

    fn acquire_lock(
        &self,
        _: &tantivy::directory::Lock,
    ) -> Result<tantivy::directory::DirectoryLock, tantivy::directory::error::LockError> {
        Ok(tantivy::directory::DirectoryLock::from(Box::new(())))
    }

    fn watch(
        &self,
        callback: tantivy::directory::WatchCallback,
    ) -> tantivy::Result<tantivy::directory::WatchHandle> {
        self.0.watch(callback)
    }
}

/// Parsed address components, shared between callers and the cache.
pub type Parsed = Arc<[(String, String)]>;

fn field<'a>(parsed: &'a [(String, String)], key: &str) -> &'a str {
    parsed
        .iter()
        .find(|(k, _)| k == key)
        .map_or("", |(_, v)| v.as_str())
}

/// "street, town, STATE ZIP" read with its commas as the boundaries. libpostal reads across them:
/// "14 Breezewood Common, East Amherst, NY 14051" puts "East" in the road, and "3 Canterbury
/// Street, Egg Harbor Township, NJ 08234" splits the town. Each outer part parses cleanly alone
/// (`head` as only a house number and road, `tail` as only a state and postcode), so when the
/// parse of the `whole` query names a different town, or none because its road swallowed the
/// middle part ("4724 3rd Place Northwest, Washingotn, DC"), the middle part is the town. A whole
/// parse without a town whose road is its own is left alone (the town check it then skips would be
/// a new, stricter one), and so is one that splits the middle into a suburb and a town ("Avondale
/// Goodyear").
pub fn comma_aligned(
    head: &[(String, String)],
    extras: &[&str],
    middle: &str,
    tail: &[(String, String)],
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let only =
        |a: &[(String, String)], keys: &[&str]| a.iter().all(|(k, _)| keys.contains(&k.as_str()));
    if !alignable(middle, extras, whole)
        || extras
            .iter()
            .any(|e| e.trim().is_empty() || e.chars().any(|c| c.is_ascii_digit()))
        || !head_aligns(head)
        || field(tail, "postcode").is_empty()
        || !only(tail, &["state", "postcode"])
    {
        return None;
    }
    let middle = middle.trim().to_lowercase();
    let mut aligned = head.to_vec();
    aligned.extend(
        extras
            .iter()
            .map(|e| ("state_district".to_string(), e.trim().to_lowercase())),
    );
    aligned.push(("city".into(), middle));
    aligned.extend(tail.iter().cloned());
    Some(aligned)
}

/// The checks of [`comma_aligned`] that need no parse of the outer parts, so the two extra
/// libpostal calls are made only for the few queries that can still pass.
fn alignable(middle: &str, extras: &[&str], whole: &[(String, String)]) -> bool {
    let middle = middle.trim().to_lowercase();
    let (city, road, suburb) = (
        field(whole, "city"),
        field(whole, "road"),
        field(whole, "suburb"),
    );
    // a road read as a suburb ("2219 Pacific Coast Highway, Lomita")
    let road_as_suburb = road.is_empty() && !suburb.is_empty();
    // a county part read into the road ("15 Westervelt Place, Essex, Irvington Township")
    let county_in_road = extras.iter().any(|e| {
        let e = e.trim().to_lowercase();
        !e.is_empty() && road.contains(&e)
    });
    !middle.is_empty()
        && !middle.chars().any(|c| c.is_ascii_digit())
        && field(whole, "country").is_empty()
        && field(whole, "state") != middle
        && (!city.is_empty() || road.contains(&middle))
        && (suburb.is_empty() || road_as_suburb)
        && field(whole, "city_district").is_empty()
        && (city != middle || road_as_suburb || county_in_road)
}

/// "701 GREENWOOD Avenue, OH 45229": with no town, libpostal takes the street type for one
/// ("road=greenwood", "city=avenue"), or the state for part of the road ("1516 RUTH Avenue, OH"
/// gives "road=ruth avenue oh"). When the street part alone parses as a house on a road, and the
/// state part as a state, those are the parse.
pub fn street_aligned(
    head: &[(String, String)],
    head_text: &str,
    tail: &[(String, String)],
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let city = field(whole, "city");
    // "avenue" or "shady lane" taken for the town, or the street name with no road left
    // ("Clairemont"). A town the whole parse already split from its road ("Temple" after
    // "Kutztown Rd.") is also inside the unsplit street part; dropping it leaves the postcode,
    // which another country may share ("19560" is French).
    let word = |w: &str| canon::canon(w).is_some() || canon::is_canonical(w);
    let last = city.split_whitespace().next_back().unwrap_or("");
    let streetish = !city.is_empty() && (word(city) || word(last));
    let in_head = !city.is_empty() && head_text.to_lowercase().contains(city);
    let misread = field(whole, "state").is_empty()
        || (in_head && (streetish || field(whole, "road").is_empty()));
    let only = tail
        .iter()
        .all(|(k, _)| matches!(k.as_str(), "state" | "postcode"));
    (misread && head_aligns(head) && only && !field(tail, "state").is_empty())
        .then(|| head.iter().chain(tail).cloned().collect())
}

fn nz_localities_aligned(
    head: &[(String, String)],
    suburb: &str,
    city: &str,
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let suburb = suburb.trim().to_lowercase();
    let city = city.trim().to_lowercase();
    if !head_aligns(head)
        || !one_house(head)
        || field(head, "house_number") != field(whole, "house_number")
        || field(head, "unit") != field(whole, "unit")
        || !whole.iter().all(|(k, _)| {
            matches!(
                k.as_str(),
                "house_number" | "road" | "unit" | "city" | "suburb" | "city_district" | "house"
            )
        })
        || [&suburb, &city]
            .iter()
            .any(|s| !s.chars().any(char::is_alphabetic) || s.chars().any(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let mut parsed = head.to_vec();
    parsed.extend([("suburb".into(), suburb), ("city".into(), city)]);
    Some(parsed)
}

fn postal_city_aligned(
    head: &[(String, String)],
    tail: &str,
    country: &(String, String),
    whole: &[(String, String)],
) -> Option<Vec<(String, String)>> {
    let cc = crate::matching::expected_country(&[country.clone()].into_iter().collect());
    let words: Vec<_> = tail.split_whitespace().collect();
    let (postcode, city) = [2, 1]
        .into_iter()
        .filter(|&at| at < words.len())
        .map(|at| (words[..at].join(" "), words[at..].join(" ")))
        .find(|(pc, _)| crate::matching::postcode_shape(&cc, pc) == Some(true))?;
    // A conflicting two-word postcode must stay intact: "9406 KX" is not Belgian "9406".
    if postcode == words[0]
        && words.len() > 2
        && crate::matching::postcode_shape("nl", &words[..2].join(" ")) == Some(true)
    {
        return None;
    }
    if !head_aligns(head)
        || !one_house(head)
        || (postcode_country_suffix(tail, whole)
            && ["house_number", "road"]
                .iter()
                .any(|key| field(head, key) != field(whole, key)))
        || !["state", "state_district", "suburb", "city_district"]
            .iter()
            .all(|k| field(whole, k).is_empty())
        || !city.chars().any(char::is_alphabetic)
        || city.chars().any(|c| c.is_ascii_digit())
    {
        return None;
    }
    let mut parsed = head.to_vec();
    parsed.extend([
        ("postcode".into(), postcode.to_lowercase()),
        ("city".into(), city.trim().to_lowercase()),
    ]);
    Some(parsed)
}

fn postcode_country_suffix(tail: &str, whole: &[(String, String)]) -> bool {
    if !one_house(whole)
        || whole.iter().filter(|(k, _)| k == "postcode").count() != 1
        || whole.iter().filter(|(k, _)| k == "country").count() != 2
        || whole.iter().any(|(k, _)| k == "country_region")
    {
        return false;
    }
    let words: Vec<_> = tail.split_whitespace().collect();
    // postcode_shape also accepts outcodes and strips non-ASCII characters.
    words.len() > 2
        && words[0].len() == 4
        && words[0].bytes().all(|b| b.is_ascii_digit())
        && words[1].len() == 2
        && words[1].bytes().all(|b| b.is_ascii_alphabetic())
        && whole.windows(2).any(|pair| {
            pair[0].0 == "postcode"
                && pair[0].1 == words[0]
                && pair[1].0 == "country"
                && pair[1].1.eq_ignore_ascii_case(words[1])
        })
}

fn postal_without_commas(
    q: &str,
    whole: &[(String, String)],
    libpostal: impl Fn(&str) -> Result<Vec<(String, String)>>,
) -> Result<Option<Vec<(String, String)>>> {
    if whole.iter().filter(|(k, _)| k == "house_number").count() < 2 {
        return Ok(None);
    }
    let Some(country) = whole.last().filter(|(k, _)| k == "country") else {
        return Ok(None);
    };
    let cc = crate::matching::expected_country(&[country.clone()].into_iter().collect());
    let lower = q.trim().to_lowercase();
    let Some(body) = lower.strip_suffix(&country.1).map(str::trim_end) else {
        return Ok(None);
    };
    for (_, postcode) in whole.iter().rev().filter(|(k, v)| {
        k == "house_number" && crate::matching::postcode_shape(&cc, v) == Some(true)
    }) {
        if let Some((head, rest)) = body.rsplit_once(postcode)
            && head.ends_with(char::is_whitespace)
            && rest.starts_with(char::is_whitespace)
            && let Some(mut aligned) = postal_city_aligned(
                &libpostal(head.trim())?,
                &format!("{postcode}{rest}"),
                country,
                whole,
            )
        {
            aligned.push(country.clone());
            return Ok(Some(aligned));
        }
    }
    Ok(None)
}

/// A US state code, alone or before a ZIP: the second part of "street, STATE ZIP".
fn state_part(part: &str) -> bool {
    static STATE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^[A-Za-z]{2}(\s+\d{5}(-\d{4})?)?$").unwrap());
    STATE.is_match(part.trim())
}

fn head_aligns(head: &[(String, String)]) -> bool {
    !field(head, "road").is_empty()
        && head
            .iter()
            .all(|(k, _)| matches!(k.as_str(), "house_number" | "road" | "unit"))
}

/// "Country, STATE ZIP, town, number street": libpostal reads such a reversed address front to
/// back and finds two roads ("road=shawnee", "road=south dixon avenue") or takes the country for
/// a district, or with the state as one road ("road=united states wy"). When the first part is
/// the country, the parts in the usual order, if they parse into one house on one road.
fn reversed(
    q: &str,
    whole: &[(String, String)],
    libpostal: impl Fn(&str) -> Result<Vec<(String, String)>>,
) -> Result<Option<String>> {
    let parts: Vec<&str> = q.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return Ok(None);
    }
    let labelled = whole.first().is_some_and(|(k, v)| {
        matches!(k.as_str(), "country" | "state_district") && *v == parts[0].to_lowercase()
    });
    let repeats = whole
        .iter()
        .enumerate()
        .any(|(i, (k, _))| whole[..i].iter().any(|(seen, _)| seen == k));
    let country_first =
        labelled || (repeats && matches!(&libpostal(parts[0])?[..], [(k, _)] if k == "country"));
    Ok(country_first.then(|| parts.into_iter().rev().collect::<Vec<_>>().join(", ")))
}

fn one_house(parsed: &[(String, String)]) -> bool {
    ["house_number", "road"]
        .iter()
        .all(|key| parsed.iter().filter(|(k, _)| k == key).count() == 1)
}

/// A unit libpostal left at the end of the road ("south fillmore street #b", "201st avenue
/// souhtwest apt 10a"), moved to its own field. The unit has a digit or is one letter, so "Ste
/// Foy" and "Rue Sainte Anne" keep theirs.
fn unit_from_road(parsed: &mut Vec<(String, String)>) {
    static UNIT: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(
            r"^(.+?)\s+((?:#\s*|(?:apt|apartment|unit|suite|ste)\.?\s+#?)(?:[a-z]|[a-z]{0,3}\d[a-z0-9-]{0,4}))$",
        )
        .unwrap()
    });
    if parsed.iter().any(|(k, _)| k == "unit") {
        return;
    }
    let Some(at) = parsed.iter().position(|(k, _)| k == "road") else {
        return;
    };
    if let Some(c) = UNIT.captures(&parsed[at].1) {
        let (road, unit) = (c[1].to_string(), c[2].to_string());
        parsed[at].1 = road;
        parsed.insert(at + 1, ("unit".into(), unit));
    }
}

/// The process's one libpostal parser.
pub struct Parser {
    postal: Mutex<rustpostal::LibModules>,
    cache: Mutex<HashMap<Box<str>, Parsed>>,
    capacity: usize,
}

impl Parser {
    /// Parses kept by default: a resolve parses its query twice (resolver, then index) and
    /// libpostal's single lock is the service's throughput limit.
    pub const DEFAULT_CACHE: usize = 65_536;

    /// Loads the models, caching [`Self::DEFAULT_CACHE`] parses.
    pub fn setup() -> Result<Self> {
        Self::setup_with_cache(Self::DEFAULT_CACHE)
    }

    /// Loads the models; once per process, shared by the resolver and the index. `cache` is the
    /// number of parses kept; 0 parses every call.
    pub fn setup_with_cache(cache: usize) -> Result<Self> {
        let parser = Self::unloaded(cache);
        parser.load()?;
        Ok(parser)
    }

    /// A parser whose models are not loaded: [`Self::load`] it before the first `parse`. Lets
    /// the several seconds of model loading overlap other startup work.
    pub fn unloaded(cache: usize) -> Self {
        Self {
            postal: Mutex::new(rustpostal::LibModules::Address),
            cache: Mutex::default(),
            capacity: cache,
        }
    }

    /// Loads the models; once per process.
    pub fn load(&self) -> Result<()> {
        let postal = self.postal.lock().unwrap_or_else(|e| e.into_inner());
        postal
            .setup()
            .map_err(|_| anyhow::anyhow!("libpostal model setup failed"))
    }

    /// Blocks: call it off the async executor.
    // ponytail: one lock serializes every libpostal call (the parser keeps one scratch context
    // in native state, so parses cannot overlap). The cache is emptied when full, not an LRU.
    pub fn parse(&self, q: &str) -> Result<Parsed> {
        if self.capacity > 0
            && let Some(hit) = self.cache.lock().unwrap_or_else(|e| e.into_inner()).get(q)
        {
            return Ok(hit.clone());
        }
        let parsed: Parsed = {
            let _guard = self.postal.lock().unwrap_or_else(|e| e.into_inner());
            let libpostal = |text: &str| -> Result<Vec<(String, String)>> {
                Ok(rustpostal::address::parse_address(text, None, None)?
                    .into_iter()
                    .collect())
            };
            let whole = libpostal(q)?;
            let mut parts: Vec<&str> = q.split(',').collect();
            // "…, NJ 07111, United States": the country part is set aside while the rest aligns
            let country = match whole.last() {
                Some((k, v))
                    if k == "country"
                        && parts.len() >= 3
                        && parts.last().is_some_and(|p| p.trim().to_lowercase() == *v) =>
                {
                    parts.pop();
                    Some((k.clone(), v.clone()))
                }
                _ => None,
            };
            let inner: Vec<_> = whole
                .iter()
                .filter(|(k, _)| country.is_none() || k != "country")
                .cloned()
                .collect();
            let mut aligned = None;
            match parts[..] {
                [head, tail] if state_part(tail) => {
                    aligned = street_aligned(
                        &libpostal(head.trim())?,
                        head,
                        &libpostal(tail.trim())?,
                        &inner,
                    );
                    // "Riverside, CT": a lone town and its state code that libpostal fused into
                    // one road ("riverside ct")
                    if aligned.is_none()
                        && tail.trim().len() == 2
                        && matches!(&whole[..], [(k, _)] if k == "road")
                        && !head.chars().any(|c| c.is_ascii_digit())
                    {
                        aligned = Some(vec![
                            ("city".to_string(), head.trim().to_lowercase()),
                            ("state".to_string(), tail.trim().to_lowercase()),
                        ]);
                    }
                }
                // "New York, United States": the comma ends the place, but libpostal splits it
                // into a town "new" and a state "york" when the country follows
                [head, tail]
                    if whole.last().is_some_and(|(k, v)| {
                        k == "country" && *v == tail.trim().to_lowercase()
                    }) =>
                {
                    let split = whole[..whole.len() - 1]
                        .iter()
                        .map(|(_, v)| v.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    // only a name split into places: a postcode or house number is its own field
                    let places = whole[..whole.len() - 1].iter().all(|(k, _)| {
                        matches!(
                            k.as_str(),
                            "city" | "state" | "state_district" | "suburb" | "city_district"
                        )
                    });
                    if whole.len() > 2
                        && places
                        && let [(k, v)] = &libpostal(head.trim())?[..]
                        && *v == split
                    {
                        aligned =
                            Some(vec![(k.clone(), v.clone()), whole[whole.len() - 1].clone()]);
                    }
                }
                // "street, county, town, STATE ZIP": the parts between the street and the town
                // are the county
                [head, ref extras @ .., middle, tail] if alignable(middle, extras, &inner) => {
                    let head = libpostal(head.trim())?;
                    if head_aligns(&head) {
                        aligned =
                            comma_aligned(&head, extras, middle, &libpostal(tail.trim())?, &inner);
                    }
                }
                [head, tail]
                    if country.is_some()
                        && whole
                            .iter()
                            .any(|(k, v)| k == "road" && !head.to_lowercase().contains(v)) =>
                {
                    aligned = postal_city_aligned(
                        &libpostal(head.trim())?,
                        tail,
                        country.as_ref().unwrap(),
                        &inner,
                    );
                }
                // The general alignment may repair the road; this path must preserve it.
                [head, tail] if country.is_some() && postcode_country_suffix(tail, &whole) => {
                    aligned = postal_city_aligned(
                        &libpostal(head.trim())?,
                        tail,
                        country.as_ref().unwrap(),
                        &whole,
                    );
                }
                _ => {}
            }
            if aligned.is_none() && parts.len() == 1 {
                aligned = postal_without_commas(q, &whole, libpostal)?;
            }
            let mut parsed = match aligned {
                Some(mut a) => {
                    a.extend(country);
                    a
                }
                None => whole,
            };
            unit_from_road(&mut parsed);
            parsed.into_iter().collect()
        };
        if self.capacity > 0 {
            let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            if cache.len() >= self.capacity {
                cache.clear();
            }
            cache.insert(q.into(), parsed.clone());
        }
        Ok(parsed)
    }

    /// A reversed address ("Country, STATE ZIP, town, number street") in the usual order, for
    /// the caller to resolve instead. Not done in [`Self::parse`]: the resolver's structured
    /// re-queries are written country first on purpose.
    pub fn reordered(&self, q: &str) -> Result<Option<String>> {
        let whole = self.parse(q)?;
        let Some(turned) = reversed(q, &whole, |t| Ok(self.parse(t)?.to_vec()))? else {
            return Ok(None);
        };
        Ok(one_house(&self.parse(&turned)?).then_some(turned))
    }
}

/// Refuses an index whose `build.json` names another builder: its terms were folded, and its
/// house numbers packed, by code this binary may not match. Hand-built indexes have no
/// `build.json` and are opened as they are.
fn check_builder(dir: &Path) -> Result<()> {
    let Ok(text) = fs::read_to_string(dir.join("build.json")) else {
        return Ok(());
    };
    let build: Value = serde_json::from_str(&text).context("build.json")?;
    let found = build["builder_hash"].as_str().unwrap_or("none");
    anyhow::ensure!(
        found == env!("BUILDER_HASH"),
        "index {} was built by builder {found}, this binary reads builder {}'s indexes",
        dir.display(),
        env!("BUILDER_HASH")
    );
    Ok(())
}

impl Index {
    /// Blocks for several seconds: the place table is read into memory.
    pub fn open(dir: &Path, parser: Arc<Parser>) -> Result<Index> {
        check_builder(dir)?;
        let mmap = tantivy::directory::MmapDirectory::open(dir)?;
        let index = tantivy::Index::open(ReadOnly(mmap.clone()))
            .with_context(|| format!("open index {}", dir.display()))?;
        index.tokenizers().register("fold", schema::analyzer());
        let searcher = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::Manual)
            .try_into()?
            .searcher();
        let (_, f) = schema::schema();
        let shards: usize = fs::read_to_string(dir.join("numbers.shards"))
            .context("numbers.shards")?
            .trim()
            .parse()
            .context("numbers.shards")?;
        let numbers = (0..shards)
            .map(|s| {
                use tantivy::directory::Directory;
                let file = mmap.open_read(Path::new(&format!("numbers-{s:02}.fst")))?;
                Ok(tantivy_fst::Map::from(tantivy_fst::raw::Fst::new(
                    file.read_bytes()?,
                )?))
            })
            .collect::<Result<_>>()?;
        let reverse = if dir.join(reverse::FILE).exists() {
            use tantivy::directory::Directory;
            let file = mmap.open_read(Path::new(reverse::FILE))?;
            Some(reverse::Reverse::new(file.read_bytes()?)?)
        } else {
            None
        };
        let areas = if dir.join(area::FILE).exists() {
            use tantivy::directory::Directory;
            let file = mmap.open_read(Path::new(area::FILE))?;
            Some(area::Areas::new(file.read_bytes()?)?)
        } else {
            None
        };
        let streets = if dir.join(reverse::STREETS).exists() {
            use tantivy::directory::Directory;
            let file = mmap.open_read(Path::new(reverse::STREETS))?;
            Some(reverse::Streets::new(file.read_bytes()?)?)
        } else {
            None
        };
        let reference: Value =
            serde_json::from_str(include_str!("../../data/reference.json")).unwrap();
        let below = iso_by_item(&reference, "iso3166_2_lvl6");
        let mut iso = iso_by_item(&reference, "iso3166_2");
        iso.extend(&below);
        let (mut places, own, history) = load_places(&dir.join("places.tsv"), &iso)?;
        let (below_region, own): (Vec<_>, Vec<_>) = own
            .into_iter()
            .partition(|(_, code)| reference["iso3166_2_lvl6"].get(code).is_some());
        {
            // counties containing a same-named neighbourhood, minus those with such a locality
            let mut hood_named: std::collections::HashSet<u32> = Default::default();
            let mut town_named: std::collections::HashSet<u32> = Default::default();
            for p in &places {
                let parent = p.parent;
                let named = match &*p.subtype {
                    "neighborhood" | "macrohood" => &mut hood_named,
                    "locality" | "localadmin" => &mut town_named,
                    _ => continue,
                };
                if parent != NONE
                    && places[parent as usize].subtype.as_ref() == "county"
                    && name_key(&p.primary) == name_key(&places[parent as usize].primary)
                {
                    named.insert(parent);
                }
            }
            for c in hood_named.difference(&town_named) {
                places[*c as usize].city_like = true;
            }
        }
        let localities = locality::Localities::new(
            places
                .iter()
                .enumerate()
                .filter(|(_, p)| p.subtype.as_ref() == "locality")
                .map(|(i, p)| (i as u32, p.x as f64, p.y as f64)),
        );
        let mut countries = HashMap::new();
        let mut regions: HashMap<String, u32> = HashMap::new();
        let mut country_places: HashMap<String, u32> = HashMap::new();
        for (i, p) in places.iter().enumerate() {
            if p.subtype.as_ref() == "country" && !p.cc.is_empty() {
                country_places.entry(p.cc.to_string()).or_insert(i as u32);
            }
            if matches!(&*p.subtype, "country" | "dependency") && !p.cc.is_empty() {
                for n in p.all_names() {
                    countries.entry(fold(n)).or_insert_with(|| p.cc.to_string());
                }
                countries.insert(p.cc.to_string(), p.cc.to_string());
            }
            if p.subtype.as_ref() == "region" && !p.region.is_empty() {
                regions.entry(p.region.to_string()).or_insert(i as u32);
            }
        }
        // A code Overture does supply wins, so a later release that names it needs no data change.
        let (mut subdivisions, mut subdivision_codes) = (HashMap::new(), HashMap::new());
        for (place, code) in below_region {
            if places[place as usize].subtype.as_ref() == "county"
                && !regions.contains_key(&code)
                && let std::collections::hash_map::Entry::Vacant(entry) = subdivisions.entry(code)
            {
                subdivision_codes.insert(place, entry.key().as_str().into());
                entry.insert(place);
            }
        }
        let mut own_codes = HashMap::new();
        for (place, code) in own {
            if let std::collections::hash_map::Entry::Vacant(entry) = regions.entry(code) {
                own_codes.insert(place, entry.key().as_str().into());
                entry.insert(place);
            }
        }
        for (name, cc) in reference["countries"].as_object().into_iter().flatten() {
            if let Some(cc) = cc.as_str() {
                countries
                    .entry(fold(name))
                    .or_insert_with(|| cc.to_lowercase());
            }
        }
        countries.entry("uae".into()).or_insert_with(|| "ae".into());
        let place_names = suggest::place_names(&places)?;
        let landmarks = landmark::Landmarks::open(dir, places.len())?;
        let mut provinces = HashMap::new();
        for (code, place) in irish_provinces(&places, &regions) {
            provinces.insert(code, places.len() as u32);
            places.push(place);
        }
        let mut country_codes = HashMap::new();
        let columns: Vec<Columns> = searcher
            .segment_readers()
            .iter()
            .map(|r| Columns::open(r, f.country, &mut country_codes))
            .collect::<Result<_>>()?;
        let suggestion_scope =
            ring::digest::digest(&ring::digest::SHA256, &fs::read(dir.join("meta.json"))?)
                .as_ref()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
        Ok(Index {
            searcher,
            suggestion_scope,
            columns,
            country_codes,
            f,
            numbers,
            reverse,
            areas,
            streets,
            places,
            history,
            localities,
            countries,
            country_places,
            regions,
            own_codes,
            subdivisions,
            subdivision_codes,
            provinces,
            parser,
            near: Mutex::default(),
            prefixes: Mutex::default(),
            place_names,
            place_prefixes: Mutex::default(),
            suggest: Default::default(),
            landmarks,
        })
    }

    pub fn places_loaded(&self) -> usize {
        self.places.len()
    }

    fn nz_comma_house(&self, q: &str, whole: &[(String, String)]) -> Result<Option<(Parsed, Hit)>> {
        let parts: Vec<_> = q.split(',').map(str::trim).collect();
        let [head, suburb, city, country] = parts.as_slice() else {
            return Ok(None);
        };
        if self.country(country).as_deref() != Some("nz")
            || whole
                .iter()
                .filter(|(k, _)| k == "country")
                .any(|(_, country)| self.country(country).as_deref() != Some("nz"))
        {
            return Ok(None);
        }
        let inner: Vec<_> = whole
            .iter()
            .filter(|(k, _)| k != "country")
            .cloned()
            .collect();
        let Some(mut parsed) =
            nz_localities_aligned(&self.parser.parse(head)?, suburb, city, &inner)
        else {
            return Ok(None);
        };
        let road = field(&parsed, "road");
        let number = field(&parsed, "house_number");
        let mut hit = self
            .streets(road, Some(number), Some(suburb), None, Some("nz"), true)
            .into_iter()
            .find(|hit| {
                matches!(hit, Hit::Street { name, number: Some(n), postal_town, .. }
                if fold(name) == fold(road)
                    && schema::number_key(n) == schema::number_key(number)
                    && self.house_locality_named(hit, city)
                    && (self.house_locality_named(hit, suburb)
                        || postal_town.as_deref().is_some_and(|town| {
                            crate::matching::names_agree(suburb, town)
                        })))
            });
        if let Some(Hit::Street { city, .. }) = &mut hit {
            *city = None;
        }
        parsed.push(("country".into(), country.to_lowercase()));
        Ok(hit.map(|hit| (parsed.into(), hit)))
    }

    pub(crate) fn parse_address(&self, q: &str, corroboration: bool) -> Result<Option<Parsed>> {
        let mut parsed = self.parser.parse(q)?;
        if q.contains('號')
            && let Some((units, road, number)) = chinese(q)
            && let [state, city] = units.as_slice()
            && state.ends_with('市')
            && city.ends_with('區')
        {
            let native = q.split([',', '，']).find(|p| p.contains('號'));
            if let Some(native) = native {
                let cc = self.country(field(&parsed, "country"));
                let city_key = name_key(city);
                let state_key = name_key(state);
                if !self
                    .places(city, cc.as_deref(), Some(state), &|p| p.named(city_key))
                    .iter()
                    .any(|hit| {
                        let Hit::Place(id) = hit else { return false };
                        self.chain(*id)
                            .iter()
                            .any(|&p| self.places[p as usize].named(state_key))
                    })
                {
                    return Ok(None);
                }
                let mut repaired = Vec::new();
                for (key, value) in parsed.iter() {
                    if matches!(key.as_str(), "road" | "house_number") {
                        continue;
                    }
                    if matches!(
                        key.as_str(),
                        "city" | "suburb" | "city_district" | "state" | "state_district"
                    ) {
                        if !native.contains(value.as_str()) {
                            return Ok(None);
                        }
                        continue;
                    }
                    repaired.push((key.clone(), value.clone()));
                }
                repaired.extend([
                    ("state".into(), state.clone()),
                    ("city".into(), city.clone()),
                    ("road".into(), road),
                    ("house_number".into(), number),
                ]);
                parsed = repaired.into();
            }
        }
        // Repairing the parse also changes structured fallbacks; require a corroborated house first.
        if let Some((aligned, _)) = self.nz_comma_house(q, &parsed)? {
            return Ok(Some(aligned));
        }
        if !parsed
            .iter()
            .any(|(k, _)| matches!(k.as_str(), "house_number" | "postcode"))
        {
            let parts: Vec<_> = q.split(',').map(str::trim).collect();
            if let [town, region, country] = parts.as_slice()
                && let Some(cc) = self.country(country)
                && self
                    .country(field(&parsed, "country"))
                    .is_none_or(|stated| stated == cc)
            {
                let mut name = *town;
                let (mut found, mut exact) = self.places_named(
                    name,
                    Some(&cc),
                    Some(region),
                    &|p| matches!(p.subtype.as_ref(), "locality" | "localadmin"),
                    false,
                );
                if !exact
                    && let Some((bare, kind)) = town.rsplit_once(char::is_whitespace)
                    && kind.eq_ignore_ascii_case("village")
                {
                    name = bare.trim();
                    (found, exact) = self.places_named(
                        name,
                        Some(&cc),
                        Some(region),
                        &|p| p.key() == "village",
                        false,
                    );
                }
                if exact {
                    let canonical = found.iter().any(|hit| {
                        let Hit::Place(id) = hit else { return false };
                        let p = &self.places[*id as usize];
                        crate::matching::names_agree(name, &p.primary)
                            || crate::matching::names_agree(name, p.name(true))
                    });
                    for hit in found {
                        let Hit::Place(id) = hit else { continue };
                        if !self.places[id as usize].named(name_key(name)) {
                            continue;
                        }
                        if let Some(parent) = self.chain(id).iter().skip(1).find_map(|&id| {
                            let p = &self.places[id as usize];
                            (matches!(
                                p.subtype.as_ref(),
                                "localadmin" | "county" | "macrocounty" | "region" | "macroregion"
                            ) && p.all_names().any(crate::matching::agrees_with(region)))
                            .then_some(p)
                        }) {
                            let key = if matches!(parent.subtype.as_ref(), "region" | "macroregion")
                            {
                                "state"
                            } else {
                                "state_district"
                            };
                            return Ok(Some(
                                vec![
                                    ("city".into(), name.into()),
                                    (key.into(), parent.name(true).into()),
                                    ("country".into(), (*country).into()),
                                ]
                                .into(),
                            ));
                        }
                    }
                    if !canonical {
                        let (_, known) = self.places_exact(region, Some(&cc), None, &|p| {
                            matches!(
                                p.subtype.as_ref(),
                                "localadmin" | "county" | "macrocounty" | "region" | "macroregion"
                            )
                        });
                        if known {
                            // An alias cannot bypass a contradictory qualifier through fragment fallbacks.
                            return Ok(None);
                        }
                    }
                }
            }
        }
        if corroboration && parsed.iter().any(|(k, _)| k == "postcode") {
            return Ok(Some(parsed));
        }
        let first_cc = parsed
            .iter()
            .find(|(l, _)| l == "country")
            .and_then(|(_, c)| self.country(c));
        let stated = parsed
            .iter()
            .find(|(l, _)| l == "postcode")
            .map(|(_, v)| pc_norm(v));
        // A US house number can also be a ZIP ("15411 Woodcliffe Trail, IN").
        let house = parsed
            .iter()
            .find(|(l, _)| l == "house_number")
            .map(|(_, v)| pc_norm(v))
            .filter(|_| parsed.iter().any(|(l, _)| l == "state"));
        if let Some((pc, rest)) = self.find_postcode(q, first_cc.as_deref())
            && stated.as_ref().is_none_or(|s| {
                pc_norm(&pc).len() > s.len() && pc_norm(&pc).starts_with(s.as_str())
            })
            && house != Some(pc_norm(&pc))
            // Lookup may try an ambiguous number; corroboration cannot overwrite requested fields.
            && (!corroboration
                || !["house_number", "road", "po_box", "unit", "level"].iter().any(|key| {
                    parsed.iter().find(|(k, _)| k == key).is_some_and(|(_, value)| {
                        pc_norm(value).contains(&pc_norm(&pc))
                    })
                }))
        {
            let mut repaired = self.parser.parse(&rest)?.to_vec();
            if corroboration {
                if ["house_number", "road"].iter().any(|key| {
                    repaired.iter().filter(|(k, _)| k == key).count() > 1
                        || (!field(&parsed, key).is_empty()
                            && field(&parsed, key) != field(&repaired, key))
                }) {
                    return Ok(Some(parsed));
                }
                for (key, value) in parsed.iter().filter(|(k, _)| {
                    matches!(
                        k.as_str(),
                        "country" | "state" | "state_district" | "country_region"
                    )
                }) {
                    repaired.retain(|(k, _)| k != key);
                    repaired.push((key.clone(), value.clone()));
                }
            }
            repaired.push(("postcode".into(), pc));
            parsed = repaired.into();
        }
        Ok(Some(parsed))
    }

    fn parse(&self, q: &str) -> Vec<(String, String)> {
        // libpostal returns "İ" decomposed; the tokenizer would split at the combining dot
        let clean = |t: &str| {
            t.chars()
                .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
                .collect()
        };
        self.parse_address(q, false)
            .ok()
            .flatten()
            .map(|v| v.iter().map(|(l, t)| (l.clone(), clean(t))).collect())
            .unwrap_or_default()
    }

    fn fast(&self, a: DocAddress, field: &str) -> u64 {
        let c = &self.columns[a.segment_ord as usize];
        match field {
            "id" => &c.id,
            "place" => &c.place,
            _ => unreachable!("{field}"),
        }
        .first(a.doc_id)
        .unwrap_or(0)
    }

    fn fast_f64(&self, a: DocAddress, field: &str) -> f64 {
        let c = &self.columns[a.segment_ord as usize];
        match field {
            "x" => &c.x,
            "y" => &c.y,
            _ => unreachable!("{field}"),
        }
        .first(a.doc_id)
        .unwrap_or(0.0)
    }

    /// The `n` most important documents of `kind` (and of country `cc`, if given) that match
    /// `cl`; with no other required clause, all of them. TopDocs' fast-field ordering reopens the
    /// column of every segment on every query, which cost more than the search itself.
    fn top_of(
        &self,
        kind: Kind,
        cc: Option<&str>,
        cl: Vec<(Occur, Box<dyn TQuery>)>,
        n: usize,
    ) -> Vec<DocAddress> {
        self.matching(kind, cc, cl, n, false)
    }

    /// Whether `top_of` would find anything, without looking for the most important match.
    fn any_of(&self, kind: Kind, cc: Option<&str>, cl: Vec<(Occur, Box<dyn TQuery>)>) -> bool {
        !self.matching(kind, cc, cl, 1, true).is_empty()
    }

    fn matching(
        &self,
        kind: Kind,
        cc: Option<&str>,
        mut cl: Vec<(Occur, Box<dyn TQuery>)>,
        n: usize,
        any: bool,
    ) -> Vec<DocAddress> {
        let country = match cc.map(|cc| self.country_codes.get(&cc.to_uppercase())) {
            Some(None) => return vec![],
            found => found.flatten().copied(),
        };
        if cl.iter().all(|(occur, _)| *occur != Occur::Must) {
            cl.push(must(Box::new(tantivy::query::AllQuery)));
        }
        let key = MEMO
            .with_borrow(Option::is_some)
            .then(|| format!("{kind:?} {country:?} {n} {any} {cl:?}"));
        if let Some(found) = key.as_ref().and_then(|key| {
            MEMO.with_borrow(|memo| memo.as_ref().and_then(|m| m.get(key).cloned()))
        }) {
            return found;
        }
        let by = ByImportance {
            n,
            kind,
            country,
            any,
            columns: &self.columns,
        };
        let found = self.searcher.search(&BooleanQuery::new(cl), &by).unwrap();
        if let Some(key) = key {
            MEMO.with_borrow_mut(|memo| memo.as_mut().map(|m| m.insert(key, found.clone())));
        }
        found
    }

    /// Runs `f` with lookups remembered until it returns. One request repeats a lookup several
    /// times (the same town for each fallback and street scope): over half of a sample's lookups
    /// were repeats. Nothing is kept across requests.
    pub fn memoized<T>(&self, f: impl FnOnce() -> T) -> T {
        struct Forget;
        impl Drop for Forget {
            fn drop(&mut self) {
                MEMO.set(None);
            }
        }
        if MEMO.with_borrow(Option::is_some) {
            return f();
        }
        MEMO.set(Some(HashMap::new()));
        let _forget = Forget;
        f()
    }

    fn musts(&self, field: Field, text: &str) -> Vec<(Occur, Box<dyn TQuery>)> {
        fold(text)
            .split(' ')
            .filter(|t| !t.is_empty())
            .map(|t| must(term(field, t)))
            .collect()
    }

    fn country(&self, text: &str) -> Option<String> {
        // The analyzer drops tokens of 40+ bytes; losing an address must not leave a bare country.
        if text.len() >= 40
            && text
                .split(|c: char| !c.is_alphanumeric())
                .any(|word| word.len() >= 40)
        {
            return None;
        }
        let folded = fold(text);
        if folded.is_empty() {
            return None;
        }
        if let Some(c) = self.countries.get(&folded) {
            return Some(c.clone());
        }
        // "india india": a repeated field the parser joined
        let words: Vec<&str> = folded.split(' ').collect();
        let half = words.len() / 2;
        (half > 0 && words.len().is_multiple_of(2) && words[..half] == words[half..])
            .then(|| self.countries.get(&words[..half].join(" ")).cloned())
            .flatten()
    }

    /// The countries a stated region may be in: those with an ISO 3166-2 code that ends in a
    /// written code ("PA", "BCS"), or the one whose region has that name.
    fn region_countries(&self, region: &str) -> Vec<String> {
        let code: String = region
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .collect::<String>()
            .to_uppercase();
        let mut found = vec![];
        if (2..=3).contains(&code.len())
            && code.len() == region.chars().filter(|c| *c != '.').count()
        {
            let suffix = format!("-{code}");
            found.extend(
                self.regions
                    .keys()
                    .filter_map(|k| k.strip_suffix(&suffix)?.to_lowercase().into()),
            );
        }
        let named = crate::matching::implied_country(
            &[("state".to_string(), region.to_string())]
                .into_iter()
                .collect(),
        );
        if !named.is_empty() {
            found.push(named);
        }
        found.sort();
        found.dedup();
        found
    }

    fn chain(&self, mut p: u32) -> Vec<u32> {
        let mut out = vec![];
        while p != NONE && out.len() < 16 {
            out.push(p);
            p = self.places[p as usize].parent;
        }
        out
    }

    /// Places named `text`: exact name matches first, then those under a place named `within`,
    /// then by importance.
    fn places(
        &self,
        text: &str,
        cc: Option<&str>,
        within: Option<&str>,
        ok: &dyn Fn(&Place) -> bool,
    ) -> Vec<Hit> {
        self.places_exact(text, cc, within, ok).0
    }

    /// `places`, and whether the first one is named exactly `text`; failing that, the same for
    /// `text` without an administrative word ("Vas County" is named "Vas").
    fn places_exact(
        &self,
        text: &str,
        cc: Option<&str>,
        within: Option<&str>,
        ok: &dyn Fn(&Place) -> bool,
    ) -> (Vec<Hit>, bool) {
        let found = self.places_named(text, cc, within, ok, false);
        if found.0.is_empty()
            && let Some(bare) = without_admin_word(text)
        {
            // "Kreisfreie Stadt München" names a city; "Vas County" an area
            let city_word = ["city", "stadt", "kreisfreie", "township", "borough", "town"]
                .iter()
                .any(|w| text.split_whitespace().any(|t| t.to_lowercase() == *w));
            return self.places_named(&bare, cc, within, ok, !city_word);
        }
        found
    }

    fn places_named(
        &self,
        text: &str,
        cc: Option<&str>,
        within: Option<&str>,
        ok: &dyn Fn(&Place) -> bool,
        area_named: bool,
    ) -> (Vec<Hit>, bool) {
        let cl = self.musts(self.f.names, text);
        if cl.is_empty() {
            return (vec![], false);
        }
        let within_text = within;
        let want = name_key(text);
        let long = long_words(text);
        // "Cauca Department" is named "Cauca"
        // a region written as its code ("Elmwood Park, NJ"): the "-NJ" of US-NJ
        let within_code = within
            .map(str::trim)
            .filter(|w| (2..=3).contains(&w.len()) && w.chars().all(|c| c.is_ascii_alphabetic()))
            .map(|w| format!("-{}", w.to_uppercase()));
        let within: Vec<u64> = within
            .filter(|w| !fold(w).is_empty())
            .into_iter()
            .flat_map(|w| std::iter::once(w.to_string()).chain(without_admin_word(w)))
            .map(|w| name_key(&w))
            .collect();
        let mut ranked = self.top_of(Kind::Place, cc, cl, 100);
        // The stated region places a small town among many of its name ("Temple, PA"): when
        // none of the top 100 is inside it, look in the countries that have such a region too.
        let is_inside = |a: &DocAddress| {
            let id = self.fast(*a, "id") as u32;
            let pl = &self.places[id as usize];
            within.iter().any(|&w| {
                self.chain(id)
                    .iter()
                    .skip(1)
                    .any(|&x| self.places[x as usize].named(w))
            }) || within_code
                .as_ref()
                .is_some_and(|c| pl.region.ends_with(c.as_str()))
        };
        if cc.is_none()
            && let Some(region) = within_text
            && !ranked.iter().any(is_inside)
        {
            for country in self.region_countries(region) {
                for a in self.top_of(
                    Kind::Place,
                    Some(&country),
                    self.musts(self.f.names, text),
                    100,
                ) {
                    if !ranked.contains(&a) {
                        ranked.push(a);
                    }
                }
            }
        }
        let ids: Vec<(usize, u32)> = ranked
            .into_iter()
            .enumerate()
            .map(|(rank, a)| (rank, self.fast(a, "id") as u32))
            .filter(|(_, id)| ok(&self.places[*id as usize]))
            .filter(|(_, id)| {
                long.is_empty() || names_hold(self.places[*id as usize].all_names(), &long)
            })
            .collect();
        // The most important exact match among administrative areas, and the population of the
        // largest exact-named settlement: only that settlement may outrank the area.
        let area = ids
            .iter()
            .map(|&(_, id)| &self.places[id as usize])
            .position(|p| {
                p.named(want)
                    && matches!(
                        &*p.subtype,
                        "region" | "macroregion" | "county" | "macrocounty"
                    )
            })
            .map(|k| ids[k].1)
            .filter(|_| !area_named);
        let largest = ids
            .iter()
            .map(|&(_, id)| &self.places[id as usize])
            .filter(|p| p.subtype.as_ref() == "locality" && p.named(want))
            .map(|p| p.population)
            .max()
            .unwrap_or(0);
        let mut found: Vec<(bool, bool, bool, usize, u32)> = ids
            .into_iter()
            .map(|(rank, id)| {
                let pl = &self.places[id as usize];
                let exact = pl.named(want);
                let chain = self.chain(id);
                let inside = within.iter().any(|&w| {
                    chain
                        .iter()
                        .skip(1)
                        .any(|&a| self.places[a as usize].named(w))
                }) || within_code
                    .as_ref()
                    .is_some_and(|c| pl.region.ends_with(c.as_str()));
                // A major city outranks a same-named region, as with Nominatim (Tokyo, New York),
                // and so does a town inside a same-named area (Brasília in the Federal District
                // also called Brasília, Yalova in Yalova Province).
                let town = pl.subtype.as_ref() == "locality"
                    && (matches!(&*pl.class, "city" | "town") || pl.population >= 10_000)
                    && [&*pl.primary, pl.name(true)]
                        .iter()
                        .any(|n| exact_key(n) == exact_key(text))
                    && pl.population >= largest;
                let big = pl.subtype.as_ref() == "locality" && pl.population >= 500_000
                    || town && area.is_some_and(|a| chain.contains(&a))
                    || area_named && !matches!(&*pl.subtype, "locality" | "localadmin");
                (!exact, !inside, !big, rank, id)
            })
            .collect();
        found.sort();
        // Places outside the stated region ("big bend, co") only when none is inside it.
        if found.first().is_some_and(|f| !f.1) {
            found.retain(|f| !f.1);
        }
        let exact = found.first().is_some_and(|f| !f.0);
        (
            found
                .into_iter()
                .take(10)
                .map(|(.., id)| Hit::Place(id))
                .collect(),
            exact,
        )
    }

    fn streets(
        &self,
        road: &str,
        number: Option<&str>,
        city: Option<&str>,
        postcode: Option<&str>,
        cc: Option<&str>,
        exact: bool,
    ) -> Vec<Hit> {
        let named: Vec<u32> = city
            .into_iter()
            .flat_map(|city| {
                self.places(city, cc, None, &|p| {
                    !matches!(&*p.subtype, "country" | "dependency")
                })
            })
            .filter_map(|h| match h {
                Hit::Place(id) => Some(id),
                _ => None,
            })
            .collect();
        let hits = self.streets_by(road, number, city, &named, postcode, cc, exact);
        if !hits.iter().any(|h| {
            matches!(
                h,
                Hit::Street {
                    number: Some(_),
                    ..
                }
            )
        }) && let Some(compact) = number.and_then(compact_house_suffix)
        {
            let found = self.streets(road, Some(&compact), city, postcode, cc, exact);
            if found.iter().any(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            }) {
                return found;
            }
        }
        if hits.is_empty() && number.is_some_and(|n| !n.trim().is_empty()) {
            let mut alternatives = cc
                .map(|cc| crate::matching::road_expansions(road, cc))
                .unwrap_or_default();
            alternatives.extend(crate::matching::bare_road(road, cc));
            if !alternatives.is_empty()
                && (!named.is_empty() || postcode.is_some_and(|p| self.postcode_exists(p, cc)))
            {
                let mut found = vec![];
                let mut ids = std::collections::BTreeSet::new();
                for name in alternatives {
                    let next: Vec<_> = self
                        .streets_by(&name, number, city, &named, postcode, cc, false)
                        .into_iter()
                        .filter(|h| {
                            matches!(h, Hit::Street { number: Some(_), postcode: own, .. }
                            if postcode_not_contradicted(own.as_deref(), postcode))
                        })
                        .collect();
                    if next.is_empty() {
                        continue;
                    }
                    let next_ids = next
                        .iter()
                        .filter_map(|h| match h {
                            Hit::Street { sid, .. } => Some(*sid),
                            _ => None,
                        })
                        .collect();
                    // "vn." can name two distinct roads, not just two spellings of one.
                    if !found.is_empty() && ids != next_ids {
                        return vec![];
                    }
                    ids = next_ids;
                    found = next;
                }
                if !found.is_empty() {
                    return found;
                }
            }
        }
        // A misspelt street ("Tarilwood Dr, Clinton"): only inside a stated town or postcode, and
        // only a house that exists there answers.
        if hits.is_empty()
            && number.is_some_and(|n| !n.trim().is_empty())
            && let Some(name) = self.respelt(road, city, &named, postcode, cc)
        {
            return self
                .streets_by(&name, number, city, &named, postcode, cc, false)
                .into_iter()
                .filter(|h| match h {
                    Hit::Street {
                        number: Some(_),
                        postcode: own,
                        ..
                    } => postcode_not_contradicted(own.as_deref(), postcode),
                    _ => false,
                })
                .collect();
        }
        hits
    }

    /// The name of a street in the stated town or postcode that has every word of `road` but
    /// one, and that one within an edit of the word there, or the misspelt street type
    /// ("srteet"). Words missing from the scope are counted first, so a road the index lacks
    /// costs a few lookups rather than an expansion.
    fn respelt(
        &self,
        road: &str,
        city: Option<&str>,
        named: &[u32],
        postcode: Option<&str>,
        cc: Option<&str>,
    ) -> Option<String> {
        // Most misses are streets the index lacks; without a real town or postcode to search
        // in, there is nothing to correct against.
        if named.is_empty() && !postcode.is_some_and(|p| self.postcode_exists(p, cc)) {
            return None;
        }
        let mut scope: Vec<(Occur, Box<dyn TQuery>)> = named
            .iter()
            .map(|id| (Occur::Should, term(self.f.ids, &id.to_string())))
            .collect();
        if let Some(city) = city {
            let postal = self.musts(self.f.city, city);
            if !postal.is_empty() {
                scope.push((Occur::Should, Box::new(BooleanQuery::new(postal))));
            }
        }
        if let Some(pc) = postcode.map(pc_norm).filter(|p| !p.is_empty()) {
            scope.push((Occur::Should, term(self.f.pcode, &pc)));
        }
        if scope.is_empty() {
            return None;
        }
        let scoped = |mut cl: Vec<(Occur, Box<dyn TQuery>)>| {
            cl.push(must(Box::new(BooleanQuery::new(
                scope.iter().map(|(o, q)| (*o, q.box_clone())).collect(),
            ))));
            cl
        };
        let words: Vec<String> = fold(road)
            .split(' ')
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect();
        let mut missing = words.iter().enumerate().filter(|(_, w)| {
            !self.any_of(Kind::Street, cc, scoped(vec![must(term(self.f.names, w))]))
        });
        let (wrong, word) = missing.next()?;
        if missing.next().is_some() || !fuzzable(word) {
            return None;
        }
        // a street type or direction, or a name ("adlee" is near "allee" and "adele"); the
        // words themselves, not tantivy's FuzzyTermQuery: that marks every document holding any
        // of them in a bitset per segment before the scope narrows anything
        let mut alternatives = misspelt_kind(word);
        alternatives.extend(self.terms_near(word).iter().cloned());
        if alternatives.is_empty() {
            return None;
        }
        let mut cl: Vec<(Occur, Box<dyn TQuery>)> = words
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != wrong)
            .map(|(_, w)| must(term(self.f.names, w)))
            .collect();
        cl.push(must(Box::new(BooleanQuery::new(
            alternatives
                .iter()
                .map(|w| (Occur::Should, term(self.f.names, w)))
                .collect(),
        ))));
        let best = self
            .top_of(Kind::Street, cc, scoped(cl), 1)
            .into_iter()
            .next()?;
        let doc: tantivy::TantivyDocument = self.searcher.doc(best).ok()?;
        let mut respelt = words.clone();
        let name = best_name(&doc, self.f.name, road);
        // the street's own spelling of the word, so the exact search finds it
        respelt[wrong] = fold(&name)
            .split(' ')
            .find(|w| alternatives.iter().any(|a| a == w))?
            .to_string();
        Some(respelt.join(" "))
    }

    /// Street-name words within one edit of `word`, with the same first letter.
    fn terms_near(&self, word: &str) -> std::sync::Arc<[String]> {
        if word.len() > MAX_WORD {
            return std::sync::Arc::new([]);
        }
        if let Some(hit) = self.near.lock().unwrap().get(word) {
            return hit.clone();
        }
        let mut out = std::collections::BTreeSet::new();
        for segment in self.searcher.segment_readers() {
            let Ok(index) = segment.inverted_index(self.f.names) else {
                continue;
            };
            let Ok(mut terms) = index.terms().search(OneEdit(word.as_bytes())).into_stream() else {
                continue;
            };
            while terms.advance() {
                out.extend(std::str::from_utf8(terms.key()).ok().map(String::from));
            }
        }
        let out: std::sync::Arc<[String]> = out.into_iter().collect();
        let mut cache = self.near.lock().unwrap();
        // ponytail: dropped whole when full, not LRU; ~100k words is a few MB
        if cache.len() >= 100_000 {
            cache.clear();
        }
        cache.insert(word.to_string(), out.clone());
        out
    }

    #[expect(clippy::too_many_arguments)]
    fn streets_by(
        &self,
        road: &str,
        number: Option<&str>,
        city: Option<&str>,
        named: &[u32],
        postcode: Option<&str>,
        cc: Option<&str>,
        prefer_exact: bool,
    ) -> Vec<Hit> {
        let base = || self.musts(self.f.names, road);
        let long = long_words(road);
        let exact = (!road.is_ascii() && road.contains(['路', '街', '道', '段', '巷', '弄']))
            .then(|| fold(road));
        if self.musts(self.f.names, road).is_empty() {
            return vec![];
        }
        let mut scopes: Vec<Option<Box<dyn TQuery>>> = vec![];
        let mut near = None;
        if let Some(city) = city {
            let ids: Vec<u32> = named
                .iter()
                .copied()
                // A neighbourhood's streets belong to the town around it (Seynod is now part of
                // Annecy): scope to that town too.
                .flat_map(|id| {
                    let town = matches!(
                        &*self.places[id as usize].subtype,
                        "macrohood" | "neighborhood" | "microhood"
                    )
                    .then(|| {
                        self.chain(id).into_iter().skip(1).find(|&p| {
                            matches!(&*self.places[p as usize].subtype, "locality" | "localadmin")
                        })
                    })
                    .flatten();
                    std::iter::once(id).chain(town)
                })
                .collect();
            // Towns before same-named counties: Milan's Via Brera, not one elsewhere in the
            // province of Milan that has the number.
            let county = |id: &&u32| {
                matches!(
                    &*self.places[**id as usize].subtype,
                    "county" | "macrocounty"
                )
            };
            // City-provinces (Istanbul, Bangkok, Moscow) hold streets outside the town proper,
            // but only a region named exactly like the city stands for it.
            let want = name_key(city);
            let region = |id: &&u32| {
                matches!(
                    &*self.places[**id as usize].subtype,
                    "region" | "macroregion"
                )
            };
            let named_region = |id: &&u32| region(id) && self.places[**id as usize].named(want);
            let scope = |ids: Vec<&u32>, postal: bool| {
                let mut any: Vec<(Occur, Box<dyn TQuery>)> = ids
                    .into_iter()
                    .map(|id| (Occur::Should, term(self.f.ids, &id.to_string())))
                    .collect();
                let city_terms = self.musts(self.f.city, city);
                if postal && !city_terms.is_empty() {
                    any.push((Occur::Should, Box::new(BooleanQuery::new(city_terms))));
                }
                (!any.is_empty()).then(|| Box::new(BooleanQuery::new(any)) as Box<dyn TQuery>)
            };
            if let Some(s) = scope(
                ids.iter().filter(|id| !county(id) && !region(id)).collect(),
                true,
            ) {
                scopes.push(Some(s));
            }
            if let Some(s) = scope(ids.iter().filter(county).collect(), false) {
                scopes.push(Some(s));
            }
            if let Some(s) = scope(ids.iter().filter(named_region).collect(), false) {
                scopes.push(Some(s));
            }
        }
        let town_scopes = scopes.len();
        if let Some(pc) = postcode.map(pc_norm).filter(|p| !p.is_empty()) {
            scopes.push(Some(term(self.f.pcode, &pc)));
            // Streets near the postcode, when neither the town nor the street's own postcodes
            // place it ("Köpmangatan 26, 895 30 Anundsjö": Anundsjö is a parish).
            if let Some(Hit::Postcode { x, y, .. }) = self.postcodes(&pc, cc).first() {
                near = Some((*x, *y));
            }
        }
        if city.is_none() && postcode.is_none() {
            scopes.push(None);
        }
        // last: same-named streets within about 5 km of the postcode, filtered by distance
        // rather than a range query, which would scan every document's coordinates.
        // ponytail: only the 1000 most important same-named streets are checked, so a name
        // shared by more ("Main Street") can miss; a geo-cell term field would lift that.
        let near_scope = near.is_some();
        if near_scope {
            scopes.push(None);
        }
        let scope_count = scopes.len();
        // Streets whose addresses name the requested city as their postal city.
        let postal_city: std::collections::HashSet<DocAddress> = match city {
            Some(c) if !self.musts(self.f.city, c).is_empty() => {
                let mut cl = base();
                cl.push(must(Box::new(BooleanQuery::new(
                    self.musts(self.f.city, c),
                ))));
                self.top_of(Kind::Street, cc, cl, 1000)
                    .into_iter()
                    .collect()
            }
            _ => Default::default(),
        };
        let number = number.map(schema::number_key);
        // Streets that hold the requested postcode report it, as Nominatim reports an address's.
        let with_postcode: std::collections::HashSet<DocAddress> =
            match postcode.map(pc_norm).filter(|p| !p.is_empty()) {
                Some(pc) => {
                    let mut cl = base();
                    cl.push(must(term(self.f.pcode, &pc)));
                    self.top_of(Kind::Street, cc, cl, 1000)
                        .into_iter()
                        .collect()
                }
                None => Default::default(),
            };
        let postcode_places = postcode
            .map(|pc| self.postcodes(pc, cc))
            .unwrap_or_default();
        let near_postcode = match postcode_places.first() {
            Some(Hit::Postcode { x, y, .. }) => Some((*x, *y)),
            _ => None,
        };
        let exact_road = fold(road);
        for (i, scope) in scopes.into_iter().enumerate() {
            let mut cl = base();
            if let Some(s) = scope {
                cl.push(must(s));
            }
            let mut top = match near {
                Some((px, py)) if near_scope && i + 1 == scope_count => self
                    .top_of(Kind::Street, cc, cl, 1000)
                    .into_iter()
                    .filter(|&a| {
                        let (x, y) = (self.fast_f64(a, "x"), self.fast_f64(a, "y"));
                        ((x - px) * py.to_radians().cos()).hypot(y - py) < 0.05
                    })
                    .take(STREET_CANDIDATES)
                    .collect(),
                // ponytail: exact native names are filtered below; 1,000 segments cap the scan
                // until an indexed exact-name field can replace it.
                _ => self.top_of(
                    Kind::Street,
                    cc,
                    cl,
                    if exact.is_some() {
                        1000
                    } else {
                        STREET_CANDIDATES
                    },
                ),
            };
            if !long.is_empty() || exact.is_some() {
                top.retain(|&a| {
                    use tantivy::schema::Value as _;
                    let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                    names_hold(doc.get_all(self.f.name).filter_map(|v| v.as_str()), &long)
                        && exact.as_ref().is_none_or(|road| {
                            doc.get_all(self.f.name)
                                .filter_map(|v| v.as_str())
                                .any(|name| fold(name) == *road)
                        })
                });
            }
            if prefer_exact {
                let named: Vec<_> = top
                    .iter()
                    .copied()
                    .filter(|&a| {
                        use tantivy::schema::Value as _;
                        let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                        doc.get_all(self.f.name)
                            .filter_map(|v| v.as_str())
                            .any(|name| fold(name) == exact_road)
                    })
                    .collect();
                // An exact road without the number must not borrow a house from a longer name.
                if !named.is_empty() {
                    top = named;
                }
            }
            if top.is_empty() {
                continue;
            }
            // Streets holding the postcode first, then those nearest its centre: a municipality
            // can have two Fjällvägen, and the postcode tells them apart.
            let dist = |a: &DocAddress| {
                near_postcode.map_or(0, |(px, py)| {
                    let (x, y) = (self.fast_f64(*a, "x"), self.fast_f64(*a, "y"));
                    (((x - px) * py.to_radians().cos()).hypot(y - py) * 1e4) as u64
                })
            };
            // and those in the place named rather than the town around it (Port Chalmers' Beach
            // Street, not central Dunedin's)
            let in_named = |a: &DocAddress| {
                let place = self.fast(*a, "place") as u32;
                place != NONE && self.chain(place).iter().any(|p| named.contains(p))
            };
            top.sort_by_cached_key(|a| (!with_postcode.contains(a), !in_named(a), dist(a)));
            // `own`: the house's postcode slot (1-based, into the street's stored list)
            let street = |a: DocAddress, number: Option<String>, (x, y): (f64, f64), own: u8| {
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                let name = best_name(&doc, self.f.name, road);
                let km =
                    |(px, py): (f64, f64)| ((px - x) * py.to_radians().cos()).hypot(py - y) * 111.0;
                let own = (own > 0)
                    .then(|| {
                        use tantivy::schema::Value as _;
                        doc.get_all(self.f.pcode)
                            .nth(own as usize - 1)
                            .and_then(|v| v.as_str())
                            .map(|pc| self.postcode_display(pc, cc))
                    })
                    .flatten();
                let postal_town = number
                    .as_deref()
                    .and_then(|n| postal_town(&doc, self.f.postal_town, n, own.as_deref()));
                // Sources disagree on some houses' postcodes (one OSM object says 112 21, another
                // 111 52); the requested one stands when the street holds it and it is centred
                // within 1 km of the house.
                let own = own.filter(|o| {
                    postcode.is_none_or(|p| {
                        pc_norm(o) == pc_norm(p)
                            || !(with_postcode.contains(&a)
                                && near_postcode.is_some_and(|c| km(c) < 1.0))
                    })
                });
                // Nominatim gives an address without a postcode its nearest postcode's; here, the
                // requested one when the street holds it or its centre is within 3 km.
                let postcode = own.or_else(|| {
                    postcode
                        .filter(|_| {
                            with_postcode.contains(&a) || near_postcode.is_some_and(|p| km(p) < 3.0)
                        })
                        .map(|p| p.trim().to_string())
                });
                let place = self.fast(a, "place") as u32;
                let named = |c: &str| {
                    let want = name_key(c);
                    place != NONE
                        && self
                            .chain(place)
                            .iter()
                            .any(|&p| self.places[p as usize].named(want))
                };
                let city = city
                    .filter(|c| {
                        use tantivy::schema::Value as _;
                        postal_city.contains(&a)
                            && !named(c)
                            && (doc.get_first(self.f.postal_town).is_none()
                                || doc.get_all(self.f.postal_town).any(|value| {
                                    value
                                        .as_str()
                                        .and_then(|v| {
                                            serde_json::from_str::<(&str, &str, &str, &str)>(v).ok()
                                        })
                                        .is_some_and(|(_, _, name, _)| {
                                            let name = fold(name);
                                            fold(c)
                                                .split(' ')
                                                .all(|word| name.split(' ').any(|w| w == word))
                                        })
                                }))
                    })
                    .map(title);
                Hit::Street {
                    doc: a,
                    sid: self.fast(a, "id") as u32,
                    place,
                    name,
                    x,
                    y,
                    number,
                    postcode,
                    city,
                    postal_town,
                    cc: cc.unwrap_or_default().to_string(),
                }
            };
            let own_postcode = |a: DocAddress, own: u8| {
                use tantivy::schema::Value as _;
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
                (own > 0)
                    .then(|| {
                        doc.get_all(self.f.pcode)
                            .nth(own as usize - 1)?
                            .as_str()
                            .map(pc_norm)
                    })
                    .flatten()
            };
            let lookup = |a: DocAddress, n: &str| {
                let sid = self.fast(a, "id") as u32;
                let mut key = sid.to_be_bytes().to_vec();
                key.extend_from_slice(n.as_bytes());
                self.numbers[sid as usize % self.numbers.len()].get(&key)
            };
            if city.is_none() && postcode.is_none() && cc.is_none() {
                // Nothing places the street: answer only when few same-named streets (with the
                // number) exist, all in one country ("oosterschelde 3", not "66 parks").
                let n = number.as_deref().filter(|n| !n.is_empty());
                let fits: Vec<DocAddress> = top
                    .iter()
                    .copied()
                    .filter(|&a| n.is_none_or(|n| lookup(a, n).is_some()))
                    .collect();
                let countries: std::collections::HashSet<&str> = fits
                    .iter()
                    .map(|&a| self.fast(a, "place") as u32)
                    .map(|p| {
                        if p == NONE {
                            ""
                        } else {
                            &*self.places[p as usize].cc
                        }
                    })
                    .collect();
                if fits.is_empty()
                    || fits.len() > 3
                    || countries.len() != 1
                    || countries.contains("")
                {
                    return vec![];
                }
            }
            if let Some(n) = number.as_deref().filter(|n| !n.is_empty()) {
                let mut key = vec![];
                let mut conflicting = false;
                // Of the streets holding the house, those named exactly as written first (Jaszcz,
                // not Wybudowanie pod Jaszcz nearer the postcode's centre).
                let folded = fold(road);
                let as_written = |a: &DocAddress| {
                    use tantivy::schema::Value as _;
                    self.searcher
                        .doc::<tantivy::TantivyDocument>(*a)
                        .is_ok_and(|doc| {
                            doc.get_all(self.f.name)
                                .filter_map(|v| v.as_str())
                                .any(|name| fold(name) == folded)
                        })
                };
                let mut by_name: Vec<DocAddress> = top
                    .iter()
                    .copied()
                    .filter(|&a| lookup(a, n).is_some())
                    .collect();
                if by_name.len() > 1 {
                    by_name.sort_by_cached_key(|a| {
                        (!with_postcode.contains(a), !in_named(a), !as_written(a))
                    });
                }
                for &a in &by_name {
                    let sid = self.fast(a, "id") as u32;
                    key.clear();
                    key.extend_from_slice(&sid.to_be_bytes());
                    key.extend_from_slice(n.as_bytes());
                    if let Some(v) = self.numbers[sid as usize % self.numbers.len()].get(&key) {
                        let (x, y, own) = schema::unpack(v);
                        // The town and the postcode disagree: when the house in the town has
                        // another postcode and the requested postcode holds the same street and
                        // number, the postcode is taken to be right and the town wrong.
                        let wanted = postcode.map(pc_norm).filter(|p| !p.is_empty());
                        if i < town_scopes
                            && let Some(wanted) = &wanted
                            && own_postcode(a, own).is_some_and(|o| o != *wanted)
                            && let Some((b, (x, y, own))) = with_postcode
                                .iter()
                                .filter_map(|&b| Some((b, schema::unpack(lookup(b, n)?))))
                                .filter(|&(b, (_, _, own))| {
                                    own_postcode(b, own).as_ref() == Some(wanted)
                                })
                                .min_by_key(|(b, _)| (dist(b), self.fast(*b, "id")))
                        {
                            return vec![street(b, Some(n.to_string()), (x, y), own)];
                        }
                        let found = street(a, Some(n.to_string()), (x, y), own);
                        // Raw house postcodes can disagree with the nearby-source corroboration above.
                        if with_postcode.contains(&a)
                            && wanted.as_ref().is_some_and(|wanted| match &found {
                                Hit::Street {
                                    postcode: Some(actual),
                                    ..
                                } => !crate::matching::postcodes_compatible(wanted, actual),
                                _ => false,
                            })
                            && postcode_places.iter().any(|hit| match hit {
                                Hit::Postcode { place, .. } if *place != NONE => {
                                    let street_place = self.fast(a, "place") as u32;
                                    street_place != NONE
                                        && matches!(
                                            self.places[*place as usize].subtype.as_ref(),
                                            "locality" | "localadmin"
                                        )
                                        && (self.chain(street_place).contains(place)
                                            || (matches!(
                                                self.places[street_place as usize].subtype.as_ref(),
                                                "locality" | "localadmin"
                                            ) && self.chain(*place).contains(&street_place)))
                                }
                                _ => false,
                            })
                        {
                            conflicting = true;
                            continue;
                        }
                        // like Nominatim, a found house is the only answer
                        return vec![found];
                    }
                }
                if conflicting {
                    return vec![];
                }
            }
            // A street crossing a cell edge is indexed in parts: answer the middle of the parts
            // near the first, not one part's.
            let a = top[0];
            let xy = |b: DocAddress| (self.fast_f64(b, "x"), self.fast_f64(b, "y"));
            let name = |b: DocAddress| {
                let doc: tantivy::TantivyDocument = self.searcher.doc(b).unwrap();
                stored(&doc, self.f.name)
            };
            let ((ax, ay), place, first) = (xy(a), self.fast(a, "place"), name(a));
            let parts: Vec<(f64, f64)> = top
                .iter()
                .copied()
                .filter(|&b| {
                    let (bx, by) = xy(b);
                    let km = ((bx - ax) * ay.to_radians().cos()).hypot(by - ay) * 111.0;
                    km < 3.0 && self.fast(b, "place") == place && name(b) == first
                })
                .map(xy)
                .collect();
            let n = parts.len() as f64;
            let mid = parts
                .iter()
                .fold((0.0, 0.0), |(x, y), p| (x + p.0 / n, y + p.1 / n));
            return vec![street(a, None, mid, 0)];
        }
        vec![]
    }

    fn postcodes(&self, pc: &str, cc: Option<&str>) -> Vec<Hit> {
        let pc = pc_norm(pc);
        if pc.is_empty() {
            return vec![];
        }
        self.top_of(Kind::Postcode, cc, vec![must(term(self.f.pcode, &pc))], 5)
            .into_iter()
            .map(|a| {
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                let get = |f: Field| stored(&doc, f);
                Hit::Postcode {
                    display: get(self.f.name),
                    cc: String::new(),
                    place: self.fast(a, "place") as u32,
                    x: self.fast_f64(a, "x"),
                    y: self.fast_f64(a, "y"),
                }
            })
            .collect()
    }

    /// A normalized postcode as the data writes it ("SW1A2AA" -> "SW1A 2AA").
    fn postcode_display(&self, norm: &str, cc: Option<&str>) -> String {
        let cl = vec![must(term(self.f.pcode, norm))];
        match self.top_of(Kind::Postcode, cc, cl, 1).first() {
            Some(&a) => stored(&self.searcher.doc(a).unwrap(), self.f.name),
            None => norm.to_string(),
        }
    }

    /// `text`, or its most specific part, if it names a place: words dropped from the front
    /// ("fra seynod" -> "seynod"), or for CJK the administrative parts ("南京市江宁区" ->
    /// "江宁区", "南京市").
    fn resolve_place(&self, text: &str, cc: Option<&str>) -> Option<String> {
        let local = |p: &Place| !matches!(&*p.subtype, "country" | "dependency");
        let mut tries: Vec<String> = vec![text.to_string()];
        let words: Vec<&str> = text.split_whitespace().collect();
        tries.extend((1..words.len()).map(|i| words[i..].join(" ")));
        let mut part = String::new();
        let mut parts = vec![];
        for c in text.chars() {
            part.push(c);
            if "市区县省都道府県町村".contains(c) {
                parts.push(std::mem::take(&mut part));
            }
        }
        if parts.len() > 1 {
            tries.extend(parts.into_iter().rev());
        }
        tries
            .into_iter()
            .find(|t| !self.places(t, cc, None, &local).is_empty())
    }

    /// Whether a place named exactly `name` (in country `cc`, if given) has its centre within
    /// `km` of the point.
    /// "Maplewood Township" may be named "Maplewood".
    pub fn place_near(&self, name: &str, cc: Option<&str>, lat: f32, lon: f32, km: f32) -> bool {
        std::iter::once(name.to_string())
            .chain(without_admin_word(name))
            .any(|name| {
                let want = name_key(&name);
                let local =
                    |p: &Place| !matches!(&*p.subtype, "country" | "dependency") && p.named(want);
                self.places(&name, cc, None, &local)
                    .iter()
                    .any(|h| match h {
                        Hit::Place(id) => {
                            let p = &self.places[*id as usize];
                            let dx = (p.x - lon) * lat.to_radians().cos();
                            dx.hypot(p.y - lat) * 111.2 < km
                        }
                        _ => false,
                    })
            })
    }

    /// The postcode's own places, as hits to render.
    pub fn postcode_hits(&self, pc: &str, cc: Option<&str>) -> Vec<Hit> {
        self.postcodes(pc, cc)
    }

    /// Whether the data holds this postcode (in country `cc`, if given).
    pub fn knows_postcode(&self, pc: &str, cc: Option<&str>) -> bool {
        self.postcode_exists(pc, cc)
    }

    /// Whether `text`, or its last words, names a place in the data (in country `cc`, if given).
    pub fn names_place(&self, text: &str, cc: Option<&str>) -> bool {
        self.resolve_place(text, cc).is_some()
    }

    fn postcode_exists(&self, pc: &str, cc: Option<&str>) -> bool {
        let cl = vec![must(term(self.f.pcode, &pc_norm(pc)))];
        self.any_of(Kind::Postcode, cc, cl)
    }

    /// A one- or two-word postcode within a comma-separated part of `q`, and `q` without it.
    /// Without a known country a single word must mix letters and digits or have five or more
    /// characters, so house numbers aren't read as some country's postcode.
    fn find_postcode(&self, q: &str, cc: Option<&str>) -> Option<(String, String)> {
        let parts: Vec<&str> = q.split(',').collect();
        for len in [2, 1] {
            for (pi, part) in parts.iter().enumerate() {
                let words: Vec<&str> = part.split_whitespace().collect();
                for start in 0..(words.len() + 1).saturating_sub(len) {
                    let cand = words[start..start + len].join(" ");
                    let norm = pc_norm(&cand);
                    let digits = norm.chars().any(|c| c.is_ascii_digit());
                    let letters = norm.chars().any(|c| c.is_alphabetic());
                    if !digits
                        || norm.len() < 3
                        || (len == 1 && cc.is_none() && !letters && norm.len() < 5)
                    {
                        continue;
                    }
                    if self.postcode_exists(&cand, cc) {
                        let mut rest_words = words.clone();
                        rest_words.drain(start..start + len);
                        let mut rest: Vec<String> =
                            parts.iter().map(|p| p.trim().to_string()).collect();
                        rest[pi] = rest_words.join(" ");
                        let rest = rest
                            .into_iter()
                            .filter(|p| !p.is_empty())
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Some((cand, rest));
                    }
                }
            }
        }
        None
    }

    /// The English name of a country's place.
    pub fn country_name(&self, cc: &str) -> Option<&str> {
        let place = self.country_places.get(cc)?;
        Some(self.places[*place as usize].name(true))
    }

    /// Every ISO 3166-2 code that names a region place or a subdivision below one, with that
    /// place.
    pub fn region_places(&self) -> impl Iterator<Item = (&str, u32)> {
        self.regions
            .iter()
            .chain(&self.subdivisions)
            .chain(&self.provinces)
            .map(|(code, &place)| (code.as_str(), place))
    }

    /// The code of the region a subdivision's place lies in.
    pub fn parent_region_code(&self, place: u32) -> Option<&str> {
        self.region_code(self.chain(place).get(1..)?)
    }

    /// Whether every street named `road` in place `within` that holds house `number` holds it
    /// within about a kilometre of the others: one house, perhaps on several segments.
    fn house_unique_in(&self, road: &str, number: &str, within: u32) -> bool {
        const MAX: usize = 1000;
        let mut cl = self.musts(self.f.names, road);
        if cl.is_empty() {
            return false;
        }
        cl.push(must(term(self.f.ids, &within.to_string())));
        let top = self.top_of(Kind::Street, None, cl, MAX);
        let number = schema::number_key(number);
        let houses: Vec<(f64, f64)> = top
            .iter()
            .filter(|&&a| {
                let sid = self.fast(a, "id") as u32;
                let mut key = sid.to_be_bytes().to_vec();
                key.extend_from_slice(number.as_bytes());
                self.numbers[sid as usize % self.numbers.len()]
                    .get(&key)
                    .is_some()
            })
            .map(|&a| (self.fast_f64(a, "x"), self.fast_f64(a, "y")))
            .collect();
        top.len() < MAX
            && houses
                .first()
                .is_some_and(|&(x0, y0)| houses.iter().all(|&(x, y)| (x - x0).hypot(y - y0) < 0.01))
    }

    /// A US state's region place, by its postal code ("OH") or its name.
    fn us_state(&self, state: &str) -> Option<u32> {
        let state = state.trim();
        if state.len() == 2 {
            return self
                .regions
                .get(&format!("US-{}", state.to_uppercase()))
                .copied();
        }
        let want = name_key(state);
        self.places(state, Some("us"), None, &|p| {
            &*p.subtype == "region" && p.named(want)
        })
        .into_iter()
        .find_map(|h| match h {
            Hit::Place(id) => Some(id),
            _ => None,
        })
    }

    /// `query` with a part that is a region's ISO 3166-2 code ("B.C.S" for Baja California Sur)
    /// replaced by the region's name; see [`crate::matching::with_region_names`].
    pub fn with_region_names(&self, query: &str) -> Option<String> {
        crate::matching::with_region_names(query, |code| {
            if self.country(code).is_some() {
                return None;
            }
            let suffix = format!("-{code}");
            let mut named = self.regions.iter().filter(|(k, _)| k.ends_with(&suffix));
            match (named.next(), named.next()) {
                (Some((_, &id)), None) => Some(self.places[id as usize].primary.to_string()),
                _ => None,
            }
        })
    }

    /// A country/subdivision/name slug. Some(empty) keeps a conflicting code or name from
    /// falling through to a less constrained interpretation.
    pub fn place_slug(&self, query: &str) -> Option<Vec<Hit>> {
        let mut parts = query.trim().splitn(3, '-');
        let (cc, division, town) = (parts.next()?, parts.next()?, parts.next()?);
        if cc.len() != 2
            || !cc.bytes().all(|c| c.is_ascii_alphabetic())
            || !(1..=3).contains(&division.len())
            || !division.bytes().all(|c| c.is_ascii_alphanumeric())
            || town.is_empty()
            || town.split('-').any(|word| word.is_empty())
            || !town.chars().all(|c| c.is_alphabetic() || c == '-')
        {
            return None;
        }
        let cc = self.country(cc)?;
        let code = format!("{}-{}", cc.to_uppercase(), division.to_uppercase());
        let Some(region) = self.region_query(&format!("{cc}, {code}")) else {
            return Some(vec![]);
        };
        let town = town.replace('-', " ");
        let key = name_key(&town);
        Some(
            self.places_named(
                &town,
                Some(&cc),
                Some(self.places[region as usize].name(true)),
                &|p| matches!(p.subtype.as_ref(), "locality" | "localadmin") && p.named(key),
                false,
            )
            .0
            .into_iter()
            .filter(|hit| matches!(hit, Hit::Place(id) if self.chain(*id).contains(&region)))
            .collect(),
        )
    }

    /// Reviewed membership observations, only after current lookups have abstained.
    pub fn historical_place(&self, query: &str) -> Option<Hit> {
        if query.chars().count() > 256 {
            return None;
        }
        let parts: Vec<_> = query.split(',').map(str::trim).collect();
        let [town, ancestor, country] = parts.as_slice() else {
            return None;
        };
        let cc = self.country(country)?;
        if self.country(ancestor).is_some() || self.country(town).is_some_and(|other| other != cc) {
            return None;
        }
        self.history
            .find(&self.places, town, ancestor, &cc)
            .map(Hit::Place)
    }

    /// `<cc>, <ISO 3166-2 code>`, as a query for a region by its code.
    fn region_query(&self, q: &str) -> Option<u32> {
        let (cc, code) = q.split_once(", ")?;
        let code = code.trim().to_uppercase();
        (cc.trim().len() == 2 && code.starts_with(&format!("{}-", cc.trim().to_uppercase())))
            .then(|| {
                let found = self.regions.get(&code).or(self.subdivisions.get(&code));
                found.or(self.provinces.get(&code)).copied()
            })
            .flatten()
    }

    fn chinese_streets(
        &self,
        units: &[String],
        road: &str,
        number: &str,
        city: Option<&str>,
        cc: Option<&str>,
    ) -> Vec<Hit> {
        let towns = units.iter().rev().map(String::as_str).chain(city);
        let mut street = None;
        for town in towns {
            let hits: Vec<_> = self
                .streets(road, Some(number), Some(town), None, cc, false)
                .into_iter()
                .filter(|hit| {
                    let Hit::Street { place, .. } = hit else {
                        return false;
                    };
                    let chain = self.chain(*place);
                    units.iter().all(|unit| {
                        chain
                            .iter()
                            .any(|&p| self.places[p as usize].named(name_key(unit)))
                    })
                })
                .collect();
            if let Some(h) = hits.iter().find(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            }) {
                return vec![h.clone()];
            }
            if street.is_none() {
                street = hits.into_iter().next();
            }
        }
        street.into_iter().collect()
    }

    /// Candidates for a free-text query, best first; at most ten. Blocks: call it off the
    /// async executor.
    pub fn search(&self, q: &str) -> Vec<Hit> {
        let q = q.trim();
        if q.is_empty() {
            return vec![];
        }
        if let Some(hits) = self.korean_search(q)
            && !hits.is_empty()
        {
            return hits;
        }
        // Japanese block addresses: libpostal splits them, and the data names the chōme
        // ("芝公園四丁目", number "2-8"), so they're read here.
        let mut chome = None;
        for (city, street, number) in japanese(q) {
            let city = self.resolve_place(&city, Some("jp")).unwrap_or(city);
            let hits = self.streets(&street, Some(&number), Some(&city), None, Some("jp"), false);
            let housed = |h: &Hit| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            };
            if let Some(h) = hits.iter().find(|h| housed(h)) {
                return vec![h.clone()];
            }
            if chome.is_none() && street.ends_with("丁目") {
                chome = hits.into_iter().next();
            }
        }
        // the block's chōme when the house isn't in the data
        if let Some(h) = chome {
            return vec![h];
        }
        if let Some(p) = self.region_query(q) {
            return vec![Hit::Place(p)];
        }
        // "Birmingham, Birmingham, United Kingdom, United Kingdom": repeated parts parse as one
        // doubled value ("united kingdom united kingdom") that names nothing.
        let mut seen = std::collections::HashSet::new();
        let deduped: Vec<&str> = q
            .split(',')
            .map(str::trim)
            .filter(|p| seen.insert(p.to_lowercase()))
            .collect();
        let repeated = deduped.len() < q.split(',').count();
        let deduped = deduped.join(", ");
        let q = deduped.as_str();
        let parsed = self.parse(q);
        if let Ok(Some((_, hit))) = self.nz_comma_house(q, &parsed) {
            return vec![hit];
        }
        let get = |k: &str| parsed.iter().find(|(l, _)| l == k).map(|(_, v)| v.as_str());
        let country_text = get("country");
        let cc = country_text.and_then(|c| self.country(c));
        let cc = cc.as_deref();
        let city = get("city")
            .or(get("city_district"))
            .or(get("suburb"))
            .or(get("island"));
        let state = get("state")
            .or(get("state_district"))
            .or(get("country_region"));
        let local = |p: &Place| !matches!(&*p.subtype, "country" | "dependency");
        // A query that only names a country ("Malta", "North Macedonia") means the country,
        // unless it was repeated and the country has a town of that name ("Luxembourg,
        // Luxembourg", "Djibouti, Djibouti").
        if let Some(c) = self.country(q)
            && let Some(&p) = self.country_places.get(&c)
        {
            let town = |p: &Place| matches!(&*p.subtype, "locality" | "localadmin");
            if repeated && let (places, true) = self.places_exact(q, Some(&c), None, &town) {
                return places.into_iter().take(1).collect();
            }
            return vec![Hit::Place(p)];
        }
        // Chinese road addresses ("上海市虹口区中山北一路295号"): libpostal splits the road
        // from its number; read them here, scoped by the written district or city.
        if let Some((units, road, number)) = chinese(q) {
            let hits = self.chinese_streets(&units, &road, &number, city, cc);
            if !hits.is_empty() {
                return hits;
            }
        }
        let mut hits = vec![];
        let mut roads: Vec<&str> = parsed
            .iter()
            .filter(|(l, _)| l == "road")
            .map(|(_, v)| v.as_str())
            .collect();
        // The street is the road written next to the house number ("Pte.Roca 731, Rosario"
        // gives roads "pte.roca" and "rosario"), else the last road.
        if let Some(at) = parsed.iter().position(|(l, _)| l == "house_number") {
            let next_to = [at.checked_sub(1), Some(at + 1)]
                .into_iter()
                .flatten()
                .find(|&i| parsed.get(i).is_some_and(|(l, _)| l == "road"));
            if let Some(i) = next_to {
                let street = parsed[i].1.as_str();
                roads.retain(|r| *r != street);
                roads.push(street);
            }
        }
        if let Some(road) = roads.last().map(|r| without_number_mark(r)) {
            let exact = cc == Some("nz")
                && matches!(q.split(',').count(), 3 | 4)
                && q.rsplit(',')
                    .next()
                    .and_then(|c| self.country(c))
                    .as_deref()
                    == Some("nz")
                && self
                    .parser
                    .parse(q.split(',').next().unwrap())
                    .is_ok_and(|head| {
                        head_aligns(&head)
                            && one_house(&head)
                            && ["road", "house_number", "unit"]
                                .iter()
                                .all(|key| field(&head, key) == field(&parsed, key))
                    });
            // libpostal often puts the town in another part: "zürich - birchstrasse 160" gives
            // house "zürich", "fra - seynod - 26 rue de sansy" a first road "fra seynod". The
            // street is the last road; the town, if none is labelled, the first other part that
            // names a place.
            let resolved_city = match city {
                Some(c) => Some(self.resolve_place(c, cc).unwrap_or_else(|| c.to_string())),
                None => get("house")
                    .into_iter()
                    .chain(roads[..roads.len() - 1].iter().copied())
                    .find_map(|t| self.resolve_place(t, cc)),
            };
            hits = self.streets(
                road,
                get("house_number").map(house_number),
                resolved_city.as_deref(),
                get("postcode"),
                cc,
                exact,
            );
            // "70 N Professor St, Oberlin 44074" parses one road "n professor st oberlin": with
            // no town labelled, the road's last words may be it.
            if hits.is_empty() && city.is_none() && (cc.is_some() || get("postcode").is_some()) {
                let words: Vec<&str> = road.split_whitespace().collect();
                // a street-type word ("road", "st") or a short one is no town
                let kind = |w: &str| {
                    let f = fold(w);
                    f.len() < 3 || canon::canon(&f).is_some() || canon::is_canonical(&f)
                };
                for k in 1..words.len().min(3) {
                    let (street, town) = words.split_at(words.len() - k);
                    if town.iter().any(|w| kind(w)) || street.iter().all(|w| fold(w).len() < 3) {
                        continue;
                    }
                    let town = town.join(" ");
                    if let Some(town) = self.resolve_place(&town, cc) {
                        hits = self.streets(
                            &street.join(" "),
                            get("house_number").map(house_number),
                            Some(&town),
                            get("postcode"),
                            cc,
                            false,
                        );
                        if !hits.is_empty() {
                            break;
                        }
                    }
                }
            }
        }
        // "1516 Ruth Avenue, OH": a house with only its US state to place it, answered when the
        // state holds that number on one street of that name.
        if hits.is_empty()
            && city.is_none()
            && get("postcode").is_none()
            && cc.is_none_or(|c| c == "us")
            && let (Some(road), Some(n), Some(state)) =
                (roads.last(), get("house_number").map(house_number), state)
            && let Some(region) = self.us_state(state)
            && self.house_unique_in(without_number_mark(road), n, region)
        {
            let name = self.places[region as usize].primary.to_string();
            let found: Vec<Hit> = self
                .streets_by(
                    without_number_mark(road),
                    Some(n),
                    Some(&name),
                    &[region],
                    None,
                    Some("us"),
                    false,
                )
                .into_iter()
                .filter(|h| {
                    matches!(
                        h,
                        Hit::Street {
                            number: Some(_),
                            ..
                        }
                    )
                })
                .collect();
            hits = found.into_iter().take(1).collect();
        }
        // "Santiago, Metropolitana, Chile" parses "santiago" as a road: without a house number,
        // an unmatched road is tried as a place name too.
        // Village addresses ("Libá 156, 351 31 Libá"): the number belongs to the village,
        // indexed as a street of that name. Only an exact house answers.
        if roads.is_empty()
            && let (Some(n), Some(town)) = (get("house_number").map(house_number), city)
        {
            let found = self.streets(town, Some(n), Some(town), get("postcode"), cc, false);
            if let Some(h) = found.into_iter().find(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            }) {
                return vec![h];
            }
        }
        // libpostal reads a numbered street or village with no street type as a venue ("Świtały
        // 12, 76-231" and "św. Jana 10, 36-050 Sokołów" give a house and no road): only an exact
        // house on a street of that name answers. So with a number it leaves in the road
        // ("Rakowice Małe 20BS, 59-600").
        let venue = roads
            .is_empty()
            .then(|| get("house").zip(get("house_number").map(house_number)))
            .flatten();
        let in_road = (hits.is_empty() && get("house_number").is_none())
            .then(|| roads.last().and_then(|r| r.rsplit_once(' ')))
            .flatten()
            .filter(|(_, n)| n.starts_with(|c: char| c.is_ascii_digit()));
        if let Some((name, n)) = venue.or(in_road)
            && (city.is_some() || get("postcode").is_some())
        {
            let town = city.map(|c| self.resolve_place(c, cc).unwrap_or_else(|| c.to_string()));
            let found = self.streets(name, Some(n), town.as_deref(), get("postcode"), cc, false);
            if let Some(h) = found.into_iter().find(|h| {
                matches!(
                    h,
                    Hit::Street {
                        number: Some(_),
                        ..
                    }
                )
            }) {
                return vec![h];
            }
        }
        let road_as_place = hits.is_empty() && get("house_number").is_none();
        if get("road").is_none() || road_as_place {
            // libpostal's labels for place-only text are unreliable ("Doha, Ad Dawhah, Qatar"
            // gives house "doha", city "ad dawhah"): try each place-like part in turn.
            const PLACE_LABELS: [&str; 8] = [
                "city",
                "city_district",
                "suburb",
                "island",
                "house",
                "state_district",
                "state",
                "country_region",
            ];
            // in the order written: the first part is usually the most specific
            let texts: Vec<&str> = parsed
                .iter()
                .filter(|(l, _)| {
                    PLACE_LABELS.contains(&l.as_str()) || (road_as_place && l == "road")
                })
                .map(|(_, v)| v.as_str())
                // The country's own text, or another of its names, unless that also names a place
                // there: Finnish calls Brazil "Brasilia", and some languages Tunisia "Tunis".
                .filter(|t| {
                    !(Some(*t) == country_text
                        || (cc.is_some()
                            && self.country(t).as_deref() == cc
                            && !self.places_exact(t, cc, None, &local).1))
                })
                .collect();
            if let Some(pc) = get("postcode") {
                let found = self.postcodes(pc, cc);
                let countries: std::collections::HashSet<&str> = found
                    .iter()
                    .filter_map(|h| match h {
                        Hit::Postcode { place, .. } if *place != NONE => {
                            Some(&*self.places[*place as usize].cc)
                        }
                        _ => None,
                    })
                    .collect();
                // A bare postcode that several countries use is a guess without a country.
                if cc.is_some() || !texts.is_empty() || countries.len() <= 1 {
                    hits = found;
                }
            }
            let mut matched = false;
            // all parts together first: "région bruxelles-capitale" is one name, and "région"
            // alone matches every region
            let joined = texts.join(" ");
            // ...but not with a code ("Stratford, CT" is not "Stratford Court")
            let tries = (texts.len() > 1 && texts.iter().all(|t| t.trim().len() > 3))
                .then_some(joined.as_str())
                .into_iter()
                .chain(texts.iter().copied());
            // the first exact match, else the first match
            let mut best: Option<Vec<Hit>> = None;
            for text in tries {
                let within = state.filter(|s| *s != text);
                let (places, exact) = self.places_exact(text, cc, within, &local);
                if exact {
                    best = Some(places);
                    break;
                }
                // Without a country, a partial name ("beach gardens" in Palm Beach Gardens) is a
                // guess anywhere in the world.
                if best.is_none() && !places.is_empty() && cc.is_some() {
                    best = Some(places);
                }
            }
            if let Some(places) = best {
                // a postcode elsewhere ("00871" is Warsaw's; Lakeport is American) doesn't
                // belong to this place
                let place_cc = |h: &Hit| match h {
                    Hit::Place(p) | Hit::Postcode { place: p, .. } if *p != NONE => {
                        Some(self.places[*p as usize].cc.clone())
                    }
                    _ => None,
                };
                let found: std::collections::HashSet<_> =
                    places.iter().filter_map(place_cc).collect();
                hits.retain(|h| place_cc(h).is_some_and(|c| found.contains(&c)));
                hits.extend(places);
                matched = true;
            }
            if texts.is_empty()
                && hits.is_empty()
                && let Some(text) = country_text
            {
                hits.extend(self.places(text, None, None, &|p| {
                    matches!(&*p.subtype, "country" | "dependency")
                }));
            }
            if !matched && !texts.is_empty() {
                // Like Nominatim, every stated part has to match: "Qwertyuiop 12345" is not
                // postcode 12345.
                hits.clear();
            }
            if !matched
                && !texts.is_empty()
                && get("house_number").is_none()
                && get("postcode").is_none()
            {
                // "Dubai, UAE" parses as one "house": the first comma part as a place, in the
                // last part's country if it names one.
                let parts: Vec<&str> = q
                    .split(',')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .collect();
                let last_cc = (parts.len() > 1)
                    .then(|| self.country(parts[parts.len() - 1]))
                    .flatten();
                if let (Some(c), Some(first)) = (&last_cc, parts.first()) {
                    hits = self.places(first, Some(c), None, &local);
                }
            }
        }
        // "Gaza Strip, Palestine": the strip is its own country in the data (xg) and Palestine
        // is not a country place, so a search scoped to PS finds nothing. A user-assigned code
        // the reference files under the stated country is that place. "Georgia" is not one: its
        // code is a real country's, and the US state was found above.
        if hits.is_empty()
            && get("house_number").is_none()
            && roads.is_empty()
            && let Some(text) = state.or(city)
            && let Some(code) = self.country(text)
            && code.starts_with('x')
            && let Some(stated) = cc
        {
            let associated = crate::matching::implied_country(
                &[("state".to_string(), text.to_string())]
                    .into_iter()
                    .collect(),
            );
            if stated == associated
                && let Some(&p) = self.country_places.get(&code)
            {
                return vec![Hit::Place(p)];
            }
        }
        hits.truncate(10);
        hits
    }

    /// The ISO 3166-2 code of a place chain. The lowest subdivision the data names itself beats
    /// the region code Overture gives the places inside it (the district over its region).
    fn region_code(&self, chain: &[u32]) -> Option<&str> {
        let own = chain.iter().find_map(|p| self.own_codes.get(p));
        // A child's inherited region column can disagree with its explicit region parent.
        own.map(|c| &**c)
            .or_else(|| {
                chain
                    .iter()
                    .map(|&p| &self.places[p as usize])
                    .find(|p| p.subtype.as_ref() == "region" && !p.region.is_empty())
                    .map(|p| &*p.region)
            })
            .or_else(|| {
                chain
                    .iter()
                    .map(|&p| &*self.places[p as usize].region)
                    .find(|r| !r.is_empty())
            })
    }

    fn house_chain(&self, chain: &[u32], x: f64, y: f64) -> Option<Vec<u32>> {
        let region = |chain: &[u32]| {
            chain
                .iter()
                .copied()
                .find(|&p| self.places[p as usize].subtype.as_ref() == "region")
        };
        let named = region(chain)?;
        let spatial = self.chain(self.area_at(y, x).0?);
        let containing = region(&spatial)?;
        if named == containing
            || self.places[named as usize].cc != self.places[containing as usize].cc
        {
            return None;
        }
        Some(
            chain
                .iter()
                .copied()
                .take_while(|&p| {
                    !matches!(
                        self.places[p as usize].subtype.as_ref(),
                        "localadmin"
                            | "county"
                            | "region"
                            | "macroregion"
                            | "dependency"
                            | "country"
                    ) && !self.own_codes.contains_key(&p)
                })
                .chain(spatial)
                .collect(),
        )
    }

    fn address(
        &self,
        chain: &[u32],
        en: bool,
        a: &mut Map<String, Value>,
        display: &mut Vec<String>,
    ) {
        for &p in chain {
            let pl = &self.places[p as usize];
            let key = pl.key();
            if !a.contains_key(key) {
                a.insert(key.into(), pl.name(en).into());
                display.push(pl.name(en).into());
            }
            if !pl.cc.is_empty() {
                a.entry("country_code")
                    .or_insert_with(|| pl.cc.to_string().into());
            }
        }
        // Nominatim lists a dependency (Svalbard, Puerto Rico) as a state as well.
        if let Some(&dep) = chain
            .iter()
            .find(|&&p| self.places[p as usize].subtype.as_ref() == "dependency")
        {
            a.entry("state")
                .or_insert_with(|| self.places[dep as usize].name(en).into());
        }
        let sovereign = a
            .get("country_code")
            .and_then(Value::as_str)
            .and_then(sovereign)
            .and_then(|cc| Some((cc, self.country_name(cc)?)));
        if let Some((cc, name)) = sovereign {
            a.insert("country_code".into(), cc.into());
            a.insert("country".into(), name.into());
            display.push(name.into());
        }
        // The index has no PS country: the Gaza Strip and the West Bank are countries of
        // their own (xg, xw). A query for Palestine names them under PS, and the territory
        // stays a region so the answer is not only the country.
        if matches!(
            a.get("country_code").and_then(Value::as_str),
            Some("xg" | "xw")
        ) {
            let territory = a.get("country").cloned();
            a.insert("country_code".into(), "ps".into());
            a.insert("country".into(), "Palestine".into());
            if let Some(name) = territory {
                a.entry("state").or_insert(name);
            }
            display.push("Palestine".into());
        }
        if let Some(code) = self.region_code(chain) {
            if let Some(province) = irish_province(code) {
                a.insert("ISO3166-2-lvl5".into(), province.into());
            }
            a.insert("ISO3166-2-lvl4".into(), code.into());
        }
        if let Some(code) = chain.iter().find_map(|p| self.subdivision_codes.get(p)) {
            a.insert("ISO3166-2-lvl6".into(), code.to_string().into());
        }
        if let Some((code, _)) = self
            .provinces
            .iter()
            .find(|(_, p)| chain.first() == Some(*p))
        {
            a.insert("ISO3166-2-lvl5".into(), code.clone().into());
        }
    }

    /// A candidate as Nominatim-style JSON, with English names when `en`.
    pub fn render(&self, hit: &Hit, en: bool) -> Value {
        let mut a = Map::new();
        let mut display: Vec<String> = vec![];
        let (id, osm_type, class, typ, addresstype, x, y, bbox, namedetails) = match hit {
            Hit::Place(p) => {
                let pl = &self.places[*p as usize];
                let mut names = Map::new();
                names.insert("name".into(), pl.primary.to_string().into());
                namedetails(pl, &mut names);
                self.address(&self.chain(*p), en, &mut a, &mut display);
                let admin = matches!(
                    &*pl.subtype,
                    "country" | "dependency" | "region" | "macroregion" | "county" | "localadmin"
                );
                let (class, typ) = if admin {
                    ("boundary", "administrative")
                } else {
                    ("place", pl.key())
                };
                (
                    *p as u64,
                    "relation",
                    class,
                    typ,
                    pl.key(),
                    pl.x as f64,
                    pl.y as f64,
                    pl.bbox,
                    names,
                )
            }
            Hit::Street {
                sid,
                place,
                name,
                x,
                y,
                number,
                postcode,
                city,
                postal_town,
                cc,
                ..
            } => {
                if let Some(town) = postal_town {
                    a.insert("postal_town".into(), town.clone().into());
                }
                if let Some(pc) = postcode {
                    a.insert("postcode".into(), pc.clone().into());
                }
                if let Some(n) = number {
                    a.insert("house_number".into(), n.clone().into());
                    display.push(n.clone());
                }
                a.insert("road".into(), name.clone().into());
                display.push(name.clone());
                if *place != NONE {
                    let chain = self.chain(*place);
                    let chain = number
                        .as_ref()
                        .and_then(|_| self.house_chain(&chain, *x, *y))
                        .unwrap_or(chain);
                    self.address(&chain, en, &mut a, &mut display);
                } else if let Some(country) = self.country_name(cc) {
                    a.insert("country_code".into(), cc.clone().into());
                    a.insert("country".into(), country.into());
                    display.push(country.into());
                }
                if let Some(c) = city {
                    // Nominatim names the postal city and lists the smaller locality as suburb.
                    for key in ["city", "town", "village"] {
                        if let Some(v) = a.remove(key) {
                            a.entry("suburb").or_insert(v);
                        }
                    }
                    a.insert("city".into(), c.clone().into());
                }
                let id = match number {
                    Some(n) => {
                        HOUSE
                            | (*sid as u64) << 8
                                ^ n.bytes()
                                    .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64))
                                    & 0xff_ffff_ffff
                    }
                    None => STREET | *sid as u64,
                };
                let d = 0.001;
                let (class, typ, at) = if number.is_some() {
                    ("place", "house", "place")
                } else {
                    ("highway", "residential", "road")
                };
                let names = Map::from_iter([("name".to_string(), Value::from(name.clone()))]);
                (
                    id,
                    if number.is_some() { "node" } else { "way" },
                    class,
                    typ,
                    at,
                    *x,
                    *y,
                    [*x as f32 - d, *y as f32 - d, *x as f32 + d, *y as f32 + d],
                    names,
                )
            }
            Hit::Postcode {
                display: pc,
                cc,
                place,
                x,
                y,
            } => {
                a.insert("postcode".into(), pc.clone().into());
                display.push(pc.clone());
                if *place != NONE {
                    self.address(&self.chain(*place), en, &mut a, &mut display);
                }
                if !cc.is_empty() {
                    a.entry("country_code").or_insert_with(|| cc.clone().into());
                }
                let id = POSTCODE
                    | pc.bytes()
                        .fold(0u64, |h, b| h.wrapping_mul(31).wrapping_add(b as u64))
                        & 0xff_ffff_ffff;
                let d = 0.01;
                (
                    id,
                    "relation",
                    "place",
                    "postcode",
                    "postcode",
                    *x,
                    *y,
                    [*x as f32 - d, *y as f32 - d, *x as f32 + d, *y as f32 + d],
                    Map::new(),
                )
            }
        };
        let [x0, y0, x1, y1] = bbox;
        display.dedup();
        json!({
            "place_id": id,
            "osm_type": osm_type,
            "osm_id": id,
            "lat": y.to_string(),
            "lon": x.to_string(),
            "class": class,
            "type": typ,
            "addresstype": addresstype,
            "display_name": display.join(", "),
            "address": a,
            "namedetails": namedetails,
            "boundingbox": [y0.to_string(), y1.to_string(), x0.to_string(), x1.to_string()],
        })
    }

    pub(crate) fn house_locality_named(&self, hit: &Hit, name: &str) -> bool {
        matches!(
            hit,
            Hit::Street {
                number: Some(_),
                ..
            }
        ) && self.street_locality_named(hit, name)
    }

    /// A town or municipality above the street is named so.
    pub(crate) fn street_locality_named(&self, hit: &Hit, name: &str) -> bool {
        let Hit::Street { place, .. } = hit else {
            return false;
        };
        if *place == NONE || name.chars().count() <= 2 {
            return false;
        }
        let key = name_key(name);
        self.chain(*place).iter().any(|&id| {
            let place = &self.places[id as usize];
            matches!(place.subtype.as_ref(), "locality" | "localadmin") && place.named(key)
        })
    }

    pub(crate) fn house_admin_locality_named(&self, hit: &Hit, name: &str) -> bool {
        let Hit::Street {
            place,
            number: Some(_),
            ..
        } = hit
        else {
            return false;
        };
        if *place == NONE || name.chars().count() <= 2 {
            return false;
        }
        let key = name_key(name);
        self.chain(*place).iter().any(|&id| {
            let place = &self.places[id as usize];
            place.settlement_twin && place.named(key)
        })
    }

    pub(crate) fn isolated_house_without_postcode(
        &self,
        hit: &Hit,
        postcode: &str,
        country: &str,
    ) -> bool {
        use tantivy::schema::Value as _;
        let Hit::Street {
            name,
            number: Some(number),
            postcode: Some(actual),
            x,
            y,
            ..
        } = hit
        else {
            return false;
        };
        let (wanted, actual) = (pc_norm(postcode), pc_norm(actual));
        let numeric_typo = crate::matching::postcode_shape(country, &wanted) == Some(true)
            && wanted.len() == actual.len()
            && wanted
                .bytes()
                .chain(actual.bytes())
                .all(|b| b.is_ascii_digit())
            && wanted
                .bytes()
                .zip(actual.bytes())
                .filter(|(a, b)| a != b)
                .count()
                == 1;
        if country.is_empty() || !(numeric_typo || self.knows_postcode(postcode, Some(country))) {
            return false;
        }
        let terms = self.musts(self.f.names, name);
        if terms.is_empty() {
            return false;
        }
        let streets = self.top_of(Kind::Street, Some(country), terms, 1000);
        if streets.len() == 1000 {
            return false;
        }
        let mut found = false;
        for a in streets {
            let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
            if doc.get_all(self.f.pcode).any(|value| {
                value
                    .as_str()
                    .is_some_and(|pc| crate::matching::postcodes_compatible(postcode, pc))
            }) {
                return false;
            }
            let sid = self.fast(a, "id") as u32;
            let mut key = sid.to_be_bytes().to_vec();
            key.extend_from_slice(schema::number_key(number).as_bytes());
            if let Some(value) = self.numbers[sid as usize % self.numbers.len()].get(&key) {
                let (lon, lat, _) = schema::unpack(value);
                if ((lon - x) * y.to_radians().cos()).hypot(lat - y) * 111_195. > 100. {
                    return false;
                }
                found = true;
            }
        }
        found
    }

    pub fn details(&self, id: u64) -> Value {
        let Some(pl) = (id < STREET)
            .then(|| self.places.get(id as usize))
            .flatten()
        else {
            return json!({});
        };
        let mut names = Map::new();
        names.insert("name".into(), pl.primary.to_string().into());
        namedetails(pl, &mut names);
        json!({
            "place_id": id,
            "country_code": pl.cc,
            "admin_level": pl.admin_level(),
            "localname": pl.primary,
            "names": names,
            "addresstype": pl.key(),
            "centroid": {"type": "Point", "coordinates": [pl.x, pl.y]},
        })
    }

    /// The houses within `radius_m` of a point, nearest first, each with its distance in metres.
    /// Streets are looked up by the grid cells their houses occupy, then their houses by distance.
    // ponytail: rings of cells grow in every direction by the smaller cell side, so a high-latitude
    // search reads more cells than it needs, and cells past the antimeridian or poles aren't
    // searched. Upgrade: per-axis ring counts and wrapped columns.
    fn reverse_houses(&self, lat: f64, lon: f64, radius_m: f64, size: usize) -> Vec<(Value, f64)> {
        use tantivy::schema::Value as _;
        let Some(rev) = &self.reverse else {
            return vec![];
        };
        let (cx, cy) = reverse::cell_at(lon, lat);
        let cos = lat.to_radians().cos().max(0.01);
        let cell_m = reverse::CELL_LAT_M.min(reverse::CELL_LON_M * cos);
        let meters = |x: f64, y: f64| ((x - lon) * cos).hypot(y - lat) * 111_195.0;
        // (distance, street document, number, x, y, postcode slot)
        let mut found: Vec<(f64, DocAddress, String, f64, f64, u8)> = vec![];
        let (mut seen, mut docs) = (std::collections::HashSet::new(), vec![]);
        for r in 0..=(radius_m / cell_m).ceil() as i64 {
            docs.clear();
            ring(r, |dx, dy| rev.docs(cx + dx, cy + dy, &mut docs));
            for &packed in &docs {
                if !seen.insert(packed) {
                    continue;
                }
                let (segment, doc) = reverse::unpack_doc(packed);
                let a = DocAddress::new(segment, doc);
                self.each_house(self.fast(a, "id") as u32, |number, x, y, own| {
                    let m = meters(x, y);
                    if m <= radius_m {
                        let number = String::from_utf8_lossy(number).into_owned();
                        found.push((m, a, number, x, y, own));
                    }
                });
            }
            found.sort_by(|p, q| {
                p.0.total_cmp(&q.0).then_with(|| {
                    (p.1.segment_ord, p.1.doc_id, &p.2).cmp(&(q.1.segment_ord, q.1.doc_id, &q.2))
                })
            });
            found.truncate(size);
            // everything within `r` cells' width of the point has been read
            if found.len() == size && found[size - 1].0 <= r as f64 * cell_m {
                break;
            }
        }
        found
            .into_iter()
            .filter_map(|(m, a, number, x, y, own)| {
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
                let postcode = (own > 0)
                    .then(|| {
                        doc.get_all(self.f.pcode)
                            .nth(own as usize - 1)?
                            .as_str()
                            .map(pc_norm)
                    })
                    .flatten();
                let hit = Hit::Street {
                    doc: a,
                    sid: self.fast(a, "id") as u32,
                    place: self.fast(a, "place") as u32,
                    name: stored(&doc, self.f.name),
                    x,
                    y,
                    postal_town: postal_town(
                        &doc,
                        self.f.postal_town,
                        &number,
                        postcode.as_deref(),
                    ),
                    number: Some(number),
                    postcode,
                    city: None,
                    cc: String::new(),
                };
                Some((self.render(&hit, true), m))
            })
            .collect()
    }

    /// Calls `each` with every house of street `sid`: its number, position and postcode slot.
    fn each_house(&self, sid: u32, mut each: impl FnMut(&[u8], f64, f64, u8)) {
        use tantivy_fst::{IntoStreamer, Streamer};
        let mut range = self.numbers[sid as usize % self.numbers.len()]
            .range()
            .ge(sid.to_be_bytes());
        if let Some(next) = sid.checked_add(1) {
            range = range.lt(next.to_be_bytes());
        }
        let mut houses = range.into_stream();
        while let Some((key, value)) = houses.next() {
            let (x, y, own) = schema::unpack(value);
            each(&key[4..], x, y, own);
        }
    }

    /// The streets within `radius_m` of a point, nearest first, each at its point nearest it,
    /// with the distance in metres and the postcode of the street's house nearest that point.
    // ponytail: rings grow by the smaller side of a street cell, as in `reverse_houses`.
    fn reverse_streets(&self, lat: f64, lon: f64, radius_m: f64, size: usize) -> Vec<(Value, f64)> {
        use tantivy::schema::Value as _;
        let Some(streets) = &self.streets else {
            return vec![];
        };
        let (cx, cy) = reverse::street_cell_at(lon, lat);
        let cos = lat.to_radians().cos().max(0.01);
        let cell_m = reverse::STREET_CELL_LAT_M.min(reverse::STREET_CELL_LON_M * cos);
        // in latitude steps, from the point
        let (p, sx) = (reverse::steps(lon, lat), 2.0 * cos);
        let from_p = |(x, y): (f64, f64)| ((x - p.0) * sx, y - p.1);
        let (region, country) = self.area_at(lat, lon);
        let code = |p| self.region_code(&self.chain(p));
        let region = region.and_then(code);
        let mut allowed = HashMap::new();
        let mut best: HashMap<u32, (f64, (f64, f64))> = HashMap::new();
        let mut edges = vec![];
        let read_m = |r: i64| (r as f64 - reverse::STREET_SLACK) * cell_m;
        for r in 0.. {
            edges.clear();
            ring(r, |dx, dy| streets.edges(cx + dx, cy + dy, &mut edges));
            for &(a, b, doc) in &edges {
                if !*allowed.entry(doc).or_insert_with(|| {
                    let (segment, id) = reverse::unpack_doc(doc);
                    let place = self.fast(DocAddress::new(segment, id), "place") as u32;
                    country.is_none_or(|c| {
                        place != NONE
                            && self.places[place as usize].cc == self.places[c as usize].cc
                    }) && region.is_none_or(|r| code(place).is_none_or(|c| c == r))
                }) {
                    continue;
                }
                let (d, q) = reverse::nearest((0.0, 0.0), from_p(a), from_p(b));
                let m = d * reverse::STEP_LAT_M;
                if m <= radius_m && best.get(&doc).is_none_or(|e| m < e.0) {
                    best.insert(doc, (m, q));
                }
            }
            if read_m(r) >= radius_m {
                break;
            }
            if best.len() >= size {
                let mut ms: Vec<f64> = best.values().map(|e| e.0).collect();
                if *ms.select_nth_unstable_by(size - 1, f64::total_cmp).1 <= read_m(r) {
                    break;
                }
            }
        }
        let mut found: Vec<_> = best.into_iter().collect();
        found.sort_by(|a, b| a.1.0.total_cmp(&b.1.0).then(a.0.cmp(&b.0)));
        found.truncate(size);
        found
            .into_iter()
            .filter_map(|(packed, (m, q))| {
                let (x, y) = reverse::degrees(p.0 + q.0 / sx, p.1 + q.1);
                let (segment, doc) = reverse::unpack_doc(packed);
                let a = DocAddress::new(segment, doc);
                let sid = self.fast(a, "id") as u32;
                let mut own = (f64::MAX, 0);
                self.each_house(sid, |_, hx, hy, slot| {
                    let d = ((hx - x) * cos).hypot(hy - y);
                    if d < own.0 {
                        own = (d, slot);
                    }
                });
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
                let postcode = (own.1 > 0)
                    .then(|| {
                        doc.get_all(self.f.pcode)
                            .nth(own.1 as usize - 1)?
                            .as_str()
                            .map(pc_norm)
                    })
                    .flatten();
                let (place, city) = self.street_place(self.fast(a, "place") as u32, lat, lon);
                let hit = Hit::Street {
                    doc: a,
                    sid,
                    place,
                    name: stored(&doc, self.f.name),
                    x,
                    y,
                    number: None,
                    postcode,
                    city,
                    postal_town: None,
                    cc: String::new(),
                };
                Some((self.render(&hit, true), m))
            })
            .collect()
    }

    /// A segment's centre can fall in a township's polygon even when the query is near a town
    /// point with no polygon. A descendant keeps the street's ancestry; a sibling contributes
    /// only its name, never its region. A country alone does not corroborate a sibling.
    fn street_place(&self, place: u32, lat: f64, lon: f64) -> (u32, Option<String>) {
        let pl = |p: u32| &self.places[p as usize];
        let size = |p: u32| match pl(p).key() {
            "city" => 3,
            "town" => 2,
            "village" => 1,
            _ => 0,
        };
        let Some((near, _)) = self.nearest_locality_within(lat, lon, STREET_LOCALITY_M, |p| {
            &*self.places[p as usize].class != "hamlet"
        }) else {
            return (place, None);
        };
        if place == NONE {
            return (place, None);
        }
        let ours = self.chain(place);
        let parent = pl(near).parent;
        if ours.contains(&near) {
            (place, None)
        } else if self.chain(near).contains(&place) {
            (near, None)
        } else if ours.contains(&parent)
            && !matches!(&*pl(parent).subtype, "country" | "dependency")
            && size(near) >= size(place)
        {
            (place, Some(pl(near).name(true).to_string()))
        } else {
            (place, None)
        }
    }

    /// Up to `size` results for a point: the houses within `radius_m`, nearest first, as
    /// ("address", ...); with none, the streets within it ("street", ...); with none either, the
    /// nearest locality ("locality", ...) wherever it is within about 50 km. Each result comes
    /// with its distance in metres.
    pub fn reverse_search(
        &self,
        lat: f64,
        lon: f64,
        radius_m: f64,
        size: usize,
    ) -> Vec<(&'static str, Value, f64)> {
        let houses = self.reverse_houses(lat, lon, radius_m, size);
        if !houses.is_empty() {
            return houses.into_iter().map(|(v, m)| ("address", v, m)).collect();
        }
        let streets = self.reverse_streets(lat, lon, radius_m, size);
        if !streets.is_empty() {
            return streets.into_iter().map(|(v, m)| ("street", v, m)).collect();
        }
        let place = self.reverse(lat, lon);
        let coordinate = |key: &str| place[key].as_str().and_then(|s| s.parse::<f64>().ok());
        let (Some(y), Some(x)) = (coordinate("lat"), coordinate("lon")) else {
            return vec![];
        };
        let m = ((x - lon) * lat.to_radians().cos()).hypot(y - lat) * 111_195.0;
        vec![("locality", place, m)]
    }

    /// Nearest locality point within about 50 km. One in the region whose polygon holds the point
    /// (or, with none, in the country whose polygon does) comes first, so a nearer town across a
    /// border does not name the point; with no such town in range, the nearest of any.
    pub fn reverse(&self, lat: f64, lon: f64) -> Value {
        let code = |p: u32| self.region_code(&self.chain(p));
        let (region, country) = self.area_at(lat, lon);
        let region = region.and_then(code);
        let same = |p: u32| {
            country.is_none_or(|c| self.places[p as usize].cc == self.places[c as usize].cc)
                && region.is_none_or(|r| code(p).is_none_or(|c| c == r))
        };
        match self
            .nearest_locality_within(lat, lon, NEAR_LOCALITY_M, same)
            .or_else(|| self.nearest_locality_within(lat, lon, NEAR_LOCALITY_M, |_| true))
        {
            Some((p, _)) => self.render(&Hit::Place(p), true),
            None => json!({"error": "Unable to geocode"}),
        }
    }

    /// The polygons that hold a point: the most specific one whose place has a region code, and
    /// the country's.
    fn area_at(&self, lat: f64, lon: f64) -> (Option<u32>, Option<u32>) {
        let containing: Vec<_> = self
            .areas
            .iter()
            .flat_map(|a| a.containing(lat, lon))
            .collect();
        // County polygons can spill across a region boundary; the containing region wins.
        let parent = containing.iter().find_map(|&(kind, p)| {
            (kind == 1 && self.region_code(&self.chain(p)).is_some()).then_some(p)
        });
        let (mut region, mut country) = (None, None);
        for (kind, p) in containing {
            let chain = self.chain(p);
            let county_region = (kind == 0)
                .then(|| {
                    chain
                        .iter()
                        .copied()
                        .find(|&p| self.places[p as usize].subtype.as_ref() == "region")
                })
                .flatten();
            if parent.zip(county_region).is_some_and(|(a, b)| a != b) {
                continue;
            }
            if region.is_none() && self.region_code(&chain).is_some() {
                region = Some(p);
            }
            if kind == area::COUNTRY && country.is_none() {
                country = Some(p);
            }
        }
        (region, country)
    }

    /// The address that names a point's region, for `ResolveRegion`: the most specific county,
    /// region, dependency or country polygon that holds the point and has a region code, with
    /// its subdivision below the region when it is a county that has one.
    ///
    /// A point no such polygon holds (offshore past the territorial sea, a gap in the data, a
    /// country whose regions the index lacks) falls back to the nearest points. There a house
    /// within 100 m comes first: its place was assigned by polygon, where the nearest locality
    /// point can lie across a border. Failing that, the nearest locality within 50 km, when it
    /// has a region code. Otherwise (the point is past 50 km, up to 150, or the nearest
    /// locality's parents stop short of a region) a coded locality answers only if it is in the
    /// nearest one's country and no locality of another region is within twice its distance, or
    /// four times when the nearest has no code: that gap suggests the data lacks the point's own
    /// region, which a neighbour's code would misname. When a country polygon holds the point,
    /// only its own localities are read and the house is skipped.
    pub fn reverse_region(&self, lat: f64, lon: f64) -> Value {
        let (region, country) = self.area_at(lat, lon);
        if let Some(p) = region {
            return self.render(&Hit::Place(p), true);
        }
        if country.is_none()
            && let Some((house, _)) = self
                .reverse_houses(lat, lon, HOUSE_REGION_M, 1)
                .into_iter()
                .next()
            && house["address"].get("ISO3166-2-lvl4").is_some()
        {
            return house;
        }
        let code = |p: u32| self.region_code(&self.chain(p));
        let cc = |p: u32| &self.places[p as usize].cc;
        let inside = |p: u32| country.is_none_or(|c| cc(p) == cc(c));
        let found = self
            .nearest_locality_within(lat, lon, FAR_REGION_M, inside)
            .and_then(|(nearest, m)| {
                let coded = code(nearest).is_some();
                if coded && m <= NEAR_LOCALITY_M {
                    return Some(nearest);
                }
                let (p, m) = self.nearest_locality_within(lat, lon, FAR_REGION_M, |p| {
                    inside(p) && code(p).is_some()
                })?;
                let margin = if coded { 2.0 } else { 4.0 } * m;
                let alone = || {
                    self.nearest_locality_within(lat, lon, margin, |q| {
                        inside(q) && code(q).is_some_and(|c| Some(c) != code(p))
                    })
                    .is_none()
                };
                (cc(p) == cc(nearest) && alone()).then_some(p)
            });
        match found {
            Some(p) => self.render(&Hit::Place(p), true),
            None => json!({"error": "Unable to geocode"}),
        }
    }

    fn nearest_locality_within(
        &self,
        lat: f64,
        lon: f64,
        max_m: f64,
        accept: impl Fn(u32) -> bool,
    ) -> Option<(u32, f64)> {
        self.localities.nearest(
            lat,
            lon,
            max_m,
            |p| {
                let pl = &self.places[p as usize];
                (pl.x as f64, pl.y as f64)
            },
            accept,
        )
    }
}

/// Calls `cell` with the offsets of the cells `r` cells from the centre, around a square ring.
fn ring(r: i64, mut cell: impl FnMut(i64, i64)) {
    for dy in -r..=r {
        let step = if dy.abs() == r { 1 } else { 2 * r };
        for dx in (-r..=r).step_by(step as usize) {
            cell(dx, dy);
        }
    }
}

const HOUSE_REGION_M: f64 = 100.0;
const STREET_LOCALITY_M: f64 = 1_000.0;
const FAR_REGION_M: f64 = 150_000.0;
const NEAR_LOCALITY_M: f64 = 50_000.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_another_builders_index() {
        let dir = std::env::temp_dir().join(format!("builder-check-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        check_builder(&dir).unwrap();
        let write = |hash: &str| {
            std::fs::write(
                dir.join("build.json"),
                format!(r#"{{"builder_hash":"{hash}"}}"#),
            )
            .unwrap()
        };
        write(env!("BUILDER_HASH"));
        check_builder(&dir).unwrap();
        write("0000000000000000");
        assert!(check_builder(&dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_exact_road_alias_beats_extra_words() {
        let (_, f) = schema::schema();
        let doc = tantivy::doc!(
            f.name => "East Kōwhai Road",
            f.name => "Kōwhai Road"
        );
        for query in ["Kōwhai Road", "Kowhai Rd"] {
            assert_eq!(best_name(&doc, f.name, query), "Kōwhai Road");
        }
        assert_eq!(
            best_name(&doc, f.name, "East Kowhai Rd"),
            "East Kōwhai Road"
        );
    }

    #[test]
    fn nz_comma_alignment_keeps_numbers_units_and_geographic_guards() {
        let pairs = |p: &[(&str, &str)]| -> Vec<(String, String)> {
            p.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        for number in ["5a", "10-12", "2/19", "19b"] {
            let head = pairs(&[("house_number", number), ("road", "orchard road")]);
            let whole = pairs(&[
                ("house_number", number),
                ("road", "orchard road kōwhai bay"),
                ("city", "harbourton"),
            ]);
            let aligned = nz_localities_aligned(&head, "Kōwhai Bay", "Harbourton", &whole).unwrap();
            assert_eq!(field(&aligned, "house_number"), number);
            assert_eq!(field(&aligned, "road"), "orchard road");
            assert_eq!(field(&aligned, "suburb"), "kōwhai bay");
            assert_eq!(field(&aligned, "city"), "harbourton");
            assert_eq!(
                nz_localities_aligned(&head, "Kōwhai Bay", "Harbourton", &aligned).unwrap(),
                aligned
            );
            for key in [
                "state",
                "state_district",
                "country",
                "postcode",
                "po_box",
                "level",
            ] {
                let mut guarded = whole.clone();
                guarded.push((key.into(), "other".into()));
                assert!(
                    nz_localities_aligned(&head, "Kōwhai Bay", "Harbourton", &guarded).is_none()
                );
            }
            for (suburb, city) in [("", "Harbourton"), ("Kōwhai Bay", "Harbourton 1234")] {
                assert!(nz_localities_aligned(&head, suburb, city, &whole).is_none());
            }
            let changed = pairs(&[("house_number", "999"), ("road", "orchard road")]);
            assert!(nz_localities_aligned(&changed, "Kōwhai Bay", "Harbourton", &whole).is_none());
        }
        let head = pairs(&[
            ("unit", "2"),
            ("house_number", "19"),
            ("road", "orchard road"),
        ]);
        let mut whole = head.clone();
        whole.push(("city".into(), "kōwhai bay harbourton".into()));
        let aligned = nz_localities_aligned(&head, "Kōwhai Bay", "Harbourton", &whole).unwrap();
        assert_eq!(field(&aligned, "unit"), "2");
        whole[0].1 = "3".into();
        assert!(nz_localities_aligned(&head, "Kōwhai Bay", "Harbourton", &whole).is_none());
    }

    #[test]
    fn postcode_repair_preserves_requested_fields_for_corroboration() {
        let dir = std::env::temp_dir().join(format!("parse-postcode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (schema, f) = schema::schema();
        let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
        tantivy_index
            .tokenizers()
            .register("fold", schema::analyzer());
        let mut writer: tantivy::IndexWriter = tantivy_index
            .writer_with_num_threads(1, 15_000_000)
            .unwrap();
        writer
            .add_document(tantivy::doc!(
                f.kind => "z", f.pcode => "12345", f.pcode => "12345ST",
                f.country => "US", f.place => 0u64, f.importance => 1u64
            ))
            .unwrap();
        writer.commit().unwrap();
        drop(writer);
        std::fs::write(dir.join("numbers.shards"), "1").unwrap();
        std::fs::write(
            dir.join("numbers-00.fst"),
            tantivy_fst::MapBuilder::memory().into_inner().unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("places.tsv"),
            "country\t\tUS\t\t\t0\t0\t0\t0\t0\t0\t0\t\tUnited States\ten=United States\n",
        )
        .unwrap();
        let parser = Arc::new(Parser::unloaded(16));
        let pairs = |p: &[(&str, &str)]| -> Parsed {
            p.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let street = pairs(&[
            ("house_number", "20"),
            ("road", "oak"),
            ("city", "springfeld"),
            ("country", "usa"),
        ]);
        let mut garbled = street.to_vec();
        garbled.insert(2, ("house_number".into(), "12345".into()));
        let house = pairs(&[("house_number", "12345"), ("road", "oak"), ("state", "ny")]);
        let protected = [
            (
                "20 Ash, 12345 Springfeld, USA",
                pairs(&[
                    ("house_number", "20"),
                    ("road", "ash"),
                    ("city", "springfeld"),
                    ("country", "usa"),
                ]),
            ),
            (
                "21 Oak, 12345 Springfeld, USA",
                pairs(&[
                    ("house_number", "21"),
                    ("road", "oak"),
                    ("city", "springfeld"),
                    ("country", "usa"),
                ]),
            ),
            (
                "12345 Oak, USA",
                pairs(&[
                    ("house_number", "12345"),
                    ("road", "oak"),
                    ("country", "usa"),
                ]),
            ),
            (
                "20 Oak 12345, Springfeld, USA",
                pairs(&[
                    ("house_number", "20"),
                    ("road", "oak 12345"),
                    ("city", "springfeld"),
                    ("country", "usa"),
                ]),
            ),
            (
                "P.O. Box 12345, Springfeld, USA",
                pairs(&[
                    ("po_box", "p.o. box 12345"),
                    ("city", "springfeld"),
                    ("country", "usa"),
                ]),
            ),
            (
                "20 Oak, 12345 St. Johns, USA",
                pairs(&[
                    ("house_number", "20"),
                    ("road", "oak"),
                    ("postcode", "12345"),
                    ("city", "st. johns"),
                    ("country", "usa"),
                ]),
            ),
        ];
        let mut duplicate = street.to_vec();
        duplicate.insert(1, ("road".into(), "ash".into()));
        let mut cache = parser.cache.lock().unwrap();
        cache.extend([
            ("20 Oak, 12345 Springfeld, USA".into(), garbled.into()),
            (
                "20 Oak, Springfeld, USA".into(),
                street
                    .iter()
                    .filter(|(k, _)| k != "country")
                    .cloned()
                    .collect(),
            ),
            ("12345 Oak, NY".into(), house.clone()),
            ("Oak, USA".into(), street.clone()),
            ("P.O. Box, Springfeld, USA".into(), street.clone()),
            ("20 Oak, Johns, USA".into(), street.clone()),
            ("20 Ash, Springfeld, USA".into(), duplicate.into()),
            ("21 Oak, Springfeld, USA".into(), street.clone()),
        ]);
        cache.extend(protected.iter().map(|(q, p)| ((*q).into(), p.clone())));
        drop(cache);
        let index = Index::open(&dir, parser).unwrap();
        let mut expected = street.to_vec();
        expected.push(("postcode".into(), "12345".into()));
        assert_eq!(
            &*index
                .parse_address("20 Oak, 12345 Springfeld, USA", true)
                .unwrap()
                .unwrap(),
            expected
        );
        assert_eq!(
            index.parse_address("12345 Oak, NY", true).unwrap().unwrap(),
            house
        );
        assert_eq!(
            index
                .parse_address("12345 Oak, NY", false)
                .unwrap()
                .unwrap(),
            house
        );
        for (query, parsed) in protected {
            assert_eq!(
                index.parse_address(query, true).unwrap().unwrap(),
                parsed,
                "{query}"
            );
        }
        assert_eq!(
            &*index
                .parse_address("20 Oak 12345, Springfeld, USA", false)
                .unwrap()
                .unwrap(),
            expected
                .iter()
                .filter(|(k, _)| k != "country")
                .cloned()
                .collect::<Vec<_>>()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Builds a tiny index through the builder's own number and reverse writers, then reads houses
    /// back by distance: nearest first, only within the radius, across streets, with postcodes.
    #[test]
    fn native_roads_are_filtered_before_the_ranked_children_hide_a_house() {
        let dir = std::env::temp_dir().join(format!("native-roads-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (_, mut writer, f) = schema::create(dir.to_str().unwrap(), 1, 15_000_000);
        for sid in 0..32u64 {
            let name = if sid == 31 {
                "明光路3段".to_string()
            } else {
                format!("明光路3段{sid}巷")
            };
            writer
                .add_document(tantivy::doc!(
                    f.kind => "s", f.names => name.clone(), f.name => name,
                    f.country => "TW", f.id => sid, f.place => 0u64,
                    f.importance => 32 - sid, f.x => 123.0, f.y => 25.0
                ))
                .unwrap();
        }
        writer.commit().unwrap();
        drop(writer);
        let (x, y) = schema::pack(123.0, 25.0, 0);
        let spill = schema::NumberSpill::new(dir.join(".numbers-spill"), 1);
        spill.add(vec![schema::Bucket {
            short: vec![schema::Num::new(31, "237", x, y).ok().unwrap()],
            long: vec![],
        }]);
        spill.write_fsts(dir.to_str().unwrap(), 1, &|sid, slot| (sid, slot));
        std::fs::write(
            dir.join("places.tsv"),
            "country\t\tTW\t\t\t0\t123\t25\t123\t25\t123\t25\t\t臺灣\ten=Taiwan\n",
        )
        .unwrap();
        let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
        let found = index.streets("明光路三段", Some("237"), None, None, Some("tw"), false);
        assert!(matches!(
            found.as_slice(),
            [Hit::Street { sid: 31, number: Some(number), .. }] if number == "237"
        ));
        let missing = index.streets("明光路三段", Some("999"), None, None, Some("tw"), false);
        assert!(matches!(
            missing.as_slice(),
            [Hit::Street {
                sid: 31,
                number: None,
                ..
            }]
        ));
        drop(index);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reverse_finds_the_nearest_houses() {
        let dir = std::env::temp_dir().join(format!("reverse-houses-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (schema, f) = schema::schema();
        let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
        tantivy_index
            .tokenizers()
            .register("fold", schema::analyzer());
        let mut writer: tantivy::IndexWriter = tantivy_index
            .writer_with_num_threads(1, 15_000_000)
            .unwrap();
        for (sid, name, pcode) in [
            (1000u64, "Oak Street", "12345"),
            (1001, "Elm Street", ""),
            (1003, "Oak Street", "67890"),
            (1002, "Oak Street", "67890"),
        ] {
            let mut d = tantivy::doc!(
                f.kind => "s", f.names => name.to_lowercase(), f.country => "US", f.id => sid,
                f.place => 0u64, f.importance => 1u64, f.x => 10.0, f.y => 50.0, f.name => name,
                f.city => if sid == 1000 { "Wrongtown" } else { "Righttown" }
            );
            if !pcode.is_empty() {
                d.add_text(f.pcode, pcode);
            }
            writer.add_document(d).unwrap();
        }
        writer.commit().unwrap();
        drop(writer);
        let at = |sid, number: &str, lon, lat, own| {
            let (x, y) = schema::pack(lon, lat, own);
            schema::Num::new(sid, number, x, y).ok().unwrap()
        };
        let spill = schema::NumberSpill::new(dir.join(".numbers-spill"), 1);
        spill.add(vec![schema::Bucket {
            short: vec![
                at(1000, "1", 10.0, 50.0, 1),
                at(1000, "2", 10.0002, 50.0, 0),
                at(1001, "7", 10.0003, 50.0004, 0),
                at(1001, "9", 10.05, 50.0, 0),
                at(1002, "1", 11.0, 50.0, 1),
                at(1003, "1", 12.0, 50.0, 1),
            ],
            long: vec![],
        }]);
        spill.write_fsts(dir.to_str().unwrap(), 1, &|sid, slot| (sid, slot));
        assert_eq!(
            reverse::build(&dir, &tantivy_index, f.kind, |_| None).unwrap(),
            (5, 0)
        );
        std::fs::write(
            dir.join("places.tsv"),
            "country\t\tUS\t\t\t0\t0\t0\t0\t0\t0\t0\t\tUnited States\ten=United States\n",
        )
        .unwrap();
        let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
        for _ in 0..32 {
            let found = index.streets(
                "Oak Street",
                Some("1"),
                Some("Wrongtown"),
                Some("67890"),
                Some("US"),
                false,
            );
            assert!(matches!(found.as_slice(), [Hit::Street { sid: 1002, .. }]));
        }
        let find = |lat, lon, radius, size| {
            index
                .reverse_search(lat, lon, radius, size)
                .into_iter()
                .map(|(layer, v, m)| {
                    let a = &v["address"];
                    let number = a["house_number"].as_str().unwrap_or_default().to_string();
                    (
                        layer,
                        a["road"].as_str().unwrap_or_default().to_string(),
                        number,
                        m,
                    )
                })
                .collect::<Vec<_>>()
        };
        let hits = find(50.0, 10.00005, 500.0, 2);
        let named: Vec<_> = hits
            .iter()
            .map(|h| (h.0, h.1.as_str(), h.2.as_str()))
            .collect();
        assert_eq!(
            named,
            [
                ("address", "Oak Street", "1"),
                ("address", "Oak Street", "2")
            ]
        );
        assert!(hits[0].3 < 5.0 && hits[0].3 < hits[1].3, "{hits:?}");
        let postcode =
            index.reverse_search(50.0, 10.0, 500.0, 1)[0].1["address"]["postcode"].clone();
        assert_eq!(postcode, "12345");
        // 100 m takes in the first three houses of two streets, not the one 3 km away
        let numbers: Vec<_> = find(50.0, 10.0, 100.0, 10)
            .into_iter()
            .map(|h| h.2)
            .collect();
        assert_eq!(numbers, ["1", "2", "7"]);
        // beyond a ring of cells the nearest house is still found, from another street
        let far = find(50.0, 10.05, 500.0, 1);
        assert_eq!((far[0].1.as_str(), far[0].2.as_str()), ("Elm Street", "9"));
        // no house and no locality in range
        assert!(find(51.0, 11.0, 500.0, 1).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Documents of each kind share terms; `top_of` must return only the asked kind and country,
    /// without a `kind` clause, across segments and with the kinds interleaved in one.
    #[test]
    fn equally_important_hits_come_in_the_same_order_whatever_the_document_order() {
        let found = |name: &str, order: [u64; 4], commit_after: usize| {
            let dir =
                std::env::temp_dir().join(format!("ties-index-{name}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let (schema, f) = schema::schema();
            let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
            tantivy_index
                .tokenizers()
                .register("fold", schema::analyzer());
            let mut writer: tantivy::IndexWriter = tantivy_index
                .writer_with_num_threads(1, 15_000_000)
                .unwrap();
            for (i, id) in order.into_iter().enumerate() {
                writer
                    .add_document(tantivy::doc!(
                        f.kind => "s", f.names => "oak", f.country => "US", f.id => id,
                        f.place => 7u64, f.importance => 5u64, f.x => id as f64, f.y => 0.0
                    ))
                    .unwrap();
                if i == commit_after {
                    writer.commit().unwrap();
                }
            }
            writer.commit().unwrap();
            drop(writer);
            std::fs::write(dir.join("numbers.shards"), "1").unwrap();
            std::fs::write(
                dir.join("numbers-00.fst"),
                tantivy_fst::MapBuilder::memory().into_inner().unwrap(),
            )
            .unwrap();
            std::fs::write(
                dir.join("places.tsv"),
                "country\t\tUS\t\t\t0\t0\t0\t0\t0\t0\t0\t\tUnited States\ten=United States\n",
            )
            .unwrap();
            let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
            let names = index.musts(index.f.names, "oak");
            let top = |n| -> Vec<u64> {
                index
                    .top_of(
                        Kind::Street,
                        None,
                        names.iter().map(|(o, q)| (*o, q.box_clone())).collect(),
                        n,
                    )
                    .into_iter()
                    .map(|a| index.fast(a, "id"))
                    .collect()
            };
            let all = (top(10), top(2));
            drop(index);
            std::fs::remove_dir_all(&dir).unwrap();
            all
        };
        let expected = (vec![1, 2, 3, 4], vec![1, 2]);
        assert_eq!(found("a", [3, 1, 4, 2], 1), expected);
        assert_eq!(found("b", [2, 4, 1, 3], 0), expected);
        assert_eq!(found("c", [1, 2, 3, 4], 2), expected);
    }

    #[test]
    fn top_of_separates_kinds_and_countries() {
        let dir = std::env::temp_dir().join(format!("kinds-index-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (schema, f) = schema::schema();
        let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
        tantivy_index
            .tokenizers()
            .register("fold", schema::analyzer());
        let mut writer: tantivy::IndexWriter = tantivy_index
            .writer_with_num_threads(1, 15_000_000)
            .unwrap();
        let docs = [
            tantivy::doc!(f.kind => "p", f.names => "oak", f.country => "US", f.id => 7u64, f.importance => 1u64),
            tantivy::doc!(f.kind => "s", f.names => "oak", f.country => "US", f.pcode => "12345", f.id => 8u64, f.place => 7u64, f.importance => 2u64),
            tantivy::doc!(f.kind => "s", f.names => "oak", f.country => "DK", f.id => 9u64, f.place => 7u64, f.importance => 3u64),
            tantivy::doc!(f.kind => "z", f.pcode => "12345", f.country => "US", f.place => 7u64, f.importance => 4u64),
            tantivy::doc!(f.kind => "s", f.names => "oak", f.country => "DK", f.id => 11u64, f.place => 7u64, f.importance => 1u64),
            tantivy::doc!(f.kind => "p", f.names => "oak", f.country => "DK", f.id => 10u64, f.importance => 5u64),
        ];
        for (i, doc) in docs.into_iter().enumerate() {
            writer.add_document(doc).unwrap();
            if i == 3 || i == 5 {
                writer.commit().unwrap();
            }
        }
        drop(writer);
        std::fs::write(dir.join("numbers.shards"), "1").unwrap();
        std::fs::write(
            dir.join("numbers-00.fst"),
            tantivy_fst::MapBuilder::memory().into_inner().unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("places.tsv"),
            "country\t\tUS\t\t\t0\t0\t0\t0\t0\t0\t0\t\tUnited States\ten=United States\n",
        )
        .unwrap();
        let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
        let names = || index.musts(index.f.names, "oak");
        let ids = |hits: Vec<DocAddress>| {
            let mut ids: Vec<u64> = hits.into_iter().map(|a| index.fast(a, "id")).collect();
            ids.sort();
            ids
        };
        assert_eq!(index.columns.len(), 2);
        assert_eq!(ids(index.top_of(Kind::Place, None, names(), 10)), [7, 10]);
        assert_eq!(ids(index.top_of(Kind::Place, None, names(), 1)), [10]);
        assert_eq!(ids(index.top_of(Kind::Place, Some("us"), names(), 10)), [7]);
        assert_eq!(
            ids(index.top_of(Kind::Street, None, names(), 10)),
            [8, 9, 11]
        );
        assert_eq!(
            ids(index.top_of(Kind::Street, Some("us"), names(), 10)),
            [8]
        );
        assert_eq!(
            ids(index.top_of(Kind::Street, Some("dk"), names(), 10)),
            [9, 11]
        );
        assert_eq!(ids(index.top_of(Kind::Street, Some("dk"), names(), 1)), [9]);
        assert!(
            index
                .top_of(Kind::Street, Some("fr"), names(), 10)
                .is_empty()
        );
        assert_eq!(
            ids(index.top_of(Kind::Street, None, vec![], 10)),
            [8, 9, 11]
        );
        assert!(index.any_of(Kind::Street, Some("dk"), names()));
        assert!(!index.any_of(Kind::Place, Some("fr"), names()));
        assert!(!index.any_of(Kind::Postcode, None, names()));
        let twice = index
            .memoized(|| [0, 1].map(|_| ids(index.top_of(Kind::Street, Some("dk"), names(), 10))));
        assert_eq!(twice, [vec![9, 11], vec![9, 11]]);
        let pcode = || vec![must(term(index.f.pcode, "12345"))];
        let hits = index.top_of(Kind::Postcode, None, pcode(), 10);
        assert_eq!(hits.len(), 1);
        assert!(
            index.columns[hits[0].segment_ord as usize]
                .id
                .first(hits[0].doc_id)
                .is_none()
        );
        assert_eq!(ids(index.top_of(Kind::Street, None, names(), 1)), [9]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn english_name_is_found_among_the_pairs() {
        let at = |names: &str| match Place::english(names) {
            NONE => None,
            at => names[at as usize..].split(SEP).next().map(str::to_owned),
        };
        assert_eq!(at("en=London").as_deref(), Some("London"));
        assert_eq!(
            at("fr=Londres\u{1f}en=London\u{1f}de=London").as_deref(),
            Some("London")
        );
        assert_eq!(at("fr=Londres\u{1f}en_x=London"), None);
        assert_eq!(
            at("alt_name=en=x\u{1f}en=London").as_deref(),
            Some("London")
        );
        assert_eq!(at(""), None);
    }

    #[test]
    fn reverse_returns_the_nearest_locality_within_50_km() {
        let dir = std::env::temp_dir().join(format!("reverse-index-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (schema, f) = schema::schema();
        let tantivy_index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
        tantivy_index
            .tokenizers()
            .register("fold", schema::analyzer());
        let mut writer: tantivy::IndexWriter = tantivy_index
            .writer_with_num_threads(1, 15_000_000)
            .unwrap();
        // id, subtype, country, region, x, y, parent, name: a place's row number is its id
        let places = [
            (0, "country", "DK", "", 0.0, 0.0, "", "Denmark"),
            (1, "region", "DK", "DK-84", 0.0, 0.0, "0", "Hovedstaden"),
            (2, "locality", "DK", "", 12.5683, 55.6761, "1", "Copenhagen"),
            (3, "locality", "DK", "", 12.0803, 55.6415, "1", "Roskilde"),
            (4, "neighborhood", "DK", "", 12.55, 55.69, "2", "Norrebro"),
            (5, "region", "DK", "DK-82", 0.0, 0.0, "0", "Midtjylland"),
            (6, "locality", "DK", "", 10.2039, 56.1629, "5", "Aarhus"),
            (7, "locality", "DK", "", 12.569, 55.67, "2", "Small Hamlet"),
        ];
        let mut tsv = String::new();
        for (id, subtype, cc, region, x, y, parent, name) in places {
            let class = match (id, subtype) {
                (7, _) => "hamlet",
                (_, "locality") => "city",
                _ => "",
            };
            tsv += &format!(
                "{subtype}\t{class}\t{cc}\t{region}\t\t0\t{x}\t{y}\t0\t0\t0\t0\t{parent}\t{name}\t\n"
            );
            if x != 0.0 {
                writer
                    .add_document(tantivy::doc!(
                        f.kind => "p", f.country => cc, f.id => id as u64, f.importance => 1u64,
                        f.x => x, f.y => y
                    ))
                    .unwrap();
            }
        }
        writer.commit().unwrap();
        drop(writer);
        std::fs::write(dir.join("numbers.shards"), "1").unwrap();
        std::fs::write(
            dir.join("numbers-00.fst"),
            tantivy_fst::MapBuilder::memory().into_inner().unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("places.tsv"), tsv).unwrap();
        let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
        let mut house = Hit::Street {
            doc: DocAddress::new(0, 0),
            sid: 0,
            place: 2,
            name: "Test Road".into(),
            x: 12.5683,
            y: 55.6761,
            number: Some("1".into()),
            postcode: None,
            city: None,
            postal_town: None,
            cc: "DK".into(),
        };
        assert!(index.house_locality_named(&house, "Copenhagen"));
        for name in ["Hovedstaden", "Denmark", "Cop", "DK"] {
            assert!(!index.house_locality_named(&house, name), "{name}");
        }
        if let Hit::Street { number, .. } = &mut house {
            *number = None;
        }
        assert!(!index.house_locality_named(&house, "Copenhagen"));
        let city_and_region = |lat: f64, lon: f64| {
            let found = index.reverse(lat, lon);
            let address = &found["address"];
            (
                address["city"].as_str().map(String::from),
                address["ISO3166-2-lvl4"].as_str().map(String::from),
            )
        };
        let found = |city: &str, region: &str| (Some(city.to_string()), Some(region.to_string()));
        assert_eq!(
            city_and_region(55.6761, 12.5683),
            found("Copenhagen", "DK-84")
        );
        // a neighbourhood is never the answer; its town is
        assert_eq!(city_and_region(55.69, 12.55), found("Copenhagen", "DK-84"));
        assert_eq!(city_and_region(55.64, 12.09), found("Roskilde", "DK-84"));
        // no locality within 5 km: the nearest within 50
        assert_eq!(city_and_region(55.65, 12.3), found("Roskilde", "DK-84"));
        assert_eq!(city_and_region(56.1, 10.5), found("Aarhus", "DK-82"));
        assert_eq!(index.street_place(1, 55.6761, 12.5683), (2, None));
        assert_eq!(index.street_place(4, 55.6761, 12.5683), (4, None));
        assert_eq!(
            index.street_place(2, 55.6415, 12.0803),
            (2, Some("Roskilde".into()))
        );
        assert_eq!(index.street_place(2, 56.1629, 10.2039), (2, None));
        assert_eq!(index.street_place(2, 55.65, 12.3), (2, None));
        assert_eq!(index.street_place(2, 55.67, 12.569), (2, None));
        assert_eq!(index.street_place(NONE, 55.6761, 12.5683), (NONE, None));
        // beyond 50 km (Roskilde is 61 km away), or at sea
        for (lat, lon) in [(55.64, 11.1), (56.0, 3.0), (0.0, 0.0), (-90.0, 180.0)] {
            let found = index.reverse(lat, lon);
            assert_eq!(found["error"], "Unable to geocode", "{lat} {lon}: {found}");
            assert!(found.get("address").is_none());
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn opens_without_writing_a_lock() {
        let dir = std::env::temp_dir().join(format!("read-only-index-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let index = tantivy::Index::create_in_dir(&dir, schema::schema().0).unwrap();
        index
            .writer::<tantivy::TantivyDocument>(15_000_000)
            .unwrap()
            .commit()
            .unwrap();
        drop(index);
        let locks = || {
            std::fs::read_dir(&dir)
                .unwrap()
                .filter(|e| {
                    e.as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".lock")
                })
                .count()
        };
        for e in std::fs::read_dir(&dir).unwrap() {
            let path = e.unwrap().path();
            if path.to_string_lossy().ends_with(".lock") {
                std::fs::remove_file(path).unwrap();
            }
        }
        let mmap = tantivy::directory::MmapDirectory::open(&dir).unwrap();
        let reader = tantivy::Index::open(ReadOnly(mmap))
            .unwrap()
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::Manual)
            .try_into()
            .unwrap();
        assert_eq!(reader.searcher().num_docs(), 0);
        assert_eq!(locks(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reversed_addresses_are_turned_and_units_leave_the_road() {
        let pairs = |p: &[(&str, &str)]| -> Vec<(String, String)> {
            p.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        };
        let country = |text: &str| -> Result<Vec<(String, String)>> {
            Ok(if text == "United States" {
                pairs(&[("country", "united states")])
            } else {
                pairs(&[("road", "x")])
            })
        };
        let q = "United States, WY 82636, Evansville, 498 Hat Six Road";
        let two_roads = pairs(&[
            ("road", "united states wy"),
            ("postcode", "82636"),
            ("road", "evansville"),
            ("house_number", "498"),
            ("road", "hat six road"),
        ]);
        assert_eq!(
            reversed(q, &two_roads, country).unwrap().as_deref(),
            Some("498 Hat Six Road, Evansville, WY 82636, United States")
        );
        let labelled = pairs(&[("country", "france"), ("postcode", "76160")]);
        assert!(
            reversed("France, 76160 Darnétal, 2 Rue X", &labelled, country)
                .unwrap()
                .is_some()
        );
        // repeated roads after a street, not a country, are left alone
        let q = "Nieuwe Bosscheweg 6, 5017 JM Tilburg, Netherlands";
        let repeats = pairs(&[("road", "nieuwe bosscheweg"), ("road", "jm")]);
        assert!(reversed(q, &repeats, country).unwrap().is_none());
        assert!(one_house(&pairs(&[("house_number", "1"), ("road", "a")])));
        assert!(!one_house(&two_roads));

        for (road, kept, unit) in [
            ("south fillmore street #b", "south fillmore street", "#b"),
            (
                "201st avenue souhtwest apt 10a",
                "201st avenue souhtwest",
                "apt 10a",
            ),
            ("main street suite 200", "main street", "suite 200"),
            ("rue ste foy", "rue ste foy", ""),
            ("avenue unit", "avenue unit", ""),
        ] {
            let mut parsed = pairs(&[("house_number", "1"), ("road", road)]);
            unit_from_road(&mut parsed);
            assert_eq!(field(&parsed, "road"), kept, "{road}");
            assert_eq!(field(&parsed, "unit"), unit, "{road}");
        }
    }

    #[test]
    fn one_edit_apart() {
        for (a, b) in [
            ("srteet", "street"),
            ("stret", "street"),
            ("streeet", "street"),
            ("strete", "street"),
            ("street", "street"),
            ("snta", "santa"),
        ] {
            assert!(one_edit(a, b), "{a} {b}");
        }
        for (a, b) in [
            ("sretet", "street"),
            ("str", "street"),
            ("avenue", "street"),
            ("tsreet", "streets"),
        ] {
            assert!(!one_edit(a, b), "{a} {b}");
        }
        assert_eq!(misspelt_kind("southewst"), ["se", "sw"]);
        assert_eq!(misspelt_kind("srteet"), ["st"]);
        assert!(misspelt_kind("sw").is_empty());
        assert!(postcode_not_contradicted(Some("04103"), Some("04103")));
        assert!(postcode_not_contradicted(None, Some("04103")));
        assert!(postcode_not_contradicted(Some("04101"), None));
        assert!(!postcode_not_contradicted(Some("04101"), Some("04103")));
        use tantivy_fst::Automaton;
        let run = |word: &str, input: &str| {
            let a = OneEdit(word.as_bytes());
            let state = input.bytes().fold(a.start(), |s, c| a.accept(&s, c));
            a.is_match(&state)
        };
        for word in ["street", "santa", "jacinthes", "trailwood"] {
            for input in [
                "street",
                "srteet",
                "stret",
                "strete",
                "streeet",
                "sretet",
                "snta",
                "santa",
                "jaicnthes",
                "tarilwood",
                "trailwod",
                "avenue",
                "tsreet",
                "atreet",
            ] {
                let same_start = input.as_bytes().first() == word.as_bytes().first();
                assert_eq!(
                    run(word, input),
                    same_start && one_edit(input, word),
                    "{word} {input}"
                );
            }
        }
    }

    #[test]
    fn admin_words_and_number_prefixes() {
        assert_eq!(without_admin_word("Vas County").as_deref(), Some("Vas"));
        assert_eq!(
            without_admin_word("Departamento de Santa Cruz").as_deref(),
            Some("Santa Cruz")
        );
        assert_eq!(
            without_admin_word("Kreisfreie Stadt München").as_deref(),
            Some("München")
        );
        assert_eq!(without_admin_word("County"), None);
        assert_eq!(
            without_admin_word("Maplewood Township").as_deref(),
            Some("Maplewood")
        );
        assert_eq!(
            without_admin_word("City and Borough of Juneau").as_deref(),
            Some("Juneau")
        );
        assert_eq!(without_admin_word("Leinster"), None);
        assert_eq!(house_number("No:5"), "5");
        assert_eq!(without_number_mark("köprüsü sk. no"), "köprüsü sk.");
        assert_eq!(without_number_mark("nord"), "nord");
        assert_eq!(house_number("nº 1824"), "1824");
        assert_eq!(house_number("№ 43"), "43");
        assert_eq!(house_number("12b"), "12b");
        assert_eq!(house_number("Nordweg"), "Nordweg");
    }

    #[test]
    fn spaced_house_suffixes_and_postcode_city_parts() {
        assert_eq!(compact_house_suffix("7 bis").as_deref(), Some("7bis"));
        assert_eq!(compact_house_suffix("40 C").as_deref(), Some("40C"));
        assert_eq!(compact_house_suffix("63 P2").as_deref(), Some("63P2"));
        for n in [
            "7bis",
            "12 34",
            "12 bis apt 4",
            "12/3 A",
            "7 rue",
            "12 avenue",
            "20 route",
            "5 apt",
            "10 north",
        ] {
            assert!(compact_house_suffix(n).is_none(), "{n}");
        }
        let head = vec![
            ("house_number".into(), "28".into()),
            ("road".into(), "rue des cygnes".into()),
        ];
        let country = ("country".into(), "france".into());
        let wrong = vec![
            ("house_number".into(), "89116".into()),
            ("road".into(), "belle-rive".into()),
            ("city".into(), "vallee".into()),
        ];
        let aligned =
            postal_city_aligned(&head, "89116 Belle-Rive la Vallée", &country, &wrong).unwrap();
        assert_eq!(field(&aligned, "house_number"), "28");
        assert_eq!(field(&aligned, "postcode"), "89116");
        assert_eq!(field(&aligned, "city"), "belle-rive la vallée");
        let mut duplicate = head.clone();
        duplicate.extend(wrong.clone());
        duplicate.push(country.clone());
        let unseparated = postal_without_commas(
            "28 rue des cygnes 89116 Belle-Rive la Vallée France",
            &duplicate,
            |q| {
                assert_eq!(q, "28 rue des cygnes");
                Ok(head.clone())
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(field(&unseparated, "house_number"), "28");
        assert_eq!(field(&unseparated, "postcode"), "89116");
        assert_eq!(field(&unseparated, "city"), "belle-rive la vallée");
        let dutch = postal_city_aligned(
            &head,
            "6571 AD Berg en Dal",
            &("country".into(), "netherlands".into()),
            &wrong,
        )
        .unwrap();
        assert_eq!(field(&dutch, "postcode"), "6571 ad");
        assert_eq!(field(&dutch, "city"), "berg en dal");
        assert!(
            postal_city_aligned(
                &head,
                "9406 KX Assen",
                &("country".into(), "belgium".into()),
                &wrong
            )
            .is_none()
        );
        assert!(postal_city_aligned(&head, "8911 Belle-Rive", &country, &wrong).is_none());
        assert!(postal_city_aligned(&head, "89116 Belle-Rive 2", &country, &wrong).is_none());
        assert!(
            postal_city_aligned(
                &head,
                "89116 Belle-Rive",
                &country,
                &[("state".into(), "bourgogne".into())]
            )
            .is_none()
        );
    }

    #[test]
    fn postcode_suffix_country_label_needs_an_intact_dutch_code() {
        let head = vec![
            ("house_number".into(), "4a".into()),
            ("road".into(), "meadowln. - east".into()),
        ];
        let country = ("country".into(), "netherlands".into());
        let mut whole = head.clone();
        whole.extend([
            ("postcode".into(), "1234".into()),
            ("country".into(), "tj".into()),
            ("city".into(), "polderdam".into()),
            country.clone(),
        ]);
        assert!(postcode_country_suffix("1234 TJ Polderdam", &whole));
        let aligned = postal_city_aligned(&head, "1234 TJ Polderdam", &country, &whole).unwrap();
        for key in ["house_number", "road"] {
            assert_eq!(field(&aligned, key), field(&whole, key));
        }
        assert_eq!(field(&aligned, "postcode"), "1234 tj");
        assert_eq!(field(&aligned, "city"), "polderdam");
        for (slot, value) in [(0, "4b"), (1, "other road")] {
            let mut changed = head.clone();
            changed[slot].1 = value.into();
            assert!(postal_city_aligned(&changed, "1234 TJ Polderdam", &country, &whole).is_none());
        }
        for tail in [
            "1234 TJ",
            "1234, TJ Polderdam",
            "5678 TJ Polderdam",
            "12345 TJ Polderdam",
            "1234 Tajikistan Polderdam",
        ] {
            assert!(!postcode_country_suffix(tail, &whole), "{tail}");
        }
        for suffix in ["中国", "ΤЈ", "T-J", "TJ.", "T"] {
            whole[3].1 = suffix.to_lowercase();
            assert!(
                !postcode_country_suffix(&format!("1234 {suffix} Polderdam"), &whole),
                "{suffix}"
            );
        }
        whole[3].1 = "tj".into();
        for cc in ["belgium", "germany", "norway", "france"] {
            assert!(
                postal_city_aligned(
                    &head,
                    "1234 TJ Polderdam",
                    &("country".into(), cc.into()),
                    &whole
                )
                .is_none(),
                "{cc}"
            );
        }
        whole.pop();
        assert!(!postcode_country_suffix("1234 TJ Polderdam", &whole));
        whole.push(country);
        for key in [
            "country",
            "house_number",
            "road",
            "postcode",
            "country_region",
        ] {
            whole.push((key.into(), "germany".into()));
            assert!(!postcode_country_suffix("1234 TJ Polderdam", &whole));
            whole.pop();
        }
        whole[3].1 = "tajikistan".into();
        assert!(!postcode_country_suffix("1234 TJ Polderdam", &whole));
    }

    #[test]
    fn chinese_road_addresses() {
        assert_eq!(
            chinese("上海市虹口区中山北一路295号, Hongkou 200083, China"),
            Some((
                vec!["上海市".into(), "虹口区".into()],
                "中山北一路".into(),
                "295".into()
            ))
        );
        assert_eq!(
            chinese("浙江省海宁市学林街60至62号"),
            Some((
                vec!["浙江省".into(), "海宁市".into()],
                "学林街".into(),
                "60".into()
            ))
        );
        assert_eq!(
            chinese("学院路18号, Beijing 100083, China"),
            Some((vec![], "学院路".into(), "18".into()))
        );
        for query in [
            "臺北市松山區南京東路五段３８９巷６弄１１號, Taiwan",
            "臺北市松山區南京東路五段389巷6弄11號, Taiwan",
            "臺北市松山區南京東路五段３８９巷６弄１１號三樓, Taiwan",
        ] {
            assert_eq!(
                chinese(query),
                Some((
                    vec!["臺北市".into(), "松山區".into()],
                    "南京東路五段389巷6弄".into(),
                    "11".into()
                ))
            );
        }
        assert_eq!(
            chinese("臺北市松山區市民大道五段１３１號七樓, Taiwan"),
            Some((
                vec!["臺北市".into(), "松山區".into()],
                "市民大道五段".into(),
                "131".into()
            ))
        );
        assert_eq!(
            chinese("臺北市松山區南京東路三段２６９巷６之４號二樓, Taiwan")
                .unwrap()
                .2,
            "6之4"
        );
        assert_eq!(chinese("臺北市松山區南京東路11號未知商店"), None);
        assert_eq!(chinese("10 Downing Street"), None);
    }

    #[test]
    fn japanese_block_addresses() {
        let got = japanese("〒105-0011 東京都港区芝公園４丁目２番８号");
        assert_eq!(got[0], ("港区".into(), "芝公園4丁目".into(), "2-8".into()));
        assert_eq!(got[1], ("港区".into(), "芝公園".into(), "4-2".into()));
        assert_eq!(
            japanese("東京都千代田区大手町1-1-1")[0],
            ("千代田区".into(), "大手町1丁目".into(), "1-1".into())
        );
        assert_eq!(japanese("大阪府大阪市北区梅田三丁目1-3")[0].1, "梅田3丁目");
        assert_eq!(kanji_number("二十一"), Some(21));
        assert!(japanese("1600 Pennsylvania Avenue").is_empty());
        assert!(japanese("東京都").is_empty());
    }

    #[test]
    fn names_and_codes() {
        assert_eq!(cjk_stem("東京都"), Some("東京"));
        assert_eq!(cjk_stem("서울특별시"), Some("서울"));
        assert_eq!(cjk_stem("Kyoto"), None);
        assert_eq!(exact_key("上海市"), exact_key("上海"));
        assert_eq!(irish_province("IE-D"), Some("IE-L"));
        assert_eq!(irish_province("GB-ENG"), None);
        assert_eq!(title("københavn v"), "København V");
    }

    #[test]
    fn an_administrative_settlement_twin_needs_the_same_source_identity_and_geography() {
        let rows = [
            ("locality", "GB", "GB-WLS", "Q999900001", false),
            ("region", "GB", "GB-WLS", "Q999900001", true),
            ("county", "GB", "GB-WLS", "Q999900002", false),
            ("region", "FR", "GB-WLS", "Q999900001", false),
            ("region", "GB", "GB-ENG", "Q999900001", false),
            ("region", "GB", "GB-WLS", "", false),
            ("locality", "GB", "GB-WLS", "", false),
            ("county", "GB", "GB-WLS", "Q999900001", true),
        ];
        let text: String = rows
            .iter()
            .map(|(kind, cc, region, item, _)| {
                format!(
                    "{kind}\t\t{cc}\t{region}\t0\t100\t1\t2\t1\t2\t1\t2\t\tTwinport\t\t{item}\n"
                )
            })
            .collect();
        let path =
            std::env::temp_dir().join(format!("settlement-twins-{}.tsv", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let (places, _, _) = load_places(&path, &HashMap::new()).unwrap();
        std::fs::remove_file(path).unwrap();
        for (place, row) in places.iter().zip(rows) {
            assert_eq!(place.settlement_twin, row.4, "{row:?}");
        }
    }

    #[test]
    fn subdivisions_are_named_by_the_item_in_the_last_column() {
        let reference: Value =
            serde_json::from_str(include_str!("../../data/reference.json")).unwrap();
        let iso = iso_by_item(&reference, "iso3166_2");
        let listed: usize = reference["iso3166_2"]
            .as_object()
            .unwrap()
            .values()
            .map(|items| items.as_array().unwrap().len())
            .sum();
        assert_eq!(iso.len(), listed, "an item names two codes");
        for (item, code) in &iso {
            assert!(
                item.strip_prefix('Q')
                    .is_some_and(|n| n.parse::<u64>().is_ok())
            );
            assert!(
                code.split_once('-')
                    .is_some_and(|(cc, rest)| cc.len() == 2 && !rest.is_empty())
            );
        }
        let path = std::env::temp_dir().join(format!("places-test-{}.tsv", std::process::id()));
        let (item, code) = iso.iter().next().unwrap();
        // 15 columns, then the item; a place with no names still has its names column
        std::fs::write(
            &path,
            format!(
                "county\t\tUG\tUG-C\t2\t0\t1\t2\t1\t2\t1\t2\t\tKampala\t\t{item}\n\
                 county\t\tUG\tUG-C\t2\t0\t1\t2\t1\t2\t1\t2\t\tMpigi\ten=Mpigi\tQ0\n"
            ),
        )
        .unwrap();
        let (places, own, _) = load_places(&path, &iso).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(places.len(), 2);
        assert_eq!(own, [(0, code.to_string())]);
    }
}
