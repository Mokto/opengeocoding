use crate::score::{Expect, LEVELS, Summary, rank};
use anyhow::{Context, Result, bail, ensure};
use geolocation::eval::{ADDRESS_COLUMNS, holdout, read_tsv};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn gold() -> PathBuf {
    root().join("tests/gold.jsonl")
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
    pub ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_calls: Option<usize>,
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

const LICENSES: [&str; 4] = [
    "CC0-1.0",
    "etalab-2.0",
    "US-Government-Public-Domain",
    "OGL-UK-3.0",
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
        ensure!(LICENSES.contains(&license), "{at}: licence {license:?}");
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
        if expect.error.is_some() {
            continue;
        }
        for alt in expect.alternatives() {
            ensure!(
                LEVELS.contains(&alt.level.as_str()),
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
    fn address_corpus_has_only_public_columns_and_a_matching_snapshot() {
        assert!(validate_addresses(&addresses(), &address_snapshot()).unwrap() >= 500);
    }
}
