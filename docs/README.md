# Documentation

[Project overview](../README.md) · [Hosted API documentation](https://opengeocoding.com/docs)

These documents cover the self-hosted engine and contributor tooling. Run commands from the repository root unless a guide explicitly changes directory.

## Use and operate the engine

| Task | Guide |
| --- | --- |
| Call the self-hosted RPCs and understand matching behavior | [API guide](guides/api.md) |
| Run Docker or Kubernetes, provision models and configure the service | [Deployment guide](guides/deployment.md) |
| Choose geographic inputs and build a compatible index | [Index-building guide](guides/indexing.md) |
| Build, test, refresh reference data and publish releases | [Development guide](guides/development.md) |

The [Protobuf contract](../proto/opengeocoding-v1.proto) is the wire-format source of truth. The engine has no authentication; keep it on a trusted network or behind an authenticated proxy.

## Evaluate correctness and coverage

- [Evaluation workflow](evaluation/README.md): corpora, authoritative expectations, held-out cases, snapshots and regression gates.
- [Registry sources](evaluation/registry/): country-specific preparation, provenance and limitations.
- [House-number coverage](evaluation/coverage.md): historical counts, source review and the supporting [measurement table](evaluation/house-number-coverage.tsv).
- [Provider comparisons](evaluation/providers.md): offline replay, request budgets and storage restrictions; aggregate results live in the [scorecard](../SCORECARD.md).
- [Adjudications](evaluation/adjudications/) and [triage records](evaluation/triage/): evidence for individual case decisions and unresolved failures.

Registry preparation helpers live beside the corpora in [tests/registry/](../tests/registry/). They are offline curation tools, not runtime dependencies. Agreement with an index input is not independent accuracy evidence.

## Measure performance

[Performance methodology and benchmarks](performance/README.md) separates historical index-build measurements, deployed request measurements, Nominatim planning context and the local benchmark procedure. [Aggregate benchmark records](performance/benchmarks/) preserve the supporting evidence and scope; they are not capacity guarantees.

## Understand implementation investigations

[Implementation notes](notes/README.md) preserve diagnoses, bounded recovery rules and optimization history. They describe particular changes and validation runs; use the guides for current setup instructions, not these historical records.
