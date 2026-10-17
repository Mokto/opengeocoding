//! Landmark rows and names, mapped separately from the address index.
use super::{SEP, schema::fold};
use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tantivy::directory::{Directory, MmapDirectory, OwnedBytes, error::OpenReadError};
use tantivy_fst::{IntoStreamer, Streamer};

pub(super) struct Landmark {
    pub category: &'static str,
    /// The place it lies in, by `places.tsv` line.
    pub place: u32,
    pub x: f32,
    pub y: f32,
    // Keep names file-backed so unused landmark pages can be reclaimed.
    name: std::ops::Range<usize>,
    pub score: u64,
}

pub(super) struct Landmarks {
    pub list: Vec<Landmark>,
    /// Folded name, a NUL, whether the name starts there (0) or at a later word of it (1), and the
    /// landmark number -> its score.
    names: tantivy_fst::Map<OwnedBytes>,
    text: OwnedBytes,
    /// `find` results by text and whether it is open: a short prefix walks thousands of names.
    found: Mutex<HashMap<(Box<str>, bool), Found>>,
}

const CATEGORIES: [&str; 7] = [
    "airport",
    "station",
    "attraction",
    "museum",
    "university",
    "hospital",
    "stadium",
];
pub(super) type Found = Arc<[u32]>;
/// Landmarks a name keeps, best first, for the country filter and qualifiers to pick from.
const CANDIDATES: usize = 400;
/// Entries of the `find` cache, each at most 400 landmark ids.
const MAX_CACHED: usize = 20_000;
/// Not language names: these count no fame.
const OTHER_KEYS: [&str; 5] = [
    "alt_name",
    "official_name",
    "short_name",
    "int_name",
    "iata",
];

/// Airports, then stations, then the rest; within a tier, the landmarks named in more languages.
// ponytail: the number of OSM `name:xx` tags stands in for fame, which favours what is known
// abroad over what is known locally, and a category beats any name fit (an airport whose name
// starts with the text comes before a station named exactly so). Wikidata sitelink counts are the
// upgrade.
fn score(category: &str, languages: usize) -> u64 {
    let tier = match category {
        "airport" => 2,
        "station" => 1,
        _ => 0,
    };
    (tier << 32) | languages.min(u32::MAX as usize) as u64
}

pub fn build(dir: &std::path::Path, places: usize) -> Result<()> {
    let landmarks = Landmarks::read(dir, places, true)?;
    std::fs::write(dir.join("landmarks.fst"), landmarks.names.as_fst().to_vec())?;
    Ok(())
}

impl Landmarks {
    pub fn open(dir: &std::path::Path, places: usize) -> Result<Landmarks> {
        Self::read(dir, places, false)
    }

    fn read(dir: &std::path::Path, places: usize, compile: bool) -> Result<Landmarks> {
        let directory = MmapDirectory::open(dir)?;
        let text = match directory.open_read(std::path::Path::new("landmarks.tsv")) {
            Ok(file) => file.read_bytes()?,
            Err(OpenReadError::FileDoesNotExist(_)) => OwnedBytes::empty(),
            Err(e) => return Err(e).context(dir.display().to_string()),
        };
        let rows = std::str::from_utf8(&text)?;
        let mut list = vec![];
        let mut keys: Vec<(Vec<u8>, u64)> = vec![];
        for line in rows.lines() {
            let c: Vec<&str> = line.split('\t').collect();
            let (Some(category), Some(place), Some(x), Some(y), Some(primary), Some(pairs)) = (
                CATEGORIES.iter().find(|k| Some(**k) == c.first().copied()),
                c.get(1).and_then(|p| p.parse::<u32>().ok()),
                c.get(2).and_then(|v| v.parse::<f32>().ok()),
                c.get(3).and_then(|v| v.parse::<f32>().ok()),
                c.get(5),
                c.get(6),
            ) else {
                continue;
            };
            if place as usize >= places
                || !(-180.0..=180.0).contains(&x)
                || !(-90.0..=90.0).contains(&y)
                || primary.trim().is_empty()
            {
                continue;
            }
            let pairs: Vec<(&str, &str)> =
                pairs.split(SEP).filter_map(|p| p.split_once('=')).collect();
            let languages = pairs
                .iter()
                .filter(|(k, _)| !OTHER_KEYS.contains(k))
                .count();
            let id = list.len() as u32;
            let en = pairs.iter().find(|(k, _)| *k == "en").map(|p| p.1);
            let s = score(category, languages);
            let mut key = |name: &str, flag: u8| {
                if !name.is_empty() {
                    let mut k = name.as_bytes().to_vec();
                    k.extend_from_slice(&[0, flag]);
                    k.extend_from_slice(&id.to_be_bytes());
                    keys.push((k, s));
                }
            };
            if compile {
                for (k, n) in std::iter::once(("", *primary)).chain(pairs.iter().copied()) {
                    let n = fold(n);
                    key(&n, 0);
                    // "gare du nord" finds "Paris Gare du Nord": its own and English names, from each
                    // later word that is not a short article
                    if k.is_empty() || k == "en" || (k != "iata" && OTHER_KEYS.contains(&k)) {
                        let mut at = 0;
                        for w in n.split(' ') {
                            if at > 0 && (w.len() >= 3 || w.bytes().any(|b| b.is_ascii_digit())) {
                                key(&n[at..], 1);
                            }
                            at += w.len() + 1;
                        }
                    }
                }
            }
            let name = en.unwrap_or(primary);
            let start = name.as_ptr().addr() - text.as_ptr().addr();
            list.push(Landmark {
                category,
                place,
                x,
                y,
                name: start..start + name.len(),
                score: s,
            });
        }
        let names = if compile || text.is_empty() {
            keys.sort_unstable();
            keys.dedup_by(|a, b| a.0 == b.0);
            let mut build = tantivy_fst::MapBuilder::memory();
            for (k, v) in keys {
                build.insert(k, v)?;
            }
            OwnedBytes::new(build.into_inner()?)
        } else {
            directory
                .open_read(std::path::Path::new("landmarks.fst"))?
                .read_bytes()?
        };
        Ok(Landmarks {
            list,
            names: tantivy_fst::Map::from(tantivy_fst::raw::Fst::new(names)?),
            text,
            found: Mutex::default(),
        })
    }

    pub fn name(&self, landmark: &Landmark) -> &str {
        std::str::from_utf8(&self.text[landmark.name.clone()]).unwrap()
    }

    /// Landmarks with a name that starts with `text`, best first; when `open` its last word may
    /// be unfinished, else it is whole ("paris museum " is not "Paris Museums").
    pub fn find(&self, text: &str, open: bool) -> Found {
        if text.is_empty() {
            return Arc::new([]);
        }
        let key = (Box::<str>::from(text), open);
        if let Some(hit) = self.found.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let after = |t: &[u8]| {
            let mut to = t.to_vec();
            *to.last_mut().unwrap() += 1;
            to
        };
        let ranges = if open {
            vec![(text.as_bytes().to_vec(), after(text.as_bytes()))]
        } else {
            let word = format!("{text} ");
            vec![
                (
                    [text.as_bytes(), &[0]].concat(),
                    [text.as_bytes(), &[1]].concat(),
                ),
                (word.as_bytes().to_vec(), after(word.as_bytes())),
            ]
        };
        // score, then how the name fits: the text itself, a name that starts with it, one with it
        // at a later word
        let mut best: HashMap<u32, (u64, u8)> = HashMap::new();
        for (from, to) in ranges {
            let mut range = self.names.range().ge(&from).lt(&to).into_stream();
            while let Some((k, s)) = range.next() {
                let at = k.len() - 6;
                let id = u32::from_be_bytes(k[at + 2..].try_into().unwrap());
                let fit = match k[at + 1] {
                    0 if at == text.len() => 0,
                    0 => 1,
                    _ => 2,
                };
                let e = best.entry(id).or_insert((s, fit));
                e.1 = e.1.min(fit);
            }
        }
        let mut all: Vec<(u32, (u64, u8))> = best.into_iter().collect();
        all.sort_unstable_by(|a, b| {
            b.1.0
                .cmp(&a.1.0)
                .then(a.1.1.cmp(&b.1.1))
                .then(a.0.cmp(&b.0))
        });
        all.truncate(CANDIDATES);
        let all: Found = all.into_iter().map(|(id, _)| id).collect();
        let mut cache = self.found.lock().unwrap();
        // ponytail: dropped whole when full, not LRU, like the place caches
        if cache.len() >= MAX_CACHED {
            cache.clear();
        }
        cache.insert(key, all.clone());
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_iata_codes_and_later_words_find_a_landmark_by_tier_then_fame() {
        let path = std::env::temp_dir().join(format!("landmarks-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let row = |cat: &str, place: u32, primary: &str, pairs: &[&str]| {
            format!(
                "{cat}\t{place}\t2.35\t48.88\tQ1\t{primary}\t{}\n",
                pairs.join(&SEP.to_string())
            )
        };
        let rows = [
            row(
                "station",
                0,
                "Paris Gare du Nord",
                &["en=Paris North", "de=Paris Nord"],
            ),
            row(
                "airport",
                0,
                "Aéroport de Paris-Charles-de-Gaulle",
                &["en=Paris Charles de Gaulle Airport", "iata=CDG"],
            ),
            row(
                "museum",
                0,
                "Musée du Louvre",
                &["en=Louvre", "de=Louvre", "it=Louvre"],
            ),
            row("museum", 0, "Gare Museum", &[]),
            // a place the index does not have, and a category it does not know
            row("museum", 9, "Elsewhere", &[]),
            row("shop", 0, "Bakery", &[]),
        ];
        std::fs::write(path.join("landmarks.tsv"), rows.concat()).unwrap();
        build(&path, 1).unwrap();
        let l = Landmarks::open(&path, 1).unwrap();
        std::fs::remove_file(path.join("landmarks.tsv")).unwrap();
        std::fs::remove_file(path.join("landmarks.fst")).unwrap();
        assert_eq!(l.list.len(), 4);
        assert_eq!(l.name(&l.list[0]), "Paris North");
        let ids = |found: Found| found.to_vec();
        assert_eq!(*l.find("cdg", false), [1]);
        assert_eq!(*l.find("cd", true), [1]);
        // a later word of the primary name, but not of another language's
        assert_eq!(*l.find("gare du nord", false), [0]);
        assert!(ids(l.find("nord", false)).contains(&0));
        assert!(l.find("du nord", false).is_empty());
        // the station before the museum, though the museum's name starts with the text
        assert_eq!(ids(l.find("gare", true)), [0, 3]);
        // the airport's tier first
        assert_eq!(ids(l.find("paris", true)), [1, 0]);
        assert_eq!(ids(l.find("louv", true)), [2]);
        assert_eq!(*l.find("louvre", true), [2]);
        assert!(l.find("louv", false).is_empty());
        // whole words of a longer name
        assert_eq!(ids(l.find("paris gare", false)), [0]);
        assert!(l.find("paris gar", false).is_empty());
        assert!(Landmarks::open(&path, 1).unwrap().list.is_empty());
        std::fs::remove_dir(path).unwrap();
    }
}
