//! Model-backed RPC integration test against a real, isolated ClickHouse database and a local
//! serve_index fixture. It creates and removes its own database. Ignored by default:
//!
//! ```text
//! CLICKHOUSE_URL=http://127.0.0.1:8125 cargo test --test rpc -- --ignored --nocapture
//! SERVICE_IMAGE=opengeocoding LIBPOSTAL_MODELS=$PWD/models/libpostal CLICKHOUSE_URL=... cargo test --test rpc -- --ignored --nocapture
//! ```
//!
//! With grpcurl installed, legacy gRPC is exercised too; its known hang fails the test with a
//! bounded timeout rather than being skipped (see README).

use anyhow::{Context, Result, ensure};
use axum::{
    Router,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use buffa::Message;
use opengeocoding::{
    proto::opengeocoding::{EmptyRequest, FullAddress, GeoCoordinates, SearchAddressTask},
    regions::python_repr,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::process::{Child, Command};

const SERVICE: &str = "opengeocoding.Geolocation";

#[derive(Default)]
struct Fixture {
    failures: u32,
    languages: Vec<String>,
    mode: &'static str,
}

type Shared = Arc<Mutex<Fixture>>;

async fn index_backend(
    State(fixture): State<Shared>,
    uri: Uri,
    RawQuery(_): RawQuery,
    headers: HeaderMap,
) -> Response {
    if uri.path() == "/status" {
        return axum::Json(json!({"status": 0, "software_version": "overture-index"}))
            .into_response();
    }
    let language = headers
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let mode = {
        let mut fixture = fixture.lock().expect("fixture lock");
        fixture.languages.push(language.clone());
        if fixture.failures > 0 {
            fixture.failures -= 1;
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
        fixture.mode
    };
    let address = match mode {
        "wrong_postcode" => {
            json!({"country": "United States", "country_code": "us", "city": "Paris", "postcode": "75460"})
        }
        "local" => json!({"country": "Germany", "country_code": "de",
                          "city": if language.is_empty() { "München" } else { "Munich" }}),
        _ => {
            json!({"country": "Denmark", "country_code": "dk", "city": "Copenhagen", "ISO3166-2-lvl4": "DK-84"})
        }
    };
    let value = json!({"display_name": address["city"], "address": address, "lat": "55.676098",
                       "lon": "12.568337", "osm_id": 1, "osm_type": "node"});
    let body = if uri.path().starts_with("/reverse") {
        value
    } else {
        json!([value])
    };
    (
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

struct Env {
    client: reqwest::Client,
    clickhouse: String,
    database: String,
    child: Option<Child>,
    rpc: String,
    metrics: String,
    map_dir: PathBuf,
    fixture: Shared,
}

impl Env {
    async fn clickhouse(&self, query: &str) -> Result<()> {
        let response = self
            .client
            .post(&self.clickhouse)
            .basic_auth(
                std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into()),
                Some(std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default()),
            )
            .query(&[("query", query)])
            .header(header::CONTENT_LENGTH, "0")
            .timeout(Duration::from_secs(30))
            .send()
            .await?;
        ensure!(
            response.status().is_success(),
            "ClickHouse: {}",
            response.text().await?
        );
        Ok(())
    }

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

    fn set(&self, update: impl FnOnce(&mut Fixture)) {
        update(&mut self.fixture.lock().expect("fixture lock"));
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

async fn start(env: &mut Env, index_port: u16, image: Option<&str>) -> Result<()> {
    env.clickhouse(&format!("CREATE DATABASE {}", env.database))
        .await?;
    env.clickhouse(&format!(
        "CREATE TABLE {}.administrative_levels (country_code String, country_name String, level UInt8,
         local_name String, en_name String, admin_code String, updated_at Nullable(DateTime('UTC')),
         parent_admin_code String) ENGINE = ReplacingMergeTree ORDER BY (country_code, admin_code)",
        env.database
    ))
    .await?;
    env.clickhouse(&format!(
        "INSERT INTO {}.administrative_levels VALUES ('dk','Denmark',4,'Hovedstaden','Capital Region','84',NULL,'')",
        env.database
    ))
    .await?;
    let port = |listener: std::net::TcpListener| listener.local_addr().map(|a| a.port());
    let rpc_port = port(std::net::TcpListener::bind("127.0.0.1:0")?)?;
    let metrics_port = port(std::net::TcpListener::bind("127.0.0.1:0")?)?;
    env.rpc = format!("http://127.0.0.1:{rpc_port}");
    env.metrics = format!("http://127.0.0.1:{metrics_port}/metrics");
    std::fs::create_dir_all(&env.map_dir)?;
    std::fs::write(env.map_dir.join("regions.json"), "{}")?;
    let mut settings = vec![
        ("INDEX_URL", format!("http://127.0.0.1:{index_port}")),
        ("CLICKHOUSE_URL", env.clickhouse.clone()),
        (
            "CLICKHOUSE_USER",
            std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into()),
        ),
        (
            "CLICKHOUSE_PASSWORD",
            std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default(),
        ),
        ("CLICKHOUSE_DATABASE", env.database.clone()),
        ("CLICKHOUSE_PREFIX", String::new()),
        ("CLICKHOUSE_SINGLE_NODE", "1".into()),
        (
            "REGION_MAP_PATH",
            env.map_dir.join("regions.json").to_string_lossy().into(),
        ),
        ("RPC_PORT", rpc_port.to_string()),
        ("METRICS_PORT", metrics_port.to_string()),
        ("SENTRY_DSN", String::new()),
    ];
    let mut command = if let Some(image) = image {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&env.map_dir, std::fs::Permissions::from_mode(0o777))?;
        std::fs::set_permissions(
            env.map_dir.join("regions.json"),
            std::fs::Permissions::from_mode(0o666),
        )?;
        for (key, value) in &mut settings {
            match *key {
                "INDEX_URL" | "CLICKHOUSE_URL" => {
                    *value = value.replace("127.0.0.1", "host.docker.internal")
                }
                "REGION_MAP_PATH" => *value = "/region-map/regions.json".into(),
                _ => {}
            }
        }
        let mut command = Command::new("docker");
        command
            .args([
                "run",
                "--rm",
                "--add-host=host.docker.internal:host-gateway",
            ])
            .args(["-p", &format!("127.0.0.1:{rpc_port}:{rpc_port}")])
            .args(["-p", &format!("127.0.0.1:{metrics_port}:{metrics_port}")])
            .args(["-v", &format!("{}:/region-map", env.map_dir.display())]);
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
    for _ in 0..100 {
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
    ensure!(
        found["addressDetails"]["regions"][0]["adminCode"] == "84",
        "{found}"
    );
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
                .is_some_and(|r| r.local_name == "Hovedstaden")
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

    env.set(|f| {
        f.mode = "local";
        f.languages.clear();
    });
    let found = env.search("München, Deutschland").await?;
    ensure!(found["addressDetails"]["city"] == "Munich", "{found}");
    let languages = env.fixture.lock().expect("fixture lock").languages.clone();
    ensure!(languages == ["en", ""], "{languages:?}");
    env.set(|f| f.mode = "wrong_postcode");
    let found = env.search("Paris, France, 75001").await?;
    ensure!(found == json!({}), "{found}");
    env.set(|f| f.mode = "normal");
    println!("PASS: local-language corroboration and postcode/country rejection");

    let regions = env
        .json("ResolveRegion", json!({"lat": 55.676098, "lon": 12.568337}))
        .await?;
    ensure!(regions["regions"][0]["adminCode"] == "84", "{regions}");
    ensure!(env.json("RefreshRegionMap", json!({})).await? == json!({}));
    let served = env.json("GetRegionMap", json!({})).await?;
    let served = served["regionMap"].as_str().unwrap_or("");
    ensure!(served.contains("Hovedstaden"), "{served}");
    let stored: Value =
        serde_json::from_str(&std::fs::read_to_string(env.map_dir.join("regions.json"))?)?;
    ensure!(
        python_repr(&stored)? == served,
        "stored and served region maps differ"
    );
    println!("PASS: reverse lookup and atomic region-map refresh");

    env.set(|f| f.failures = 1);
    ensure!(env.search("Copenhagen, Denmark").await?["addressDetails"]["countryCode"] == "dk");
    ensure!(env.fixture.lock().expect("fixture lock").failures == 0);
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
    println!("PASS: retry, invalid-input errors and 64 concurrent requests");

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
#[ignore = "needs ClickHouse (CLICKHOUSE_URL) and libpostal models; run with --ignored"]
async fn rpc_contracts() {
    let image = std::env::var("SERVICE_IMAGE").ok();
    let fixture: Shared = Arc::new(Mutex::new(Fixture {
        mode: "normal",
        ..Default::default()
    }));
    let bind = if image.is_some() {
        "0.0.0.0:0"
    } else {
        "127.0.0.1:0"
    };
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .expect("bind fixture");
    let index_port = listener.local_addr().expect("fixture address").port();
    let app = Router::new()
        .fallback(index_backend)
        .with_state(fixture.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let mut env = Env {
        client: reqwest::Client::new(),
        clickhouse: std::env::var("CLICKHOUSE_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8125".into()),
        database: format!("opengeocoding_rpc_test_{}", std::process::id()),
        child: None,
        rpc: String::new(),
        metrics: String::new(),
        map_dir: std::env::temp_dir()
            .join(format!("opengeocoding-rpc-test-{}", std::process::id())),
        fixture,
    };
    let outcome = match start(&mut env, index_port, image.as_deref()).await {
        Ok(()) => checks(&mut env).await,
        Err(error) => Err(error),
    };
    if let Some(mut child) = env.child.take() {
        let _ = child.kill().await;
    }
    server.abort();
    let dropped = env
        .clickhouse(&format!("DROP DATABASE IF EXISTS {}", env.database))
        .await;
    let _ = std::fs::remove_dir_all(&env.map_dir);
    outcome.unwrap();
    dropped.unwrap();
}
