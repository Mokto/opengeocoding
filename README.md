# OpenGeocoding

**Kubernetes-native geocoding: address search, autocomplete and reverse geocoding — on your infrastructure or through a hosted API.**

[![Release](https://img.shields.io/github/v/release/Mokto/opengeocoding)](https://github.com/Mokto/opengeocoding/releases) [![License: PolyForm Shield](https://img.shields.io/badge/license-PolyForm_Shield-blue)](LICENSE) [![CodSpeed](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://app.codspeed.io/Mokto/opengeocoding?utm_source=badge)

Turn free-text addresses into coordinates, help users find an address as they type, and resolve coordinates to nearby addresses or administrative regions.

Built in Rust, OpenGeocoding combines [libpostal](https://github.com/openvenues/libpostal) parsing with a read-only index built from [Overture Maps](https://overturemaps.org/), with optional OpenStreetMap, OpenAddresses and GeoNames inputs. The engine serves lookups in-process: no database, external geocoding backend or runtime data downloads.

**[Try the live demo](https://opengeocoding.com/) · [Hosted API docs](https://opengeocoding.com/docs) · [Self-host](#self-host) · [Releases](https://github.com/Mokto/opengeocoding/releases)**

> Source-available under [PolyForm Shield 1.0.0](#license), not OSI open source. Releases follow semantic versioning: a breaking change to the RPC contract (including the legacy routes), the Helm chart's values or the index store layout is a major release. Coverage and accuracy vary by country; check the [scorecard](SCORECARD.md) and [coverage limitations](docs/evaluation/coverage.md#house-number-coverage) for yours before production.

## What you can build

| Capability | Use it for |
| --- | --- |
| **Address search** | Turn an address or place name into coordinates and structured address fields |
| **Autocomplete** | Complete places, streets, houses and postcodes, with country filters and location-biased place ranking |
| **Selection retrieval** | Resolve an autocomplete selection by its opaque ID, without another text search |
| **Reverse geocoding** | Find nearby houses, with street and locality fallbacks |
| **Region resolution** | Find administrative regions and ISO 3166-2 codes for a point |
| **Region directory** | List the regions available in your index |

Autocomplete can also include selected landmark categories: airports, stations, attractions, museums, universities, hospitals and stadiums. It is not a general business or POI directory.

## Why OpenGeocoding

- **Keep lookups on your infrastructure.** Self-host the engine and its data; address resolution does not call a third-party geocoder.
- **Operate one serving process.** Parsing, search and reverse lookups share an in-process index. Models and index files are mounted read-only.
- **Search across languages and scripts.** Indexed local and alternative place names, Unicode normalization and street-abbreviation handling support multilingual inputs.
- **Distinguish matches from fallbacks.** Structured fields and `goodMatch` let clients identify weaker forward-geocoding answers; it is not a calibrated confidence score.
- **Kubernetes-native by design.** The Helm chart manages index-build Jobs and serving Deployments through a leader elected with a Kubernetes Lease, with verified index downloads and rolling updates. Linux amd64/arm64 containers, readiness checks, Prometheus metrics and graceful shutdown are included.

## Performance and resources

**A world index built in about 5 minutes from already-downloaded sources; deployed forward search measured 1,774–2,675 queries/s per pod with 16 concurrent connections.** The build was measured on 2026-10-28, search on release 0.14.0 on 2026-10-19:

| Workload | Observed result |
| --- | --- |
| World index build, including landmarks | **289 s, excluding downloads**, against 474 s for the previous builder on the same sources and machine; **12.1 GB peak memory** with its adaptive phases held to 12 GB, plus 5 GB of memory-mapped polygons, on an 18-core Apple M5 Max with 64 GB RAM |
| Built world index | About **16 GB on disk**, excluding libpostal models and build scratch |
| Forward search, 16 connections to one pod | **1,774–2,675 queries/s per pod**, **18.4–29.9 ms p95**, across three 20,000-request runs |
| Serving process in the search benchmark | About **6.3 GiB process RSS** |

**Search benchmark CPU:** Intel Core i5-13500 (14 cores/20 threads) on a shared node. The pod requested **1 CPU with no CPU limit** and used an observed **4.1–5.0 CPU cores averaged over each benchmark invocation**; this is not a one-CPU throughput claim.

All **60,000 measured requests completed without errors**. The client ran on another node and called one pod directly over Connect JSON/HTTP keep-alive, bypassing ingress and the public API edge. The workload repeated 1,000 address samples with the parse cache enabled and warmed. The third run was slower; the table includes all three runs, not only the fastest. These short runs are not a sustained-capacity or hosted API guarantee. Build downloads and model provisioning are additional. See the [results and methodology](docs/performance/README.md#performance-methodology-and-nominatim-comparison) and [aggregate measurement record](docs/performance/benchmarks/2026-10-19-search.json).

### Compared with Nominatim

[Nominatim 5.3's installation guide](https://nominatim.org/release-docs/5.3/admin/Installation/#hardware) strongly recommends **128 GB RAM or more for a planet import**, requires **at least 1 TB of disk for a planet installation**, and describes an import taking **around 2.5 days** on a well-configured machine (4–5 days on traditional SSDs). OpenGeocoding instead builds a compact read-only serving index and requires no PostgreSQL/PostGIS service.

**This is operational context, not a like-for-like speedup claim.** The input datasets, indexing work, coverage and hardware differ. Nominatim's import RAM recommendation is not its serving RAM usage. We have not published matched Nominatim queries/s, p95 latency or serving-memory measurements; those comparisons remain unmeasured.

## Try the hosted API

For the shortest path to a request, create an API key in the [dashboard](https://opengeocoding.com/dashboard) and set `OPENGEOCODING_API_KEY` in your environment:

```sh
curl --fail-with-body https://api.opengeocoding.com/v1/search \
  -H "x-api-key: ${OPENGEOCODING_API_KEY:?Set your API key first}" \
  -H 'Content-Type: application/json' \
  -d '{"addressString":"Copenhagen, Denmark"}'
```

The hosted service manages authentication, credits and rate limits. This repository contains the self-hosted engine, not the hosted account or billing platform. Hosted routes and available operations are documented in the [API reference](https://opengeocoding.com/docs); they differ from the direct RPC paths below.

## Self-host

```text
Your application → trusted network or authenticated proxy → OpenGeocoding
                                                           ├─ libpostal models
                                                           └─ read-only geographic index
```

You need **both libpostal models and a compatible built index**. Neither is included in the runtime image. Source downloads and index building happen before serving, not at service startup.

### Run with Docker

Follow the [Docker setup guide](docs/guides/deployment.md#run-with-docker) to obtain the image, provision models and build an index. With models at `models/libpostal/` and that image's index at `out/`, start the service:

```sh
docker run --rm \
  -p 127.0.0.1:11121:11121 -p 127.0.0.1:8000:8000 \
  -v "$PWD/models/libpostal:/data/libpostal:ro" \
  -v "$PWD/out:/data/index:ro" \
  -e INDEX_DIR=/data/index \
  opengeocoding
```

In another terminal, make a Connect JSON request (no SDK required):

```sh
curl --fail-with-body http://localhost:11121/opengeocoding.OpenGeocoding/SearchAddress \
  -H 'Content-Type: application/json' \
  -H 'Connect-Protocol-Version: 1' \
  -d '{"addressString":"Copenhagen, Denmark"}'
```

Use a query inside your index's coverage if you built a regional index. Readiness is at `http://localhost:11121/healthz`; metrics are at `http://localhost:8000/metrics`.

**The engine has no authentication.** The command binds ports to loopback. Keep it on a trusted network or behind an authenticated proxy; do not expose the RPC or metrics ports directly to the internet.

### Deploy on Kubernetes

The [Helm deployment guide](docs/guides/deployment.md#deploy-on-kubernetes) covers building and publishing indexes into an existing S3-compatible bucket, verified downloads into serving pods, and rolling upgrades. Each serving pod runs one geocoder instance with its own read-only index. Regional builds are supported.

The [chart defaults](manifests/values.yaml) request **1 CPU and 6 GiB RAM per serving pod**, with a **16 GiB memory limit and no CPU limit**. A CPU request is not a cap: the benchmark pod had these settings but consumed more than one CPU. The measured throughput is not a one-CPU guarantee. A world build requires substantial memory, scratch disk and initial downloads; review the guide before provisioning.

## Geographic evaluation

Coverage depends on the source data and the index you build. Worldwide place coverage does **not** mean complete house-level coverage: substantial gaps remain in parts of Africa and Asia, and coverage varies elsewhere too. Regional indexes should not be treated as a safe geographic filter; out-of-area queries can still match an indexed namesake.

The repository includes authoritative-source evaluation corpora, held-out cases, regression gates and tools for comparing saved runs. These are diagnostic checks, not a production accuracy percentage. Gold also includes a [48-market OSM address panel](docs/evaluation/global-addresses.md) for broader regression coverage. Some evaluation registries and OSM also supply index data, so agreement is not independent accuracy evidence.

- [Evaluation methodology and regression gates](docs/evaluation/README.md)
- [Country-specific sources and limitations](docs/evaluation/registry/)
- [House-number coverage measurements](docs/evaluation/coverage.md#house-number-coverage)
- [Provider comparison scorecard](SCORECARD.md) — provisional results with methodology and limitations

Connect JSON and Connect Protobuf are the primary interfaces. **Direct gRPC interoperability is not signed off:** the documented `grpcurl` search timeout remains a [known issue](docs/guides/api.md#known-interoperability-issue). Autocomplete IDs are valid only for the same index snapshot. See the [API guide](docs/guides/api.md) for matching semantics and operation-specific limits.

## Documentation

Start with the [documentation index](docs/README.md) for guides, evaluation workflows, performance evidence and implementation notes.

| Area | Contents |
| --- | --- |
| [API](docs/guides/api.md) | Wire contract, request examples, autocomplete, landmarks, reverse geocoding and compatibility |
| [Deployment](docs/guides/deployment.md) | Docker, Helm, model/index provisioning, configuration and diagnostics |
| [Index building](docs/guides/indexing.md) | Data inputs, licensing, index construction and limitations |
| [Development](docs/guides/development.md) | Build requirements, tests, reference data and publishing |
| [Performance](docs/performance/README.md) | Measurement scope, resource observations, benchmarks and Nominatim context |
| [Evaluation](docs/evaluation/README.md) | Corpora, scoring, held-out cases and regression gates |
| [Provider evaluation](docs/evaluation/providers.md) | Offline comparisons, request budgets and provider storage restrictions |

The self-hosted wire contract is [`proto/opengeocoding-v1.proto`](proto/opengeocoding-v1.proto). Found a bug? [Open an issue](https://github.com/Mokto/opengeocoding/issues), without personal addresses, credentials or private endpoints.

## License

Project source is licensed under **[PolyForm Shield 1.0.0](LICENSE)**. It permits use, modification and redistribution, including commercial use, except providing a product that competes with this software or a product the licensor provides using it. A free product can compete too. Read the license before adopting or offering a service built on it.

This is **source-available, not OSI open source**, with no later conversion to an open-source license. Versions published before the switch retain FSL-1.1-ALv2, including its Apache-2.0 grant two years after each publication.

Data, models and third-party software have separate terms. Overture's divisions and transportation data and OpenStreetMap carry ODbL obligations; other input licenses vary. Review the [index guide](docs/guides/indexing.md), [NOTICE](NOTICE), [third-party notices](THIRD_PARTY_NOTICES.txt) and [license texts](LICENSES/) before distributing an index or deployment.
