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
pub use history::reference_sha256 as historical_reference_sha256;
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

mod collect;
pub mod country;
mod fuzzy;
mod house;
mod nearby;
mod parse;
mod place;
mod postcode;
mod search;
mod street;
use collect::*;
use country::{
    aw_cw, cn_tw, co, cz_sk, de, ee, es, hk_mo, ie, it, jp, lt, lv, mx, pl, pt, si, th, tw, xk,
};
pub use fuzzy::*;
pub use house::same_number;
pub use parse::*;
use place::*;
use postcode::*;
mod suggest;
pub use suggest::{Focus, Suggestion};

const SEP: char = '\u{1f}';
const NONE: u32 = u32::MAX;
const STREET: u64 = 1 << 40;
const HOUSE: u64 = 2 << 40;
const POSTCODE: u64 = 3 << 40;
const STREET_CANDIDATES: usize = 20;
/// Terms a union may have to be an `AnyTerm`, which seeks each: with more, tantivy's union,
/// which reads them a window at a time, is the cheaper.
const ANY_TERMS: usize = 16;
/// Decompressed doc store blocks (16 KB each) a segment keeps: tantivy's default of 100 for each
/// of the four segments indexes had before they became one.
const DOC_STORE_BLOCKS: usize = 400;

/// What one request remembers while `Index::memoized` runs on its thread.
#[derive(Default)]
struct Memo {
    /// `Index::matching` results by query (`clause_key`).
    lookups: HashMap<Vec<u8>, Vec<DocAddress>>,
    /// `Parser::parse` results by text: a resolve parses the same text several times, which the
    /// shared parse cache absorbs only while it is on.
    parses: HashMap<Box<str>, Parsed>,
    documents: HashMap<DocAddress, Arc<tantivy::TantivyDocument>>,
}

thread_local! {
    static MEMO: std::cell::RefCell<Option<Memo>> = const { std::cell::RefCell::new(None) };
}

pub use schema::sovereign;
pub use search::part_of_sovereign;

pub struct Index {
    searcher: Searcher,
    suggestion_scope: String,
    /// Fast-field readers per segment, opened once: opening one parses its block index.
    columns: Vec<Columns>,
    /// Uppercase `country` terms -> their number in `Columns::countries`.
    country_codes: HashMap<String, u16>,
    /// Each kind's documents, by `Kind as usize`: the first clause of a lookup of that kind.
    kinds: [OfKind; 3],
    /// Whether the index has `cnames`, which hand-built ones lack: a lookup in one country then
    /// reads its words from there (`in_country`).
    country_words: bool,
    f: Fields,
    /// Memory-mapped, so cold parts stay in the page cache instead of private memory.
    numbers: Vec<tantivy_fst::Map<tantivy::directory::OwnedBytes>>,
    /// Per shard, a second house of a street's number far from the first (`schema::far_file`);
    /// absent from hand-built indexes.
    far: Vec<Option<tantivy_fst::Map<tantivy::directory::OwnedBytes>>>,
    /// GeoNames' other names of places (`schema::ALTERNATES`); absent from older indexes.
    alternates: Option<tantivy_fst::Map<tantivy::directory::OwnedBytes>>,
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
    /// Each code's dependency place (Guernsey, Greenland, Puerto Rico): what a query of only its
    /// name answers, not the country an address there falls back to.
    dependencies: HashMap<String, u32>,
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
    /// `region_spelt`'s region names of a country as words, made when first asked for.
    region_words: Mutex<HashMap<String, RegionWords>>,
    region_named: Mutex<HashMap<String, Option<Box<str>>>>,
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

/// A country's regions' names as words: (region place, words of one name).
type RegionWords = std::sync::Arc<[(u32, Vec<String>)]>;

/// "Vas County" -> "Vas", "Departamento de Santa Cruz" -> "Santa Cruz": words that classify a
/// region rather than name it, which the data's names mostly leave out.
/// Each folded name of the countries (and dependencies) to its country, and each code to
/// itself. A name several carry belongs to the one most of its languages call so: Serbian calls
/// Anguilla "Angola"; the first seen wins a tie.
fn country_names<'a, N: Iterator<Item = &'a str>>(
    countries: impl Iterator<Item = (&'a str, N)>,
) -> HashMap<String, String> {
    let mut votes: HashMap<String, Vec<(&str, usize)>> = HashMap::new();
    let mut codes = vec![];
    for (cc, names) in countries {
        codes.push(cc);
        for name in names {
            let named = votes.entry(fold(name)).or_default();
            match named.iter_mut().find(|(c, _)| *c == cc) {
                Some((_, count)) => *count += 1,
                None => named.push((cc, 1)),
            }
        }
    }
    let mut out: HashMap<String, String> = votes
        .into_iter()
        .filter_map(|(name, named)| {
            let (cc, _) = named.iter().rev().max_by_key(|(_, count)| *count)?;
            Some((name, cc.to_string()))
        })
        .collect();
    out.extend(codes.into_iter().map(|cc| (cc.to_string(), cc.to_string())));
    out
}

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

/// "11-A" and "#108-D", Mexico's way of writing a lettered number, as the index keys it.
fn hyphened_house_suffix(n: &str) -> Option<String> {
    let (number, letter) = n.trim().trim_start_matches('#').split_once('-')?;
    (!number.is_empty()
        && number.bytes().all(|c| c.is_ascii_digit())
        && letter.len() == 1
        && letter.bytes().all(|c| c.is_ascii_alphabetic()))
    .then(|| format!("{number}{letter}"))
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
        let (pc, house, name, source): (&str, &str, &str, &str) =
            serde_json::from_str(value.as_str()?).ok()?;
        if source == schema::TOWN_CITY {
            continue;
        }
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

/// `q` with each union of a few terms (at most `ANY_TERMS`) read as an `AnyTerm`.
fn any_term(q: Box<dyn TQuery>) -> Box<dyn TQuery> {
    let Some(b) = q.downcast_ref::<BooleanQuery>() else {
        return q;
    };
    let terms: Option<Vec<Term>> = b
        .clauses()
        .iter()
        .map(|(occur, q)| {
            (*occur == Occur::Should)
                .then(|| q.downcast_ref::<TermQuery>().map(|t| t.term().clone()))
                .flatten()
        })
        .collect();
    match terms {
        Some(terms)
            if (2..=ANY_TERMS).contains(&terms.len())
                && b.get_minimum_number_should_match() <= 1 =>
        {
            Box::new(AnyTerm(terms))
        }
        _ => Box::new(BooleanQuery::with_minimum_required_clauses(
            b.clauses()
                .iter()
                .map(|(occur, q)| (*occur, any_term(q.box_clone())))
                .collect(),
            b.get_minimum_number_should_match(),
        )),
    }
}

/// A lookup clause as part of its memo key: each term's field and bytes behind their length, in
/// the clauses' structure. Formatting the clauses with `Debug` took 4% of a search.
fn clause_key(occur: Occur, q: &dyn TQuery, out: &mut Vec<u8>) {
    out.push(match occur {
        Occur::Must => b'+',
        Occur::Should => b'?',
        Occur::MustNot => b'-',
    });
    if let Some(t) = q.downcast_ref::<TermQuery>() {
        let bytes = t.term().serialized_value_bytes();
        out.push(b't');
        out.extend(t.term().field().field_id().to_le_bytes());
        out.extend((bytes.len() as u32).to_le_bytes());
        out.extend(bytes);
    } else if let Some(b) = q.downcast_ref::<BooleanQuery>() {
        out.push(b'(');
        out.extend((b.get_minimum_number_should_match() as u32).to_le_bytes());
        for (occur, q) in b.clauses() {
            clause_key(*occur, q.as_ref(), out);
        }
        out.push(b')');
    } else {
        let debug = format!("{q:?}");
        out.push(b'd');
        out.extend((debug.len() as u32).to_le_bytes());
        out.extend(debug.as_bytes());
    }
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
            .doc_store_cache_num_blocks(DOC_STORE_BLOCKS)
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
        let far = (0..shards)
            .map(|s| {
                use tantivy::directory::Directory;
                let name = schema::far_file(s);
                if !dir.join(&name).exists() {
                    return Ok(None);
                }
                let file = mmap.open_read(Path::new(&name))?;
                Ok(Some(tantivy_fst::Map::from(tantivy_fst::raw::Fst::new(
                    file.read_bytes()?,
                )?)))
            })
            .collect::<Result<_>>()?;
        let alternates = if dir.join(schema::ALTERNATES).exists() {
            use tantivy::directory::Directory;
            let file = mmap.open_read(Path::new(schema::ALTERNATES))?;
            Some(tantivy_fst::Map::from(tantivy_fst::raw::Fst::new(
                file.read_bytes()?,
            )?))
        } else {
            None
        };
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
        // One city or town inside is often a slip of the data (Sharjah holds a town).
        let mut held: HashMap<u32, u32> = HashMap::new();
        for p in &places {
            if p.subtype.as_ref() == "locality"
                && matches!(&*p.class, "megacity" | "city" | "town")
                && p.parent != NONE
                && places[p.parent as usize].subtype.as_ref() == "locality"
                && places[p.parent as usize].class.is_empty()
            {
                *held.entry(p.parent).or_default() += 1;
            }
        }
        for (p, _) in held.into_iter().filter(|&(_, n)| n >= 2) {
            places[p as usize].agglomeration = true;
        }
        let localities = locality::Localities::new(
            places
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    p.subtype.as_ref() == "locality" && &*p.class != schema::SMALL_GEONAMES
                })
                .map(|(i, p)| (i as u32, p.x as f64, p.y as f64)),
        );
        let mut countries = HashMap::new();
        let mut regions: HashMap<String, u32> = HashMap::new();
        let mut country_places: HashMap<String, u32> = HashMap::new();
        for (i, p) in places.iter().enumerate() {
            if p.subtype.as_ref() == "country" && !p.cc.is_empty() {
                country_places.entry(p.cc.to_string()).or_insert(i as u32);
            }
            if p.subtype.as_ref() == "region" && !p.region.is_empty() {
                regions.entry(p.region.to_string()).or_insert(i as u32);
            }
        }
        let mut dependencies: HashMap<String, u32> = HashMap::new();
        for (i, p) in places.iter().enumerate() {
            if p.subtype.as_ref() == "dependency" && !p.cc.is_empty() {
                dependencies.entry(p.cc.to_string()).or_insert(i as u32);
            }
        }
        countries.extend(country_names(
            places
                .iter()
                .filter(|p| matches!(&*p.subtype, "country" | "dependency") && !p.cc.is_empty())
                .map(|p| (&*p.cc, p.all_names())),
        ));
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
        for (name, cc) in crate::matching::COUNTRY_ALIASES {
            countries
                .entry(name.to_string())
                .or_insert_with(|| cc.to_string());
        }
        let place_names = suggest::place_names(&places)?;
        let landmarks = landmark::Landmarks::open(dir, places.len())?;
        let mut provinces = HashMap::new();
        for (code, place) in ie::irish_provinces(&places, &regions) {
            provinces.insert(code, places.len() as u32);
            places.push(place);
        }
        let mut country_codes = HashMap::new();
        let columns: Vec<Columns> = searcher
            .segment_readers()
            .iter()
            .map(|r| {
                Columns::open(r, f.country, &mut country_codes, &|id| {
                    places
                        .get(id as usize)
                        .is_some_and(|p| &*p.class == schema::SMALL_GEONAMES)
                })
            })
            .collect::<Result<_>>()?;
        let kinds = [Kind::Place, Kind::Street, Kind::Postcode]
            .map(|kind| OfKind::new(searcher.segment_readers(), &columns, kind));
        let country_words = searcher
            .segment_readers()
            .iter()
            .map(|r| Ok(r.inverted_index(f.cnames)?.terms().num_terms() > 0))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .any(|has| has);
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
            kinds,
            country_words,
            f,
            numbers,
            far,
            alternates,
            reverse,
            areas,
            streets,
            places,
            history,
            localities,
            countries,
            country_places,
            dependencies,
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
            region_words: Mutex::default(),
            region_named: Mutex::default(),
        })
    }

    pub fn places_loaded(&self) -> usize {
        self.places.len()
    }

    fn document(&self, a: DocAddress) -> tantivy::Result<Arc<tantivy::TantivyDocument>> {
        if let Some(doc) =
            MEMO.with_borrow(|memo| memo.as_ref().and_then(|m| m.documents.get(&a).cloned()))
        {
            return Ok(doc);
        }
        let doc = Arc::new(self.searcher.doc::<tantivy::TantivyDocument>(a)?);
        MEMO.with_borrow_mut(|memo| memo.as_mut().map(|m| m.documents.insert(a, doc.clone())));
        Ok(doc)
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

    /// The place of document `a`'s own country, which a street outside every place still has.
    fn doc_country(&self, a: DocAddress) -> Option<u32> {
        let code = self.columns[a.segment_ord as usize].countries[a.doc_id as usize];
        let (cc, _) = self
            .country_codes
            .iter()
            .find(|&(_, &c)| c == code && c != 0)?;
        self.country_places.get(&cc.to_lowercase()).copied()
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
        // ponytail: Palestine is searched in the West Bank (xw) only, not in Gaza (xg); both
        // would need a set of countries here.
        let cc = cc.map(country::ps::country_place);
        let country = match cc.map(|cc| self.country_codes.get(&cc.to_uppercase())) {
            Some(None) => return vec![],
            found => found.flatten().copied(),
        };
        if cl.iter().all(|(occur, _)| *occur != Occur::Must) {
            cl.push(must(Box::new(tantivy::query::AllQuery)));
        }
        let key = MEMO.with_borrow(Option::is_some).then(|| {
            let mut key = vec![kind as u8, any as u8];
            key.extend(country.map_or(u32::MAX, u32::from).to_le_bytes());
            key.extend((n as u64).to_le_bytes());
            for (occur, q) in &cl {
                clause_key(*occur, q.as_ref(), &mut key);
            }
            key
        });
        if let Some(found) = key.as_ref().and_then(|key| {
            MEMO.with_borrow(|memo| memo.as_ref().and_then(|m| m.lookups.get(key).cloned()))
        }) {
            return found;
        }
        if self.country_words
            && let Some(cc) = cc.map(str::to_uppercase).filter(|cc| !cc.is_empty())
        {
            cl = cl
                .iter()
                .map(|(occur, q)| (*occur, self.in_country(q.as_ref(), &cc)))
                .collect();
        }
        cl = cl
            .into_iter()
            .map(|(occur, q)| (occur, any_term(q)))
            .collect();
        cl.insert(0, must(Box::new(self.kinds[kind.runs()].clone())));
        let by = ByImportance {
            n,
            kind,
            country,
            any,
            columns: &self.columns,
        };
        let found = self.searcher.search(&BooleanQuery::new(cl), &by).unwrap();
        if let Some(key) = key {
            MEMO.with_borrow_mut(|memo| {
                memo.as_mut().map(|m| m.lookups.insert(key, found.clone()))
            });
        }
        found
    }

    /// `q` with each `names` and `city` word read from `cnames` and `ccity` for `country` (a
    /// `country` term). It matches the same documents of that country, the only ones a lookup
    /// scoped to it keeps.
    fn in_country(&self, q: &dyn TQuery, country: &str) -> Box<dyn TQuery> {
        if let Some(t) = q.downcast_ref::<TermQuery>()
            && let Some(word) = t.term().value().as_str()
            && let Some(field) = match t.term().field() {
                f if f == self.f.names => Some(self.f.cnames),
                f if f == self.f.city => Some(self.f.ccity),
                _ => None,
            }
        {
            return term(field, &schema::country_word(country, word));
        }
        if let Some(b) = q.downcast_ref::<BooleanQuery>() {
            return Box::new(BooleanQuery::with_minimum_required_clauses(
                b.clauses()
                    .iter()
                    .map(|(occur, q)| (*occur, self.in_country(q.as_ref(), country)))
                    .collect(),
                b.get_minimum_number_should_match(),
            ));
        }
        q.box_clone()
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
        MEMO.set(Some(Memo::default()));
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
}

#[cfg(test)]
mod tests;
