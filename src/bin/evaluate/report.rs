use crate::{
    model::{Case, Record, SuggestCheck, load_jsonl},
    score::{
        CLASSES, DRIFT_KINDS, SUGGEST_CLASSES, Summary, critical, drift, haversine, severity,
        snapshot_point, suggest_class, suggest_severity,
    },
};
use anyhow::{Context, Result};
use indexmap::IndexMap;
use opengeocoding::eval::{Row, read_tsv, write_tsv};
use serde_json::Value;
use std::{collections::BTreeMap, fmt::Write as _, path::Path};

/// One row per case so a PR diff shows exactly which answers changed; the manifest lives beside it.
pub const SNAPSHOT_COLUMNS: [&str; 15] = [
    "id",
    "class",
    "error",
    "country",
    "level",
    "lat",
    "lon",
    "good_match",
    "house_number",
    "road",
    "postcode",
    "locality",
    "region_codes",
    "regions",
    "query",
];

fn manifest_path(snapshot: &Path) -> std::path::PathBuf {
    snapshot.with_extension("manifest.json")
}

/// A run directory, a TSV snapshot or a JSON snapshot: (manifest, records by id).
pub fn load_side(path: &Path) -> Result<(Value, IndexMap<String, Record>)> {
    if path.extension().is_some_and(|e| e == "json") {
        let mut snapshot: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        let records: Vec<Record> = serde_json::from_value(snapshot["results"].take())?;
        return Ok((
            snapshot["manifest"].take(),
            records.into_iter().map(|r| (r.id.clone(), r)).collect(),
        ));
    }
    if path.is_dir() {
        let manifest = serde_json::from_str(&std::fs::read_to_string(path.join("manifest.json"))?)?;
        let records = load_jsonl::<Record>(&path.join("results.jsonl"))?;
        return Ok((
            manifest,
            records.into_iter().map(|r| (r.id.clone(), r)).collect(),
        ));
    }
    let manifest = match std::fs::read_to_string(manifest_path(path)) {
        Ok(text) => serde_json::from_str(&text)?,
        Err(_) => Value::Null,
    };
    let mut records = IndexMap::new();
    for row in read_tsv(path)?.1 {
        let field = |key: &str| row.get(key).cloned().unwrap_or_default();
        let list = |key: &str| {
            let value = field(key);
            if value.is_empty() {
                vec![]
            } else {
                value.split(',').map(String::from).collect()
            }
        };
        let point = match (field("lat").parse(), field("lon").parse()) {
            (Ok(lat), Ok(lon)) => Some([lat, lon]),
            _ => None,
        };
        let record = Record {
            id: field("id"),
            query: field("query"),
            class: field("class"),
            error: Some(field("error")).filter(|e| !e.is_empty()),
            summary: Some(Summary {
                country: field("country"),
                level: field("level"),
                point,
                good_match: field("good_match") == "1",
                region_codes: list("region_codes"),
                regions: Some(list("regions")),
                house_number: Some(field("house_number")),
                road: Some(field("road")),
                postcode: Some(field("postcode")),
                locality: Some(field("locality")),
                ..Default::default()
            }),
            ..Default::default()
        };
        records.insert(record.id.clone(), record);
    }
    Ok((manifest, records))
}

pub fn snapshot(run_dir: &Path, out: &Path) -> Result<()> {
    let (manifest, mut records) = load_side(run_dir)?;
    records.sort_keys();
    if out.extension().is_some_and(|e| e == "json") {
        return json_snapshot(manifest, records, out);
    }
    anyhow::ensure!(
        out.extension().is_some_and(|e| e == "tsv"),
        "snapshot output must be a .tsv or .json file"
    );
    let rows: Vec<Row> = records
        .values()
        .map(|r| {
            let s = r.summary.clone().unwrap_or_default();
            let [lat, lon] = s.point.map(snapshot_point).unwrap_or_default();
            let opt = |v: &Option<String>| v.clone().unwrap_or_default();
            [
                ("id", r.id.clone()),
                ("class", r.class.clone()),
                ("error", r.error.clone().unwrap_or_default()),
                ("country", s.country.clone()),
                (
                    "level",
                    if s.level.is_empty() {
                        "none".into()
                    } else {
                        s.level.clone()
                    },
                ),
                ("lat", lat),
                ("lon", lon),
                ("good_match", if s.good_match { "1" } else { "0" }.into()),
                ("house_number", opt(&s.house_number)),
                ("road", opt(&s.road)),
                ("postcode", opt(&s.postcode)),
                ("locality", opt(&s.locality)),
                ("region_codes", s.region_codes.join(",")),
                ("regions", s.regions.clone().unwrap_or_default().join(",")),
                ("query", r.query.clone()),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
        })
        .collect();
    write_tsv(out, &SNAPSHOT_COLUMNS, &rows)?;
    std::fs::write(
        manifest_path(out),
        serde_json::to_string_pretty(&sorted(manifest))? + "\n",
    )?;
    println!(
        "snapshot of {} cases -> {} (+ {})",
        rows.len(),
        out.display(),
        manifest_path(out).display()
    );
    Ok(())
}

/// A run's manifest, class counts and full per-case records (responses and direct backend
/// checks, without timings or backend traces), one case per line so diffs stay readable.
fn json_snapshot(manifest: Value, records: IndexMap<String, Record>, out: &Path) -> Result<()> {
    let mut classes: std::collections::BTreeMap<String, usize> = Default::default();
    for r in records.values() {
        *classes.entry(r.class.clone()).or_default() += 1;
    }
    let lines = records
        .into_values()
        .map(|mut r| {
            r.ms = None;
            r.backend_trace.clear();
            Ok(format!(
                "    {}",
                serde_json::to_string(&sorted(serde_json::to_value(r)?))?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let head = |v: Value| -> Result<String> {
        Ok(serde_json::to_string_pretty(&sorted(v))?.replace('\n', "\n  "))
    };
    let text = format!(
        "{{\n  \"classes\": {},\n  \"manifest\": {},\n  \"results\": [\n{}\n  ]\n}}\n",
        head(serde_json::to_value(&classes)?)?,
        head(manifest)?,
        lines.join(",\n")
    );
    std::fs::write(out, text)?;
    println!("snapshot of {} cases -> {}", lines.len(), out.display());
    Ok(())
}

/// Recursively sorts object keys, so generated JSON is stable regardless of insertion order.
pub fn sorted(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<_, _> = map.into_iter().map(|(k, v)| (k, sorted(v))).collect();
            Value::Object(ordered.into_iter().collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        other => other,
    }
}

fn percentile<T: Copy + PartialOrd>(values: &[T], q: f64) -> T {
    let mut ordered = values.to_vec();
    ordered.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let index = ((q * ordered.len() as f64).ceil() as usize).saturating_sub(1);
    ordered[index.min(ordered.len() - 1)]
}

fn counter(items: impl IntoIterator<Item = String>) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in items {
        *counts.entry(item).or_default() += 1;
    }
    counts
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn display(record: &Record) -> String {
    record
        .summary
        .as_ref()
        .map(|s| s.display_name.clone())
        .filter(|name| !name.is_empty())
        .or_else(|| record.error.clone())
        .unwrap_or_default()
}

fn clip(text: &str, chars: usize) -> String {
    text.chars().take(chars).collect()
}

fn km(distance: Option<f64>) -> String {
    distance.map_or("?".into(), |d| d.to_string())
}

/// Markdown report and whether the run gate failed.
pub fn summarize(cases: &[Case], results: &[Record], manifest: &Value) -> (String, bool) {
    let by_id: IndexMap<_, _> = cases.iter().map(|c| (c.id.as_str(), c)).collect();
    let results: Vec<_> = results
        .iter()
        .filter(|r| by_id.contains_key(r.id.as_str()))
        .collect();
    let scored: Vec<_> = results
        .iter()
        .copied()
        .filter(|r| r.class != "unscored")
        .collect();
    let case = |r: &Record| by_id[r.id.as_str()];
    let mut out = String::new();
    let consumer = &manifest["consumer"];
    let status = &manifest["backend"]["status"];
    let _ = writeln!(
        out,
        "# OpenGeocoding evaluation ({}, {} cases)\n",
        manifest["suite"].as_str().unwrap_or("?"),
        results.len()
    );
    let _ = writeln!(
        out,
        "Backend: {} data {} ({}); consumer {}{}; region cache: {}.",
        status["software_version"].as_str().unwrap_or("?"),
        status["data_updated"].as_str().unwrap_or("?"),
        manifest["backend"]["label"]
            .as_str()
            .filter(|l| !l.is_empty())
            .unwrap_or("unlabelled"),
        clip(consumer["git"].as_str().unwrap_or(""), 12),
        if consumer["dirty"].as_bool().unwrap_or(false) {
            " (dirty)"
        } else {
            ""
        },
        consumer["region_cache"].as_str().unwrap_or("?"),
    );
    if let Some(stopped) = manifest["stopped"].as_str() {
        let _ = writeln!(out, "**Run stopped early: {stopped}.**");
    }
    out += "\n## Outcomes\n\n| Class | Cases |\n|---|---:|\n";
    for class in CLASSES.iter().chain(&["unscored"]) {
        let count = results.iter().filter(|r| r.class == *class).count();
        if count > 0 {
            let _ = writeln!(out, "| {class} | {count} |");
        }
    }
    let mut table = |title: &str, key: &dyn Fn(&Case) -> String| {
        let mut groups: BTreeMap<String, Vec<&Record>> = BTreeMap::new();
        for r in &scored {
            groups.entry(key(case(r))).or_default().push(r);
        }
        let _ = write!(
            out,
            "\n## {title}\n\n| Group | Pass | Cases | Locations passing | Critical |\n|---|---:|---:|---:|---:|\n"
        );
        for (group, rows) in groups {
            let mut locations: IndexMap<&str, bool> = IndexMap::new();
            for r in &rows {
                *locations.entry(case(r).location.as_str()).or_insert(true) &= r.class == "pass";
            }
            let _ = writeln!(
                out,
                "| {group} | {} | {} | {}/{} | {} |",
                rows.iter().filter(|r| r.class == "pass").count(),
                rows.len(),
                locations.values().filter(|v| **v).count(),
                locations.len(),
                rows.iter().filter(|r| critical(&r.class)).count()
            );
        }
    };
    table("By cohort", &|c| c.cohort.clone());
    table("Tuning vs held-out", &|c| {
        if c.holdout {
            "held-out".into()
        } else {
            "tuning".into()
        }
    });
    table("By expected country (per-case evidence, not rates)", &|c| {
        let mut countries: Vec<_> = c
            .expect
            .iter()
            .flat_map(|e| e.alternatives())
            .flat_map(|a| a.country.clone())
            .collect();
        countries.sort();
        countries.dedup();
        if countries.is_empty() {
            "-".into()
        } else {
            countries.join(",")
        }
    });
    let must: Vec<_> = scored
        .iter()
        .filter(|r| case(r).must_pass && r.class != "pass")
        .collect();
    out += "\n## Must-pass failures\n\n";
    for r in &must {
        let _ = writeln!(out, "- `{}` {}: {}", r.id, r.class, display(r));
    }
    if must.is_empty() {
        out += "None.\n";
    }
    let critical_results: Vec<_> = scored.iter().filter(|r| critical(&r.class)).collect();
    let untriaged = critical_results
        .iter()
        .any(|r| case(r).known_failure.is_none());
    let mut failed = !must.is_empty() || untriaged;
    out += "\n## Critical results (wrong country, >100 km, or a result where none exists)\n\n";
    for r in &critical_results {
        let s = r.summary.clone().unwrap_or_default();
        let _ = writeln!(
            out,
            "- [{}] `{}` {}{}: {:?} -> {} {:?} ({} km){}",
            if case(r).known_failure.is_some() {
                "known"
            } else {
                "NEW/UNTRIAGED"
            },
            r.id,
            r.class,
            if r.flags.iter().any(|f| f == "confident") {
                " **goodMatch=true**"
            } else {
                ""
            },
            r.query,
            s.country,
            clip(&s.display_name, 90),
            km(r.distance_km),
            r.attribution_hint
                .as_ref()
                .map(|h| format!("; {h}"))
                .unwrap_or_default()
        );
    }
    if critical_results.is_empty() {
        out += "None.\n";
    }
    out += "\n## Known failures\n\n";
    let known: Vec<_> = scored
        .iter()
        .filter(|r| case(r).known_failure.is_some())
        .collect();
    for r in &known {
        let state = if r.class == "pass" {
            "NOW PASSING (verify, then remove the label)"
        } else {
            &r.class
        };
        let _ = writeln!(
            out,
            "- `{}` {state}; recorded: {}",
            r.id,
            case(r).known_failure.as_deref().unwrap_or("")
        );
    }
    if known.is_empty() {
        out += "None.\n";
    }
    out += "\n## Other failures\n\n";
    let others: Vec<_> = scored
        .iter()
        .filter(|r| r.class != "pass" && !critical(&r.class) && case(r).known_failure.is_none())
        .collect();
    for r in &others {
        if let Some(error) = &r.error {
            let _ = writeln!(
                out,
                "- `{}` {} (RPC status {error}, no answer)",
                r.id, r.class
            );
            continue;
        }
        let _ = writeln!(
            out,
            "- `{}` {} (level {}, {} km): {:?}{}",
            r.id,
            r.class,
            r.level.as_deref().unwrap_or("?"),
            km(r.distance_km),
            clip(&display(r), 90),
            r.attribution_hint
                .as_ref()
                .map(|h| format!("; {h}"))
                .unwrap_or_default()
        );
    }
    if others.is_empty() {
        out += "None.\n";
    }
    suggest_section(&mut out, &results, &|r| case(r).cohort.clone());
    let unscored: Vec<_> = results.iter().filter(|r| r.class == "unscored").collect();
    if !unscored.is_empty() {
        let referenced = unscored.iter().any(|r| {
            case(r)
                .stored
                .as_ref()
                .is_some_and(|s| !s.as_object().is_none_or(|o| o.is_empty()))
        });
        let mut columns = vec![
            ("empty", "Empty"),
            ("good", "goodMatch"),
            ("fine", "House/street"),
        ];
        if referenced {
            columns.extend([
                ("referenced", "With reference point"),
                ("country", "Reference country differs"),
                ("moved", ">25 km from reference"),
                ("resolved", "Reference miss now resolved"),
                ("lost", "Reference result now empty"),
            ]);
        }
        let titles: Vec<_> = columns.iter().map(|(_, t)| *t).collect();
        let _ = write!(
            out,
            "\n## Unscored cases (drift only, not accuracy)\n\n| Cohort | Cases | {} |\n|---|---:|{}\n",
            titles.join(" | "),
            "---:|".repeat(columns.len())
        );
        let mut groups: BTreeMap<&str, Vec<&Record>> = BTreeMap::new();
        for r in &unscored {
            groups.entry(case(r).cohort.as_str()).or_default().push(r);
        }
        for (cohort, rows) in groups {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for r in &rows {
                let s = r.summary.clone().unwrap_or_default();
                let stored = case(r).stored.clone().unwrap_or(Value::Null);
                let reference = stored["lat"]
                    .as_f64()
                    .zip(stored["lon"].as_f64())
                    .map(|(a, b)| [a, b]);
                let stored_country = stored["country"].as_str().unwrap_or("");
                for (key, hit) in [
                    ("empty", s.point.is_none()),
                    ("good", s.good_match),
                    ("fine", s.level == "house" || s.level == "street"),
                    ("referenced", reference.is_some()),
                    (
                        "country",
                        !stored_country.is_empty()
                            && !s.country.is_empty()
                            && stored_country.to_lowercase() != s.country.to_lowercase(),
                    ),
                    (
                        "moved",
                        reference
                            .zip(s.point)
                            .is_some_and(|(a, b)| haversine(a, b) > 25.),
                    ),
                    // Only a recorded miss (a reference with no point) counts; no reference is not a miss.
                    (
                        "resolved",
                        stored.get("lat").is_some() && reference.is_none() && s.point.is_some(),
                    ),
                    ("lost", reference.is_some() && s.point.is_none()),
                ] {
                    *counts.entry(key).or_default() += usize::from(hit);
                }
            }
            let cells: Vec<_> = columns
                .iter()
                .map(|(k, _)| counts.get(k).copied().unwrap_or(0).to_string())
                .collect();
            let _ = writeln!(out, "| {cohort} | {} | {} |", rows.len(), cells.join(" | "));
        }
        if referenced {
            out += "\nReference values (`stored`) are earlier production geocodes or source-provided coordinates, not ground truth; a difference is something to review, not an error.\n";
        }
    }
    out += "\n## Backend and RPC behaviour\n\n";
    let calls: Vec<_> = results.iter().filter_map(|r| r.backend_calls).collect();
    if !calls.is_empty() {
        let _ = writeln!(
            out,
            "- Index searches per SearchAddress: mean {:.2}, p50 {}, p95 {}, max {}; backend error responses {}.",
            calls.iter().sum::<usize>() as f64 / calls.len() as f64,
            percentile(&calls, 0.5),
            percentile(&calls, 0.95),
            calls.iter().max().unwrap_or(&0),
            results
                .iter()
                .filter_map(|r| r.backend_errors)
                .sum::<usize>()
        );
    }
    let errors = counter(
        results
            .iter()
            .filter(|r| r.class != "pass")
            .filter_map(|r| r.error.clone()),
    );
    let _ = writeln!(
        out,
        "- RPC errors/timeouts: {}.",
        if errors.is_empty() { "none" } else { &errors }
    );
    let ok = |v: &Option<Value>, key: &str| v.as_ref().map(|v| v[key].as_bool().unwrap_or(false));
    let checks: [(&str, Vec<bool>); 5] = [
        (
            "ResolveRegion RPC returns an expected region",
            results
                .iter()
                .filter_map(|r| ok(&r.rpc_resolve_region, "ok"))
                .collect(),
        ),
        (
            "Direct reverse returns an expected region",
            results
                .iter()
                .filter_map(|r| ok(&r.direct_reverse, "region_ok"))
                .collect(),
        ),
        (
            "Direct details admin_level as expected",
            results
                .iter()
                .filter_map(|r| ok(&r.direct_details, "ok"))
                .collect(),
        ),
        (
            "Direct structured search top-1 passes",
            results
                .iter()
                .filter_map(|r| r.direct_structured.as_ref().map(|s| s == "pass"))
                .collect(),
        ),
        (
            "Direct raw search top-1 in area",
            results
                .iter()
                .filter_map(|r| r.direct_raw.as_ref().map(|d| d.top1 == "pass"))
                .collect(),
        ),
    ];
    for (label, values) in checks {
        if !values.is_empty() {
            let _ = writeln!(
                out,
                "- {label}: {}/{}.",
                values.iter().filter(|v| **v).count(),
                values.len()
            );
        }
    }
    let flags = counter(
        results
            .iter()
            .flat_map(|r| &r.flags)
            .filter(|f| *f != "confident")
            .map(|f| f.split(':').next().unwrap_or("").to_string()),
    );
    if !flags.is_empty() {
        let _ = writeln!(
            out,
            "- Region/enrichment flags (reported, not scored): {flags}."
        );
    }
    for (name, check) in manifest["transport"].as_object().into_iter().flatten() {
        let known = check["known_failure"].as_bool().unwrap_or(false);
        let state = match (check["ok"].as_bool(), known) {
            (Some(true), false) => "pass",
            (Some(true), true) => "NOW PASSING (known failure resolved?)",
            (Some(false), true) => "known failure reproduced",
            (Some(false), false) => {
                failed = true;
                "FAIL"
            }
            (None, _) => "not run",
        };
        let _ = writeln!(out, "- Transport `{name}`: {state}.");
    }
    let _ = writeln!(
        out,
        "\nGate: {} (must-pass cases, untriaged critical results, transport checks).",
        if failed { "FAIL" } else { "pass" }
    );
    (out, failed)
}

/// Where the expected place came, and what came first.
fn suggest_label(check: &SuggestCheck) -> String {
    let first = check.found.first().map_or("nothing".into(), |s| {
        format!("{} {} [{}]", s.kind, clip(&s.name, 60), s.country)
    });
    match (check.rank, &check.error) {
        (_, Some(error)) => format!("error {error}"),
        (Some(rank), _) => format!("rank {rank} (want <= {}); first {first}", check.want),
        (None, _) => format!("missing (want <= {}); first {first}", check.want),
    }
}

fn suggest_section(out: &mut String, results: &[&Record], cohort: &dyn Fn(&Record) -> String) {
    let mut groups: BTreeMap<(&str, String), Vec<usize>> = BTreeMap::new();
    for r in results {
        for (name, check) in &r.suggest {
            groups
                .entry((name, cohort(r)))
                .or_insert_with(|| vec![0; SUGGEST_CLASSES.len()])
                [suggest_severity(suggest_class(check))] += 1;
        }
    }
    if groups.is_empty() {
        return;
    }
    let _ = write!(
        out,
        "\n## SuggestAddress (reported, not gated)\n\n`full` types the query; `name` types the place's name alone, which must come no later than the places of that name with more population (Wikidata).\n\n| Check | Cohort | {} |\n|---|---|{}\n",
        SUGGEST_CLASSES.join(" | "),
        "---:|".repeat(SUGGEST_CLASSES.len())
    );
    for ((name, cohort), counts) in &groups {
        let cells: Vec<_> = counts.iter().map(usize::to_string).collect();
        let _ = writeln!(out, "| {name} | {cohort} | {} |", cells.join(" | "));
    }
    *out += "\n### Names out of population order\n\n";
    let mut none = true;
    for r in results {
        if let Some(check) = r.suggest.get("name").filter(|c| suggest_class(c) != "pass") {
            none = false;
            let _ = writeln!(
                out,
                "- `{}` {:?}: {}",
                r.id,
                check.text,
                suggest_label(check)
            );
        }
    }
    if none {
        *out += "None.\n";
    }
}

fn side_line(name: &str, manifest: &Value) -> String {
    let consumer = &manifest["consumer"];
    let status = &manifest["backend"]["status"];
    let short = |v: &Value| clip(v.as_str().unwrap_or("?"), 12);
    format!(
        "- {name}: {}, backend {} data {}, consumer {}{} binary {} models {}",
        manifest["backend"]["label"]
            .as_str()
            .filter(|l| !l.is_empty())
            .unwrap_or("unlabelled"),
        status["software_version"].as_str().unwrap_or("?"),
        status["data_updated"].as_str().unwrap_or("?"),
        short(&consumer["git"]),
        if consumer["dirty"].as_bool().unwrap_or(false) {
            " (dirty)"
        } else {
            ""
        },
        short(&consumer["binary_sha256"]),
        short(&consumer["libpostal_models_sha256"]),
    )
}

fn label(record: &Record) -> String {
    if let Some(error) = &record.error {
        return format!("error {error}");
    }
    let Some(s) = record.summary.as_ref().filter(|s| s.point.is_some()) else {
        return "(empty)".into();
    };
    let opt = |v: &Option<String>| v.clone().unwrap_or_default();
    let parts = [
        format!("{} {}", opt(&s.house_number), opt(&s.road))
            .trim()
            .to_string(),
        opt(&s.postcode),
        opt(&s.locality),
    ];
    let mut text = parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if text.is_empty() {
        text = clip(&s.display_name, 50);
    }
    let [lat, lon] = snapshot_point(s.point.unwrap_or_default());
    format!("{} {text} [{}] {lat},{lon}", s.level, s.country)
}

/// Base and head totals and percentiles in ms, and the relative change of each.
fn ms_table(old_ms: &[f64], new_ms: &[f64]) -> (String, Vec<f64>) {
    let mut table =
        String::from("| | Total | p50 | p90 | p99 | Max |\n|---|---:|---:|---:|---:|---:|\n");
    let mut stats = vec![];
    for (name, ms) in [("base", old_ms), ("head", new_ms)] {
        let row = [
            ms.iter().sum(),
            percentile(ms, 0.5),
            percentile(ms, 0.9),
            percentile(ms, 0.99),
            percentile(ms, 1.),
        ];
        let cells: Vec<_> = row.iter().map(|v| format!("{v:.1}")).collect();
        let _ = writeln!(table, "| {name} | {} |", cells.join(" | "));
        stats.push(row);
    }
    let change: Vec<f64> = stats[0]
        .iter()
        .zip(&stats[1])
        .map(|(a, b)| b / a.max(1e-3) - 1.)
        .collect();
    let cells: Vec<_> = change
        .iter()
        .map(|c| format!("{:+.0}%", c * 100.))
        .collect();
    let _ = writeln!(table, "| change | {} |", cells.join(" | "));
    (table, change)
}

/// SuggestAddress speed of the checks both sides timed, for the speed comment. Reported, not gated.
fn suggest_latency(
    shared: &[&String],
    old: &IndexMap<String, Record>,
    new: &IndexMap<String, Record>,
) -> String {
    let (old_ms, new_ms): (Vec<f64>, Vec<f64>) = shared
        .iter()
        .flat_map(|id| {
            new[*id]
                .suggest
                .iter()
                .filter_map(|(name, check)| Some((old[*id].suggest.get(name)?.ms?, check.ms?)))
        })
        .unzip();
    if old_ms.is_empty() {
        return String::new();
    }
    format!(
        "\n### SuggestAddress (reported, not gated)\n\nTime in ms over {} requests.\n\n{}",
        old_ms.len(),
        ms_table(&old_ms, &new_ms).0
    )
}

/// How much slower the candidate's total, p50, p90 or p99 may be. Three runs of one binary on a
/// cluster node agreed within 2% in total and 5% at each percentile; single cases moved by 30%,
/// so they are not compared, nor is the max (one case).
const MAX_SLOWDOWN: f64 = 0.10;

/// SearchAddress speed on both sides, between `<!-- speed: pass|fail|skip -->` and
/// `<!-- speed end -->` lines that CI cuts out into its own comment and commit status. Not part of
/// the `diff` exit code.
fn latency(
    shared: &[&String],
    old: &IndexMap<String, Record>,
    new: &IndexMap<String, Record>,
    manifests: [&Value; 2],
) -> String {
    let (old_ms, new_ms): (Vec<f64>, Vec<f64>) = shared
        .iter()
        .filter_map(|id| Some((old[*id].ms?, new[*id].ms?)))
        .unzip();
    if old_ms.is_empty() {
        return String::new();
    }
    const METRICS: [&str; 5] = ["total", "p50", "p90", "p99", "max"];
    let (table, change) = ms_table(&old_ms, &new_ms);
    let repeats = manifests.map(|m| m["repeat"].as_u64().unwrap_or(1));
    let slower: Vec<_> = METRICS[..4]
        .iter()
        .zip(&change)
        // on the rounded percentage the table shows, so +10% never fails a 10% limit
        .filter(|(_, c)| (**c * 100.).round() > MAX_SLOWDOWN * 100.)
        .map(|(m, c)| format!("{m} {:+.0}%", c * 100.))
        .collect();
    // Pacing between cases changes how warm the service is, so timings only compare at one rate.
    let rates = manifests.map(|m| m["max_rps"].as_f64());
    let limit = MAX_SLOWDOWN * 100.;
    let (verdict, headline) = if rates[0] != rates[1] {
        (
            "skip",
            "not compared, base and head ran at different --max-rps".to_string(),
        )
    } else if repeats[0] != repeats[1] {
        (
            "skip",
            format!(
                "not compared, base timed the fastest of {} requests per case and head of {}",
                repeats[0], repeats[1]
            ),
        )
    } else if slower.is_empty() {
        ("pass", format!("no slowdown over {limit:.0}%"))
    } else {
        (
            "fail",
            format!(
                "SearchAddress slower, {} (limit +{limit:.0}%)",
                slower.join(", ")
            ),
        )
    };
    format!(
        "\n<!-- speed: {verdict} -->\n## Speed: {headline}\n\n### SearchAddress\n\nTime in ms over {} cases, each the fastest of {} requests.\n\n{table}\nFails when the total, p50, p90 or p99 is more than {limit:.0}% slower. Runs of one binary differ by up to 5%; the max is one case and only reported.\n<!-- speed end -->\n",
        old_ms.len(),
        repeats[1]
    )
}

/// "SearchAddress 3340 → 3342 of 3759 passing (+2, +0.1%)", or "... 3342 of 3759 passing, unchanged".
fn passing(name: &str, [base, head]: [usize; 2], total: usize) -> String {
    if base == head {
        return format!("{name} {head} of {total} passing, unchanged");
    }
    let (delta, pct) = (
        head as i64 - base as i64,
        100. * (head as f64 / base.max(1) as f64 - 1.),
    );
    format!("{name} {base} → {head} of {total} passing ({delta:+}, {pct:+.1}%)")
}

/// A collapsed (or `open`) section; the blank lines let GitHub render the markdown inside.
fn fold(title: &str, body: &str, open: bool) -> String {
    format!(
        "\n<details{}>\n<summary><b>{title}</b></summary>\n\n{}\n</details>\n",
        if open { " open" } else { "" },
        body.trim_end()
    )
}

/// Baseline vs candidate report; returns whether the comparison gate failed.
pub fn diff(
    cases: &[Case],
    baseline: &Path,
    candidate: &Path,
    out: Option<&Path>,
    fail_on_drift: bool,
    relative: bool,
) -> Result<bool> {
    let cases: IndexMap<_, _> = cases.iter().map(|c| (c.id.as_str(), c)).collect();
    let (old_manifest, old) =
        load_side(baseline).with_context(|| format!("load {}", baseline.display()))?;
    let (new_manifest, new) =
        load_side(candidate).with_context(|| format!("load {}", candidate.display()))?;
    let shared: Vec<_> = new.keys().filter(|id| old.contains_key(*id)).collect();
    let mut report = format!(
        "# Baseline vs candidate ({} shared cases)\n\n",
        shared.len()
    );
    let mut runs = side_line("baseline", &old_manifest) + "\n";
    runs += &side_line("candidate", &new_manifest);
    runs += "\n";
    let missing = old.keys().filter(|k| !new.contains_key(*k)).count()
        + new.keys().filter(|k| !old.contains_key(*k)).count();
    if missing > 0 {
        let _ = writeln!(
            runs,
            "- {missing} case ids are present on only one side and not compared."
        );
    }
    for (name, records) in [("baseline", &old), ("candidate", &new)] {
        let calls: Vec<_> = records.values().filter_map(|r| r.backend_calls).collect();
        if !calls.is_empty() {
            let _ = writeln!(
                runs,
                "- {name} index searches per query: mean {:.2}, max {}",
                calls.iter().sum::<usize>() as f64 / calls.len() as f64,
                calls.iter().max().unwrap_or(&0)
            );
        }
    }
    let headline_at = report.len();
    report += &fold("Runs", &runs, false);
    let speed = latency(&shared, &old, &new, [&old_manifest, &new_manifest]);
    report += &speed.replace(
        "<!-- speed end -->",
        &(suggest_latency(&shared, &old, &new) + "<!-- speed end -->"),
    );
    let (mut newly_failing, mut newly_passing, mut worse, mut better) =
        (vec![], vec![], vec![], vec![]);
    let mut changed: IndexMap<&str, Vec<String>> = IndexMap::new();
    let mut by_cohort: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    let mut scored = false;
    let mut search_pass = [0; 2];
    // Per check kind: checked, base passing, head passing, head's other classes.
    let mut suggest_counts: BTreeMap<&str, [usize; 6]> = BTreeMap::new();
    for id in &shared {
        let (a, b) = (&old[*id], &new[*id]);
        search_pass[0] += usize::from(a.class == "pass");
        search_pass[1] += usize::from(b.class == "pass");
        let cohort = cases
            .get(id.as_str())
            .map_or("?".into(), |c| c.cohort.clone());
        let counts = by_cohort.entry(cohort.clone()).or_default();
        *counts.entry("cases").or_default() += 1;
        let kinds = drift(
            (a.error.as_deref(), a.summary.as_ref()),
            (b.error.as_deref(), b.summary.as_ref()),
        );
        if let Some(first) = kinds.first() {
            *counts.entry("changed").or_default() += 1;
            for kind in &kinds {
                *counts.entry(kind).or_default() += 1;
            }
            changed.entry(first).or_default().push(format!(
                "- `{id}` [{cohort}] {}: {:?}: {:?} -> {:?}",
                kinds.join(", "),
                clip(&b.query, 60),
                label(a),
                label(b)
            ));
        }
        for (name, sb) in &b.suggest {
            let Some(sa) = a.suggest.get(name) else {
                continue;
            };
            scored = true;
            let (ca, cb) = (suggest_class(sa), suggest_class(sb));
            let counts = suggest_counts.entry(name).or_default();
            counts[0] += 1;
            counts[1] += usize::from(ca == "pass");
            counts[2] += usize::from(cb == "pass");
            if cb != "pass" {
                counts[2 + suggest_severity(cb)] += 1;
            }
            let line = format!(
                "- `{id}` suggest {name} {ca} -> {cb}: {:?} -> {}",
                sb.text,
                suggest_label(sb)
            );
            if ca == "pass" && cb != "pass" {
                newly_failing.push(line);
            } else if ca != "pass" && cb == "pass" {
                newly_passing.push(line);
            } else if suggest_severity(cb) > suggest_severity(ca) {
                worse.push(line);
            } else if suggest_severity(cb) < suggest_severity(ca) {
                better.push(line);
            }
        }
        if a.class == "unscored" || b.class == "unscored" {
            continue;
        }
        scored = true;
        let line = format!(
            "- `{id}` {} -> {}: {:?} -> {:?}",
            a.class,
            b.class,
            b.query,
            label(b)
        );
        if a.class == "pass" && b.class != "pass" {
            newly_failing.push(line);
        } else if a.class != "pass" && b.class == "pass" {
            newly_passing.push(line);
        } else if severity(&b.class) > severity(&a.class) {
            worse.push(line);
        } else if severity(&b.class) < severity(&a.class) {
            better.push(line);
        }
    }
    let new_critical = shared
        .iter()
        .any(|id| critical(&new[*id].class) && !critical(&old[*id].class));
    // `relative` leaves out must-pass cases that also fail in the baseline: an index without the
    // full data fails some on both sides. One that passed there is a newly failing case anyway.
    let must = !relative
        && new.iter().any(|(id, r)| {
            cases.get(id.as_str()).is_some_and(|c| c.must_pass) && r.class != "pass"
        });
    let kinds: Vec<_> = DRIFT_KINDS
        .iter()
        .filter(|k| by_cohort.values().any(|c| c.contains_key(*k)))
        .collect();
    let rows: Vec<_> = by_cohort
        .iter()
        .filter(|(_, counts)| counts.contains_key("changed"))
        .collect();
    report += "\n## SearchAddress: changes by cohort\n\n";
    if rows.is_empty() {
        report += "No SearchAddress result changed.\n";
    } else {
        let _ = write!(
            report,
            "| Cohort | Cases | Changed |{}\n|---|---:|---:|{}\n",
            kinds.iter().map(|k| format!(" {k} |")).collect::<String>(),
            "---:|".repeat(kinds.len())
        );
        for (cohort, counts) in rows {
            let cells: Vec<_> = ["cases", "changed"]
                .iter()
                .chain(kinds.iter().map(|k| &**k))
                .map(|k| counts.get(k).copied().unwrap_or(0).to_string())
                .collect();
            let _ = writeln!(report, "| {cohort} | {} |", cells.join(" | "));
        }
        report += "\nOnly cohorts with a change and kinds that occur are shown. gained/lost: a result appeared/disappeared; finer/coarser: resolution level; moved: >1 km; nudged: <1 km but visible at 3 decimals.\n";
    }
    let mut headline = vec![passing("SearchAddress", search_pass, shared.len())];
    if !suggest_counts.is_empty() {
        let sums = suggest_counts
            .values()
            .fold([0; 3], |t, c| [t[0] + c[0], t[1] + c[1], t[2] + c[2]]);
        headline.push(passing("SuggestAddress", [sums[1], sums[2]], sums[0]));
    }
    report.insert_str(
        headline_at,
        &format!("<!-- headline: {} -->\n", headline.join(" · ")),
    );
    if !suggest_counts.is_empty() {
        report += "\n## SuggestAddress\n\n| Check | Checked | Pass base | Pass head | Change | ranked_low | missing | error |\n|---|---:|---:|---:|---:|---:|---:|---:|\n";
        for (name, c) in &suggest_counts {
            let _ = writeln!(
                report,
                "| {name} | {} | {} | {} | {:+} | {} | {} | {} |",
                c[0],
                c[1],
                c[2],
                c[2] as i64 - c[1] as i64,
                c[3],
                c[4],
                c[5]
            );
        }
        report += "\n`full` types the query; `name` types the place's name alone, which must come no later than the places of that name with more population. The last three columns count the head's failures.\n";
    }
    if scored {
        let mut none = vec![];
        // Gating lists stay open; the lists to review are folded.
        for (title, items, open) in [
            ("Newly failing", &newly_failing, true),
            ("Worse failure class", &worse, true),
            (
                "Newly passing (verify before relabelling)",
                &newly_passing,
                false,
            ),
            ("Better failure class", &better, false),
        ] {
            if items.is_empty() {
                none.push(title);
            } else if open {
                let _ = write!(
                    report,
                    "\n## {title} ({})\n\n{}\n",
                    items.len(),
                    items.join("\n")
                );
            } else {
                report += &fold(
                    &format!("{title} ({})", items.len()),
                    &items.join("\n"),
                    false,
                );
            }
        }
        if !none.is_empty() {
            let _ = write!(report, "\nNone: {}.\n", none.join(", ").to_lowercase());
        }
    }
    let total: usize = changed.values().map(Vec::len).sum();
    if total > 0 {
        let mut body = String::new();
        for kind in DRIFT_KINDS {
            if let Some(items) = changed.get(kind) {
                let _ = write!(
                    body,
                    "\n### {kind} ({})\n\n{}\n",
                    items.len(),
                    items.join("\n")
                );
            }
        }
        report += &fold(
            &format!("SearchAddress: changed cases ({total})"),
            &body,
            false,
        );
    }
    let failed = !newly_failing.is_empty()
        || !worse.is_empty()
        || new_critical
        || must
        || (fail_on_drift && total > 0);
    let _ = writeln!(
        report,
        "\nGate: {} (no newly failing or worse cases, no new critical results, {}{}).",
        if failed { "FAIL" } else { "pass" },
        if relative {
            "baseline failures not gated"
        } else {
            "all must-pass cases pass"
        },
        if fail_on_drift {
            ", no snapshot drift"
        } else {
            "; snapshot drift is reported, not gated"
        }
    );
    if let Some(out) = out {
        std::fs::write(out, &report)?;
    }
    print!("{report}");
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_snapshot_round_trips() {
        let dir = std::env::temp_dir().join(format!("snapshot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let record = json!({"id": "b", "query": "Paris", "class": "pass", "ms": 12,
            "backend_trace": [{"endpoint": "search", "q": "Paris", "language": "en", "status": 200, "ms": 2}],
            "summary": {"country": "fr", "level": "locality", "good_match": true}});
        let other = json!({"id": "a", "query": "", "class": "pass", "flags": []});
        std::fs::write(dir.join("manifest.json"), r#"{"suite":"gold"}"#).unwrap();
        std::fs::write(dir.join("results.jsonl"), format!("{record}\n{other}\n")).unwrap();
        let out = dir.join("snapshot.json");
        snapshot(&dir, &out).unwrap();
        let (manifest, records) = load_side(&out).unwrap();
        assert_eq!(manifest["suite"], "gold");
        assert_eq!(records.keys().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(records["b"].summary.as_ref().unwrap().country, "fr");
        assert!(records["b"].ms.is_none() && records["b"].backend_trace.is_empty());
        let text = std::fs::read_to_string(&out).unwrap();
        assert!(text.contains(r#""classes": {"#) && text.contains(r#""pass": 2"#));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn latency_compares_shared_timed_cases() {
        let side = |times: &[(&str, Option<f64>)]| -> IndexMap<String, Record> {
            times
                .iter()
                .map(|(id, ms)| {
                    let record = Record {
                        id: id.to_string(),
                        ms: *ms,
                        ..Default::default()
                    };
                    (id.to_string(), record)
                })
                .collect()
        };
        let old = side(&[("a", Some(1.)), ("b", Some(3.)), ("c", None)]);
        let new = side(&[("a", Some(2.)), ("b", Some(6.)), ("c", Some(9.))]);
        let ids: Vec<_> = old.keys().collect();
        let three = json!({"repeat": 3});
        let report = latency(&ids, &old, &new, [&three, &three]);
        assert!(report.starts_with("\n<!-- speed: fail -->\n"), "{report}");
        assert!(report.contains("total +100%, p50 +100%, p90 +100%, p99 +100%"));
        assert!(
            report.contains("over 2 cases, each the fastest of 3"),
            "{report}"
        );
        assert!(report.contains("| base | 4.0 | 1.0 | 3.0 | 3.0 | 3.0 |"));
        assert!(report.trim_end().ends_with("<!-- speed end -->"));
        // 10% slower is within the limit; only more fails.
        let near = side(&[("a", Some(1.1)), ("b", Some(3.3)), ("c", None)]);
        let report = latency(&ids, &old, &near, [&three, &three]);
        assert!(report.contains("<!-- speed: pass -->"), "{report}");
        let report = latency(&ids, &old, &new, [&json!({}), &three]);
        assert!(report.contains("<!-- speed: skip -->"), "{report}");
        let (slow, fast) = (
            json!({"repeat": 3, "max_rps": 20.}),
            json!({"repeat": 3, "max_rps": 1e6}),
        );
        let report = latency(&ids, &old, &new, [&slow, &fast]);
        assert!(report.contains("<!-- speed: skip -->"), "{report}");
        assert!(report.contains("--max-rps"), "{report}");
        let report = latency(&ids, &old, &new, [&fast, &fast]);
        assert!(report.contains("<!-- speed: fail -->"), "{report}");
        let untimed = side(&[("a", None), ("b", None), ("c", None)]);
        assert!(latency(&ids, &old, &untimed, [&json!({}), &json!({})]).is_empty());
    }

    #[test]
    fn nominatim_gold_reference_loads() {
        let path = crate::model::root().join("tests/snapshots/gold.nominatim.json");
        let (manifest, records) = load_side(&path).unwrap();
        assert_eq!(manifest["corpus"], "tests/gold.jsonl");
        assert!(records.len() >= 1000);
    }

    fn summary(cc: &str, level: &str, locality: &str) -> Summary {
        Summary {
            country: cc.into(),
            level: level.into(),
            point: Some([1., 1.]),
            good_match: true,
            display_name: cc.into(),
            locality: Some(locality.into()).filter(|l: &String| !l.is_empty()),
            ..Default::default()
        }
    }

    #[test]
    fn snapshot_round_trip_and_diff_gates() {
        let dir = std::env::temp_dir().join(format!("eval-diff-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cases: Vec<Case> = "abcd"
            .chars()
            .map(|id| Case {
                id: id.into(),
                cohort: "c".into(),
                ..Default::default()
            })
            .collect();
        let sides = [
            (
                "old",
                vec![
                    ("pass", Some(summary("dk", "locality", ""))),
                    ("abstain", None),
                    ("unscored", Some(summary("fr", "locality", "Paris"))),
                    ("unscored", Some(summary("de", "locality", ""))),
                ],
            ),
            (
                "new",
                vec![
                    ("wrong_country", Some(summary("se", "locality", ""))),
                    ("pass", Some(summary("dk", "locality", ""))),
                    ("unscored", Some(summary("fr", "house", "Paris"))),
                    ("unscored", Some(summary("de", "locality", ""))),
                ],
            ),
        ];
        for (name, rows) in sides {
            std::fs::create_dir_all(dir.join(name)).unwrap();
            std::fs::write(dir.join(name).join("manifest.json"), "{}").unwrap();
            let lines: String = "abcd"
                .chars()
                .zip(rows)
                .map(|(id, (class, summary))| {
                    serde_json::to_string(&Record {
                        id: id.into(),
                        query: id.into(),
                        class: class.into(),
                        summary,
                        ..Default::default()
                    })
                    .unwrap()
                        + "\n"
                })
                .collect();
            std::fs::write(dir.join(name).join("results.jsonl"), lines).unwrap();
        }
        snapshot(&dir.join("old"), &dir.join("old.tsv")).unwrap();
        assert_eq!(
            load_side(&dir.join("old.tsv")).unwrap().1["c"]
                .summary
                .as_ref()
                .unwrap()
                .locality
                .as_deref(),
            Some("Paris")
        );
        assert_eq!(
            json!(
                serde_json::from_str::<Value>(
                    &std::fs::read_to_string(dir.join("old.manifest.json")).unwrap()
                )
                .unwrap()
            ),
            json!({})
        );
        for baseline in [dir.join("old"), dir.join("old.tsv")] {
            for strict in [false, true] {
                let out = dir.join("diff.md");
                assert!(
                    diff(
                        &cases,
                        &baseline,
                        &dir.join("new"),
                        Some(&out),
                        strict,
                        false
                    )
                    .unwrap()
                );
                let report = std::fs::read_to_string(&out).unwrap();
                assert!(report.contains("## Newly failing (1)"), "{report}");
                assert!(
                    report.contains("<b>Newly passing (verify before relabelling) (1)</b>"),
                    "{report}"
                );
                assert!(
                    report.contains("### finer (1)") && report.contains("| c | 4 | 3 |"),
                    "{report}"
                );
            }
        }
        snapshot(&dir.join("new"), &dir.join("new.tsv")).unwrap();
        assert!(
            !diff(
                &cases,
                &dir.join("new.tsv"),
                &dir.join("new"),
                None,
                true,
                false
            )
            .unwrap()
        );
        let mut must_pass = cases;
        must_pass[3].must_pass = true;
        let new = dir.join("new");
        assert!(diff(&must_pass, &new, &new, None, false, false).unwrap());
        assert!(!diff(&must_pass, &new, &new, None, false, true).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_suggestion_ranked_lower_fails_the_diff() {
        let dir = std::env::temp_dir().join(format!("eval-suggest-{}", std::process::id()));
        let cases = [Case {
            id: "a".into(),
            ..Default::default()
        }];
        for (side, rank) in [("old", 1), ("new", 2)] {
            let check = SuggestCheck {
                text: "Paris".into(),
                want: 1,
                rank: Some(rank),
                ms: Some(rank as f64 * 2.),
                ..Default::default()
            };
            let record = Record {
                id: "a".into(),
                class: "pass".into(),
                suggest: [("name".into(), check)].into(),
                ..Default::default()
            };
            std::fs::create_dir_all(dir.join(side)).unwrap();
            std::fs::write(dir.join(side).join("manifest.json"), "{}").unwrap();
            std::fs::write(
                dir.join(side).join("results.jsonl"),
                serde_json::to_string(&record).unwrap(),
            )
            .unwrap();
        }
        let (old, new, out) = (dir.join("old"), dir.join("new"), dir.join("diff.md"));
        assert!(diff(&cases, &old, &new, Some(&out), false, true).unwrap());
        let report = std::fs::read_to_string(&out).unwrap();
        assert!(
            report.contains("`a` suggest name pass -> ranked_low"),
            "{report}"
        );
        assert!(
            report.contains("| name | 1 | 1 | 0 | -1 | 1 | 0 | 0 |"),
            "{report}"
        );
        assert!(
            report.contains("\n<!-- headline: SearchAddress 1 of 1 passing, unchanged · SuggestAddress 1 → 0 of 1 passing (-1, -100.0%) -->\n"),
            "{report}"
        );
        assert_eq!(
            passing("SearchAddress", [3340, 3342], 3759),
            "SearchAddress 3340 → 3342 of 3759 passing (+2, +0.1%)"
        );
        let (a, b) = (load_side(&old).unwrap().1, load_side(&new).unwrap().1);
        let ids: Vec<_> = a.keys().collect();
        let speed = suggest_latency(&ids, &a, &b);
        assert!(speed.contains("over 1 requests"), "{speed}");
        assert!(speed.contains("| change | +100% |"), "{speed}");
        assert!(!diff(&cases, &old, &old, Some(&out), false, true).unwrap());
        let report = std::fs::read_to_string(&out).unwrap();
        assert!(
            report.contains("No SearchAddress result changed."),
            "{report}"
        );
        assert!(report.contains("None: newly failing, worse failure class"));
        assert!(!report.contains("| Cohort") && !report.contains("changed cases"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
