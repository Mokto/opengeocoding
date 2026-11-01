use crate::{
    model::{Case, Located, Record, SuggestCheck, load_jsonl},
    score::{
        CLASSES, DRIFT_KINDS, LOCATE_CLASSES, SUGGEST_CLASSES, Summary, comparable, critical,
        drift, expected_place, haversine, locate_severity, severity, snapshot_point, suggest_class,
        suggest_severity,
    },
};
use anyhow::{Context, Result};
use indexmap::IndexMap;
use opengeocoding::eval::{Row, read_tsv, write_tsv};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    path::Path,
};

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
            r.reverse_ms = None;
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
    if results
        .iter()
        .any(|r| evidence(case(r)) == "location diagnostic")
    {
        out += "\n**Building-location diagnostics:** proximity, level and confidence do not establish building/B-number identity, a postal address or an entrance. Their reverse checks establish only the expected country/area, not the building.\n";
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
    table(
        "By expected country (per-case evidence, not rates)",
        &expected_countries,
    );
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
    let mut failed = !must.is_empty()
        || untriaged
        || results.iter().any(|r| {
            r.suggest
                .values()
                .any(|c| c.landmarks_off && suggest_class(c) != "pass")
        });
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
    locate_section(&mut out, &results, &|r| case(r).cohort.clone());
    let unscored: Vec<_> = results
        .iter()
        .filter(|r| {
            r.class == "unscored"
                && case(r).suggest_address.is_none()
                && !case(r)
                    .expect
                    .as_ref()
                    .is_some_and(|e| e.level == "landmark")
        })
        .collect();
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
        "\nGate: {} (must-pass cases, untriaged critical results, landmark flag-off and transport checks).",
        if failed { "FAIL" } else { "pass" }
    );
    (out, failed)
}

/// Where the expected place came, and what came first.
fn suggest_label(check: &SuggestCheck) -> String {
    if check.landmarks_off && check.error.is_none() {
        return format!(
            "flag off; {} landmark suggestions",
            check.found.iter().filter(|s| s.kind == "landmark").count()
        );
    }
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
        "\n## SuggestAddress (geography reported, flag-off checks gated)\n\n`full` types the query; `name` types the place's name alone, which must come no later than the places of that name with more population (Wikidata). For landmark cases, `full` enables landmarks and `off` repeats the query with the flag false, requiring no landmark results.\n\n| Check | Cohort | {} |\n|---|---|{}\n",
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
    let landmarks: Vec<_> = results
        .iter()
        .filter(|r| cohort(r) == "suggest_landmark")
        .filter_map(|r| Some((r, r.suggest.get("full")?)))
        .filter(|(_, c)| suggest_class(c) != "pass")
        .collect();
    if !landmarks.is_empty() {
        *out += "\n### Landmarks not suggested\n\n";
        for (r, check) in landmarks {
            let _ = writeln!(
                out,
                "- `{}` {:?}: {}",
                r.id,
                check.text,
                suggest_label(check)
            );
        }
    }
}

/// The checks of a record that ask where a point is: ReverseGeocode at the point and off a
/// house's point, and ResolveRegion. A check's RPC is its name's first word.
fn located(record: &Record) -> [(&'static str, Option<&Located>); 3] {
    [
        ("ReverseGeocode", record.reverse.as_ref()),
        ("ReverseGeocode offset", record.reverse_offset.as_ref()),
        ("ResolveRegion", record.region.as_ref()),
    ]
}

fn rpc_of(check: &'static str) -> &'static str {
    check.split(' ').next().unwrap_or(check)
}

fn suggest_rpc(check: &SuggestCheck) -> &'static str {
    if check.landmarks_off {
        "SuggestAddress (flag off)"
    } else {
        "SuggestAddress"
    }
}

fn diagnostic_rpc(check: &'static str) -> &'static str {
    match check {
        "SearchAddress" => "SearchAddress (location only)",
        "SuggestAddress" => "SuggestAddress (location only)",
        "ReverseGeocode" => "ReverseGeocode (country/area only)",
        "ResolveRegion" => "ResolveRegion (country/area only)",
        _ => check,
    }
}

fn locate_label(found: &Located) -> String {
    match &found.error {
        Some(error) => format!("error {error}"),
        None if found.country.is_empty() => "nothing".into(),
        None => {
            let places = found.places.as_deref().unwrap_or_default();
            let places = places
                .iter()
                .take(3)
                .map(String::as_str)
                .collect::<Vec<_>>();
            let house = found.answer.as_ref().map_or(String::new(), |a| {
                let parts = [&a.house_number, &a.road, &a.postcode];
                let parts: Vec<_> = parts
                    .iter()
                    .filter(|p| !p.is_empty())
                    .map(|p| p.as_str())
                    .collect();
                format!(" \u{b7} {} {}", a.layer, parts.join(" "))
            });
            format!(
                "[{}] {} {}{house}",
                found.country,
                found.regions.join(", "),
                places.join(", ")
            )
            .trim_end()
            .into()
        }
    }
}

fn locate_section(out: &mut String, results: &[&Record], cohort: &dyn Fn(&Record) -> String) {
    let mut groups: BTreeMap<(&str, String), Vec<usize>> = BTreeMap::new();
    for r in results {
        for (name, found) in located(r) {
            if let Some(found) = found {
                groups
                    .entry((name, cohort(r)))
                    .or_insert_with(|| vec![0; LOCATE_CLASSES.len()])
                    [locate_severity(&found.class)] += 1;
            }
        }
    }
    if groups.is_empty() {
        return;
    }
    let _ = write!(
        out,
        "\n## ReverseGeocode and ResolveRegion\n\nBoth are asked about each case's single expected place, including same-point address alternatives. ReverseGeocode: the nearest result is in the expected country (and region, where the case has one), names the place (a locality's English Wikidata label, or one of the source's names for a house's town) and, at a house's point, is that house (number, within the case's radius, postcode); `offset` asks 25 m away and wants a house within the radius. ResolveRegion: one of the regions found is an expected one.\n\n| Check | Cohort | {} |\n|---|---|{}\n",
        LOCATE_CLASSES.join(" | "),
        "---:|".repeat(LOCATE_CLASSES.len())
    );
    for ((name, cohort), counts) in &groups {
        let cells: Vec<_> = counts.iter().map(usize::to_string).collect();
        let _ = writeln!(out, "| {name} | {cohort} | {} |", cells.join(" | "));
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

/// How much slower the candidate's total, p50, p90 or p99 may be. Three runs of one binary on a
/// cluster node agreed within 2% in total and 5% at each percentile; single cases moved by 30%,
/// so they are not compared, nor is the max (one case).
const MAX_SLOWDOWN: f64 = 0.15;

/// Newly failing OpenStreetMap-panel locations pass the gate when each is outweighed by this
/// many newly passing ones, and there are at most `MAX_TRADED` of them. Losses anywhere else, a
/// worse failure class (an abstain turned wrong), a new critical result or a must-pass failure
/// are never traded.
const TRADE: usize = 5;
const MAX_TRADED: usize = 5;

/// Per-request times of one RPC on both sides, over the requests both timed.
struct Timings {
    rpc: &'static str,
    unit: &'static str,
    old: Vec<f64>,
    new: Vec<f64>,
}

/// The speed section, between `<!-- speed: pass|fail|skip -->` and `<!-- speed end -->` lines that
/// CI cuts out into its own comment and commit status: SearchAddress, SuggestAddress and
/// ReverseGeocode, each gated alike. Not part of the `diff` exit code.
fn latency(
    shared: &[&String],
    old: &IndexMap<String, Record>,
    new: &IndexMap<String, Record>,
    manifests: [&Value; 2],
) -> String {
    type Pairs = Vec<(f64, f64)>;
    let series = |rpc, unit, times: &dyn Fn(&Record, &Record) -> Pairs| {
        let (old, new) = shared
            .iter()
            .flat_map(|id| times(&old[*id], &new[*id]))
            .unzip();
        Timings {
            rpc,
            unit,
            old,
            new,
        }
    };
    let timings: Vec<_> = [
        series("SearchAddress", "cases", &|a, b| {
            a.ms.zip(b.ms).into_iter().collect()
        }),
        series("SuggestAddress", "checks", &|a, b| {
            b.suggest
                .iter()
                .filter_map(|(name, check)| Some((a.suggest.get(name)?.ms?, check.ms?)))
                .collect()
        }),
        series("ReverseGeocode", "cases", &|a, b| {
            a.reverse_ms.zip(b.reverse_ms).into_iter().collect()
        }),
    ]
    .into_iter()
    .filter(|t| !t.old.is_empty())
    .collect();
    if timings.is_empty() {
        return String::new();
    }
    let repeats = manifests.map(|m| m["repeat"].as_u64().unwrap_or(1));
    let (mut sections, mut slower) = (String::new(), vec![]);
    for t in &timings {
        let (table, change) = ms_table(&t.old, &t.new);
        let _ = write!(
            sections,
            "\n### {}\n\nTime in ms over {} {}, each the fastest of {} requests.\n\n{table}",
            t.rpc,
            t.old.len(),
            t.unit,
            repeats[1]
        );
        let metrics = ["total", "p50", "p90", "p99"]
            .iter()
            .zip(&change)
            // on the rounded percentage the table shows, so +10% never fails a 10% limit
            .filter(|(_, c)| (**c * 100.).round() > MAX_SLOWDOWN * 100.)
            .map(|(m, c)| format!("{m} {:+.0}%", c * 100.))
            .collect::<Vec<_>>();
        if !metrics.is_empty() {
            slower.push(format!("{} slower, {}", t.rpc, metrics.join(", ")));
        }
    }
    // Pacing between cases changes how warm the service is, so timings only compare at one rate.
    let rates = manifests.map(|m| m["max_rps"].as_f64());
    let limit = MAX_SLOWDOWN * 100.;
    let (verdict, headline) = if rates[0] != rates[1] {
        (
            "skip",
            "not compared, base and head ran at different --max-rps".to_string(),
        )
    } else if manifests[0]["speed_cases"] != manifests[1]["speed_cases"] {
        // Timed apart from the other cases or among them, the service is not equally warm.
        (
            "skip",
            "not compared, base and head timed different --speed-cases".to_string(),
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
            format!("{} (limit +{limit:.0}%)", slower.join("; ")),
        )
    };
    let _ = writeln!(
        sections,
        "\nFails when the total, p50, p90 or p99 of any table is more than {limit:.0}% slower. Runs of one binary differ by up to 5%; the max is one request and only reported."
    );
    if verdict == "pass" {
        sections = fold("Details", &sections, false);
    }
    format!("\n<!-- speed: {verdict} -->\n## Speed: {headline}\n{sections}\n<!-- speed end -->\n")
}

/// "| SearchAddress | 3759 | 3340 | 3342 | +2 (+0.1%) | 5 |", a row of the diff's summary table.
/// The countries a case expects, comma-joined ("-" for none).
fn expected_countries(c: &Case) -> String {
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
}

/// Two cells of the per-country table: passing checks on both sides out of those checked, and
/// the same as a percentage.
fn country_cells([checked, base, head]: [usize; 3]) -> [String; 2] {
    let pct = |n| format!("{:.1}%", 100. * n as f64 / checked as f64);
    match (checked, base == head) {
        (0, _) => ["-".into(), "-".into()],
        (_, true) => [format!("{head} / {checked}"), pct(head)],
        _ => [
            format!(
                "{base} → {head} / {checked} ({:+})",
                head as i64 - base as i64
            ),
            format!("{} → {}", pct(base), pct(head)),
        ],
    }
}

/// "Belgium, France" for "be,fr": the English names of a joined group of country codes.
fn country_label(codes: &str) -> String {
    codes
        .split(',')
        .map(|cc| opengeocoding::matching::country_name(cc).unwrap_or(cc))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Per country (the expected one; a case with several is counted in the joined group), the
/// passing checks of each RPC on both sides, for the countries where any of them changed, those
/// with the largest change first. Every country is in `by_evidence`, at the report's end.
fn by_country(counts: &BTreeMap<String, BTreeMap<&str, [usize; 3]>>) -> String {
    const RPCS: [&str; 4] = [
        "SearchAddress",
        "SuggestAddress",
        "ReverseGeocode",
        "ResolveRegion",
    ];
    if counts.is_empty() {
        return String::new();
    }
    let header = format!(
        "| Country | {} |\n|---|{}\n",
        RPCS.map(|rpc| format!("{rpc} | %")).join(" | "),
        "---:|".repeat(2 * RPCS.len())
    );
    let row = |country: &str, rpcs: &BTreeMap<&str, [usize; 3]>| {
        let cells: Vec<_> = RPCS
            .iter()
            .flat_map(|rpc| country_cells(rpcs.get(rpc).copied().unwrap_or_default()))
            .collect();
        format!("| {} | {} |\n", country_label(country), cells.join(" | "))
    };
    let mut rows: Vec<_> = counts
        .iter()
        .map(|(country, rpcs)| {
            let delta: i64 = rpcs.values().map(|c| c[2] as i64 - c[1] as i64).sum();
            let moved: i64 = rpcs
                .values()
                .map(|c| (c[2] as i64 - c[1] as i64).abs())
                .sum();
            (country, rpcs, delta, moved)
        })
        .filter(|(.., moved)| *moved > 0)
        .collect();
    rows.sort_by_key(|(country, _, delta, moved)| (std::cmp::Reverse(*moved), *delta, *country));
    let mut out = String::from("\n## Pass rate by country\n\n");
    if rows.is_empty() {
        out += "No expected country's passing checks changed.\n";
    } else {
        let shown = rows.len().min(40);
        let _ = write!(
            out,
            "Passing checks out of those checked, baseline → candidate, for the {} expected countr{} where any RPC changed. A case that expects several countries has its own row. Counts are checks, not independent locations.\n\n{header}",
            rows.len(),
            if rows.len() == 1 { "y" } else { "ies" },
        );
        for (country, rpcs, ..) in &rows[..shown] {
            out += &row(country, rpcs);
        }
        if rows.len() > shown {
            let _ = writeln!(
                out,
                "\n{} more countries changed, with smaller changes.",
                rows.len() - shown
            );
        }
    }
    out
}

/// Passing checks per country (as `by_country`) by where the expectation comes from: `osm` for
/// the OpenStreetMap address panel, which the index reads itself in many countries; otherwise
/// `registry`, `places`, `landmarks`, `towns`, `gleif`, `constructed` or location diagnostics.
#[derive(Default)]
struct Evidence {
    independent: BTreeMap<String, EvidenceRow>,
    osm: BTreeMap<String, EvidenceRow>,
    diagnostics: BTreeMap<String, EvidenceRow>,
}

type EvidenceRow = (BTreeSet<&'static str>, BTreeMap<&'static str, [usize; 3]>);

fn evidence(case: &Case) -> &'static str {
    if case
        .source
        .as_ref()
        .is_some_and(|s| s["scope"] == "building-location-diagnostic")
    {
        return "location diagnostic";
    }
    let source = case
        .source
        .as_ref()
        .and_then(|s| s["name"].as_str())
        .unwrap_or("");
    if source.starts_with("openstreetmap") {
        "osm"
    } else if case.cohort == "town" {
        "towns"
    } else if source == "gleif" {
        "gleif"
    } else if case.expect.as_ref().is_some_and(|e| e.level == "landmark") {
        "landmarks"
    } else if source == "wikidata" {
        "places"
    } else if source.is_empty() || source == "constructed" {
        "constructed"
    } else {
        "registry"
    }
}

/// One RPC's passing checks out of those checked, and the rate: `18/20 · 90.0%`, or
/// `17→18/20 · 85.0%→90.0%` when it changed.
fn rate_cell([checked, base, head]: [usize; 3]) -> String {
    let pct = |n| format!("{:.1}%", 100. * n as f64 / checked as f64);
    match checked {
        0 => "-".into(),
        _ if base == head => format!("{head}/{checked} · {}", pct(head)),
        _ => format!("{base}→{head}/{checked} · {}→{}", pct(base), pct(head)),
    }
}

/// Passing checks by evidence, between markers the nightly gold issue reads.
fn by_evidence(counts: &Evidence) -> String {
    const RPCS: [&str; 4] = [
        "SearchAddress",
        "SuggestAddress",
        "ReverseGeocode",
        "ResolveRegion",
    ];
    if counts.independent.is_empty() && counts.osm.is_empty() && counts.diagnostics.is_empty() {
        return String::new();
    }
    let table = |rows: &BTreeMap<String, EvidenceRow>, kinds: bool, diagnostic: bool| {
        let cells = |rpcs: &BTreeMap<&str, [usize; 3]>| {
            RPCS.map(|rpc| rate_cell(rpcs.get(rpc).copied().unwrap_or_default()))
                .join(" | ")
        };
        let mut all: BTreeMap<&str, [usize; 3]> = BTreeMap::new();
        for (_, rpcs) in rows.values() {
            for (rpc, c) in rpcs {
                let t = all.entry(rpc).or_default();
                *t = [t[0] + c[0], t[1] + c[1], t[2] + c[2]];
            }
        }
        let kind = |k: &BTreeSet<&str>| match (kinds, k.is_empty()) {
            (false, _) => String::new(),
            (true, true) => " |".into(),
            (true, false) => format!(" {} |", k.iter().copied().collect::<Vec<_>>().join(", ")),
        };
        let mut out = format!(
            "| Country |{} {} |\n|---|{}{}\n| **All** |{} {} |\n",
            if kinds { " Evidence |" } else { "" },
            if diagnostic {
                RPCS.map(diagnostic_rpc)
            } else {
                RPCS
            }
            .join(" | "),
            if kinds { "---|" } else { "" },
            "---:|".repeat(RPCS.len()),
            kind(&BTreeSet::new()),
            cells(&all)
        );
        let mut named: Vec<_> = rows
            .iter()
            .map(|(country, (k, rpcs))| (country_label(country), k, rpcs))
            .collect();
        named.sort();
        for (label, k, rpcs) in named {
            let _ = writeln!(out, "| {label} |{} {} |", kind(k), cells(rpcs));
        }
        out
    };
    let mut out = String::from(
        "\n<!-- by-evidence -->\n## Pass rate by country and evidence\n\nThe \"Independent sources\" table groups registers, curated Wikidata places, Wikidata towns and GLEIF company addresses placed by Wikidata towns, selected outside the index. Registers can also supply index data: those checks measure source fidelity, not independent real-world accuracy. OpenStreetMap cases check agreement with mapped addresses, which the index itself reads in many countries: consistency, not accuracy. Operator landmarks are reported as landmarks, not address registers. Flag-off integrity checks are excluded from retrieval rates. Each cell is passing checks / checked · rate, baseline → candidate where it changed.\n",
    );
    let countries = |n: usize| format!("{n} countr{}", if n == 1 { "y" } else { "ies" });
    out += &fold(
        &format!(
            "Independent sources ({})",
            countries(counts.independent.len())
        ),
        &table(&counts.independent, true, false),
        false,
    );
    out += &fold(
        &format!(
            "Agreement with OpenStreetMap ({})",
            countries(counts.osm.len())
        ),
        &table(&counts.osm, false, false),
        false,
    );
    if !counts.diagnostics.is_empty() {
        out += "\nBuilding-location diagnostics are excluded from the source-fidelity totals above. They check proximity, level and confidence, not building identity or postal/entrance accuracy; their reverse checks establish only the expected country/area. Landmark flag-off integrity checks are excluded from retrieval rates.\n";
        out += &fold(
            &format!(
                "Building-location diagnostics ({})",
                countries(counts.diagnostics.len())
            ),
            &table(&counts.diagnostics, false, true),
            false,
        );
    }
    out + "<!-- by-evidence end -->\n"
}

fn passing(name: &str, [base, head]: [usize; 2], total: usize, changed: usize) -> String {
    let delta = if base == head {
        "0".into()
    } else {
        format!(
            "{:+} ({:+.1}%)",
            head as i64 - base as i64,
            100. * (head as f64 / base.max(1) as f64 - 1.)
        )
    };
    format!("| {name} | {total} | {base} | {head} | {delta} | {changed} |\n")
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
    let summary_at = report.len();
    report += &fold("Runs", &runs, false);
    report += &latency(&shared, &old, &new, [&old_manifest, &new_manifest]);
    let (mut newly_failing, mut newly_passing, mut worse, mut better) =
        (vec![], vec![], vec![], vec![]);
    // Locations with a newly failing or newly passing check, and whether a loss can be traded.
    let (mut lost, mut won) = (BTreeMap::new(), BTreeSet::new());
    let mut changed: IndexMap<&str, Vec<String>> = IndexMap::new();
    let mut by_cohort: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    let mut summary_counts: BTreeMap<&str, [usize; 4]> = BTreeMap::new();
    // Per check kind: checked, base passing, head passing, head's other classes.
    let mut suggest_counts: BTreeMap<&str, [usize; 2 + SUGGEST_CLASSES.len()]> = BTreeMap::new();
    let mut added_counts: BTreeMap<&str, [usize; 2]> = BTreeMap::new();
    let mut country_counts: BTreeMap<String, BTreeMap<&str, [usize; 3]>> = BTreeMap::new();
    let mut evidence_counts = Evidence::default();
    let mut added_failures = String::new();
    for (id, record) in &new {
        for (name, check) in &record.suggest {
            if old.get(id).is_some_and(|r| r.suggest.contains_key(name)) {
                continue;
            }
            let class = suggest_class(check);
            let counts = added_counts.entry(name).or_default();
            counts[0] += 1;
            counts[1] += usize::from(class == "pass");
            if class != "pass" {
                let line = format!(
                    "- `{id}` suggest {name} {class}: {:?}: {}",
                    check.text,
                    suggest_label(check)
                );
                let _ = writeln!(added_failures, "{line}");
                if check.landmarks_off {
                    newly_failing.push(line);
                    lost.insert(id.clone(), false);
                }
            }
        }
    }
    if !added_counts.is_empty() {
        report += "\n## New SuggestAddress checks (head only)\n\nThese have no baseline and are not improvements to shared cases. For landmark cases, `full` has the flag on; `off` requires no landmarks with the flag false.\n\n| Check | Pass head | Total |\n|---|---:|---:|\n";
        for (name, counts) in added_counts {
            let _ = writeln!(report, "| {name} | {} | {} |", counts[1], counts[0]);
        }
        report.push('\n');
        report += &added_failures;
    }
    // Per ReverseGeocode and ResolveRegion: checked, base passing, head passing, head's classes.
    let mut locate_counts: BTreeMap<&str, [usize; 2 + LOCATE_CLASSES.len()]> = BTreeMap::new();
    let mut changed_checks: BTreeMap<&str, usize> = BTreeMap::new();
    for id in &shared {
        let (a, b) = (&old[*id], &new[*id]);
        let cohort = cases
            .get(id.as_str())
            .map_or("?".into(), |c| c.cohort.clone());
        let country = cases
            .get(id.as_str())
            .map_or("-".into(), |c| expected_countries(c));
        let kind = cases
            .get(id.as_str())
            .map_or("constructed", |c| evidence(c));
        let location = cases
            .get(id.as_str())
            .map(|c| &c.location)
            .filter(|l| !l.is_empty())
            .unwrap_or(id)
            .clone();
        let lose = |lost: &mut BTreeMap<String, bool>| {
            let tradable = lost.entry(location.clone()).or_insert(true);
            *tradable &= kind == "osm";
        };
        let mut tally = |check: &'static str, base: bool, head: bool, changed: bool| {
            let diagnostic = kind == "location diagnostic";
            let label = if diagnostic {
                diagnostic_rpc(check)
            } else {
                check
            };
            let counts = summary_counts.entry(label).or_default();
            *counts = [
                counts[0] + 1,
                counts[1] + usize::from(base),
                counts[2] + usize::from(head),
                counts[3] + usize::from(changed),
            ];
            if check == "SuggestAddress (flag off)" {
                return;
            }
            let rpc = rpc_of(check);
            let add = |c: &mut [usize; 3]| {
                *c = [c[0] + 1, c[1] + usize::from(base), c[2] + usize::from(head)];
            };
            if !diagnostic {
                add(country_counts
                    .entry(country.clone())
                    .or_default()
                    .entry(rpc)
                    .or_default());
            }
            let rows = match kind {
                "osm" => &mut evidence_counts.osm,
                "location diagnostic" => &mut evidence_counts.diagnostics,
                _ => &mut evidence_counts.independent,
            };
            let row = rows.entry(country.clone()).or_default();
            row.0.insert(kind);
            add(row.1.entry(rpc).or_default());
        };
        let counts = by_cohort.entry(cohort.clone()).or_default();
        *counts.entry("cases").or_default() += 1;
        let kinds = drift(
            (a.error.as_deref(), a.summary.as_ref()),
            (b.error.as_deref(), b.summary.as_ref()),
        );
        if !cases.get(id.as_str()).is_some_and(|c| {
            c.suggest_address.is_some() || c.expect.as_ref().is_some_and(|e| e.level == "landmark")
        }) {
            tally(
                "SearchAddress",
                a.class == "pass",
                b.class == "pass",
                !kinds.is_empty(),
            );
        }
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
            let (ca, cb) = (suggest_class(sa), suggest_class(sb));
            if ca != cb {
                *changed_checks.entry(name.as_str()).or_default() += 1;
            }
            tally(suggest_rpc(sb), ca == "pass", cb == "pass", ca != cb);
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
                lose(&mut lost);
            } else if ca != "pass" && cb == "pass" {
                newly_passing.push(line);
                won.insert(location.clone());
            } else if suggest_severity(cb) > suggest_severity(ca) {
                worse.push(line);
            } else if suggest_severity(cb) < suggest_severity(ca) {
                better.push(line);
            }
        }
        for ((name, fa), (_, fb)) in located(a).into_iter().zip(located(b)) {
            let (Some(fa), Some(fb)) = (fa, fb) else {
                continue;
            };
            let level = cases
                .get(id.as_str())
                .and_then(|c| c.expect.as_ref())
                .and_then(expected_place)
                .map_or("", |e| e.level.as_str());
            let (ca, cb) = comparable(fa, fb, level);
            if ca != cb {
                *changed_checks.entry(name).or_default() += 1;
            }
            tally(name, ca == "pass", cb == "pass", ca != cb);
            let counts = locate_counts.entry(name).or_default();
            counts[0] += 1;
            counts[1] += usize::from(ca == "pass");
            counts[2] += usize::from(cb == "pass");
            if cb != "pass" {
                counts[2 + locate_severity(cb)] += 1;
            }
            let line = format!(
                "- `{id}` {name} {ca} -> {cb}: {:?}: {} -> {}",
                clip(&b.query, 60),
                locate_label(fa),
                locate_label(fb)
            );
            if ca == "pass" && cb != "pass" {
                newly_failing.push(line);
                lose(&mut lost);
            } else if ca != "pass" && cb == "pass" {
                newly_passing.push(line);
                won.insert(location.clone());
            } else if locate_severity(cb) > locate_severity(ca) {
                worse.push(line);
            } else if locate_severity(cb) < locate_severity(ca) {
                better.push(line);
            }
        }
        if a.class == "unscored" || b.class == "unscored" {
            continue;
        }
        let line = format!(
            "- `{id}` {} -> {}: {:?} -> {:?}",
            a.class,
            b.class,
            b.query,
            label(b)
        );
        if a.class == "pass" && b.class != "pass" {
            newly_failing.push(line);
            lose(&mut lost);
        } else if a.class != "pass" && b.class == "pass" {
            newly_passing.push(line);
            won.insert(location.clone());
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
    if !rows.is_empty() {
        let _ = write!(
            report,
            "\n## SearchAddress: changes by cohort\n\n| Cohort | Cases | Changed |{}\n|---|---:|---:|{}\n",
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
    let total: usize = changed.values().map(Vec::len).sum();
    let mut summary = String::from(
        "| RPC | Checked | Pass base | Pass head | Change | Results changed |\n|---|---:|---:|---:|---:|---:|\n",
    );
    for (check, [checked, base, head, changed]) in summary_counts {
        summary += &passing(check, [base, head], checked, changed);
    }
    let countries = by_country(&country_counts);
    summary += &countries;
    let mut added_counts: BTreeMap<(&str, &str), [usize; 2]> = BTreeMap::new();
    let mut added = 0;
    for (id, record) in new.iter().filter(|(id, _)| !old.contains_key(*id)) {
        added += 1;
        let cohort = cases.get(id.as_str()).map_or("?", |c| c.cohort.as_str());
        let diagnostic = cases
            .get(id.as_str())
            .is_some_and(|c| evidence(c) == "location diagnostic");
        let mut count = |rpc, class: &str| {
            let rpc = if diagnostic { diagnostic_rpc(rpc) } else { rpc };
            if !matches!(class, "unscored" | "") {
                let counts = added_counts.entry((cohort, rpc)).or_default();
                counts[0] += usize::from(class == "pass");
                counts[1] += 1;
            }
        };
        count("SearchAddress", &record.class);
        for check in record.suggest.values() {
            count(suggest_rpc(check), suggest_class(check));
        }
        for (name, result) in located(record) {
            if let Some(result) = result {
                count(name, &result.class);
            }
        }
    }
    if added > 0 {
        let _ = write!(
            summary,
            "\n## Added cases ({added})\n\nCandidate-only results, excluded from regression and speed comparisons. Rates count checks, not independent locations. Landmark flag-off integrity and building-location/area diagnostics are separate from retrieval and identity checks.\n\n| Cohort | Check | Passed | Checked | Success rate |\n|---|---|---:|---:|---:|\n"
        );
        for ((cohort, rpc), [passed, checked]) in added_counts {
            let _ = writeln!(
                summary,
                "| {cohort} | {rpc} | {passed} | {checked} | {:.1}% |",
                100. * passed as f64 / checked as f64
            );
        }
    }
    report.insert_str(summary_at, &summary);
    if suggest_counts
        .keys()
        .any(|name| changed_checks.contains_key(name))
    {
        report += "\n## SuggestAddress\n\n| Check | Checked | Pass base | Pass head | Change | ranked_low | missing | unexpected | error |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|\n";
        for (name, c) in suggest_counts
            .iter()
            .filter(|(name, _)| changed_checks.contains_key(*name))
        {
            let _ = writeln!(
                report,
                "| {name} | {} | {} | {} | {:+} | {} | {} | {} | {} |",
                c[0],
                c[1],
                c[2],
                c[2] as i64 - c[1] as i64,
                c[3],
                c[4],
                c[5],
                c[6]
            );
        }
        report += "\n`full` types the query; `name` types the place's name alone, which must come no later than the places of that name with more population. `off` requires no landmarks with the flag false. The last four columns count the head's failures.\n";
    }
    for (name, c) in locate_counts
        .iter()
        .filter(|(name, _)| changed_checks.contains_key(*name))
    {
        let rule = match *name {
            "ReverseGeocode" => {
                "the nearest result is in the expected country, and region where the case has one, names the place (a locality's English Wikidata label, or one of the source's names for a house's town) and, at a house's point, is that house: its number, within the case's radius, and its postcode where both have one"
            }
            "ReverseGeocode offset" => {
                "the nearest result 25 m from a house's point is in the expected country, region and place, and is a house within the case's radius of it"
            }
            _ => "one of the regions found is an expected one",
        };
        let failures = &LOCATE_CLASSES[1..];
        let cells: Vec<_> = c[3..].iter().map(usize::to_string).collect();
        let _ = write!(
            report,
            "\n## {name}\n\n| Checked | Pass base | Pass head | Change | {} |\n|---:|---:|---:|---:|{}\n| {} | {} | {} | {:+} | {} |\n\nPasses when {rule}. The last {} columns count the head's failures.\n",
            failures.join(" | "),
            "---:|".repeat(failures.len()),
            c[0],
            c[1],
            c[2],
            c[2] as i64 - c[1] as i64,
            cells.join(" | "),
            failures.len()
        );
    }
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
            continue;
        }
        if open {
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
    let gained = won.iter().filter(|l| !lost.contains_key(*l)).count();
    let traded = !lost.is_empty()
        && lost.len() <= MAX_TRADED
        && lost.len() * TRADE <= gained
        && lost.values().all(|&tradable| tradable)
        && worse.is_empty()
        && !new_critical
        && !must
        && !(fail_on_drift && total > 0);
    let mut reasons = vec![];
    for (n, what) in [
        (
            if traded { 0 } else { newly_failing.len() },
            "newly failing",
        ),
        (worse.len(), "with a worse failure class"),
        (if fail_on_drift { total } else { 0 }, "with snapshot drift"),
    ] {
        if n > 0 {
            reasons.push(format!("{n} {what}"));
        }
    }
    if new_critical {
        reasons.push("new critical results".into());
    }
    if must {
        reasons.push("must-pass cases failing".into());
    }
    report += &by_evidence(&evidence_counts);
    let failed = !reasons.is_empty();
    if failed {
        let _ = writeln!(report, "\nGate: FAIL ({}).", reasons.join(", "));
    } else if traded {
        let _ = writeln!(
            report,
            "\nGate: PASS ({} newly failing OpenStreetMap-panel location{}, outweighed by {gained} newly passing; explain each in the pull request).",
            lost.len(),
            if lost.len() == 1 { "" } else { "s" }
        );
    }
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
    fn new_landmark_checks_are_visible_and_flag_off_leaks_fail_both_gates() {
        let dir = std::env::temp_dir().join(format!("eval-landmarks-{}", std::process::id()));
        for side in ["base", "head"] {
            std::fs::create_dir_all(dir.join(side)).unwrap();
            std::fs::write(dir.join(side).join("manifest.json"), "{}").unwrap();
        }
        std::fs::write(dir.join("base/results.jsonl"), "").unwrap();
        let cases = vec![Case {
            id: "airport".into(),
            cohort: "suggest_landmark".into(),
            ..Default::default()
        }];
        let airport = crate::model::Suggested {
            kind: "landmark".into(),
            category: "airport".into(),
            ..Default::default()
        };
        let mut record = Record {
            id: "airport".into(),
            query: "PNV".into(),
            class: "unscored".into(),
            suggest: [
                (
                    "full".into(),
                    SuggestCheck {
                        want: 5,
                        rank: Some(1),
                        found: vec![airport.clone()],
                        ..Default::default()
                    },
                ),
                (
                    "off".into(),
                    SuggestCheck {
                        landmarks_off: true,
                        ..Default::default()
                    },
                ),
            ]
            .into(),
            ..Default::default()
        };
        for leaks in [false, true] {
            record.suggest.get_mut("off").unwrap().found =
                if leaks { vec![airport.clone()] } else { vec![] };
            std::fs::write(
                dir.join("head/results.jsonl"),
                serde_json::to_string(&record).unwrap() + "\n",
            )
            .unwrap();
            assert_eq!(summarize(&cases, &[record.clone()], &json!({})).1, leaks);
            let out = dir.join("diff.md");
            assert_eq!(
                diff(
                    &cases,
                    &dir.join("base"),
                    &dir.join("head"),
                    Some(&out),
                    false,
                    true
                )
                .unwrap(),
                leaks
            );
            let report = std::fs::read_to_string(out).unwrap();
            assert!(
                report.contains("## New SuggestAddress checks (head only)"),
                "{report}"
            );
            assert!(report.contains("| full | 1 | 1 |"), "{report}");
            assert!(
                report.contains(&format!("| off | {} | 1 |", usize::from(!leaks))),
                "{report}"
            );
            if leaks {
                assert!(report.contains("suggest off unexpected"), "{report}");
            }
        }
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
        assert!(!report.contains("<details"), "{report}");
        assert!(
            report.contains("over 2 cases, each the fastest of 3"),
            "{report}"
        );
        assert!(report.contains("| base | 4.0 | 1.0 | 3.0 | 3.0 | 3.0 |"));
        assert!(report.trim_end().ends_with("<!-- speed end -->"));
        // 15% slower is within the limit; only more fails.
        let near = side(&[("a", Some(1.15)), ("b", Some(3.45)), ("c", None)]);
        let report = latency(&ids, &old, &near, [&three, &three]);
        assert!(report.contains("<!-- speed: pass -->"), "{report}");
        assert!(
            report.contains(
                "## Speed: no slowdown over 15%\n\n<details>\n<summary><b>Details</b></summary>"
            ),
            "{report}"
        );
        assert!(
            report.contains("</details>\n\n<!-- speed end -->"),
            "{report}"
        );
        assert!(report.find("<details>").unwrap() < report.find("### SearchAddress").unwrap());
        let report = latency(&ids, &old, &new, [&json!({}), &three]);
        assert!(report.contains("<!-- speed: skip -->"), "{report}");
        assert!(!report.contains("<details"), "{report}");
        // each RPC is its own table, and any of them fails the verdict
        let reversed = |ms: f64| -> IndexMap<String, Record> {
            [(
                "a".to_string(),
                Record {
                    ms: Some(1.),
                    reverse_ms: Some(ms),
                    ..Default::default()
                },
            )]
            .into()
        };
        let (fast, slow) = (reversed(1.), reversed(2.));
        let one = [&"a".to_string()];
        let report = latency(&one, &fast, &slow, [&three, &three]);
        assert!(report.contains("### ReverseGeocode"), "{report}");
        assert!(
            report.contains("## Speed: ReverseGeocode slower, total +100%, p50 +100%"),
            "{report}"
        );
        assert!(!report.contains("SearchAddress slower"), "{report}");
        let (slow, fast) = (
            json!({"repeat": 3, "max_rps": 20.}),
            json!({"repeat": 3, "max_rps": 1e6}),
        );
        let report = latency(&ids, &old, &new, [&slow, &fast]);
        assert!(report.contains("<!-- speed: skip -->"), "{report}");
        assert!(report.contains("--max-rps"), "{report}");
        let report = latency(&ids, &old, &new, [&fast, &fast]);
        assert!(report.contains("<!-- speed: fail -->"), "{report}");
        let sampled = json!({"repeat": 3, "max_rps": 1e6, "speed_cases": 2000});
        let report = latency(&ids, &old, &new, [&fast, &sampled]);
        assert!(report.contains("<!-- speed: skip -->"), "{report}");
        assert!(report.contains("--speed-cases"), "{report}");
        let untimed = side(&[("a", None), ("b", None), ("c", None)]);
        assert!(latency(&ids, &old, &untimed, [&json!({}), &json!({})]).is_empty());
    }

    #[test]
    fn diagnostics_and_flag_off_do_not_inflate_retrieval_rates() {
        let dir = std::env::temp_dir().join(format!("eval-scopes-{}", std::process::id()));
        for side in ["base", "head"] {
            std::fs::create_dir_all(dir.join(side)).unwrap();
            std::fs::write(dir.join(side).join("manifest.json"), "{}").unwrap();
        }
        let cases: Vec<Case> = serde_json::from_value(json!([
            {"id":"building","query":"B-12","cohort":"building","location":"building:12","source":{"scope":"building-location-diagnostic"},"expect":{"level":"street","country":["gl"]}},
            {"id":"station","query":"Station","cohort":"stations","location":"station:1","source":{"name":"operator"},"expect":{"level":"landmark","country":["hu"]}}
        ])).unwrap();
        let mut records = vec![
            json!({"id":"building","query":"B-12","class":"pass","reverse":{"class":"pass","country":"gl","regions":[]}}),
            json!({"id":"station","query":"Station","class":"unscored","suggest":{
                "full":{"text":"Station","want":5,"rank":null,"found":[]},
                "off":{"text":"Station","want":5,"rank":null,"found":[],"landmarks_off":true}
            }}),
        ];
        let lines =
            |records: &[Value]| records.iter().map(|r| format!("{r}\n")).collect::<String>();
        std::fs::write(dir.join("base/results.jsonl"), "").unwrap();
        std::fs::write(dir.join("head/results.jsonl"), lines(&records)).unwrap();
        let out = dir.join("diff.md");
        let decoded: Vec<Record> = records
            .iter()
            .cloned()
            .map(serde_json::from_value)
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        let (single, _) = summarize(&cases, &decoded, &json!({}));
        assert!(
            single.contains("**Building-location diagnostics:**"),
            "{single}"
        );
        assert!(
            !diff(
                &cases,
                &dir.join("base"),
                &dir.join("head"),
                Some(&out),
                false,
                true
            )
            .unwrap()
        );
        let report = std::fs::read_to_string(&out).unwrap();
        for row in [
            "| building | SearchAddress (location only) | 1 | 1 | 100.0% |",
            "| building | ReverseGeocode (country/area only) | 1 | 1 | 100.0% |",
            "| stations | SuggestAddress | 0 | 1 | 0.0% |",
            "| stations | SuggestAddress (flag off) | 1 | 1 | 100.0% |",
        ] {
            assert!(report.contains(row), "{report}");
        }
        assert!(
            !diff(
                &cases,
                &dir.join("head"),
                &dir.join("head"),
                Some(&out),
                false,
                true
            )
            .unwrap()
        );
        let report = std::fs::read_to_string(&out).unwrap();
        assert!(
            report.contains("| SuggestAddress | 1 | 0 | 0 | 0 | 0 |"),
            "{report}"
        );
        assert!(
            report.contains("| SuggestAddress (flag off) | 1 | 1 | 1 | 0 | 0 |"),
            "{report}"
        );
        assert!(
            report.contains("Independent sources (1 country)"),
            "{report}"
        );
        assert!(
            report.contains("Building-location diagnostics (1 country)"),
            "{report}"
        );
        assert!(
            report.contains("| Hungary | landmarks | - | 0/1 · 0.0% | - | - |"),
            "{report}"
        );
        std::fs::write(dir.join("base/results.jsonl"), lines(&records)).unwrap();
        records[1]["suggest"]["off"]["found"] =
            json!([{"kind":"landmark","name":"Station","country":"hu","point":[47.5,19.05]}]);
        std::fs::write(dir.join("head/results.jsonl"), lines(&records)).unwrap();
        assert!(
            diff(
                &cases,
                &dir.join("base"),
                &dir.join("head"),
                Some(&out),
                false,
                true
            )
            .unwrap()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn added_cases_report_success_rates_without_changing_the_comparison() {
        let dir = std::env::temp_dir().join(format!("eval-added-{}", std::process::id()));
        let mut cases: Vec<_> = ["shared", "added-pass", "added-fail", "unscored"]
            .into_iter()
            .map(|id| Case {
                id: id.into(),
                cohort: if id == "unscored" {
                    "unscored"
                } else {
                    "fr_house"
                }
                .into(),
                ..Default::default()
            })
            .collect();
        cases.push(Case {
            id: "suggest-only".into(),
            cohort: "suggest_dk_floor".into(),
            suggest_address: Some(crate::model::SuggestAddressExpect {
                building: "Test Road 1".into(),
                unverified_suffix: "4. th.".into(),
                country_code: None,
            }),
            ..Default::default()
        });
        let record = |id: &str, class: &str| Record {
            id: id.into(),
            class: class.into(),
            ..Default::default()
        };
        let check = |rank, error| SuggestCheck {
            want: 1,
            rank,
            error,
            ..Default::default()
        };
        let located = |class: &str| Located {
            class: class.into(),
            ..Default::default()
        };
        let mut passed = record("added-pass", "pass");
        passed.suggest = [
            ("name".into(), check(Some(1), None)),
            ("full".into(), check(None, None)),
        ]
        .into();
        passed.reverse = Some(located("pass"));
        passed.region = Some(located("error"));
        let mut address = record("suggest-only", "unscored");
        address.suggest = [("full".into(), check(Some(1), None))].into();
        let mut failed = record("added-fail", "wrong_country");
        failed.suggest = [("full".into(), check(None, Some("timeout".into())))].into();
        failed.reverse = Some(located("wrong_country"));
        for (side, records) in [
            (
                "old",
                vec![record("shared", "pass"), record("removed", "pass")],
            ),
            (
                "new",
                vec![
                    record("shared", "pass"),
                    passed,
                    failed,
                    record("unscored", "unscored"),
                    address,
                ],
            ),
        ] {
            std::fs::create_dir_all(dir.join(side)).unwrap();
            std::fs::write(dir.join(side).join("manifest.json"), "{}").unwrap();
            let lines: Vec<_> = records
                .iter()
                .map(|r| serde_json::to_string(r).unwrap())
                .collect();
            std::fs::write(dir.join(side).join("results.jsonl"), lines.join("\n")).unwrap();
        }
        let out = dir.join("diff.md");
        assert!(
            !diff(
                &cases,
                &dir.join("old"),
                &dir.join("new"),
                Some(&out),
                false,
                true
            )
            .unwrap()
        );
        let report = std::fs::read_to_string(&out).unwrap();
        assert!(
            report.contains("| SearchAddress | 1 | 1 | 1 | 0 | 0 |"),
            "{report}"
        );
        assert!(report.contains("## Added cases (4)"), "{report}");
        for row in [
            "| fr_house | SearchAddress | 1 | 2 | 50.0% |",
            "| fr_house | SuggestAddress | 1 | 3 | 33.3% |",
            "| fr_house | ReverseGeocode | 1 | 2 | 50.0% |",
            "| fr_house | ResolveRegion | 0 | 1 | 0.0% |",
            "| suggest_dk_floor | SuggestAddress | 1 | 1 | 100.0% |",
        ] {
            assert!(report.contains(row), "{report}");
        }
        assert!(!report.contains("| unscored |"), "{report}");
        assert!(
            !report.contains("| suggest_dk_floor | SearchAddress |"),
            "{report}"
        );
        assert!(!report.contains("NaN"), "{report}");
        assert!(report.find("## Added cases").unwrap() < report.find("<summary><b>Runs").unwrap());
        assert!(
            !diff(
                &cases,
                &dir.join("new"),
                &dir.join("new"),
                Some(&out),
                false,
                true
            )
            .unwrap()
        );
        let report = std::fs::read_to_string(out).unwrap();
        assert!(!report.contains("## Added cases"), "{report}");
        assert!(
            report.contains("| SearchAddress | 4 | 2 | 2 | 0 | 0 |"),
            "{report}"
        );
        std::fs::remove_dir_all(dir).unwrap();
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
    fn a_few_osm_losses_pass_when_far_more_locations_are_won() {
        let dir = std::env::temp_dir().join(format!("eval-trade-{}", std::process::id()));
        // (lost OSM locations, lost registry locations, won locations, passes)
        for (lost, registry, won, passes) in [
            (1, 0, 5, true),
            (1, 0, 4, false),
            (0, 1, 10, false),
            (6, 0, 30, false),
            (5, 0, 25, true),
        ] {
            let (mut cases, mut old, mut new) = (vec![], vec![], vec![]);
            for (n, (osm, before, after)) in std::iter::repeat_n((true, "pass", "near_miss"), lost)
                .chain(std::iter::repeat_n((false, "pass", "near_miss"), registry))
                .chain(std::iter::repeat_n((true, "abstain", "pass"), won))
                .enumerate()
            {
                // two forms of each location
                for form in 0..2 {
                    let id = format!("l{n}-{form}");
                    let source = if osm {
                        "openstreetmap-address-node"
                    } else {
                        "ban"
                    };
                    cases.push(Case {
                        id: id.clone(),
                        location: format!("l{n}"),
                        source: Some(json!({"name": source})),
                        ..Default::default()
                    });
                    for (side, class) in [(&mut old, before), (&mut new, after)] {
                        let record = Record {
                            id: id.clone(),
                            class: class.into(),
                            ..Default::default()
                        };
                        side.push(serde_json::to_string(&record).unwrap());
                    }
                }
            }
            for (side, lines) in [("old", &old), ("new", &new)] {
                std::fs::create_dir_all(dir.join(side)).unwrap();
                std::fs::write(dir.join(side).join("manifest.json"), "{}").unwrap();
                std::fs::write(dir.join(side).join("results.jsonl"), lines.join("\n")).unwrap();
            }
            let (a, b, out) = (dir.join("old"), dir.join("new"), dir.join("diff.md"));
            let failed = diff(&cases, &a, &b, Some(&out), false, true).unwrap();
            let report = std::fs::read_to_string(&out).unwrap();
            assert_eq!(failed, !passes, "{lost} {registry} {won}: {report}");
            let n = lost + registry;
            let line = if passes {
                format!(
                    "Gate: PASS ({n} newly failing OpenStreetMap-panel location{}, outweighed by {won} newly passing",
                    if n == 1 { "" } else { "s" }
                )
            } else {
                format!("Gate: FAIL ({} newly failing", 2 * n)
            };
            assert!(report.contains(&line), "{line}: {report}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_reverse_or_region_answer_that_gets_worse_fails_the_diff() {
        let dir = std::env::temp_dir().join(format!("eval-locate-{}", std::process::id()));
        let cases = [Case {
            id: "a".into(),
            ..Default::default()
        }];
        let found = |class: &str, country: &str| Located {
            class: class.into(),
            country: country.into(),
            regions: vec![],
            places: None,
            answer: None,
            error: None,
        };
        for (side, reverse, region) in [
            ("old", found("pass", "dk"), found("pass", "dk")),
            ("new", found("wrong_country", "se"), found("pass", "dk")),
        ] {
            let record = Record {
                id: "a".into(),
                class: "pass".into(),
                reverse: Some(reverse),
                region: Some(region),
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
            report.contains("`a` ReverseGeocode pass -> wrong_country"),
            "{report}"
        );
        assert!(!report.contains("ResolveRegion pass ->"), "{report}");
        assert!(
            report.contains("## ReverseGeocode\n\n| Checked | Pass base | Pass head | Change | no_house | empty | wrong_postcode | wrong_house | wrong_place | wrong_region | wrong_country | error |\n|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n| 1 | 1 | 0 | -1 | 0 | 0 | 0 | 0 | 0 | 0 | 1 | 0 |\n"),
            "{report}"
        );
        assert!(!report.contains("## ResolveRegion"), "{report}");
        assert!(
            report.contains("| ResolveRegion | 1 | 1 | 1 | 0 | 0 |\n| ReverseGeocode | 1 | 1 | 0 | -1 (-100.0%) | 1 |\n"),
            "{report}"
        );
        assert!(report.contains("Gate: FAIL (1 newly failing)."), "{report}");
        assert!(!diff(&cases, &old, &old, Some(&out), false, true).unwrap());
        let report = std::fs::read_to_string(&out).unwrap();
        assert!(!report.contains("## Reverse"), "{report}");
        assert!(!report.contains("Gate:"), "{report}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn offsetting_changes_still_show_check_tables() {
        let dir = std::env::temp_dir().join(format!("eval-offsetting-{}", std::process::id()));
        let cases: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|id| Case {
                id: id.into(),
                ..Default::default()
            })
            .collect();
        for (side, ranks) in [("old", [1, 2]), ("new", [2, 1])] {
            std::fs::create_dir_all(dir.join(side)).unwrap();
            std::fs::write(dir.join(side).join("manifest.json"), "{}").unwrap();
            let records = cases
                .iter()
                .zip(ranks)
                .map(|(case, rank)| {
                    let located = Located {
                        class: if rank == 1 { "pass" } else { "wrong_region" }.into(),
                        ..Default::default()
                    };
                    let record = Record {
                        id: case.id.clone(),
                        class: "pass".into(),
                        suggest: [(
                            "full".into(),
                            SuggestCheck {
                                want: 1,
                                rank: Some(rank),
                                ..Default::default()
                            },
                        )]
                        .into(),
                        reverse: Some(located.clone()),
                        region: Some(located),
                        ..Default::default()
                    };
                    serde_json::to_string(&record).unwrap() + "\n"
                })
                .collect::<String>();
            std::fs::write(dir.join(side).join("results.jsonl"), records).unwrap();
        }
        let out = dir.join("diff.md");
        assert!(
            diff(
                &cases,
                &dir.join("old"),
                &dir.join("new"),
                Some(&out),
                false,
                true
            )
            .unwrap()
        );
        let report = std::fs::read_to_string(out).unwrap();
        assert!(report.contains("| full | 2 | 1 | 1 | +0 |"), "{report}");
        for name in ["ReverseGeocode", "ResolveRegion"] {
            assert!(
                report.contains(&format!("## {name}\n\n| Checked | Pass base | Pass head | Change | no_house | empty | wrong_postcode | wrong_house | wrong_place | wrong_region | wrong_country | error |\n|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n| 2 | 1 | 1 | +0 |")),
                "{report}"
            );
            assert!(
                report.contains(&format!("| {name} | 2 | 1 | 1 | 0 | 2 |")),
                "{report}"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn countries_show_passing_checks_where_any_changed() {
        let mut counts: BTreeMap<String, BTreeMap<&str, [usize; 3]>> = BTreeMap::new();
        for (country, rpc, c) in [
            ("dk", "SearchAddress", [1883, 1779, 1867]),
            ("dk", "SuggestAddress", [1883, 293, 1838]),
            ("dk", "ReverseGeocode", [1883, 1882, 1882]),
            ("fr", "SearchAddress", [505, 498, 498]),
            ("no", "SearchAddress", [714, 709, 705]),
            ("be,fr", "ResolveRegion", [4, 2, 3]),
        ] {
            counts.entry(country.into()).or_default().insert(rpc, c);
        }
        let table = by_country(&counts);
        assert!(table.contains("for the 3 expected countries where any RPC changed"));
        // the largest change first, by English name, an RPC nobody asked about as a dash, an
        // unchanged one without an arrow, and a country that did not change not at all
        assert!(
            table.contains(
                "| Denmark | 1779 → 1867 / 1883 (+88) | 94.5% → 99.2% | 293 → 1838 / 1883 (+1545) | 15.6% → 97.6% | 1882 / 1883 | 99.9% | - | - |\n| Norway | 709 → 705 / 714 (-4) | 99.3% → 98.7% | - | - | - | - | - | - |\n| Belgium, France | - | - | - | - | - | - | 2 → 3 / 4 (+1) | 50.0% → 75.0% |\n"
            ),
            "{table}"
        );
        assert!(!table.contains("| France |"), "{table}");
        counts.retain(|country, _| country == "fr");
        assert!(by_country(&counts).contains("No expected country's passing checks changed."),);
        assert_eq!(by_country(&BTreeMap::new()), "");
    }

    #[test]
    fn every_country_is_split_by_evidence_with_a_sum_first() {
        let case = |cohort: &str, source: Option<&str>| Case {
            cohort: cohort.into(),
            source: source.map(|name| serde_json::json!({ "name": name })),
            ..Default::default()
        };
        assert_eq!(
            evidence(&case("global_osm_ke", Some("openstreetmap-address-way"))),
            "osm"
        );
        assert_eq!(evidence(&case("town", Some("wikidata"))), "towns");
        assert_eq!(evidence(&case("gleif", Some("gleif"))), "gleif");
        assert_eq!(evidence(&case("city", Some("wikidata"))), "places");
        assert_eq!(evidence(&case("dk", Some("dar-json"))), "registry");
        assert_eq!(evidence(&case("malformed", None)), "constructed");
        let mut counts = Evidence::default();
        let mut add = |osm: bool, country: &str, kind: &'static str, rpc: &'static str, c| {
            let rows = if osm {
                &mut counts.osm
            } else {
                &mut counts.independent
            };
            let row = rows.entry(country.into()).or_default();
            row.0.insert(kind);
            row.1.insert(rpc, c);
        };
        add(false, "dk", "registry", "SearchAddress", [100, 90, 95]);
        add(false, "dk", "places", "SearchAddress", [100, 90, 95]);
        add(false, "ke", "towns", "SearchAddress", [10, 5, 5]);
        add(true, "ke", "osm", "SearchAddress", [100, 75, 75]);
        let text = by_evidence(&counts);
        assert!(text.starts_with("\n<!-- by-evidence -->\n"), "{text}");
        assert!(text.ends_with("<!-- by-evidence end -->\n"), "{text}");
        assert!(text.contains("Registers can also supply index data"));
        assert!(!text.contains("none of them the index's own data"));
        assert!(
            text.contains("<summary><b>Independent sources (2 countries)</b></summary>\n\n| Country | Evidence | SearchAddress | SuggestAddress | ReverseGeocode | ResolveRegion |\n|---|---|---:|---:|---:|---:|\n| **All** | | 95→100/110 · 86.4%→90.9% | - | - | - |\n| Denmark | places, registry | 90→95/100 · 90.0%→95.0% | - | - | - |\n| Kenya | towns | 5/10 · 50.0% | - | - | - |\n"),
            "{text}"
        );
        assert!(
            text.contains("<summary><b>Agreement with OpenStreetMap (1 country)</b></summary>\n\n| Country | SearchAddress | SuggestAddress | ReverseGeocode | ResolveRegion |\n|---|---:|---:|---:|---:|\n| **All** | 75/100 · 75.0% | - | - | - |\n| Kenya | 75/100 · 75.0% | - | - | - |\n"),
            "{text}"
        );
        assert_eq!(by_evidence(&Evidence::default()), "");
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
            report.contains("\n| RPC | Checked | Pass base | Pass head | Change | Results changed |\n|---|---:|---:|---:|---:|---:|\n| SearchAddress | 1 | 1 | 1 | 0 | 0 |\n| SuggestAddress | 1 | 1 | 0 | -1 (-100.0%) | 1 |\n"),
            "{report}"
        );
        assert_eq!(
            passing("SearchAddress", [3340, 3342], 3759, 5),
            "| SearchAddress | 3759 | 3340 | 3342 | +2 (+0.1%) | 5 |\n"
        );
        let (a, b) = (load_side(&old).unwrap().1, load_side(&new).unwrap().1);
        let ids: Vec<_> = a.keys().collect();
        let speed = latency(&ids, &a, &b, [&json!({}), &json!({})]);
        assert!(speed.contains("### SuggestAddress"), "{speed}");
        assert!(speed.contains("over 1 checks"), "{speed}");
        assert!(
            speed.contains("<!-- speed: fail -->\n## Speed: SuggestAddress slower, total +100%"),
            "{speed}"
        );
        assert!(!diff(&cases, &old, &old, Some(&out), false, true).unwrap());
        let report = std::fs::read_to_string(&out).unwrap();
        assert!(!report.contains("| Check |"), "{report}");
        assert!(
            !report.contains("\n## Sugg") && !report.contains("Gate:"),
            "{report}"
        );
        assert!(!report.contains("| Cohort") && !report.contains("changed cases"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
