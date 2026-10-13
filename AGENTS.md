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
- `proto/geolocation-v1.proto`, `build.rs`: the single wire contract and build-time generation.
- `data/`, `src/bin/update_reference.rs`: public reference data and its generator.
- `tests/matching.rs`, `tests/rpc.rs`: regression tests and the model-backed RPC integration test (ignored by default).
- `src/bin/benchmark.rs`, `src/bin/notices.rs`: local mock benchmark and third-party notice generation.
- `src/bin/build_index/`: experimental Overture-to-Tantivy index builder (polygon assignment, street merging, house-number FSTs; `sources.rs` reads OSM and OpenAddresses); `src/bin/serve_index/`: Nominatim-compatible HTTP shim over that index, for evaluating it through the unchanged service; not used by the service.
- `src/bin/evaluate/`, `src/eval.rs`: geographic-correctness evaluator (scoring, runs, reports, snapshots, gold builder) and the TSV/hold-out helpers it shares with external samplers.
- `tests/gold.seeds.jsonl`, `tests/gold.jsonl`: curated gold case list and the corpus generated from it.
- `benches/`: CodSpeed benchmark suites (divan); `.github/workflows/codspeed.yml` runs them.
- `tests/addresses.tsv`, `tests/snapshots/addresses.tsv`: unscored real-address corpus and its accepted snapshot for change detection.
- `Dockerfile`, `.github/workflows/`: runtime packaging, CI and GHCR publishing.

## Compatibility and safety

- The package is `geolocation`; the service is `Geolocation`. Messages, field numbers and types are unchanged from the earlier `nominatim_geolocation.NominatimGeolocation` service, whose paths `src/main.rs` still rewrites to the new ones. Preserve the package and service names, field numbers, types and that legacy path until every caller uses the new package. Modify `proto/` and regenerate through Cargo; never edit generated files under `target/`.
- Preserve deliberate fallback ordering, country/region conflict guards, postcode-area compatibility, local-language candidate corroboration and `good_match` semantics (false for structured fallbacks and for a missing requested house number). Other-language names (`namedetails`) corroborate settlements, and regions only for bare place names; they never justify overriding a recognized stated country.
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
- Run `cargo test --test rpc -- --ignored --nocapture` with an isolated ClickHouse instance (`CLICKHOUSE_URL`) and provisioned models for parsing, RPC, persistence or lifecycle changes. Install `grpcurl` to exercise its gRPC checks; its known failure is documented in the README.
- Evaluator and corpus checks run in `cargo test` (also `target/release/evaluate check`). After changing scoring or expectations, re-score saved runs with `evaluate report RUN_DIR --rescore` and explain every changed class. For changes to parsing, matching, fallbacks, reference data or the backend, also run the smoke or gold suite against an agreed backend (`NOMINATIM_URL`) and compare snapshots with `--compare`; see the README's Geographic evaluation section.
- Preserve amd64 and arm64 support. CI unit checks do not download models or establish model-backed accuracy.
- Performance claims require comparable measurements and accuracy checks; synthetic CPU timings are not request-latency guarantees.

## Evaluation corpora

- Expected answers in `tests/gold.jsonl` come only from licensed authoritative sources via `evaluate build-gold`. Curate cases in `tests/gold.seeds.jsonl`, regenerate, and review the corpus diff as a data update. Never derive or regenerate expectations from Nominatim, this service or another geocoder's responses, and never hand-edit `tests/gold.jsonl`.
- Nonempty responses, `goodMatch=true`, agreement with another implementation and stored production coordinates are not ground truth.
- Do not weaken expectations, drop cases or skip known failures to make a gate pass. `known_failure` labels record triaged, attributed baseline failures (backend, parser, matching, reference data or region cache). Remove one only after verifying a fix. Triage every new critical result before labelling it.
- Keep the held-out subset (stable location hash) out of tuning. Variants of one place share a `location` key and are not independent samples.
- `tests/addresses.tsv` may contain only the columns in `geolocation::eval::ADDRESS_COLUMNS` (query strings and their geography; `cargo test` enforces this). Never add source or provider names, coordinates, company names, domains, people or profile identifiers, or imprint and other likely home addresses. It is regenerated by a sampler kept outside this repository; do not hand-edit it or add the sampler, its internal queries or raw pools here.
- Address cases measure change, not accuracy. Keep snapshots as one TSV row per case with the manifest in a separate file, so a PR diff shows only changed answers. Replace `tests/snapshots/addresses.tsv` only after reviewing the diff, and say in the PR which changes were accepted and why. Do not regenerate the snapshot to silence drift.
- Other corpora with production metadata (earlier geocodes, source labels) and their runs stay private and out of this repository.
- Use a fresh region cache and the same consumer binary, models and reference data on both sides of a comparison. Confirm a zero-drift rerun before trusting a diff. Treat a code/configuration change and a data/import change as separate experiments.
- Respect the backend's request budget and rate (`--max-rps`, `--budget`). Do not add evaluator retries. Load testing needs an approved isolated backend.
- The grpcurl SearchAddress hang remains a known failure even if a run reports it passing; it reproduces on a fresh service.

## Licensing, data and models

- Project source is FSL-1.1-Apache-2.0. Keep dependencies compatible with distribution under it and retain their required notices.
- Transliteration uses BSD-3-Clause `deunicode`. Do not copy or export GPL Unidecode tables to restore old outputs. Transliteration changes require multilingual accuracy checks.
- CLDR and GeoNames data retain their separate licenses and attribution. Refresh `data/` through `cargo run --bin update_reference`; review dataset changes and provenance together. Changing the pinned Unicode version is a data update too.
- Gold-corpus sources (Wikidata, BAN, PDOK/BAG, US Census, ONSPD via postcodes.io) are attributed in `NOTICE`; update it when adding a source. Do not add Google Maps or other restricted geocoder output to any corpus.
- Regenerate `THIRD_PARTY_NOTICES.txt` with `cargo run --bin notices` after dependency changes. Do not edit generated data or notices by hand.
- Models are external, large and not bundled in the runtime image. Do not add automatic startup downloads or commit model files. The container expects a read-only model mount at `/data/libpostal`.
- Keep the runtime Python-free and non-root (uid/gid 1000), with a writable region-map directory. Do not conflate a parser/model upgrade with unrelated service changes.

## Git and publishing

- Use feature branches and pull requests; never push directly to `main` or continue a branch whose PR has merged.
- Keep public PR descriptions accurate after every push and free of internal references.
- Pin GitHub Actions and reusable workflows by commit SHA. Keep publishing credentials out of pull-request jobs.
- Pushes to `main`, `v*` tag pushes and manual dispatch trigger checked, multi-platform image publishing to `ghcr.io/mokto/geolocation`. Only `main` updates `latest`; branch, SHA and version tags are separate.
- Do not merge, create release tags, publish crates or change package visibility without explicit authorization. GHCR publication already configured by this repository runs automatically on pushes to `main` and `v*` tags.
