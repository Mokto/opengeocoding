//! Loopback serve_index mock and Connect JSON benchmark for comparing local service builds.
//!
//! ```text
//! benchmark --serve 18080
//! benchmark --service rust=http://127.0.0.1:18121 --service reference=http://127.0.0.1:18122 \
//!   [--mock-url http://127.0.0.1:18080] [--iterations 100] [--rounds 3] [--concurrency 1,8] [--delays 0,10]
//! ```
//!
//! Results compare full responses and backend request counts; no failed request is retried.

use anyhow::{Context, Result, bail, ensure};
use axum::{
    Router,
    body::Bytes,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
    routing::{get, post},
    serve::ListenerExt,
};
use reqwest::Url;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const SCENARIOS: [(&str, usize); 5] = [
    ("raw", 1),
    ("raw", 10),
    ("raw", 100),
    ("localized", 1),
    ("fallback", 1),
];

fn query(scenario: &str) -> &'static str {
    match scenario {
        "localized" => "München, Deutschland",
        "fallback" => "Paris, France, 75001",
        _ => "Copenhagen, Denmark",
    }
}

fn country(scenario: &str) -> &'static str {
    match scenario {
        "localized" => "de",
        "fallback" => "fr",
        _ => "dk",
    }
}

/// Pre-encoded index search results for a scenario.
fn hits(scenario: &str, count: usize, local: bool, postcode: &str) -> Bytes {
    let code = country(scenario);
    let city = match scenario {
        "localized" if local => "München",
        "localized" => "Munich",
        "fallback" => "Paris",
        _ => "Copenhagen",
    };
    let (name, region, point) = match code {
        "de" => ("Germany", "DE-BY", ["48.1372", "11.5756"]),
        "fr" => ("France", "FR-IDF", ["48.8566", "2.3522"]),
        _ => ("Denmark", "DK-84", ["55.6761", "12.5683"]),
    };
    let mut address =
        json!({ "country_code": code, "country": name, "city": city, "ISO3166-2-lvl4": region });
    if scenario == "fallback" {
        address["postcode"] = json!(postcode);
    }
    let results: Vec<_> = (0..count)
        .map(|i| {
            json!({ "display_name": city, "address": address, "lat": point[0], "lon": point[1],
                    "osm_id": i + 1, "osm_type": "node" })
        })
        .collect();
    Bytes::from(serde_json::to_vec(&results).expect("fixture JSON"))
}

struct Fixture {
    scenario: String,
    delay: Duration,
    english: Bytes,
    local: Bytes,
    wrong: Bytes,
}

#[derive(Clone)]
struct Mock {
    fixture: Arc<Mutex<Fixture>>,
    counts: Arc<Mutex<BTreeMap<String, u64>>>,
}

fn respond(status: StatusCode, body: impl Into<Bytes>) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.into(),
    )
        .into_response()
}

async fn control(State(mock): State<Mock>, body: Bytes) -> Response {
    let fixture = (|| {
        let config: Value = serde_json::from_slice(&body).ok()?;
        let scenario = config["scenario"].as_str()?.to_string();
        let count = config["candidates"].as_u64()? as usize;
        let delay = config["delay_ms"].as_f64()?;
        let valid = ["raw", "localized", "fallback"].contains(&scenario.as_str())
            && (1..=1000).contains(&count)
            && (0. ..=1000.).contains(&delay);
        valid.then(|| Fixture {
            english: hits(&scenario, count, false, "75001"),
            local: hits(&scenario, count, true, "75001"),
            wrong: hits(&scenario, count, false, "75004"),
            delay: Duration::from_secs_f64(delay / 1000.),
            scenario,
        })
    })();
    let Some(fixture) = fixture else {
        return respond(
            StatusCode::BAD_REQUEST,
            r#"{"error":"invalid fixture settings"}"#,
        );
    };
    *mock.fixture.lock().expect("fixture lock") = fixture;
    mock.counts.lock().expect("counts lock").clear();
    respond(StatusCode::OK, "{}")
}

async fn stats(State(mock): State<Mock>) -> Response {
    let counts = mock.counts.lock().expect("counts lock").clone();
    respond(
        StatusCode::OK,
        serde_json::to_vec(&counts).expect("stats JSON"),
    )
}

async fn search(
    State(mock): State<Mock>,
    uri: Uri,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Response {
    if uri.path().trim_end_matches('/').rsplit('/').next() != Some("search") {
        return respond(StatusCode::NOT_FOUND, "{}");
    }
    let q = Url::parse(&format!("http://mock/?{}", raw.unwrap_or_default()))
        .ok()
        .and_then(|u| {
            u.query_pairs()
                .find(|(k, _)| k == "q")
                .map(|(_, v)| v.into_owned())
        })
        .unwrap_or_default();
    let local = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        != Some("en");
    let (delay, data, key) = {
        let fixture = mock.fixture.lock().expect("fixture lock");
        let key = if fixture.scenario == "fallback" && !q.to_lowercase().starts_with("75001") {
            "wrong"
        } else if local {
            "local"
        } else {
            "english"
        };
        let data = match key {
            "wrong" => fixture.wrong.clone(),
            "local" => fixture.local.clone(),
            _ => fixture.english.clone(),
        };
        (fixture.delay, data, key)
    };
    *mock
        .counts
        .lock()
        .expect("counts lock")
        .entry(key.into())
        .or_default() += 1;
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    respond(StatusCode::OK, data)
}

async fn serve(port: u16) -> Result<()> {
    let mock = Mock {
        fixture: Arc::new(Mutex::new(Fixture {
            scenario: "raw".into(),
            delay: Duration::ZERO,
            english: hits("raw", 1, false, "75001"),
            local: hits("raw", 1, true, "75001"),
            wrong: hits("raw", 1, false, "75001"),
        })),
        counts: Default::default(),
    };
    let app = Router::new()
        .route(
            "/status",
            get(|| async {
                axum::Json(json!({"status": 0, "software_version": "overture-index"}))
            }),
        )
        .route("/__control", post(control))
        .route("/__stats", get(stats))
        .fallback(get(search))
        .with_state(mock);
    // Split headers and bodies otherwise add delayed-ACK latency to a loopback benchmark.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await?
        .tap_io(|tcp| {
            let _ = tcp.set_nodelay(true);
        });
    println!("Mock serve_index listening on 127.0.0.1:{port}");
    axum::serve(listener, app).await?;
    Ok(())
}

fn endpoint(url: &str) -> Result<String> {
    let parsed = Url::parse(url)?;
    ensure!(
        parsed.scheme() == "http"
            && parsed.host_str().is_some()
            && parsed.username().is_empty()
            && parsed.password().is_none()
            && parsed.query().is_none()
            && parsed.fragment().is_none(),
        "benchmark endpoints must be plain HTTP URLs without credentials, queries or fragments"
    );
    Ok(url.trim_end_matches('/').to_string())
}

fn search_endpoint(base: &str) -> String {
    if base.ends_with("/geolocation.Geolocation")
        || base.ends_with("/nominatim_geolocation.NominatimGeolocation")
    {
        format!("{base}/SearchAddress")
    } else {
        format!("{base}/geolocation.Geolocation/SearchAddress")
    }
}

fn client(workers: usize) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .pool_max_idle_per_host(workers)
        .tcp_nodelay(true)
        .timeout(Duration::from_secs(30))
        .build()?)
}

async fn request(client: &reqwest::Client, url: &str, body: Option<&Value>) -> Result<Value> {
    let request = match body {
        Some(body) => client.post(url).json(body),
        None => client.get(url),
    };
    let response = request
        .header("Connect-Protocol-Version", "1")
        .send()
        .await?;
    let status = response.status();
    let data = response.bytes().await?;
    ensure!(
        status.is_success(),
        "HTTP {status}: {}",
        String::from_utf8_lossy(&data[..data.len().min(200)])
    );
    Ok(serde_json::from_slice(&data)?)
}

fn percentile(values: &[f64], q: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() as f64 * q).ceil() as usize).saturating_sub(1)]
}

fn validate(result: &Value, scenario: &str) -> Result<()> {
    let details = &result["addressDetails"];
    ensure!(
        details["countryCode"] == country(scenario),
        "country: {result}"
    );
    ensure!(
        details["regions"].as_array().is_some_and(|r| !r.is_empty()),
        "regions: {result}"
    );
    ensure!(
        result["goodMatch"].as_bool().unwrap_or(false) == (scenario != "fallback"),
        "goodMatch: {result}"
    );
    if scenario == "fallback" {
        ensure!(details["postcode"] == "75001", "postcode: {result}");
    } else {
        let city = if scenario == "raw" {
            "Copenhagen"
        } else {
            "Munich"
        };
        ensure!(details["city"] == city, "city: {result}");
    }
    Ok(())
}

/// `calls` SearchAddress requests over `workers` concurrent connections: latencies and responses.
async fn batch(
    client: &reqwest::Client,
    base: &str,
    scenario: &str,
    calls: usize,
    workers: usize,
) -> Result<Vec<(f64, Value)>> {
    let next = Arc::new(AtomicUsize::new(0));
    let url = Arc::new(search_endpoint(base));
    let body = Arc::new(json!({ "addressString": query(scenario) }));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..workers {
        let (client, next, url, body, scenario) = (
            client.clone(),
            next.clone(),
            url.clone(),
            body.clone(),
            scenario.to_string(),
        );
        tasks.spawn(async move {
            let mut results = vec![];
            while next.fetch_add(1, Ordering::SeqCst) < calls {
                let started = Instant::now();
                let result = request(&client, &url, Some(&body)).await?;
                let elapsed = started.elapsed().as_secs_f64() * 1000.;
                validate(&result, &scenario)?;
                results.push((elapsed, result));
            }
            Ok::<_, anyhow::Error>(results)
        });
    }
    let mut all = vec![];
    while let Some(results) = tasks.join_next().await {
        all.extend(results??);
    }
    Ok(all)
}

struct Options {
    services: Vec<(String, String)>,
    mock: String,
    iterations: usize,
    rounds: usize,
    concurrency: Vec<usize>,
    delays: Vec<f64>,
}

async fn compare(options: Options) -> Result<()> {
    let control = client(1)?;
    for &delay in &options.delays {
        for (scenario, candidates) in SCENARIOS {
            request(
                &control,
                &format!("{}/__control", options.mock),
                Some(&json!({ "scenario": scenario, "candidates": candidates, "delay_ms": delay })),
            )
            .await?;
            for &workers in &options.concurrency {
                let mut expected: Option<Value> = None;
                for round in 0..options.rounds {
                    let offset = round % options.services.len();
                    let order = options.services[offset..]
                        .iter()
                        .chain(&options.services[..offset]);
                    for (name, base) in order {
                        // Alternating services can exceed a server's idle timeout; reconnect and warm outside timing.
                        let connections = client(workers)?;
                        for (_, result) in
                            batch(&connections, base, scenario, workers.max(10), workers).await?
                        {
                            let expected = expected.get_or_insert(result.clone());
                            ensure!(
                                result == *expected,
                                "{name} differs: {result} vs {expected}"
                            );
                        }
                        let before =
                            request(&control, &format!("{}/__stats", options.mock), None).await?;
                        let started = Instant::now();
                        let results =
                            batch(&connections, base, scenario, options.iterations, workers)
                                .await?;
                        let seconds = started.elapsed().as_secs_f64();
                        let after =
                            request(&control, &format!("{}/__stats", options.mock), None).await?;
                        let mut values = vec![];
                        for (elapsed, result) in results {
                            ensure!(
                                Some(&result) == expected.as_ref(),
                                "{name} differs: {result}"
                            );
                            values.push(elapsed);
                        }
                        let mut counts = BTreeMap::new();
                        for key in ["english", "local", "wrong"] {
                            let delta = after[key].as_i64().unwrap_or(0)
                                - before[key].as_i64().unwrap_or(0);
                            if delta != 0 || after.get(key).is_some() || before.get(key).is_some() {
                                counts.insert(key, delta);
                            }
                        }
                        let per_call = match scenario {
                            "raw" => 1,
                            "localized" => 2,
                            _ => 3,
                        };
                        let total: i64 = counts.values().sum();
                        ensure!(
                            total == (per_call * options.iterations) as i64,
                            "{scenario}: {counts:?}, expected {} backend calls",
                            per_call * options.iterations
                        );
                        println!(
                            "{}",
                            json!({
                                "service": name, "scenario": scenario, "candidates": candidates, "delay_ms": delay,
                                "concurrency": workers, "round": round, "calls": values.len(),
                                "p50_ms": percentile(&values, 0.5), "p95_ms": percentile(&values, 0.95),
                                "mean_ms": values.iter().sum::<f64>() / values.len() as f64,
                                "rps": values.len() as f64 / seconds, "mock_calls": counts,
                            })
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

fn list<T: std::str::FromStr>(value: &str) -> Result<Vec<T>>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    Ok(value.split(',').map(str::parse).collect::<Result<_, _>>()?)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut options = Options {
        services: vec![],
        mock: "http://127.0.0.1:18080".into(),
        iterations: 100,
        rounds: 3,
        concurrency: vec![1, 8],
        delays: vec![0., 10.],
    };
    let mut serve_port = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = || iter.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--serve" => serve_port = Some(value()?.parse()?),
            "--service" => {
                let (name, url) = value()?.split_once('=').context("services use NAME=URL")?;
                ensure!(!name.is_empty(), "services use NAME=URL");
                ensure!(
                    options.services.iter().all(|(n, _)| n != name),
                    "service names must be distinct"
                );
                options.services.push((name.into(), endpoint(url)?));
            }
            "--mock-url" => options.mock = endpoint(value()?)?,
            "--iterations" => options.iterations = value()?.parse()?,
            "--rounds" => options.rounds = value()?.parse()?,
            "--concurrency" => options.concurrency = list(value()?)?,
            "--delays" => options.delays = list(value()?)?,
            other => bail!("unknown argument {other}"),
        }
    }
    if let Some(port) = serve_port {
        return serve(port).await;
    }
    ensure!(
        !options.services.is_empty() && options.iterations > 0 && options.rounds > 0,
        "provide --service NAME=URL and positive --iterations/--rounds"
    );
    ensure!(
        options.concurrency.iter().all(|c| *c > 0)
            && options.delays.iter().all(|d| (0. ..=1000.).contains(d)),
        "invalid concurrency or mock delay"
    );
    compare(options).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_validation_and_percentiles() {
        for (base, service) in [
            ("http://localhost:11121", "geolocation.Geolocation"),
            (
                "http://localhost:11121/geolocation.Geolocation",
                "geolocation.Geolocation",
            ),
            (
                "http://localhost:11121/nominatim_geolocation.NominatimGeolocation",
                "nominatim_geolocation.NominatimGeolocation",
            ),
        ] {
            assert_eq!(
                search_endpoint(base),
                format!("http://localhost:11121/{service}/SearchAddress")
            );
        }
        assert_eq!(percentile(&[1., 2., 3., 4.], 0.5), 2.);
        let parse = |b: Bytes| serde_json::from_slice::<Value>(&b).unwrap();
        assert_eq!(
            parse(hits("raw", 100, false, "75001"))
                .as_array()
                .unwrap()
                .len(),
            100
        );
        assert_eq!(
            parse(hits("localized", 1, true, "75001"))[0]["address"]["city"],
            "München"
        );
        assert_eq!(
            parse(hits("fallback", 1, false, "75004"))[0]["address"]["postcode"],
            "75004"
        );
        let good = json!({"goodMatch": true, "addressDetails": {"countryCode": "dk", "city": "Copenhagen", "regions": [{}]}});
        validate(&good, "raw").unwrap();
        assert!(validate(&json!({}), "raw").is_err());
        assert!(
            endpoint("http://user@127.0.0.1:1").is_err() && endpoint("https://127.0.0.1").is_err()
        );
    }
}
