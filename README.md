# OpenGeocoding

**Kubernetes-native geocoding: address search, autocomplete and reverse geocoding — on your infrastructure or through a hosted API.**

[![Release](https://img.shields.io/github/v/release/Mokto/opengeocoding)](https://github.com/Mokto/opengeocoding/releases) [![License: PolyForm Shield](https://img.shields.io/badge/license-PolyForm_Shield-blue)](LICENSE) [![CodSpeed](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://app.codspeed.io/Mokto/opengeocoding?utm_source=badge)

Turn free-text addresses into coordinates, help users find an address as they type, and resolve coordinates to nearby addresses or administrative regions.

Built in Rust, OpenGeocoding combines [libpostal](https://github.com/openvenues/libpostal) parsing with a read-only index built from [Overture Maps](https://overturemaps.org/), with optional OpenStreetMap, OpenAddresses and GeoNames inputs. The engine serves lookups in-process: no database, external geocoding backend or runtime data downloads.

**[Try the live demo](https://opengeocoding.com/) · [Hosted API docs](https://opengeocoding.com/docs) · [Self-host](#self-host) · [Releases](https://github.com/Mokto/opengeocoding/releases)**

> Source-available under [PolyForm Shield 1.0.0](#license), not OSI open source. The engine and index remain experimental; evaluate coverage and accuracy for your use case before production.

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

**A world index built in under 20 minutes from already-downloaded sources; forward search reached 2,850 queries/s in one serving container.** These are separate historical measurements, not one combined current-release benchmark:

| Workload | Observed result |
| --- | --- |
| World index build, including landmarks | **Under 20 min, excluding downloads**, **32.2 GB peak memory**, on an 18-core Apple M5 Max with 64 GB RAM |
| Built world index | About **16 GB on disk**, excluding libpostal models and build scratch |
| Forward search, 16 connections to one serving instance | **2,850 queries/s per instance**, not a multi-pod total; p95 was not recorded in the published summary |
| Serving process in the search benchmark | **4.2 GiB resident memory** |

The build used already-downloaded sources; downloads and model provisioning are additional. Search numbers are end-to-end Connect JSON requests over keep-alive connections to a **single serving container**, using 808 address samples, an earlier 11 GB world index and `PARSE_CACHE=0`. **Search benchmark CPU:** a busy 18-core laptop; the CPU model and container vCPU allocation were not recorded. This is not a Kubernetes per-pod capacity measurement at a specified CPU budget, nor a hosted API limit. See [measurement scope and methodology](docs/development.md#performance-methodology-and-nominatim-comparison).

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

Follow the [Docker setup guide](docs/deployment.md#run-with-docker) to obtain the image, provision models and build an index. With models at `models/libpostal/` and that image's index at `out/`, start the service:

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

The [Helm deployment guide](docs/deployment.md#deploy-on-kubernetes) covers building and publishing indexes into an existing S3-compatible bucket, verified downloads into serving pods, and rolling upgrades. Each serving pod runs one geocoder instance with its own read-only index. Regional builds are supported.

The [chart defaults](manifests/values.yaml) request **1 CPU and 6 GiB RAM per serving pod**, with a **16 GiB memory limit and no CPU limit**. A CPU request is not a cap; these defaults are not the recorded benchmark allocation and do not establish 2,850 queries/s on one CPU. A world build requires substantial memory, scratch disk and initial downloads; review the guide before provisioning.

## Geographic evaluation

Coverage depends on the source data and the index you build. Worldwide place coverage does **not** mean complete house-level coverage: substantial gaps remain in parts of Africa and Asia, and coverage varies elsewhere too. Regional indexes should not be treated as a safe geographic filter; out-of-area queries can still match an indexed namesake.

The repository includes authoritative-source evaluation corpora, held-out cases, regression gates and tools for comparing saved runs. These are diagnostic checks, not a production accuracy percentage. Some evaluation registries also supply index data, so agreement is not independent accuracy evidence.

- [Evaluation methodology and regression gates](docs/evaluation.md)
- [Country-specific sources and limitations](docs/registry/)
- [House-number coverage measurements](docs/indexing.md#house-number-coverage)
- [Provider comparison scorecard](SCORECARD.md) — provisional results with methodology and limitations

Connect JSON and Connect Protobuf are the primary interfaces. **Direct gRPC interoperability is not signed off:** the documented `grpcurl` search timeout remains a [known issue](docs/api.md#known-interoperability-issue). Autocomplete IDs are valid only for the same index snapshot. See the [API guide](docs/api.md) for matching semantics and operation-specific limits.

## Documentation

| Guide | Contents |
| --- | --- |
| [API](docs/api.md) | Wire contract, request examples, autocomplete, landmarks, reverse geocoding and compatibility |
| [Deployment](docs/deployment.md) | Docker, Helm, model/index provisioning, configuration and diagnostics |
| [Index building](docs/indexing.md) | Data inputs, licensing, coverage and historical build/performance measurements |
| [Development](docs/development.md) | Build requirements, tests, performance methodology, Nominatim context, benchmarks and publishing |
| [Evaluation](docs/evaluation.md) | Corpora, scoring, held-out cases and regression gates |
| [Provider evaluation](docs/providers.md) | Offline comparisons, request budgets and provider storage restrictions |

The self-hosted wire contract is [`proto/opengeocoding-v1.proto`](proto/opengeocoding-v1.proto). Found a bug? [Open an issue](https://github.com/Mokto/opengeocoding/issues), without personal addresses, credentials or private endpoints.

## License

Project source is licensed under **[PolyForm Shield 1.0.0](LICENSE)**. It permits use, modification and redistribution, including commercial use, except providing a product that competes with this software or a product the licensor provides using it. A free product can compete too. Read the license before adopting or offering a service built on it.

This is **source-available, not OSI open source**, with no later conversion to an open-source license. Versions published before the switch retain FSL-1.1-ALv2, including its Apache-2.0 grant two years after each publication.

Data, models and third-party software have separate terms. Overture's divisions and transportation data and OpenStreetMap carry ODbL obligations; other input licenses vary. Review the [index guide](docs/indexing.md), [NOTICE](NOTICE), [third-party notices](THIRD_PARTY_NOTICES.txt) and [license texts](LICENSES/) before distributing an index or deployment.
