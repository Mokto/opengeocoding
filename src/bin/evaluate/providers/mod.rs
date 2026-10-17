use crate::{
    model::{Case, Record, load_corpus, load_suite},
    report::{load_side, summarize},
    run::{classify, sha256_files, utc_now},
    score::{score, summary_of},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fmt::Write as _,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

mod arcgis;
mod geoapify;
mod geocodio;
mod google;
mod here;
mod locationiq;
mod mapbox;
mod opencage;
mod tomtom;

pub struct Provider {
    pub name: &'static str,
    label: &'static str,
    credential_env: &'static str,
    auth_parameter: &'static str,
    query_error_statuses: &'static [u16],
    not_found_status: Option<u16>,
    interval_ms: u64,
    storage_terms: Option<&'static str>,
    request: fn(&str) -> Value,
    normalize: fn(&Value) -> Result<Value>,
}

static PROVIDERS: [Provider; 9] = [
    arcgis::PROVIDER,
    geoapify::PROVIDER,
    geocodio::PROVIDER,
    locationiq::PROVIDER,
    opencage::PROVIDER,
    mapbox::PROVIDER,
    google::PROVIDER,
    here::PROVIDER,
    tomtom::PROVIDER,
];

pub fn named(name: &str) -> Result<&'static Provider> {
    PROVIDERS.iter().find(|p| p.name == name).with_context(|| {
        format!(
            "unknown provider {name}; expected {}",
            PROVIDERS
                .iter()
                .map(|p| p.name)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

fn full_address(name: &str, lat: &Value, lon: &Value, details: Value) -> Result<Value> {
    let lat = lat.as_f64().context("missing latitude")?;
    let lon = lon.as_f64().context("missing longitude")?;
    ensure!(
        (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon),
        "invalid point"
    );
    let country = crate::score::text(&details, "countryCode");
    ensure!(
        country.len() == 2 && country.bytes().all(|b| b.is_ascii_alphabetic()),
        "invalid country code"
    );
    let postcode = crate::score::text(&details, "postcode");
    let postal = !postcode.is_empty()
        && ["road", "houseNumber", "city", "suburb", "state", "county"]
            .iter()
            .all(|key| crate::score::text(&details, key).is_empty());
    let name = if postal && !name.starts_with(postcode) {
        format!("{postcode}, {name}")
    } else {
        name.into()
    };
    Ok(
        json!({"displayName": name, "geoCoordinates": {"lat": lat, "lon": lon}, "addressDetails": details}),
    )
}

pub struct Options {
    pub provider: &'static Provider,
    pub storage_authorized: bool,
    pub corpus: Option<PathBuf>,
    pub out: PathBuf,
    pub cache: PathBuf,
    pub only: Option<regex::Regex>,
    pub fetch: bool,
    pub budget: usize,
    pub compare: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    request: Value,
    fetched_at: String,
    status: u16,
    response: Value,
}

fn key(request: &Value) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, request.to_string().as_bytes());
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

fn read_entry(provider: &Provider, cache: &Path, request: &Value) -> Result<Option<Entry>> {
    let path = cache.join(format!("{}.json", key(request)));
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let entry: Entry = serde_json::from_slice(&bytes)
        .with_context(|| format!("invalid cache entry {}", path.display()))?;
    ensure!(
        entry.request == *request,
        "cache request mismatch: {}",
        path.display()
    );
    ensure!(
        entry.status == 200
            || provider.query_error_statuses.contains(&entry.status)
            || provider.not_found_status == Some(entry.status),
        "invalid cached status"
    );
    Ok(Some(entry))
}

fn save_entry(cache: &Path, entry: &Entry) -> Result<()> {
    let path = cache.join(format!("{}.json", key(&entry.request)));
    let pending = path.with_extension("json.tmp");
    let mut file = File::create(&pending)?;
    serde_json::to_writer(&mut file, entry)?;
    file.sync_all()?;
    std::fs::rename(pending, path)?;
    Ok(())
}

async fn fetch(
    provider: &Provider,
    client: &reqwest::Client,
    token: &str,
    request: Value,
) -> Result<Entry> {
    let params: Vec<_> = request
        .as_object()
        .context("request object")?
        .iter()
        .filter(|(k, _)| k.as_str() != "endpoint")
        .map(|(k, v)| {
            (
                k.as_str(),
                v.as_str().map_or_else(|| v.to_string(), String::from),
            )
        })
        .chain(std::iter::once((provider.auth_parameter, token.to_owned())))
        .collect();
    let response = client
        .get(request["endpoint"].as_str().context("missing endpoint")?)
        .query(&params)
        .send()
        .await
        .map_err(|error| error.without_url())
        .with_context(|| format!("{} request failed (not retried)", provider.name))?;
    let status = response.status().as_u16();
    ensure!(
        status == 200
            || provider.query_error_statuses.contains(&status)
            || provider.not_found_status == Some(status),
        "{} HTTP {status}; stopped without retrying",
        provider.name
    );
    let response: Value = response
        .json()
        .await
        .map_err(|error| error.without_url())
        .context("invalid provider JSON")?;
    Ok(Entry {
        request,
        fetched_at: utc_now(),
        status,
        response,
    })
}

fn paired(
    provider: &Provider,
    cases: &[Case],
    records: &[Record],
    baseline: &Path,
) -> Result<String> {
    let (_, base) = load_side(baseline)?;
    let mut counts = [0usize; 4];
    let mut differences = String::new();
    for (case, record) in cases.iter().zip(records) {
        if case.expect.is_none() {
            continue;
        }
        let Some(before) = base.get(&case.id) else {
            continue;
        };
        ensure!(
            before.query == case.query,
            "baseline query differs for {}",
            case.id
        );
        let class = if let Some(response) = &before.response {
            score(case.expect.as_ref(), response, false).class
        } else {
            ensure!(
                before.error.is_some(),
                "baseline {} has no full response",
                case.id
            );
            "error"
        };
        if class == "unscored" {
            continue;
        }
        let index = match (class == "pass", record.class == "pass") {
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 3,
        };
        counts[index] += 1;
        if class != record.class {
            let _ = writeln!(
                differences,
                "- `{}`: baseline **{class}**, {} **{}**",
                case.id, provider.label, record.class
            );
        }
    }
    ensure!(
        counts.iter().sum::<usize>() > 0,
        "no paired scored cases with the baseline"
    );
    Ok(format!(
        "\n## Paired forward comparison\n\nBoth sides re-scored against the current expectations, ignoring confidence. Only identical queries with saved responses/errors are paired.\n\n| Outcome | Cases |\n|---|---:|\n| Both pass | {} |\n| Baseline only passes | {} |\n| {} only passes | {} |\n| Neither passes | {} |\n\n### Different classes\n\n{}",
        counts[0], counts[1], provider.label, counts[2], counts[3], differences,
    ))
}

pub async fn run(options: Options) -> Result<i32> {
    let started_at = utc_now();
    let provider = options.provider;
    ensure!(
        options.fetch || options.budget == 0,
        "--budget requires --fetch"
    );
    ensure!(
        !options.fetch || options.budget > 0,
        "--fetch requires an explicit positive --budget"
    );
    let cases = match &options.corpus {
        Some(path) => load_corpus(path)?,
        None => load_suite()?,
    };
    let excluded = cases
        .iter()
        .filter(|c| c.expect.as_ref().is_some_and(|e| e.error.is_some()))
        .count();
    let cases: Vec<_> = cases
        .into_iter()
        .filter(|c| c.expect.as_ref().is_none_or(|e| e.error.is_none()))
        .filter(|c| options.only.as_ref().is_none_or(|re| re.is_match(&c.id)))
        .collect();
    ensure!(!cases.is_empty(), "no cases selected");
    let mut ids = HashSet::new();
    ensure!(
        cases.iter().all(|c| ids.insert(&c.id)),
        "duplicate case ids"
    );
    let _lock = if options.fetch {
        std::fs::create_dir_all(&options.cache)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(options.cache.join(".fetch.lock"))?;
        file.try_lock()
            .context("another fetch holds this cache lock")?;
        Some(file)
    } else {
        None
    };
    let mut missing = HashSet::new();
    for case in &cases {
        let req = (provider.request)(&case.query);
        if read_entry(provider, &options.cache, &req)?.is_none() {
            missing.insert(key(&req));
        }
    }
    eprintln!(
        "{} cases, {} distinct uncached requests; offline={}",
        cases.len(),
        missing.len(),
        !options.fetch
    );
    ensure!(
        missing.is_empty() || options.fetch,
        "offline cache misses: {}; use --fetch --budget N to authorize provider requests",
        missing.len()
    );
    ensure!(
        !options.fetch || missing.len() <= options.budget,
        "{} missing requests exceed --budget {}; no requests sent",
        missing.len(),
        options.budget
    );
    let client = if missing.is_empty() {
        None
    } else {
        if let Some(terms) = provider.storage_terms {
            ensure!(
                options.storage_authorized,
                "{} response storage requires an applicable agreement authorizing this offline evaluation; verify {terms}, then pass --storage-authorized",
                provider.label
            );
        }
        let token = std::env::var(provider.credential_env).with_context(|| {
            format!(
                "set {} through your secret manager",
                provider.credential_env
            )
        })?;
        ensure!(
            !token.trim().is_empty(),
            "{} is empty",
            provider.credential_env
        );
        Some((
            reqwest::Client::builder()
                .retry(reqwest::retry::never())
                .timeout(Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            token,
        ))
    };
    let mut fetched = 0;
    let mut records = vec![];
    let mut cache_files = BTreeMap::new();
    for case in &cases {
        let req = (provider.request)(&case.query);
        let cache_path = options.cache.join(format!("{}.json", key(&req)));
        let entry = match read_entry(provider, &options.cache, &req)? {
            Some(entry) => entry,
            None => {
                let (client, token) = client.as_ref().context("offline cache miss")?;
                if fetched > 0 {
                    tokio::time::sleep(Duration::from_millis(provider.interval_ms)).await;
                }
                let entry = fetch(provider, client, token, req).await?;
                save_entry(&options.cache, &entry)?;
                fetched += 1;
                entry
            }
        };
        cache_files.insert(cache_path, ());
        let mut record = Record {
            id: case.id.clone(),
            query: case.query.clone(),
            ..Default::default()
        };
        if !provider.query_error_statuses.contains(&entry.status) {
            let response = (provider.normalize)(&entry.response)?;
            classify(case, Ok(&response), &mut record, false);
            record.summary = Some(summary_of(&response));
            record.response = Some(response);
        } else {
            classify(
                case,
                Err(&format!("http_{}", entry.status)),
                &mut record,
                false,
            );
        }
        records.push(record);
        if records.len() % 50 == 0 {
            eprintln!("{}/{} cases; {fetched} fetched", records.len(), cases.len());
        }
    }
    let files = options
        .corpus
        .clone()
        .map_or_else(crate::model::suite, |p| vec![p]);
    let manifest = json!({
        "suite": format!("{}-forward", provider.name), "started_at": started_at, "cases": cases.len(),
        "corpus_sha256": sha256_files(&files)?,
        "cache_sha256": sha256_files(&cache_files.into_keys().collect::<Vec<_>>())?,
        "scoring": {"confidence": false},
        "request_settings": (provider.request)(""), "fetched": fetched,
        "storage_authorized": options.storage_authorized, "interval_ms": provider.interval_ms,
        "excluded_contract_cases": excluded,
        "backend": {"label": provider.label, "status": {"software_version": provider.name, "data_updated": "unknown"}},
    });
    let (report, _) = summarize(&cases, &records, &manifest);
    let mut report = format!(
        "Forward geocoding only; confidence is not scored. Known failures remain failures. This existing corpus is diagnostic, not an unbiased accuracy estimate. Native responses and collection dates are in the private cache. No request-latency comparison is made.\n\n{report}"
    );
    if let Some(baseline) = &options.compare {
        report += &paired(provider, &cases, &records, baseline)?;
    }
    std::fs::create_dir_all(&options.out)?;
    let mut output = File::create(options.out.join("results.jsonl"))?;
    for record in &records {
        serde_json::to_writer(&mut output, record)?;
        writeln!(output)?;
    }
    std::fs::write(
        options.out.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    std::fs::write(options.out.join("report.md"), &report)?;
    println!(
        "{fetched} requests fetched; report: {}",
        options.out.join("report.md").display()
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(kind: &str) -> Value {
        json!({"features": [{"properties": {
            "feature_type": kind, "name": "12 Example Road", "full_address": "12 Example Road, Exampletown",
            "context": {
                "address": {"address_number": "12"}, "street": {"name": "Example Road"},
                "postcode": {"name": "12345"}, "place": {"name": "Exampletown"},
                "region": {"region_code_full": "US-CA"}, "country": {"country_code": "US"}
            }
        }, "geometry": {"coordinates": [-122.0, 37.0]}}]})
    }

    #[test]
    fn providers_have_isolated_requests_without_credentials() {
        let mut keys = HashSet::new();
        for provider in &PROVIDERS {
            let request = (provider.request)("Exampletown");
            assert!(request.get(provider.auth_parameter).is_none());
            assert!(keys.insert(key(&request)));
            assert!(
                request["endpoint"]
                    .as_str()
                    .unwrap()
                    .starts_with("https://")
            );
        }
        assert!(named("unknown").is_err());
    }

    #[tokio::test]
    async fn provider_specific_http_outcomes_are_cached_without_hiding_errors() {
        let dir =
            std::env::temp_dir().join(format!("locationiq-cache-test-{}", std::process::id()));
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let provider = named("locationiq").unwrap();
        let case = Case {
            id: "missing".into(),
            query: "Made-up missing address".into(),
            expect: Some(crate::score::Expect {
                level: "none".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let corpus = dir.join("cases.jsonl");
        std::fs::write(&corpus, serde_json::to_string(&case).unwrap() + "\n").unwrap();
        let request = (provider.request)(&case.query);
        save_entry(
            &cache,
            &Entry {
                request: request.clone(),
                fetched_at: String::new(),
                status: 404,
                response: json!({"error": "Unable to geocode"}),
            },
        )
        .unwrap();
        assert!(read_entry(named("google").unwrap(), &cache, &request).is_err());
        run(Options {
            provider,
            storage_authorized: false,
            corpus: Some(corpus),
            out: dir.join("out"),
            cache,
            only: None,
            fetch: false,
            budget: 0,
            compare: None,
        })
        .await
        .unwrap();
        let records: Vec<Record> =
            crate::model::load_jsonl(&dir.join("out/results.jsonl")).unwrap();
        assert_eq!(records[0].class, "pass");
        assert_eq!(records[0].response, Some(json!({})));
        let provider = named("opencage").unwrap();
        let request = (provider.request)("");
        save_entry(
            &dir.join("cache"),
            &Entry {
                request: request.clone(),
                fetched_at: String::new(),
                status: 400,
                response: json!({"status": {"code": 400}, "results": []}),
            },
        )
        .unwrap();
        assert!(read_entry(named("google").unwrap(), &dir.join("cache"), &request).is_err());
        std::fs::write(
            dir.join("cases.jsonl"),
            serde_json::to_string(&Case {
                query: String::new(),
                ..case
            })
            .unwrap()
                + "\n",
        )
        .unwrap();
        run(Options {
            provider,
            storage_authorized: false,
            corpus: Some(dir.join("cases.jsonl")),
            out: dir.join("invalid"),
            cache: dir.join("cache"),
            only: None,
            fetch: false,
            budget: 0,
            compare: None,
        })
        .await
        .unwrap();
        let records: Vec<Record> =
            crate::model::load_jsonl(&dir.join("invalid/results.jsonl")).unwrap();
        assert_eq!(records[0].class, "error");
        assert_eq!(records[0].error.as_deref(), Some("http_400"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn offline_cache_replay_deduplicates_and_never_requires_a_token() {
        let dir = std::env::temp_dir().join(format!("mapbox-test-{}", std::process::id()));
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let corpus = dir.join("cases.jsonl");
        let case = Case {
            id: "one".into(),
            query: "Exampletown".into(),
            expect: Some(crate::score::Expect {
                level: "locality".into(),
                country: vec!["us".into()],
                good_match: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        };
        let other = Case {
            id: "two".into(),
            query: "Other town".into(),
            ..case.clone()
        };
        let write_cases = |second: &Case| {
            std::fs::write(
                &corpus,
                format!(
                    "{}\n{}\n",
                    serde_json::to_string(&case).unwrap(),
                    serde_json::to_string(second).unwrap()
                ),
            )
            .unwrap()
        };
        write_cases(&other);
        let provider = named("mapbox").unwrap();
        let options = |fetch, budget| Options {
            provider,
            storage_authorized: false,
            corpus: Some(corpus.clone()),
            cache: cache.clone(),
            out: dir.join("out"),
            only: None,
            fetch,
            budget,
            compare: None,
        };
        assert!(
            run(options(false, 0))
                .await
                .unwrap_err()
                .to_string()
                .contains("offline cache misses")
        );
        assert!(run(options(true, 0)).await.is_err());
        for provider in PROVIDERS.iter().filter(|p| p.storage_terms.is_some()) {
            let mut restricted = options(true, 2);
            restricted.provider = provider;
            assert!(
                run(restricted)
                    .await
                    .unwrap_err()
                    .to_string()
                    .contains("--storage-authorized")
            );
        }
        assert!(
            run(options(true, 1))
                .await
                .unwrap_err()
                .to_string()
                .contains("exceed --budget")
        );
        let duplicate = Case {
            query: case.query.clone(),
            ..other
        };
        write_cases(&duplicate);
        let req = (provider.request)(&case.query);
        assert_ne!(key(&req), key(&(provider.request)("Other town")));
        assert_eq!(req["permanent"], true);
        assert_eq!(req["autocomplete"], false);
        save_entry(
            &cache,
            &Entry {
                request: req.clone(),
                fetched_at: "2026-01-01T00:00:00Z".into(),
                status: 200,
                response: response("place"),
            },
        )
        .unwrap();
        assert_eq!(run(options(false, 0)).await.unwrap(), 0);
        assert_eq!(run(options(true, 1)).await.unwrap(), 0);
        let manifest: Value =
            serde_json::from_slice(&std::fs::read(dir.join("out/manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest["fetched"], 0);
        assert_eq!(manifest["scoring"]["confidence"], false);
        let mut records: Vec<Record> =
            crate::model::load_jsonl(&dir.join("out/results.jsonl")).unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.iter().all(|r| r.class == "pass"));
        crate::rescore(&[case.clone(), duplicate.clone()], &mut records, false);
        assert!(records.iter().all(|r| r.class == "pass"));
        let baseline = dir.join("baseline.json");
        std::fs::write(
            &baseline,
            json!({"manifest": {}, "results": records}).to_string(),
        )
        .unwrap();
        let paired_report =
            paired(provider, &[case.clone(), duplicate], &records, &baseline).unwrap();
        assert!(paired_report.contains("| Both pass | 2 |"));
        let mut changed = req.clone();
        changed["language"] = "da".into();
        assert_ne!(key(&req), key(&changed));
        assert!(read_entry(provider, &cache, &changed).unwrap().is_none());
        save_entry(
            &cache,
            &Entry {
                request: req.clone(),
                fetched_at: String::new(),
                status: 422,
                response: json!({"message": "Query too long"}),
            },
        )
        .unwrap();
        assert_eq!(run(options(false, 0)).await.unwrap(), 0);
        let records: Vec<Record> =
            crate::model::load_jsonl(&dir.join("out/results.jsonl")).unwrap();
        assert!(
            records
                .iter()
                .all(|r| r.error.as_deref() == Some("http_422"))
        );
        std::fs::write(cache.join(format!("{}.json", key(&req))), "broken").unwrap();
        assert!(
            run(options(true, 1))
                .await
                .unwrap_err()
                .to_string()
                .contains("invalid cache entry")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
