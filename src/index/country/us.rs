use crate::index::suggest::*;
use crate::index::*;
use regex::Regex;
use std::sync::LazyLock;

/// `text` with each ZIP+4 ("37208-4501") as its ZIP, which is how the index holds it.
pub(in crate::index) fn zip5(text: &str) -> std::borrow::Cow<'_, str> {
    static ZIP4: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(\d{5})-\d{4}\b").unwrap());
    ZIP4.replace_all(text, "$1")
}

/// A municipality's name without the kind its register writes after it ("Irvington Township",
/// "Leonia Borough"), which the index's town names leave out.
pub(in crate::index) fn municipality(town: &str) -> Option<&str> {
    let (name, kind) = town.rsplit_once(' ')?;
    ["township", "borough", "town", "village"]
        .iter()
        .any(|k| kind.eq_ignore_ascii_case(k))
        .then_some(name.trim_end())
        .filter(|n| !n.is_empty())
}

pub(crate) fn full_road(road: &str) -> Option<String> {
    let words = fold(road);
    words.split(' ').any(|word| word == "mtn").then(|| {
        words
            .split(' ')
            .map(|word| if word == "mtn" { "mountain" } else { word })
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// Words a trailing secondary unit takes at the end of `words` ("Apt 106", "Ste B", "#5"): 0 when
/// it has none.
fn unit_words(words: &[&str]) -> usize {
    let designator = |w: &str| {
        matches!(
            w.trim_end_matches('.').to_ascii_lowercase().as_str(),
            "apt"
                | "apartment"
                | "unit"
                | "ste"
                | "suite"
                | "lot"
                | "rm"
                | "room"
                | "bldg"
                | "trlr"
                | "spc"
                | "#"
        )
    };
    let unit = |w: &str| {
        (1..=6).contains(&w.len()) && w.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    };
    match words {
        [.., d, u] if designator(d) && unit(u) => 2,
        [.., u] if u.strip_prefix('#').is_some_and(unit) => 1,
        _ => 0,
    }
}

/// A road part's house and road before its unit ("4862 Rural Road Southwest Apt 106"), and the
/// unit. None without a unit after a house number and road.
pub(in crate::index) fn unit_suffix(part: &str) -> Option<(String, String)> {
    let words: Vec<&str> = part.split(' ').collect();
    let at = words.len() - unit_words(&words);
    // "Calle 21 # 102-80" is Colombian and "4ta Calle Casa #101" Nicaraguan: a US address starts
    // with its house number
    let house = words[0].starts_with(|c: char| c.is_ascii_digit())
        && words[0].bytes().filter(u8::is_ascii_alphabetic).count() <= 1;
    (at < words.len() && at >= 2 && house).then(|| (words[..at].join(" "), words[at..].join(" ")))
}

/// A part that is a unit alone ("Apt 4").
fn unit_part(part: &str) -> bool {
    let words: Vec<&str> = part.split(' ').collect();
    unit_words(&words) == words.len()
}

impl Index {
    /// A US address with its unit ("Apt 106") after the road or as a part of its own: the
    /// building's houses, the unit kept as `unverified_suffix`. Units are not indexed.
    pub(in crate::index) fn suggest_us_unit(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        if cc.is_some_and(|c| !c.eq_ignore_ascii_case("us")) {
            return vec![];
        }
        let mut building = Typed {
            parts: typed.parts.clone(),
            open: typed.open,
        };
        let suffix = if let Some((road, unit)) = unit_suffix(&typed.parts[0]) {
            building.parts[0] = road;
            building.open &= typed.parts.len() > 1;
            unit
        } else if typed.parts.len() > 1 && unit_part(&typed.parts[1]) {
            building.open &= typed.parts.len() > 2;
            building.parts.remove(1)
        } else {
            return vec![];
        };
        let mut found = self.suggest_streets(&building, cc, focus, want);
        if !found.iter().any(|s| s.kind == "address") {
            found = self.suggest_us_repair(&building, cc, focus, want);
        }
        found
            .into_iter()
            .filter(|s| s.kind == "address")
            .map(|mut s| {
                if let Some((at, _)) = s.display_name.match_indices(", ").nth(1) {
                    s.display_name.insert_str(at, &format!(", {suffix}"));
                }
                s.unverified_suffix = suffix.clone();
                s
            })
            .collect()
    }

    /// A US address written out of the usual "road, town, ST ZIP" ("United States, WA 98501,
    /// Olympia, 2502 Boulevard Heights Loop Southeast", or without commas), or whose town or ZIP
    /// does not fit the house: the houses of that order, then without the ZIP and without the town.
    pub(in crate::index) fn suggest_us_repair(
        &self,
        typed: &Typed,
        cc: Option<&str>,
        focus: Option<&Focus>,
        want: usize,
    ) -> Vec<Suggestion> {
        if cc.is_some_and(|c| !c.eq_ignore_ascii_case("us")) {
            return vec![];
        }
        let us = |p: &str| self.country(p).as_deref() == Some("us");
        let mut parts = typed.parts.clone();
        if parts.len() > 2
            && us(&parts[0])
            && parts
                .last()
                .is_some_and(|p| p.starts_with(|c: char| c.is_ascii_digit()))
        {
            parts.remove(0);
            parts.reverse();
        }
        if parts.len() > 2 && parts.last().is_some_and(|p| us(p)) {
            parts.pop();
        }
        // the state of a "ST ZIP" part
        let state_of = |p: &str| {
            let (state, zip) = p.rsplit_once(' ')?;
            (zip.len() == 5
                && zip.bytes().all(|b| b.is_ascii_digit())
                && self.us_state(state).is_some())
            .then(|| state.to_owned())
        };
        let mut forms = vec![];
        if let [whole] = parts.as_slice() {
            // "23 Saint Matthew Avenue Ofallon MO 63366": the town is the last words before
            let words: Vec<&str> = whole.split(' ').collect();
            let n = words.len();
            if n > 4 && state_of(&words[n - 2..].join(" ")).is_some() {
                for k in 1..=3.min(n - 4) {
                    forms.push(vec![
                        words[..n - 2 - k].join(" "),
                        words[n - 2 - k..n - 2].join(" "),
                        words[n - 2..].join(" "),
                    ]);
                }
            }
        } else if let [road, between @ .., last] = parts.as_slice()
            && let Some(state) = state_of(last)
        {
            if parts != typed.parts {
                forms.push(parts.clone());
            }
            if !between.is_empty() {
                forms.push(
                    [road.clone()]
                        .into_iter()
                        .chain(between.iter().cloned())
                        .chain([state])
                        .collect(),
                );
                forms.push(vec![road.clone(), last.clone()]);
            }
        }
        for parts in forms {
            let found: Vec<_> = self
                .suggest_streets(&Typed { parts, open: false }, Some("us"), focus, want)
                .into_iter()
                .filter(|s| s.kind == "address")
                .collect();
            if !found.is_empty() {
                return found;
            }
        }
        vec![]
    }

    /// A US state's region place, by its postal code ("OH") or its name.
    pub(crate) fn us_state(&self, state: &str) -> Option<u32> {
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
}
