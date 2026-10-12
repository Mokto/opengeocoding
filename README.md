# Geolocation

A Rust service that combines libpostal address parsing, Nominatim lookups and ClickHouse-backed administrative regions.

- Connect JSON and Connect protobuf; an experimental legacy gRPC surface.
- Forward geocoding, reverse region resolution and region-map refresh/read APIs.
- Country/region corroboration, local-language candidate matching (including each place's names in other languages and scripts), postcode-area safeguards and ordered fallback queries.
- `goodMatch` is false for structured-fallback results and for street results that lack the requested house number.
- Prometheus metrics, gRPC reflection and graceful shutdown.
- A non-root, Python-free runtime for Linux amd64 and arm64.

This is an initial implementation, not a production accuracy or throughput guarantee. libpostal remains a native C parser, not a pure-Rust replacement. Calls are serialized off the async executor pending native concurrency validation.

## Run with Docker

Build the image locally:

```sh
docker build -t geolocation .
```

Or pull a published image:

```sh
docker pull ghcr.io/mokto/geolocation:latest
docker tag ghcr.io/mokto/geolocation:latest geolocation
```

`latest` is published from `main`. Models are not included in the image.

Download libpostal models explicitly into a writable directory. Downloads are large; nothing downloads automatically at import or startup:

```sh
mkdir -p models
docker run --rm --entrypoint libpostal_data \
  -v "$PWD/models:/data" geolocation download all /data
```

The resulting directory is `models/libpostal`. Keep it for subsequent runs.

Provide your own Nominatim and ClickHouse instances:

```sh
docker run --rm \
  -p 11121:11121 -p 8000:8000 \
  -v "$PWD/models/libpostal:/data/libpostal:ro" \
  -e NOMINATIM_HOST=http://your-nominatim:8080 \
  -e CLICKHOUSE_URL=http://your-clickhouse:8123 \
  -e CLICKHOUSE_USER=default \
  -e CLICKHOUSE_PASSWORD=your-password \
  geolocation
```

Addresses inside Docker must be reachable from the container; `localhost` refers to that container. The service creates its ClickHouse database/table and warms its region cache at startup.

**Deploy behind a trusted network or authenticated proxy.** The RPCs, including region-map refresh, do not implement authentication. Do not expose them directly to the public internet. There is no automatic rate limiting for a shared public Nominatim endpoint; configure a backend you are permitted to use.

Example request:

```sh
curl http://localhost:11121/geolocation.Geolocation/SearchAddress \
  -H 'Content-Type: application/json' \
  -H 'Connect-Protocol-Version: 1' \
  -d '{"addressString":"Copenhagen, Denmark"}'
```

The contract is `proto/geolocation-v1.proto`, package `geolocation`, service `Geolocation`. The four methods are `SearchAddress`, `ResolveRegion`, `RefreshRegionMap` and `GetRegionMap`. Clients generated from the earlier `nominatim_geolocation.NominatimGeolocation` service keep working: `/nominatim_geolocation.NominatimGeolocation/*` is served by the same service, with identical messages and field numbers. gRPC reflection lists only the new names.

## Known interoperability issue

The initial model-backed integration run passes Connect JSON/protobuf, local-language corroboration, country/postcode rejection, reverse regions, map refresh, retry/error handling and 64 concurrent requests. However, grpcurl 1.9.4 can list services but its SearchAddress invocation hangs, both with reflection and an explicit proto file. The integration script fails with a bounded timeout rather than skipping that failure. Legacy gRPC interoperability is not signed off; do not treat this initial push as a production-ready replacement.

## Configuration

| Variable | Default |
| --- | --- |
| `NOMINATIM_HOST` | Required; endpoint base URL |
| `NOMINATIM_AUTHORITY` | Optional Host header override |
| `CLICKHOUSE_URL` | `http://127.0.0.1:8123` |
| `CLICKHOUSE_DATABASE` | `geolocation` |
| `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` | `default` / empty |
| `CLICKHOUSE_PREFIX` | Empty; otherwise prefixes the table name |
| `CLICKHOUSE_SINGLE_NODE` | Enabled; an empty value selects clustered DDL |
| `REGION_MAP_PATH` | `regions.json` in the working directory |
| `RPC_PORT` / `METRICS_PORT` | `11121` / `8000` |
| `RUST_LOG` | `info` |
| `SENTRY_DSN` / `ENV` | Optional |

Clustered ClickHouse needs the usual `cluster`, `shard` and `replica` macros. Region-map refresh uses atomic file replacement; its directory must be writable. The default container's `/app` directory belongs to uid/gid 1000.

`/healthz` reports readiness after initialization. Metrics are exposed at `/metrics` on port 8000.

`GetRegionMap` returns a Python-style dictionary string for wire compatibility; the stored file is JSON. Coordinates use protobuf float32. Postcode compatibility deliberately includes shared outward/area codes for supported national schemes; it is not a promise of an exact street-level postcode match.

## Build and test

Local builds require Rust 1.99+, Buf and a compiled libpostal installation. Native library/model paths are determined by libpostal's configure options. Nonstandard linker paths can be supplied through `RUSTFLAGS="-L native=/path/to/lib"`.

```sh
cargo build --locked --bin geolocation
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
docker build --target checks .
```

`tests/rpc.rs` exercises Connect JSON/protobuf, persistence, candidate safeguards, concurrent requests, metrics and shutdown against a real isolated ClickHouse database and a local HTTP fixture. It creates and removes its own database. It needs ClickHouse and provisioned models, so it is ignored by default. Install `grpcurl` to include gRPC/reflection checks:

```sh
CLICKHOUSE_URL=http://127.0.0.1:8125 cargo test --locked --test rpc -- --ignored --nocapture
```

To test the final container instead of a local binary:

```sh
SERVICE_IMAGE=geolocation LIBPOSTAL_MODELS="$PWD/models/libpostal" \
  CLICKHOUSE_URL=http://127.0.0.1:8125 cargo test --locked --test rpc -- --ignored --nocapture
```

CI builds and checks both amd64 and arm64 without downloading models. Regenerate `THIRD_PARTY_NOTICES.txt` with `cargo run --locked --bin notices` after dependency changes. Model-backed RPC tests run separately with provisioned models.

## Local performance benchmark

The `benchmark` binary provides a loopback Nominatim mock and a Connect JSON benchmark. It compares full responses and checks that each implementation makes the same number of backend requests. No failed requests are retried. Its fixture and validation checks run in `cargo test`.

```sh
cargo build --locked --release --bin geolocation --bin benchmark
target/release/benchmark --serve 18080
```

Start release-built local services with `NOMINATIM_HOST=http://127.0.0.1:18080`, the same provisioned libpostal models, and isolated ClickHouse databases. Seed their administrative-region tables identically, then restart the services to warm their caches:

| Country | Admin code | Level | Country name | Local name | English name |
| --- | --- | --- | --- | --- | --- |
| dk | 84 | 4 | Denmark | Hovedstaden | Capital Region |
| de | BY | 4 | Germany | Bayern | Bavaria |
| fr | IDF | 4 | France | Île-de-France | Ile-de-France |

Use empty parent codes. Benchmarking must not depend on live Nominatim or production databases. The Python reference service is supplied separately; this repository does not bundle one.

```sh
target/release/benchmark \
  --service rust=http://127.0.0.1:18121 \
  --service reference=http://127.0.0.1:18122 \
  --iterations 100 --rounds 3 > benchmark.jsonl
```

The default matrix covers 1/10/100 candidates, local-language corroboration, postcode fallback, concurrency 1/8, and backend delays of 0/10 ms. Service order rotates between rounds. Connections and caches are warmed outside each timed batch. Mock responses are pre-encoded and TCP delayed-ACK artifacts are disabled; injected delays still depend on operating-system scheduling.

Use at least one complete rotation of all service names. Keep timed batches below the servers' per-connection request limits. Compare release builds on the same machine. Results include the mock, client, HTTP transport, native parsing and RPC framework; they are not production throughput or accuracy guarantees.

## Experimental Overture address index

`build_index` builds a Tantivy index and house-number tables from a local [Overture Maps](https://docs.overturemaps.org/) release, without Nominatim. The service does not use it yet; it exists to evaluate replacing Nominatim lookups.

Download the `division` and `division_area` (divisions theme), `segment` (transportation) and `address` (addresses) parquet files of one release into same-named directories, about 100 GB in total, for example:

```sh
aws s3 sync --no-sign-request \
  s3://overturemaps-us-west-2/release/2026-09-23.1/theme=addresses/type=address/ overture/address/
cargo build --locked --release --bin build_index
target/release/build_index overture out/ --geonames-postal allCountries.txt
```

The optional GeoNames postal file (`download.geonames.org/export/zip/allCountries.zip`, CC-BY-4.0) adds postcodes for countries without Overture addresses. The output holds places, streets (one per normalized name per place, merging road names with address streets) and postcodes in Tantivy, plus house numbers with coordinates in sharded FSTs.

Streets and addresses are assigned to the containing locality polygon, else a locality point within 5 km, else the local admin area or county. One world release took about 3 minutes on an 18-core laptop with a 39 GB memory peak; that is a single measurement, not a guarantee.

Known limitations: only the countries in Overture's address theme have house numbers; abbreviated and expanded street names (`AVE`/`Avenue`) are not merged; accuracy has not been evaluated. Overture's divisions and transportation themes are ODbL and address licenses vary by source; review them before distributing an index.

## Geographic evaluation

The `evaluate` binary (`src/bin/evaluate`) measures geographic correctness against a real Nominatim backend through this service's RPCs. It is for backend, data and consumer changes; it does not measure throughput. Build it with the service:

```sh
cargo build --locked --release --bin geolocation --bin evaluate
```

| Suite | Command | Needs |
| --- | --- | --- |
| PR (CI) | `cargo test --locked` (or `target/release/evaluate check`) | Nothing; scorer, diff gate, corpus and snapshot checks |
| Deployed smoke | `target/release/evaluate run --suite smoke --out eval-results/smoke` | Backend, models, ClickHouse |
| Pre-release | `target/release/evaluate run --suite gold --out eval-results/gold` | Same |
| Address snapshots | `target/release/evaluate run --suite addresses --out eval-results/addresses --compare tests/snapshots/addresses.tsv` | Same |
| Compare | `target/release/evaluate diff BASELINE CANDIDATE` | Two saved runs or snapshots |
| Re-score | `target/release/evaluate report RUN_DIR --rescore` | A saved run; lists cases whose class changed |

Set `NOMINATIM_URL` (and optionally `NOMINATIM_AUTHORITY`). `run` starts `target/release/geolocation` (or `SERVICE_BINARY`) with a fresh ClickHouse database at `CLICKHOUSE_URL` and an empty region map. All backend traffic, including the service's own, goes through an in-process proxy. The proxy rate-limits it (`--max-rps`, default 20), enforces a request budget (`--budget`, default 20,000) and attributes calls to cases. `--rpc URL` targets a running service instead; backend counts and region-cache control are then unavailable. Results are checkpointed per case (`--resume`); the database, process and proxy are removed on exit, including after Ctrl-C. No request is retried by the evaluator.

Each run writes `manifest.json` (Nominatim status and data date, a hashed endpoint, consumer commit and binary hash, libpostal model and reference-data hashes, cache state and limits), `results.jsonl` (full responses, backend traces, direct-backend evidence) and `report.md`. Label the backend with `--backend-label`, e.g. image digest and configuration. A code or configuration comparison requires the same OSM snapshot; a data or import change is a separate experiment.

### Gold corpus

`tests/gold.jsonl` is generated by `evaluate build-gold` from the curated case list in `tests/gold.seeds.jsonl` (one line per place or construct: queries, cohort, tolerances, must-pass and known-failure labels). Expected coordinates, countries, regions and components come from Wikidata, Base Adresse Nationale, PDOK/BAG, the US Census geocoder and postcodes.io, never from Nominatim or this service. It covers cities in many scripts, street addresses, postcodes, regions, ambiguous names, rural places, borders, fallbacks, malformed input and no-result input. Each case records an expected country, a resolution range, an area (point and radius, or ISO 3166-2 code), relevant components, a goodMatch expectation, provenance, licence and review status. Variants share a `location` key so they are not counted as independent places. Roughly one in five locations is held out by a stable hash and reported separately; keep it out of tuning. Review status is `source-verified`; human review is pending.

Cases are scored as `pass`, `confidence_mismatch`, `too_coarse` (a fallback never passes an exact-address case), `abstain`, `too_fine`, `wrong_component`, `wrong_region`, `near_miss`, `far_miss` (>100 km), `wrong_country` or `error`. Critical classes are listed individually, flagged when goodMatch was true. For failures the evaluator queries Nominatim directly (raw, structured, reverse, details) to hint whether the backend lacked an acceptable candidate or the consumer rejected one. All four RPCs are exercised. Connect protobuf answers are decoded into the contract type and must equal the Connect JSON answers field for field. grpcurl runs as a known failure and is reported, not skipped.

`known_failure` labels record triaged baseline failures with their attribution. They stay visible in every report, and a label is only removed after a fix is verified. Do not weaken expectations or regenerate them from a run.

### Gates

`run` fails on a must-pass failure, an untriaged critical result or a transport regression. `diff` fails on any newly failing case, a worse failure class, a new critical result or a must-pass failure in the candidate. It lists newly passing cases for review and drift in unscored cases. Accept a backend change only when both gates pass on the same corpus, consumer, models, reference data and fresh region cache. Report cohort changes as case counts: with about 250 cases, aggregate rates carry several points of uncertainty and per-country rows are individual evidence, not rates.

### Address snapshots

`tests/addresses.tsv` holds 1,000 real address and location strings sampled from business addresses, job postings and profile locations. They are spread across 125 countries and stratified by the granularity they target: full address, street, postcode, city, region, country and unparseable. They show how a code or backend change moves everything else; they have no expected answers and measure change, not accuracy. The file holds only the query strings and their geography. Source names, providers, coordinates and identities are removed, and so are imprint addresses and strings naming a company. `tests/snapshots/addresses.tsv` records the accepted behaviour, one row per case: country, level, coordinates (3 decimals, about 100 m), goodMatch, house number, road, postcode, locality, ISO and enriched regions, and the query. A changed answer is one changed row in a PR; `git diff --word-diff` shows just the changed fields. Run metadata (backend version and data date, consumer commit, binary, model and reference-data hashes) is in `addresses.manifest.json` beside it, so it does not clutter the row diff. Full responses stay in the run's `results.jsonl`. Tabs, newlines and other control or separator characters in values are written as backslash escapes.

```sh
target/release/evaluate run --suite addresses --out eval-results/addresses \
  --compare tests/snapshots/addresses.tsv
# after reviewing the changes, accept them
target/release/evaluate snapshot eval-results/addresses tests/snapshots/addresses.tsv
```

The diff counts per target granularity which cases gained or lost a result, changed country, became finer or coarser, moved more than 1 km (or less, but visibly in the snapshot), flipped goodMatch, or changed components or regions, and lists each one. `--fail-on-drift` turns any change into a failure. Rerunning an unchanged binary against the same backend produces no drift. A changed answer is not automatically better or worse. Snapshots depend on the backend's data; regenerate the baseline on the target backend before comparing code changes there.

The sampler that queries production data is kept outside this repository; it reuses `geolocation::eval` for TSV escaping and hold-out assignment. Other corpora are JSONL in the gold format, or TSV with at least `id` and `query` columns, passed with `--corpus`. Private ones (e.g. replay sets with earlier production geocodes) use the same tooling; keep those and their runs out of this repository.

GitHub-hosted runners cannot reach an IP-restricted backend and do not download models, so CI runs only the offline checks in `cargo test`. Run deployed suites locally or from a runner inside the trusted network.

## GHCR publishing

`.github/workflows/docker.yml` runs on pushes to `main`, `v*` tag pushes and manual dispatch. It runs the existing checks first, then uses Docker's pinned reusable workflow to build on native amd64/arm64 runners, push to `ghcr.io/mokto/geolocation`, and assemble and verify the multi-platform manifest. Pull requests do not publish images.

Tags include the sanitized branch name and `sha-<full-commit-SHA>`. A `v1.2.3` Git tag produces `1.2.3`; only `main` updates `latest`. Publishing uses the workflow's `GITHUB_TOKEN` with `packages: write`, so no personal access token secret is required. OCI labels link the package to this repository. Crates and GitHub releases are not published.

GHCR can initially create packages as private even for public repositories. After the first successful push, set the [package visibility](https://github.com/users/Mokto/packages/container/geolocation/settings) to public if anonymous image pulls are required.

## Reference data and normalization

Country aliases use public Unicode CLDR and GeoNames data. Administrative-region hints use GeoNames. Sources, revision and download hashes are recorded in `data/sources.json`; the checked-in data allows offline reference generation during normal builds.

To refresh it, run `cargo run --locked --bin update_reference`. This requires network access and changes the checked-in dataset; review it as a data update. Character classes come from the Unicode Character Database version pinned in the generator (currently 16.0.0), not from the toolchain.

An optional build-time `REFERENCE_DATA` path can supply a custom reference JSON. Search overrides are empty by default. No private deployments, endpoints or address overrides are included.

Transliteration uses BSD-3-Clause `deunicode`, **not Python Unidecode**. Outputs differ for some scripts and symbols. In a migration comparison of 7,067 geographic/sample strings, 55 normalized outputs differed, including Hebrew and Japanese forms. Matching normalizes both sides consistently, but these differences still need real-address accuracy validation before replacing another service.

## License

Project source: **FSL-1.1-Apache-2.0** (Functional Source License). Any use is allowed except a Competing Use: making the software available in a commercial product or service that substitutes for it or offers substantially similar functionality. Each release converts to Apache-2.0 two years after publication. This is source-available, not OSI open source. See `LICENSE`, `NOTICE` and `LICENSES/` for third-party software and reference-data terms.

libpostal and rustpostal are MIT-licensed; deunicode is BSD-3-Clause. CLDR data is Unicode-3.0; GeoNames data is CC-BY-4.0. Model files are not bundled; their source-data licensing and attribution obligations are separate.
