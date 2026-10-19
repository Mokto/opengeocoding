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

The README highlights two separate historical measurements. They are not a benchmark of the current release, and the build, disk and request figures must not be presented as if they came from one deployment.

### World-index build

The [landmark build record](api.md#landmarks) reports a 2026-10-17 build taking **409 seconds**, with a **32.2 GB peak memory footprint**, on an **18-core Apple M5 Max with 64 GB RAM**. The resulting world index was roughly **16 GB**, including administrative polygons, street-reverse files, GeoNames places and landmarks. Source inputs included Overture, OpenAddresses, GeoNames and OSM extracts, including landmark-only extracts.

This times index construction from local sources, not first installation: source downloads, libpostal model provisioning, application compilation, artifact upload and deployment are outside the figure. The index size excludes models, source mirrors and build scratch. It is a single observed build, not an isolated before/after experiment or a memory ceiling.

### Forward-search throughput, p95 and memory

The [lookup optimization record](indexing.md#lookups) reports a comparison on an earlier **11 GB world index**, using **808 address samples** on a busy **18-core laptop**. Each result measures **one serving instance in one Linux container**, not aggregate traffic across replicas. The CPU model, Docker VM CPU allocation and per-container CPU quota were not recorded; the host's 18 cores must not be presented as the container's assigned vCPU count. The Linux-container results for the optimized build were:

| Connections | Queries/s | Request p95 |
| --- | ---: | ---: |
| 1 | 599 | 5.3 ms |
| 16 | 2,850 | Not recorded in the published summary |

A Kubernetes serving pod runs one such instance, but this benchmark did not measure Kubernetes pods at a fixed CPU allocation. The [chart defaults](../manifests/values.yaml) request 1 CPU and 6 GiB memory per serving pod, set a 16 GiB memory limit and leave CPU unlimited. Requests are scheduling reservations, not CPU caps or a throughput guarantee; the 2,850 queries/s result cannot be attributed to that 1-CPU request.

The serving process used **4.2 GiB resident memory**. The index was bind-mounted from the host and the parse cache was disabled (`PARSE_CACHE=0`); filesystem page-cache state was not recorded. These are end-to-end forward-geocoding requests, not the much smaller timings of an isolated index lookup. Responses were identical to the baseline in that comparison, which establishes no independent accuracy percentage.

The corresponding host-native run reached 806 queries/s with 3.9 ms p95 over one connection and 5,512 queries/s over 16. Do not combine its throughput with the container's latency or memory figure. Later measurements in the lookup history use different corpora, indexes and cache settings; the 16,000 queries/s result repeats queries with the parse cache enabled and is not the README's headline.

The original summaries do not attach complete per-run manifests, raw latency samples or a p95 for 16 connections. They support historical observations, not an independently reproducible current-release performance claim. No new benchmark was run for this documentation update.

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
