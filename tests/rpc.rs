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
    EmptyRequest, FullAddress, GeoCoordinates, RegionsResponse, SearchAddressTask,
    SuggestAddressRequest, SuggestAddressResponse,
};
use reqwest::{StatusCode, header};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::process::{Child, Command};

const SERVICE: &str = "opengeocoding.OpenGeocoding";

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
            "ListRegions",
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

    // Production queries that once got a wrong answer, a confident wrong one, or none.
    for (query, country, good) in [
        // a place named by a few words of a long query is a guess, not Thuin in Belgium
        (
            "Shop 35, Pietersburg Rd, Twin City Shopping Centre, Burgersfort, Lebowokgomo",
            "za",
            false,
        ),
        // the country the parser filed as a house
        (
            "7th Flr. Al Zamel Building, Manama. P.O. Box 54482, Kingdom of Bahrain",
            "bh",
            false,
        ),
        ("Pier 83 Midtown", "", false),
        ("P.O. Box 21, Mt. Gretna, PA, 17064", "us", false),
        ("Mt. Gretna, PA 17064", "us", true),
        ("Taman Intan, 41300, Klang, Selangor", "my", false),
        ("Launde, Lincs, LN4 3JG", "gb", false),
        ("coulsdon, london", "gb", true),
        ("Hegelstraße 8, 63628 Bad Soden-Salmünster", "de", true),
        ("567 S 5th Street, West Dundee, IL 60118 IDX", "us", true),
        (
            "1236 Somerset Avenue, Taunton, MA 02780, Canada",
            "us",
            true,
        ),
        // a territory answers under its sovereign, as the services this replaced did
        ("San Juan, Puerto Rico", "us", true),
        ("Saint-Denis, Réunion", "fr", true),
    ] {
        let found = env.search(query).await?;
        let details = &found["addressDetails"];
        ensure!(
            (country.is_empty() || details["countryCode"] == country)
                && found["goodMatch"].as_bool().unwrap_or(false) == good,
            "{query}: {found}"
        );
    }
    let found = env
        .search("Hegelstraße 8, 63628 Bad Soden-Salmünster")
        .await?;
    ensure!(found["addressDetails"]["houseNumber"] == "8", "{found}");
    // a street the data lacks is never answered by another street, or with confidence
    let found = env.search("Шарикоподшипниковская ул., 4, Москва").await?;
    ensure!(
        found["addressDetails"]["houseNumber"].is_null()
            && found["addressDetails"]["road"].is_null()
            && found["goodMatch"].as_bool() != Some(true),
        "{found}"
    );
    // postcodes as the country writes them, with a country even for a street filed under no town
    for (query, country, postcode) in [
        ("Damrak 1, Amsterdam", "nl", "1012 LG"),
        ("Pointe-Claire, QC H9R 5Z5", "ca", "H9R 5Z5"),
        (
            "Monument House, 1st Floor 215 Marsh Road Pinner ha5 5ne",
            "gb",
            "HA5 5NE",
        ),
        (
            "U Krčské vodárny 1133/61, 140 00, Praha 4 – Krč, Česká republika",
            "cz",
            "140 00",
        ),
    ] {
        let found = env.search(query).await?;
        let details = &found["addressDetails"];
        ensure!(
            details["countryCode"] == country && details["postcode"] == postcode,
            "{query}: {found}"
        );
    }
    // no part of a display name repeats the one before it
    let found = env.search("10 Rue de Rivoli, Paris, France").await?;
    let display = found["displayName"].as_str().unwrap_or("");
    let parts: Vec<_> = display.split(", ").collect();
    ensure!(parts.windows(2).all(|w| w[0] != w[1]), "{found}");
    println!("PASS: noisy, ambiguous and unmatched queries, postcode formats, territories");

    let regions = env
        .json("ResolveRegion", json!({"lat": 55.676098, "lon": 12.568337}))
        .await?;
    ensure!(regions["regions"][0]["adminCode"] == "84", "{regions}");
    let listed = env.json("ListRegions", json!({})).await?;
    ensure!(
        listed["regions"].as_array().is_some_and(|r| r
            .iter()
            .any(|r| r["adminCode"] == "84" && r["localName"] == local_name)),
        "{listed}"
    );
    let listed = listed["regions"].as_array().unwrap();
    for (lat, lon, country, admin_code) in [
        (55.676098, 12.568337, "dk", "84"),
        (48.1374, 11.5755, "de", "BY"),
        (48.8566, 2.3522, "fr", "IDF"),
    ] {
        let found = env
            .json("ResolveRegion", json!({"lat": lat, "lon": lon}))
            .await?;
        let found = found["regions"].as_array().context("regions")?;
        ensure!(
            found
                .iter()
                .any(|r| r["countryCode"] == country && r["adminCode"] == admin_code),
            "{lat},{lon}: {found:?}"
        );
        // every region it names is one ListRegions serves
        ensure!(
            found.iter().all(|r| listed.contains(r)),
            "{lat},{lon}: {found:?}"
        );
    }
    // far from any locality (mid-Atlantic) and the poles are valid questions with no answer
    for (lat, lon) in [(0., -30.), (90., 180.), (-90., -180.)] {
        for proto in [false, true] {
            let body = if proto {
                GeoCoordinates {
                    lat,
                    lon,
                    ..Default::default()
                }
                .encode_to_vec()
            } else {
                json!({"lat": lat, "lon": lon}).to_string().into_bytes()
            };
            let (status, body) = env.rpc("ResolveRegion", body, proto).await?;
            ensure!(status.is_success(), "{lat},{lon}: {status}");
            let empty = if proto {
                RegionsResponse::decode_from_slice(&body)?
                    .regions
                    .is_empty()
            } else {
                serde_json::from_slice::<Value>(&body)?["regions"]
                    .as_array()
                    .is_none_or(|r| r.is_empty())
            };
            ensure!(empty, "{lat},{lon}: {}", String::from_utf8_lossy(&body));
        }
    }
    println!("PASS: reverse lookup and region list");

    let reversed = env
        .json(
            "ReverseGeocode",
            json!({"lat": 55.676098, "lon": 12.568337, "size": 3}),
        )
        .await?;
    let results = reversed["results"].as_array().filter(|r| !r.is_empty());
    ensure!(
        results.is_some_and(|r| r.iter().all(|r| {
            r["address"]["addressDetails"]["countryCode"] == "dk"
                && ["address", "street", "locality"]
                    .contains(&r["layer"].as_str().unwrap_or_default())
        })),
        "{reversed}"
    );
    println!("PASS: reverse geocoding");

    let suggested = env
        .json("SuggestAddress", json!({"text": "copenha", "limit": 3}))
        .await?;
    let first = &suggested["suggestions"][0];
    ensure!(
        first["countryCode"] == "dk"
            && first["type"] == "city"
            && first["id"].as_str().is_some_and(|id| !id.is_empty())
            && first["geoCoordinates"]["lat"]
                .as_f64()
                .is_some_and(|lat| (lat - 55.68).abs() < 0.2),
        "{suggested}"
    );
    let floor_text = "Rådhuspladsen 1, 4. th., København";
    let building = env
        .json(
            "SuggestAddress",
            json!({"text": "Rådhuspladsen 1, København", "limit": 1}),
        )
        .await?;
    let floor = env
        .json("SuggestAddress", json!({"text": floor_text, "limit": 1}))
        .await?;
    let first = &floor["suggestions"][0];
    ensure!(
        first["unverifiedSuffix"] == "4. th."
            && first["id"] == building["suggestions"][0]["id"]
            && first["geoCoordinates"] == building["suggestions"][0]["geoCoordinates"]
            && first["displayName"]
                .as_str()
                .is_some_and(|n| n.contains(", 4. th., ")),
        "{floor}"
    );
    let request = SuggestAddressRequest {
        text: floor_text.into(),
        limit: 1,
        ..Default::default()
    };
    let (status, body) = env
        .rpc("SuggestAddress", request.encode_to_vec(), true)
        .await?;
    ensure!(
        status.is_success(),
        "protobuf SuggestAddress returned {status}"
    );
    let decoded = SuggestAddressResponse::decode_from_slice(&body)?;
    ensure!(
        decoded.suggestions.len() == 1
            && decoded.suggestions[0].unverified_suffix == "4. th."
            && decoded.suggestions[0].id == first["id"].as_str().unwrap_or("")
            && decoded.suggestions[0].display_name == first["displayName"].as_str().unwrap_or(""),
        "{decoded:?}"
    );
    println!("PASS: unverified floor suffix preserved over Connect JSON and protobuf");
    // near Dallas, Paris is the one in Texas
    let focused = env
        .json(
            "SuggestAddress",
            json!({"text": "paris", "limit": 3, "focus": {"lat": 32.78, "lon": -96.8}}),
        )
        .await?;
    ensure!(
        focused["suggestions"][0]["countryCode"] == "us",
        "{focused}"
    );
    // the airport only when asked for, with its category; other types carry none
    let landmark = |s: &Value| s["type"] == "landmark";
    let plain = env.json("SuggestAddress", json!({"text": "CDG"})).await?;
    let disabled = env
        .json(
            "SuggestAddress",
            json!({"text": "CDG", "includeLandmarks": false}),
        )
        .await?;
    ensure!(
        plain == disabled,
        "absent and false flags differ: {plain} {disabled}"
    );
    let asked = env
        .json(
            "SuggestAddress",
            json!({"text": "CDG", "includeLandmarks": true}),
        )
        .await?;
    let list = |v: &Value| v["suggestions"].as_array().cloned().unwrap_or_default();
    ensure!(
        !list(&plain).iter().any(landmark)
            && list(&plain).iter().all(|s| s.get("category").is_none())
            && list(&asked).first().is_some_and(|s| {
                landmark(s) && s["category"] == "airport" && s["countryCode"] == "fr"
            }),
        "{plain} {asked}"
    );
    println!("PASS: address suggestions");

    for (method, payload) in [
        ("SearchAddress", json!({"addressString": "bad\u{0}address"})),
        ("ResolveRegion", json!({"lat": 100, "lon": 0})),
        ("ReverseGeocode", json!({"lat": 0, "lon": 181})),
        ("SuggestAddress", json!({"text": "bad\u{0}text"})),
        (
            "SuggestAddress",
            json!({"text": "paris", "countryCode": "USA"}),
        ),
        (
            "SuggestAddress",
            json!({"text": "paris", "focus": {"lat": 91, "lon": 0}}),
        ),
        ("ResolveRegion", json!({"lat": 0, "lon": -180.5})),
        ("ResolveRegion", json!({"lat": -90.5, "lon": 0})),
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
    // JSON cannot carry these; protobuf can
    for (lat, lon) in [(f32::NAN, 0.), (0., f32::INFINITY)] {
        let body = GeoCoordinates {
            lat,
            lon,
            ..Default::default()
        }
        .encode_to_vec();
        let (status, body) = env.rpc("ResolveRegion", body, true).await?;
        ensure!(
            status == StatusCode::BAD_REQUEST,
            "ResolveRegion accepted {lat},{lon}: {}",
            String::from_utf8_lossy(&body)
        );
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
            && metrics.contains(r#"grpc_service="opengeocoding.OpenGeocoding""#)
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
