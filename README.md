# Geolocation

A Rust service that combines libpostal address parsing, Nominatim lookups and ClickHouse-backed administrative regions.

- Connect JSON and Connect protobuf; an experimental legacy gRPC surface.
- Forward geocoding, reverse region resolution and region-map refresh/read APIs.
- Country/region corroboration, local-language candidate matching, postcode-area safeguards and ordered fallback queries.
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

`latest` is published from `main`. Before the initial import is merged, use the branch tag `ghcr.io/mokto/geolocation:feat-initial-geolocation` instead. Models are not included in either image.

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
curl http://localhost:11121/nominatim_geolocation.NominatimGeolocation/SearchAddress \
  -H 'Content-Type: application/json' \
  -H 'Connect-Protocol-Version: 1' \
  -d '{"addressString":"Copenhagen, Denmark"}'
```

The protocol definitions are in `proto/`. The four methods are `SearchAddress`, `ResolveRegion`, `RefreshRegionMap` and `GetRegionMap`.

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

`tests/rpc.py` exercises Connect JSON/protobuf, persistence, candidate safeguards, concurrent requests, metrics and shutdown against a real isolated ClickHouse database and a local HTTP fixture. It creates and removes its own database. Install `grpcurl` to include gRPC/reflection checks:

```sh
CLICKHOUSE_URL=http://127.0.0.1:8125 python3 tests/rpc.py
```

To test the final container instead of a local binary:

```sh
SERVICE_IMAGE=geolocation LIBPOSTAL_MODELS="$PWD/models/libpostal" \
  CLICKHOUSE_URL=http://127.0.0.1:8125 python3 tests/rpc.py
```

CI builds and checks both amd64 and arm64 without downloading models. Model-backed RPC tests run separately with provisioned models.

## GHCR publishing

`.github/workflows/docker.yml` runs on repository branch pushes, `v*` tag pushes and manual dispatch. It runs the existing checks first, then uses Docker's pinned reusable workflow to build on native amd64/arm64 runners, push to `ghcr.io/mokto/geolocation`, and assemble and verify the multi-platform manifest. Pull requests do not publish images.

Tags include the sanitized branch name and `sha-<full-commit-SHA>`. A `v1.2.3` Git tag produces `1.2.3`; only `main` updates `latest`. Publishing uses the workflow's `GITHUB_TOKEN` with `packages: write`, so no personal access token secret is required. OCI labels link the package to this repository. Crates and GitHub releases are not published.

GHCR can initially create packages as private even for public repositories. After the first successful push, set the [package visibility](https://github.com/users/Mokto/packages/container/geolocation/settings) to public if anonymous image pulls are required.

## Reference data and normalization

Country aliases use public Unicode CLDR and GeoNames data. Administrative-region hints use GeoNames. Sources, revision and download hashes are recorded in `data/sources.json`; the checked-in data allows offline reference generation during normal builds.

To refresh it, run `python3 tools/update_reference.py`. This requires network access and changes the checked-in dataset; review it as a data update.

An optional build-time `REFERENCE_DATA` path can supply a custom reference JSON. Search overrides are empty by default. No private deployments, endpoints or address overrides are included.

Transliteration uses BSD-3-Clause `deunicode`, **not Python Unidecode**. Outputs differ for some scripts and symbols. In a migration comparison of 7,067 geographic/sample strings, 55 normalized outputs differed, including Hebrew and Japanese forms. Matching normalizes both sides consistently, but these differences still need real-address accuracy validation before replacing another service.

## License

Project source: **Apache-2.0**. See `LICENSE`, `NOTICE` and `LICENSES/` for third-party software and reference-data terms.

libpostal and rustpostal are MIT-licensed; deunicode is BSD-3-Clause. CLDR data is Unicode-3.0; GeoNames data is CC-BY-4.0. Model files are not bundled; their source-data licensing and attribution obligations are separate.
