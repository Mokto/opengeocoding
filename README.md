# OpenGeocoding

A Rust service that combines libpostal address parsing, Overture-backed index lookups and ClickHouse-backed administrative regions.

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

`latest` is published from `main`. Models and indexes are not included in the image.

Download libpostal models explicitly into a writable directory. Downloads are large; nothing downloads automatically at import or startup:

```sh
mkdir -p models
docker run --rm --entrypoint libpostal_data \
  -v "$PWD/models:/data" geolocation download all /data
```

The resulting directory is `models/libpostal`. Keep it for subsequent runs.

Build an index into `out/` (see the index section below), start `serve_index`, and provide your own ClickHouse instance:

```sh
docker network create geolocation
docker run -d --name geolocation-index --network geolocation \
  --entrypoint serve_index \
  -v "$PWD/out:/index:ro" \
  -v "$PWD/models/libpostal:/data/libpostal:ro" \
  geolocation /index 0.0.0.0:8099 overture-2026-09-23.1

docker run --rm --network geolocation \
  -p 11121:11121 -p 8000:8000 \
  -v "$PWD/models/libpostal:/data/libpostal:ro" \
  -e INDEX_URL=http://geolocation-index:8099 \
  -e CLICKHOUSE_URL=http://your-clickhouse:8123 \
  -e CLICKHOUSE_USER=default \
  -e CLICKHOUSE_PASSWORD=your-password \
  geolocation
```

Addresses inside Docker must be reachable from the container; `localhost` refers to that container. The service creates its ClickHouse database/table and warms its region cache at startup.

**Deploy behind a trusted network or authenticated proxy.** The RPCs, including region-map refresh, do not implement authentication. Do not expose them directly to the public internet. Keep `serve_index` on the same trusted network; its HTTP endpoints are also unauthenticated.

Example request:

```sh
curl http://localhost:11121/opengeocoding.Geolocation/SearchAddress \
  -H 'Content-Type: application/json' \
  -H 'Connect-Protocol-Version: 1' \
  -d '{"addressString":"Copenhagen, Denmark"}'
```

The contract is `proto/opengeocoding-v1.proto`, package `opengeocoding`, service `Geolocation`. The four methods are `SearchAddress`, `ResolveRegion`, `RefreshRegionMap` and `GetRegionMap`.

The canonical RPC path is `/opengeocoding.Geolocation/*`. The Go package is `github.com/Mokto/opengeocoding/gen/opengeocoding`; the Rust crate remains `geolocation`, with generated types under `geolocation::proto::opengeocoding`.

`/geolocation.Geolocation/*` and `/nominatim_geolocation.NominatimGeolocation/*` remain aliases for every method, including for benchmarking. Message fields, numbers and types are unchanged. These aliases do not enable a Nominatim backend. gRPC reflection advertises only `opengeocoding.Geolocation`, and the `grpc_service` metric label uses this canonical name on all routes.

## Known interoperability issue

The initial model-backed integration run passes Connect JSON/protobuf, local-language corroboration, country/postcode rejection, reverse regions, map refresh, retry/error handling and 64 concurrent requests. However, grpcurl 1.9.4 can list services but its SearchAddress invocation hangs, both with reflection and an explicit proto file. The integration script fails with a bounded timeout rather than skipping that failure. Legacy gRPC interoperability is not signed off; do not treat this initial push as a production-ready replacement.

## Configuration

| Variable | Default |
| --- | --- |
| `INDEX_URL` | Required; `serve_index` HTTP(S) base URL |
| `CLICKHOUSE_URL` | `http://127.0.0.1:8123` |
| `CLICKHOUSE_DATABASE` | `geolocation` |
| `CLICKHOUSE_USER` / `CLICKHOUSE_PASSWORD` | `default` / empty |
| `CLICKHOUSE_PREFIX` | Empty; otherwise prefixes the table name |
| `CLICKHOUSE_SINGLE_NODE` | Enabled; an empty value selects clustered DDL |
| `REGION_MAP_PATH` | `regions.json` in the working directory |
| `RPC_PORT` / `METRICS_PORT` | `11121` / `8000` |
| `RUST_LOG` | `info` |
| `SENTRY_DSN` / `ENV` | Optional |

The service validates `/status` at startup and accepts only a ready `serve_index` backend (`software_version: overture-index`, `status: 0`). `NOMINATIM_HOST`, `NOMINATIM_URL` and `NOMINATIM_AUTHORITY` are no longer supported; there is no Nominatim fallback. Legacy RPC paths and metric prefixes remain for compatibility, not backend selection.

Clustered ClickHouse needs the usual `cluster`, `shard` and `replica` macros. Region-map refresh uses atomic file replacement; its directory must be writable. The default container's `/app` directory belongs to uid/gid 1000.

`/healthz` reports readiness after initialization. Metrics are exposed at `/metrics` on port 8000.

`GetRegionMap` returns a Python-style dictionary string for wire compatibility; the stored file is JSON. Coordinates use protobuf float32. Postcode compatibility deliberately includes shared outward/area codes for supported national schemes; it is not a promise of an exact street-level postcode match.

## Build and test

Local builds require Rust 1.99+, Buf and a compiled libpostal installation. Native library/model paths are determined by libpostal's configure options. Nonstandard linker paths can be supplied through `RUSTFLAGS="-L native=/path/to/lib"`.

```sh
cargo build --locked --bin geolocation --bin serve_index
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

CI builds and checks both amd64 and arm64 without downloading models. [CodSpeed](https://codspeed.io/) runs the `benches/` suites (the index analyzer and house-number keys; the service's candidate matching) in its simulation mode on pushes to `main` and on pull requests, and reports changes in instruction counts; run them locally with `cargo bench`, or `cargo codspeed build && cargo codspeed run`. The workflow builds libpostal without models, since the benchmarks link it but never parse. Regenerate `THIRD_PARTY_NOTICES.txt` with `cargo run --locked --bin notices` after dependency changes. Model-backed RPC tests run separately with provisioned models.

## Local performance benchmark

The `benchmark` binary provides a loopback `serve_index` mock and a Connect JSON benchmark. It compares full responses and checks that each implementation makes the same number of backend requests. No failed requests are retried. Its fixture and validation checks run in `cargo test`.

```sh
cargo build --locked --release --bin geolocation --bin benchmark
target/release/benchmark --serve 18080
```

Start release-built local services with `INDEX_URL=http://127.0.0.1:18080`, the same provisioned libpostal models, and isolated ClickHouse databases. Seed their administrative-region tables identically, then restart the services to warm their caches:

| Country | Admin code | Level | Country name | Local name | English name |
| --- | --- | --- | --- | --- | --- |
| dk | 84 | 4 | Denmark | Hovedstaden | Capital Region |
| de | BY | 4 | Germany | Bayern | Bavaria |
| fr | IDF | 4 | France | Île-de-France | Ile-de-France |

Use empty parent codes. Benchmarking must not depend on production backends or databases. The Python reference service is supplied separately; this repository does not bundle one.

```sh
target/release/benchmark \
  --service rust=http://127.0.0.1:18121 \
  --service reference=http://127.0.0.1:18122 \
  --iterations 100 --rounds 3 > benchmark.jsonl
```

The default matrix covers 1/10/100 candidates, local-language corroboration, postcode fallback, concurrency 1/8, and backend delays of 0/10 ms. Service order rotates between rounds. Connections and caches are warmed outside each timed batch. Mock responses are pre-encoded and TCP delayed-ACK artifacts are disabled; injected delays still depend on operating-system scheduling.

A benchmark service URL without a route uses `opengeocoding.Geolocation`; an explicit `/opengeocoding.Geolocation` suffix also works. To benchmark old clients, use `--service legacy=http://127.0.0.1:18121/geolocation.Geolocation` or `--service legacy=http://127.0.0.1:18121/nominatim_geolocation.NominatimGeolocation`. All three routes use the same index-backed resolver.

Use at least one complete rotation of all service names. Keep timed batches below the servers' per-connection request limits. Compare release builds on the same machine. Results include the mock, client, HTTP transport, native parsing and RPC framework; they are not production throughput or accuracy guarantees.

## Overture address index

`build_index` builds a Tantivy index and house-number tables from a local [Overture Maps](https://docs.overturemaps.org/) release, without Nominatim. `serve_index` serves that index and is the service's only supported lookup backend. The backend remains experimental; the coverage and reverse-lookup limitations below still apply.

Download the `division` and `division_area` (divisions theme), `segment` (transportation) and `address` (addresses) parquet files of one release into same-named directories, about 100 GB in total, for example:

```sh
aws s3 sync --no-sign-request \
  s3://overturemaps-us-west-2/release/2026-09-23.1/theme=addresses/type=address/ overture/address/
cargo build --locked --release --bin build_index
target/release/build_index overture out/ --openaddresses openaddresses/ --osm osm/ \
  --geonames-postal GB_full.txt --geonames-postal allCountries.txt
```

Overture's address theme covers about 40 countries. The optional sources fill the rest:

- `--osm DIR`: every `*.osm.pbf` in the directory. Addressed nodes, addressed ways and multipolygon relations at their first node, `addr:place` addresses (numbered within the place), and `addr:interpolation` ways (the numbers between their addressed nodes, spaced along the way; Argentina's block ranges alone give about 45 million); the country is that of the containing place. Files must be sorted by type then id, as [Geofabrik](https://download.geofabrik.de/) extracts are. Pick the extracts for countries without Overture addresses from `download.geofabrik.de/index-v1-nogeom.json` (`iso3166-1:alpha2`); about 150 extracts and 30 GB in October 2026. Add extracts where Overture's coverage has holes: in October 2026 it had no addresses for Bavaria or the city of Milan, so `germany/bayern` and `italy` were added (3 GB). Extracts overlap at borders and with Overture; duplicate house numbers collapse.
- `--openaddresses DIR`: every `*.geojson.gz` in the directory, one OpenAddresses batch job output each, named `<cc>_<source path>-<layer>.geojson.gz` (the country comes from the prefix). List address layers with `batch.openaddresses.io/api/data` and download `v2.openaddresses.io/batch-prod/job/<job>/source.geojson.gz` for the sources Overture doesn't list (about 2,700 files, 13 GB in October 2026). Many overlap Overture's data; identical house numbers on the same street collapse, and in a sample 19% of US and 60% of Mexican OpenAddresses addresses were on a street the Overture-only index had without that number.
- `--geonames-postal FILE`, repeatable: GeoNames postal-code dumps (`download.geonames.org/export/zip/`, CC-BY-4.0). `allCountries` has only outward codes for GB and 3-character codes for CA; `GB_full`, `CA_full` and `NL_full` hold full codes. A postcode is added only when the address data and earlier files lack it.

The output holds places, streets (one per normalized name per place and 0.1° cell, merging road names with address streets) and postcodes in Tantivy, plus house numbers with coordinates in sharded FSTs. The cell keeps same-named streets of one large locality apart (Port Chalmers' and central Dunedin's Beach Street); a street crossing a cell edge is indexed in parts, and the shim answers the middle of the parts. Addresses without a street (Overture's Polish, Czech, Slovak and Baltic village addresses, OSM `addr:place`) are numbered on a street named after their village.

Names are folded by one analyzer at build and query time: Unicode tokens, lowercased, ASCII-folded, then street-name abbreviations canonicalized (`src/bin/build_index/canon.rs`: `Avenue`/`Ave`/`Avenida`/`Av.` → `av`, `Straße` and compounds → `str`, `Calle`/`C/` → `cl`, `Drottninggatan` → `drottningg` like `Drottningg.`, and so on for English, Spanish, Portuguese, French, German, Dutch, Nordic, Hungarian, Italian, Turkish, Polish, Russian and Ukrainian; Japanese chōme and Taiwanese section numerals read as digits, so `芝公園四丁目` and `芝公園4丁目` match). A query must use the same analyzer, including when correcting typos against the canonical vocabulary.

Streets and addresses are assigned to the containing locality polygon, else a locality point within 5 km, else the local admin area or county. House numbers are spilled to `out/.numbers-spill` during the build (about 7 GB for the world) and removed afterwards. The builder reads available memory at startup (cgroup limit and usage, capped by the host's `MemAvailable`; `vm_stat` on macOS) and sizes the index writer and the number sort from the remaining headroom. One world release took about 100 seconds on an 18-core laptop with a 19 GB peak memory footprint; with all the OSM extracts, OpenAddresses and GeoNames files above it took about 300 seconds and 20 GB (OpenAddresses about 75 seconds, OSM about 105), for 563 million house numbers and 43 million streets in 12 GB. These are single measurements, not guarantees, and the build needs that memory regardless of the writer budget.

Known limitations: outside Overture's address countries, house numbers are only as complete as OSM and OpenAddresses there (sparse in the UK, India and much of Africa and Asia); OSM ways and relations are placed at their first node; interpolated numbers assume even spacing; abbreviations outside that table (`MLK`, `CR 123` for County Road) still index as separate streets; house numbers are matched lowercased and without full-width forms or a trailing 号/號, otherwise exactly as written. Overture's divisions and transportation themes and OpenStreetMap are ODbL (attribution "© OpenStreetMap contributors", share-alike for a distributed derived database); OpenAddresses licenses vary per source and many state none; review them before distributing an index.

### Evaluating the index through the service

`serve_index` answers `/search`, `/details`, `/reverse`, `/parse` and `/status` from a `build_index` output. Its JSON responses retain the existing candidate format; this does not make Nominatim a supported backend. Build and evaluate it through the service:

```sh
cargo build --locked --release --bin geolocation --bin serve_index --bin evaluate
target/release/serve_index out/ 127.0.0.1:8099 overture-2026-09-23.1 &
INDEX_URL=http://127.0.0.1:8099 target/release/evaluate run --suite gold --out eval-results/gold-index --max-rps 200
target/release/evaluate diff BASELINE_GOLD_RUN eval-results/gold-index
```

It parses with libpostal (serialized), matches places, streets, postcodes and house numbers like Nominatim's all-terms search, and recognizes postcodes from the index when libpostal mislabels them. It reads Japanese block addresses (`東京都港区芝公園4-2-8` is chōme 4 of 芝公園, block 2, house 8) and Chinese road addresses itself, since libpostal splits them. Streets are looked up in the named town, then a same-named county, then a same-named city-province (Istanbul), then by postcode and near the postcode's centre. It reports each house's own postcode, as the source data gives it. It remains experimental: `/reverse` returns the nearest locality point, not a containing administrative polygon. It reads importance through cached per-segment fast-field columns and checks exact names against hashes folded at startup; Tantivy's `order_by_fast_field` reopened every segment's column per query and cost more than the search itself. On an 18-core laptop shared with other work it answered about 1,100 of the service's recorded backend searches per second on one core and 6,500 on about 8 cores (p99 7 ms), with about 4 GB of private memory (libpostal about 2 GB, the place table about 2 GB). The 12 GB index (Tantivy, house-number FSTs and `places.tsv`) is memory-mapped; in an earlier measurement 1.3 GB of it was resident after 20 seconds of that load. The service in front of it handled 2,200 address requests per second from the production sample on about 7 cores in total (p99 23 ms) and 3,500 when the machine saturated. These are single measurements on one machine, not capacity guarantees.

In one run against the October 2026 Overture release with the OSM, OpenAddresses and GeoNames sources above, the gold suite passed 1,038 of its 1,280 cases (231 of the 286 held out), against 992 for the index and shim before the October 13 data and parsing work. Of the original 247 cases it passed 239; Nominatim 5.3.2 with January 2026 OSM data passed 227, and the only case Nominatim passed and the index did not is a Dubai boulevard whose street data names a smaller locality than Dubai. Of the 405 address cases (Wikidata landmarks in 90 countries and registry addresses from France, the Netherlands, the United States, Norway, Switzerland and Spain), it passed 286. Most remaining failures are data gaps (streets or villages missing in OSM-only countries) or the service rejecting the index's right first answer on libpostal's parse ("Addis Ababa, Ethiopia" parses as one city name); those critical ones are labelled in `tests/gold.seeds.jsonl`. On the 1,000 address samples, the index answered 827 (Nominatim 855) and 143 at house level (117). Where the sample's country is known, it chose the right country for 97.3% of its answers (Nominatim 97.5%) and claimed a good match in the wrong country 14 times (Nominatim 14). To keep wrong countries down, the shim answers a street with nothing to place it only when at most three same-named streets with that number exist, all in one country. It answers a place name without a country only on an exact name match, and a bare postcode without a country only when one country uses it. These are single runs on curated and sampled inputs, not production accuracy.

## Geographic evaluation

The `evaluate` binary (`src/bin/evaluate`) measures geographic correctness against `serve_index` through this service's RPCs. It is for backend, data and consumer changes; it does not measure throughput. Build it with the service:

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

Set `INDEX_URL` to a running `serve_index` backend. `run` starts `target/release/geolocation` (or `SERVICE_BINARY`) with a fresh ClickHouse database at `CLICKHOUSE_URL` and an empty region map. All backend traffic, including the service's own, goes through an in-process proxy. The proxy rate-limits it (`--max-rps`, default 20), enforces a request budget (`--budget`, default 20,000) and attributes calls to cases. `--rpc URL` targets a running service instead; backend counts and region-cache control are then unavailable. Results are checkpointed per case (`--resume`); the database, process and proxy are removed on exit, including after Ctrl-C. No request is retried by the evaluator.

Each run writes `manifest.json` (index status and release label, a hashed endpoint, consumer commit and binary hash, libpostal model and reference-data hashes, cache state and limits), `results.jsonl` (full responses, backend traces, direct-backend evidence) and `report.md`. Label the backend with `--backend-label`, e.g. image digest and configuration. A code or configuration comparison requires the same built index; a data or import change is a separate experiment.

### Gold corpus

`tests/gold.jsonl` is generated by `evaluate build-gold` from the curated case list in `tests/gold.seeds.jsonl` (one line per place or construct: queries, cohort, tolerances, must-pass and known-failure labels). Expected coordinates, countries, regions and components come from Wikidata, Base Adresse Nationale, PDOK/BAG, the US Census geocoder, postcodes.io, Kartverket (© Kartverket, CC BY 4.0), swisstopo's building address register (© swisstopo) and CartoCiudad (© IGN/CNIG, CC BY 4.0), never from Nominatim or this service. Sampled cities, towns and regions in about 240 countries and territories were picked from GeoNames by population and production weight, sampled registry addresses from Overture, and landmarks from Wikidata items with a street address; GeoNames and Overture only chose the cases, and their expectations come from the sources above. A place in a territory with its own ISO code (Puerto Rico, Réunion) accepts that code or the sovereign country's. It covers cities in many scripts, street addresses, postcodes, regions, ambiguous names, rural places, borders, fallbacks, malformed input and no-result input. Each case records an expected country, a resolution range, an area (point and radius, or ISO 3166-2 code), relevant components, a goodMatch expectation, provenance, licence and review status. Variants share a `location` key so they are not counted as independent places. Roughly one in five locations is held out by a stable hash and reported separately; keep it out of tuning. Review status is `source-verified`; human review is pending.

Cases are scored as `pass`, `confidence_mismatch`, `too_coarse` (a fallback never passes an exact-address case), `abstain`, `too_fine`, `wrong_component`, `wrong_region`, `near_miss`, `far_miss` (>100 km), `wrong_country` or `error`. Critical classes are listed individually, flagged when goodMatch was true. For failures the evaluator queries the index directly (raw, structured, reverse, details) to hint whether the backend lacked an acceptable candidate or the consumer rejected one. All four RPCs are exercised. Connect protobuf answers are decoded into the contract type and must equal the Connect JSON answers field for field. grpcurl runs as a known failure and is reported, not skipped.

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

`tests/snapshots/gold.nominatim.json` is a reference run of the gold suite through this service against the Nominatim 5.3.2 deployment it replaces (OSM data of 2026-01-26), taken on 2026-10-13 at 20 requests per second: 888 of 1,280 cases passed (227 of the original 247, 194 of the 286 held out). It holds the run manifest, the class counts and every case's full result except timings and backend traces, one case per line. It is kept for reference and is not regenerated when the gold set changes; compare a run with it using `evaluate diff tests/snapshots/gold.nominatim.json RUN_DIR` (cases added since then show as new). `evaluate snapshot RUN_DIR FILE.json` writes this format.

The diff counts per target granularity which cases gained or lost a result, changed country, became finer or coarser, moved more than 1 km (or less, but visibly in the snapshot), flipped goodMatch, or changed components or regions, and lists each one. `--fail-on-drift` turns any change into a failure. Rerunning an unchanged binary against the same backend produces no drift. A changed answer is not automatically better or worse. Snapshots depend on the backend's data; regenerate the baseline on the target backend before comparing code changes there.

The sampler that queries production data is kept outside this repository; it reuses `geolocation::eval` for TSV escaping and hold-out assignment. Other corpora are JSONL in the gold format, or TSV with at least `id` and `query` columns, passed with `--corpus`. Private ones (e.g. replay sets with earlier production geocodes) use the same tooling; keep those and their runs out of this repository.

GitHub-hosted runners cannot reach an IP-restricted backend and do not download models, so CI runs only the offline checks in `cargo test`. Run deployed suites locally or from a runner inside the trusted network.

## GHCR publishing

`.github/workflows/docker.yml` runs on pushes to `main`, `v*` tag pushes and manual dispatch. It runs the existing checks first, then uses Docker's pinned reusable workflow to build on native amd64/arm64 runners, push to `ghcr.io/mokto/geolocation`, and assemble and verify the multi-platform manifest. Pull requests do not publish images.

The repository is `Mokto/opengeocoding`; the existing GHCR image remains `ghcr.io/mokto/geolocation`. The repository rename does not rename the Rust crate, executable or registry package.

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
