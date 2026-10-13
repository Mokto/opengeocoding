//! Model-backed RPC integration test against a `build_index` output with world coverage
//! (Copenhagen, Munich, Paris). Ignored by default:
//!
//! ```text
//! INDEX_DIR=out cargo test --test rpc -- --ignored --nocapture
//! SERVICE_IMAGE=opengeocoding LIBPOSTAL_MODELS=$PWD/models/libpostal INDEX_DIR=out cargo test --test rpc -- --ignored --nocapture
//! ```
//!
//! With grpcurl installed, legacy gRPC is exercised too; its known hang fails the test with a
//! bounded timeout rather than being skipped (see README).

use anyhow::{Context, Result, ensure};
use buffa::Message;
use opengeocoding::proto::opengeocoding::{
    EmptyRequest, FullAddress, GeoCoordinates, SearchAddressTask,
};
use reqwest::{StatusCode, header};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::process::{Child, Command};

const SERVICE: &str = "opengeocoding.Geolocation";

struct Env {
    client: reqwest::Client,
    child: Option<Child>,
    rpc: String,
    metrics: String,
    index: PathBuf,
}

impl Env {
    async fn rpc(&self, method: &str, body: Vec<u8>, proto: bool) -> Result<(StatusCode, Vec<u8>)> {
        let response = self
            .client
            .post(format!("{}/{SERVICE}/{method}", self.rpc))
            .header(
                header::CONTENT_TYPE,
                if proto {
                    "application/proto"
                } else {
                    "application/json"
                },
            )
            .header("Connect-Protocol-Version", "1")
            .header("x-organization-id", "rpc-test")
            .timeout(Duration::from_secs(30))
            .body(body)
            .send()
            .await?;
        Ok((response.status(), response.bytes().await?.to_vec()))
    }

    async fn json(&self, method: &str, payload: Value) -> Result<Value> {
        let (status, body) = self
            .rpc(method, payload.to_string().into_bytes(), false)
            .await?;
        ensure!(
            status.is_success(),
            "{method} returned {status}: {}",
            String::from_utf8_lossy(&body)
        );
        Ok(serde_json::from_slice(&body)?)
    }

    async fn search(&self, address: &str) -> Result<Value> {
        self.json("SearchAddress", json!({ "addressString": address }))
            .await
    }
}

fn which(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

async fn grpcurl(args: &[&str]) -> Result<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new("grpcurl")
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("grpcurl timed out")??;
    ensure!(
        output.status.success(),
        "grpcurl failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

async fn start(env: &mut Env, image: Option<&str>) -> Result<()> {
    let port = |listener: std::net::TcpListener| listener.local_addr().map(|a| a.port());
    let rpc_port = port(std::net::TcpListener::bind("127.0.0.1:0")?)?;
    let metrics_port = port(std::net::TcpListener::bind("127.0.0.1:0")?)?;
    env.rpc = format!("http://127.0.0.1:{rpc_port}");
    env.metrics = format!("http://127.0.0.1:{metrics_port}/metrics");
    let mut settings = vec![
        ("INDEX_DIR", env.index.to_string_lossy().into()),
        ("RPC_PORT", rpc_port.to_string()),
        ("METRICS_PORT", metrics_port.to_string()),
        ("SENTRY_DSN", String::new()),
    ];
    let mut command = if let Some(image) = image {
        for (key, value) in &mut settings {
            if *key == "INDEX_DIR" {
                *value = "/data/index".into();
            }
        }
        let mut command = Command::new("docker");
        command
            .args(["run", "--rm"])
            .args(["-p", &format!("127.0.0.1:{rpc_port}:{rpc_port}")])
            .args(["-p", &format!("127.0.0.1:{metrics_port}:{metrics_port}")])
            .args([
                "-v",
                &format!(
                    "{}:/data/index:ro",
                    std::fs::canonicalize(&env.index)?.display()
                ),
            ]);
        if let Some(models) = std::env::var_os("LIBPOSTAL_MODELS") {
            let models = std::fs::canonicalize(models)?;
            command.args(["-v", &format!("{}:/data/libpostal:ro", models.display())]);
        }
        for (key, value) in &settings {
            command.args(["-e", &format!("{key}={value}")]);
        }
        command.arg(image);
        command
    } else {
        let binary = std::env::var("SERVICE_BINARY")
            .unwrap_or_else(|_| env!("CARGO_BIN_EXE_opengeocoding").into());
        let mut command = Command::new(binary);
        command.envs(settings);
        command
    };
    let child = env.child.insert(command.kill_on_drop(true).spawn()?);
    // loading the index and models takes seconds
    for _ in 0..900 {
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("service exited: {status}");
        }
        let ready = env
            .client
            .get(format!("{}/healthz", env.rpc))
            .timeout(Duration::from_secs(1))
            .send()
            .await;
        if ready.is_ok_and(|r| r.status().is_success()) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    anyhow::bail!("service did not become ready")
}

async fn checks(env: &mut Env) -> Result<()> {
    let found = env.search("Copenhagen, Denmark").await?;
    ensure!(found["addressDetails"]["countryCode"] == "dk", "{found}");
    let region = &found["addressDetails"]["regions"][0];
    ensure!(
        region["adminCode"] == "84" && region["adminLevel"] == 4 && region["countryCode"] == "dk",
        "{found}"
    );
    let local_name = region["localName"].as_str().unwrap_or("").to_owned();
    ensure!(!local_name.is_empty(), "{found}");
    let task = SearchAddressTask {
        address_string: "Copenhagen, Denmark".into(),
        ..Default::default()
    };
    let (status, body) = env.rpc("SearchAddress", task.encode_to_vec(), true).await?;
    ensure!(
        status.is_success(),
        "protobuf SearchAddress returned {status}"
    );
    let decoded = FullAddress::decode_from_slice(&body)?;
    let details = decoded
        .address_details
        .as_option()
        .context("address details")?;
    ensure!(
        details.city == "Copenhagen"
            && details
                .regions
                .first()
                .is_some_and(|r| r.local_name == local_name)
    );
    println!("PASS: Connect JSON and protobuf search with region enrichment");

    for (method, json_body, proto_body) in [
        (
            "SearchAddress",
            json!({"addressString": "Copenhagen, Denmark"}),
            task.encode_to_vec(),
        ),
        (
            "ResolveRegion",
            json!({"lat": 55.676098, "lon": 12.568337}),
            GeoCoordinates {
                lat: 55.676098,
                lon: 12.568337,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RefreshRegionMap",
            json!({}),
            EmptyRequest::default().encode_to_vec(),
        ),
        (
            "GetRegionMap",
            json!({}),
            EmptyRequest::default().encode_to_vec(),
        ),
        (
            "SearchAddress",
            json!({"addressString": "bad\u{0}address"}),
            SearchAddressTask {
                address_string: "bad\0address".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ResolveRegion",
            json!({"lat": 100, "lon": 0}),
            GeoCoordinates {
                lat: 100.,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
    ] {
        for proto in [false, true] {
            let body = if proto {
                proto_body.clone()
            } else {
                json_body.to_string().into_bytes()
            };
            let (expected_status, expected) = env.rpc(method, body.clone(), proto).await?;
            for service in [
                "geolocation.Geolocation",
                "nominatim_geolocation.NominatimGeolocation",
            ] {
                let response = env
                    .client
                    .post(format!("{}/{service}/{method}", env.rpc))
                    .header(
                        header::CONTENT_TYPE,
                        if proto {
                            "application/proto"
                        } else {
                            "application/json"
                        },
                    )
                    .header("Connect-Protocol-Version", "1")
                    .body(body.clone())
                    .send()
                    .await?;
                ensure!(
                    response.status() == expected_status,
                    "{service}/{method}: status differs"
                );
                let actual = response.bytes().await?;
                if proto && expected_status.is_success() {
                    ensure!(
                        actual.as_ref() == expected,
                        "{service}/{method}: protobuf answer differs"
                    );
                } else {
                    ensure!(
                        serde_json::from_slice::<Value>(&actual)?
                            == serde_json::from_slice::<Value>(&expected)?,
                        "{service}/{method}: JSON answer differs"
                    );
                }
            }
        }
    }
    println!("PASS: both legacy RPC aliases match canonical JSON/protobuf responses and errors");

    let found = env.search("München, Deutschland").await?;
    ensure!(found["addressDetails"]["city"] == "Munich", "{found}");
    let found = env.search("Paris, France, 75001").await?;
    ensure!(
        found["addressDetails"]["countryCode"] == "fr"
            && found["addressDetails"]["postcode"] == "75001",
        "{found}"
    );
    let found = env.search("Qwertyuiop 12345").await?;
    ensure!(found == json!({}), "{found}");
    println!("PASS: local-language corroboration, postcode match and unmatched-part rejection");

    let regions = env
        .json("ResolveRegion", json!({"lat": 55.676098, "lon": 12.568337}))
        .await?;
    ensure!(regions["regions"][0]["adminCode"] == "84", "{regions}");
    ensure!(env.json("RefreshRegionMap", json!({})).await? == json!({}));
    let served = env.json("GetRegionMap", json!({})).await?;
    let served = served["regionMap"].as_str().unwrap_or("");
    ensure!(served.contains(&local_name), "{served}");
    ensure!(
        env.json("GetRegionMap", json!({})).await?["regionMap"] == served,
        "the region map changed between reads"
    );
    println!("PASS: reverse lookup and region map");

    for (method, payload) in [
        ("SearchAddress", json!({"addressString": "bad\u{0}address"})),
        ("ResolveRegion", json!({"lat": 100, "lon": 0})),
    ] {
        let (status, body) = env
            .rpc(method, payload.to_string().into_bytes(), false)
            .await?;
        ensure!(
            status == StatusCode::BAD_REQUEST,
            "{method} accepted invalid input"
        );
        ensure!(serde_json::from_slice::<Value>(&body)?["code"] == "invalid_argument");
    }
    let shared = Arc::new(tokio::sync::Semaphore::new(16));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..64 {
        let (client, url, permit) = (
            env.client.clone(),
            format!("{}/{SERVICE}/SearchAddress", env.rpc),
            shared.clone(),
        );
        tasks.spawn(async move {
            let _permit = permit.acquire().await?;
            let response: Value = client
                .post(url)
                .header("Connect-Protocol-Version", "1")
                .json(&json!({"addressString": "Copenhagen, Denmark"}))
                .send()
                .await?
                .json()
                .await?;
            ensure!(
                response["addressDetails"]["countryCode"] == "dk",
                "{response}"
            );
            Ok::<_, anyhow::Error>(())
        });
    }
    while let Some(result) = tasks.join_next().await {
        result??;
    }
    println!("PASS: invalid-input errors and 64 concurrent requests");

    if which("grpcurl") {
        let address = env.rpc.trim_start_matches("http://").to_string();
        let services = grpcurl(&["-plaintext", "-max-time", "5", &address, "list"]).await?;
        ensure!(
            services.lines().any(|service| service == SERVICE),
            "{services}"
        );
        ensure!(
            !services.lines().any(|service| matches!(
                service,
                "geolocation.Geolocation" | "nominatim_geolocation.NominatimGeolocation"
            )),
            "reflection exposes legacy services: {services}"
        );
        let body = grpcurl(&[
            "-plaintext",
            "-max-time",
            "5",
            "-d",
            r#"{"addressString":"Copenhagen, Denmark"}"#,
            &address,
            &format!("{SERVICE}/SearchAddress"),
        ])
        .await?;
        ensure!(serde_json::from_str::<Value>(&body)?["addressDetails"]["countryCode"] == "dk");
        println!("PASS: legacy gRPC and reflection");
    } else {
        println!("NOT RUN: legacy gRPC/reflection; install grpcurl to exercise these checks");
    }

    let metrics = env.client.get(&env.metrics).send().await?.text().await?;
    ensure!(metrics.contains("nominatim_geolocation_query_tier_total"));
    ensure!(
        metrics.contains("grpc_requests_total")
            && metrics.contains(r#"grpc_service="opengeocoding.Geolocation""#)
            && metrics.contains(r#"organization_id="rpc-test""#)
    );
    let mut child = env.child.take().context("service process")?;
    let pid = child.id().context("service pid")?;
    std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()?;
    let status = tokio::time::timeout(Duration::from_secs(15), child.wait())
        .await
        .context("SIGTERM shutdown timed out")??;
    ensure!(status.success(), "service exited with {status}");
    println!("PASS: metrics and SIGTERM shutdown");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs an index (INDEX_DIR) and libpostal models; run with --ignored"]
async fn rpc_contracts() {
    let image = std::env::var("SERVICE_IMAGE").ok();
    let mut env = Env {
        client: reqwest::Client::new(),
        child: None,
        rpc: String::new(),
        metrics: String::new(),
        index: std::env::var_os("INDEX_DIR")
            .expect("INDEX_DIR: a build_index output")
            .into(),
    };
    let outcome = match start(&mut env, image.as_deref()).await {
        Ok(()) => checks(&mut env).await,
        Err(error) => Err(error),
    };
    if let Some(mut child) = env.child.take() {
        let _ = child.kill().await;
    }
    outcome.unwrap();
}
