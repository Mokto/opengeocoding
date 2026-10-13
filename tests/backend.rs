use axum::{Json, Router, routing::get};
use geolocation::resolver::{Resolver, check_index_status};
use serde_json::json;

#[tokio::test]
async fn only_serve_index_is_supported() {
    assert!(
        check_index_status(&json!({"status": 0, "software_version": "overture-index"})).is_ok()
    );
    for status in [
        json!({"status": 0, "software_version": "5.3.2"}),
        json!({"status": 1, "software_version": "overture-index"}),
        json!({"status": "0", "software_version": "overture-index"}),
        json!({}),
    ] {
        assert!(check_index_status(&status).is_err());
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/status",
                get(|| async { Json(json!({"status": 0, "software_version": "5.3.2"})) }),
            ),
        )
        .await
        .unwrap();
    });
    let error = Resolver::new(base.clone()).await.err().unwrap();
    server.abort();
    assert!(error.to_string().contains("ready serve_index"), "{error}");

    for url in [
        "ftp://localhost",
        "http://user:password@localhost",
        "http://localhost?query=1",
        "http://localhost#fragment",
    ] {
        assert!(Resolver::new(url.into()).await.is_err());
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_geolocation"))
        .env_remove("INDEX_URL")
        .env("NOMINATIM_HOST", base)
        .env("SENTRY_DSN", "")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("INDEX_URL is required"));
}
