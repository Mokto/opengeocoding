//! Completion of a place, street, house or postcode typed so far. Unlike `search`, it never
//! calls libpostal, which misreads half-typed words: the last word is a prefix of terms read
//! from the index's term dictionary, and the commas the user types say where the town starts.
use super::*;
use levenshtein_automata::{DFA, Distance, LevenshteinAutomatonBuilder, SINK_STATE};
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
const MAX_NUMBER: usize = 10;
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
fn words_open(text: &str, open: bool) -> Vec<String> {
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
        // a place the builder added from GeoNames weighs like a locality Overture gives no size,
        // however populous: it fills a gap and must not crowd out Overture's places
        "locality" if &*p.class == "geonames" => 100,
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
/// Of those, how many a qualifier is checked against: each reads its places' ancestors.
const EXAMINED: usize = 150;
/// How much a name's fit counts: the name itself, its main name starts with the text, another
/// language's does, a name has it inside (or a later word of the main name starts with it).
const FIT: [u64; 4] = [100, 20, 5, 1];
/// The most a country's name in a language that is not its main one is worth.
const OTHER_NAME_COUNTRY: u64 = 2_000_000;

/// Folded name, a NUL, whether it is the primary or English name (0), another (1) or a main
/// name from its second word or a later one on (2: "york" of "new york"), and the place number
/// -> the place's weight. One entry per place and name, so places sharing a name have distinct
/// keys and a prefix is a range.
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
                let inside: Vec<String> = names
                    .iter()
                    .filter(|(_, flag)| *flag == 0 && w >= CITY)
                    .flat_map(|(n, _)| {
                        let words: Vec<&str> = n.split(' ').collect();
                        (1..words.len())
                            .filter(|&i| words[i].chars().count() >= MIN_INSIDE)
                            .map(|i| words[i..].join(" "))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                for n in inside {
                    if !names.iter().any(|(m, _)| *m == n) {
                        names.push((n, 2));
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

/// A Danish unit part: a floor with or without a door ("st.", "1. th", "st. 11", "1. c"), or a
/// side door alone.
fn danish_floor(text: &str) -> bool {
    let folded = text.to_ascii_lowercase().replace('.', " ");
    let words: Vec<_> = folded.split_whitespace().collect();
    let Some(floor) = words.first() else {
        return false;
    };
    let side = |w: &str| matches!(w, "t" | "th" | "tv" | "m" | "mf");
    // "st. john" is a saint, not a door
    let door = |w: &str| {
        side(w)
            || (w.len() <= 4
                && w.bytes().all(|b| b.is_ascii_alphanumeric())
                && (w.len() == 1 || w.bytes().any(|b| b.is_ascii_digit())))
    };
    let floored = matches!(*floor, "st" | "kl")
        || (floor.len() <= 2 && floor.bytes().all(|b| b.is_ascii_digit()));
    match words.len() {
        1 => floored || matches!(*floor, "th" | "tv" | "mf"),
        2 => floored && door(words[1]),
        _ => false,
    }
}

/// An Australian unit before its building's number: "1/44 Macleay St", "Unit 1 44 Macleay St"
/// or "Unit 1, 44 Macleay St". The unit as typed, whether a slash joined it to the number, and
/// the text without it.
fn australian_unit(typed: &Typed) -> Option<(String, bool, Typed)> {
    const KIND: &str = r"(?:unit|u|apt|apartment|flat|shop|suite|villa|townhouse|office|level|lvl|l|floor|fl|site)";
    static SLASH: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"^([A-Za-z]?\d{1,5}[A-Za-z]?)/(\d.*\s.*)$").unwrap());
    static NAMED: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(&format!(
            r"(?i)^({KIND}\s*[a-z]?\d{{1,5}}[a-z]?)(?:\s+(\d.*\s.*))?$"
        ))
        .unwrap()
    });
    let head = typed.parts.first()?;
    let mut parts = typed.parts.clone();
    let (unit, slash) = if let Some(c) = SLASH.captures(head) {
        parts[0] = c[2].to_string();
        (c[1].to_string(), true)
    } else {
        let c = NAMED.captures(head)?;
        match c.get(2) {
            Some(rest) => parts[0] = rest.as_str().to_string(),
            None if parts.len() > 1 => {
                parts.remove(0);
            }
            None => return None,
        }
        (c[1].to_string(), false)
    };
    Some((
        unit,
        slash,
        Typed {
            parts,
            open: typed.open,
        },
    ))
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
        if let Some(found) = self.chinese_suggest(text, cc) {
            return found;
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
        if !out.iter().any(|s| s.kind == "address")
            && cc.is_none_or(|c| c.eq_ignore_ascii_case("ca"))
            && let Some((building, number)) = crate::matching::canadian_building(text)
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
        let mut seen = std::collections::HashSet::new();
        // the filter matches a street's own country, which its place may not be in
        out.retain(|s| {
            cc.is_none_or(|c| s.country_code.is_empty() || s.country_code.eq_ignore_ascii_case(c))
                && seen.insert((s.display_name.clone(), s.category))
        });
        out.truncate(limit);
        out
    }

    fn chinese_suggest(&self, text: &str, cc: Option<&str>) -> Option<Vec<Suggestion>> {
        if !text.contains('號') {
            return None;
        }
        let (units, road, number) = chinese(text)?;
        if !matches!(units.as_slice(), [state, city] if state.ends_with('市') && city.ends_with('區'))
        {
            return None;
        }
        let parts: Vec<_> = text.split([',', '，']).map(str::trim).collect();
        if !(parts.len() == 1
            || parts.len() == 2 && self.country(parts[1]).as_deref() == Some("tw"))
        {
            return None;
        }
        let (building, suffix) = parts[0].rsplit_once('號')?;
        if schema::number_key(building) != format!("{}{road}{number}", units.concat()) {
            return None;
        }
        let road_key = fold(&road);
        if road_key.is_empty()
            || number.len() > MAX_NUMBER
            || cc.is_some_and(|c| !c.eq_ignore_ascii_case("tw"))
        {
            return Some(vec![]);
        }
        let suffix = suffix.trim();
        Some(
            self.chinese_streets(&units, &road, &number, None, Some("tw"))
                .into_iter()
                .filter_map(|hit| {
                    let Hit::Street {
                        doc,
                        sid,
                        place,
                        name,
                        number: Some(found_number),
                        x,
                        y,
                        ..
                    } = hit
                    else {
                        return None;
                    };
                    if fold(&name) != road_key || schema::number_key(&found_number) != number {
                        return None;
                    }
                    let mut found =
                        self.street_suggestion(doc, sid, place, &name, Some(&found_number), (x, y));
                    if !suffix.is_empty() {
                        if let Some((at, _)) = found.display_name.match_indices(", ").nth(1) {
                            found.display_name.insert_str(at, &format!(", {suffix}"));
                        }
                        found.unverified_suffix = suffix.into();
                    }
                    Some(found)
                })
                .collect(),
        )
    }

    fn korean_suggest(&self, text: &str, cc: Option<&str>, want: usize) -> Option<Vec<Suggestion>> {
        let address = korea::parse(text, true)?;
        let scope = self.korean_scope(&address)?;
        if scope.is_empty() || cc.is_some_and(|c| !c.eq_ignore_ascii_case("kr")) {
            return Some(vec![]);
        }
        let open =
            address.country.is_none() && !text.ends_with(|c: char| c.is_whitespace() || c == ',');
        let number = (!address.number.is_empty()).then_some(address.number.as_str());
        let road_open = open && number.is_none();
        let road = fold(address.road);
        Some(
            self.streets_typed(
                &words_open(address.road, road_open),
                road_open,
                Some(&scope),
                number,
                open,
                Some("kr"),
                None,
                None,
                want.max(5),
            )
            .into_iter()
            .filter(|s| {
                s.display_name
                    .split(", ")
                    .nth(usize::from(s.kind == "address"))
                    .map(fold)
                    .is_some_and(|name| name == road || road_open && name.starts_with(&road))
            })
            .take(want)
            .collect(),
        )
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

    /// Country codes of the countries a qualifier names, or starts to.
    fn countries_named(&self, q: &str, open: bool) -> Vec<&str> {
        let names = self.suggest.country_names.get_or_init(|| {
            let mut all: Vec<_> = self
                .countries
                .iter()
                .map(|(n, cc)| (n.clone(), cc.clone()))
                .collect();
            all.sort_unstable();
            all
        });
        let from = names.partition_point(|(n, _)| n.as_str() < q);
        let mut out: Vec<&str> = names[from..]
            .iter()
            .take_while(|(n, _)| n == q || (open && q.len() >= 3 && n.starts_with(q)))
            .map(|(_, cc)| cc.as_str())
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// A place's folded name and folded English name.
    fn folded_names(&self, id: u32) -> Arc<[String; 2]> {
        remember(&self.suggest.folded, id, || {
            let pl = &self.places[id as usize];
            Arc::new([fold(&pl.primary), fold(pl.name(true))])
        })
    }

    /// The place's name and what it lies in, as an address line: "Paris, Ile-de-France, France".
    fn label(&self, id: u32) -> Arc<str> {
        remember(&self.suggest.labels, id, || {
            let mut keys: Vec<&str> = vec![];
            let mut parts: Vec<&str> = vec![];
            for p in self.chain(id) {
                let pl = &self.places[p as usize];
                if !keys.contains(&pl.key()) {
                    keys.push(pl.key());
                    parts.push(pl.name(true));
                }
            }
            parts.join(", ").into()
        })
    }

    /// Whether `place` lies in what each qualifier names: a country, a region by name or code,
    /// or any larger place around it.
    fn qualifies(&self, place: u32, quals: &[(String, Vec<&str>)], open_last: bool) -> bool {
        let pl = &self.places[place as usize];
        let chain = self.chain(place);
        let country_named = quals.iter().any(|(_, ccs)| {
            ccs.contains(&&*pl.cc) || super::sovereign(&pl.cc).is_some_and(|cc| ccs.contains(&cc))
        });
        let matches =
            |n: &str, q: &str, open: bool| n == q || (open && q.len() >= 3 && n.starts_with(q));
        quals.iter().enumerate().all(|(i, (q, ccs))| {
            let open = open_last && i + 1 == quals.len();
            let code = format!("-{}", q.to_uppercase());
            let key = std::cell::LazyCell::new(|| super::name_key(q));
            ccs.contains(&&*pl.cc)
                || (q.len() <= 3 && pl.region.ends_with(&code))
                || chain.iter().skip(1).any(|&a| {
                    let names = self.folded_names(a);
                    matches(&names[0], q, open)
                        || matches(&names[1], q, open)
                        || ((ccs.is_empty() || country_named)
                            && self.places[a as usize].named(*key))
                        || (q.len() <= 3 && self.places[a as usize].region.ends_with(&code))
                })
        })
    }

    /// The English or primary name of the best-known place whose name is one edit from `text`
    /// ("Mississauaga", "Soihull") when `text` names nothing, with its first two letters right.
    /// None when no place, or several equally well known, is that close.
    pub fn similar_place(&self, text: &str, cc: Option<&str>) -> Option<String> {
        let key = fold(text);
        if key.chars().count() < 5 || !key.chars().all(|c| c.is_alphabetic() || c == ' ') {
            return None;
        }
        let head: String = key.chars().take(2).collect();
        let mut to = head.as_bytes().to_vec();
        *to.last_mut()? += 1;
        let mut best: Option<(u64, u32)> = None;
        let mut tied = false;
        let mut range = self
            .place_names
            .range()
            .ge(head.as_bytes())
            .lt(&to)
            .into_stream();
        while let Some((k, w)) = range.next() {
            let at = k.len() - 6;
            let id = u32::from_be_bytes(k[at + 2..].try_into().ok()?);
            let Ok(name) = std::str::from_utf8(&k[..at]) else {
                continue;
            };
            if k[at + 1] == 2
                || name == key
                || !super::one_edit(&key, name)
                || !cc.is_none_or(|c| self.places[id as usize].cc.eq_ignore_ascii_case(c))
            {
                continue;
            }
            match best {
                Some((bw, bid)) if bid != id && w == bw => tied = true,
                Some((bw, _)) if w <= bw => {}
                _ => {
                    best = Some((w, id));
                    tied = false;
                }
            }
        }
        let (_, id) = best.filter(|_| !tied)?;
        Some(self.places[id as usize].name(true).to_owned())
    }

    /// The places of `place_names` whose name is `text`, or starts with it when `open`, best
    /// first: place, fit (0 the name itself, 1 its main name starts with the text, 2 another
    /// language's does, `INSIDE` a later word of its main name does) and weight. With `typo`,
    /// the names one edit from `text` (or starting so when `open`) and with its first letter, all
    /// of fit `TYPO`. Reading a short prefix's range is the cost, so it is kept.
    fn name_candidates(&self, text: &str, open: bool, typo: bool) -> Candidates {
        if text.is_empty() {
            return Arc::new([]);
        }
        let key = (Box::<str>::from(text), open, typo);
        if let Some(hit) = self.place_prefixes.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let inside = text.chars().count() >= MIN_INSIDE;
        let mut best: HashMap<u32, (u64, u8)> = HashMap::new();
        let mut add = |k: &[u8], w: u64| {
            let at = k.len() - 6;
            let id = u32::from_be_bytes(k[at + 2..].try_into().unwrap());
            let flag = k[at + 1];
            if flag == 2 && !inside {
                return;
            }
            let fit = match flag {
                _ if typo => TYPO,
                2 => INSIDE,
                _ if at == text.len() => 0,
                _ => 1 + flag,
            };
            // a country's weight is for its own name: another language's word that happens to
            // be "lagos" or start "lima" is not worth a country's
            let w = match &*self.places[id as usize].subtype {
                "country" | "dependency" if flag >= 1 => w.min(OTHER_NAME_COUNTRY),
                _ => w,
            };
            let score = w * FIT[if typo { 1 + flag } else { fit.min(3) } as usize];
            let e = best.entry(id).or_insert((score, fit));
            if score > e.0 {
                *e = (score, fit);
            }
        };
        if typo {
            let mut first = [0; 4];
            let from = text
                .chars()
                .next()
                .map_or(&[][..], |c| c.encode_utf8(&mut first).as_bytes());
            let mut to = from.to_vec();
            if let Some(last) = to.last_mut() {
                *last += 1;
            }
            let dfa = if open {
                ONE_EDIT.build_prefix_dfa(text)
            } else {
                ONE_EDIT.build_dfa(text)
            };
            let mut found = self
                .place_names
                .search(NameWithin(&dfa))
                .ge(from)
                .lt(&to)
                .into_stream();
            while let Some((k, w)) = found.next() {
                add(k, w);
            }
        } else {
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
            let mut range = self.place_names.range().ge(&from).lt(&to).into_stream();
            while let Some((k, w)) = range.next() {
                add(k, w);
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
    /// starts with the text, 2 a name in another language does, 3 a name has it inside,
    /// `INSIDE` a later word of its main name starts with it (only with `Names::Inside`), `TYPO`
    /// a name is one edit from it (only with `Names::Typo`).
    fn suggest_places(
        &self,
        parts: &[String],
        open: bool,
        cc: Option<&str>,
        focus: Option<&Focus>,
        names: Names,
        want: usize,
    ) -> Vec<(u32, u8)> {
        let Some(first) = parts.first() else {
            return vec![];
        };
        let all = words_open(first, open && parts.len() == 1);
        // "paris fra": the whole text names a place, or its front does and its end says where
        let splits: Vec<usize> = if parts.len() == 1 && names != Names::Typo {
            std::iter::once(all.len())
                .chain((1..all.len()).rev().take(3))
                .collect()
        } else {
            vec![all.len()]
        };
        let typo = names == Names::Typo;
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
            // a qualifier of one letter says nothing yet, and no place matches it
            let quals: Vec<String> = quals
                .into_iter()
                .filter(|q| !(open && q.chars().count() < 2))
                .collect();
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
            if typo
                && (text.chars().count() < MIN_TYPO
                    || !text.chars().all(|c| c.is_alphabetic() || c == ' '))
            {
                return vec![];
            }
            let acceptable = |id: u32| {
                let pl = &self.places[id as usize];
                pl.subtype.as_ref() != "microhood"
                    && cc.is_none_or(|c| pl.cc.eq_ignore_ascii_case(c))
                    && (quals.is_empty() || self.qualifies(id, &quals, open))
            };
            let mut ranked: Vec<(Reverse<u64>, u8, u32)> = vec![];
            let forms = if head_open && !typo {
                open_forms(head)
            } else {
                vec![text.clone()]
            };
            let mut candidates: Vec<(u32, u8, u64)> = vec![];
            for form in &forms {
                candidates.extend(self.name_candidates(form, head_open, typo).iter());
            }
            // "koeln" is read as "köln" only when no place is named as typed
            let umlaut = umlaut_form(&text).filter(|_| !typo);
            for n in 0..2 {
                if n == 1 {
                    match &umlaut {
                        Some(alt) if ranked.is_empty() => {
                            candidates = self.name_candidates(alt, head_open, false).to_vec()
                        }
                        _ => break,
                    }
                }
                if let Some(focus) = focus {
                    for c in &mut candidates {
                        c.2 = self.focused(c.2, c.0, focus);
                    }
                }
                candidates.sort_unstable_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
                // candidates come best first, so the first `want` that qualify are the best
                for (examined, &(id, fit, score)) in candidates.iter().enumerate() {
                    if ranked.len() >= want || examined >= EXAMINED {
                        break;
                    }
                    if (names == Names::Inside || fit != INSIDE)
                        && !found.iter().any(|f| f.0 == id)
                        && !ranked.iter().any(|r| r.2 == id)
                        && acceptable(id)
                    {
                        ranked.push((Reverse(score), fit, id));
                    }
                }
            }
            // The rest of the places, by the words the term dictionary holds: those as typed
            // first, for the places a prefix adds ("paris" -> "parish") are many.
            if ranked.len() < want && !typo {
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
                    let score = weight(pl) * FIT[fit as usize];
                    let score = focus.map_or(score, |f| self.focused(score, id, f));
                    ranked.push((Reverse(score), fit, id));
                }
            }
            ranked.sort_unstable();
            found.extend(ranked.into_iter().map(|(_, fit, id)| (id, fit)));
        }
        found.truncate(want);
        found
    }

    fn focused(&self, score: u64, id: u32, focus: &Focus) -> u64 {
        let pl = &self.places[id as usize];
        (score as f64 * focus.boost(pl.y as f64, pl.x as f64)) as u64
    }

    fn suggest_landmarks(&self, typed: &Typed, cc: Option<&str>, want: usize) -> Vec<Suggestion> {
        let first = &typed.parts[0];
        // two letters start too many names to rank
        if first.chars().count() < 3 {
            return vec![];
        }
        let one_part = typed.parts.len() == 1;
        let open = typed.open && one_part;
        let words = words_open(first, open);
        let forms = if open {
            open_forms(&words)
        } else {
            vec![words.join(" ")]
        };
        let quals: Vec<(String, Vec<&str>)> = typed.parts[1..]
            .iter()
            .map(|p| fold(p))
            .filter(|q| !(typed.open && q.chars().count() < 2))
            .map(|q| {
                let ccs = self.countries_named(&q, typed.open);
                (q, ccs)
            })
            .collect();
        let mut ids: Vec<u32> = vec![];
        for form in &forms {
            for &id in self.landmarks.find(form, open).iter() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids.sort_unstable_by_key(|&id| (Reverse(self.landmarks.list[id as usize].score), id));
        ids.into_iter()
            .map(|id| (id, &self.landmarks.list[id as usize]))
            .filter(|(_, l)| {
                cc.is_none_or(|c| self.places[l.place as usize].cc.eq_ignore_ascii_case(c))
                    && quals.iter().enumerate().all(|(i, (q, _))| {
                        let open = typed.open && i + 1 == quals.len();
                        let names = self.folded_names(l.place);
                        names
                            .iter()
                            .any(|n| n == q || open && q.len() >= 3 && n.starts_with(q))
                            || self.qualifies(l.place, &quals[i..=i], open)
                    })
            })
            .take(want)
            .map(|(id, _)| self.landmark_suggestion(id))
            .collect()
    }

    fn landmark_suggestion(&self, id: u32) -> Suggestion {
        let l = &self.landmarks.list[id as usize];
        Suggestion {
            id: format!("l{id}"),
            retrieval_id: format!("{}/l{id}", self.suggestion_scope),
            display_name: format!("{}, {}", self.landmarks.name(l), self.label(l.place)),
            unverified_suffix: String::new(),
            kind: "landmark",
            category: l.category,
            country_code: self.places[l.place as usize].cc.to_string(),
            x: l.x as f64,
            y: l.y as f64,
            bbox: None,
        }
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
                    if number.is_empty() || number.len() > MAX_NUMBER {
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

    fn postcode_suggestion(&self, a: tantivy::DocAddress) -> Option<Suggestion> {
        let doc: tantivy::TantivyDocument = self.searcher.doc(a).ok()?;
        let place = self.fast(a, "place") as u32;
        let mut display = vec![stored(&doc, self.f.name)];
        let mut country_code = String::new();
        if place != NONE {
            display.push(self.label(place).to_string());
            country_code = self.places[place as usize].cc.to_string();
        }
        Some(Suggestion {
            id: format!("z{}:{}", country_code, pc_norm(&stored(&doc, self.f.pcode))),
            retrieval_id: format!("{}/z{}:{}", self.suggestion_scope, a.segment_ord, a.doc_id),
            display_name: display.join(", "),
            unverified_suffix: String::new(),
            kind: "postcode",
            category: "",
            country_code,
            x: self.fast_f64(a, "x"),
            y: self.fast_f64(a, "y"),
            bbox: None,
        })
    }

    fn place_suggestion(&self, id: u32) -> Suggestion {
        let pl = &self.places[id as usize];
        Suggestion {
            id: format!("p{id}"),
            retrieval_id: format!("{}/p{id}", self.suggestion_scope),
            display_name: self.label(id).to_string(),
            unverified_suffix: String::new(),
            kind: place_kind(pl),
            category: "",
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
        qualifiers: &[String],
        open: bool,
        cc: Option<&str>,
        limit: usize,
    ) -> Vec<Suggestion> {
        let pc = pc_norm(text);
        if pc.len() < MIN_POSTCODE {
            return vec![];
        }
        let scope = (!qualifiers.is_empty()).then(|| self.scope(qualifiers, open, cc, None, true));
        let pool = if scope.is_some() { POOL } else { limit * 2 };
        // the code as typed before the longer ones it starts
        let mut docs = self.top_of(
            Kind::Postcode,
            cc,
            vec![must(term(self.f.pcode, &pc))],
            pool,
        );
        if qualifiers.is_empty()
            && open
            && let Some((cl, _)) = self.typed_words(self.f.pcode, std::slice::from_ref(&pc), true)
        {
            for a in self.top_of(Kind::Postcode, cc, cl, pool) {
                if !docs.contains(&a) {
                    docs.push(a);
                }
            }
        }
        docs.into_iter()
            .filter_map(|a| {
                let place = self.fast(a, "place") as u32;
                if scope.as_ref().is_some_and(|ids| {
                    place == NONE || !self.chain(place).iter().any(|id| ids.contains(id))
                }) {
                    return None;
                }
                self.postcode_suggestion(a)
            })
            .take(limit)
            .collect()
    }

    fn suggest_number_comma(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        let [number, road, rest @ ..] = typed.parts.as_slice() else {
            return vec![];
        };
        if number.len() > MAX_NUMBER
            || !number.starts_with(|c: char| c.is_ascii_digit())
            || !number
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'/' || c == b'-')
        {
            return vec![];
        }
        let mut parts = vec![format!("{number} {road}")];
        parts.extend_from_slice(rest);
        self.suggest_streets(
            &Typed {
                parts,
                open: typed.open,
            },
            cc,
            focus,
            want,
        )
        .into_iter()
        .filter(|s| s.kind == "address")
        .collect()
    }

    /// Is the text Australian: Australia filtered for or named, or a part that is a state's
    /// code or ends with one and a postcode ("Crace ACT 2911")?
    fn australian(&self, typed: &Typed, cc: Option<&str>) -> bool {
        if let Some(cc) = cc {
            return cc.eq_ignore_ascii_case("au");
        }
        let state = |w: &str| {
            self.regions
                .contains_key(&format!("AU-{}", w.to_uppercase()))
        };
        typed.parts[1..].iter().any(|p| {
            let words: Vec<&str> = p.split(' ').collect();
            self.country(p).as_deref() == Some("au")
                || matches!(words.as_slice(), [code] if state(code))
                || matches!(words.as_slice(), [.., code, pc]
                    if state(code) && pc.len() == 4 && pc.bytes().all(|b| b.is_ascii_digit()))
        })
    }

    /// Houses of an Australian text that finds none as typed: with Australia's state codes and
    /// postcodes understood when it is not stated, then with a unit before the building's number
    /// set aside. The unit stays in the display name, unverified.
    fn suggest_australian(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        if !self.australian(typed, cc) {
            return vec![];
        }
        let houses = |typed: &Typed| -> Vec<Suggestion> {
            self.suggest_streets(typed, Some("au"), focus, want)
                .into_iter()
                .filter(|s| s.kind == "address" && s.country_code == "au")
                .collect()
        };
        if cc.is_none() {
            let found = houses(typed);
            if !found.is_empty() {
                return found;
            }
        }
        let Some((unit, slash, building)) = australian_unit(typed) else {
            return vec![];
        };
        houses(&building)
            .into_iter()
            .map(|mut s| {
                s.display_name = if slash {
                    format!("{unit}/{}", s.display_name)
                } else {
                    format!("{unit}, {}", s.display_name)
                };
                s.unverified_suffix = unit.clone();
                s
            })
            .collect()
    }

    fn suggest_floor(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        if cc.is_some_and(|c| !c.eq_ignore_ascii_case("dk"))
            || typed.parts.len() < 2
            || !danish_floor(&typed.parts[1])
        {
            return vec![];
        }
        let mut building = Typed {
            parts: typed.parts.clone(),
            open: typed.open && typed.parts.len() > 2,
        };
        let suffix = building.parts.remove(1);
        self.suggest_streets(&building, Some("dk"), focus, want)
            .into_iter()
            .filter(|s| s.kind == "address" && s.country_code == "dk")
            .map(|mut s| {
                if let Some((at, _)) = s.display_name.match_indices(", ").nth(1) {
                    s.display_name.insert_str(at, &format!(", {suffix}"));
                }
                s.unverified_suffix = suffix.clone();
                s
            })
            .collect()
    }

    /// Streets, and houses when a number is typed, with the road in the first part and the
    /// town after a comma ("12 main st, spring"). With no comma the town may still end the text.
    fn suggest_streets(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        let one_part = typed.parts.len() == 1;
        let country = (!one_part)
            .then(|| {
                typed
                    .parts
                    .last()
                    .filter(|p| p.chars().count() > 3)
                    .and_then(|p| {
                        let cc = self.country(p)?;
                        // a region named so ("Georgia"), not one with the country's name as
                        // a later word ("South Australia", "Capital Region of Denmark")
                        (!self.name_candidates(&fold(p), false, false).iter().any(
                            |&(id, fit, _)| {
                                fit != INSIDE
                                    && (self.places[id as usize].subtype.as_ref() == "region"
                                        || self.own_codes.contains_key(&id)
                                        || self.subdivision_codes.contains_key(&id))
                            },
                        ))
                        .then_some(cc)
                    })
            })
            .flatten();
        if cc
            .zip(country.as_deref())
            .is_some_and(|(a, b)| !a.eq_ignore_ascii_case(b))
        {
            return vec![];
        }
        let stated = cc.is_some();
        let cc = cc.or(country.as_deref());
        let qualifiers = typed.parts[1..].join(", ");
        let extracted = qualifiers
            .chars()
            .any(|c| c.is_ascii_digit())
            .then(|| self.find_postcode(&qualifiers, cc))
            .flatten();
        // a stated France also means the places of its overseas departments, which have codes of
        // their own (their streets are France's)
        static SOVEREIGNS: LazyLock<std::collections::HashSet<&str>> = LazyLock::new(|| {
            let letters = || b'a'..=b'z';
            letters()
                .flat_map(|a| letters().map(move |b| String::from_utf8(vec![a, b]).unwrap()))
                .filter_map(|t| super::sovereign(&t))
                .collect()
        });
        let place_cc = cc.filter(|c| stated || !SOVEREIGNS.contains(c.to_lowercase().as_str()));
        let parts = extracted
            .as_ref()
            .map(|(_, rest)| Typed::new(rest).parts)
            .unwrap_or_else(|| typed.parts[1..].to_vec());
        let postcode = extracted.as_ref().map(|(pc, _)| pc.as_str());
        let raw: Vec<&str> = typed.parts[0].split(' ').collect();
        // the number: the first or last word, when a road remains; both when both could be
        // ("11 Listopada 26" is house 26 on a road named for a date)
        let number_at = (raw.len() > 1)
            .then(|| {
                [0, raw.len() - 1]
                    .into_iter()
                    .filter(|&i| has_digit(raw[i]) && raw[i].len() <= MAX_NUMBER)
            })
            .into_iter()
            .flatten();
        let mut tries: Vec<(Option<usize>, Vec<&str>)> = vec![];
        for i in number_at {
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
            if postcode.is_some()
                && number_word.is_some()
                && (parts.len() == 1 || ((2..=3).contains(&parts.len()) && country.is_some()))
            {
                // "9 pittevej, aidt, 8881 thorsø, denmark": a place between the road and the
                // postcode's town is another name of the house's place, or its postal town
                let candidates: &[usize] = if parts.len() == 3 { &[1, 0] } else { &[0] };
                for &at in candidates {
                    let city = parts
                        .get(at)
                        .filter(|p| country.is_none() || self.country(p) != country)
                        .map(|p| (p.as_str(), typed.open && parts.len() == 1));
                    let found = self.streets_typed(
                        &road_words,
                        false,
                        None,
                        number_word,
                        false,
                        cc,
                        postcode,
                        city,
                        want,
                    );
                    if found.iter().any(|s| s.kind == "address") {
                        return found;
                    }
                }
            }
            let found = if one_part {
                // "main st springfield": the end of the text may name the town, and then the
                // streets there are better than any other "main st"
                let mut found = vec![];
                let mut split = road_words.len();
                while split > 1 && road_words.len() - split < 3 && found.len() < want {
                    split -= 1;
                    let town = [road_words[split..].join(" ")];
                    let ids = self.scope(&town, road_open, cc, focus, false);
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
                        postcode,
                        None,
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
                    postcode,
                    None,
                    want,
                ));
                found
            } else {
                let mut ids = self.scope(&parts, typed.open, place_cc, focus, true);
                if ids.is_empty() && country.is_some() {
                    ids = self.scope(&parts[..parts.len() - 1], false, place_cc, focus, true);
                }
                let mut found = if ids.is_empty() {
                    vec![]
                } else {
                    self.streets_typed(
                        &road_words,
                        false,
                        Some(&ids),
                        number_word,
                        false,
                        cc,
                        postcode,
                        None,
                        want,
                    )
                };
                // a suburb the houses' addresses name but their places do not ("2 Clara St,
                // Randwick" is filed under Sydney), with nothing else after it but the country.
                // A region code written after it ("Kingsford QLD") holds the houses to that region.
                if number_word.is_some()
                    && !found.iter().any(|s| s.kind == "address")
                    && (parts.len() == 1 || (parts.len() == 2 && country.is_some()))
                    && let Some(town) = parts
                        .first()
                        .filter(|p| country.is_none() || self.country(p) != country)
                {
                    let coded = town.rsplit_once(' ').and_then(|(name, code)| {
                        let region = self
                            .regions
                            .get(&format!("{}-{code}", cc?).to_uppercase())?;
                        Some((name, [*region]))
                    });
                    let (town, region) = match &coded {
                        Some((name, region)) => (*name, Some(&region[..])),
                        None => (town.as_str(), None),
                    };
                    // with a postcode, the town alone was looked for already
                    if postcode.is_none() || region.is_some() {
                        let by_town = self.streets_typed(
                            &road_words,
                            false,
                            region,
                            number_word,
                            false,
                            cc,
                            postcode,
                            Some((town, typed.open && parts.len() == 1)),
                            want,
                        );
                        if by_town.iter().any(|s| s.kind == "address") {
                            found = by_town;
                        }
                    }
                }
                found
            };
            if !found.is_empty() {
                out = found;
                break;
            }
        }
        if !out.iter().any(|s| s.kind == "address") && raw.len() > 2 {
            for start in [0, raw.len() - 2] {
                if let Some(number) = compact_house_suffix(&raw[start..start + 2].join(" ")) {
                    let mut parts = typed.parts.clone();
                    parts[0] = raw[..start]
                        .iter()
                        .copied()
                        .chain([number.as_str()])
                        .chain(raw[start + 2..].iter().copied())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let found = self.suggest_streets(
                        &Typed {
                            parts,
                            open: typed.open,
                        },
                        cc,
                        focus,
                        want,
                    );
                    if found.iter().any(|s| s.kind == "address") {
                        return found;
                    }
                }
            }
        }
        // "ul. M. Kopernika 11": the index names it Mikołaja Kopernika, mostly without "ul."
        if !out.iter().any(|s| s.kind == "address")
            && let Some(bare) = crate::matching::bare_road(&typed.parts[0], cc)
        {
            let mut parts = typed.parts.clone();
            parts[0] = bare;
            let found = self.suggest_streets(
                &Typed {
                    parts,
                    open: typed.open,
                },
                cc,
                focus,
                want,
            );
            if out.is_empty() || found.iter().any(|s| s.kind == "address") {
                return found;
            }
        }
        // "12 Rua dos Farias": Portugal's register names it "R FARIAS". Only Portugal's form,
        // which expands no further: other countries' turn back ("straat" <-> "street").
        if !out.iter().any(|s| s.kind == "address")
            && typed.parts[0].split(' ').any(has_digit)
            && cc.is_some_and(|c| c.eq_ignore_ascii_case("pt"))
        {
            for road in crate::matching::road_expansions(&typed.parts[0], "pt") {
                let mut parts = typed.parts.clone();
                parts[0] = road;
                let found = self.suggest_streets(
                    &Typed {
                        parts,
                        open: typed.open,
                    },
                    cc,
                    focus,
                    want,
                );
                if found.iter().any(|s| s.kind == "address") {
                    return found;
                }
            }
        }
        out.truncate(want);
        out
    }

    /// Ids of the places the parts name, to scope streets to: those places and all inside them.
    fn scope(
        &self,
        parts: &[String],
        open: bool,
        cc: Option<&str>,
        focus: Option<&Focus>,
        codes: bool,
    ) -> Vec<u32> {
        // two letters of a name are too many places to scope streets to. Only after a comma is a
        // town misspelt or named by a later word ("main st, lodnon"): the end of "rue de rivoli"
        // or "united states" is not a town
        let short = open && parts.len() == 1 && parts[0].chars().count() < 3;
        let places = |names| -> Vec<u32> {
            self.suggest_places(parts, open, cc, focus, names, 8)
                .into_iter()
                .map(|(id, _)| id)
                .collect()
        };
        let mut ids = match (short, codes) {
            (true, _) => vec![],
            (false, false) => places(Names::Strict),
            (false, true) => {
                let ids = places(Names::Inside);
                if ids.is_empty() {
                    places(Names::Typo)
                } else {
                    ids
                }
            }
        };
        // "main st, il" and "10 downing st, uk": a region's code, a country's name or code. Only
        // after a comma: in "main st" the "st" is a word being typed.
        if let ([one], true) = (parts, codes) {
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
        postcode: Option<&str>,
        city: Option<(&str, bool)>,
        want: usize,
    ) -> Vec<Suggestion> {
        if road_open && road.len() == 1 && road[0].chars().count() < 3 {
            return vec![];
        }
        let Some((mut base, rarest)) = self.typed_words(self.f.names, road, road_open) else {
            return vec![];
        };
        let mut in_town = None;
        if let Some((city, open)) = city {
            let Some((clauses, rarest)) =
                self.typed_words(self.f.city, &words_open(city, open), open)
            else {
                return vec![];
            };
            base.extend(clauses);
            in_town = Some(rarest);
        }
        let in_scope = scope
            .map(|ids| {
                ids.iter()
                    .map(|id| self.docs_with(self.f.ids, &id.to_string()))
                    .sum::<u64>()
            })
            .into_iter()
            .chain(postcode.map(|pc| self.docs_with(self.f.pcode, &pc_norm(pc))))
            .chain(in_town)
            .min();
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
        // each name's best ranked part: its houses come before those of the name's other parts
        let mut first = HashMap::new();
        let mut segments = std::collections::HashSet::new();
        // the best place of the scope alone first: "5th avenue, new york" is Manhattan's
        // before any other in the state
        let phases: Vec<Option<&[u32]>> = match scope {
            Some(ids) if ids.len() > 1 => vec![Some(&ids[..1]), Some(ids)],
            other => vec![other],
        };
        for ids in phases {
            if first.len() >= want {
                break;
            }
            let mut cl: Vec<_> = base.iter().map(|(o, q)| (*o, q.box_clone())).collect();
            if let Some(pc) = postcode {
                cl.push(must(term(self.f.pcode, &pc_norm(pc))));
            }
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
                first.entry((fold(&name), place)).or_insert(sid);
                if segments.insert(sid) {
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
        // and the street so named before one that goes on ("Gran Vía" before "Gran Vía del Sur")
        streets.sort_by_cached_key(|s| {
            let name = fold(&s.3);
            (
                scope_rank(s.2),
                !name.starts_with(&road_text),
                name != road_text,
                first.get(&(name, s.2)) != Some(&s.1),
            )
        });
        let mut houses = vec![];
        let mut plain = vec![];
        // Each cell holds different houses; deduplicate names only after checking the number.
        let mut answered = std::collections::HashSet::new();
        for (a, sid, place, name) in streets {
            let key = (fold(&name), place);
            if answered.contains(&key) {
                continue;
            }
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
                    let (x, y, own) = schema::unpack(v);
                    if let Some(pc) = postcode.filter(|_| own > 0) {
                        use tantivy::schema::Value as _;
                        let doc: tantivy::TantivyDocument = self.searcher.doc(a).unwrap();
                        if doc
                            .get_all(self.f.pcode)
                            .nth(own as usize - 1)
                            .and_then(|v| v.as_str())
                            .map(pc_norm)
                            != Some(pc_norm(pc))
                        {
                            continue;
                        }
                    }
                    houses.push(self.street_suggestion(a, sid, place, &name, Some(&n), (x, y)));
                    found = true;
                }
            }
            if found {
                answered.insert(key);
            } else {
                plain.push((sid, place, name, a));
            }
        }
        let mut out = houses;
        for (sid, place, name, a) in plain {
            if answered.insert((fold(&name), place)) {
                let xy = (self.fast_f64(a, "x"), self.fast_f64(a, "y"));
                out.push(self.street_suggestion(a, sid, place, &name, None, xy));
            }
        }
        out.truncate(want);
        out
    }

    fn street_suggestion(
        &self,
        a: tantivy::DocAddress,
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
            if let Some(chain) = number.and_then(|_| self.house_chain(&self.chain(place), x, y)) {
                self.address(&chain, true, &mut Map::new(), &mut display);
            } else {
                display.push(self.label(place).to_string());
            }
            country_code = self.places[place as usize].cc.to_string();
        }
        Suggestion {
            id: match number {
                Some(n) => format!("h{sid}:{n}"),
                None => format!("s{sid}"),
            },
            retrieval_id: format!(
                "{}/{}{}:{}{}",
                self.suggestion_scope,
                if number.is_some() { "h" } else { "s" },
                a.segment_ord,
                a.doc_id,
                number
                    .map(|n| format!(":{}", schema::number_key(n)))
                    .unwrap_or_default(),
            ),
            display_name: display.join(", "),
            unverified_suffix: String::new(),
            kind: if number.is_some() {
                "address"
            } else {
                "street"
            },
            category: "",
            country_code,
            x,
            y,
            bbox: None,
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn danish_unit_parts_are_floors_doors_and_nothing_else() {
        for unit in [
            "st.", "kl.", "1.", "st. th", "1. tv", "st. 11", "2. 3b", "1. c", "th",
        ] {
            assert!(danish_floor(unit), "{unit}");
        }
        for other in [
            "st. john", "st john", "12 main", "1. 12345", "th mf", "", "Grenaa", "8500",
        ] {
            assert!(!danish_floor(other), "{other}");
        }
    }
    use super::*;

    #[test]
    fn typed_words_and_digraphs() {
        // a hyphen ends a word like a space: "saint" is whole and abbreviated, as indexed
        assert_eq!(words_open("saint-etien", true), ["st", "etien"]);
        assert_eq!(words_open("saint-etienne", false), ["st", "etienne"]);
        assert_eq!(words_open("calle", true), ["calle"]);
        for name in ["東京都", "上海市", "서울특별시", "İstanbul", "İzmir"] {
            assert_eq!(words_open(name, true), words(name), "{name}");
        }
        assert_eq!(umlaut_form("koeln").as_deref(), Some("koln"));
        assert_eq!(umlaut_form("muenchen").as_deref(), Some("munchen"));
        assert_eq!(umlaut_form("paris"), None);
    }

    #[test]
    fn a_place_added_from_geonames_weighs_a_localitys_unknown_size() {
        let place = |class: &str, population| Place {
            subtype: "locality".into(),
            class: class.into(),
            cc: "vn".into(),
            region: "".into(),
            population,
            x: 0.0,
            y: 0.0,
            bbox: [f32::NAN; 4],
            parent: NONE,
            primary: "Huyện Lâm Hà".into(),
            names: "".into(),
            en: Place::english(""),
            keys: Box::default(),
            city_like: false,
            settlement_twin: false,
        };
        assert_eq!(weight(&place("geonames", 90_000)), 100);
        assert_eq!(weight(&place("", 90_000)), 90_000);
        assert_eq!(weight(&place("", 0)), 100);
    }

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
            place(
                "country",
                "",
                "DK",
                "",
                6_000_000,
                10.0,
                56.0,
                "",
                "Denmark",
                &en("Denmark"),
            ),
            place(
                "locality",
                "city",
                "DK",
                "DK-84",
                600_000,
                12.57,
                55.68,
                "9",
                "Copenhagen",
                &en("Copenhagen"),
            ),
            place(
                "locality",
                "village",
                "DK",
                "DK-84",
                6_000,
                12.0,
                55.0,
                "9",
                "4",
                &en("4"),
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
        // Rue de Rivoli crosses a cell edge: a second part (10) holds house 26. Street 11 is
        // named for a date.
        for (sid, name, x) in [
            (7u64, "Rue de Rivoli", 2.34f64),
            (10, "Rue de Rivoli", 2.36),
            (11, "11 Listopada", 2.35),
        ] {
            let mut street = tantivy::doc!(
                f.kind => "s", f.names => name, f.country => "FR", f.name => name,
                f.id => sid, f.place => 2u64, f.importance => 50u64, f.x => x, f.y => 48.86f64
            );
            for id in ["2", "1", "0"] {
                street.add_text(f.ids, id);
            }
            writer.add_document(street).unwrap();
        }
        for (sid, name, place) in [(8u64, "Examplegade", 10u64), (9, "Scopegade", 11)] {
            writer
                .add_document(tantivy::doc!(
                    f.kind => "s", f.names => name, f.country => "DK", f.name => name,
                    f.id => sid, f.place => place, f.ids => place.to_string(), f.ids => "9",
                    f.importance => 50u64, f.x => 12.57f64, f.y => 55.68f64
                ))
                .unwrap();
        }
        writer
            .add_document(tantivy::doc!(
                f.kind => "z", f.pcode => "75001", f.name => "75001", f.country => "FR",
                f.place => 2u64, f.importance => 10u64, f.x => 2.336f64, f.y => 48.862f64
            ))
            .unwrap();
        writer.commit().unwrap();
        drop(writer);

        let mut fst = tantivy_fst::MapBuilder::memory();
        for (sid, numbers) in [
            (7u32, &["12", "120", "12a"][..]),
            (8, &["12", "120", "12a"][..]),
            (9, &["12"][..]),
            (10, &["26"][..]),
            (11, &["26"][..]),
        ] {
            for n in numbers {
                let mut key = sid.to_be_bytes().to_vec();
                key.extend_from_slice(n.as_bytes());
                let (x, y) = match sid {
                    7 => (2.3 + n.len() as f64 / 100.0, 48.86),
                    8 => (12.57, 55.68),
                    _ => (12.0, 55.0),
                };
                let (x, y) = schema::pack(x, y, 0);
                fst.insert(key, (x as u64) << 32 | y as u64).unwrap();
            }
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
    fn retrieval_ids_survive_reopening_and_reject_invalid_references() {
        let (index, dir) = tiny();
        let reopened = Index::open(&dir, Arc::new(Parser::unloaded(0))).unwrap();
        let suggestion = index.suggest("paris", 1, None, None, false).remove(0);
        let found = reopened
            .retrieve_suggestion(&suggestion.retrieval_id)
            .unwrap();
        assert_eq!(found.retrieval_id, suggestion.retrieval_id);
        assert_eq!((found.x, found.y), (suggestion.x, suggestion.y));
        for key in [
            "",
            "p4294967295",
            "l4294967295",
            "s4294967295:0",
            "s0:4294967295",
            "z0:4294967295",
            "h0:0",
            "s0:0:extra",
            "💥",
            "h0:0:",
            "h0:0:unknown",
            "x0",
        ] {
            let id = format!("{}/{key}", index.suggestion_scope);
            assert!(index.retrieve_suggestion(&id).is_none(), "{key}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn completes_places_by_name_start_and_weight() {
        let (index, dir) = tiny();
        let cold = names(&index.suggest("par", 5, None, None, false)).join("|");
        index.warm_suggest();
        assert_eq!(
            names(&index.suggest("par", 5, None, None, false)).join("|"),
            cold
        );
        let found = index.suggest("par", 5, None, None, false);
        assert_eq!(found[0].id, "p2");
        assert_eq!(
            (found[0].kind, found[0].country_code.as_str()),
            ("city", "fr")
        );
        assert_eq!(found[1].id, "p5");
        assert_eq!(found[0].display_name, "Paris, Ile-de-France, France");
        // the whole word typed, and then the town's country or region
        assert_eq!(index.suggest("paris tex", 5, None, None, false)[0].id, "p5");
        assert_eq!(index.suggest("paris, tx", 5, None, None, false)[0].id, "p5");
        assert_eq!(index.suggest("paris, fr", 5, None, None, false)[0].id, "p2");
        assert_eq!(
            index.suggest("paris, unit", 5, None, None, false)[0].id,
            "p5"
        );
        // a hamlet named exactly as typed does not outrank the city the text starts
        assert_eq!(index.suggest("londo", 5, None, None, false)[0].id, "p7");
        assert_eq!(index.suggest("londo ", 5, None, None, false)[0].id, "p8");
        assert_eq!(
            names(&index.suggest("PARIS", 1, Some("us"), None, false)),
            ["Paris, Texas, United States"]
        );
        assert!(index.suggest("p", 5, None, None, false).is_empty());
        assert!(index.suggest("  ,, ", 5, None, None, false).is_empty());
        assert!(index.suggest("zzzzz", 5, None, None, false).is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_name_one_edit_off_finds_the_best_known_place() {
        let (index, dir) = tiny();
        assert_eq!(
            index.similar_place("Pariss", None).as_deref(),
            Some("Paris")
        );
        assert_eq!(
            index.similar_place("Parris", Some("us")).as_deref(),
            Some("Paris")
        );
        // the name itself, a different one, a short one and one in another country
        assert_eq!(index.similar_place("Paris", None), None);
        assert_eq!(index.similar_place("Lyonn", None), None);
        assert_eq!(index.similar_place("Pari", None), None);
        assert_eq!(index.similar_place("Pariss", Some("gb")), None);
        // "kingdom" is a key of United Kingdom, but not one of its names
        assert_eq!(index.similar_place("Kingdon", None), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn typos_words_inside_names_and_the_users_position() {
        let (index, dir) = tiny();
        let ids = |text: &str, focus: Option<&Focus>| -> Vec<String> {
            index
                .suggest(text, 5, None, focus, false)
                .into_iter()
                .map(|s| s.id)
                .collect()
        };
        // one transposition, deletion or substitution, while typing and once typed
        for typo in [
            "lodnon",
            "lndon",
            "londn",
            "lodnon, united kingdom",
            "lomdon ",
        ] {
            assert_eq!(ids(typo, None)[..1], ["p7"], "{typo}");
        }
        // but not the first letter, not two edits and not a short text
        for far in ["ondon", "lnodno", "pariz, france", "lndn"] {
            assert!(!ids(far, None).contains(&"p7".into()), "{far}");
        }
        assert_eq!(ids("pariz ", None)[..2], ["p2", "p5"]);
        // a name typed exactly comes before one a letter off
        assert_eq!(ids("londo ", None), ["p8", "p7"]);
        // a later word of a name, from its third letter
        assert_eq!(ids("kingd", None), ["p6"]);
        assert_eq!(ids("states", None), ["p3"]);
        assert!(index.name_candidates("ki", true, false).is_empty());
        // Paris, Texas near Dallas; Paris, France near Lyon or with no position
        let dallas = Focus {
            lat: 32.78,
            lon: -96.8,
            scale_km: 100.0,
        };
        let lyon = Focus {
            lat: 45.76,
            lon: 4.84,
            scale_km: 100.0,
        };
        assert_eq!(ids("paris", Some(&dallas))[..2], ["p5", "p2"]);
        assert_eq!(ids("pari", Some(&lyon))[..2], ["p2", "p5"]);
        assert_eq!(ids("paris", None)[..2], ["p2", "p5"]);
        // and as the town that scopes a street
        assert_eq!(ids("rue de rivoli, paris", Some(&dallas))[0], "s7");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn focus_boost_falls_with_distance() {
        let f = Focus {
            lat: 0.0,
            lon: 0.0,
            scale_km: 100.0,
        };
        assert_eq!(f.boost(0.0, 0.0), 1001.0);
        // a degree of longitude at the equator is 111.2 km
        let at = |km: f64| f.boost(0.0, km / 111.195);
        assert!((at(100.0) - 501.0).abs() < 1.0);
        assert!((at(1000.0) - 10.9).abs() < 0.1);
        assert!(f.boost(0.0, 180.0) < 1.03);
    }

    #[test]
    fn a_comma_can_separate_the_house_number_from_the_road() {
        let (index, dir) = tiny();
        for (query, id) in [
            ("12, rue de rivoli", "h7:12"),
            (" 12 , rue de rivoli ", "h7:12"),
            ("12, rue de riv", "h7:12"),
            ("12, rue de rivoli, par", "h7:12"),
            ("12, rue de rivoli, paris, france", "h7:12"),
            ("12A, rue de rivoli, paris", "h7:12a"),
            ("120, rue de rivoli, paris", "h7:120"),
        ] {
            let found = index.suggest(query, 5, None, None, false);
            assert_eq!(found.len(), 1, "{query}");
            let s = &found[0];
            assert_eq!(s.id, id, "{query}");
            assert_eq!(s.kind, "address");
            assert_eq!(s.country_code, "fr");
        }
        for query in [
            "1, rue de rivoli",
            "99, rue de rivoli",
            "12, rue de rivoli, london",
            "12, rue de rivoli, paris, united states",
            "12, rue de rivoli, paris, texas",
            "12,",
            "12345678901, rue de rivoli",
            "75001, london",
        ] {
            assert!(
                index.suggest(query, 5, None, None, false).is_empty(),
                "{query}"
            );
        }
        assert!(
            index
                .suggest("12, rue de rivoli", 5, Some("us"), None, false)
                .is_empty()
        );
        assert!(
            index
                .suggest("12, rue de rivoli", 0, None, None, false)
                .is_empty()
        );
        assert_eq!(
            index.suggest("75001, paris", 1, None, None, false)[0].id,
            "zfr:75001"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn floor_fallback_uses_the_buildings_country_and_preserves_constraints() {
        let (index, dir) = tiny();
        let base = index
            .suggest("Examplegade 12, Copenhagen", 1, None, None, false)
            .remove(0);
        for suffix in ["4", "4.", "4. t", "4. th.", "ST. TV.", "kl.", "12. mf."] {
            for tail in ["", ", Copen", ", Copenhagen, Denmark"] {
                let query = format!("Examplegade 12, {suffix}{tail}");
                let found = index.suggest(&query, 5, None, None, false);
                assert_eq!(found.len(), 1, "{query}");
                let s = &found[0];
                assert_eq!((&s.id, s.x, s.y), (&base.id, base.x, base.y), "{query}");
                assert_eq!(s.unverified_suffix, suffix);
                assert_eq!(
                    s.display_name,
                    format!("12, Examplegade, {suffix}, Copenhagen, Denmark")
                );
                assert!(s.id.ends_with(":12"));
            }
        }
        let letter = index.suggest("Examplegade 12A, 4", 1, Some("DK"), None, false);
        assert_eq!(letter[0].id, "h8:12a");
        assert_eq!(letter[0].unverified_suffix, "4");
        for query in [
            "Examplegade 1, 4",
            "Examplegade 99, 4",
            "Examplegade, 4",
            "Examplegade 12, 1550",
            "Examplegade 12, 4, London",
            "Examplegade 12, 4, United States",
            "Examplegade 12, 4, 9999",
            "Examplegade 12, not a floor",
            "12 rue de rivoli, 4",
        ] {
            assert!(
                index.suggest(query, 5, None, None, false).is_empty(),
                "{query}"
            );
        }
        assert!(
            index
                .suggest("Examplegade 12, 4", 5, Some("fr"), None, false)
                .is_empty()
        );
        assert!(
            index
                .suggest("Examplegade 12, 4", 0, None, None, false)
                .is_empty()
        );
        let literal = index.suggest("Scopegade 12, 4,", 1, None, None, false);
        assert_eq!(literal[0].id, "h9:12");
        assert!(literal[0].unverified_suffix.is_empty());
        assert!(base.unverified_suffix.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn completes_streets_houses_and_postcodes() {
        let (index, dir) = tiny();
        let street = &index.suggest("rue de riv", 5, None, None, false)[0];
        assert_eq!((street.id.as_str(), street.kind), ("s7", "street"));
        assert_eq!(
            street.display_name,
            "Rue de Rivoli, Paris, Ile-de-France, France"
        );
        // the town after a comma, still being typed
        assert!(
            index
                .suggest("rue de rivoli, par", 5, None, None, false)
                .iter()
                .any(|s| s.id == "s7")
        );
        let house = &index.suggest("12 rue de rivoli, par", 5, None, None, false)[0];
        assert_eq!((house.id.as_str(), house.kind), ("h7:12", "address"));
        assert!(house.display_name.starts_with("12, Rue de Rivoli, Paris"));
        // a house number typed last is being typed: 12 and 120
        let ids: Vec<_> = index
            .suggest("rue de rivoli 12", 5, None, None, false)
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids[..2], ["h7:12", "h7:120"]);
        // "ul." (ulica), which the index leaves out
        assert_eq!(
            index.suggest("ul. rue de rivoli 12, paris", 5, None, None, false)[0].id,
            "h7:12"
        );
        // a house on the street's other part
        assert_eq!(
            index.suggest("26 rue de rivoli, paris", 5, None, None, false)[0].id,
            "h10:26"
        );
        // the number last, after a road named for a date
        assert_eq!(
            index.suggest("11 listopada 26, paris", 5, None, None, false)[0].id,
            "h11:26"
        );
        // no such number: the street
        assert_eq!(
            index.suggest("99 rue de rivoli", 5, None, None, false)[0].id,
            "s7"
        );
        // no place by that name in the scope
        assert!(
            index
                .suggest("rue de rivoli, texas", 5, None, None, false)
                .is_empty()
        );
        let code = &index.suggest("7500", 5, None, None, false)[0];
        assert_eq!((code.id.as_str(), code.kind), ("zfr:75001", "postcode"));
        assert!(index.suggest("75", 5, None, None, false).is_empty());
        assert!(
            index
                .suggest("rue de riv", 5, Some("us"), None, false)
                .is_empty()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
