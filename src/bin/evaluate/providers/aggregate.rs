use super::{Options, Provider, fetch};
use crate::{
    model::{Case, load_corpus, load_suite},
    report::load_side,
    run::{sha256_files, utc_now},
    score::{score, text},
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{File, OpenOptions},
    path::Path,
    time::Duration,
};

#[derive(Default, Deserialize, Serialize)]
struct Counts {
    outcomes: BTreeMap<String, usize>,
    paired: [usize; 4],
    adapter_errors: usize,
    query_errors: usize,
    #[serde(default)]
    normalization_failures: BTreeMap<String, usize>,
}

#[derive(Deserialize, Serialize)]
struct State {
    settings: Value,
    started_at: String,
    updated_at: String,
    phase: String,
    next: usize,
    attempts: usize,
    uncertain_attempts: usize,
    in_flight: bool,
    retryable: bool,
    failure_kind: Option<String>,
    groups: BTreeMap<String, Counts>,
}

impl State {
    fn save(&mut self, out: &Path) -> Result<()> {
        self.updated_at = utc_now();
        let pending = out.join("aggregate.json.tmp");
        let mut file = File::create(&pending)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        file.sync_all()?;
        std::fs::rename(pending, out.join("aggregate.json"))?;
        let mut report = String::from(
            "# Aggregate-only provider evaluation\n\nNo provider responses, normalized addresses, coordinates, per-case outcomes or response diagnostics are retained. Confidence is ignored. This diagnostic corpus is not an unbiased production-accuracy estimate.\n\n",
        );
        report.push_str(&format!(
            "Phase: {}. Completed: {}/{}. Request attempts: {}/{} (including uncertain attempts).\n\n",
            self.phase, self.next, self.settings["cases"], self.attempts, self.settings["budget"]
        ));
        for (name, counts) in &self.groups {
            report.push_str(&format!("## {name}\n\n| Outcome | Cases |\n|---|---:|\n"));
            for (class, count) in &counts.outcomes {
                report.push_str(&format!("| {class} | {count} |\n"));
            }
            if counts.paired.iter().sum::<usize>() > 0 {
                report.push_str(&format!(
                    "\nBoth pass: {}. Baseline only: {}. Provider only: {}. Neither: {}.\n",
                    counts.paired[0], counts.paired[1], counts.paired[2], counts.paired[3]
                ));
            }
            report.push_str(&format!(
                "\nAdapter errors: {}. Query errors: {}.\n\n",
                counts.adapter_errors, counts.query_errors
            ));
            for (kind, count) in &counts.normalization_failures {
                report.push_str(&format!("- Normalization `{kind}`: {count}\n"));
            }
            report.push('\n');
        }
        std::fs::write(out.join("report.md"), report)?;
        Ok(())
    }
}

fn groups(case: &Case) -> Vec<String> {
    let mut groups = vec![
        "all".into(),
        if case.holdout { "held_out" } else { "tuning" }.into(),
        format!("cohort:{}", case.cohort),
    ];
    let countries: HashSet<_> = case
        .expect
        .iter()
        .flat_map(|e| e.alternatives())
        .flat_map(|e| &e.country)
        .map(|cc| cc.to_ascii_lowercase())
        .collect();
    if !countries.is_empty()
        && countries
            .iter()
            .all(|cc| matches!(cc.as_str(), "us" | "ca"))
    {
        groups.push("us_ca".into());
    }
    groups
}

fn retryable(error: &anyhow::Error, provider: &Provider) -> bool {
    if error.chain().any(|cause| {
        let message = cause.to_string().to_ascii_lowercase();
        [
            "certificate",
            "unknown issuer",
            "invalid peer",
            "invalid url",
        ]
        .iter()
        .any(|word| message.contains(word))
    }) {
        return false;
    }
    error
        .downcast_ref::<reqwest::Error>()
        .is_some_and(|e| e.is_timeout() || e.is_connect() || e.is_body())
        || [408, 500, 502, 503, 504].iter().any(|status| {
            error
                .to_string()
                .contains(&format!("{} HTTP {status};", provider.name))
        })
}

pub async fn run(options: Options, resume: bool) -> Result<i32> {
    ensure!(
        options.fetch && options.budget > 0,
        "aggregate-only requires --fetch and a positive --budget"
    );
    ensure!(
        options.cache.as_os_str().is_empty(),
        "aggregate-only does not accept --cache"
    );
    ensure!(
        !options.storage_authorized,
        "aggregate-only does not accept --storage-authorized"
    );
    let provider = options.provider;
    let cases = match &options.corpus {
        Some(path) => load_corpus(path)?,
        None => load_suite()?,
    };
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|c| c.expect.as_ref().is_some_and(|e| e.error.is_none()))
        .filter(|c| options.only.as_ref().is_none_or(|re| re.is_match(&c.id)))
        .collect();
    let mut ids = HashSet::new();
    ensure!(
        !cases.is_empty() && cases.iter().all(|c| ids.insert(&c.id)),
        "no scored cases or duplicate case ids"
    );
    ensure!(
        options.budget >= cases.len(),
        "request budget is smaller than the selected corpus; no requests sent"
    );
    let baseline = options
        .compare
        .as_deref()
        .map(load_side)
        .transpose()?
        .map(|(_, records)| records);
    let mut baseline_passes = vec![];
    if let Some(base) = &baseline {
        for case in &cases {
            let before = base
                .get(&case.id)
                .context("baseline lacks a selected case")?;
            ensure!(before.query == case.query, "baseline query differs");
            baseline_passes.push(match &before.response {
                Some(response) => score(case.expect.as_ref(), response, false).class == "pass",
                None => {
                    ensure!(
                        before.error.is_some(),
                        "baseline lacks a full response or error"
                    );
                    false
                }
            });
        }
    }
    let files = options
        .corpus
        .clone()
        .map_or_else(crate::model::suite, |p| vec![p]);
    let baseline_hash = options
        .compare
        .as_ref()
        .map(|path| {
            let files = if path.is_file() {
                vec![path.clone()]
            } else {
                vec![path.join("manifest.json"), path.join("results.jsonl")]
            };
            sha256_files(&files)
        })
        .transpose()?;
    let settings = json!({
        "version": 1, "provider": provider.name, "label": provider.label,
        "cases": cases.len(), "budget": options.budget, "confidence": false,
        "corpus_sha256": sha256_files(&files)?, "baseline_sha256": baseline_hash,
        "only": options.only.as_ref().map(|re| re.as_str()),
        "binary_sha256": sha256_files(&[std::env::current_exe()?])?,
        "request_settings": (provider.request)(""), "interval_ms": provider.interval_ms,
        "retention": "aggregates_only"
    });
    std::fs::create_dir_all(&options.out)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(options.out.join(".aggregate.lock"))?;
    lock.try_lock()
        .context("another aggregate run holds this output lock")?;
    for entry in std::fs::read_dir(&options.out)? {
        let name = entry?.file_name();
        ensure!(
            name == ".aggregate.lock"
                || (resume
                    && matches!(
                        name.to_str(),
                        Some("aggregate.json" | "aggregate.json.tmp" | "report.md")
                    )),
            "output is not an empty aggregate-only directory"
        );
    }
    let mut state = if resume {
        let state: State =
            serde_json::from_slice(&std::fs::read(options.out.join("aggregate.json"))?)?;
        ensure!(
            state.settings == settings,
            "resume settings, corpus, baseline or binary differ"
        );
        ensure!(
            state.next <= cases.len()
                && state.attempts >= state.next
                && state.attempts <= options.budget,
            "invalid aggregate progress"
        );
        ensure!(
            state
                .groups
                .get("all")
                .map_or(0, |c| c.outcomes.values().sum::<usize>())
                == state.next,
            "aggregate totals differ from progress"
        );
        state
    } else {
        State {
            settings,
            started_at: utc_now(),
            updated_at: utc_now(),
            phase: "running".into(),
            next: 0,
            attempts: 0,
            uncertain_attempts: 0,
            in_flight: false,
            retryable: false,
            failure_kind: None,
            groups: BTreeMap::new(),
        }
    };
    if state.in_flight {
        state.uncertain_attempts += 1;
        state.in_flight = false;
    }
    let token = std::env::var(provider.credential_env).with_context(|| {
        format!(
            "set {} through your secret manager",
            provider.credential_env
        )
    })?;
    ensure!(!token.trim().is_empty(), "empty provider credential");
    let client = reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    state.phase = "running".into();
    state.save(&options.out)?;
    while state.next < cases.len() {
        if state.attempts >= options.budget {
            state.phase = "budget_exhausted".into();
            state.save(&options.out)?;
            return Ok(2);
        }
        if state.attempts > 0 {
            tokio::time::sleep(Duration::from_millis(provider.interval_ms)).await;
        }
        let case = &cases[state.next];
        // Count before network I/O so an interrupted request cannot reset the attempt budget.
        state.attempts += 1;
        state.in_flight = true;
        state.retryable = false;
        state.failure_kind = None;
        state.save(&options.out)?;
        let entry = match fetch(
            provider,
            &client,
            &token,
            (provider.request)(&case.query),
            true,
        )
        .await
        {
            Ok(entry) => entry,
            Err(error) => {
                state.phase = "blocked".into();
                state.in_flight = false;
                state.retryable = retryable(&error, provider);
                state.failure_kind = Some(
                    error
                        .to_string()
                        .strip_prefix(&format!("{} HTTP ", provider.name))
                        .and_then(|rest| rest.split(';').next()?.parse::<u16>().ok())
                        .map_or_else(|| "request".into(), |status| format!("http_{status}")),
                );
                state.save(&options.out)?;
                bail!(
                    "provider request failed; response details are not retained in aggregate-only mode"
                );
            }
        };
        state.in_flight = false;
        let query_error = provider.query_error_statuses.contains(&entry.status)
            || (provider.name == "google"
                && (entry.status == 400 || text(&entry.response, "status") == "INVALID_REQUEST"));
        if !query_error
            && entry.status == 200
            && (entry.response.get("error").is_some()
                || provider
                    .validate_status
                    .is_some_and(|validate| validate(&entry.response).is_err()))
        {
            state.phase = "blocked".into();
            state.failure_kind = Some("provider_status".into());
            state.retryable =
                provider.name == "google" && text(&entry.response, "status") == "UNKNOWN_ERROR";
            state.save(&options.out)?;
            bail!("provider API status blocked the run; response details are not retained");
        }
        let (class, normalization_failure) = if query_error {
            ("error", None)
        } else {
            match (provider.normalize)(&entry.response) {
                Ok(response) => (score(case.expect.as_ref(), &response, false).class, None),
                Err(error) => (
                    "error",
                    Some(if provider.name == "google" {
                        super::google::normalization_failure(&error, &entry.response)
                    } else {
                        "other"
                    }),
                ),
            }
        };
        drop(entry);
        for group in groups(case) {
            let counts = state.groups.entry(group).or_default();
            *counts.outcomes.entry(class.into()).or_default() += 1;
            counts.adapter_errors += usize::from(normalization_failure.is_some());
            counts.query_errors += usize::from(query_error);
            if let Some(kind) = normalization_failure {
                *counts
                    .normalization_failures
                    .entry(kind.into())
                    .or_default() += 1;
            }
            if baseline.is_some() {
                let index = match (baseline_passes[state.next], class == "pass") {
                    (true, true) => 0,
                    (true, false) => 1,
                    (false, true) => 2,
                    (false, false) => 3,
                };
                counts.paired[index] += 1;
            }
        }
        state.next += 1;
        state.save(&options.out)?;
        if state.next % 50 == 0 {
            eprintln!(
                "{}/{} cases; {} attempts; aggregates only",
                state.next,
                cases.len(),
                state.attempts
            );
        }
    }
    state.phase = "complete".into();
    state.save(&options.out)?;
    println!(
        "{} cases completed; {} attempts; aggregate report: {}",
        state.next,
        state.attempts,
        options.out.join("report.md").display()
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::atomic::{AtomicU16, Ordering},
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    static PORT: AtomicU16 = AtomicU16::new(0);

    fn request(query: &str) -> Value {
        json!({"endpoint": format!("http://127.0.0.1:{}/", PORT.load(Ordering::SeqCst)), "address": query})
    }

    #[tokio::test]
    async fn aggregate_run_discards_responses_and_resumes_with_a_durable_budget() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        PORT.store(listener.local_addr().unwrap().port(), Ordering::SeqCst);
        let responses = [
            (200, json!({"status":"OK","results":[{"types":["street_address"],"formatted_address":"PRIVATE_RESPONSE_MARKER","geometry":{"location":{"lat":37.,"lng":-122.}},"address_components":[{"types":["country"],"short_name":"US"},{"types":["route"],"long_name":"PRIVATE_PROVIDER_ROAD"},{"types":["street_number"],"long_name":"12"}]}]}).to_string()),
            (400, "<html>PRIVATE_RESPONSE_MARKER</html>".into()),
            (200, json!({"status":"INVALID_REQUEST","error_message":"PRIVATE_RESPONSE_MARKER"}).to_string()),
            (200, json!({"status":"REQUEST_DENIED","error_message":"PRIVATE_RESPONSE_MARKER"}).to_string()),
            (200, json!({"status":"ZERO_RESULTS","results":[]}).to_string()),
            (200, json!({"status":"OK","results":[],"error_message":"PRIVATE_RESPONSE_MARKER"}).to_string()),
        ];
        let server = std::thread::spawn(move || {
            let mut requests = vec![];
            for (status, body) in responses {
                let deadline = Instant::now() + Duration::from_secs(15);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "mock request timed out");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = vec![];
                while !bytes.ends_with(b"\r\n\r\n") {
                    let mut buffer = [0; 4096];
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0 && bytes.len() < 16384);
                    bytes.extend_from_slice(&buffer[..count]);
                }
                requests.push(String::from_utf8(bytes).unwrap());
                write!(stream, "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        let root = std::env::temp_dir().join(format!(
            "aggregate-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let baseline = root.join("baseline");
        std::fs::create_dir_all(&baseline).unwrap();
        let cases: Vec<Case> = ["first", "second", "third", "fourth", "fifth"].into_iter().enumerate().map(|(i, name)| Case {
            id: name.into(), query: format!("{name}-input"), cohort: "made_up".into(), holdout: i == 0,
            expect: Some(serde_json::from_value(if i == 0 { json!({"level":"house","country":["us"],"point":[37.,-122.],"radius_km":0.1,"components":{"house_number":"12"}}) } else { json!({"level":"none","country":["us"]}) }).unwrap()),
            ..Default::default()
        }).collect();
        let corpus = root.join("corpus.jsonl");
        std::fs::write(
            &corpus,
            cases
                .iter()
                .map(|c| serde_json::to_string(c).unwrap() + "\n")
                .collect::<String>(),
        )
        .unwrap();
        let records: Vec<_> = cases.iter().enumerate().map(|(i, c)| crate::model::Record {
            id: c.id.clone(), query: c.query.clone(), class: "pass".into(),
            response: Some(if i == 0 { json!({"geoCoordinates":{"lat":37.,"lon":-122.},"addressDetails":{"countryCode":"us","houseNumber":"12","road":"Example Road"}}) } else { json!({}) }),
            ..Default::default()
        }).collect();
        std::fs::write(baseline.join("manifest.json"), "{}").unwrap();
        std::fs::write(
            baseline.join("results.jsonl"),
            records
                .iter()
                .map(|r| serde_json::to_string(r).unwrap() + "\n")
                .collect::<String>(),
        )
        .unwrap();
        let provider = Box::leak(Box::new(Provider {
            credential_env: "PATH",
            request,
            interval_ms: 0,
            ..super::super::google::PROVIDER
        }));
        let options = || Options {
            provider,
            corpus: Some(corpus.clone()),
            out: root.join("out"),
            cache: Default::default(),
            only: None,
            storage_authorized: false,
            fetch: true,
            budget: 6,
            compare: Some(baseline.clone()),
            reverse: false,
            rpc: None,
        };
        let mut invalid = options();
        invalid.cache = root.join("must-not-exist");
        assert!(run(invalid, false).await.is_err());
        assert!(!root.join("must-not-exist").exists());
        let mut invalid = options();
        invalid.budget = 2;
        assert!(run(invalid, false).await.is_err());
        assert!(!root.join("out").exists());
        let error = run(options(), false).await.unwrap_err().to_string();
        assert!(!error.contains("PRIVATE_"));
        let path = root.join("out/aggregate.json");
        let mut state: State = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!((state.next, state.attempts), (3, 4));
        assert_eq!(state.phase, "blocked");
        assert!(!state.retryable);
        assert!(run(options(), false).await.is_err());
        state.in_flight = true;
        state.phase = "running".into();
        state.save(&root.join("out")).unwrap();
        assert_eq!(run(options(), true).await.unwrap(), 0);
        let state: State = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            (state.next, state.attempts, state.uncertain_attempts),
            (5, 6, 1)
        );
        assert_eq!(state.phase, "complete");
        assert_eq!(state.groups["all"].paired, [2, 3, 0, 0]);
        assert_eq!(state.groups["all"].outcomes["pass"], 2);
        assert_eq!(state.groups["all"].query_errors, 2);
        assert_eq!(state.groups["us_ca"].outcomes.values().sum::<usize>(), 5);
        assert_eq!(state.groups["all"].adapter_errors, 1);
        assert_eq!(
            state.groups["all"].normalization_failures["missing_results"],
            1
        );
        let mut changed = options();
        changed.only = Some(regex::Regex::new(".*").unwrap());
        assert!(run(changed, true).await.is_err());
        for entry in std::fs::read_dir(root.join("out")).unwrap() {
            let entry = entry.unwrap();
            assert!(matches!(
                entry.file_name().to_str(),
                Some("aggregate.json" | "report.md" | ".aggregate.lock")
            ));
            let bytes = std::fs::read_to_string(entry.path()).unwrap();
            for marker in [
                "PRIVATE_RESPONSE_MARKER",
                "PRIVATE_PROVIDER_ROAD",
                "geoCoordinates",
                "formatted_address",
                "houseNumber",
            ] {
                assert!(
                    !bytes.contains(marker),
                    "provider data leaked into aggregate artifacts"
                );
            }
            assert!(!bytes.contains(&std::env::var("PATH").unwrap()));
        }
        let requests = server.join().unwrap();
        for (request, expected) in requests.iter().zip([
            "first-input",
            "second-input",
            "third-input",
            "fourth-input",
            "fourth-input",
            "fifth-input",
        ]) {
            assert!(request.contains(expected));
        }
        assert!(retryable(
            &anyhow::anyhow!("google HTTP 503; stopped"),
            provider
        ));
        assert!(!retryable(
            &anyhow::anyhow!("google HTTP 429; stopped"),
            provider
        ));
        std::fs::remove_dir_all(root).unwrap();
    }
}
