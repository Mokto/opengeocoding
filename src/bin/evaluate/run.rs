use crate::{
    model::{Case, DirectRaw, Record, root},
    report::{diff, sorted, summarize},
    score::{
        Expect, details, from_index, rank, region_codes, region_flags, score, summary_of, text,
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
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant, SystemTime},
};
use tokio::process::{Child, Command};

const SERVICE: &str = "opengeocoding.Geolocation";

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

async fn rpc_json(
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

/// Scores an RPC outcome against a case; also used to re-score saved responses.
pub fn classify(case: &Case, outcome: Result<&Value, &str>, record: &mut Record) {
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
            let scored = score(case.expect.as_ref(), response, true);
            record.class = scored.class.into();
            record.level = Some(scored.level.into());
            record.distance_km = scored.distance_km;
            record.flags = scored.flags;
            record.flags.extend(region_flags(response));
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
    /// Requests per case; `ms` is the fastest. Only for the local service, so outside the budget.
    repeat: usize,
}

impl Checker {
    async fn check(&self, case: &Case) -> Result<Record> {
        let mut record = Record {
            id: case.id.clone(),
            query: case.query.clone(),
            ..Default::default()
        };
        let before = match &self.metrics {
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
        if let (Some(metrics), Some(before)) = (&self.metrics, before)
            && let Some(after) = index_searches(&self.client, metrics).await
        {
            record.backend_calls = Some((after - before).round() as usize);
        }
        for _ in 1..self.repeat {
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
        let response = match result {
            Ok(response) => response,
            Err(error) => {
                classify(case, Err(&error), &mut record);
                return Ok(record);
            }
        };
        classify(case, Ok(&response), &mut record);
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
            let hit = index.reverse(lat, lon);
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

    async fn transport(&self, cases: &[&Case], port: Option<u16>) -> Value {
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
        let grpcurl = match (port, which("grpcurl")) {
            (Some(port), true) => {
                let output = tokio::time::timeout(
                    Duration::from_secs(20),
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
                // Known failure (README): grpcurl's unary call hangs. Reported, never skipped or retried.
                json!({ "ok": passed, "known_failure": true })
            }
            _ => {
                json!({ "ok": null, "known_failure": true, "note": "not run: grpcurl unavailable or external service" })
            }
        };
        json!({
            "connect_proto_matches_json": { "ok": mismatches.is_empty(), "checked": checked.len(), "mismatches": mismatches },
            "list_regions": { "ok": regions_ok },
            "grpcurl_search_address": grpcurl,
        })
    }
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
    let mut context = ring::digest::Context::new(&ring::digest::SHA256);
    let mut file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buffer = vec![0; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
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
                return Ok(base);
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        bail!("service did not become ready")
    }

    async fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if let Some(pid) = child.id() {
                let _ = std::process::Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
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
    let corpus = options.corpus.clone().unwrap_or_else(|| {
        if options.suite == "addresses" {
            crate::model::addresses()
        } else {
            crate::model::gold()
        }
    });
    let all_cases = crate::model::load_corpus(&corpus)?;
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
    let relative = corpus.strip_prefix(root()).map_or_else(
        |_| {
            corpus
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        },
        |p| p.to_string_lossy().into_owned(),
    );
    let mut manifest = json!({
        "started_at": utc_now(),
        "corpus": relative,
        "corpus_sha256": sha256_file(&corpus)?,
        "suite": options.suite,
        "cases": cases.len(),
        "resumed": !done.is_empty(),
        "backend": {
            "status": status,
            "label": options.backend_label,
        },
        "consumer": { "git": git(&["rev-parse", "HEAD"]), "dirty": !git(&["status", "--porcelain", "--untracked-files=no"]).is_empty() },
        "reference_data_sha256": sha256_file(&root().join("data/reference.json"))?,
        "max_rps": options.max_rps,
        "repeat": options.repeat,
        "budget": options.budget,
        "evaluator": env!("CARGO_PKG_VERSION"),
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
            let index = match &index_dir {
                Some(dir) if options.direct => {
                    let dir = dir.clone();
                    Some(
                        tokio::task::spawn_blocking(move || {
                            Index::open(&dir, std::sync::Arc::new(Parser::setup()?))
                        })
                        .await??,
                    )
                }
                _ => None,
            };
            let checker = Checker {
                client: client.clone(),
                base,
                budget,
                metrics: local.metrics.map(|port| format!("http://127.0.0.1:{port}")),
                index,
                repeat: options.repeat,
            };
            run_cases(
                &checker,
                &cases,
                &done,
                &results_path,
                &options,
                &mut manifest,
                local.port,
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

#[allow(clippy::too_many_arguments)]
async fn run_cases(
    checker: &Checker,
    cases: &[&Case],
    done: &std::collections::HashSet<String>,
    results_path: &Path,
    options: &Options,
    manifest: &mut Value,
    port: Option<u16>,
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
    for (n, case) in cases.iter().enumerate() {
        if done.contains(&case.id) {
            continue;
        }
        if checker.budget.used() >= checker.budget.limit {
            stopped = Some("request budget exhausted");
            break;
        }
        let record = tokio::select! {
            record = checker.check(case) => record?,
            _ = &mut interrupted => {
                stopped = Some("interrupted");
                break;
            }
        };
        writeln!(sink, "{}", serde_json::to_string(&record)?)?;
        sink.flush()?;
        println!(
            "[{}/{}] {:<19} {}",
            n + 1,
            cases.len(),
            record.class,
            case.id
        );
    }
    if stopped.is_none() {
        let gating: Vec<_> = cases
            .iter()
            .copied()
            .filter(|c| c.must_pass && c.expect.as_ref().is_some_and(|e| e.error.is_none()))
            .collect();
        manifest["transport"] = checker
            .transport(if gating.is_empty() { cases } else { &gating }, port)
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
    use super::*;

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
        classify(&case, Err("invalid_argument"), &mut record);
        assert_eq!(record.class, "pass");
        classify(&case, Ok(&json!({})), &mut record);
        assert_eq!(record.class, "unexpected_result");
    }
}
