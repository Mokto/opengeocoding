# Development guide

[Overview](../README.md) · [API](api.md) · [Evaluation](evaluation.md)

Run commands from the repository root. Historical measurements live in the API and index guides; they are not production capacity guarantees.

[Build and test](#build-and-test) · [Performance and Nominatim](#performance-methodology-and-nominatim-comparison) · [Benchmark](#local-performance-benchmark) · [Publishing](#ghcr-publishing) · [Reference data](#reference-data-and-normalization)

## Build and test

Local builds require Rust 1.99+, Buf and a compiled libpostal installation. Native library/model paths are determined by libpostal's configure options. Nonstandard linker paths can be supplied through `RUSTFLAGS="-L native=/path/to/lib"`.

```sh
cargo build --locked --bin opengeocoding
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
docker build --target checks .
```

`tests/rpc.rs` exercises Connect JSON/protobuf, both legacy routes, candidate safeguards, concurrent requests, metrics and shutdown against a world index. It needs an index and provisioned models, so it is ignored by default. Install `grpcurl` to include gRPC/reflection checks:

```sh
INDEX_DIR=out cargo test --locked --test rpc -- --ignored --nocapture
```

To test the final container instead of a local binary:

```sh
SERVICE_IMAGE=opengeocoding LIBPOSTAL_MODELS="$PWD/models/libpostal" INDEX_DIR=out \
  cargo test --locked --test rpc -- --ignored --nocapture
```

CI builds and checks both amd64 and arm64 without downloading models. [CodSpeed](https://codspeed.io/) runs the `benches/` suites in its simulation mode and reports changes in instruction counts: `index` (the index analyzer and house-number keys; `src/index/{schema,canon}.rs`), `matching` (the service's candidate matching; `src/matching.rs`, `src/resolver.rs`, `data/`) and `resolve` (model-backed libpostal parsing of `tests/addresses.tsv`; `src/index/mod.rs`, where the parser lives). A pull request runs only the suites whose files it changes (or all of them when it changes `Cargo.toml`, `Cargo.lock`, `build.rs` or the workflow); pushes to `main` run all of them, so each `main` commit is a complete baseline. Only `resolve` loads models; none loads an index. Run them locally with `cargo bench` (`resolve` needs `LIBPOSTAL_MODELS` set), or `cargo codspeed build && cargo codspeed run`. Index searches, suggestions and reverse lookups are timed end to end by the [full-index gold check](evaluation.md). Regenerate `THIRD_PARTY_NOTICES.txt` with `cargo run --locked --bin notices` after dependency changes. Model-backed RPC tests run separately with provisioned models.

## Performance methodology and Nominatim comparison

The README combines a historical world-index build with a measured query benchmark of deployed release 0.14.0. The build and request figures come from different machines and runs; they must not be presented as one combined deployment measurement.

### World-index build

The [landmark build record](api.md#landmarks) reports a 2026-10-17 build taking **409 seconds**, with a **32.2 GB peak memory footprint**, on an **18-core Apple M5 Max with 64 GB RAM**. The resulting world index was roughly **16 GB**, including administrative polygons, street-reverse files, GeoNames places and landmarks. Source inputs included Overture, OpenAddresses, GeoNames and OSM extracts, including landmark-only extracts.

This times index construction from local sources, not first installation: source downloads, libpostal model provisioning, application compilation, artifact upload and deployment are outside the figure. The index size excludes models, source mirrors and build scratch. It is a single observed build, not an isolated before/after experiment or a memory ceiling.

### Forward-search throughput, p95 and memory

On **2026-10-19**, release **0.14.0** was benchmarked against **one deployed Kubernetes pod**. The [aggregate measurement record](benchmarks/2026-10-19-search.json) preserves all three client summaries, software/image identifiers, corpus hash and resource observations. It contains no addresses or private endpoints.

| Run | Measured requests | Queries/s | p95 | p99 | Errors |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 20,000 | 2,675.0 | 18.45 ms | 34.69 ms | 0 |
| 2 | 20,000 | 2,656.2 | 19.27 ms | 35.87 ms | 0 |
| 3 | 20,000 | 1,773.9 | 29.86 ms | 60.62 ms | 0 |

The README reports the range across all three runs, including the slower third run. Percentiles are computed per run, not pooled across runs.

**Server and resources.** The host CPU was an **Intel Core i5-13500**, with **14 physical cores and 20 logical CPUs**, on a shared node. The serving container could use CPUs 0–19, requested **1 CPU and 6 GiB memory**, had **no CPU quota**, and had a **16 GiB memory limit**. Cgroup CPU counters showed **4.09–5.03 CPU cores on average over each invocation**, including preliminary queries and warm-up. This is not throughput on one reserved or capped CPU.

Process RSS sampled roughly every 1–2 seconds peaked at **6.29–6.30 GiB** during the runs. Container-charged memory peaked at **5.23–5.24 GiB**. Process RSS and cgroup-charged memory use different accounting for mapped pages and must not be substituted for one another. No server CPU throttling was recorded. The client used about 0.10–0.12 CPU cores over its invocations, with no CPU throttling; it was on a different node with a 4-CPU limit.

**Workload and transport.** The existing `src/bin/benchmark.rs` was compiled unchanged as a standalone client using the direct dependency versions recorded in the JSON. Each run used **16 concurrent workers** and **20,000 measured requests**, cycling through the **1,000 queries** in the `query` column of `tests/addresses.tsv`. Each invocation first made one preliminary request per query, then 160 connection-warming requests, outside the timed batch. The deployed parse cache was enabled at its default 65,536 entries; no page-cache reset was performed. This is a warm, repeating-corpus measurement, not unique-query or cold-cache performance.

The client called the selected pod IP directly, over Connect JSON/HTTP keep-alive from another node in the same namespace. Ingress, TLS, authentication and the public API's metering proxy were bypassed. The target was image `ghcr.io/mokto/opengeocoding:0.14.0`, with builder `a7481f0be4727233`, index `2026-09-23.1-2026-10-ff34388b4830` and model version `v1`; the JSON records the immutable image digest. No deployment, cache or resource settings were changed.

To repeat the request phase with the extracted one-query-per-line corpus and a reachable single pod:

```sh
for round in 1 2 3; do
  target/release/benchmark --queries queries.txt \
    --service "pod=http://${POD_IP:?Set a single pod IP}:11121" \
    --requests 20000 --rounds 1 --concurrency 16
done
```

Run the client on a different node and verify that namespace network policies permit access. The corpus and client source hashes in the JSON identify the exact inputs; use those revisions when reproducing the historical run.

**Limits.** There were **60,000 measured requests with zero HTTP, transport or JSON-decoding errors**, no retries, no pod restarts, and successful readiness checks after every round. Requests returning valid JSON are not necessarily geographically correct; no accuracy evaluation was performed. The timed batches lasted about 7.5–11.3 seconds each on a live instance on a shared node; this is not a sustained-capacity test or an isolated explanation of the third-run slowdown. Preliminary connectivity probes were excluded from the measurements. Raw per-request latency samples were not retained. The temporary client pod was removed afterwards.

The older 2,850 queries/s result and its incomplete hardware/latency record remain in the [lookup optimization history](indexing.md#lookups), not the README headline. That older run used a different index, an 808-query corpus and disabled parse caching; it cannot be used as a speed comparison with this run.

### What the Nominatim figures mean

Source: [Nominatim 5.3 installation prerequisites](https://nominatim.org/release-docs/5.3/admin/Installation/#hardware), checked 2026-10-19. For a planet import it strongly recommends **128 GB RAM or more**; for a planet installation it requires **at least 1 TB disk**, preferably NVMe. It describes a planet import taking **around 2.5 days** on a well-configured machine, or **4–5 days** on traditional SSDs.

These are Nominatim's published planning figures, not measurements from the OpenGeocoding benchmark machine. Nominatim imports an OSM planet into PostgreSQL/PostGIS; OpenGeocoding builds a different, compact serving index from already-processed geographic inputs. Neither the data coverage nor the import work is equivalent. Import-memory recommendations cannot be compared with measured serving-process RSS as a RAM-saving ratio, and a final index size is not comparable to total installation/scratch requirements as a disk-saving ratio.

There is no matched Nominatim throughput, request-p95 or serving-memory result in this record. A numeric speedup or resource-saving percentage would require both services on identified hardware, documented source snapshots and coverage, the same queries and concurrency, explicit cache/warm-up settings, request/error counts, latency distributions, and separate build/serving memory and disk accounting. Measure both under the same protocol or clearly account for adapter overhead, and evaluate geographic correctness separately. The benchmark below speaks Connect JSON; it cannot call a stock Nominatim HTTP endpoint without an adapter.

## Local performance benchmark

The `benchmark` binary sends Connect JSON SearchAddress requests to running services. It first sends every query once to each service and counts differing responses, then times `--requests` calls per service and concurrency over keep-alive connections, rotating the service order between rounds and warming connections outside each timed batch. No failed request is retried. Its endpoint checks run in `cargo test`.

Start release-built services on the same index and models. Benchmarking must not depend on production backends or databases.

```sh
cargo build --locked --release --bin opengeocoding --bin benchmark
target/release/benchmark --queries queries.txt \
  --service base=http://127.0.0.1:18121 \
  --service candidate=http://127.0.0.1:18122 \
  --requests 2000 --rounds 3 --concurrency 1,8 > benchmark.jsonl
```

`queries.txt` holds one address per line, for example the `query` column of `tests/addresses.tsv`. A service URL without a route uses `opengeocoding.OpenGeocoding`; to benchmark old clients, use `--service legacy=http://127.0.0.1:18121/geolocation.Geolocation` or `.../nominatim_geolocation.NominatimGeolocation`. All three routes use the same resolver. Results include the client, HTTP transport, parsing, index lookups and the RPC framework on one machine; they are not production throughput or accuracy guarantees.

## GHCR publishing

`.github/workflows/docker.yml` runs on `v*` tag pushes, which release-please creates when its Release PR is merged (it needs the `RELEASE_PLEASE_TOKEN` secret, because tags pushed with `GITHUB_TOKEN` trigger no workflows). It runs the existing checks first, then uses Docker's pinned reusable workflow to build on native amd64/arm64 runners, push to `ghcr.io/mokto/opengeocoding`, and assemble and verify the multi-platform manifest. Pull requests do not publish to that image.

Separately, `staging-build.yml` builds the `eval` image on native amd64 and arm64 runners (the runtime image plus the `evaluate` binary and `tests/gold.jsonl`) for every pull request (forks included) and every `main` commit, with no secrets, and `staging-publish.yml` pushes it to `ghcr.io/mokto/opengeocoding-staging` from the default branch without running it: `sha-<commit>-amd64`, `sha-<commit>-arm64` and the multi-platform `sha-<commit>`. These images are untrusted builds for evaluation environments, not releases; set that package's visibility to public after its first push so evaluators can pull it anonymously. The publish workflow only triggers once it exists on `main`, so it cannot be tried from the pull request that adds it. Old tags are not pruned yet.

The repository is `Mokto/opengeocoding`. The Rust crate and service executable are `opengeocoding`, and the GHCR image is `ghcr.io/mokto/opengeocoding`.

A `v1.2.3` Git tag produces `1.2.3` and `latest`; pushes to `main` publish nothing. Publishing uses the workflow's `GITHUB_TOKEN` with `packages: write`, so no personal access token secret is required. OCI labels link the package to this repository. GitHub releases and tags are managed by release-please. The crate is not published to crates.io.

After the image, the same workflow publishes the Helm chart to `oci://ghcr.io/mokto/charts/opengeocoding`, pinned to that image: a `v1.2.3` tag gives chart version and appVersion `1.2.3`.

GHCR can initially create packages as private even for public repositories. After the first successful push, set the [package visibility](https://github.com/users/Mokto/packages/container/opengeocoding/settings) to public if anonymous image pulls are required.

## Reference data and normalization

Country aliases use public Unicode CLDR and GeoNames data. Administrative-region hints use GeoNames; the Wikidata items of the ISO 3166-2 subdivisions Overture lacks (`iso3166_2`; see [Configuration](deployment.md#configuration)) come from Wikidata (CC0). Sources, revision and download hashes are recorded in `data/sources.json`; the checked-in data allows offline reference generation during normal builds.

To refresh it, run `cargo run --locked --bin update_reference`. This requires network access and changes the checked-in dataset; review it as a data update. Character classes come from the Unicode Character Database version pinned in the generator (currently 16.0.0), not from the toolchain.

An optional build-time `REFERENCE_DATA` path can supply a custom reference JSON. Search overrides are empty by default. No private deployments, endpoints or address overrides are included.

Transliteration uses BSD-3-Clause `deunicode`, **not Python Unidecode**. Outputs differ for some scripts and symbols. In a migration comparison of 7,067 geographic/sample strings, 55 normalized outputs differed, including Hebrew and Japanese forms. Matching normalizes both sides consistently, but these differences still need real-address accuracy validation before replacing another service.
