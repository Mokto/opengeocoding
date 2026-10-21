# Development guide

[Documentation](../README.md) · [API](api.md) · [Evaluation](../evaluation/README.md)

Run commands from the repository root. For measurements, comparisons and load testing, see [Performance and benchmarks](../performance/README.md).

[Build and test](#build-and-test) · [Publishing](#ghcr-publishing) · [Reference data](#reference-data-and-normalization)

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

CI builds and checks both amd64 and arm64 without downloading models. [CodSpeed](https://codspeed.io/) runs the `benches/` suites in its simulation mode and reports changes in instruction counts: `index` (the index analyzer and house-number keys; `src/index/{schema,canon}.rs`), `matching` (the service's candidate matching; `src/matching.rs`, `src/resolver.rs`, `src/index/country/`, `data/`) and `resolve` (model-backed libpostal parsing of `tests/addresses.tsv`; `src/index/parse.rs`, where the parser lives, and the country rules in `src/index/country/`). A pull request runs only the suites whose files it changes (or all of them when it changes `Cargo.toml`, `Cargo.lock`, `build.rs` or the workflow); pushes to `main` run all of them, so each `main` commit is a complete baseline. Only `resolve` loads models; none loads an index. Run them locally with `cargo bench` (`resolve` needs `LIBPOSTAL_MODELS` set), or `cargo codspeed build && cargo codspeed run`. Index searches, suggestions and reverse lookups are timed end to end by the [full-index gold check](../evaluation/README.md). Regenerate `THIRD_PARTY_NOTICES.txt` with `cargo run --locked --bin notices` after dependency changes. Model-backed RPC tests run separately with provisioned models.

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

Historical memberships from official datasets (`data/historical_memberships.tsv`, provenance in `data/historical_sources.json`) are regenerated with `cargo run --locked --bin update_history -- CACHE_DIR`; see [Historical memberships](../notes/historical-memberships.md#imported-datasets-216) for the inputs, the pinning and how to import a new release.

An optional build-time `REFERENCE_DATA` path can supply a custom reference JSON. Search overrides are empty by default. No private deployments, endpoints or address overrides are included.

Transliteration uses BSD-3-Clause `deunicode`, **not Python Unidecode**. Outputs differ for some scripts and symbols. In a migration comparison of 7,067 geographic/sample strings, 55 normalized outputs differed, including Hebrew and Japanese forms. Matching normalizes both sides consistently, but these differences still need real-address accuracy validation before replacing another service.
