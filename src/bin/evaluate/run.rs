use crate::{
    model::{Case, DirectRaw, Located, Record, SuggestCheck, root},
    report::{diff, sorted, summarize},
    score::{
        Expect, SUGGEST_LIMIT, details, expected_place, from_index, offset_point, rank,
        region_applies, region_class, region_codes, region_flags, resolved, reverse_applies,
        reverse_class, reversed, score, suggest_rank, suggested, suggested_address, summary_of,
        text,
    },
};
use anyhow::{Context, Result, bail};
use axum::http::header;
use buffa::Message;
use opengeocoding::{
    index::{Index, Parser},
    proto::opengeocoding::{FullAddress, SearchAddressTask},
};
use reqwest::Client;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant, SystemTime},
};
use tokio::process::{Child, Command};

const SERVICE: &str = "opengeocoding.OpenGeocoding";

pub struct Options {
    pub suite: String,
    pub corpus: Option<PathBuf>,
    pub out: PathBuf,
    pub index: Option<PathBuf>,
    pub rpc: Option<String>,
    pub only: Option<regex::Regex>,
    pub max_rps: f64,
    pub budget: usize,
    pub direct: bool,
    pub resume: bool,
    pub backend_label: String,
    pub compare: Option<PathBuf>,
    pub fail_on_drift: bool,
    pub repeat: usize,
    /// Cases timed one at a time; the rest run `workers` at a time, untimed.
    pub speed_cases: usize,
    pub workers: usize,
}

/// One rate limit and request budget for the RPCs sent to the service.
struct Budget {
    interval: Duration,
    limit: usize,
    used: AtomicUsize,
    next: tokio::sync::Mutex<tokio::time::Instant>,
}

impl Budget {
    async fn take(&self) {
        let start = {
            let mut next = self.next.lock().await;
            let start = (*next).max(tokio::time::Instant::now());
            *next = start + self.interval;
            start
        };
        self.used.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep_until(start).await;
    }

    fn used(&self) -> usize {
        self.used.load(Ordering::SeqCst)
    }
}

/// Index searches the local service has made so far, from its metrics.
async fn index_searches(client: &Client, metrics: &str) -> Option<f64> {
    let text = client
        .get(format!("{metrics}/metrics"))
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    Some(searches_in(&text))
}

/// Builds from before the gauge report none and count as warm.
fn warming(metrics: &str) -> bool {
    metrics
        .lines()
        .any(|l| l == "opengeocoding_suggest_caches_warm 0")
}

fn searches_in(metrics: &str) -> f64 {
    metrics
        .lines()
        .filter(|l| l.starts_with("nominatim_geolocation_search_candidates_seconds_count"))
        .filter_map(|l| l.rsplit(' ').next()?.parse::<f64>().ok())
        .sum()
}

/// Ok(body) or Err(Connect error code / transport failure).
async fn rpc(
    client: &Client,
    base: &str,
    method: &str,
    body: Vec<u8>,
    proto: bool,
) -> Result<Vec<u8>, String> {
    let response = client
        .post(format!("{base}/{SERVICE}/{method}"))
        .header(
            header::CONTENT_TYPE,
            if proto {
                "application/proto"
            } else {
                "application/json"
            },
        )
        .header("Connect-Protocol-Version", "1")
        .header("x-organization-id", "opengeocoding-eval")
        .timeout(Duration::from_secs(120))
        .body(body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                "timeout".into()
            } else {
                "connection_error".to_string()
            }
        })?;
    let status = response.status();
    let body = response.bytes().await.map_err(|e| {
        if e.is_timeout() {
            "timeout".into()
        } else {
            "connection_error".to_string()
        }
    })?;
    if status.is_success() {
        return Ok(body.to_vec());
    }
    let code = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|v| v["code"].as_str().map(String::from));
    Err(code.unwrap_or_else(|| format!("http_{}", status.as_u16())))
}

pub async fn rpc_json(
    client: &Client,
    base: &str,
    method: &str,
    payload: &Value,
) -> Result<Value, String> {
    let body = rpc(
        client,
        base,
        method,
        payload.to_string().into_bytes(),
        false,
    )
    .await?;
    serde_json::from_slice(&body).map_err(|_| "invalid_json".into())
}

/// Cases a completed text should be suggested for: not the damaged addresses of `degraded`,
/// whose repair is SearchAddress's job, nor those expecting no place or an error.
fn suggestible(case: &Case) -> bool {
    case.cohort != "degraded"
        && case.expect.as_ref().is_some_and(|e| {
            e.error.is_none() && e.alternatives().iter().all(|a| a.level != "none")
        })
}

fn suggestion_rank(case: &Case, check: &SuggestCheck) -> Option<usize> {
    let expect = case.expect.as_ref()?;
    let Some(address) = &case.suggest_address else {
        return suggest_rank(expect, &check.found);
    };
    let building = check.building.as_ref()?;
    if check.error.is_some() || building.id.is_empty() || !building.unverified_suffix.is_empty() {
        return None;
    }
    check
        .found
        .iter()
        .position(|s| {
            s.id == building.id
                && s.point == building.point
                && s.kind == building.kind
                && s.country == building.country
                && s.unverified_suffix == address.unverified_suffix
                && suggest_rank(expect, std::slice::from_ref(s)).is_some()
        })
        .map(|i| i + 1)
}

/// Re-ranks saved suggestions against the current corpus.
pub fn rescore_suggest(case: &Case, record: &mut Record) {
    if case.expect.is_none() {
        return;
    }
    for (name, check) in &mut record.suggest {
        if !check.landmarks_off {
            check.rank = suggestion_rank(case, check);
        }
        if let ("name", Some(s)) = (name.as_str(), &case.suggest) {
            check.want = s.rank;
        }
    }
}

/// A ReverseGeocode outcome before its answer is read: `answer` is set even without one, so a
/// diff tells it from an older run's record, which has none.
fn unanswered() -> Located {
    Located {
        answer: Some(Default::default()),
        ..Default::default()
    }
}

/// The English Wikidata label of a case's place, which `build-gold` sets as its suggest text.
pub fn place_name(case: &Case) -> Option<&str> {
    case.suggest.as_ref().map(|s| s.text.as_str())
}

/// Re-scores saved reverse and region answers against the current corpus.
pub fn rescore_locate(case: &Case, record: &mut Record) {
    let Some(full) = case.expect.as_ref() else {
        return;
    };
    let Some(expect) = expected_place(full) else {
        return;
    };
    for (found, offset) in [
        (&mut record.reverse, false),
        (&mut record.reverse_offset, true),
    ] {
        if let Some(found) = found {
            found.class = reverse_class(full, place_name(case), found, offset).into();
        }
    }
    if let Some(found) = &mut record.region {
        found.class = region_class(expect, found).into();
    }
}

/// Scores an RPC outcome against a case; also used to re-score saved responses.
pub fn classify(case: &Case, outcome: Result<&Value, &str>, record: &mut Record, confidence: bool) {
    let wants_error = case.expect.as_ref().and_then(|e| e.error.as_deref());
    record.flags.clear();
    match outcome {
        Err(error) => {
            record.error = Some(error.into());
            record.class = if wants_error == Some(error) {
                "pass"
            } else {
                "error"
            }
            .into();
        }
        Ok(_) if wants_error.is_some() => record.class = "unexpected_result".into(),
        Ok(response) => {
            let scored = score(case.expect.as_ref(), response, confidence);
            record.class = scored.class.into();
            record.level = Some(scored.level.into());
            record.distance_km = scored.distance_km;
            record.flags = scored.flags;
            if confidence {
                record.flags.extend(region_flags(response));
            }
        }
    }
}

struct Checker {
    client: Client,
    base: String,
    budget: Budget,
    /// The local service's metrics endpoint, to count its index searches per case.
    metrics: Option<String>,
    /// The same index, loaded here for direct checks.
    index: Option<Index>,
    /// Requests per timed case; `ms` is the fastest. Only for the local service, so outside the budget.
    repeat: usize,
}

impl Checker {
    /// Untimed cases run alongside others: one request per check and no `ms`, since concurrent
    /// requests would time each other.
    async fn check(&self, case: &Case, timed: bool) -> Result<Record> {
        let mut record = self.answer(case, timed).await?;
        if !timed {
            record.ms = None;
            record.reverse_ms = None;
            for check in record.suggest.values_mut() {
                check.ms = None;
            }
        }
        Ok(record)
    }

    async fn answer(&self, case: &Case, timed: bool) -> Result<Record> {
        let repeat = if timed { self.repeat } else { 1 };
        let mut record = Record {
            id: case.id.clone(),
            query: case.query.clone(),
            ..Default::default()
        };
        if case.suggest_address.is_some()
            || case.expect.as_ref().is_some_and(|e| e.level == "landmark")
        {
            record.class = "unscored".into();
            record.suggest = self.suggest(case, repeat).await;
            return Ok(record);
        }
        let metrics = self.metrics.as_ref().filter(|_| timed);
        let before = match metrics {
            Some(metrics) => index_searches(&self.client, metrics).await,
            None => None,
        };
        self.budget.take().await;
        let started = Instant::now();
        let result = rpc_json(
            &self.client,
            &self.base,
            "SearchAddress",
            &json!({ "addressString": case.query }),
        )
        .await;
        let mut elapsed = started.elapsed();
        if let (Some(metrics), Some(before)) = (metrics, before)
            && let Some(after) = index_searches(&self.client, metrics).await
        {
            record.backend_calls = Some((after - before).round() as usize);
        }
        for _ in 1..repeat {
            let started = Instant::now();
            let _ = rpc_json(
                &self.client,
                &self.base,
                "SearchAddress",
                &json!({ "addressString": case.query }),
            )
            .await;
            elapsed = elapsed.min(started.elapsed());
        }
        record.ms = Some((elapsed.as_secs_f64() * 1e6).round() / 1e3);
        record.suggest = self.suggest(case, repeat).await;
        (record.reverse_ms, record.reverse) = self.reverse(case, repeat).await;
        record.reverse_offset = self.reverse_offset(case).await;
        record.region = self.region(case).await;
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                classify(case, Err(&error), &mut record, true);
                return Ok(record);
            }
        };
        classify(case, Ok(&response), &mut record, true);
        record.summary = Some(summary_of(&response));
        record.response = Some(response);
        match (&case.expect, &self.index) {
            (Some(expect), Some(index)) if expect.error.is_none() => {
                self.attribute(index, case, expect, &mut record).await
            }
            _ => {}
        }
        Ok(record)
    }

    /// ReverseGeocode at the case's single expected point (a house where the case is one,
    /// else the nearest locality): its time, and what it answered where the case checks that.
    async fn reverse(&self, case: &Case, repeat: usize) -> (Option<f64>, Option<Located>) {
        let Some((full, expect)) = case
            .expect
            .as_ref()
            .and_then(|full| Some((full, expected_place(full)?)))
        else {
            return (None, None);
        };
        let [lat, lon] = expect.point.unwrap_or_default();
        self.budget.take().await;
        let request = json!({ "lat": lat, "lon": lon });
        let (mut fastest, mut found) = (None::<Duration>, unanswered());
        for _ in 0..repeat.max(1) {
            let started = Instant::now();
            match rpc_json(&self.client, &self.base, "ReverseGeocode", &request).await {
                Ok(body) => found = reversed(&body),
                Err(error) => {
                    found.error = Some(error);
                    break;
                }
            }
            let took = started.elapsed();
            fastest = Some(fastest.map_or(took, |f| f.min(took)));
        }
        let ms = fastest.map(|f| (f.as_secs_f64() * 1e6).round() / 1e3);
        found.class = reverse_class(full, place_name(case), &found, false).into();
        (ms, reverse_applies(expect).then_some(found))
    }

    /// ReverseGeocode `OFFSET_M` from a house case's point, untimed.
    async fn reverse_offset(&self, case: &Case) -> Option<Located> {
        let full = case.expect.as_ref()?;
        let [lat, lon] = offset_point(full, &case.location)?;
        self.budget.take().await;
        let request = json!({ "lat": lat, "lon": lon });
        let mut found = match rpc_json(&self.client, &self.base, "ReverseGeocode", &request).await {
            Ok(body) => reversed(&body),
            Err(error) => Located {
                error: Some(error),
                ..unanswered()
            },
        };
        found.class = reverse_class(full, place_name(case), &found, true).into();
        Some(found)
    }

    /// ResolveRegion at the point of a place in a known region.
    async fn region(&self, case: &Case) -> Option<Located> {
        let expect = case
            .expect
            .as_ref()
            .and_then(expected_place)
            .filter(|e| region_applies(e))?;
        let [lat, lon] = expect.point?;
        self.budget.take().await;
        let mut found = match rpc_json(
            &self.client,
            &self.base,
            "ResolveRegion",
            &json!({ "lat": lat, "lon": lon }),
        )
        .await
        {
            Ok(body) => resolved(&body),
            Err(error) => Located {
                error: Some(error),
                ..Default::default()
            },
        };
        found.class = region_class(expect, &found).into();
        Some(found)
    }

    /// The query, and the place's name alone when the corpus ranks it among its namesakes, typed
    /// into SuggestAddress.
    async fn suggest(&self, case: &Case, repeat: usize) -> BTreeMap<String, SuggestCheck> {
        let mut checks = BTreeMap::new();
        let Some(expect) = case.expect.as_ref().filter(|_| suggestible(case)) else {
            return checks;
        };
        let mut building = None;
        let mut building_error = None;
        if let Some(address) = &case.suggest_address {
            let mut request = json!({ "text": address.building, "limit": SUGGEST_LIMIT });
            if let Some(cc) = &address.country_code {
                request["countryCode"] = json!(cc);
            }
            self.budget.take().await;
            match rpc_json(&self.client, &self.base, "SuggestAddress", &request).await {
                Ok(body) => {
                    let found = suggested_address(&body);
                    building = suggest_rank(expect, &found).map(|rank| found[rank - 1].clone());
                }
                Err(error) => building_error = Some(error),
            }
        }
        let mut texts = vec![("full", case.query.clone(), SUGGEST_LIMIT)];
        texts.extend(
            case.suggest
                .as_ref()
                .map(|s| ("name", s.text.clone(), s.rank)),
        );
        if case.include_landmarks && expect.level == "landmark" {
            texts.push(("off", case.query.clone(), SUGGEST_LIMIT));
        }
        for (name, text, want) in texts {
            self.budget.take().await;
            let mut check = SuggestCheck {
                text: text.clone(),
                want,
                landmarks_off: name == "off",
                building: building.clone(),
                error: building_error.clone(),
                ..Default::default()
            };
            let mut request = json!({ "text": text, "limit": SUGGEST_LIMIT });
            if let Some(cc) = case
                .suggest_address
                .as_ref()
                .and_then(|s| s.country_code.as_ref())
            {
                request["countryCode"] = json!(cc);
            }
            if case.include_landmarks {
                request["includeLandmarks"] = json!(!check.landmarks_off);
            }
            let mut elapsed = None::<std::time::Duration>;
            for _ in 0..repeat.max(1) {
                let started = Instant::now();
                let result = rpc_json(&self.client, &self.base, "SuggestAddress", &request).await;
                let took = started.elapsed();
                elapsed = Some(elapsed.map_or(took, |e| e.min(took)));
                match result {
                    Ok(body) => {
                        check.found = if case.suggest_address.is_some() {
                            suggested_address(&body)
                        } else {
                            suggested(&body)
                        }
                    }
                    Err(error) => check.error = Some(error),
                }
            }
            check.ms = elapsed.map(|e| (e.as_secs_f64() * 1e6).round() / 1e3);
            if !check.landmarks_off {
                check.rank = suggestion_rank(case, &check);
            }
            checks.insert(name.into(), check);
        }
        checks
    }

    /// Direct backend checks attribute a failure to the backend or to the consumer.
    async fn attribute(&self, index: &Index, case: &Case, expect: &Expect, record: &mut Record) {
        let search = |q: &str| -> Vec<Value> {
            index
                .search(q)
                .iter()
                .map(|h| index.render(h, true))
                .collect()
        };
        let raw = search(case.query.trim());
        let scored: Vec<_> = raw
            .iter()
            .map(|hit| score(Some(expect), &from_index(hit), false))
            .collect();
        record.direct_raw = Some(DirectRaw {
            count: raw.len(),
            top1: scored.first().map_or("abstain", |s| s.class).into(),
            top1_name: raw.first().map_or("", |h| text(h, "display_name")).into(),
            best_rank: scored.iter().position(|s| s.class == "pass").map(|i| i + 1),
        });
        let single = expect.any_of.is_none().then_some(expect);
        if let Some(alt) = single
            && let (Some(road), Some(house)) = (
                alt.components.get("road"),
                alt.components.get("house_number"),
            )
        {
            // Nominatim's structured street and postalcode, written in order
            let structured = [
                Some(format!("{house} {road}")),
                alt.components.get("postcode").cloned(),
            ]
            .into_iter()
            .flatten()
            .filter(|p| !p.trim().is_empty())
            .map(|p| p.trim().to_string())
            .collect::<Vec<_>>()
            .join(", ");
            let hits = search(&structured);
            record.direct_structured = Some(
                hits.first()
                    .map_or("abstain", |hit| {
                        score(Some(expect), &from_index(hit), false).class
                    })
                    .into(),
            );
        }
        if let Some(alt) = single
            && let Some([lat, lon]) = alt.point
            && !alt.region.is_empty()
            && rank(&alt.level) >= rank("region")
        {
            let hit = index.reverse_region(lat, lon);
            let reverse = if hit.get("address").is_some() {
                from_index(&hit)
            } else {
                json!({})
            };
            let codes = region_codes(&reverse);
            record.direct_reverse = Some(json!({
                "country_ok": alt.country.contains(&text(details(&reverse), "countryCode").to_lowercase()),
                "region_ok": alt.region.iter().any(|r| codes.contains(r)),
            }));
            self.budget.take().await;
            record.rpc_resolve_region = Some(
                match rpc_json(
                    &self.client,
                    &self.base,
                    "ResolveRegion",
                    &json!({ "lat": lat, "lon": lon }),
                )
                .await
                {
                    Ok(body) => {
                        let mut found: Vec<_> = body["regions"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|r| {
                                format!(
                                    "{}-{}",
                                    text(r, "countryCode").to_uppercase(),
                                    text(r, "adminCode")
                                )
                            })
                            .collect();
                        found.sort();
                        json!({ "ok": alt.region.iter().any(|r| found.contains(r)), "codes": found })
                    }
                    Err(error) => json!({ "ok": false, "error": error }),
                },
            );
        }
        if let Some(alt) = single
            && let Some(level) = alt.admin_level
            && let Some(first) = raw.first()
        {
            let info = index.details(first["place_id"].as_u64().unwrap_or(u64::MAX));
            record.direct_details = Some(json!({
                "admin_level": info["admin_level"],
                "ok": info["admin_level"].as_u64() == Some(level)
                    && alt.country.contains(&text(&info, "country_code").to_lowercase()),
            }));
        }
        if record.class != "pass" && record.class != "unscored" {
            let direct = record.direct_raw.as_ref().expect("direct_raw");
            record.attribution_hint = Some(
                if direct.best_rank.is_some() {
                    "consumer: backend returned an acceptable raw candidate"
                } else if record.direct_structured.as_deref() == Some("pass") {
                    "consumer/parser: structured backend query succeeds"
                } else if raw.is_empty() {
                    "backend/data or query: no raw candidates"
                } else {
                    "backend/data or query: no acceptable raw candidate in top 10"
                }
                .into(),
            );
        }
    }

    async fn transport(&self, cases: &[&Case], grpcurl: tokio::task::JoinHandle<Value>) -> Value {
        let mut mismatches = vec![];
        let checked = cases.iter().take(5).collect::<Vec<_>>();
        for case in &checked {
            let json_result = rpc_json(
                &self.client,
                &self.base,
                "SearchAddress",
                &json!({ "addressString": case.query }),
            )
            .await;
            let task = SearchAddressTask {
                address_string: case.query.clone(),
                ..Default::default()
            };
            let proto_result = rpc(
                &self.client,
                &self.base,
                "SearchAddress",
                task.encode_to_vec(),
                true,
            )
            .await;
            match (json_result, proto_result) {
                (Ok(json_body), Ok(proto_body)) => {
                    // The protobuf answer, decoded into the contract type, must equal the JSON answer field for field.
                    let decoded = FullAddress::decode_from_slice(&proto_body)
                        .map_err(|e| e.to_string())
                        .and_then(|m| proto_json(&m).map_err(|e| e.to_string()));
                    if decoded.as_ref() != Ok(&json_body) {
                        mismatches.push(json!({ "id": case.id, "json": json_body, "proto": decoded.unwrap_or_else(|e| json!(e)) }));
                    }
                }
                (a, b) => mismatches.push(json!({ "id": case.id, "error": [a.err(), b.err()] })),
            }
        }
        let regions_ok = rpc_json(&self.client, &self.base, "ListRegions", &json!({}))
            .await
            .is_ok_and(|body| body["regions"].as_array().is_some_and(|r| !r.is_empty()));
        let grpcurl = grpcurl.await.unwrap_or_else(
            |error| json!({ "ok": false, "known_failure": true, "error": error.to_string() }),
        );
        json!({
            "connect_proto_matches_json": { "ok": mismatches.is_empty(), "checked": checked.len(), "mismatches": mismatches },
            "list_regions": { "ok": regions_ok },
            "grpcurl_search_address": grpcurl,
        })
    }
}

/// Known failure (README): grpcurl's unary call hangs past its own `-max-time`. The hang holds only
/// its own connection, so the probe runs alongside the cases. Reported, never skipped or retried.
async fn grpcurl(port: Option<u16>) -> Value {
    let Some(port) = port.filter(|_| which("grpcurl")) else {
        return json!({ "ok": null, "known_failure": true, "note": "not run: grpcurl unavailable or external service" });
    };
    let output = tokio::time::timeout(
        Duration::from_secs(11),
        Command::new("grpcurl")
            .args([
                "-plaintext",
                "-max-time",
                "10",
                "-d",
                r#"{"addressString":"Copenhagen, Denmark"}"#,
            ])
            .arg(format!("127.0.0.1:{port}"))
            .arg(format!("{SERVICE}/SearchAddress"))
            .kill_on_drop(true)
            .output(),
    )
    .await;
    let passed = matches!(output, Ok(Ok(ref o)) if o.status.success()
        && serde_json::from_slice::<Value>(&o.stdout)
            .is_ok_and(|v| v["addressDetails"]["countryCode"] == "dk"));
    json!({ "ok": passed, "known_failure": true })
}

/// Serializes through text like the service does: `to_value` would widen float32 fields
/// (55.686726 -> 55.68672561645508) and report equal answers as different.
fn proto_json(message: &FullAddress) -> serde_json::Result<Value> {
    serde_json::from_str(&serde_json::to_string(message)?)
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    sha256_files(&[path.to_path_buf()])
}

/// Hash of the files read one after another, each ended by a newline as `gold-corpus` writes them.
pub fn sha256_files(paths: &[PathBuf]) -> Result<String> {
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    for path in paths {
        let mut file =
            std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mut buffer = vec![0; 1 << 20];
        let mut last = b'\n';
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            last = buffer[read - 1];
            context.update(&buffer[..read]);
        }
        if paths.len() > 1 && last != b'\n' {
            context.update(b"\n");
        }
    }
    Ok(hex(context.finish().as_ref()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Same construction as the previous Python manifests: sha256 over (relative path, file sha256) pairs.
fn models_digest(directory: &Path) -> Result<String> {
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, files)?;
            } else if path.is_file() {
                files.push(path);
            }
        }
        Ok(())
    }
    let mut files = vec![];
    walk(directory, &mut files)?;
    let mut relative: Vec<_> = files
        .into_iter()
        .map(|p| {
            (
                p.strip_prefix(directory)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .into_owned(),
                p,
            )
        })
        .collect();
    relative.sort();
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    for (name, path) in relative {
        context.update(name.as_bytes());
        context.update(sha256_file(&path)?.as_bytes());
    }
    Ok(hex(context.finish().as_ref()))
}

pub fn utc_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let [year, month, day] = civil(days as i64);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
pub fn civil(days: i64) -> [i64; 3] {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    [yoe + era * 400 + i64::from(month <= 2), month, day]
}

fn git(args: &[&str]) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root())
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn free_port() -> Result<u16> {
    Ok(std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port())
}

/// The local service under test; stopped by `stop`.
#[derive(Default)]
struct Local {
    child: Option<Child>,
    port: Option<u16>,
    metrics: Option<u16>,
}

impl Local {
    async fn start(
        &mut self,
        client: &Client,
        index: &Path,
        out: &Path,
        binary: &Path,
    ) -> Result<String> {
        let (port, metrics) = (free_port()?, free_port()?);
        self.port = Some(port);
        self.metrics = Some(metrics);
        let log = std::fs::File::create(out.join("service.log"))?;
        let child = Command::new(binary)
            .envs([
                ("INDEX_DIR", index.to_string_lossy().into()),
                ("RPC_PORT", port.to_string()),
                ("METRICS_PORT", metrics.to_string()),
                ("SENTRY_DSN", String::new()),
                // Repeated queries would otherwise skip parsing and time only the lookup.
                ("PARSE_CACHE", "0".into()),
                (
                    "RUST_LOG",
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "warn".into()),
                ),
            ])
            .stdout(log.try_clone()?)
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("start {}", binary.display()))?;
        let child = self.child.insert(child);
        let base = format!("http://127.0.0.1:{port}");
        // loading the index and models takes seconds warm, longer from a cold disk
        for _ in 0..900 {
            if let Some(status) = child.try_wait()? {
                bail!(
                    "service exited with {status}; see {}",
                    out.join("service.log").display()
                );
            }
            if client
                .get(format!("{base}/healthz"))
                .timeout(Duration::from_secs(1))
                .send()
                .await
                .is_ok_and(|r| r.status().is_success())
            {
                return self.warmed(client, metrics).await.map(|()| base);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        bail!("service did not become ready")
    }

    /// Waits for the suggestion warm-up the service runs after it is ready, which would otherwise
    /// share the cores with the first timed cases.
    async fn warmed(&mut self, client: &Client, metrics: u16) -> Result<()> {
        for _ in 0..900 {
            if let Some(status) = self.child.as_mut().and_then(|c| c.try_wait().transpose()) {
                bail!("service exited with {}", status?);
            }
            let text = match client
                .get(format!("http://127.0.0.1:{metrics}/metrics"))
                .timeout(Duration::from_secs(1))
                .send()
                .await
            {
                Ok(response) => response.text().await.ok(),
                Err(_) => None,
            };
            if text.is_some_and(|t| !warming(&t)) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        bail!("service did not finish warming its suggestion caches")
    }

    async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Some(pid) = child.id() {
                // The shell's builtin: slim images such as the eval image have no kill binary.
                let _ = std::process::Command::new("sh")
                    .args(["-c", "kill -TERM \"$1\"", "sh", &pid.to_string()])
                    .status();
            }
            if tokio::time::timeout(Duration::from_secs(30), child.wait())
                .await
                .is_err()
            {
                let _ = child.kill().await;
            }
        }
    }
}

pub async fn run(options: Options) -> Result<i32> {
    let files = match (&options.corpus, options.suite.as_str()) {
        (Some(path), _) => vec![path.clone()],
        (None, "addresses") => vec![crate::model::addresses()],
        (None, _) => crate::model::suite(),
    };
    let mut all_cases = vec![];
    for file in &files {
        all_cases.extend(crate::model::load_corpus(file)?);
    }
    let cases: Vec<_> = all_cases
        .iter()
        .filter(|c| options.suite != "smoke" || c.must_pass)
        .filter(|c| options.only.as_ref().is_none_or(|re| re.is_match(&c.id)))
        .collect();
    std::fs::create_dir_all(&options.out)?;
    let results_path = options.out.join("results.jsonl");
    let done: std::collections::HashSet<String> = if options.resume && results_path.exists() {
        crate::model::load_jsonl::<Record>(&results_path)?
            .into_iter()
            .map(|r| r.id)
            .collect()
    } else {
        let _ = std::fs::remove_file(&results_path);
        Default::default()
    };
    let client = Client::builder().build()?;
    let budget = Budget {
        interval: Duration::from_secs_f64(1. / options.max_rps),
        limit: options.budget,
        used: AtomicUsize::new(0),
        next: tokio::sync::Mutex::new(tokio::time::Instant::now()),
    };
    let index_dir = options
        .index
        .clone()
        .or_else(|| std::env::var_os("INDEX_DIR").map(PathBuf::from));
    if options.rpc.is_none() && index_dir.is_none() {
        bail!("--index DIR or INDEX_DIR is required to start the service");
    }
    // Identifies the build: Tantivy's meta.json lists its segments.
    let status = match &index_dir {
        Some(dir) => json!({
            "software_version": "overture-index",
            "data_updated": format!("index {}", &sha256_file(&dir.join("meta.json"))?[..12]),
        }),
        None => json!({ "software_version": "unknown (external service)" }),
    };
    // several files hash as their concatenation, which is also what `gold-corpus` writes
    let relative = files
        .iter()
        .map(|file| {
            file.strip_prefix(root()).map_or_else(
                |_| {
                    file.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                },
                |p| p.to_string_lossy().into_owned(),
            )
        })
        .collect::<Vec<_>>()
        .join(" + ");
    let mut manifest = json!({
        "started_at": utc_now(),
        "corpus": relative,
        "corpus_sha256": sha256_files(&files)?,
        "suite": options.suite,
        "cases": cases.len(),
        "resumed": !done.is_empty(),
        "backend": {
            "status": status,
            "label": options.backend_label,
        },
        "consumer": { "git": git(&["rev-parse", "HEAD"]), "dirty": !git(&["status", "--porcelain", "--untracked-files=no"]).is_empty() },
        "reference_data_sha256": sha256_file(&root().join("data/reference.json"))?,
        "historical_memberships_sha256": opengeocoding::index::historical_reference_sha256(),
        "max_rps": options.max_rps,
        "repeat": options.repeat,
        "speed_cases": options.speed_cases,
        "workers": options.workers,
        "budget": options.budget,
        "evaluator": env!("CARGO_PKG_VERSION"),
    });
    // The evaluator's own copy loads while the service loads its.
    let direct = index_dir.clone().filter(|_| options.direct).map(|dir| {
        tokio::task::spawn_blocking(move || {
            Index::open(&dir, std::sync::Arc::new(Parser::setup()?))
        })
    });
    let mut local = Local::default();
    let started = async {
        if let Some(rpc) = &options.rpc {
            manifest["consumer"]["external_rpc"] = json!(true);
            manifest["consumer"]["region_cache"] = json!("unknown (external service)");
            return Ok(rpc.trim_end_matches('/').to_string());
        }
        let binary = std::env::var_os("SERVICE_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root().join("target/release/opengeocoding"));
        let index = index_dir.as_deref().context("index directory")?;
        let base = local.start(&client, index, &options.out, &binary).await?;
        let models = std::env::var_os("LIBPOSTAL_DATA_DIR")
            .map(PathBuf::from)
            .or_else(|| {
                [
                    "/data/libpostal",
                    "/opt/homebrew/share/libpostal",
                    "/usr/local/share/libpostal",
                ]
                .iter()
                .map(PathBuf::from)
                .find(|d| d.is_dir())
            });
        manifest["consumer"]["binary_sha256"] = json!(sha256_file(&binary)?);
        manifest["consumer"]["libpostal_models_sha256"] =
            json!(models.map(|d| models_digest(&d)).transpose()?);
        manifest["consumer"]["region_cache"] = json!("none: regions come from the index");
        Ok::<_, anyhow::Error>(base)
    }
    .await;
    let outcome = match started {
        Ok(base) => {
            let grpcurl = tokio::spawn(grpcurl(local.port));
            let index = match direct {
                Some(task) => Some(task.await??),
                None => None,
            };
            let checker = std::sync::Arc::new(Checker {
                client: client.clone(),
                base,
                budget,
                metrics: local
                    .metrics
                    .filter(|_| options.suite != "gold")
                    .map(|port| format!("http://127.0.0.1:{port}")),
                index,
                repeat: options.repeat,
            });
            run_cases(
                &checker,
                &cases,
                &done,
                &results_path,
                &options,
                &mut manifest,
                grpcurl,
            )
            .await
        }
        Err(error) => Err(error),
    };
    local.stop().await;
    outcome?;
    let results = crate::model::load_jsonl::<Record>(&results_path)?;
    let (report, failed) = summarize(&all_cases, &results, &manifest);
    std::fs::write(options.out.join("report.md"), &report)?;
    print!("{report}");
    if manifest["stopped"].is_string() {
        return Ok(3);
    }
    let mut failed = failed;
    if let Some(baseline) = &options.compare {
        failed |= diff(
            &all_cases,
            baseline,
            &options.out,
            Some(&options.out.join("diff.md")),
            options.fail_on_drift,
            false,
        )?;
    }
    Ok(i32::from(failed))
}

/// The ids of the `n` cases timed: ranked by a hash of the id, so that runs of two commits on one
/// corpus time the same cases.
fn speed_sample(cases: &[&Case], n: usize) -> std::collections::HashSet<String> {
    let mut ranked: Vec<_> = cases
        .iter()
        .map(|c| {
            (
                ring::digest::digest(&ring::digest::SHA256, c.id.as_bytes()),
                &c.id,
            )
        })
        .collect();
    ranked.sort_by(|a, b| a.0.as_ref().cmp(b.0.as_ref()));
    ranked
        .into_iter()
        .take(n)
        .map(|(_, id)| id.clone())
        .collect()
}

#[allow(clippy::too_many_arguments)]
async fn run_cases(
    checker: &std::sync::Arc<Checker>,
    cases: &[&Case],
    done: &std::collections::HashSet<String>,
    results_path: &Path,
    options: &Options,
    manifest: &mut Value,
    grpcurl: tokio::task::JoinHandle<Value>,
) -> Result<()> {
    let write = |manifest: &Value| {
        std::fs::write(
            options.out.join("manifest.json"),
            serde_json::to_string_pretty(&sorted(manifest.clone()))? + "\n",
        )
    };
    write(manifest)?;
    let mut sink = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(results_path)?;
    let mut stopped = None;
    let interrupted = tokio::signal::ctrl_c();
    tokio::pin!(interrupted);
    let sample = speed_sample(cases, options.speed_cases);
    let (timed, untimed): (Vec<&Case>, Vec<&Case>) = cases
        .iter()
        .copied()
        .filter(|c| !done.contains(&c.id))
        .partition(|c| sample.contains(&c.id));
    let mut finished = cases.len() - timed.len() - untimed.len();
    // The timed cases go first, one at a time, on an otherwise idle service.
    'phases: for (phase, workers, timed) in
        [(timed, 1, true), (untimed, options.workers.max(1), false)]
    {
        let mut pending = phase.into_iter();
        let mut running = tokio::task::JoinSet::new();
        loop {
            while running.len() < workers
                && let Some(case) = pending.next()
            {
                if checker.budget.used() >= checker.budget.limit {
                    stopped = Some("request budget exhausted");
                    break 'phases;
                }
                let (checker, case) = (checker.clone(), case.clone());
                running.spawn(async move { checker.check(&case, timed).await });
            }
            let record = tokio::select! {
                joined = running.join_next() => match joined {
                    Some(record) => record??,
                    None => break,
                },
                _ = &mut interrupted => {
                    stopped = Some("interrupted");
                    break 'phases;
                }
            };
            writeln!(sink, "{}", serde_json::to_string(&record)?)?;
            sink.flush()?;
            finished += 1;
            println!(
                "[{finished}/{}] {:<19} {}",
                cases.len(),
                record.class,
                record.id
            );
        }
    }
    if stopped.is_none() {
        let gating: Vec<_> = cases
            .iter()
            .copied()
            .filter(|c| c.must_pass && c.expect.as_ref().is_some_and(|e| e.error.is_none()))
            .collect();
        manifest["transport"] = checker
            .transport(if gating.is_empty() { cases } else { &gating }, grpcurl)
            .await;
    }
    manifest["finished_at"] = json!(utc_now());
    manifest["rpc_requests"] = json!(checker.budget.used());
    manifest["stopped"] = json!(stopped);
    write(manifest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_manifest_hash_of_the_suite_is_the_hash_of_the_combined_file() {
        let combined = std::env::temp_dir().join(format!("suite-{}.jsonl", std::process::id()));
        crate::model::write_suite(&combined).unwrap();
        assert!(crate::model::suite().len() > 1);
        assert_eq!(
            sha256_files(&crate::model::suite()).unwrap(),
            sha256_file(&combined).unwrap()
        );
        std::fs::remove_file(combined).ok();
    }

    use super::*;

    #[tokio::test]
    async fn number_aliases_share_coordinate_requests_and_rescoring() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().fallback(
            |uri: axum::http::Uri, axum::Json(request): axum::Json<Value>| async move {
                let at = request == json!({"lat":55., "lon":12.});
                axum::Json(if uri.path().ends_with("/ResolveRegion") {
                    assert!(at);
                    json!({"regions":[{"countryCode":"dk", "adminCode":"84"}]})
                } else {
                    assert!(uri.path().ends_with("/ReverseGeocode"));
                    // the offset request is 25 m away, and finds the same house
                    let point = ["lat", "lon"].map(|k| request[k].as_f64().unwrap());
                    let metres = crate::score::haversine([55., 12.], point) * 1000.;
                    assert!(at || (metres - crate::score::OFFSET_M).abs() < 0.1);
                    json!({"results":[{"layer":"address", "address":{"geoCoordinates":{"lat":55.0001, "lon":12.}, "addressDetails":{"countryCode":"dk", "regionCodes":[{"code":"DK-84"}], "houseNumber":"45"}}}]})
                })
            },
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let checker = Checker {
            client: Client::new(),
            base,
            metrics: None,
            index: None,
            repeat: 1,
            budget: Budget {
                interval: Duration::ZERO,
                limit: 3,
                used: AtomicUsize::new(0),
                next: tokio::sync::Mutex::new(tokio::time::Instant::now()),
            },
        };
        let alternative = |number| json!({"country":["dk"], "region":["DK-84"], "point":[55.,12.], "radius_km":0.15, "level":"house", "components":{"house_number":number}});
        let case = Case {
            location: "dar:1".into(),
            expect: Some(
                serde_json::from_value(json!({"any_of":[alternative("12"),alternative("45")]}))
                    .unwrap(),
            ),
            ..Default::default()
        };
        let (_, reverse) = checker.reverse(&case, 1).await;
        let reverse_offset = checker.reverse_offset(&case).await;
        let region = checker.region(&case).await;
        let mut record = Record {
            reverse,
            reverse_offset,
            region,
            ..Default::default()
        };
        let classes = |r: &Record| {
            [&r.reverse, &r.reverse_offset, &r.region].map(|l| l.as_ref().unwrap().class.clone())
        };
        assert_eq!(classes(&record), ["pass", "pass", "pass"]);
        rescore_locate(&case, &mut record);
        assert_eq!(classes(&record), ["pass", "pass", "pass"]);
        // another number is another house at the point, but still a house near it from the offset
        for found in [&mut record.reverse, &mut record.reverse_offset] {
            found
                .as_mut()
                .unwrap()
                .answer
                .as_mut()
                .unwrap()
                .house_number = "13".into();
        }
        rescore_locate(&case, &mut record);
        assert_eq!(classes(&record), ["wrong_house", "pass", "pass"]);
        server.abort();
        assert_eq!(checker.budget.used(), 3);
    }

    #[tokio::test]
    async fn landmark_case_checks_the_same_query_with_the_flag_on_and_off() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let airport = json!({
            "type": "landmark", "category": "airport", "countryCode": "fr",
            "displayName": "Made-up Airport", "geoCoordinates": { "lat": 48., "lon": 2. }
        });
        let handler_airport = airport.clone();
        let app = axum::Router::new().route(
            &format!("/{SERVICE}/SuggestAddress"),
            axum::routing::post(move |axum::Json(request): axum::Json<Value>| async move {
                assert_eq!(request["text"], "PNV");
                assert_eq!(request["limit"], SUGGEST_LIMIT);
                let found = match request["includeLandmarks"].as_bool().unwrap() {
                    true => vec![handler_airport],
                    false => vec![],
                };
                axum::Json(json!({ "suggestions": found }))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let checker = Checker {
            client: Client::new(),
            base,
            budget: Budget {
                interval: Duration::ZERO,
                limit: 2,
                used: AtomicUsize::new(0),
                next: tokio::sync::Mutex::new(tokio::time::Instant::now()),
            },
            metrics: None,
            index: None,
            repeat: 1,
        };
        let case = Case {
            query: "PNV".into(),
            include_landmarks: true,
            expect: Some(Expect {
                level: "landmark".into(),
                country: vec!["fr".into()],
                point: Some([48., 2.]),
                radius_km: Some(1.),
                components: [("category".into(), "airport".into())].into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut checks = checker.suggest(&case, 1).await;
        server.abort();
        assert_eq!(checker.budget.used(), 2);
        assert_eq!(checks.len(), 2);
        assert_eq!(checks["full"].rank, Some(1));
        assert_eq!(crate::score::suggest_class(&checks["full"]), "pass");
        let off = checks.get_mut("off").unwrap();
        assert!(off.landmarks_off && off.found.is_empty());
        assert_eq!(crate::score::suggest_class(off), "pass");
        off.found = suggested(&json!({ "suggestions": [airport] }));
        assert_eq!(crate::score::suggest_class(off), "unexpected");
        off.error = Some("timeout".into());
        assert_eq!(crate::score::suggest_class(off), "error");
    }

    #[tokio::test]
    async fn address_suggestions_preserve_the_building_and_unverified_suffix() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().route(
            &format!("/{SERVICE}/SuggestAddress"),
            axum::routing::post(|axum::Json(request): axum::Json<Value>| async move {
                assert!(request.get("countryCode").is_none() || request["countryCode"] == "dk");
                assert_eq!(request["limit"], SUGGEST_LIMIT);
                let suffix = match request["text"].as_str().unwrap() {
                    "test road 1" => "",
                    "test road 1, 4. th." => "4. th.",
                    other => panic!("unexpected request {other}"),
                };
                axum::Json(json!({ "suggestions": [{
                    "id": "building-1", "type": "address", "countryCode": "dk",
                    "displayName": "Test Road 1", "unverifiedSuffix": suffix,
                    "geoCoordinates": { "lat": 55.675278321, "lon": 12.570278321 }
                }] }))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let checker = Checker {
            client: Client::new(),
            base,
            metrics: None,
            index: None,
            repeat: 1,
            budget: Budget {
                interval: Duration::ZERO,
                limit: 4,
                used: AtomicUsize::new(0),
                next: tokio::sync::Mutex::new(tokio::time::Instant::now()),
            },
        };
        let mut case = Case {
            query: "test road 1, 4. th.".into(),
            expect: Some(Expect {
                level: "house".into(),
                country: vec!["dk".into()],
                point: Some([55.675278, 12.570278]),
                radius_km: Some(0.25),
                ..Default::default()
            }),
            suggest_address: Some(crate::model::SuggestAddressExpect {
                building: "test road 1".into(),
                unverified_suffix: "4. th.".into(),
                country_code: None,
            }),
            ..Default::default()
        };
        for cc in [None, Some("dk".into())] {
            case.suggest_address.as_mut().unwrap().country_code = cc;
            let record = checker.check(&case, true).await.unwrap();
            assert_eq!(record.class, "unscored");
            assert!(record.reverse.is_none() && record.region.is_none());
            let saved = serde_json::to_value(record).unwrap();
            let mut record: Record = serde_json::from_value(saved).unwrap();
            rescore_suggest(&case, &mut record);
            assert_eq!(crate::score::suggest_class(&record.suggest["full"]), "pass");
            for field in ["id", "point", "kind", "country", "unverified_suffix"] {
                let mut broken = record.clone();
                let found = &mut broken.suggest.get_mut("full").unwrap().found[0];
                match field {
                    "id" => found.id = "another-building".into(),
                    "point" => found.point[0] += 0.00000001,
                    "kind" => found.kind = "street".into(),
                    "country" => found.country = "se".into(),
                    "unverified_suffix" => found.unverified_suffix = "4. tv.".into(),
                    _ => unreachable!(),
                }
                rescore_suggest(&case, &mut broken);
                assert_eq!(
                    crate::score::suggest_class(&broken.suggest["full"]),
                    "missing",
                    "{field}"
                );
            }
        }
        server.abort();
        assert_eq!(checker.budget.used(), 4);
    }

    #[test]
    fn the_speed_sample_depends_on_the_ids_alone() {
        let cases: Vec<_> = (0..100)
            .map(|i| Case {
                id: format!("case-{i}"),
                ..Default::default()
            })
            .collect();
        let refs: Vec<_> = cases.iter().collect();
        let sample = speed_sample(&refs, 10);
        assert_eq!(sample.len(), 10);
        let reversed: Vec<_> = refs.iter().rev().copied().collect();
        assert_eq!(speed_sample(&reversed, 10), sample);
        assert_eq!(speed_sample(&refs[..50], usize::MAX).len(), 50);
    }

    #[test]
    fn index_searches_sum_every_tier() {
        let text = "# TYPE nominatim_geolocation_search_candidates_seconds histogram\n\
            nominatim_geolocation_search_candidates_seconds_bucket{tier=\"raw\",le=\"0.005\"} 2\n\
            nominatim_geolocation_search_candidates_seconds_count{tier=\"raw\"} 3\n\
            nominatim_geolocation_search_candidates_seconds_count{tier=\"loose\"} 2\n\
            nominatim_geolocation_query_tier_total{tier=\"raw\"} 1\n";
        assert_eq!(searches_in(text), 5.);
    }

    #[test]
    fn warming_reads_the_services_own_gauge() {
        use opengeocoding::metrics::METRICS;
        let text = || {
            prometheus::TextEncoder::new()
                .encode_to_string(&METRICS.registry.gather())
                .unwrap()
        };
        assert!(warming(&text()));
        METRICS.suggest_warm.set(1);
        assert!(!warming(&text()));
        assert!(!warming("grpc_server_started_total 0\n"));
    }

    #[test]
    fn calendar_dates() {
        assert_eq!(civil(0), [1970, 1, 1]);
        assert_eq!(civil(20_738), [2026, 10, 12]);
        assert_eq!(civil(11_016), [2000, 2, 29]);
    }

    #[test]
    fn protobuf_answers_compare_as_float32() {
        let served: Value =
            serde_json::from_str(r#"{"geoCoordinates":{"lat":55.686726,"lon":12.570072}}"#)
                .unwrap();
        let message = FullAddress {
            geo_coordinates: opengeocoding::proto::opengeocoding::GeoCoordinates {
                lat: 55.686_726,
                lon: 12.570_072,
                ..Default::default()
            }
            .into(),
            ..Default::default()
        };
        let decoded = FullAddress::decode_from_slice(&message.encode_to_vec()).unwrap();
        assert_eq!(proto_json(&decoded).unwrap(), served);
    }

    #[test]
    fn classify_expected_errors() {
        let case = Case {
            expect: Some(Expect {
                error: Some("invalid_argument".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut record = Record::default();
        classify(&case, Err("invalid_argument"), &mut record, true);
        assert_eq!(record.class, "pass");
        classify(&case, Ok(&json!({})), &mut record, true);
        assert_eq!(record.class, "unexpected_result");
    }
}
