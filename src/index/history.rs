use super::{Place, name_key};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{collections::HashMap, time::SystemTime};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    name: String,
    wikidata: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    url: String,
    published_on: Option<String>,
    retrieved_on: String,
    supports: Vec<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    id: String,
    country: String,
    observed_on: String,
    reviewed_on: String,
    path: Vec<Identity>,
    sources: Vec<Source>,
    review: String,
}

struct Binding {
    name: u64,
    places: Vec<u32>,
}

#[derive(Default)]
pub(super) struct History(HashMap<String, HashMap<u64, Vec<Binding>>>);

fn date(text: &str) -> Result<u64> {
    ensure!(
        text.len() == 10
            && text.as_bytes()[4] == b'-'
            && text.as_bytes()[7] == b'-'
            && text
                .bytes()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit()),
        "expected YYYY-MM-DD"
    );
    let year: u64 = text[..4].parse()?;
    let month: usize = text[5..7].parse()?;
    let day: u64 = text[8..].parse()?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let months = [
        31,
        28 + u64::from(leap),
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    ensure!(year > 0 && (1..=12).contains(&month), "invalid date");
    ensure!((1..=months[month - 1]).contains(&day), "invalid date");
    let prior = year - 1;
    Ok(prior * 365 + prior / 4 - prior / 100
        + prior / 400
        + months[..month - 1].iter().sum::<u64>()
        + day)
}

fn observations(text: &str) -> Result<Vec<Observation>> {
    let records: Vec<Observation> =
        serde_json::from_str(text).context("reviewed historical memberships")?;
    let today = 719_163
        + SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs()
            / 86_400;
    let mut ids = std::collections::HashSet::new();
    for record in &records {
        ensure!(
            !record.id.is_empty() && ids.insert(&record.id),
            "duplicate or empty observation ID"
        );
        ensure!(
            record.country.len() == 2 && record.country.bytes().all(|b| b.is_ascii_uppercase()),
            "invalid observation country"
        );
        ensure!(
            (2..=16).contains(&record.path.len()),
            "invalid membership path"
        );
        ensure!(
            !record.review.trim().is_empty(),
            "missing membership review"
        );
        let observed = date(&record.observed_on)?;
        let reviewed = date(&record.reviewed_on)?;
        ensure!(
            observed <= reviewed && reviewed <= today,
            "invalid observation/review dates"
        );
        let mut items = std::collections::HashSet::new();
        for (i, node) in record.path.iter().enumerate() {
            ensure!(
                !node.name.trim().is_empty()
                    && node.name.chars().count() <= 256
                    && !node.name.chars().any(char::is_control),
                "invalid identity name"
            );
            if let Some(item) = &node.wikidata {
                ensure!(
                    item.strip_prefix('Q').is_some_and(|n| !n.is_empty()
                        && !n.starts_with('0')
                        && n.bytes().all(|b| b.is_ascii_digit()))
                        && items.insert(item),
                    "invalid or repeated source identity"
                );
            } else {
                ensure!(
                    i == 0 && record.path.len() >= 4,
                    "an unlinked settlement needs two identified ancestors"
                );
            }
        }
        let mut supported = vec![false; record.path.len() - 1];
        for source in &record.sources {
            let url = reqwest::Url::parse(&source.url)?;
            ensure!(
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none(),
                "invalid evidence URL"
            );
            let retrieved = date(&source.retrieved_on)?;
            ensure!(
                retrieved <= reviewed && !source.supports.is_empty(),
                "invalid evidence date or scope"
            );
            if let Some(published) = &source.published_on {
                ensure!(
                    date(published)? <= retrieved,
                    "evidence retrieved before publication"
                );
            }
            for &edge in &source.supports {
                *supported
                    .get_mut(edge)
                    .context("evidence edge outside membership path")? = true;
            }
        }
        ensure!(
            supported.iter().all(|&s| s),
            "membership path lacks evidence"
        );
    }
    Ok(records)
}

impl History {
    pub(super) fn load(places: &[Place], lines: &[&str]) -> Result<Self> {
        Self::bind(
            places,
            lines,
            include_str!("../../data/historical_memberships.json"),
        )
    }

    fn bind(places: &[Place], lines: &[&str], text: &str) -> Result<Self> {
        ensure!(
            places.len() == lines.len(),
            "historical identity table length"
        );
        let records = observations(text)?;
        if records.is_empty() {
            return Ok(Self::default());
        }
        let mut names: HashMap<u64, Vec<usize>> = HashMap::new();
        for (i, record) in records.iter().enumerate() {
            names
                .entry(name_key(&record.path[0].name))
                .or_default()
                .push(i);
        }
        let mut candidates = vec![vec![]; records.len()];
        for (i, place) in places.iter().enumerate() {
            if !matches!(place.subtype.as_ref(), "locality" | "localadmin") {
                continue;
            }
            for key in &place.keys {
                for &r in names.get(key).into_iter().flatten() {
                    let record = &records[r];
                    let mut at = i as u32;
                    let mut seen = std::collections::HashSet::new();
                    let mut matched =
                        record.path[..record.path.len() - 1]
                            .iter()
                            .enumerate()
                            .all(|(n, node)| {
                                if !seen.insert(at) {
                                    return false;
                                }
                                let Some(place) = places.get(at as usize) else {
                                    return false;
                                };
                                let item = lines[at as usize].rsplit('\t').next().unwrap_or("");
                                let identity = match node.wikidata.as_deref() {
                                    Some(wanted) if !item.is_empty() => item == wanted,
                                    // A missing index ID needs two reviewed, adjacent parent identities.
                                    _ => n == 0 && record.path.len() >= 4,
                                };
                                let matched = identity
                                    && place.cc.eq_ignore_ascii_case(&record.country)
                                    && place.named(name_key(&node.name));
                                at = place.parent;
                                matched
                            });
                    while matched && at != super::NONE {
                        let Some(parent) = places.get(at as usize) else {
                            matched = false;
                            break;
                        };
                        if !parent.cc.eq_ignore_ascii_case(&record.country)
                            || !seen.insert(at)
                            || seen.len() > 16
                        {
                            matched = false;
                            break;
                        }
                        at = parent.parent;
                    }
                    if matched {
                        candidates[r].push(i as u32);
                    }
                }
            }
        }
        let mut history = Self::default();
        for (record, candidates) in records.iter().zip(candidates) {
            history
                .0
                .entry(record.country.to_lowercase())
                .or_default()
                .entry(name_key(&record.path.last().unwrap().name))
                .or_default()
                .push(Binding {
                    name: name_key(&record.path[0].name),
                    places: candidates,
                });
        }
        Ok(history)
    }

    pub(super) fn find(
        &self,
        places: &[Place],
        town: &str,
        ancestor: &str,
        country: &str,
    ) -> Option<u32> {
        if town.chars().any(char::is_numeric) || ancestor.chars().any(char::is_numeric) {
            return None;
        }
        let (town, ancestor) = (name_key(town), name_key(ancestor));
        let mut hits = vec![];
        for binding in self.0.get(country)?.get(&ancestor)? {
            if binding.name == town
                || binding
                    .places
                    .iter()
                    .any(|&p| places[p as usize].named(town))
            {
                let [place] = binding.places.as_slice() else {
                    return None;
                };
                hits.push(*place);
            }
        }
        hits.sort_unstable();
        hits.dedup();
        match hits.as_slice() {
            [place] => Some(*place),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::NONE;

    fn evidence() -> serde_json::Value {
        serde_json::json!([{
            "id": "authored-2021", "country": "FR", "observed_on": "2021-07-25", "reviewed_on": "2026-10-19",
            "path": [
                {"name":"Northhaven", "wikidata":"Q990000001"},
                {"name":"Green District", "wikidata":"Q990000002"},
                {"name":"Coast Province", "wikidata":"Q990000003"},
                {"name":"Former Coast", "wikidata":"Q990000004"}
            ],
            "sources": [{"url":"https://example.org/public-register/2021", "published_on":"2021-07-25", "retrieved_on":"2026-10-19", "supports":[0,1,2]}],
            "review":"Authored membership observation, not derived from a province's current children."
        }])
    }

    fn world() -> (Vec<Place>, Vec<String>) {
        let rows = [
            ("locality", "fr", "Northhaven", "", 1, "alt_name=Nordhaven"),
            ("localadmin", "fr", "Green District", "Q990000002", 2, ""),
            ("county", "fr", "Coast Province", "Q990000003", 3, ""),
            ("region", "fr", "New Coast", "Q990000005", NONE, ""),
            ("locality", "fr", "Northhaven", "Q990000006", 3, ""),
            ("locality", "fr", "Newharbour", "Q990000007", 1, ""),
            ("locality", "us", "Northhaven", "Q990000001", 1, ""),
        ];
        let lines = rows.iter().map(|r| format!("{}\t{}", r.2, r.3)).collect();
        let places = rows
            .into_iter()
            .map(|(kind, cc, name, _, parent, names)| {
                let mut p = Place {
                    subtype: kind.into(),
                    class: "town".into(),
                    cc: cc.into(),
                    region: "".into(),
                    population: 100,
                    x: 1.,
                    y: 2.,
                    bbox: [1., 2., 1., 2.],
                    parent,
                    primary: name.into(),
                    names: names.into(),
                    en: NONE,
                    keys: Box::default(),
                    city_like: false,
                    settlement_twin: false,
                };
                let mut keys: Vec<_> = p.all_names().map(name_key).collect();
                keys.sort_unstable();
                keys.dedup();
                p.keys = keys.into();
                p
            })
            .collect();
        (places, lines)
    }

    #[test]
    fn only_the_attested_identity_recovers_not_current_descendants_or_namesakes() {
        let (mut places, mut lines) = world();
        let bind = |places: &[Place], lines: &[String]| {
            History::bind(
                places,
                &lines.iter().map(String::as_str).collect::<Vec<_>>(),
                &evidence().to_string(),
            )
            .unwrap()
        };
        let history = bind(&places, &lines);
        assert_eq!(
            history.find(&places, "Nordhaven", "Former Coast", "fr"),
            Some(0)
        );
        for (town, ancestor, cc) in [
            ("Newharbour", "Former Coast", "fr"),
            ("Northhaven", "Other Coast", "fr"),
            ("Northhaven", "Former Coast", "us"),
            ("10 Northhaven", "Former Coast", "fr"),
            ("North", "Former Coast", "fr"),
        ] {
            assert_eq!(history.find(&places, town, ancestor, cc), None);
        }
        places[0].parent = 2;
        assert_eq!(
            bind(&places, &lines).find(&places, "Northhaven", "Former Coast", "fr"),
            None
        );
        places[0].parent = 1;
        lines[0] = "Northhaven\tQ990000099".into();
        assert_eq!(
            bind(&places, &lines).find(&places, "Northhaven", "Former Coast", "fr"),
            None
        );
        lines[0] = "Northhaven\t".into();
        places[1].parent = 0;
        assert_eq!(
            bind(&places, &lines).find(&places, "Northhaven", "Former Coast", "fr"),
            None
        );
    }

    #[test]
    fn duplicate_identity_bindings_and_conflicting_observations_abstain() {
        let (mut places, mut lines) = world();
        places[4].parent = 1;
        lines[4] = "Northhaven\t".into();
        let refs: Vec<_> = lines.iter().map(String::as_str).collect();
        let ambiguous = History::bind(&places, &refs, &evidence().to_string()).unwrap();
        for town in ["Northhaven", "Nordhaven"] {
            assert_eq!(ambiguous.find(&places, town, "Former Coast", "fr"), None);
        }
        let h = History(HashMap::from([(
            "fr".into(),
            HashMap::from([(
                name_key("Former Coast"),
                vec![
                    Binding {
                        name: name_key("Northhaven"),
                        places: vec![0],
                    },
                    Binding {
                        name: name_key("Northhaven"),
                        places: vec![4],
                    },
                ],
            )]),
        )]));
        assert_eq!(h.find(&places, "Northhaven", "Former Coast", "fr"), None);
    }

    #[test]
    fn an_identified_settlement_can_name_a_removed_parent_in_another_country() {
        let (mut places, lines) = world();
        places[6].parent = NONE;
        let mut data = evidence();
        data[0]["country"] = serde_json::json!("US");
        data[0]["path"] = serde_json::json!([
            {"name":"Northhaven", "wikidata":"Q990000001"},
            {"name":"Former Coast", "wikidata":"Q990000004"}
        ]);
        data[0]["sources"][0]["supports"] = serde_json::json!([0]);
        let refs: Vec<_> = lines.iter().map(String::as_str).collect();
        let history = History::bind(&places, &refs, &data.to_string()).unwrap();
        assert_eq!(
            history.find(&places, "Northhaven", "Former Coast", "us"),
            Some(6)
        );
        assert_eq!(
            history.find(&places, "Northhaven", "Former Coast", "fr"),
            None
        );
        let empty = History::bind(&places, &refs, "[]").unwrap();
        assert_eq!(
            empty.find(&places, "Northhaven", "Former Coast", "us"),
            None
        );
        places[6].parent = 1;
        let conflict = History::bind(&places, &refs, &data.to_string()).unwrap();
        assert_eq!(
            conflict.find(&places, "Northhaven", "Former Coast", "us"),
            None
        );
    }

    #[test]
    fn reviewed_data_rejects_missing_proof_invalid_dates_and_weak_identity_scopes() {
        assert!(observations(include_str!("../../data/historical_memberships.json")).is_ok());
        assert_eq!(date("1970-01-01").unwrap(), 719_163);
        assert!(date("2024-02-29").is_ok());
        for bad in [
            "2023-02-29",
            "2021-00-00",
            "2021-02-30",
            "0000-01-01",
            "2021-1-01",
            "２０２１-01-01",
        ] {
            assert!(date(bad).is_err(), "{bad}");
        }
        for (pointer, value) in [
            ("/0/sources/0/supports", serde_json::json!([0, 1])),
            ("/0/observed_on", serde_json::json!("2027-07-25")),
            ("/0/path/1/wikidata", serde_json::Value::Null),
            ("/0/path/1/wikidata", serde_json::json!("Q990000001")),
            (
                "/0/sources/0/url",
                serde_json::json!("https://user:secret@example.org"),
            ),
            ("/0/country", serde_json::json!("France")),
        ] {
            let mut data = evidence();
            *data.pointer_mut(pointer).unwrap() = value;
            assert!(observations(&data.to_string()).is_err(), "{pointer}");
        }
    }
}
