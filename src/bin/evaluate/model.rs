use crate::score::{Expect, LEVELS, SUGGEST_LIMIT, Summary, rank};
use anyhow::{Context, Result, bail, ensure};
use opengeocoding::eval::{ADDRESS_COLUMNS, holdout, read_tsv};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn gold() -> PathBuf {
    root().join("tests/gold.jsonl")
}

/// Landmarks typed into SuggestAddress with `includeLandmarks`, generated from
/// `tests/gold.landmarks.seeds.jsonl`.
pub fn gold_landmarks() -> PathBuf {
    root().join("tests/gold.landmarks.jsonl")
}

/// Registry-first corpora (`tests/registry/*.jsonl`, generated from the `*.seeds.jsonl` (and `*.labels.jsonl`) beside them).
pub fn registry_corpora() -> Vec<PathBuf> {
    let mut found: Vec<_> = std::fs::read_dir(root().join("tests/registry"))
        .into_iter()
        .flatten()
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| {
            p.to_str().is_some_and(|p| {
                p.ends_with(".jsonl")
                    && !p.ends_with(".seeds.jsonl")
                    && !p.ends_with(".labels.jsonl")
            })
        })
        .collect();
    found.sort();
    found
}

/// The gold suite: the curated corpus, every registry-first corpus, then the landmarks.
pub fn suite() -> Vec<PathBuf> {
    let mut files = vec![gold()];
    files.extend(registry_corpora());
    files.push(gold_landmarks());
    files
}

/// The suite as one file: each corpus ended by exactly one newline, so hashing the files in order
/// gives the hash of this file (see `run::sha256_files`).
pub fn write_suite(out: &Path) -> Result<()> {
    let mut text = String::new();
    for path in suite() {
        let file =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        text += file.trim_end_matches('\n');
        text += "\n";
    }
    std::fs::write(out, text)?;
    Ok(())
}

pub fn load_suite() -> Result<Vec<Case>> {
    let mut cases = vec![];
    for path in suite() {
        cases.extend(load_corpus(&path)?);
    }
    Ok(cases)
}

pub fn addresses() -> PathBuf {
    root().join("tests/addresses.tsv")
}

pub fn address_snapshot() -> PathBuf {
    root().join("tests/snapshots/addresses.tsv")
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Case {
    pub id: String,
    pub query: String,
    pub cohort: String,
    pub location: String,
    #[serde(default)]
    pub expect: Option<Expect>,
    #[serde(default)]
    pub must_pass: bool,
    #[serde(default)]
    pub holdout: bool,
    #[serde(default)]
    pub known_failure: Option<String>,
    /// An earlier geocode or source coordinate for drift comparison (private corpora only).
    #[serde(default)]
    pub stored: Option<Value>,
    #[serde(default)]
    pub source: Option<Value>,
    #[serde(default)]
    pub review: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggest: Option<SuggestExpect>,
    /// Sets `includeLandmarks` on this case's suggestion requests.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub include_landmarks: bool,
}

/// A place's name typed alone into SuggestAddress: it should come at `rank` or better, behind
/// only the `larger` places of the same name (Wikidata items with more population).
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SuggestExpect {
    pub text: String,
    pub rank: usize,
    #[serde(default)]
    pub larger: Vec<String>,
}

/// One SuggestAddress request: "full" types the query, "name" the place's name,
/// "off" repeats a landmark query with landmarks explicitly disabled.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct SuggestCheck {
    pub text: String,
    /// The worst passing rank.
    pub want: usize,
    /// 1-based rank of the first suggestion at the expected place.
    pub rank: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub found: Vec<Suggested>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub landmarks_off: bool,
    /// The fastest of `repeat` requests, as for a SearchAddress case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<f64>,
}

/// What ReverseGeocode or ResolveRegion answered at the point a case expects, scored by
/// `reverse_class` or `region_class`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Located {
    pub class: String,
    /// Lowercase ISO country code of the first answer; empty when there is none.
    pub country: String,
    /// ISO 3166-2 codes of the first answer, uppercase.
    pub regions: Vec<String>,
    /// The first ReverseGeocode answer's locality and region names; absent from ResolveRegion
    /// answers and from runs that predate them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub places: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Suggested {
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
    pub name: String,
    pub country: String,
    pub point: [f64; 2],
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Call {
    pub endpoint: String,
    pub q: String,
    pub language: String,
    pub status: u16,
    pub ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct DirectRaw {
    pub count: usize,
    pub top1: String,
    pub top1_name: String,
    pub best_rank: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Record {
    pub id: String,
    pub query: String,
    pub class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_calls: Option<usize>,
    /// ReverseGeocode and ResolveRegion at the case's expected point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse: Option<Located>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Located>,
    /// ReverseGeocode at the case's expected point, the fastest of `repeat` requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverse_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_errors: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub backend_trace: Vec<Call>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance_km: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_raw: Option<DirectRaw>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_structured: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_reverse: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rpc_resolve_region: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direct_details: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution_hint: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub suggest: BTreeMap<String, SuggestCheck>,
}

/// Splits on newlines only: real addresses contain U+2028 and U+001C–U+001E, which
/// line-splitting helpers in other languages treat as line breaks.
pub fn load_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Vec<T>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    text.split('\n')
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(n, line)| {
            serde_json::from_str(line).with_context(|| format!("{}:{}", path.display(), n + 1))
        })
        .collect()
}

/// JSONL corpora, or TSV corpora with at least `id` and `query` columns (unscored, one case per row).
pub fn load_corpus(path: &Path) -> Result<Vec<Case>> {
    if path.extension().is_none_or(|ext| ext != "tsv") {
        return load_jsonl(path);
    }
    let (_, rows) = read_tsv(path)?;
    rows.into_iter()
        .map(|row| {
            let field = |key: &str| row.get(key).cloned().unwrap_or_default();
            Ok(Case {
                id: row
                    .get("id")
                    .cloned()
                    .context("TSV corpus needs an id column")?,
                query: row
                    .get("query")
                    .cloned()
                    .context("TSV corpus needs a query column")?,
                cohort: row
                    .get("target")
                    .filter(|t| !t.is_empty())
                    .map_or("tsv".into(), |t| format!("target_{t}")),
                location: Some(field("location"))
                    .filter(|l| !l.is_empty())
                    .unwrap_or_else(|| field("id")),
                holdout: field("holdout") == "1",
                ..Default::default()
            })
        })
        .collect()
}

const LICENSES: [&str; 6] = [
    "CC0-1.0",
    "etalab-2.0",
    "US-Government-Public-Domain",
    "OGL-UK-3.0",
    "CC-BY-4.0",
    "swisstopo-OGD",
];

/// Gold corpus integrity: provenance, licence, hold-out rule and well-formed expectations.
pub fn validate_gold(cases: &[Case]) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    let mut queries = std::collections::HashSet::new();
    for case in cases {
        let at = &case.id;
        ensure!(ids.insert(case.id.clone()), "duplicate id {at}");
        ensure!(queries.insert(case.query.clone()), "{at}: duplicate query");
        let license = case
            .source
            .as_ref()
            .and_then(|s| s["license"].as_str())
            .unwrap_or("");
        let registry = case
            .source
            .as_ref()
            .and_then(|s| s["name"].as_str())
            .and_then(crate::registry::get);
        ensure!(
            registry.map_or_else(|| LICENSES.contains(&license), |r| r.license == license),
            "{at}: licence {license:?}"
        );
        ensure!(case.review.is_some(), "{at}: review status");
        ensure!(
            case.holdout == holdout(&case.location),
            "{at}: hold-out flag"
        );
        let Some(expect) = &case.expect else {
            ensure!(
                !case.must_pass && case.known_failure.is_none(),
                "{at}: unscored case cannot gate"
            );
            continue;
        };
        if let Some(s) = &case.suggest {
            ensure!(
                !s.text.trim().is_empty()
                    && (1..=SUGGEST_LIMIT).contains(&s.rank)
                    && s.rank == s.larger.len() + 1,
                "{at}: suggest"
            );
        }
        if expect.error.is_some() {
            continue;
        }
        for alt in expect.alternatives() {
            ensure!(
                LEVELS.contains(&alt.level.as_str())
                    || alt.level == "landmark" && case.include_landmarks,
                "{at}: level {:?}",
                alt.level
            );
            if alt.level == "none" {
                continue;
            }
            ensure!(
                !alt.country.is_empty()
                    && alt
                        .country
                        .iter()
                        .all(|c| c.len() == 2 && c.chars().all(|ch| ch.is_ascii_lowercase())),
                "{at}: country"
            );
            ensure!(
                rank(&alt.level) <= rank(alt.max_level.as_deref().unwrap_or("house")),
                "{at}: level range"
            );
            if alt.level != "country" {
                ensure!(
                    alt.point.is_some() && alt.radius_km.unwrap_or(0.) > 0.,
                    "{at}: area"
                );
            }
            ensure!(
                alt.region
                    .iter()
                    .all(|code| alt.country.contains(&code[..2].to_lowercase())),
                "{at}: region country"
            );
        }
    }
    Ok(())
}

/// The public address corpus may carry only `ADDRESS_COLUMNS`; its snapshot must cover exactly its ids.
pub fn validate_addresses(path: &Path, snapshot: &Path) -> Result<usize> {
    let (columns, rows) = read_tsv(path)?;
    if columns != ADDRESS_COLUMNS {
        bail!(
            "{} columns must be exactly {ADDRESS_COLUMNS:?}",
            path.display()
        );
    }
    let ids: std::collections::HashSet<_> = rows.iter().map(|r| r["id"].clone()).collect();
    ensure!(ids.len() == rows.len(), "duplicate address ids");
    ensure!(
        rows.iter()
            .all(|r| r["holdout"] == "0" || r["holdout"] == "1"),
        "holdout must be 0 or 1"
    );
    if snapshot.exists() {
        let (columns, snapshot_rows) = read_tsv(snapshot)?;
        ensure!(
            columns == crate::report::SNAPSHOT_COLUMNS,
            "snapshot columns changed"
        );
        let snapshot_ids: std::collections::HashSet<_> =
            snapshot_rows.iter().map(|r| r["id"].clone()).collect();
        ensure!(
            snapshot_ids == ids,
            "{} does not match {}",
            snapshot.display(),
            path.display()
        );
    }
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gold_corpus_is_well_formed() {
        let cases = load_corpus(&gold()).unwrap();
        assert!(cases.len() >= 100);
        validate_gold(&cases).unwrap();
    }

    #[test]
    fn the_gold_suite_has_unique_ids_and_queries_across_its_files() {
        let cases = load_suite().unwrap();
        assert!(cases.len() > load_corpus(&gold()).unwrap().len());
        validate_gold(&cases).unwrap();
    }

    #[test]
    fn registry_corpora_are_well_formed() {
        for path in registry_corpora() {
            let cases = load_corpus(&path).unwrap();
            assert!(!cases.is_empty(), "{}", path.display());
            validate_gold(&cases).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        }
    }

    #[test]
    fn address_corpus_has_only_public_columns_and_a_matching_snapshot() {
        assert!(validate_addresses(&addresses(), &address_snapshot()).unwrap() >= 500);
    }
}
