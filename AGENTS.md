# Agent instructions

## Scope and approach

This is a standalone public Rust geolocation service. Read `README.md` before changing behavior or deployment assumptions.

- Prefer the standard library and existing dependencies. Keep changes small; avoid speculative abstractions and unrelated cleanup.
- Comments explain hidden constraints, not what the code does.
- Never include credentials, private endpoints, internal tickets, proprietary overrides or private checkout dependencies in source, documentation, commits or pull requests.
- Do not claim production accuracy, throughput or interoperability from compilation or unit tests alone. Keep known limitations visible.

## Repository map

- `src/matching.rs`: normalization, country/region corroboration and candidate selection.
- `src/resolver.rs`: parsing, index lookups, candidate formatting and fallback ordering.
- `src/index/`: the in-process lookup over a `build_index` output (`mod.rs`), the shared libpostal `Parser`, and the analyzer, schema and house-number format shared with the builder (`canon.rs`, `schema.rs`).
- `src/index/locality.rs`: the startup-built grid of locality points behind `Index::reverse` (not part of the index files, so it does not change the builder hash).
- `src/index/reverse.rs`: `reverse.bin`, the street-by-grid-cell file `ReverseGeocode` reads (the builder writes it after the final segment merge, from per-shard spills of `schema.rs`'s number pass); houses themselves stay in the number FSTs. It also holds `streets.bin`, the streets' simplified lines by grid cell, which `ReverseGeocode` reads when no house is in range (spilled by the segment pass, written next to `reverse.bin`).
- `src/index/area.rs`: `areas.bin`, the simplified county, region, dependency and country polygons `ResolveRegion` answers from (and `ReverseGeocode`'s locality fallback keeps to); the builder writes it from Overture's territorial `division_area` rows. Points no polygon with a region code holds fall back to the nearest-point rule in `Index::reverse_region`.
- `src/index/suggest.rs`: `SuggestAddress` completion of a half-typed place, street, house or postcode, with one-edit typos, later words of a big place's name and an optional `focus`; reads the term dictionary and a name FST built at startup, never libpostal.
- `src/regions.rs`: the regions and region map derived from the index at startup.
- `src/service.rs`, `src/main.rs`, `src/metrics.rs`: RPCs, process lifecycle and observability.
- `proto/opengeocoding-v1.proto`, `build.rs`: the single wire contract and build-time generation.
- `data/`, `src/bin/update_reference.rs`: public reference data and its generator.
- `tests/matching.rs`, `tests/rpc.rs`: regression tests and the model-backed RPC integration test (ignored by default).
- `src/bin/benchmark.rs`, `src/bin/notices.rs`: SearchAddress benchmark across running services and third-party notice generation.
- `src/bin/build_index/`: experimental offline Overture-to-Tantivy index builder (polygon assignment, street merging, house-number FSTs; `sources.rs` reads OSM and OpenAddresses), plus `job`/`pull`/`hash` subcommands (`job.rs`): fetching and mirroring sources (`fetch.rs`) and publishing and pulling sealed directories in any S3-compatible store (`store.rs`). The service loads its output read-only from `INDEX_DIR`.
- `manifests/`: the Helm chart (the leader Deployment and its ConfigMap, which holds the rendered build Job and serving Deployment with a `pull` init container); `tests/chart.rs` checks that it pins this code's builder hash.
- `src/bin/leader/`: the in-cluster controller that creates the build Job when the store lacks the index and applies the serving Deployment once it is sealed; stateless, with a Lease for election. It includes `build_index/store.rs` by path and needs the bucket to exist.
- `src/bin/evaluate/`, `src/eval.rs`: geographic-correctness evaluator (scoring of SearchAddress and SuggestAddress, runs, reports, snapshots, gold builder) and the TSV/hold-out helpers it shares with external samplers.
- `tests/synthetic/`: a made-up world (`world.rs`) written as Overture parquet and built by the real `build_index`; `main.rs` checks suggestions, reverse lookups and, with the libpostal models (`-- --ignored`, run in CI), SearchAddress against facts authored in `world.rs`, never this service's answers. Add a case there for logic that needs no real data; a case that fails today carries a `known_failure` label and must keep failing until it is fixed.
- `tests/suggest.rs`, `tests/suggest.gold.tsv`: `SuggestAddress` golden cases (public facts, never this service's answers) and a typed-prefix latency run; both need `INDEX_DIR` and are ignored by default.
- `tests/gold.seeds.jsonl`, `tests/gold.jsonl`: curated gold case list and the corpus generated from it.
- `tests/registry/`, `src/bin/evaluate/nad.rs`: registry-first corpora (`*.seeds.jsonl` drawn from a registry release, `*.labels.jsonl` triage labels, the generated `*.jsonl`) and the US National Address Database sampler. They are part of the gold suite; regenerate with `evaluate sample-us` and `build-gold --nad`, never hand-edit.
- `benches/`: CodSpeed benchmark suites (divan) for the index analyzer, candidate matching and libpostal parsing, each run on a pull request only when its files change; `.github/workflows/codspeed.yml` runs them. Search, suggest and reverse speed is `evaluate`'s speed section (`gold-speed`), not a bench.
- `tests/addresses.tsv`, `tests/snapshots/addresses.tsv`: unscored real-address corpus and its accepted snapshot for change detection.
- `Dockerfile`, `.github/workflows/`: runtime packaging, CI and GHCR publishing.

## Compatibility and safety

- The protobuf package is `opengeocoding`; the service is `OpenGeocoding`. The Rust crate and service executable are `opengeocoding`. Messages, field numbers and types of `SearchAddress` and `ResolveRegion` are unchanged from the earlier `geolocation.Geolocation` and `nominatim_geolocation.NominatimGeolocation` services (`RefreshRegionMap` and `GetRegionMap` were removed, `ListRegions` added), whose paths `src/main.rs` rewrites to the canonical ones. Preserve the contract and both legacy RPC paths for benchmarking. Modify `proto/` and regenerate through Cargo; never edit generated files under `target/`.
- Preserve deliberate fallback ordering, country/region conflict guards, postcode-area compatibility, local-language candidate corroboration and `good_match` semantics (false for structured fallbacks and for a missing requested house number). Other-language names (`namedetails`) corroborate settlements, and regions only for bare place names; they never justify overriding a recognized stated country.
- Coordinates use protobuf float32, which is deliberate.
- Preserve both legacy RPC routes for benchmarking, but do not restore Nominatim backend configuration or fallback. One process parses, looks up and serves; do not reintroduce an internal HTTP hop or a separate index server. The service reads indexes and models from read-only paths and never downloads anything. Only the chart's init container downloads them, from the installer's own store, sealed and checked against `MANIFEST`; never from upstream at startup.
- Validate input at trust boundaries. Do not expose backend details in RPC errors.
- The service has no database: regions come from the index, fixed for the life of the process, and served by `ListRegions`. Do not reintroduce a database or any write at runtime. Region rows must stay what a search for `<cc>, <ISO 3166-2 code>` returns; check region changes with the evaluator.
- libpostal is native C behind `rustpostal`, not a pure-Rust parser. The resolver and the index share one `index::Parser`; keep parsing and index lookups off the async executor and libpostal calls serialized until native thread-safety testing supports widening concurrency. Set up libpostal once per process.
- The RPCs currently have no authentication. Do not present them as safe to expose publicly without an authenticated proxy or trusted network.

## Builds and checks

Host builds require Rust 1.99+, Buf and compiled libpostal. Nonstandard library paths may require `RUSTFLAGS="-L native=/path/to/lib"`.

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
docker build --target checks .
docker build --target runtime -t opengeocoding .
```

- Leave a regression check for nontrivial logic. Run the relevant checks and report exactly what ran, including failures and unavailable prerequisites.
- Do not skip tests, weaken assertions or add retries to disguise failures. Revert ineffective fixes before trying another approach.
- Run `cargo test --test rpc -- --ignored --nocapture` with an index (`INDEX_DIR`) and provisioned models for parsing, RPC or lifecycle changes. Install `grpcurl` to exercise its gRPC checks; its known failure is documented in the README.
- Evaluator and corpus checks run in `cargo test` (also `target/release/evaluate check`). After changing scoring or expectations, re-score saved runs with `evaluate report RUN_DIR --rescore` and explain every changed class. For changes to parsing, matching, fallbacks, reference data or the backend, also run the smoke or gold suite on an agreed index (`--index`) and compare snapshots with `--compare`; see the README's Geographic evaluation section.
- Gate a matching, fallback or parsing change on the full index, not only on a hunch from sample queries. Run the gold suite for the base commit and the change on the same local full index (`~/opengeocoding-index/out`), diff them with `evaluate diff BASE CANDIDATE`, and read every section: **Newly failing** and **Worse failure class** both fail the gate, and an abstain that becomes a near_miss or far_miss is a regression (a wrong answer is worse than none), however plausible it looks. CI repeats the comparison on the full index and comments it, with the speed comparison, on the pull request: after each push, read both comments and fix every listed case before calling the change done; they can list cases your local run did not.
- Debugging a case the CI gold comment lists: check the pull request's state first (a merged branch is not continued; branch from `main`). Re-run just that case on your own index with `evaluate run --suite gold --index DIR --only REGEX --repeat 1 --out DIR`, once with the base's and once with the head's builder-hash index, and compare the answer's `goodMatch`, level and postcode as well as its coordinates. Each run's untrimmed report is the artifact `gold-<head>-<run>-<attempt>` (`gh run download RUN_ID -n NAME`); it has no per-case responses, because the cluster endpoint returns only the report. A case that fails in CI but passes locally on an index from the same builder points to different source snapshots, not to the code: say so in the pull request, and do not change matching or add a `known_failure` to turn the gate green. A case whose failure matches its recorded `known_failure` class and was only "new" because the base scored differently is base noise; verify that locally and say so.
- Push only once the local diff is clean, so each push costs one CI cycle, not several. Do not poll CI in a loop of long sleeps: check it a few times, then report its state.
- A change that alters the builder hash needs a new index: rebuild locally with `~/opengeocoding-index/build.sh`, run the gold diff on it and update `builder.hash.txt`, rather than waiting for CI to build it.
- Preserve amd64 and arm64 support. CI unit checks do not download models or establish model-backed accuracy.
- Performance claims require comparable measurements and accuracy checks; synthetic CPU timings are not request-latency guarantees.

## Local test index

A world index built by this repository's builder is kept in `~/opengeocoding-index/out`, with `builder.hash.txt` holding the `build_index hash` it was built with and `build.sh` the exact build. Look there before building anything: if `build_index hash` equals `builder.hash.txt`, use `INDEX_DIR=~/opengeocoding-index/out` as it is. Rebuild (`~/opengeocoding-index/build.sh`, about 6 minutes and 20 GB of memory) only when the hash differs, and update `builder.hash.txt`. The service and `tests/suggest.rs` need no libpostal models beyond what the service itself loads.

## Evaluation corpora

- Expected answers in `tests/gold.jsonl` come only from licensed authoritative sources via `evaluate build-gold`. Curate cases in `tests/gold.seeds.jsonl`, regenerate, and review the corpus diff as a data update. Never derive or regenerate expectations from Nominatim, this service or another geocoder's responses, and never hand-edit `tests/gold.jsonl`.
- Nonempty responses, `goodMatch=true`, agreement with another implementation and stored production coordinates are not ground truth.
- Do not weaken expectations, drop cases or skip known failures to make a gate pass. `known_failure` labels record triaged, attributed baseline failures (backend, parser, matching or reference data). Remove one only after verifying a fix. Triage every new critical result before labelling it.
- Keep the held-out subset (stable location hash) out of tuning. Variants of one place share a `location` key and are not independent samples.
- `tests/addresses.tsv` may contain only the columns in `opengeocoding::eval::ADDRESS_COLUMNS` (query strings and their geography; `cargo test` enforces this). Never add source or provider names, coordinates, company names, domains, people or profile identifiers, or imprint and other likely home addresses. It is regenerated by a sampler kept outside this repository; do not hand-edit it or add the sampler, its internal queries or raw pools here.
- Address cases measure change, not accuracy. Keep snapshots as one TSV row per case with the manifest in a separate file, so a PR diff shows only changed answers. Replace `tests/snapshots/addresses.tsv` only after reviewing the diff, and say in the PR which changes were accepted and why. Do not regenerate the snapshot to silence drift.
- Other corpora with production metadata (earlier geocodes, source labels) and their runs stay private and out of this repository.
- Use the same consumer binary, models and reference data on both sides of a comparison. Confirm a zero-drift rerun before trusting a diff. Treat a code/configuration change and a data/import change as separate experiments.
- Do not add evaluator retries. Load testing needs an approved isolated backend.
- The grpcurl SearchAddress hang remains a known failure even if a run reports it passing; it reproduces on a fresh service.

## Licensing, data and models

- Project source is PolyForm Shield 1.0.0 (source-available, not OSI open source); earlier published versions also remain under FSL-1.1-ALv2. Keep dependencies compatible with distribution under it and retain their required notices.
- Transliteration uses BSD-3-Clause `deunicode`. Do not copy or export GPL Unidecode tables to restore old outputs. Transliteration changes require multilingual accuracy checks.
- CLDR and GeoNames data retain their separate licenses and attribution. Refresh `data/` through `cargo run --bin update_reference`; review dataset changes and provenance together. `iso3166_2` in `data/reference.json` is a curated list of ISO 3166-2 codes Overture's `region` column lacks; the generator refreshes only their Wikidata items, so add a code there (empty list) before expecting it. Changing the pinned Unicode version is a data update too.
- Gold-corpus sources (Wikidata, BAN, PDOK/BAG, US Census, ONSPD via postcodes.io) are attributed in `NOTICE`; update it when adding a source. Do not add Google Maps or other restricted geocoder output to any corpus.
- Regenerate `THIRD_PARTY_NOTICES.txt` with `cargo run --bin notices` after dependency changes. Do not edit generated data or notices by hand.
- Models are external, large and not bundled in the runtime image. Do not add automatic startup downloads from upstream or commit model files. The container expects read-only models at `/data/libpostal`; the build Job mirrors them into the store with libpostal's own `libpostal_data`.
- Any change under `src/bin/build_index/` other than `fetch.rs` and `store.rs`, to `src/index/{schema,canon,reverse,area}.rs` or to the tantivy or deunicode versions changes the builder hash: update `index.builderHash` in `manifests/values.yaml` (`build_index hash`) in the same commit. That names a new index, which deployed charts rebuild. Store layout and `MANIFEST`/`_COMPLETE` sealing are a compatibility surface for existing buckets.
- Keep the runtime Python-free and non-root (uid/gid 1000). Do not conflate a parser/model upgrade with unrelated service changes.

## Git and publishing

- Use feature branches and pull requests; never push directly to `main` or continue a branch whose PR has merged.
- Keep public PR descriptions accurate after every push and free of internal references.
- Pin GitHub Actions and reusable workflows by commit SHA. Keep publishing credentials out of pull-request jobs.
- Only `v*` tag pushes trigger checked, multi-platform image publishing to `ghcr.io/mokto/opengeocoding`; the tag also updates `latest`. release-please creates the tags when its Release PR is merged; commit titles must be conventional (`feat:`, `fix:`, `perf:`, `!`) to cause a release. The Helm chart is published to `oci://ghcr.io/mokto/charts` after the image, pinned to it.
- Do not merge, create release tags, publish crates or change package visibility without explicit authorization. GHCR publication already configured by this repository runs automatically on `v*` tags.
