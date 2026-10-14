//! Completion of a place, street, house or postcode typed so far. Unlike `search`, it never
//! calls libpostal, which misreads half-typed words: the last word is a prefix of terms read
//! from the index's term dictionary, and the commas the user types say where the town starts.
use super::*;
use std::{cmp::Reverse, collections::BinaryHeap};
use tantivy_fst::{IntoStreamer, Streamer};

/// Terms a prefix expands to, most common first: each is a posting list to walk.
const PREFIX_TERMS: usize = 32;
/// Candidate places fetched before a qualifier ("Springfield, IL") filters them.
const POOL: usize = 400;
/// A street search whose rarest word is in more documents than this walks too many postings to
/// answer in time. Without a town its answer would be arbitrary as well ("rue de", "main st"),
/// so it is held to less than one in a town.
const MAX_WORK: u64 = 60_000;
const MAX_WORK_IN_TOWN: u64 = 400_000;
/// The same for the long tail of places the name index does not hold: found by an exact word
/// that few documents share.
const MAX_PLACE_WORK: u64 = 20_000;
const MAX_TEXT: usize = 200;
/// Entries of each prefix cache: a place entry is at most 400 candidates of 16 bytes, so the
/// caches stay under about 150 MB however much is typed.
const MAX_CACHED: usize = 20_000;
const MAX_NUMBER: usize = 10;
/// Words of a postcode needed before suggesting one: "75" matches thousands.
const MIN_POSTCODE: usize = 3;

/// A prefix's most common terms and the documents they are in.
pub(super) type PrefixTerms = (Arc<[String]>, u64);
/// Place, fit and score of each place a name may mean.
pub(super) type Candidates = Arc<[(u32, u8, u64)]>;
type Clauses = Vec<(Occur, Box<dyn TQuery>)>;

pub struct Suggestion {
    /// Opaque; a place, street, house or postcode of this index.
    pub id: String,
    pub display_name: String,
    pub kind: &'static str,
    pub country_code: String,
    pub x: f64,
    pub y: f64,
    /// Places only: min lon, min lat, max lon, max lat.
    pub bbox: Option<[f32; 4]>,
}

fn has_digit(w: &str) -> bool {
    w.chars().any(|c| c.is_ascii_digit())
}

thread_local! {
    /// `fold` without the street-type abbreviations: the word being typed is not yet one ("c"
    /// is not "calle").
    static RAW: std::cell::RefCell<tantivy::tokenizer::TextAnalyzer> = std::cell::RefCell::new(
        tantivy::tokenizer::TextAnalyzer::builder(tantivy::tokenizer::SimpleTokenizer::default())
            .filter(tantivy::tokenizer::RemoveLongFilter::limit(40))
            .filter(tantivy::tokenizer::LowerCaser)
            .filter(tantivy::tokenizer::AsciiFoldingFilter)
            .build(),
    );
}

fn raw_words(text: &str) -> Vec<String> {
    use tantivy::tokenizer::TokenStream;
    RAW.with(|a| {
        let mut a = a.borrow_mut();
        let mut out = vec![];
        let mut ts = a.token_stream(text);
        while ts.advance() {
            out.push(ts.token().text.clone());
        }
        out
    })
}

/// The words of `text`, the last one as typed when it may be unfinished.
fn words_open(text: &str, open: bool) -> Vec<String> {
    if !open {
        return words(text);
    }
    let (before, last) = text.rsplit_once(' ').unwrap_or(("", text));
    let mut out = words(before);
    out.extend(raw_words(last));
    out
}

/// `text` as typed, and with its last word as each abbreviation it starts: "saint lo" is
/// "st lo" in the index.
fn open_forms(words: &[String]) -> Vec<String> {
    let mut forms = vec![words.join(" ")];
    if let Some((last, before)) = words.split_last().filter(|(l, _)| l.len() >= 2) {
        for (full, short) in canon::VARIANTS {
            if full.starts_with(last.as_str()) {
                let form = before
                    .iter()
                    .map(String::as_str)
                    .chain([*short])
                    .collect::<Vec<_>>()
                    .join(" ");
                if !forms.contains(&form) {
                    forms.push(form);
                }
            }
        }
    }
    forms
}

fn words(text: &str) -> Vec<String> {
    fold(text)
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// How likely a place is the one wanted: its population, or for places the data gives none (most
/// hamlets, many regions) a guess by kind. Countries are all alike, and an area is not more
/// likely than the city it is named for (Tokyo, New York).
fn weight(p: &Place) -> u64 {
    let pop = p.population as u64;
    match &*p.subtype {
        "country" | "dependency" => 50_000_000,
        "region" | "macroregion" => pop.clamp(300_000, 1_000_000),
        "county" | "macrocounty" => pop.clamp(10_000, 100_000),
        "localadmin" => pop.max(10_000),
        "locality" => pop.max(match &*p.class {
            "megacity" => 10_000_000,
            "city" => 50_000,
            "town" => 5_000,
            _ => 100,
        }),
        _ => pop.max(500),
    }
}

fn place_kind(p: &Place) -> &'static str {
    match p.key() {
        "state" => "region",
        "suburb" | "neighbourhood" => "neighbourhood",
        other => other,
    }
}

/// Places lighter than this are left to the term dictionary: only an exact word finds them.
const MIN_WEIGHT: u64 = 5_000;
/// Candidates kept for a prefix: enough for a qualifier ("Springfield, IL") to pick from.
const CANDIDATES: usize = 400;
/// How much a name's fit counts: the name itself, its main name starts with the text, another
/// language's does.
const FIT: [u64; 3] = [100, 20, 5];

/// Folded name, a NUL, whether it is the primary or English name (0) or another (1) and the
/// place number -> the place's weight. One entry per place and name, so places sharing a name
/// have distinct keys and a prefix is a range.
pub(super) fn place_names(places: &[Place]) -> Result<tantivy_fst::Map<Vec<u8>>> {
    use rayon::prelude::*;
    let mut keys: Vec<(Vec<u8>, u64)> = places
        .par_iter()
        .enumerate()
        .flat_map_iter(|(id, p)| {
            let w = weight(p);
            let mut names: Vec<(String, u8)> = vec![];
            if w >= MIN_WEIGHT {
                let main = [&*p.primary, p.name(true)];
                for n in p.all_names() {
                    let flag = u8::from(!main.contains(&n));
                    let n = fold(n);
                    match names.iter_mut().find(|(m, _)| *m == n) {
                        Some(m) => m.1 = m.1.min(flag),
                        None if !n.is_empty() => names.push((n, flag)),
                        None => {}
                    }
                }
            }
            names.into_iter().map(move |(n, flag)| {
                let mut key = n.into_bytes();
                key.extend_from_slice(&[0, flag]);
                key.extend_from_slice(&(id as u32).to_be_bytes());
                (key, w)
            })
        })
        .collect();
    keys.par_sort_unstable();
    let mut build = tantivy_fst::MapBuilder::memory();
    for (key, w) in keys {
        build.insert(key, w)?;
    }
    Ok(tantivy_fst::Map::from_bytes(build.into_inner()?)?)
}

/// The text as typed, split at the commas, and whether its last word may be unfinished.
struct Typed {
    parts: Vec<String>,
    open: bool,
}

impl Typed {
    fn new(text: &str) -> Self {
        Self {
            parts: text
                .split(',')
                .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|p| !p.is_empty())
                .collect(),
            open: !text.ends_with(|c: char| c == ',' || c.is_whitespace()),
        }
    }
}

impl Index {
    /// Up to `limit` completions of `text`, best first. Blocks: call it off the async executor.
    pub fn suggest(&self, text: &str, limit: usize, cc: Option<&str>) -> Vec<Suggestion> {
        let text = text.trim_start();
        if text.chars().count() > MAX_TEXT || text.trim().is_empty() || limit == 0 {
            return vec![];
        }
        let typed = Typed::new(text);
        let Some(head) = typed.parts.first() else {
            return vec![];
        };
        let head_words: Vec<&str> = head.split(' ').collect();
        let one_part = typed.parts.len() == 1;
        if one_part && head.chars().count() < 2 {
            return vec![];
        }
        let coded = head_words.iter().any(|w| has_digit(w));
        // "75001", or "SW1A 2": a postcode, whose words may be split by a space
        if one_part && coded && head_words.len() <= 2 {
            let found = self.suggest_postcodes(head, typed.open, cc, limit);
            if !found.is_empty() || head_words.len() == 1 {
                return found;
            }
        }
        let places = if coded {
            vec![]
        } else {
            self.suggest_places(&typed.parts, typed.open, cc, limit)
        };
        // a place whose name only contains the words (a "Main Street Historic District") comes
        // after the streets that are named so
        let (named, contains): (Vec<_>, Vec<_>) = places.into_iter().partition(|&(id, fit)| {
            fit == 0 || fit <= 2 && weight(&self.places[id as usize]) >= 5_000
        });
        let mut out: Vec<Suggestion> = named
            .into_iter()
            .map(|(id, _)| self.place_suggestion(id))
            .collect();
        if out.len() < limit {
            out.extend(self.suggest_streets(&typed, cc, limit - out.len()));
        }
        out.extend(
            contains
                .into_iter()
                .map(|(id, _)| self.place_suggestion(id)),
        );
        let mut seen = std::collections::HashSet::new();
        out.retain(|s| seen.insert(s.display_name.clone()));
        out.truncate(limit);
        out
    }

    /// The most common terms of `field` starting with `prefix`, summed over segments, and the
    /// documents they are in. Reading them is the cost of a short prefix, so the answer is kept.
    fn prefix_terms(&self, field: Field, prefix: &str) -> PrefixTerms {
        if prefix.is_empty() || prefix.len() > MAX_WORD {
            return (Arc::new([]), 0);
        }
        let key = (field.field_id(), Box::<str>::from(prefix));
        if let Some(hit) = self.prefixes.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let mut sum: HashMap<Vec<u8>, u32> = HashMap::new();
        for segment in self.searcher.segment_readers() {
            let Ok(inverted) = segment.inverted_index(field) else {
                continue;
            };
            let mut after = prefix.as_bytes().to_vec();
            *after.last_mut().unwrap() += 1;
            let Ok(mut terms) = inverted
                .terms()
                .range()
                .ge(prefix.as_bytes())
                .lt(&after)
                .into_stream()
            else {
                continue;
            };
            // the segment's top terms only, so a prefix of 200,000 terms allocates for few
            let mut top: BinaryHeap<Reverse<(u32, Vec<u8>)>> = BinaryHeap::new();
            while terms.advance() {
                let df = terms.value().doc_freq;
                if top.len() < PREFIX_TERMS || top.peek().is_some_and(|t| t.0.0 < df) {
                    if top.len() == PREFIX_TERMS {
                        top.pop();
                    }
                    top.push(Reverse((df, terms.key().to_vec())));
                }
            }
            for Reverse((df, term)) in top {
                *sum.entry(term).or_default() += df;
            }
        }
        let mut sum: Vec<_> = sum.into_iter().collect();
        sum.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        sum.truncate(PREFIX_TERMS);
        let docs = sum.iter().map(|(_, df)| *df as u64).sum();
        let terms: Arc<[String]> = sum
            .into_iter()
            .filter_map(|(t, _)| String::from_utf8(t).ok())
            .collect();
        let mut cache = self.prefixes.lock().unwrap();
        // ponytail: dropped whole when full, not LRU; the short prefixes that cost most refill first
        if cache.len() >= MAX_CACHED {
            cache.clear();
        }
        cache.insert(key, (terms.clone(), docs));
        (terms, docs)
    }

    fn docs_with(&self, field: Field, term_text: &str) -> u64 {
        self.searcher
            .doc_freq(&Term::from_field_text(field, term_text))
            .unwrap_or(0)
    }

    /// Clauses requiring every word of `words`, the last one only as far as typed when `open`,
    /// and the number of documents the rarest word is in, which bounds the search's work.
    /// `None` when no term can match.
    fn typed_words(&self, field: Field, words: &[String], open: bool) -> Option<(Clauses, u64)> {
        // one letter of a word expands to a third of the dictionary: wait for the next
        let (words, open) = match words.split_last() {
            Some((last, whole)) if open && last.chars().count() < 2 && !whole.is_empty() => {
                (whole, false)
            }
            _ => (words, open),
        };
        let (last, whole) = words.split_last()?;
        let mut cl: Vec<_> = whole.iter().map(|w| must(term(field, w))).collect();
        let mut rarest = whole
            .iter()
            .map(|w| self.docs_with(field, w))
            .min()
            .unwrap_or(u64::MAX);
        if !open {
            cl.push(must(term(field, last)));
            return Some((cl, rarest.min(self.docs_with(field, last))));
        }
        let (found, mut docs) = self.prefix_terms(field, last);
        let mut terms = found.to_vec();
        // "aven" is the start of "avenue", which the index holds as "av"
        if last.len() >= 2 {
            for (full, short) in canon::VARIANTS {
                if full.starts_with(last.as_str()) && !terms.iter().any(|t| t == short) {
                    docs += self.docs_with(field, short);
                    terms.push(short.to_string());
                }
            }
        }
        match terms.as_slice() {
            [] => return None,
            [one] => cl.push(must(term(field, one))),
            many => cl.push(must(Box::new(BooleanQuery::new(
                many.iter()
                    .map(|t| (Occur::Should, term(field, t)))
                    .collect(),
            )))),
        }
        rarest = rarest.min(docs);
        Some((cl, rarest))
    }

    /// Country codes of the countries a qualifier names, or starts to.
    fn countries_named(&self, q: &str, open: bool) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .countries
            .iter()
            .filter(|(name, _)| *name == q || (open && q.len() >= 3 && name.starts_with(q)))
            .map(|(_, cc)| cc.as_str())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Whether `place` lies in what each qualifier names: a country, a region by name or code,
    /// or any larger place around it.
    fn qualifies(&self, place: u32, quals: &[(String, Vec<&str>)], open_last: bool) -> bool {
        let pl = &self.places[place as usize];
        let chain = self.chain(place);
        let matches = |name: &str, q: &str, open: bool| {
            let n = fold(name);
            n == q || (open && q.len() >= 3 && n.starts_with(q))
        };
        quals.iter().enumerate().all(|(i, (q, ccs))| {
            let open = open_last && i + 1 == quals.len();
            let code = format!("-{}", q.to_uppercase());
            ccs.contains(&&*pl.cc)
                || (q.len() <= 3 && pl.region.ends_with(&code))
                || chain.iter().skip(1).any(|&a| {
                    let anc = &self.places[a as usize];
                    matches(&anc.primary, q, open)
                        || matches(anc.name(true), q, open)
                        || (q.len() <= 3 && anc.region.ends_with(&code))
                })
        })
    }

    /// The places of `place_names` whose name is `text`, or starts with it when `open`, best
    /// first: place, fit (0 the name itself, 1 its main name starts with the text, 2 another
    /// language's does) and weight. Reading a short prefix's range is the cost, so it is kept.
    fn name_candidates(&self, text: &str, open: bool) -> Candidates {
        let key = (Box::<str>::from(text), open);
        if let Some(hit) = self.place_prefixes.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let (from, to) = if open {
            let mut to = text.as_bytes().to_vec();
            *to.last_mut().unwrap() += 1;
            (text.as_bytes().to_vec(), to)
        } else {
            (
                [text.as_bytes(), &[0]].concat(),
                [text.as_bytes(), &[1]].concat(),
            )
        };
        let mut best: HashMap<u32, (u64, u8)> = HashMap::new();
        let mut range = self.place_names.range().ge(&from).lt(&to).into_stream();
        while let Some((k, w)) = range.next() {
            let at = k.len() - 6;
            let id = u32::from_be_bytes(k[at + 2..].try_into().unwrap());
            let fit = if at == text.len() { 0 } else { 1 + k[at + 1] };
            let score = w * FIT[fit as usize];
            let e = best.entry(id).or_insert((score, fit));
            if score > e.0 {
                *e = (score, fit);
            }
        }
        let mut all: Vec<(u32, u8, u64)> = best
            .into_iter()
            .map(|(id, (s, fit))| (id, fit, s))
            .collect();
        all.sort_unstable_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        all.truncate(CANDIDATES);
        let all: Candidates = all.into();
        let mut cache = self.place_prefixes.lock().unwrap();
        // ponytail: dropped whole when full, not LRU
        if cache.len() >= MAX_CACHED {
            cache.clear();
        }
        cache.insert(key, all.clone());
        all
    }

    /// Places named by `parts` (the first part's name, then what it lies in), best first, with
    /// how well each name fits: 0 is a name of the place, 1 its main name (local or English)
    /// starts with the text, 2 a name in another language does, 3 a name has it inside.
    fn suggest_places(
        &self,
        parts: &[String],
        open: bool,
        cc: Option<&str>,
        want: usize,
    ) -> Vec<(u32, u8)> {
        let Some(first) = parts.first() else {
            return vec![];
        };
        let all = words_open(first, open && parts.len() == 1);
        // "paris fra": the whole text names a place, or its front does and its end says where
        let splits: Vec<usize> = if parts.len() == 1 {
            std::iter::once(all.len())
                .chain((1..all.len()).rev().take(3))
                .collect()
        } else {
            vec![all.len()]
        };
        let mut found: Vec<(u32, u8)> = vec![];
        for (n, split) in splits.into_iter().enumerate() {
            if n > 0 && found.len() >= want {
                break;
            }
            let quals: Vec<String> = if split < all.len() {
                vec![all[split..].join(" ")]
            } else {
                parts[1..].iter().map(|p| fold(p)).collect()
            };
            let head = &all[..split];
            let head_open = open && quals.is_empty();
            let pool = if quals.is_empty() {
                want.max(10) * 4
            } else {
                POOL
            };
            let quals: Vec<(String, Vec<&str>)> = quals
                .into_iter()
                .map(|q| {
                    let ccs = self.countries_named(&q, open);
                    (q, ccs)
                })
                .collect();
            let text = head.join(" ");
            let acceptable = |id: u32| {
                let pl = &self.places[id as usize];
                pl.subtype.as_ref() != "microhood"
                    && cc.is_none_or(|c| pl.cc.eq_ignore_ascii_case(c))
                    && (quals.is_empty() || self.qualifies(id, &quals, open))
            };
            let mut ranked: Vec<(Reverse<u64>, u8, u32)> = vec![];
            let forms = if head_open {
                open_forms(head)
            } else {
                vec![text.clone()]
            };
            let mut candidates: Vec<(u32, u8, u64)> = vec![];
            for form in &forms {
                candidates.extend(self.name_candidates(form, head_open).iter());
            }
            candidates.sort_unstable_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
            // candidates come best first, so the first `want` that qualify are the best
            for (id, fit, score) in candidates {
                if ranked.len() >= want {
                    break;
                }
                if !found.iter().any(|f| f.0 == id)
                    && !ranked.iter().any(|r| r.2 == id)
                    && acceptable(id)
                {
                    ranked.push((Reverse(score), fit, id));
                }
            }
            // The rest of the places, by the words the term dictionary holds: those as typed
            // first, for the places a prefix adds ("paris" -> "parish") are many.
            if ranked.len() < want {
                let want_name = name_key(&text);
                let mut ids: Vec<u32> = vec![];
                for as_prefix in [false, true] {
                    if as_prefix && !head_open {
                        break;
                    }
                    let Some((cl, rarest)) = self.typed_words(self.f.names, head, as_prefix) else {
                        continue;
                    };
                    if rarest > MAX_PLACE_WORK {
                        continue;
                    }
                    for a in self.top_of(Kind::Place, cc, cl, pool) {
                        let id = self.fast(a, "id") as u32;
                        if !ids.contains(&id)
                            && !found.iter().any(|f| f.0 == id)
                            && !ranked.iter().any(|r| r.2 == id)
                        {
                            ids.push(id);
                        }
                    }
                }
                for id in ids.into_iter().filter(|&id| acceptable(id)) {
                    let pl = &self.places[id as usize];
                    let starts = |n: &str| fold(n).starts_with(&text);
                    let fit = if pl.named(want_name) {
                        0
                    } else if starts(&pl.primary) || starts(pl.name(true)) {
                        1
                    } else if pl.all_names().any(starts) {
                        2
                    } else {
                        3
                    };
                    // the name fits less as less of it is typed
                    let score = weight(pl) * [100, 20, 5, 1][fit as usize];
                    ranked.push((Reverse(score), fit, id));
                }
            }
            ranked.sort_unstable();
            found.extend(ranked.into_iter().map(|(_, fit, id)| (id, fit)));
        }
        found.truncate(want);
        found
    }

    fn place_suggestion(&self, id: u32) -> Suggestion {
        let pl = &self.places[id as usize];
        let mut a = Map::new();
        let mut display = vec![];
        self.address(id, true, &mut a, &mut display);
        Suggestion {
            id: format!("p{id}"),
            display_name: display.join(", "),
            kind: place_kind(pl),
            country_code: pl.cc.to_string(),
            x: pl.x as f64,
            y: pl.y as f64,
            // most towns' boxes are the point itself, which zooms a map no better than the point
            bbox: (pl.bbox.iter().all(|v| v.is_finite())
                && (pl.bbox[2] - pl.bbox[0]).max(pl.bbox[3] - pl.bbox[1]) > 1e-3)
                .then_some(pl.bbox),
        }
    }

    fn suggest_postcodes(
        &self,
        text: &str,
        open: bool,
        cc: Option<&str>,
        limit: usize,
    ) -> Vec<Suggestion> {
        let pc = pc_norm(text);
        if pc.len() < MIN_POSTCODE {
            return vec![];
        }
        let cl = if open {
            self.typed_words(self.f.pcode, &[pc], true)
                .map(|(cl, _)| cl)
        } else {
            Some(vec![must(term(self.f.pcode, &pc))])
        };
        let Some(cl) = cl else {
            return vec![];
        };
        self.top_of(Kind::Postcode, cc, cl, limit * 2)
            .into_iter()
            .filter_map(|a| {
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
                let (place, x, y) = (
                    self.fast(a, "place") as u32,
                    self.fast_f64(a, "x"),
                    self.fast_f64(a, "y"),
                );
                let code = stored(&doc, self.f.name);
                let mut display = vec![code];
                let mut details = Map::new();
                let mut country_code = String::new();
                if place != NONE {
                    self.address(place, true, &mut details, &mut display);
                    country_code = self.places[place as usize].cc.to_string();
                }
                Some(Suggestion {
                    id: format!("z{}:{}", country_code, pc_norm(&stored(&doc, self.f.pcode))),
                    display_name: display.join(", "),
                    kind: "postcode",
                    country_code,
                    x,
                    y,
                    bbox: None,
                })
            })
            .take(limit)
            .collect()
    }

    /// Streets, and houses when a number is typed, with the road in the first part and the
    /// town after a comma ("12 main st, spring"). With no comma the town may still end the text.
    fn suggest_streets(&self, typed: &Typed, cc: Option<&str>, want: usize) -> Vec<Suggestion> {
        let raw: Vec<&str> = typed.parts[0].split(' ').collect();
        let one_part = typed.parts.len() == 1;
        // the number: the first or last word, when a road remains
        let number_at = (raw.len() > 1)
            .then(|| {
                [0, raw.len() - 1]
                    .into_iter()
                    .find(|&i| has_digit(raw[i]) && raw[i].len() <= MAX_NUMBER)
            })
            .flatten();
        let mut tries: Vec<(Option<usize>, Vec<&str>)> = vec![];
        if let Some(i) = number_at {
            tries.push((
                Some(i),
                raw.iter()
                    .enumerate()
                    .filter(|&(j, _)| j != i)
                    .map(|(_, w)| *w)
                    .collect(),
            ));
        }
        // "route 66": a digit word that is part of the road
        tries.push((None, raw.clone()));
        let mut out = vec![];
        for (number, road) in tries {
            let number_word = number.map(|i| raw[i]);
            let number_open = typed.open && one_part && number == Some(raw.len() - 1);
            let road_open = typed.open && one_part && !number_open;
            let road_words = words_open(&road.join(" "), road_open);
            if road_words.is_empty() {
                continue;
            }
            let found = if one_part {
                // "main st springfield": the end of the text may name the town, and then the
                // streets there are better than any other "main st"
                let mut found = vec![];
                let mut split = road_words.len();
                while split > 1 && road_words.len() - split < 3 && found.len() < want {
                    split -= 1;
                    let ids = self.scope(&[road_words[split..].join(" ")], road_open, cc);
                    if ids.is_empty() {
                        continue;
                    }
                    found.extend(self.streets_typed(
                        &road_words[..split],
                        false,
                        Some(&ids),
                        number_word,
                        false,
                        cc,
                        want - found.len(),
                    ));
                }
                found.extend(self.streets_typed(
                    &road_words,
                    road_open,
                    None,
                    number_word,
                    number_open,
                    cc,
                    want,
                ));
                found
            } else {
                let ids = self.scope(&typed.parts[1..], typed.open, cc);
                if ids.is_empty() {
                    vec![]
                } else {
                    self.streets_typed(&road_words, false, Some(&ids), number_word, false, cc, want)
                }
            };
            if !found.is_empty() {
                out = found;
                break;
            }
        }
        out.truncate(want);
        out
    }

    /// Ids of the places the parts name, to scope streets to: those places and all inside them.
    fn scope(&self, parts: &[String], open: bool, cc: Option<&str>) -> Vec<u32> {
        // two letters of a name are too many places to scope streets to
        let short = open && parts.len() == 1 && parts[0].chars().count() < 3;
        let mut ids: Vec<u32> = if short {
            vec![]
        } else {
            self.suggest_places(parts, open, cc, 8)
                .into_iter()
                .map(|(id, _)| id)
                .collect()
        };
        // "main st, il" and "10 downing st, uk": a region's code, a country's name or code
        if let [one] = parts {
            let q = fold(one);
            if (2..=3).contains(&q.len()) {
                let suffix = format!("-{}", q.to_uppercase());
                ids.extend(self.regions.iter().filter_map(|(code, &place)| {
                    let pl = &self.places[place as usize];
                    (code.ends_with(&suffix) && cc.is_none_or(|c| pl.cc.eq_ignore_ascii_case(c)))
                        .then_some(place)
                }));
            }
            ids.extend(
                self.countries_named(&q, open)
                    .into_iter()
                    .filter(|c| cc.is_none_or(|cc| c.eq_ignore_ascii_case(cc)))
                    .filter_map(|c| self.country_places.get(c).copied()),
            );
        }
        ids.dedup();
        ids
    }

    #[allow(clippy::too_many_arguments)]
    fn streets_typed(
        &self,
        road: &[String],
        road_open: bool,
        scope: Option<&[u32]>,
        number: Option<&str>,
        number_open: bool,
        cc: Option<&str>,
        want: usize,
    ) -> Vec<Suggestion> {
        if road_open && road.len() == 1 && road[0].chars().count() < 3 {
            return vec![];
        }
        let Some((base, rarest)) = self.typed_words(self.f.names, road, road_open) else {
            return vec![];
        };
        let in_scope = scope.map(|ids| {
            ids.iter()
                .map(|id| self.docs_with(self.f.ids, &id.to_string()))
                .sum::<u64>()
        });
        let limit = if scope.is_some() {
            MAX_WORK_IN_TOWN
        } else {
            MAX_WORK
        };
        if rarest.min(in_scope.unwrap_or(u64::MAX)) > limit {
            return vec![];
        }
        let number = number.map(schema::number_key).filter(|n| !n.is_empty());
        let road_text = road.join(" ");
        let mut streets = vec![];
        let mut seen = std::collections::HashSet::new();
        // the best place of the scope alone first: "5th avenue, new york" is Manhattan's
        // before any other in the state
        let phases: Vec<Option<&[u32]>> = match scope {
            Some(ids) if ids.len() > 1 => vec![Some(&ids[..1]), Some(ids)],
            other => vec![other],
        };
        for ids in phases {
            if streets.len() >= want {
                break;
            }
            let mut cl: Vec<_> = base.iter().map(|(o, q)| (*o, q.box_clone())).collect();
            if let Some(ids) = ids {
                cl.push(must(Box::new(BooleanQuery::new(
                    ids.iter()
                        .map(|id| (Occur::Should, term(self.f.ids, &id.to_string())))
                        .collect(),
                ))));
            }
            for a in self.top_of(Kind::Street, cc, cl, want.max(5) * 4) {
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                let name = best_name(&doc, self.f.name, &road_text);
                let (sid, place) = (self.fast(a, "id") as u32, self.fast(a, "place") as u32);
                // a street crossing a cell edge is indexed in parts
                if seen.insert((fold(&name), place)) {
                    streets.push((a, sid, place, name));
                }
            }
        }
        let scope_rank = |place: u32| {
            scope.and_then(|ids| {
                self.chain(place)
                    .iter()
                    .filter_map(|c| ids.iter().position(|i| i == c))
                    .min()
            })
        };
        // those named as typed before those that only contain it ("Main Street" over "High
        // Springs Main Street")
        streets.sort_by_cached_key(|s| (scope_rank(s.2), !fold(&s.3).starts_with(&road_text)));
        let mut houses = vec![];
        let mut plain = vec![];
        for (a, sid, place, name) in streets {
            let mut found = false;
            if let Some(n) = &number {
                let shard = &self.numbers[sid as usize % self.numbers.len()];
                let mut key = sid.to_be_bytes().to_vec();
                key.extend_from_slice(n.as_bytes());
                let mut numbers: Vec<(String, u64)> = shard
                    .get(&key)
                    .map(|v| (n.clone(), v))
                    .into_iter()
                    .collect();
                if number_open {
                    let mut after = key.clone();
                    *after.last_mut().unwrap() += 1;
                    let mut stream = shard.range().ge(&key).lt(&after).into_stream();
                    while numbers.len() < 4
                        && let Some((k, v)) = stream.next()
                    {
                        if k != key.as_slice()
                            && let Ok(more) = std::str::from_utf8(&k[4..])
                        {
                            numbers.push((more.to_string(), v));
                        }
                    }
                }
                for (n, v) in numbers {
                    let (x, y, _) = schema::unpack(v);
                    houses.push(self.street_suggestion(sid, place, &name, Some(&n), (x, y)));
                    found = true;
                }
            }
            if !found {
                plain.push((sid, place, name, a));
            }
        }
        let mut out = houses;
        for (sid, place, name, a) in plain {
            let xy = (self.fast_f64(a, "x"), self.fast_f64(a, "y"));
            out.push(self.street_suggestion(sid, place, &name, None, xy));
        }
        out.truncate(want);
        out
    }

    fn street_suggestion(
        &self,
        sid: u32,
        place: u32,
        name: &str,
        number: Option<&str>,
        (x, y): (f64, f64),
    ) -> Suggestion {
        let mut display: Vec<String> = number.map(String::from).into_iter().collect();
        display.push(name.to_string());
        let mut country_code = String::new();
        if place != NONE {
            let mut details = Map::new();
            self.address(place, true, &mut details, &mut display);
            country_code = self.places[place as usize].cc.to_string();
        }
        Suggestion {
            id: match number {
                Some(n) => format!("h{sid}:{n}"),
                None => format!("s{sid}"),
            },
            display_name: display.join(", "),
            kind: if number.is_some() {
                "address"
            } else {
                "street"
            },
            country_code,
            x,
            y,
            bbox: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEP: char = '\u{1f}';

    /// Paris (France), Paris (Texas), London, a hamlet called Londo, a street with houses 12 and
    /// 120, and a postcode.
    fn tiny() -> (Index, std::path::PathBuf) {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "suggest-index-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // subtype, class, cc, region, _, population, x, y, bbox, parent, primary, names, item
        let place = |subtype: &str,
                     class: &str,
                     cc: &str,
                     region: &str,
                     pop: u32,
                     x: f64,
                     y: f64,
                     parent: &str,
                     primary: &str,
                     names: &str| {
            format!(
                "{subtype}\t{class}\t{cc}\t{region}\t\t{pop}\t{x}\t{y}\t0\t0\t0\t0\t{parent}\t{primary}\t{names}\t\n"
            )
        };
        let en = |n: &str| format!("en={n}");
        let places = [
            place(
                "country",
                "",
                "FR",
                "",
                68_000_000,
                2.0,
                46.0,
                "",
                "France",
                &en("France"),
            ),
            place(
                "region",
                "",
                "FR",
                "FR-IDF",
                12_000_000,
                2.5,
                48.7,
                "0",
                "Île-de-France",
                &en("Ile-de-France"),
            ),
            place(
                "locality",
                "city",
                "FR",
                "FR-IDF",
                2_100_000,
                2.35,
                48.85,
                "1",
                "Paris",
                &format!("{}{SEP}fr=Paris", en("Paris")),
            ),
            place(
                "country",
                "",
                "US",
                "",
                330_000_000,
                -100.0,
                40.0,
                "",
                "United States",
                &en("United States"),
            ),
            place(
                "region",
                "",
                "US",
                "US-TX",
                29_000_000,
                -99.0,
                31.0,
                "3",
                "Texas",
                &en("Texas"),
            ),
            place(
                "locality",
                "town",
                "US",
                "US-TX",
                25_000,
                -95.55,
                33.66,
                "4",
                "Paris",
                &en("Paris"),
            ),
            place(
                "country",
                "",
                "GB",
                "",
                67_000_000,
                -2.0,
                54.0,
                "",
                "United Kingdom",
                &en("United Kingdom"),
            ),
            place(
                "locality",
                "city",
                "GB",
                "GB-ENG",
                8_900_000,
                -0.12,
                51.5,
                "6",
                "London",
                &en("London"),
            ),
            place(
                "locality",
                "hamlet",
                "NE",
                "",
                0,
                1.1,
                14.0,
                "",
                "Londo",
                &en("Londo"),
            ),
        ];
        std::fs::write(dir.join("places.tsv"), places.concat()).unwrap();

        let (schema, f) = schema::schema();
        let index = tantivy::Index::create_in_dir(&dir, schema).unwrap();
        index.tokenizers().register("fold", schema::analyzer());
        let mut writer: tantivy::IndexWriter =
            index.writer_with_num_threads(1, 15_000_000).unwrap();
        for (i, p) in places.iter().enumerate() {
            let c: Vec<&str> = p.split('\t').collect();
            let names = format!(
                "{} {}",
                c[13],
                c[14]
                    .replace(SEP, " ")
                    .replace("en=", "")
                    .replace("fr=", "")
            );
            writer
                .add_document(tantivy::doc!(
                    f.kind => "p", f.names => names, f.country => c[2], f.ids => i.to_string(),
                    f.name => c[13], f.id => i as u64, f.importance => c[5].parse::<u64>().unwrap(),
                    f.x => c[6].parse::<f64>().unwrap(), f.y => c[7].parse::<f64>().unwrap()
                ))
                .unwrap();
        }
        let mut street = tantivy::doc!(
            f.kind => "s", f.names => "Rue de Rivoli", f.country => "FR", f.name => "Rue de Rivoli",
            f.id => 7u64, f.place => 2u64, f.importance => 50u64, f.x => 2.34f64, f.y => 48.86f64
        );
        for id in ["2", "1", "0"] {
            street.add_text(f.ids, id);
        }
        writer.add_document(street).unwrap();
        writer
            .add_document(tantivy::doc!(
                f.kind => "z", f.pcode => "75001", f.name => "75001", f.country => "FR",
                f.place => 2u64, f.importance => 10u64, f.x => 2.336f64, f.y => 48.862f64
            ))
            .unwrap();
        writer.commit().unwrap();
        drop(writer);

        let mut fst = tantivy_fst::MapBuilder::memory();
        for n in ["12", "120"] {
            let mut key = 7u32.to_be_bytes().to_vec();
            key.extend_from_slice(n.as_bytes());
            let (x, y) = schema::pack(2.3 + n.len() as f64 / 100.0, 48.86, 0);
            fst.insert(key, (x as u64) << 32 | y as u64).unwrap();
        }
        std::fs::write(dir.join("numbers.shards"), "1").unwrap();
        std::fs::write(dir.join("numbers-00.fst"), fst.into_inner().unwrap()).unwrap();
        let index = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
        (index, dir)
    }

    fn names(found: &[Suggestion]) -> Vec<&str> {
        found.iter().map(|s| s.display_name.as_str()).collect()
    }

    #[test]
    fn completes_places_by_name_start_and_weight() {
        let (index, dir) = tiny();
        let found = index.suggest("par", 5, None);
        assert_eq!(found[0].id, "p2");
        assert_eq!(
            (found[0].kind, found[0].country_code.as_str()),
            ("city", "fr")
        );
        assert_eq!(found[1].id, "p5");
        assert_eq!(found[0].display_name, "Paris, Ile-de-France, France");
        // the whole word typed, and then the town's country or region
        assert_eq!(index.suggest("paris tex", 5, None)[0].id, "p5");
        assert_eq!(index.suggest("paris, tx", 5, None)[0].id, "p5");
        assert_eq!(index.suggest("paris, fr", 5, None)[0].id, "p2");
        assert_eq!(index.suggest("paris, unit", 5, None)[0].id, "p5");
        // a hamlet named exactly as typed does not outrank the city the text starts
        assert_eq!(index.suggest("londo", 5, None)[0].id, "p7");
        assert_eq!(index.suggest("londo ", 5, None)[0].id, "p8");
        assert_eq!(
            names(&index.suggest("PARIS", 1, Some("us"))),
            ["Paris, Texas, United States"]
        );
        assert!(index.suggest("p", 5, None).is_empty());
        assert!(index.suggest("  ,, ", 5, None).is_empty());
        assert!(index.suggest("zzzzz", 5, None).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn completes_streets_houses_and_postcodes() {
        let (index, dir) = tiny();
        let street = &index.suggest("rue de riv", 5, None)[0];
        assert_eq!((street.id.as_str(), street.kind), ("s7", "street"));
        assert_eq!(
            street.display_name,
            "Rue de Rivoli, Paris, Ile-de-France, France"
        );
        // the town after a comma, still being typed
        assert!(
            index
                .suggest("rue de rivoli, par", 5, None)
                .iter()
                .any(|s| s.id == "s7")
        );
        let house = &index.suggest("12 rue de rivoli, par", 5, None)[0];
        assert_eq!((house.id.as_str(), house.kind), ("h7:12", "address"));
        assert!(house.display_name.starts_with("12, Rue de Rivoli, Paris"));
        // a house number typed last is being typed: 12 and 120
        let ids: Vec<_> = index
            .suggest("rue de rivoli 12", 5, None)
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids[..2], ["h7:12", "h7:120"]);
        // no such number: the street
        assert_eq!(index.suggest("99 rue de rivoli", 5, None)[0].id, "s7");
        // no place by that name in the scope
        assert!(index.suggest("rue de rivoli, texas", 5, None).is_empty());
        let code = &index.suggest("7500", 5, None)[0];
        assert_eq!((code.id.as_str(), code.kind), ("zfr:75001", "postcode"));
        assert!(index.suggest("75", 5, None).is_empty());
        assert!(index.suggest("rue de riv", 5, Some("us")).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
