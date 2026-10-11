# Agent instructions

## Scope and approach

This is a standalone public Rust geolocation service. Read `README.md` before changing behavior or deployment assumptions.

- Prefer the standard library and existing dependencies. Keep changes small; avoid speculative abstractions and unrelated cleanup.
- Comments explain hidden constraints, not what the code does.
- Never include credentials, private endpoints, internal tickets, proprietary overrides or private checkout dependencies in source, documentation, commits or pull requests.
- Do not claim production accuracy, throughput or interoperability from compilation or unit tests alone. Keep known limitations visible.

## Repository map

- `src/matching.rs`: normalization, country/region corroboration and candidate selection.
- `src/resolver.rs`: native parsing, Nominatim HTTP calls, candidate formatting and fallback ordering.
- `src/regions.rs`: ClickHouse persistence, cache and atomic region-map refresh.
- `src/service.rs`, `src/main.rs`, `src/metrics.rs`: RPCs, process lifecycle and observability.
- `proto/`, `build.rs`: wire contracts and build-time generation.
- `data/`, `tools/update_reference.py`: public reference data and its generator.
- `tests/matching.rs`, `tests/rpc.py`: regression tests and model-backed RPC integration checks.
- `Dockerfile`, `.github/workflows/`: runtime packaging, CI and GHCR publishing.

## Compatibility and safety

- Preserve protobuf package/service names, field numbers and types. Modify `proto/` and regenerate through Cargo; never edit generated files under `target/`.
- Preserve deliberate fallback ordering, country/region conflict guards, postcode-area compatibility, local-language candidate corroboration and `good_match` semantics.
- Coordinates use protobuf float32. `GetRegionMap` returns a Python-style dictionary string; the stored map is JSON. Neither is an accidental formatting choice.
- Validate input at trust boundaries. Do not expose backend details in RPC errors.
- Region-map refresh must remain atomic. Test database changes against an isolated database, never a shared production instance.
- libpostal is native C behind `rustpostal`, not a pure-Rust parser. Keep parsing off the async executor and serialized until native thread-safety testing supports widening concurrency.
- The RPCs currently have no authentication. Do not present them as safe to expose publicly without an authenticated proxy or trusted network.

## Builds and checks

Host builds require Rust 1.99+, Buf and compiled libpostal. Nonstandard library paths may require `RUSTFLAGS="-L native=/path/to/lib"`.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
docker build --target checks .
docker build --target runtime -t geolocation .
```

- Leave a regression check for nontrivial logic. Run the relevant checks and report exactly what ran, including failures and unavailable prerequisites.
- Do not skip tests, weaken assertions or add retries to disguise failures. Revert ineffective fixes before trying another approach.
- Run `tests/rpc.py` with an isolated ClickHouse instance and provisioned models for parsing, RPC, persistence or lifecycle changes. Install `grpcurl` to exercise its gRPC checks; its known failure is documented in the README.
- Preserve amd64 and arm64 support. CI unit checks do not download models or establish model-backed accuracy.
- Performance claims require comparable measurements and accuracy checks; synthetic CPU timings are not request-latency guarantees.

## Licensing, data and models

- Project source is Apache-2.0. Keep dependencies compatible with permissive distribution and retain their required notices.
- Transliteration uses BSD-3-Clause `deunicode`. Do not copy or export GPL Unidecode tables to restore old outputs. Transliteration changes require multilingual accuracy checks.
- CLDR and GeoNames data retain their separate licenses and attribution. Refresh `data/` through `tools/update_reference.py`; review dataset changes and provenance together.
- Regenerate `THIRD_PARTY_NOTICES.txt` with `python3 tools/dependency_notices.py` after dependency changes. Do not edit generated data or notices by hand.
- Models are external, large and not bundled in the runtime image. Do not add automatic startup downloads or commit model files. The container expects a read-only model mount at `/data/libpostal`.
- Keep the runtime Python-free and non-root (uid/gid 1000), with a writable region-map directory. Do not conflate a parser/model upgrade with unrelated service changes.

## Git and publishing

- Use feature branches and pull requests; never push directly to `main` or continue a branch whose PR has merged.
- Keep public PR descriptions accurate after every push and free of internal references.
- Pin GitHub Actions and reusable workflows by commit SHA. Keep publishing credentials out of pull-request jobs.
- Branch pushes trigger checked, multi-platform image publishing to `ghcr.io/mokto/geolocation`. Only `main` updates `latest`; branch, SHA and version tags are separate.
- Do not merge, create release tags, publish crates or change package visibility without explicit authorization. GHCR publication already configured by this repository runs automatically on branch pushes.
