use prometheus::{CounterVec, HistogramOpts, HistogramVec, Opts, Registry};
use std::sync::LazyLock;

pub struct Metrics {
    pub registry: Registry,
    pub tiers: CounterVec,
    pub search: HistogramVec,
    pub started: CounterVec,
    pub requests: CounterVec,
    pub handled: CounterVec,
    pub duration: HistogramVec,
}

pub static METRICS: LazyLock<Metrics> = LazyLock::new(|| {
    let registry = Registry::new();
    let tiers = CounterVec::new(
        Opts::new(
            "nominatim_geolocation_query_tier_total",
            "Queries resolved by fallback tier",
        ),
        &["tier"],
    )
    .unwrap();
    let search = HistogramVec::new(
        HistogramOpts::new(
            "nominatim_geolocation_search_candidates_seconds",
            "Index candidate search duration",
        ),
        &["tier"],
    )
    .unwrap();
    let started = CounterVec::new(
        Opts::new("grpc_server_started_total", "RPCs started"),
        &["grpc_service", "grpc_method"],
    )
    .unwrap();
    let requests = CounterVec::new(
        Opts::new("grpc_requests_total", "RPCs by organization"),
        &["grpc_service", "grpc_method", "organization_id"],
    )
    .unwrap();
    let handled = CounterVec::new(
        Opts::new("grpc_server_handled_total", "RPCs completed"),
        &["grpc_service", "grpc_method", "grpc_code"],
    )
    .unwrap();
    let duration = HistogramVec::new(
        HistogramOpts::new("grpc_server_handling_seconds", "RPC handler duration").buckets(vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1., 2.5, 5., 10., 15., 20., 25., 30.,
            45., 60., 90., 120.,
        ]),
        &["grpc_service", "grpc_method"],
    )
    .unwrap();
    for collector in [
        Box::new(tiers.clone()) as Box<dyn prometheus::core::Collector>,
        Box::new(search.clone()),
        Box::new(started.clone()),
        Box::new(requests.clone()),
        Box::new(handled.clone()),
        Box::new(duration.clone()),
    ] {
        registry.register(collector).unwrap();
    }
    Metrics {
        registry,
        tiers,
        search,
        started,
        requests,
        handled,
        duration,
    }
});
