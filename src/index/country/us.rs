use crate::index::suggest::*;
use crate::index::*;
use regex::Regex;
use std::{collections::HashSet, sync::LazyLock};

pub const MUNICIPALITIES_FILE: &str = "us-municipalities.bin";
pub const MUNICIPALITIES_LABELS: &str = "us-municipalities.json";
pub const CDPS_FILE: &str = "us-cdps.bin";
pub const CDPS_LABELS: &str = "us-cdps.json";

pub(in crate::index) struct Boundaries {
    areas: area::Areas,
    labels: Vec<(String, String, String)>,
}

impl Boundaries {
    pub(in crate::index) fn new(
        bytes: tantivy::directory::OwnedBytes,
        labels: &[u8],
        cdp: bool,
    ) -> anyhow::Result<Self> {
        let areas = area::Areas::new(bytes)?;
        let labels: Vec<(String, String, String)> = serde_json::from_slice(labels)?;
        let mut ids = HashSet::new();
        for (id, region, name) in &labels {
            anyhow::ensure!(
                ids.insert(id)
                    && !id.is_empty()
                    && if cdp {
                        id.strip_prefix("cdp/").is_some_and(|code| {
                            code.len() == 7 && code.bytes().all(|c| c.is_ascii_digit())
                        })
                    } else {
                        !id.starts_with("cdp/")
                    }
                    && region.len() == 5
                    && region.starts_with("US-")
                    && region[3..].bytes().all(|c| c.is_ascii_uppercase())
                    && !name.trim().is_empty()
                    && !name.chars().any(char::is_control),
                "invalid US boundary label"
            );
        }
        anyhow::ensure!(
            areas.place_ids().all(|id| (id as usize) < labels.len()),
            "US boundary polygon has no label"
        );
        Ok(Self { areas, labels })
    }

    fn containing_name(&self, x: f64, y: f64, region: &str) -> Option<&str> {
        let mut containing: Vec<_> = self
            .areas
            .containing(y, x)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        containing.sort_unstable();
        containing.dedup();
        let [id] = containing.as_slice() else {
            return None;
        };
        let (_, state, name) = &self.labels[*id as usize];
        if state != region {
            return None;
        }
        // pack floors each axis; reject a Census edge anywhere in that source-point cell plus polygon rounding.
        let steps = (1u64 << 28) as f64;
        (!self.areas.boundary_intersects([
            x - 0.5e-7,
            y - 0.5e-7,
            x + 360.0 / steps + 0.5e-7,
            y + 180.0 / steps + 0.5e-7,
        ]))
        .then_some(name)
    }
}

pub(in crate::index) fn reverse_municipality(
    index: &Index,
    hit: &Hit,
    address: &mut serde_json::Map<String, serde_json::Value>,
) {
    let Hit::Street {
        x,
        y,
        number: Some(_),
        ..
    } = hit
    else {
        return;
    };
    if address.contains_key("municipality")
        || address
            .get("country_code")
            .and_then(serde_json::Value::as_str)
            != Some("us")
    {
        return;
    }
    let Some(boundaries) = &index.us_municipalities else {
        return;
    };
    let Some(name) = address
        .get("ISO3166-2-lvl4")
        .and_then(serde_json::Value::as_str)
        .and_then(|region| boundaries.containing_name(*x, *y, region))
    else {
        return;
    };
    address.insert("municipality".into(), name.into());
}

pub(in crate::index) fn reverse_locality(
    index: &Index,
    hit: &Hit,
    address: &mut serde_json::Map<String, serde_json::Value>,
    display: &mut Vec<String>,
) {
    let Hit::Street {
        x,
        y,
        number: Some(_),
        ..
    } = hit
    else {
        return;
    };
    if address
        .get("country_code")
        .and_then(serde_json::Value::as_str)
        != Some("us")
    {
        return;
    }
    let Some(boundaries) = &index.us_cdps else {
        return;
    };
    let Some(region) = address
        .get("ISO3166-2-lvl4")
        .and_then(serde_json::Value::as_str)
    else {
        return;
    };
    let Some(name) = boundaries.containing_name(*x, *y, region) else {
        return;
    };
    let current: Vec<_> = ["city", "town", "village"]
        .iter()
        .filter_map(|key| address.get(*key).and_then(serde_json::Value::as_str))
        .collect();
    if current.iter().any(|old| fold(old) == fold(name)) {
        return;
    }
    let governments: Vec<_> = index
        .us_municipalities
        .iter()
        .flat_map(|g| {
            g.areas
                .containing(*y, *x)
                .into_iter()
                .map(|(_, id)| &g.labels[id as usize])
        })
        .filter(|(_, state, _)| state == region)
        .collect();
    if governments
        .iter()
        .any(|(source, _, _)| source.starts_with("place/"))
    {
        return;
    }
    let governing = governments.iter().any(|(_, _, government)| {
        current.iter().any(|old| {
            fold(old) == fold(government)
                || municipality(government).is_some_and(|base| fold(old) == fold(base))
        })
    });
    let same_place = match hit {
        Hit::Street { place, .. } if *place != NONE => {
            let chain = index.chain(*place);
            let chain = index.house_chain(&chain, *x, *y).unwrap_or(chain);
            chain.iter().any(|id| {
                let place = &index.places[*id as usize];
                place.subtype.as_ref() == "locality"
                    && current.contains(&place.name(true))
                    && place.all_names().any(|other| fold(other) == fold(name))
            })
        }
        _ => false,
    };
    // A town that governs the community, or the community under another of its names, stays:
    // the community is named below it.
    if governing || same_place {
        if let Some(key) = ["village", "hamlet"]
            .into_iter()
            .find(|key| !address.contains_key(*key))
        {
            display.insert(2.min(display.len()), name.into());
            address.insert(key.into(), name.into());
        }
        return;
    }
    let old: Vec<_> = ["city", "town", "village"]
        .iter()
        .filter_map(|key| address.remove(*key))
        .collect();
    for value in old {
        if let Some(at) = display
            .iter()
            .enumerate()
            .skip(2)
            .find_map(|(at, name)| (value.as_str() == Some(name)).then_some(at))
        {
            display.remove(at);
        }
    }
    display.insert(2.min(display.len()), name.into());
    // The unclassified-locality fallback is village; CDPs confer no municipal status.
    address.insert("village".into(), name.into());
}

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
