use anyhow::{Context, Result};
use axum::{Router, extract::Request, http::Uri, middleware::map_request, routing::get};
use connectrpc::{Limits, Router as ConnectRouter};
use connectrpc_reflection::{Reflector, install};
use opengeocoding::{metrics::METRICS, regions::Regions, resolver::Resolver, service::Service};
use std::{borrow::Cow, sync::Arc, time::Duration};
use tokio::sync::watch;

async fn legacy_package(mut request: Request) -> Request {
    if let Some(rest) = [
        "/geolocation.Geolocation/",
        "/nominatim_geolocation.NominatimGeolocation/",
    ]
    .iter()
    .find_map(|prefix| request.uri().path().strip_prefix(prefix))
    {
        let query = request
            .uri()
            .query()
            .map(|q| format!("?{q}"))
            .unwrap_or_default();
        if let Ok(path) = format!("/opengeocoding.Geolocation/{rest}{query}").parse() {
            let mut parts = request.uri().clone().into_parts();
            parts.path_and_query = Some(path);
            if let Ok(uri) = Uri::from_parts(parts) {
                *request.uri_mut() = uri;
            }
        }
    }
    request
}

async fn shutdown(mut receiver: watch::Receiver<bool>) {
    if !*receiver.borrow() {
        let _ = receiver.changed().await;
    }
}

async fn signal() -> Result<()> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! { result = tokio::signal::ctrl_c() => result?, _ = terminate.recv() => {} }
    Ok(())
}

async fn metrics() -> Result<String, (axum::http::StatusCode, String)> {
    prometheus::TextEncoder::new()
        .encode_to_string(&METRICS.registry.gather())
        .map_err(|error| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                error.to_string(),
            )
        })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let _sentry = sentry::init(sentry::ClientOptions {
        dsn: std::env::var("SENTRY_DSN")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| s.parse())
            .transpose()?,
        environment: std::env::var("ENV").ok().map(Cow::Owned),
        release: sentry::release_name!(),
        ..Default::default()
    });
    let dir = std::env::var_os("INDEX_DIR").context("INDEX_DIR is required")?;
    let started = std::time::Instant::now();
    let resolver = tokio::task::spawn_blocking(move || Resolver::open(dir.as_ref())).await??;
    tracing::info!(
        places = resolver.index().places_loaded(),
        seconds = started.elapsed().as_secs_f64(),
        "Index loaded"
    );
    let regions = Regions::from_resolver(&resolver)?;
    tracing::info!(regions = regions.len(), "Regions derived from the index");
    let reflector = Reflector::from_descriptor_set_bytes(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/reflection.bin"
    )))?
    .with_services([
        "opengeocoding.Geolocation",
        "grpc.reflection.v1.ServerReflection",
        "grpc.reflection.v1alpha.ServerReflection",
    ]);
    let mut connect = install(
        ConnectRouter::new().add_service(Arc::new(Service { resolver, regions })),
        reflector,
    );
    for method in ["SearchAddress", "ResolveRegion", "ListRegions"] {
        connect = connect.with_route_limits(
            &format!("opengeocoding.Geolocation/{method}"),
            Limits::unlimited(),
        );
    }
    let app = Router::new()
        .route("/healthz", get(|| async { "OK" }))
        .fallback_service(connect.into_axum_service())
        .layer(map_request(legacy_package));
    let rpc_port = std::env::var("RPC_PORT")
        .unwrap_or_else(|_| "11121".into())
        .parse::<u16>()?;
    let metrics_port = std::env::var("METRICS_PORT")
        .unwrap_or_else(|_| "8000".into())
        .parse::<u16>()?;
    let rpc = tokio::net::TcpListener::bind(("0.0.0.0", rpc_port)).await?;
    let metrics_listener = tokio::net::TcpListener::bind(("0.0.0.0", metrics_port)).await?;
    let (sender, receiver) = watch::channel(false);
    let rpc_receiver = receiver.clone();
    let mut rpc_task = tokio::spawn(async move {
        axum::serve(rpc, app)
            .with_graceful_shutdown(shutdown(rpc_receiver))
            .await
    });
    let mut metrics_task = tokio::spawn(async move {
        axum::serve(
            metrics_listener,
            Router::new().route("/metrics", get(metrics)),
        )
        .with_graceful_shutdown(shutdown(receiver))
        .await
    });
    tracing::info!(rpc_port, metrics_port, "OpenGeocoding service ready");
    tokio::select! {
        result = &mut rpc_task => result.context("RPC task")??,
        result = &mut metrics_task => result.context("metrics task")??,
        result = signal() => {
            result?;
            sender.send(true)?;
            tokio::time::timeout(Duration::from_secs(55), async {
                rpc_task.await.context("RPC shutdown")??;
                metrics_task.await.context("metrics shutdown")??;
                Ok::<_, anyhow::Error>(())
            }).await.context("graceful shutdown deadline exceeded")??;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn legacy_rpc_paths_are_preserved() {
        for service in [
            "opengeocoding.Geolocation",
            "geolocation.Geolocation",
            "nominatim_geolocation.NominatimGeolocation",
        ] {
            for method in ["SearchAddress", "ResolveRegion", "ListRegions"] {
                let request = Request::builder()
                    .uri(format!(
                        "http://localhost/{service}/{method}?key=%2Fvalue&key=other"
                    ))
                    .body(axum::body::Body::empty())
                    .unwrap();
                assert_eq!(
                    legacy_package(request).await.uri().to_string(),
                    format!(
                        "http://localhost/opengeocoding.Geolocation/{method}?key=%2Fvalue&key=other"
                    )
                );
            }
        }
        for path in ["/healthz", "/geolocation.GeolocationExtra/SearchAddress"] {
            let request = Request::builder()
                .uri(path)
                .body(axum::body::Body::empty())
                .unwrap();
            assert_eq!(legacy_package(request).await.uri().to_string(), path);
        }
    }
}
