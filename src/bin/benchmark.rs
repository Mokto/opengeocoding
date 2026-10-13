//! Connect JSON SearchAddress benchmark for comparing running service builds on the same index.
//!
//! ```text
//! benchmark --queries FILE --service base=http://127.0.0.1:18121 --service candidate=http://127.0.0.1:18122 \
//!   [--requests 2000] [--rounds 3] [--concurrency 1,8]
//! ```
//!
//! FILE holds one address per line. Every query is first sent once to each service and the full
//! responses are compared; then each round times `--requests` calls per service and concurrency,
//! alternating which service goes first. No failed request is retried.

use anyhow::{Context, Result, bail, ensure};
use reqwest::Url;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

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
    if base.ends_with("/opengeocoding.Geolocation")
        || base.ends_with("/geolocation.Geolocation")
        || base.ends_with("/nominatim_geolocation.NominatimGeolocation")
    {
        format!("{base}/SearchAddress")
    } else {
        format!("{base}/opengeocoding.Geolocation/SearchAddress")
    }
}

fn client(workers: usize) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .pool_max_idle_per_host(workers)
        .tcp_nodelay(true)
        .timeout(Duration::from_secs(30))
        .build()?)
}

async fn search(client: &reqwest::Client, url: &str, query: &str) -> Result<Value> {
    let response = client
        .post(url)
        .json(&json!({ "addressString": query }))
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

/// `calls` requests cycling through `queries` over `workers` connections: latencies in ms and
/// the number of failed requests.
async fn batch(
    client: &reqwest::Client,
    url: &str,
    queries: &Arc<Vec<String>>,
    calls: usize,
    workers: usize,
) -> (Vec<f64>, usize) {
    let next = Arc::new(AtomicUsize::new(0));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..workers {
        let (client, next, url, queries) = (
            client.clone(),
            next.clone(),
            url.to_string(),
            queries.clone(),
        );
        tasks.spawn(async move {
            let (mut latencies, mut errors) = (vec![], 0);
            loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= calls {
                    break;
                }
                let started = Instant::now();
                match search(&client, &url, &queries[i % queries.len()]).await {
                    Ok(_) => latencies.push(started.elapsed().as_secs_f64() * 1000.),
                    Err(_) => errors += 1,
                }
            }
            (latencies, errors)
        });
    }
    let (mut all, mut errors) = (vec![], 0);
    while let Some(Ok((latencies, failed))) = tasks.join_next().await {
        all.extend(latencies);
        errors += failed;
    }
    (all, errors)
}

struct Options {
    services: Vec<(String, String)>,
    queries: Vec<String>,
    requests: usize,
    rounds: usize,
    concurrency: Vec<usize>,
}

async fn compare(options: Options) -> Result<()> {
    let queries = Arc::new(options.queries);
    let check = client(1)?;
    let mut differing = 0;
    for query in queries.iter() {
        let mut first: Option<(&str, Value)> = None;
        for (name, base) in &options.services {
            let result = search(&check, &search_endpoint(base), query)
                .await
                .unwrap_or_else(|e| json!({ "error": e.to_string() }));
            match &first {
                None => first = Some((name, result)),
                Some((first_name, expected)) if *expected != result => {
                    if differing < 5 {
                        eprintln!("{query:?}: {first_name} {expected} vs {name} {result}");
                    }
                    differing += 1;
                }
                _ => {}
            }
        }
    }
    println!(
        "{}",
        json!({ "queries": queries.len(), "differing_responses": differing })
    );
    for &workers in &options.concurrency {
        for round in 0..options.rounds {
            let offset = round % options.services.len();
            let order = options.services[offset..]
                .iter()
                .chain(&options.services[..offset]);
            for (name, base) in order {
                // Alternating services can exceed a server's idle timeout: reconnect, warm untimed.
                let connections = client(workers)?;
                let url = search_endpoint(base);
                batch(&connections, &url, &queries, workers * 10, workers).await;
                let started = Instant::now();
                let (values, errors) =
                    batch(&connections, &url, &queries, options.requests, workers).await;
                let seconds = started.elapsed().as_secs_f64();
                ensure!(!values.is_empty(), "{name}: every request failed");
                println!(
                    "{}",
                    json!({
                        "service": name, "concurrency": workers, "round": round,
                        "calls": values.len(), "errors": errors,
                        "p50_ms": percentile(&values, 0.5), "p95_ms": percentile(&values, 0.95),
                        "p99_ms": percentile(&values, 0.99),
                        "mean_ms": values.iter().sum::<f64>() / values.len() as f64,
                        "rps": values.len() as f64 / seconds,
                    })
                );
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
        queries: vec![],
        requests: 2000,
        rounds: 3,
        concurrency: vec![1, 8],
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = || iter.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--service" => {
                let (name, url) = value()?.split_once('=').context("services use NAME=URL")?;
                ensure!(!name.is_empty(), "services use NAME=URL");
                ensure!(
                    options.services.iter().all(|(n, _)| n != name),
                    "service names must be distinct"
                );
                options.services.push((name.into(), endpoint(url)?));
            }
            "--queries" => {
                let path = value()?;
                options.queries = std::fs::read_to_string(path)
                    .with_context(|| path.clone())?
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(String::from)
                    .collect();
            }
            "--requests" => options.requests = value()?.parse()?,
            "--rounds" => options.rounds = value()?.parse()?,
            "--concurrency" => options.concurrency = list(value()?)?,
            other => bail!("unknown argument {other}"),
        }
    }
    ensure!(
        !options.services.is_empty()
            && !options.queries.is_empty()
            && options.requests > 0
            && options.rounds > 0
            && options.concurrency.iter().all(|c| *c > 0),
        "provide --service NAME=URL, a non-empty --queries FILE and positive --requests, \
         --rounds and --concurrency"
    );
    compare(options).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_and_percentiles() {
        for (base, service) in [
            ("http://localhost:11121", "opengeocoding.Geolocation"),
            (
                "http://localhost:11121/opengeocoding.Geolocation",
                "opengeocoding.Geolocation",
            ),
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
        assert!(
            endpoint("http://user@127.0.0.1:1").is_err() && endpoint("https://127.0.0.1").is_err()
        );
    }
}
