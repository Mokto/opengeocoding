//! Completion of a place, street, house or postcode typed so far. Unlike `search`, it never
//! calls libpostal, which misreads half-typed words: the last word is a prefix of terms read
//! from the index's term dictionary, and the commas the user types say where the town starts.
mod landmark;
mod place;
mod postcode;
mod street;

use super::country::{ca, cz_sk, dk, ee, eg, hk_mo, it, lt, pl, ps, pt, th, tw};
use super::*;
use levenshtein_automata::{DFA, Distance, LevenshteinAutomatonBuilder, SINK_STATE};
pub(super) use place::place_names;
use place::*;
use std::{cmp::Reverse, collections::BinaryHeap, sync::LazyLock};
use tantivy_fst::{IntoStreamer, Streamer};

/// Documents times terms a prefix search may check: its terms are cut to fit, as far as `MIN_TERMS`.
const TERM_WORK: u64 = 200_000;
const MIN_TERMS: u64 = 4;
/// Terms a prefix expands to, most common first: each is a posting list to walk.
const PREFIX_TERMS: usize = 32;
/// Candidate places fetched before a qualifier ("Springfield, IL") filters them.
const POOL: usize = 400;
/// A street search whose rarest word is in more documents than this walks too many postings to
/// answer in time. Without a town its answer would be arbitrary as well ("rue de", "main st"),
/// so it is held to less than one in a town.
const MAX_WORK: u64 = 20_000;
const MAX_WORK_IN_TOWN: u64 = 400_000;
/// The same for the long tail of places the name index does not hold: found by an exact word
/// that few documents share.
const MAX_PLACE_WORK: u64 = 20_000;
const MAX_TEXT: usize = 200;
/// Entries of each prefix cache: a place entry is at most 400 candidates of 16 bytes, so the
/// caches stay under about 150 MB however much is typed.
const MAX_CACHED: usize = 20_000;
pub(super) const MAX_NUMBER: usize = 10;
/// Words of a postcode needed before suggesting one: "75" matches thousands.
const MIN_POSTCODE: usize = 3;
/// Characters of a name typed before one a letter off is looked for: "pari" is one edit from
/// too many names.
const MIN_TYPO: usize = 5;
/// Characters typed before a word inside a name counts: "ci" starts the end of too many.
const MIN_INSIDE: usize = 3;
/// The fit of a place whose name is one edit from the text.
const TYPO: u8 = 4;
/// The fit of a place a later word of whose main name starts with the text ("york" of New
/// York). It counts as little as a name that has the text inside, but comes before streets.
const INSIDE: u8 = 5;
/// The weight from which a place is found by a later word of its main name: smaller ones are
/// too many to rank before streets, and the index's terms find them after.
const CITY: u64 = 100_000;

/// Names within one insertion, deletion, substitution or transposition of adjacent letters.
static ONE_EDIT: LazyLock<LevenshteinAutomatonBuilder> =
    LazyLock::new(|| LevenshteinAutomatonBuilder::new(1, true));

/// The keys of `place_names` whose name, before the NUL that ends it, the DFA accepts.
struct NameWithin<'a>(&'a DFA);

impl tantivy_fst::Automaton for NameWithin<'_> {
    /// The DFA's state, or `None` once past an accepted name.
    type State = Option<u32>;

    fn start(&self) -> Self::State {
        Some(self.0.initial_state())
    }

    fn is_match(&self, state: &Self::State) -> bool {
        state.is_none()
    }

    fn can_match(&self, state: &Self::State) -> bool {
        *state != Some(SINK_STATE)
    }

    fn will_always_match(&self, state: &Self::State) -> bool {
        state.is_none()
    }

    fn accept(&self, state: &Self::State, byte: u8) -> Self::State {
        match *state {
            None => None,
            Some(s) if byte == 0 => {
                (!matches!(self.0.distance(s), Distance::Exact(_))).then_some(SINK_STATE)
            }
            Some(s) => Some(self.0.transition(s, byte)),
        }
    }
}

/// The user's position: places near it rank higher.
#[derive(Clone, Copy, Debug)]
pub struct Focus {
    pub lat: f64,
    pub lon: f64,
    pub scale_km: f64,
}

impl Focus {
    /// What the weight of a place at this point is multiplied by: 1,001 at the focus, 501 at
    /// `scale_km`, about 11 at ten times that, and towards 1 beyond.
    fn boost(&self, lat: f64, lon: f64) -> f64 {
        let (a, b) = (self.lat.to_radians(), lat.to_radians());
        let h = ((b - a) / 2.0).sin().powi(2)
            + a.cos() * b.cos() * ((lon - self.lon).to_radians() / 2.0).sin().powi(2);
        let km = 2.0 * 6371.0 * h.sqrt().min(1.0).asin();
        1.0 + 1000.0 / (1.0 + (km / self.scale_km.max(1.0)).powi(2))
    }
}

/// A prefix's most common terms and the documents each is in, most first.
pub(super) type PrefixTerms = Arc<[(String, u32)]>;
/// Place, fit and score of each place a name may mean.
pub(super) type Candidates = Arc<[(u32, u8, u64)]>;
/// The text, whether it is still being typed, and whether names one edit from it are wanted.
pub(super) type NamesKey = (Box<str>, bool, bool);
/// What suggestions look up again and again: a place's English name and its address line, which
/// `Place::name` finds by scanning the place's names in every language.
#[derive(Default)]
pub(super) struct Caches {
    /// Folded name and folded English name, by place.
    folded: Mutex<HashMap<u32, Arc<[String; 2]>>>,
    labels: Mutex<HashMap<u32, Arc<str>>>,
    /// Every folded country name and its code, sorted.
    country_names: std::sync::OnceLock<Vec<(String, String)>>,
    egyptian_counties: std::sync::OnceLock<Vec<u32>>,
}

fn remember<V: Clone>(cache: &Mutex<HashMap<u32, V>>, id: u32, make: impl FnOnce() -> V) -> V {
    if let Some(hit) = cache.lock().unwrap().get(&id) {
        return hit.clone();
    }
    let made = make();
    let mut cache = cache.lock().unwrap();
    // ponytail: dropped whole when full, not LRU
    if cache.len() >= MAX_CACHED {
        cache.clear();
    }
    cache.insert(id, made.clone());
    made
}

type Clauses = Vec<(Occur, Box<dyn TQuery>)>;

/// Which names `suggest_places` reads.
#[derive(Clone, Copy, PartialEq)]
enum Names {
    /// The names as typed and the words inside them that the index holds.
    Strict,
    /// Also a later word of a main name ("york" of New York).
    Inside,
    /// Only the names one edit from the text, for when the others leave room.
    Typo,
}

pub struct Suggestion {
    /// Opaque; a place, street, house or postcode of this index.
    pub id: String,
    pub retrieval_id: String,
    pub display_name: String,
    pub unverified_suffix: String,
    pub kind: &'static str,
    /// Landmarks only: airport, station, attraction, museum, university, hospital or stadium.
    pub category: &'static str,
    pub country_code: String,
    pub x: f64,
    pub y: f64,
    /// Places only: min lon, min lat, max lon, max lat.
    pub bbox: Option<[f32; 4]>,
}

/// `out` in the country asked for, without repeats, and at most `limit`.
fn kept(mut out: Vec<Suggestion>, cc: Option<&str>, limit: usize) -> Vec<Suggestion> {
    let mut seen = std::collections::HashSet::new();
    // the filter matches a street's own country, which its place may not be in
    out.retain(|s| {
        cc.is_none_or(|c| s.country_code.is_empty() || s.country_code.eq_ignore_ascii_case(c))
            && seen.insert((s.display_name.clone(), s.category))
    });
    out.truncate(limit);
    out
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
            let mut word = ts.token().text.clone();
            if word.is_ascii() {
                out.push(word);
                continue;
            }
            word.retain(|c| !('\u{300}'..='\u{36f}').contains(&c));
            let folded = fold(&word);
            if folded.contains(' ') {
                out.extend(folded.split(' ').map(str::to_owned));
            } else {
                out.push(word);
            }
        }
        out
    })
}

/// The words of `text`, the last one as typed when it may be unfinished.
pub(super) fn words_open(text: &str, open: bool) -> Vec<String> {
    if !open {
        return words(text);
    }
    // "saint-etienne" is "st etienne": the word before a hyphen is whole
    let last = text
        .rsplit(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or(text);
    let before = &text[..text.len() - last.len()];
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

/// "koeln" as "koln", which is how the index holds "Köln": German digraphs for umlauts.
fn umlaut_form(text: &str) -> Option<String> {
    let form = text
        .replace("ae", "a")
        .replace("oe", "o")
        .replace("ue", "u");
    (form != text).then_some(form)
}

fn words(text: &str) -> Vec<String> {
    fold(text)
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// The text as typed, split at the commas, and whether its last word may be unfinished.
pub(super) struct Typed {
    pub(super) parts: Vec<String>,
    pub(super) open: bool,
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
    /// Reads what the first person to type a short prefix would otherwise wait for: the places
    /// and terms of one and two letters, and the names of countries and regions. One thread, once,
    /// after startup; blocks for some seconds.
    pub fn warm_suggest(&self) {
        let letters = 'a'..='z';
        for a in letters.clone() {
            self.name_candidates(&a.to_string(), true, false);
            for b in letters.clone() {
                let pair = format!("{a}{b}");
                self.name_candidates(&pair, true, false);
                self.prefix_terms(self.f.names, &pair);
            }
        }
        for (id, pl) in self.places.iter().enumerate() {
            if matches!(
                &*pl.subtype,
                "country" | "dependency" | "region" | "macroregion"
            ) {
                self.folded_names(id as u32);
                self.label(id as u32);
            }
        }
    }

    /// Up to `limit` completions of `text`, best first. Blocks: call it off the async executor.
    pub fn suggest(
        &self,
        text: &str,
        limit: usize,
        cc: Option<&str>,
        focus: Option<&Focus>,
        landmarks: bool,
    ) -> Vec<Suggestion> {
        let text = text.trim_start();
        if text.chars().count() > MAX_TEXT || text.trim().is_empty() || limit == 0 {
            return vec![];
        }
        if cc.is_none_or(|c| c.eq_ignore_ascii_case("tw"))
            && let Some(without) = tw::without_taiwan_province(text)
        {
            return self.suggest(&without, limit, cc, focus, landmarks);
        }
        if let Some(found) = self.chinese_suggest(text, cc) {
            return found;
        }
        if cc.is_none_or(|c| c.eq_ignore_ascii_case("hk")) {
            let found: Vec<Suggestion> = self
                .hk_typed_houses(text, cc)
                .iter()
                .filter_map(|hit| match hit {
                    Hit::Street {
                        doc,
                        sid,
                        place,
                        name,
                        number,
                        x,
                        y,
                        ..
                    } => Some(self.street_suggestion(
                        *doc,
                        *sid,
                        *place,
                        name,
                        number.as_deref(),
                        (*x, *y),
                    )),
                    _ => None,
                })
                .collect();
            if !found.is_empty() {
                return kept(found, cc, limit);
            }
            if let Some(spaced) = hk_mo::typed_chinese(text)
                && spaced != text
            {
                return self.suggest(&spaced, limit, cc, focus, landmarks);
            }
        }
        if cc.is_none_or(|c| c.eq_ignore_ascii_case("lt"))
            && let Some(written) = lt::suggest_text(text)
        {
            return self.suggest(&written, limit, cc, focus, landmarks);
        }
        if cc.is_none_or(|c| c.eq_ignore_ascii_case("lt"))
            && let Some(short) = lt::initialled(text)
        {
            let found = self.suggest(&short, limit, cc, focus, landmarks);
            if found.iter().any(|s| s.kind == "address") {
                return found;
            }
        }
        if cc.is_none_or(|c| c.eq_ignore_ascii_case("ee")) {
            if let Some(turned) = ee::county_first(text) {
                return self.suggest(&turned, limit, cc, focus, landmarks);
            }
            if let Some(found) = self.ee_named_house(text) {
                return kept(vec![found], cc, limit);
            }
        }
        if let Some(hits) = self.korean_search(text)
            && !hits.is_empty()
        {
            return hits
                .iter()
                .filter_map(|hit| match hit {
                    Hit::Street {
                        doc,
                        sid,
                        place,
                        name,
                        number,
                        x,
                        y,
                        ..
                    } if cc.is_none_or(|c| c.eq_ignore_ascii_case("kr")) => {
                        Some(self.street_suggestion(
                            *doc,
                            *sid,
                            *place,
                            name,
                            number.as_deref(),
                            (*x, *y),
                        ))
                    }
                    _ => None,
                })
                .take(limit)
                .collect();
        }
        if let Some(found) = self.korean_suggest(text, cc, limit) {
            return found;
        }
        let typed = Typed::new(text);
        let Some(head) = typed.parts.first() else {
            return vec![];
        };
        // the unit is plain to see, and looking for the house with it costs every lookup below
        let found = self.suggest_us_unit(&typed, cc, focus, limit);
        if !found.is_empty() {
            return kept(found, cc, limit);
        }
        let head_words: Vec<&str> = head.split(' ').collect();
        let one_part = typed.parts.len() == 1;
        if one_part && head.chars().count() < 2 {
            return vec![];
        }
        let coded = head_words.iter().any(|w| has_digit(w));
        // "75001", or "SW1A 2": a postcode, whose words may be split by a space
        if coded && head_words.len() <= 2 {
            let found = self.suggest_postcodes(head, &typed.parts[1..], typed.open, cc, limit);
            if !found.is_empty() {
                return found;
            }
            if head_words.len() == 1 && head.chars().all(|c| c.is_ascii_digit()) {
                return self.suggest_number_comma(&typed, cc, focus, limit);
            }
        }
        let places = |names| {
            self.suggest_places(&typed.parts, typed.open, cc, focus, names, limit)
                .into_iter()
        };
        let found = places(Names::Inside).collect::<Vec<_>>();
        // a place whose name only contains the words (a "Main Street Historic District") comes
        // after the streets that are named so, unless it is a city ("york" is New York)
        let (named, contains): (Vec<_>, Vec<_>) = found.into_iter().partition(|&(id, fit)| {
            fit == 0 || fit <= 2 && weight(&self.places[id as usize]) >= 5_000 || fit == INSIDE
        });
        let mut out: Vec<Suggestion> = named
            .into_iter()
            .map(|(id, _)| self.place_suggestion(id))
            .collect();
        if landmarks && out.len() < limit {
            out.extend(self.suggest_landmarks(&typed, cc, limit - out.len()));
        }
        if out.len() < limit {
            out.extend(self.suggest_streets(&typed, cc, focus, limit - out.len()));
        }
        out.extend(
            contains
                .into_iter()
                .map(|(id, _)| self.place_suggestion(id)),
        );
        // a name a letter off only fills what the text as typed leaves
        if out.len() < limit && !coded {
            out.extend(places(Names::Typo).map(|(id, _)| self.place_suggestion(id)));
        }
        if !out.iter().any(|s| s.kind == "address") {
            let found = self.suggest_australian(&typed, cc, focus, limit);
            if !found.is_empty() {
                out = found;
            }
        }
        if out.is_empty() {
            out = self.suggest_number_comma(&typed, cc, focus, limit);
        }
        if out.is_empty() {
            out = self.suggest_floor(&typed, cc, focus, limit);
        }
        if !out.iter().any(|s| s.kind == "address") {
            let found = self.suggest_us_repair(&typed, cc, focus, limit);
            if !found.is_empty() {
                out = found;
            }
        }
        if !out.iter().any(|s| s.kind == "address")
            && cc.is_none_or(|c| c.eq_ignore_ascii_case("ca"))
            && let Some((building, number)) = ca::canadian_building(text)
        {
            let number = schema::number_key(number);
            let recovered: Vec<_> = self
                .suggest(building, limit, Some("ca"), focus, landmarks)
                .into_iter()
                .filter(|s| {
                    s.kind == "address"
                        && s.country_code == "ca"
                        && s.id.rsplit(':').next() == Some(number.as_str())
                })
                .collect();
            if !recovered.is_empty() {
                out = recovered;
            }
        }
        // "Via Praga 3, Possidente, Avigliano (PZ)": the houses name the municipality, not the
        // frazione
        if !out.iter().any(|s| s.kind == "address")
            && cc.is_none_or(|c| c.eq_ignore_ascii_case("it"))
            && let Some((trimmed, _, _)) = it::frazione_parts(text)
        {
            let recovered: Vec<_> = self
                .suggest(&trimmed, limit, cc, focus, landmarks)
                .into_iter()
                .filter(|s| s.kind == "address")
                .collect();
            if !recovered.is_empty() {
                out = recovered;
            }
        }
        if !out.iter().any(|s| s.kind == "address")
            && cc.is_none_or(|c| c.eq_ignore_ascii_case("th"))
            && typed.parts.last().and_then(|p| self.country(p)).as_deref() == Some("th")
            && let Some(trimmed) = th::road_and_postcode(text)
        {
            let recovered: Vec<_> = self
                .suggest(&trimmed, limit, cc, focus, landmarks)
                .into_iter()
                .filter(|s| s.kind == "address")
                .collect();
            if !recovered.is_empty() {
                out = recovered;
            }
        }
        if !out.iter().any(|s| s.kind == "address") {
            let mut found = self.suggest_repaired(&typed, cc, limit);
            found.append(&mut out);
            out = found;
        }
        // with streets of the road found, a place left out rarely finds its house
        if !out
            .iter()
            .any(|s| s.kind == "address" || s.kind == "street")
            && let Some(found) = self.suggest_without_place(&typed, cc, focus, limit)
        {
            out = found;
        }
        kept(out, cc, limit)
    }

    /// "5A Salena road, Jojoima, Kailahun, Sierra Leone" (a district after the town) and "323
    /// Rue Gabriel Ngoua Obiang, Nzeng Ayong, Libreville, Gabon" (a quarter the houses are not
    /// filed under), and "575 Kaya Diabaas, Tera Kora, Curacao" (a neighbourhood next to the
    /// one the house is filed under): the house of that exact number that every place between a
    /// numbered road and its country but one names, near the one left out.
    fn suggest_without_place(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        limit: usize,
    ) -> Option<Vec<Suggestion>> {
        let parts = &typed.parts;
        if !(3..=5).contains(&parts.len()) || self.country(parts.last()?).is_none() {
            return None;
        }
        let words: Vec<&str> = parts[0].split(' ').collect();
        let numbers: Vec<String> = [words.first()?, words.last()?]
            .into_iter()
            .filter(|w| words.len() > 1 && has_digit(w))
            .map(|w| schema::number_key(w))
            .collect();
        // with more than one place, one has to remain beside the road: a postcode alone does not
        // place the house
        let places = parts[1..parts.len() - 1]
            .iter()
            .filter(|p| p.chars().any(char::is_alphabetic))
            .count();
        if numbers.is_empty() || places < 1 || places < 2 && parts.len() > 3 {
            return None;
        }
        // in the order that most often finds the house: of "road, town, district, country" the
        // district, of "road, quarter, town, district, country" the quarter
        let order: &[usize] = match parts.len() {
            3 => &[1],
            4 => &[2, 1],
            _ => &[1, 3, 2],
        };
        order.iter().find_map(|&skip| {
            // a postcode the house does not carry contradicts it
            if !parts[skip].chars().any(char::is_alphabetic) {
                return None;
            }
            let mut fewer = parts.clone();
            fewer.remove(skip);
            let fewer = Typed {
                parts: fewer,
                open: typed.open,
            };
            let mut found: Vec<_> = self
                .suggest_streets(&fewer, cc, focus, limit)
                .into_iter()
                .filter(|s| {
                    s.kind == "address"
                        && s.id
                            .rsplit(':')
                            .next()
                            .is_some_and(|n| numbers.iter().any(|k| k == n))
                })
                .collect();
            // the place left out still has to agree: the house's own address names it, or a
            // place of that name is near (a district or quarter the data files the house outside
            // of); a village more than 5 km off has houses of its own, and so has any place when
            // it is the only one typed
            let left_out = &parts[skip];
            let only = parts.len() == 3;
            if !found.is_empty() {
                let places = self.scope(std::slice::from_ref(left_out), false, cc, None, false);
                found.retain(|s| {
                    fold(&s.display_name).contains(&fold(left_out))
                        || places.iter().any(|&id| {
                            let pl = &self.places[id as usize];
                            let village = &*pl.subtype == "locality"
                                && matches!(&*pl.class, "village" | "hamlet");
                            let km = ((pl.x as f64 - s.x) * s.y.to_radians().cos())
                                .hypot(pl.y as f64 - s.y)
                                * 111.195;
                            km < if village || only { 5.0 } else { 15.0 }
                        })
                });
            }
            (!found.is_empty()).then_some(found)
        })
    }

    /// The most common terms of `field` starting with `prefix`, summed over segments, with the
    /// documents each is in. Reading them is the cost of a short prefix, so the answer is kept.
    fn prefix_terms(&self, field: Field, prefix: &str) -> PrefixTerms {
        if prefix.is_empty() || prefix.len() > MAX_WORD {
            return Arc::new([]);
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
        let terms: PrefixTerms = sum
            .into_iter()
            .filter_map(|(t, df)| Some((String::from_utf8(t).ok()?, df)))
            .collect();
        let mut cache = self.prefixes.lock().unwrap();
        // ponytail: dropped whole when full, not LRU; the short prefixes that cost most refill first
        if cache.len() >= MAX_CACHED {
            cache.clear();
        }
        cache.insert(key, terms.clone());
        terms
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
        let found = self.prefix_terms(field, last);
        // Every document of the other words is checked against every term: when they are many,
        // only the commonest terms are worth it.
        let cap = (TERM_WORK / rarest.max(1)).clamp(MIN_TERMS, PREFIX_TERMS as u64) as usize;
        let found = &found[..found.len().min(cap)];
        let mut docs: u64 = found.iter().map(|t| t.1 as u64).sum();
        let mut terms: Vec<String> = found.iter().map(|t| t.0.clone()).collect();
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

    pub fn retrieve_suggestion(&self, id: &str) -> Option<Suggestion> {
        let (scope, key) = id.split_once('/')?;
        if scope != self.suggestion_scope {
            return None;
        }
        let (kind, rest) = key.split_at_checked(1)?;
        match kind {
            "p" => {
                let id: u32 = rest.parse().ok()?;
                self.places.get(id as usize)?;
                Some(self.place_suggestion(id))
            }
            "l" => {
                let id: u32 = rest.parse().ok()?;
                self.landmarks.list.get(id as usize)?;
                Some(self.landmark_suggestion(id))
            }
            "s" | "h" | "z" => {
                let mut parts = rest.split(':');
                let segment: u32 = parts.next()?.parse().ok()?;
                let doc: u32 = parts.next()?.parse().ok()?;
                let number = if kind == "h" {
                    Some(parts.next()?)
                } else {
                    None
                };
                if parts.next().is_some() {
                    return None;
                }
                let reader = self.searcher.segment_readers().get(segment as usize)?;
                let expected = if kind == "z" {
                    Kind::Postcode
                } else {
                    Kind::Street
                };
                if doc >= reader.max_doc()
                    || reader
                        .alive_bitset()
                        .is_some_and(|alive| !alive.is_alive(doc))
                    || !self.columns[segment as usize].runs[expected as usize]
                        .iter()
                        .any(|run| run.contains(&doc))
                {
                    return None;
                }
                let a = tantivy::DocAddress::new(segment, doc);
                if kind == "z" {
                    return self.postcode_suggestion(a);
                }
                let sid = self.fast(a, "id") as u32;
                let xy = if let Some(number) = number {
                    if number.is_empty() || cz_sk::number_len(number) > MAX_NUMBER {
                        return None;
                    }
                    let mut key = sid.to_be_bytes().to_vec();
                    key.extend_from_slice(number.as_bytes());
                    let value = self.numbers[sid as usize % self.numbers.len()].get(&key)?;
                    let (x, y, _) = schema::unpack(value);
                    (x, y)
                } else {
                    (self.fast_f64(a, "x"), self.fast_f64(a, "y"))
                };
                let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
                Some(self.street_suggestion(
                    a,
                    sid,
                    self.fast(a, "place") as u32,
                    &stored(&doc, self.f.name),
                    number,
                    xy,
                ))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
