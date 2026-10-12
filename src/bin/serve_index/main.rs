//! Experimental: answers the subset of Nominatim's HTTP API the service and the evaluator use
//! (`/search`, `/details`, `/reverse`, `/status`) from a `build_index` output directory, so the
//! unchanged service can be evaluated against it with `NOMINATIM_URL`.
#[path = "../build_index/canon.rs"]
mod canon;
#[allow(dead_code)]
#[path = "../build_index/index.rs"]
mod index;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::HeaderMap,
    routing::get,
};
use index::{Fields, fold, pc_norm, schema};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex},
};
use tantivy::{
    DocAddress, Index, Searcher, Term,
    query::{BooleanQuery, Occur, Query as TQuery, RangeQuery, TermQuery},
    schema::{Field, IndexRecordOption},
};

const SEP: char = '\u{1f}';
const NONE: u32 = u32::MAX;
const STREET: u64 = 1 << 40;
const HOUSE: u64 = 2 << 40;
const POSTCODE: u64 = 3 << 40;
const STREET_CANDIDATES: usize = 20;

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
    /// Sorted `name_key` hashes of all names, so exact-name checks don't re-fold them.
    keys: Box<[u64]>,
    /// A county that is a city in all but name: Overture has Birmingham only as a county with
    /// a same-named neighbourhood inside, and no locality.
    city_like: bool,
}

impl Place {
    fn named(&self, key: u64) -> bool {
        self.keys.binary_search(&key).is_ok()
    }

    fn name(&self, en: bool) -> &str {
        en.then(|| self.lang("en"))
            .flatten()
            .unwrap_or(&self.primary)
    }

    fn lang(&self, lang: &str) -> Option<&str> {
        self.pairs().find(|(l, _)| *l == lang).map(|(_, n)| n)
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
            "region" => "state",
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
            "county" => 6,
            "localadmin" => 8,
            "locality" => 8,
            _ => 10,
        }
    }
}

fn load_places(path: &str) -> Vec<Place> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    text.lines()
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            let f = |i: usize| c.get(i).copied().unwrap_or("");
            let num = |i: usize| f(i).parse::<f32>().unwrap_or(f32::NAN);
            Place {
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
                keys: Box::default(),
                city_like: false,
            }
        })
        .collect()
}

struct Columns {
    id: tantivy::columnar::Column<u64>,
    place: tantivy::columnar::Column<u64>,
    x: tantivy::columnar::Column<f64>,
    y: tantivy::columnar::Column<f64>,
    importance: tantivy::columnar::Column<u64>,
}

struct ByImportance {
    n: usize,
    columns: Vec<tantivy::columnar::Column<u64>>,
}

struct SegmentTop {
    segment: u32,
    n: usize,
    column: tantivy::columnar::Column<u64>,
    heap: std::collections::BinaryHeap<std::cmp::Reverse<(u64, u32)>>,
}

impl tantivy::collector::Collector for ByImportance {
    type Fruit = Vec<DocAddress>;
    type Child = SegmentTop;

    fn for_segment(&self, segment: u32, _: &tantivy::SegmentReader) -> tantivy::Result<SegmentTop> {
        Ok(SegmentTop {
            segment,
            n: self.n,
            column: self.columns[segment as usize].clone(),
            heap: Default::default(),
        })
    }

    fn requires_scoring(&self) -> bool {
        false
    }

    fn merge_fruits(
        &self,
        fruits: Vec<Vec<(u64, DocAddress)>>,
    ) -> tantivy::Result<Vec<DocAddress>> {
        let mut all: Vec<(u64, DocAddress)> = fruits.into_iter().flatten().collect();
        all.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        Ok(all.into_iter().take(self.n).map(|(_, a)| a).collect())
    }
}

impl tantivy::collector::SegmentCollector for SegmentTop {
    type Fruit = Vec<(u64, DocAddress)>;

    fn collect(&mut self, doc: u32, _: tantivy::Score) {
        let v = self.column.first(doc).unwrap_or(0);
        if self.heap.len() < self.n {
            self.heap.push(std::cmp::Reverse((v, doc)));
        } else if self.heap.peek().is_some_and(|m| m.0.0 < v) {
            self.heap.pop();
            self.heap.push(std::cmp::Reverse((v, doc)));
        }
    }

    fn harvest(self) -> Self::Fruit {
        let segment = self.segment;
        self.heap
            .into_iter()
            .map(|std::cmp::Reverse((v, doc))| (v, DocAddress::new(segment, doc)))
            .collect()
    }
}

struct App {
    searcher: Searcher,
    /// Fast-field readers per segment, opened once: opening one parses its block index.
    columns: Vec<Columns>,
    f: Fields,
    /// Memory-mapped, so cold parts stay in the page cache instead of private memory.
    numbers: Vec<tantivy_fst::Map<tantivy::directory::OwnedBytes>>,
    places: Vec<Place>,
    /// Folded country names and ISO codes -> lowercase country code.
    countries: HashMap<String, String>,
    /// Lowercase country code -> its country place.
    country_places: HashMap<String, u32>,
    /// ISO 3166-2 code -> its region place.
    regions: HashMap<String, u32>,
    postal: Mutex<rustpostal::LibModules>,
    release: String,
}

#[derive(Clone)]
enum Hit {
    Place(u32),
    Street {
        sid: u32,
        place: u32,
        name: String,
        x: f64,
        y: f64,
        number: Option<String>,
        postcode: Option<String>,
        /// The requested city, when the street matched it only through its postal city.
        city: Option<String>,
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
    SUFFIXES.iter().find_map(|s| {
        name.strip_suffix(s)
            .filter(|stem| !stem.is_empty() && !stem.is_ascii())
    })
}

/// Nominatim-style keys (`name:fr`, `alt_name`, `short_name:ja`); repeated keys join with ';'.
/// A CJK name with an administrative suffix also gives its stem as a short name (東京都 -> 東京).
fn namedetails(pl: &Place, names: &mut Map<String, Value>) {
    let mut add = |k: String, n: &str| match names.get_mut(&k) {
        Some(Value::String(v)) => {
            if !v.split(';').any(|x| x == n) {
                v.push(';');
                v.push_str(n);
            }
        }
        Some(_) => {}
        None => {
            names.insert(k, n.into());
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

/// Overture's Irish regions are counties; ISO 3166-2:IE also has the four provinces above them.
fn irish_province(county: &str) -> Option<&'static str> {
    Some(match county.strip_prefix("IE-")? {
        "G" | "LM" | "MO" | "RN" | "SO" => "IE-C",
        "CW" | "D" | "KE" | "KK" | "LS" | "LD" | "LH" | "MH" | "OY" | "WH" | "WX" | "WW" => "IE-L",
        "CE" | "CO" | "KY" | "LK" | "TA" | "WD" => "IE-M",
        "CN" | "DL" | "MN" => "IE-U",
        _ => return None,
    })
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

fn must(q: Box<dyn TQuery>) -> (Occur, Box<dyn TQuery>) {
    (Occur::Must, q)
}

impl App {
    fn open(dir: &str, release: String) -> App {
        let index = Index::open_in_dir(dir).unwrap_or_else(|e| panic!("{dir}: {e}"));
        index.tokenizers().register("fold", index::analyzer());
        let searcher = index.reader().unwrap().searcher();
        let (_, f) = schema();
        let shards: usize = fs::read_to_string(format!("{dir}/numbers.shards"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let mmap = tantivy::directory::MmapDirectory::open(dir).unwrap();
        let numbers = (0..shards)
            .map(|s| {
                use tantivy::directory::Directory;
                let file = mmap
                    .open_read(std::path::Path::new(&format!("numbers-{s:02}.fst")))
                    .unwrap();
                tantivy_fst::Map::from(
                    tantivy_fst::raw::Fst::new(file.read_bytes().unwrap()).unwrap(),
                )
            })
            .collect();
        let mut places = load_places(&format!("{dir}/places.tsv"));
        {
            use rayon::prelude::*;
            places.par_iter_mut().for_each(|p| {
                let mut keys: Vec<u64> = p.all_names().map(name_key).collect();
                keys.sort_unstable();
                keys.dedup();
                p.keys = keys.into();
            });
            // counties containing a same-named neighbourhood, minus those with such a locality
            let mut hood_named: std::collections::HashSet<u32> = Default::default();
            let mut town_named: std::collections::HashSet<u32> = Default::default();
            for p in &places {
                let parent = p.parent;
                if parent == NONE || places[parent as usize].subtype.as_ref() != "county" {
                    continue;
                }
                let same = name_key(&p.primary) == name_key(&places[parent as usize].primary);
                match &*p.subtype {
                    "neighborhood" | "macrohood" if same => hood_named.insert(parent),
                    "locality" | "localadmin" if same => town_named.insert(parent),
                    _ => false,
                };
            }
            for c in hood_named.difference(&town_named) {
                places[*c as usize].city_like = true;
            }
        }
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
        let reference: Value =
            serde_json::from_str(include_str!("../../../data/reference.json")).unwrap();
        for (name, cc) in reference["countries"].as_object().into_iter().flatten() {
            if let Some(cc) = cc.as_str() {
                countries
                    .entry(fold(name))
                    .or_insert_with(|| cc.to_lowercase());
            }
        }
        countries.entry("uae".into()).or_insert_with(|| "ae".into());
        let postal = rustpostal::LibModules::Address;
        postal.setup().expect("libpostal model setup failed");
        let columns = searcher
            .segment_readers()
            .iter()
            .map(|r| {
                let f = r.fast_fields();
                Columns {
                    id: f.u64("id").unwrap(),
                    place: f.u64("place").unwrap(),
                    x: f.f64("x").unwrap(),
                    y: f.f64("y").unwrap(),
                    importance: f.u64("importance").unwrap(),
                }
            })
            .collect();
        App {
            searcher,
            columns,
            f,
            numbers,
            places,
            countries,
            country_places,
            regions,
            postal: Mutex::new(postal),
            release,
        }
    }

    fn parse(&self, q: &str) -> Vec<(String, String)> {
        let _guard = self.postal.lock().unwrap();
        // libpostal returns "İ" decomposed; the tokenizer would split at the combining dot
        let clean = |t: &str| {
            t.chars()
                .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
                .collect()
        };
        rustpostal::address::parse_address(q, None, None)
            .map(|v| {
                v.into_iter()
                    .map(|(l, t)| (l.to_string(), clean(&t)))
                    .collect()
            })
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

    /// The `n` most important matches. TopDocs' fast-field ordering reopens the column of every
    /// segment on every query, which cost more than the search itself.
    fn top(&self, q: &dyn TQuery, n: usize) -> Vec<DocAddress> {
        let by = ByImportance {
            n,
            columns: self.columns.iter().map(|c| c.importance.clone()).collect(),
        };
        self.searcher.search(q, &by).unwrap()
    }

    fn musts(&self, field: Field, text: &str) -> Vec<(Occur, Box<dyn TQuery>)> {
        fold(text)
            .split(' ')
            .filter(|t| !t.is_empty())
            .map(|t| must(term(field, t)))
            .collect()
    }

    fn country(&self, text: &str) -> Option<String> {
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

    /// `places`, and whether the first one is named exactly `text`.
    fn places_exact(
        &self,
        text: &str,
        cc: Option<&str>,
        within: Option<&str>,
        ok: &dyn Fn(&Place) -> bool,
    ) -> (Vec<Hit>, bool) {
        let mut cl = self.musts(self.f.names, text);
        if cl.is_empty() {
            return (vec![], false);
        }
        cl.push(must(term(self.f.kind, "p")));
        if let Some(cc) = cc {
            cl.push(must(term(self.f.country, &cc.to_uppercase())));
        }
        let want = name_key(text);
        let within = within.filter(|w| !fold(w).is_empty()).map(name_key);
        let mut found: Vec<(bool, bool, bool, usize, u32)> = self
            .top(&BooleanQuery::new(cl), 100)
            .into_iter()
            .enumerate()
            .map(|(rank, a)| (rank, self.fast(a, "id") as u32))
            .filter(|(_, id)| ok(&self.places[*id as usize]))
            .map(|(rank, id)| {
                let exact = self.places[id as usize].named(want);
                let inside = within.is_some_and(|w| {
                    self.chain(id)
                        .iter()
                        .skip(1)
                        .any(|&a| self.places[a as usize].named(w))
                });
                // A major city outranks a same-named region, as with Nominatim (Tokyo, New York).
                let pl = &self.places[id as usize];
                let big = pl.subtype.as_ref() == "locality" && pl.population >= 500_000;
                (!exact, !inside, !big, rank, id)
            })
            .collect();
        found.sort();
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
    ) -> Vec<Hit> {
        let base = || {
            let mut cl = self.musts(self.f.names, road);
            cl.push(must(term(self.f.kind, "s")));
            if let Some(cc) = cc {
                cl.push(must(term(self.f.country, &cc.to_uppercase())));
            }
            cl
        };
        if self.musts(self.f.names, road).is_empty() {
            return vec![];
        }
        let mut scopes: Vec<Option<Box<dyn TQuery>>> = vec![];
        if let Some(city) = city {
            let mut any: Vec<(Occur, Box<dyn TQuery>)> = self
                .places(city, cc, None, &|p| {
                    !matches!(&*p.subtype, "country" | "dependency" | "region")
                })
                .into_iter()
                .filter_map(|h| {
                    if let Hit::Place(id) = h {
                        Some(id)
                    } else {
                        None
                    }
                })
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
                .map(|id| (Occur::Should, term(self.f.ids, &id.to_string())))
                .collect();
            let postal = self.musts(self.f.city, city);
            if !postal.is_empty() {
                any.push((Occur::Should, Box::new(BooleanQuery::new(postal))));
            }
            if !any.is_empty() {
                scopes.push(Some(Box::new(BooleanQuery::new(any))));
            }
        }
        if let Some(pc) = postcode.map(pc_norm).filter(|p| !p.is_empty()) {
            scopes.push(Some(term(self.f.pcode, &pc)));
        }
        if city.is_none() && postcode.is_none() {
            scopes.push(None);
        }
        // Streets whose addresses name the requested city as their postal city.
        let postal_city: std::collections::HashSet<DocAddress> = match city {
            Some(c) if !self.musts(self.f.city, c).is_empty() => {
                let mut cl = base();
                cl.push(must(Box::new(BooleanQuery::new(
                    self.musts(self.f.city, c),
                ))));
                self.top(&BooleanQuery::new(cl), 1000).into_iter().collect()
            }
            _ => Default::default(),
        };
        let number = number.map(|n| {
            n.trim()
                .trim_end_matches('号')
                .chars()
                .flat_map(char::to_lowercase)
                .collect::<String>()
        });
        // Streets that hold the requested postcode report it, as Nominatim reports an address's.
        let with_postcode: std::collections::HashSet<DocAddress> =
            match postcode.map(pc_norm).filter(|p| !p.is_empty()) {
                Some(pc) => {
                    let mut cl = base();
                    cl.push(must(term(self.f.pcode, &pc)));
                    self.top(&BooleanQuery::new(cl), 1000).into_iter().collect()
                }
                None => Default::default(),
            };
        let near_postcode = postcode.and_then(|pc| match self.postcodes(pc, cc).first() {
            Some(Hit::Postcode { x, y, .. }) => Some((*x, *y)),
            _ => None,
        });
        for scope in scopes {
            let mut cl = base();
            if let Some(s) = scope {
                cl.push(must(s));
            }
            let mut top = self.top(&BooleanQuery::new(cl), STREET_CANDIDATES);
            if top.is_empty() {
                continue;
            }
            top.sort_by_key(|a| !with_postcode.contains(a));
            // `own`: the house's postcode slot (1-based, into the street's stored list)
            let street = |a: DocAddress, number: Option<String>, (x, y): (f64, f64), own: u8| {
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                let name = stored(&doc, self.f.name);
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
                    .filter(|c| postal_city.contains(&a) && !named(c))
                    .map(title);
                Hit::Street {
                    sid: self.fast(a, "id") as u32,
                    place,
                    name,
                    x,
                    y,
                    number,
                    postcode,
                    city,
                }
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
                for &a in &top {
                    let sid = self.fast(a, "id") as u32;
                    key.clear();
                    key.extend_from_slice(&sid.to_be_bytes());
                    key.extend_from_slice(n.as_bytes());
                    if let Some(v) = self.numbers[sid as usize % self.numbers.len()].get(&key) {
                        let (x, y, own) = index::unpack(v);
                        // like Nominatim, a found house is the only answer
                        return vec![street(a, Some(n.to_string()), (x, y), own)];
                    }
                }
            }
            let a = top[0];
            let out = vec![street(
                a,
                None,
                (self.fast_f64(a, "x"), self.fast_f64(a, "y")),
                0,
            )];
            return out;
        }
        vec![]
    }

    fn postcodes(&self, pc: &str, cc: Option<&str>) -> Vec<Hit> {
        let pc = pc_norm(pc);
        if pc.is_empty() {
            return vec![];
        }
        let mut cl = vec![must(term(self.f.kind, "z")), must(term(self.f.pcode, &pc))];
        if let Some(cc) = cc {
            cl.push(must(term(self.f.country, &cc.to_uppercase())));
        }
        self.top(&BooleanQuery::new(cl), 5)
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
        let mut cl = vec![must(term(self.f.kind, "z")), must(term(self.f.pcode, norm))];
        if let Some(cc) = cc {
            cl.push(must(term(self.f.country, &cc.to_uppercase())));
        }
        match self.top(&BooleanQuery::new(cl), 1).first() {
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

    fn postcode_exists(&self, pc: &str, cc: Option<&str>) -> bool {
        let mut cl = vec![
            must(term(self.f.kind, "z")),
            must(term(self.f.pcode, &pc_norm(pc))),
        ];
        if let Some(cc) = cc {
            cl.push(must(term(self.f.country, &cc.to_uppercase())));
        }
        self.searcher
            .search(&BooleanQuery::new(cl), &tantivy::collector::Count)
            .unwrap_or(0)
            > 0
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

    /// `<cc>, <ISO 3166-2 code>`, as the service asks for an unknown region.
    fn region_query(&self, q: &str) -> Option<u32> {
        let (cc, code) = q.split_once(", ")?;
        let code = code.trim().to_uppercase();
        (cc.trim().len() == 2 && code.starts_with(&format!("{}-", cc.trim().to_uppercase())))
            .then(|| self.regions.get(&code).copied())
            .flatten()
    }

    fn search(&self, q: &str, en: bool) -> Vec<Value> {
        let q = q.trim();
        if q.is_empty() {
            return vec![];
        }
        if let Some(p) = self.region_query(q) {
            return vec![self.json(&Hit::Place(p), en)];
        }
        // "Birmingham, Birmingham, United Kingdom, United Kingdom": repeated parts parse as one
        // doubled value ("united kingdom united kingdom") that names nothing.
        let mut seen = std::collections::HashSet::new();
        let deduped: Vec<&str> = q
            .split(',')
            .map(str::trim)
            .filter(|p| seen.insert(p.to_lowercase()))
            .collect();
        let deduped = deduped.join(", ");
        let q = deduped.as_str();
        let mut parsed = self.parse(q);
        let first_cc = parsed
            .iter()
            .find(|(l, _)| l == "country")
            .and_then(|(_, c)| self.country(c));
        // libpostal misreads postcodes it doesn't know (Eircodes, "3011 AD"); take one the
        // index holds and parse the rest without it.
        let stated = parsed
            .iter()
            .find(|(l, _)| l == "postcode")
            .map(|(_, v)| pc_norm(v));
        if let Some((pc, rest)) = self.find_postcode(q, first_cc.as_deref())
            && stated.as_ref().is_none_or(|s| {
                pc_norm(&pc).len() > s.len() && pc_norm(&pc).starts_with(s.as_str())
            })
        {
            parsed = self.parse(&rest);
            parsed.push(("postcode".into(), pc));
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
        // A query that only names a country ("Malta", "North Macedonia") means the country.
        if let Some(c) = self.country(q)
            && let Some(&p) = self.country_places.get(&c)
        {
            return vec![self.json(&Hit::Place(p), en)];
        }
        let mut hits = vec![];
        let roads: Vec<&str> = parsed
            .iter()
            .filter(|(l, _)| l == "road")
            .map(|(_, v)| v.as_str())
            .collect();
        if let Some(&road) = roads.last() {
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
                get("house_number"),
                resolved_city.as_deref(),
                get("postcode"),
                cc,
            );
        }
        // "Santiago, Metropolitana, Chile" parses "santiago" as a road: without a house number,
        // an unmatched road is tried as a place name too.
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
                .filter(|t| !(cc.is_some() && self.country(t).as_deref() == cc))
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
            let tries = (texts.len() > 1)
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
        hits.iter().take(10).map(|h| self.json(h, en)).collect()
    }

    fn address(&self, place: u32, en: bool, a: &mut Map<String, Value>, display: &mut Vec<String>) {
        let mut code = None;
        for p in self.chain(place) {
            let pl = &self.places[p as usize];
            let key = pl.key();
            if !a.contains_key(key) {
                a.insert(key.into(), pl.name(en).into());
                display.push(pl.name(en).into());
            }
            if code.is_none() && !pl.region.is_empty() {
                code = Some(pl.region.to_string());
            }
            if !pl.cc.is_empty() {
                a.entry("country_code")
                    .or_insert_with(|| pl.cc.to_string().into());
            }
        }
        // Nominatim lists a dependency (Svalbard, Puerto Rico) as a state as well.
        if let Some(dep) = self
            .chain(place)
            .into_iter()
            .find(|&p| self.places[p as usize].subtype.as_ref() == "dependency")
        {
            a.entry("state")
                .or_insert_with(|| self.places[dep as usize].name(en).into());
        }
        if let Some(code) = code {
            if let Some(province) = irish_province(&code) {
                a.insert("ISO3166-2-lvl5".into(), province.into());
            }
            a.insert("ISO3166-2-lvl4".into(), code.into());
        }
    }

    fn json(&self, hit: &Hit, en: bool) -> Value {
        let mut a = Map::new();
        let mut display: Vec<String> = vec![];
        let (id, osm_type, class, typ, addresstype, x, y, bbox, namedetails) = match hit {
            Hit::Place(p) => {
                let pl = &self.places[*p as usize];
                let mut names = Map::new();
                names.insert("name".into(), pl.primary.to_string().into());
                namedetails(pl, &mut names);
                self.address(*p, en, &mut a, &mut display);
                let admin = matches!(
                    &*pl.subtype,
                    "country" | "dependency" | "region" | "county" | "localadmin"
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
            } => {
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
                    self.address(*place, en, &mut a, &mut display);
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
                    self.address(*place, en, &mut a, &mut display);
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

    fn details(&self, id: u64) -> Value {
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

    /// Nearest locality point within about 50 km.
    fn reverse(&self, lat: f64, lon: f64) -> Value {
        for d in [0.05, 0.5] {
            let range = |f: Field, v: f64| -> Box<dyn TQuery> {
                Box::new(RangeQuery::new(
                    std::ops::Bound::Included(Term::from_field_f64(f, v - d)),
                    std::ops::Bound::Included(Term::from_field_f64(f, v + d)),
                ))
            };
            let q = BooleanQuery::new(vec![
                must(term(self.f.kind, "p")),
                must(range(self.f.x, lon)),
                must(range(self.f.y, lat)),
            ]);
            let best = self
                .top(&q, 500)
                .into_iter()
                .map(|a| self.fast(a, "id") as u32)
                .filter(|&p| self.places[p as usize].subtype.as_ref() == "locality")
                .min_by(|&a, &b| {
                    let dist = |p: u32| {
                        let pl = &self.places[p as usize];
                        (pl.x as f64 - lon).powi(2) + (pl.y as f64 - lat).powi(2)
                    };
                    dist(a).total_cmp(&dist(b))
                });
            if let Some(p) = best {
                return self.json(&Hit::Place(p), true);
            }
        }
        json!({"error": "Unable to geocode"})
    }
}

type Params = Query<HashMap<String, String>>;

fn english(headers: &HeaderMap) -> bool {
    headers
        .get("accept-language")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("en"))
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(dir), Some(addr)) = (args.get(1), args.get(2)) else {
        eprintln!("usage: serve_index <index-dir> <listen-addr> [release-label]");
        std::process::exit(2);
    };
    let release = args.get(3).cloned().unwrap_or_else(|| "unknown".into());
    let t = std::time::Instant::now();
    let app = Arc::new(App::open(dir, release));
    eprintln!(
        "loaded {} places in {:.1}s",
        app.places.len(),
        t.elapsed().as_secs_f64()
    );
    let blocking = |f: fn(&App, &HashMap<String, String>, bool) -> Value| {
        move |State(app): State<Arc<App>>, headers: HeaderMap, Query(p): Params| async move {
            let en = english(&headers);
            Json(
                tokio::task::spawn_blocking(move || f(&app, &p, en))
                    .await
                    .unwrap(),
            )
        }
    };
    let router = Router::new()
        .route(
            "/search",
            get(blocking(|app, p, en| {
                let q = p.get("q").map(String::as_str).unwrap_or("");
                Value::Array(app.search(q, en))
            })),
        )
        .route("/details", get(blocking(|app, p, _| app.details(p.get("place_id").and_then(|v| v.parse().ok()).unwrap_or(u64::MAX)))))
        .route(
            "/reverse",
            get(blocking(|app, p, _| {
                let f = |k: &str| p.get(k).and_then(|v| v.parse::<f64>().ok());
                match (f("lat"), f("lon")) {
                    (Some(lat), Some(lon)) => app.reverse(lat, lon),
                    _ => json!({"error": "lat and lon required"}),
                }
            })),
        )
        .route(
            "/parse",
            get(blocking(|app, p, _| {
                json!(app.parse(p.get("q").map(String::as_str).unwrap_or("")))
            })),
        )
        .route(
            "/status",
            get(blocking(|app, _, _| {
                json!({"status": 0, "message": "OK", "data_updated": app.release, "software_version": "overture-index", "database_version": "experimental"})
            })),
        )
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, router).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
