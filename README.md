# OpenGeocoding

[![CodSpeed](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://app.codspeed.io/Mokto/opengeocoding?utm_source=badge)

A Rust service that combines libpostal address parsing, Overture-backed index lookups and the administrative regions of that index.

- Connect JSON and Connect protobuf; an experimental legacy gRPC surface.
- Forward geocoding, reverse geocoding to the nearest houses, reverse region resolution and a list of the regions the index knows.
- Country/region corroboration, local-language candidate matching (including each place's names in other languages and scripts), postcode-area safeguards and ordered fallback queries.
- `goodMatch` is false for structured-fallback results, for street results that lack the requested house number, for a P.O. box (the town is answered), and for a place that explains less than half of a long query (a "weak" guess: "Twin City Shopping Centre, Burgersfort" is not Thuin). When the whole query is such a guess or the parser garbled it, each comma-separated part is searched on its own and a settlement or postcode that accounts for at least three of the query's words (or more than the guess did) answers, with `goodMatch` false unless it accounts for every word.
- Postcodes are returned as the country writes them (`1012 LG`, `H9R 5Z5`, `HA5 5NE`, `140 00`) where the shape is known, and a territory of the US, France, Norway, the Netherlands or China answers under its sovereign's country code, as the Nominatim deployment did (Greenland, the Faroes and the British territories keep their own).
- Prometheus metrics, gRPC reflection and graceful shutdown.
- A non-root, Python-free runtime for Linux amd64 and arm64.

This is an initial implementation, not a production accuracy or throughput guarantee. libpostal remains a native C parser, not a pure-Rust replacement. Calls are serialized off the async executor pending native concurrency validation.

## Run with Docker

Build the image locally:

```sh
docker build --target runtime -t opengeocoding .
```

Or pull a published image:

```sh
docker pull ghcr.io/mokto/opengeocoding:latest
docker tag ghcr.io/mokto/opengeocoding:latest opengeocoding
```

`latest` is the newest release. Models and indexes are not included in the image.

Download libpostal models explicitly into a writable directory. Downloads are large; nothing downloads automatically at import or startup:

```sh
mkdir -p models
docker run --rm --entrypoint libpostal_data \
  -v "$PWD/models:/data" opengeocoding download all /data
```

The resulting directory is `models/libpostal`. Keep it for subsequent runs.

Build an index into `out/` (see the index section below), and mount it read-only:

```sh
docker run --rm \
  -p 11121:11121 -p 8000:8000 \
  -v "$PWD/models/libpostal:/data/libpostal:ro" \
  -v "$PWD/out:/data/index:ro" \
  -e INDEX_DIR=/data/index \
  opengeocoding
```

Addresses inside Docker must be reachable from the container; `localhost` refers to that container. The service needs nothing else at runtime: it derives its administrative regions from the index at startup.

**Deploy behind a trusted network or authenticated proxy.** The RPCs, including region-map refresh, do not implement authentication. Do not expose them directly to the public internet.

Example request:

```sh
curl http://localhost:11121/opengeocoding.Geolocation/SearchAddress \
  -H 'Content-Type: application/json' \
  -H 'Connect-Protocol-Version: 1' \
  -d '{"addressString":"Copenhagen, Denmark"}'
```

The contract is `proto/opengeocoding-v1.proto`, package `opengeocoding`, service `Geolocation`. The methods are `SearchAddress`, `ResolveRegion`, `ListRegions`, `SuggestAddress` and `ReverseGeocode`. `RefreshRegionMap` and `GetRegionMap` (a Python-style dictionary string) were removed; `ListRegions` returns the same regions as typed messages.

The canonical RPC path is `/opengeocoding.Geolocation/*`. The Go package is `github.com/Mokto/opengeocoding/gen/opengeocoding`; the Rust crate is `opengeocoding`, with generated types under `opengeocoding::proto::opengeocoding`.

`/geolocation.Geolocation/*` and `/nominatim_geolocation.NominatimGeolocation/*` remain aliases for every method, including for benchmarking. Message fields, numbers and types are unchanged. These aliases do not enable a Nominatim backend. gRPC reflection advertises only `opengeocoding.Geolocation`, and the `grpc_service` metric label uses this canonical name on all routes.

## Address suggestions

`SuggestAddress` completes text as it is typed and returns up to `limit` (default 5, at most 15) suggestions, each with an opaque `id` (stable for one index), a `display_name`, a point (places also a bounding box when they have an extent), a `type` and a `country_code`. `country_code` restricts the answer to one country. There is no second lookup call: the point is in the suggestion.

```sh
curl http://localhost:11121/opengeocoding.Geolocation/SuggestAddress \
  -H 'Content-Type: application/json' -H 'Connect-Protocol-Version: 1' \
  -d '{"text":"10 downing street, lon","limit":3}'
```

It does not call libpostal, which misreads half-typed words. The commas say where the road ends and the town starts (`12 rue de rivoli, par`), and without commas the end of the text may still name the town (`main st spri`). The last word may be unfinished unless the text ends in a space or comma. Places come from an in-memory FST of the folded names, in every language, of the places whose weight (population, or a guess by kind) is at least 5,000, built at startup (25 MB and about 0.6 s more on the world index, plus about 100 MB briefly while it is built); ranking is that weight times how much of the name was typed, so `londo` is London, not the hamlet named Londo, and `ma` lists countries. Smaller places, streets, house numbers (typed last, the numbers that start with them, up to four per street) and postcodes (three characters or more) come from the index's terms. A street search over words that many streets share is not run: without a town it would walk millions of postings to give an arbitrary street, so `main st` waits for a town, which can be a country or region code (`main st, il`). Place order and the weights are hand-set heuristics.

Not supported: typo tolerance, a bias towards the user's position, and completing a word in the middle of a name (only its start). One or two letters of a lone street word are not completed.

The gold suite checks suggestions too, with places ranked by population (see Geographic evaluation). `tests/suggest.rs` checks 25 golden cases (each place's country and coordinates are public facts, not this service's answers) and times every prefix of 29 typed strings against a world index (`INDEX_DIR=out cargo test --release --test suggest -- --ignored --nocapture`). On an 18-core laptop with a 10 GB world index, 380 typed prefixes took, after the service's warm-up (about 0.8 s on a thread of its own: the one- and two-letter prefixes and the names of countries and regions), p50 0.2 ms, p90 1.7 ms and p99 5 ms in the index on their first typing (max 10-17 ms, the first time a town's ancestors are read), and p50 0.12-0.14 ms, p90 1.3-1.4 ms and p99 4.4-4.6 ms (max 7-8 ms) when typed again. The two prefix caches hold at most 20,000 entries each, under about 150 MB. A request over Connect took 24 ms by `curl`, including the connection. These are one machine's numbers on public strings, not capacity or request-latency guarantees.

## Reverse geocoding

`ReverseGeocode` takes `lat`, `lon`, an optional `radius_m` (default 500, at most 2,000) and an optional `size` (default 1, at most 10). It returns the houses within the radius, nearest first, each a `FullAddress` with its `distance_m` and `layer` `address`. With no house in range it returns the nearest locality point (within about 50 km) as `layer` `locality`, and with nothing at all an empty list.

```sh
curl http://localhost:11121/opengeocoding.Geolocation/ReverseGeocode \
  -H 'Content-Type: application/json' -H 'Connect-Protocol-Version: 1' \
  -d '{"lat":51.50354,"lon":-0.12768,"size":3}'
```

The builder writes `reverse.bin` next to the number FSTs: one 10-byte record for each street and each grid cell (2^6 steps of the 24-bit packed coordinates: 76 m by 153 m at the equator) that holds one of its houses, sorted by cell, with the Morton code of the first such house and the address of the street's document. A lookup binary-searches the cells around the point, ring by ring, then reads the candidate streets' houses from the number FSTs and ranks them by distance. Nothing is built at startup, and the file is memory-mapped like the FSTs. On the 2026-09-23 Overture world index with OpenAddresses and OSM it holds 132.6 million records (1.33 GB, about 13% more than the 10 GB index; 497.5 million house rows) and adds about 4 seconds to the build. On an 18-core laptop with a freshly written index in the page cache, 729 random points in western and central Europe that had a house within 500 m took p50 67 µs, p90 108 µs and p99 169 µs in the index, one point at a time. These are one machine's warm-cache numbers; a cold file costs a few page reads per lookup.

Limits: only houses are found, so a street without house numbers in the data (including all 12.9 million segment-only streets) never answers, and there is no street-level result between a house and a locality. A street is found through its houses, not its geometry, so a point on a long street far from any numbered house gets the locality. The `locality` fallback (and `ResolveRegion`) takes the nearest of the 3.5 million `locality` points within 50 km, by distance, from a grid sorted at startup (`src/index/locality.rs`, 28 MB, well under a second to build), where it used to run a range query over the index's coordinate columns that took about 20 to 50 ms. Unlike that query, it does not prefer the most important places in a crowded box: it names the nearest, often a hamlet, and on a sample of 4,000 points it chose another place than before for 6% (0.7% a different region code, all near borders, where a nearest point is no better a guide than the old one). Cells across the antimeridian or poles are not searched. House numbers come back in their folded, lowercase form. Searching rings use the smaller cell side, so high latitudes read more cells than needed. Accuracy is not measured against an authoritative corpus; `tests/` only checks the lookup on a tiny index and, with an index, that the RPC answers.

## Deploy on Kubernetes

The Helm chart in `manifests/` builds the index in the cluster and serves it. It needs an S3-compatible bucket (AWS S3, MinIO, SeaweedFS, Ceph, R2 and the like):

```sh
kubectl create secret generic opengeocoding-s3 \
  --from-literal=AWS_ACCESS_KEY_ID=... --from-literal=AWS_SECRET_ACCESS_KEY=...
helm install geocoding oci://ghcr.io/mokto/charts/opengeocoding \
  --set store.url=s3://my-bucket/opengeocoding \
  --set store.endpoint=http://seaweedfs-s3.seaweedfs:8333 --set store.allowHttp=true \
  --set store.credentialsSecret=opengeocoding-s3
```

`store.endpoint` is only for stores other than AWS S3; any other `AWS_*` setting, such as a web identity role, goes in `store.extraEnv`. See `manifests/values.yaml` for every setting.

- **Build Job.** `build_index job` exits at once if the bucket already holds the index the chart names. Otherwise it fetches the sources and libpostal's models, builds, opens the result as the service would and resolves one of its places, then publishes it. Each source is copied into the bucket as it arrives, so a later build or a retried pod reads the bucket instead of upstream. A world build needs about 16 CPUs, 32 GiB of memory and 250 GiB of scratch (`build.*`), and about 140 GB of downloads the first time. Scratch is deleted with the pod unless `build.scratch.existingClaim` names a volume you keep: a later build then copies from the bucket only the files that do not already match it.
- **Serving pods.** An init container (`build_index pull`) waits until the index is published, downloads it and the models to the pod's own disk and checks every file's SHA-256. The service then opens them read-only. The service itself never downloads anything. Each pod needs about 6 GiB of memory (16 GiB keeps a world index's page cache), and 14 GB of disk for a world index.
- **Index names.** The chart names the index by `index.builderHash`, `index.modelsVersion` and `index.sources`, and passes the same name to the Job and the pods. `builderHash` hashes the builder's code (`build_index hash`), except the code that fetches sources and talks to the store (`fetch.rs`, `store.rs`): sources are sealed in the bucket and reused as they are, so that code can change without rebuilding. A fix to how sources are selected therefore takes effect only with a new `snapshot` label, or for Overture (stored by release) after deleting its directory in the bucket. `cargo test` fails until `manifests/values.yaml` matches the hash, so a builder change is a visible chart change. The Job refuses an image built from other builder code, and the service refuses an index whose `build.json` names another builder. `index.sources.snapshot` labels one fetch of the sources that only publish "latest" (OSM, OpenAddresses, GeoNames); a new label fetches them again.
- **Upgrades.** A change that names a new index starts a new Job. New pods wait for the new index before old ones stop (`maxSurge: 1`, `maxUnavailable: 0`), so the old index keeps serving during the build; a first install's rollout stays in progress until the build ends. Rolling back the values rolls back to the old index, which is still in the bucket. Nothing deletes old sources or indexes.
- **Regional builds.** `index.sources.bbox: [min lon, min lat, max lon, max lat]` builds only that area. Overture is then read only where its row groups meet the box (copied unchanged, without decoding), OpenAddresses layers are picked by their bounds, GeoNames by row, and the builder drops everything outside the box. Pick OSM extracts in `index.sources.osm` by hand. A regional index answers queries about other places with whatever it holds: "Paris, France" against a Liechtenstein index answered a Paris in Lombardy, with goodMatch true.

Bucket layout, under the URL's prefix: `sources/overture/<release>/<extent>/<type>/`, `sources/<snapshot>/{osm,openaddresses/<extent>,geonames/<extent>/<file>}/`, `models/libpostal/<version>/` and `indexes/<id>/`. Every directory is sealed by `MANIFEST` (paths, sizes, SHA-256) and then `_COMPLETE`; readers use nothing unsealed.

The same commands work without Kubernetes, against a bucket or a local directory: `build_index job --store file:///srv/geocoding --index NAME --builder-hash "$(build_index hash)" --sources sources.json --scratch /tmp/scratch --models /data/libpostal`, where `sources.json` holds `index.sources` as JSON, then `build_index pull --store file:///srv/geocoding --index NAME --to /srv/index --models /data/libpostal`.

Tested on a single-node kind cluster with SeaweedFS, with a Liechtenstein box and the `liechtenstein` OSM extract. The first build took 3 minutes, including libpostal's 2 GB of models from GitHub, and produced a 4.7 MB index. Two pods pulled it and answered house-level queries. Changing the sources built a new index from the bucket in under 20 seconds and rolled the pods one at a time. A client polling once a second through the rollout got 150 answers out of 150, after a 10-second `preStop` delay was added; before it, 3 of 240 requests failed. A world build has not yet been run through the Job; the world numbers above come from building by hand.

## Known interoperability issue

The initial model-backed integration run passes Connect JSON/protobuf, local-language corroboration, country/postcode rejection, reverse regions, map refresh, error handling and 64 concurrent requests. However, grpcurl 1.9.4 can list services but its SearchAddress invocation hangs, both with reflection and an explicit proto file. The integration script fails with a bounded timeout rather than skipping that failure. Legacy gRPC interoperability is not signed off; do not treat this initial push as a production-ready replacement.

## Configuration

| Variable | Default |
| --- | --- |
| `INDEX_DIR` | Required; a `build_index` output directory, read-only |
| `RPC_PORT` / `METRICS_PORT` | `11121` / `8000` |
| `PARSE_CACHE` | `65536`; address parses kept (0 parses every call). One resolve parses its query twice and libpostal's lock is shared, so repeats skip both |
| `RUST_LOG` | `info` |
| `SENTRY_DSN` / `ENV` | Optional |

The service loads the index into its own process at startup: one process parses, looks addresses up and serves the RPCs, with no internal HTTP hop. Startup fails if `INDEX_DIR` holds no `build_index` output. The index files stay memory-mapped; the place table (about 2 GB for the world) and libpostal's models (about 2 GB) are read into memory, which took about 10 seconds from a warm disk. `INDEX_URL`, `NOMINATIM_HOST`, `NOMINATIM_URL` and `NOMINATIM_AUTHORITY` are no longer supported; there is no Nominatim fallback. Legacy RPC paths and metric prefixes remain for compatibility, not backend selection.

Overture's `region` column carries an ISO 3166-2 code for its regions only. Subdivisions it does not code (Ugandan, Czech and Nauruan districts, Palestinian governorates, overseas territories such as `US-PR` and `FR-971`; 180 in all) are named in `data/reference.json` under `iso3166_2`, by the Wikidata item Overture gives the division. The list of codes is a policy kept in that file, one entry per subdivision that consumers expect; `update_reference` refreshes only their items. The builder writes each place's Wikidata item as the last `places.tsv` column and the service gives the matching place that code: it becomes a region row and the address's `ISO3166-2-lvl4` code, ahead of the code Overture gives the places inside it (`UG-108` rather than `UG-C`). A code Overture supplies itself wins. Still without regions: Cyprus, Monaco, Madagascar and Singapore (Overture has no divisions with those codes' Wikidata items) and 29 Vietnamese provinces that Overture does not carry. Regions come from the index, not from a database: at startup the service builds one row per ISO 3166-2 code the index names a region for (about 3,700 for the world), the same row a search for `<cc>, <code>` returns, and serves them from `ListRegions` (`RegionsResponse`, ordered by country and admin code). The rows are fixed for the life of the process and need no writable directory. A region code an address carries but the index has no region for (for example the four Irish provinces) is logged and left out of the answer, as before. Earlier versions kept rows in ClickHouse (`CLICKHOUSE_*`, `REGION_MAP_PATH`); those settings are ignored.

`/healthz` reports readiness after initialization. Metrics are exposed at `/metrics` on port 8000.

Coordinates use protobuf float32. Postcode compatibility deliberately includes shared outward/area codes for supported national schemes; it is not a promise of an exact street-level postcode match.

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

CI builds and checks both amd64 and arm64 without downloading models. [CodSpeed](https://codspeed.io/) runs the `benches/` suites (the index analyzer and house-number keys; the service's candidate matching) in its simulation mode on pushes to `main` and on pull requests, and reports changes in instruction counts; run them locally with `cargo bench`, or `cargo codspeed build && cargo codspeed run`. `benches/resolve.rs` also times model-backed parsing (index searches are timed by the full-index gold check, below), and `benches/suggest.rs` suggestions for the same queries cut as if typed (no models), against a small pinned index fixture; the workflow runs them only when the `BENCH_FIXTURE_URL` repository variable is set (see `benches/fixture/README.md`), and the other suites never load models or an index. Regenerate `THIRD_PARTY_NOTICES.txt` with `cargo run --locked --bin notices` after dependency changes. Model-backed RPC tests run separately with provisioned models.

## Local performance benchmark

The `benchmark` binary sends Connect JSON SearchAddress requests to running services. It first sends every query once to each service and counts differing responses, then times `--requests` calls per service and concurrency over keep-alive connections, rotating the service order between rounds and warming connections outside each timed batch. No failed request is retried. Its endpoint checks run in `cargo test`.

Start release-built services on the same index and models. Benchmarking must not depend on production backends or databases.

```sh
cargo build --locked --release --bin opengeocoding --bin benchmark
target/release/benchmark --queries queries.txt \
  --service base=http://127.0.0.1:18121 \
  --service candidate=http://127.0.0.1:18122 \
  --requests 2000 --rounds 3 --concurrency 1,8 > benchmark.jsonl
```

`queries.txt` holds one address per line, for example the `query` column of `tests/addresses.tsv`. A service URL without a route uses `opengeocoding.Geolocation`; to benchmark old clients, use `--service legacy=http://127.0.0.1:18121/geolocation.Geolocation` or `.../nominatim_geolocation.NominatimGeolocation`. All three routes use the same resolver. Results include the client, HTTP transport, parsing, index lookups and the RPC framework on one machine; they are not production throughput or accuracy guarantees.

## Overture address index

`build_index` builds a Tantivy index and house-number tables from a local [Overture Maps](https://docs.overturemaps.org/) release, without Nominatim. The service loads that index (`src/index/`) and it is its only supported lookup backend. The index remains experimental; the coverage and reverse-lookup limitations below and in Reverse geocoding still apply.

`build_index job` (see "Deploy on Kubernetes") fetches all of the sources below itself. To build by hand, download the `division` and `division_area` (divisions theme), `segment` (transportation) and `address` (addresses) parquet files of one release into same-named directories, about 100 GB in total, for example:

```sh
aws s3 sync --no-sign-request \
  s3://overturemaps-us-west-2/release/2026-09-23.1/theme=addresses/type=address/ overture/address/
cargo build --locked --release --bin build_index
target/release/build_index overture out/ --openaddresses openaddresses/ --osm osm/ \
  --geonames-postal GB_full.txt --geonames-postal allCountries.txt
```

Overture's address theme covers about 40 countries. The optional sources fill the rest:

- `--osm DIR`: every `*.osm.pbf` in the directory. Addressed nodes, addressed ways and multipolygon relations at their first node, `addr:place` addresses (numbered within the place), and `addr:interpolation` ways (the numbers between their addressed nodes, spaced along the way; Argentina's block ranges alone give about 45 million); the country is that of the containing place. Files must be sorted by type then id, as [Geofabrik](https://download.geofabrik.de/) extracts are. Pick the extracts for countries without Overture addresses from `download.geofabrik.de/index-v1-nogeom.json` (`iso3166-1:alpha2`); about 150 extracts and 30 GB in October 2026. Add extracts where Overture's coverage has holes: in October 2026 it had no addresses for Bavaria or the city of Milan, so `germany/bayern` and `italy` were added (3 GB). Extracts overlap at borders and with Overture; duplicate house numbers collapse.
- `--openaddresses DIR`: every `*.geojson.gz` in the directory, one OpenAddresses batch job output each, named `<cc>_<source path>-<layer>.geojson.gz` (the country comes from the prefix). List address layers with `batch.openaddresses.io/api/data` and download `v2.openaddresses.io/batch-prod/job/<job>/source.geojson.gz` for the sources Overture doesn't list (about 2,700 files, 13 GB in October 2026). Many overlap Overture's data; identical house numbers on the same street collapse, and in a sample 19% of US and 60% of Mexican OpenAddresses addresses were on a street the Overture-only index had without that number. Each file needs its source definition beside it as `<cc>_<source path>-<layer>.source.json`: the JSON at the job's `source` URL from `batch.openaddresses.io/api/job/<job>` (for a deleted source, its last version in the `openaddresses/openaddresses` git history; `build_index job` doesn't recover those, so it reads them only with `--openaddresses-any-licence`, and in October 2026 that left out about 400 files). Only files whose layer licence is recognised as public domain, open with at most attribution, or ODbL are read; a file with no definition, no stated licence, an unrecognised one, another share-alike licence or a ban on sale or commercial use is skipped. The classification is a conservative text match, not legal review. `--openaddresses-any-licence` reads every file. Each file's licence and whether it was read go to `out/openaddresses.tsv`.
- `--bbox MINLON,MINLAT,MAXLON,MAXLAT`: index only streets, addresses and house numbers inside the box. Places from the input are kept as they are.
- `--geonames-postal FILE`, repeatable: GeoNames postal-code dumps (`download.geonames.org/export/zip/`, CC-BY-4.0). `allCountries` has only outward codes for GB and 3-character codes for CA; `GB_full`, `CA_full` and `NL_full` hold full codes. A postcode is added only when the address data and earlier files lack it.

The writer leaves one Tantivy segment per arena flush (319 for a world index) and every query looks its terms up in each, so the build ends by merging them into four, in parallel; the merge took 40 seconds for a world index and shrank its Tantivy files from 5.5 GB to 4.2 GB. One segment took 140 seconds and queried no faster.

The output holds places, streets (one per normalized name per place and 0.1° cell, merging road names with address streets) and postcodes in Tantivy, plus house numbers with coordinates in sharded FSTs. The cell keeps same-named streets of one large locality apart (Port Chalmers' and central Dunedin's Beach Street); a street crossing a cell edge is indexed in parts, and a lookup answers the middle of the parts. Addresses without a street (Overture's Polish, Czech, Slovak and Baltic village addresses, OSM `addr:place`) are numbered on a street named after their village.

Names are folded by one analyzer at build and query time: Unicode tokens, lowercased, ASCII-folded, then street-name abbreviations canonicalized (`src/index/canon.rs`: `Avenue`/`Ave`/`Avenida`/`Av.` → `av`, `Straße` and compounds → `str`, `Calle`/`C/` → `cl`, `Drottninggatan` → `drottningg` like `Drottningg.`, and so on for English, Spanish, Portuguese, French, German, Dutch, Nordic, Hungarian, Italian, Turkish, Polish, Russian and Ukrainian; Japanese chōme and Taiwanese section numerals read as digits, so `芝公園四丁目` and `芝公園4丁目` match). A query must use the same analyzer, including when correcting typos against the canonical vocabulary.

Streets and addresses are assigned to the containing locality polygon, else a locality point within 5 km, else the local admin area or county. House numbers are spilled to `out/.numbers-spill` during the build (about 7 GB for the world) and removed afterwards. The builder reads available memory at startup (cgroup limit and usage, capped by the host's `MemAvailable`; `vm_stat` on macOS) and sizes the index writer and the number sort from the remaining headroom. One world release took about 100 seconds on an 18-core laptop with a 19 GB peak memory footprint; with all the OSM extracts, OpenAddresses and GeoNames files above it took about 300 seconds and 20 GB (OpenAddresses about 75 seconds, OSM about 105), for 563 million house numbers and 43 million streets in 12 GB. These are single measurements, not guarantees, and the build needs that memory regardless of the writer budget.

Known limitations: outside Overture's address countries, house numbers are only as complete as OSM and OpenAddresses there (sparse in the UK, India and much of Africa and Asia); OSM ways and relations are placed at their first node; interpolated numbers assume even spacing; abbreviations outside that table (`MLK`, `CR 123` for County Road) still index as separate streets; house numbers are matched lowercased and without full-width forms or a trailing 号/號, otherwise exactly as written. Overture's divisions and transportation themes and OpenStreetMap are ODbL (attribution "© OpenStreetMap contributors", share-alike for a distributed derived database); OpenAddresses licenses vary per source and many state none; review them before distributing an index.

### Lookups

Lookups return candidates in the JSON format the resolver reads (Nominatim's search, details and reverse fields); this does not make Nominatim a supported backend. The service renders one search's candidates in English and, when it needs to corroborate a town written in another language, in local names, without searching twice.

It parses with the service's one libpostal parser (serialized), matches places, streets, postcodes and house numbers like Nominatim's all-terms search, and recognizes postcodes from the index when libpostal mislabels them. It reads Japanese block addresses (`東京都港区芝公園4-2-8` is chōme 4 of 芝公園, block 2, house 8) and Chinese road addresses itself, since libpostal splits them. Streets are looked up in the named town, then a same-named county, then a same-named city-province (Istanbul), then by postcode and near the postcode's centre. It reports each house's own postcode, as the source data gives it. It remains experimental: `ResolveRegion` uses the nearest locality point, not a containing administrative polygon (`ReverseGeocode` reads houses, see Reverse geocoding). It reads importance through cached per-segment fast-field columns and checks exact names against hashes folded at startup; Tantivy's `order_by_fast_field` reopened every segment's column per query and cost more than the search itself. The 12 GB index (Tantivy, house-number FSTs and `places.tsv`) is memory-mapped; in an earlier measurement 1.3 GB of it was resident after 20 seconds of load. On an 18-core laptop shared with other work (load average about 13), the service answered the 1,000 address samples at about 360 requests per second over one connection and 1,600 to 2,100 over eight (p99 39 ms). The same build calling a separate `serve_index` process over HTTP, on the same index, answered about 220 and 940 to 1,290 (p99 63 ms), with identical responses. The service's memory footprint was 3.5 GB, against 6.0 GB for the two processes, which each loaded libpostal's models. Per gold query it searched the index 1.8 times on average, where the HTTP version made 3.2 calls: one search now serves both languages. These are single measurements on one machine, not capacity guarantees.

A later round of changes was measured on the 11 GB world index above, with 808 sample queries over one connection and over 16, alternating the old service on the index as built (319 segments) with the new one on a copy merged to four segments, on a busy 18-core laptop. Responses were identical. With the parse cache off: 170 to 806 requests per second over one connection (p95 21.9 to 3.9 ms), 1,516 to 5,512 over 16, and 8.2 to 1.4 ms of CPU per request. Three changes account for it, in order of size: merging the segments (CPU 8.2 to 3.0 ms; the merge happens in the builder, so this needs a rebuilt index), filtering a lookup's document kind in the collector (3.0 to 1.4 ms: a `kind:s` term has 40 million postings that Tantivy walked block by block for every street lookup, which took one name from 0.007 ms to 0.7 ms; the kind is read from which fast columns a document has, so existing indexes gain from it too), and running a request on one blocking thread instead of one hop per parse and search. The same comparison between the two release images in Linux containers (cache off, index bind-mounted from the host) gave 109 to 599 requests per second over one connection (p95 33.5 to 5.3 ms) and 957 to 2,850 over 16, with resident memory 5.0 GiB before and 4.2 GiB after. With the default parse cache the same run on the host gave 875 and 8,724 requests per second, but the 808 queries repeat there, so that number flatters real traffic; within one request alone the cache saved 0.6 of about 2.3 libpostal parses. Startup with the world index dropped from 5.9 to 2.7 seconds (models load while the place table is read, and the table is parsed on all cores). A country filter in the collector measured no gain, and mimalloc gained 1% to 3% throughput for 0.75 GB more memory, so neither was kept. These are single measurements, not capacity guarantees.

A third round changed only how lookups run, not what they return. A lookup seeks the document runs of its kind (the builder writes places, streets and postcodes in 10 to 20 runs per segment), so a place's name no longer walks the millions of streets that share its words. The country filter reads a per-document country table built at startup (2 bytes a document, about 0.1 GB for a world index) instead of intersecting with a country term, whose postings tantivy's skip list walks one block at a time. A request remembers its own lookups (over half of them were repeats: the same town for every fallback and street scope), existence checks stop at the first match, and a place's English name is located once at startup. On the same busy laptop and world index, with the parse cache off, the 1,000 address samples took a mean of 0.8 ms instead of 2.4 ms over one connection (p99 5.3 ms instead of 23 ms; 1,240 requests per second instead of 410) and 5,000 to 5,400 requests per second over 16 instead of 3,100 to 3,300, where libpostal's single lock now sets the limit. With the parse cache on and the samples repeating, 16 connections gave 16,000 requests per second instead of 4,400. The gold suite's 3,732 cases returned identical responses, with total SearchAddress time down 63% (p99 31.5 to 10.7 ms), and `tests/suggest.rs` typed prefixes again took p50 82 µs instead of 125 µs (p99 3.4 ms instead of 4.6 ms). These are single measurements, not capacity guarantees.

In one run against the October 2026 Overture release with the OSM, OpenAddresses and GeoNames sources above, the gold suite passed 1,038 of its first 1,280 cases (231 of the 286 held out), against 992 for the index before the October 13 data and parsing work. On the 2,875-case version it passed 2,136 (433 of the 582 held out), including 545 of the 926 damaged addresses: a typo in the street (2 of 95) and a wrong postcode (10 of 95) were its weakest kinds, since streets then matched only on exact (canonicalized) words. Lookups now correct a street with one misspelt word (within one edit, same first letter, or a misspelt street type) when a resolved town or postcode holds a street with the other words and the house number. The service accepts a candidate whose postcode disagrees with the requested one when its house number, road and town all agree, after every candidate holding the postcode has been refused, and returns it with goodMatch false. With both, and 32 Census expectations corrected (their five-digit house numbers had been read as postcodes), the 2,875 cases passed 2,308 (459 of the 582 held out), including 701 of the 926 damaged addresses: 82 of 95 street typos and 73 of 95 wrong postcodes. One must-pass case regressed with them: "10 Rue de Rivoli, 75001 Paris, France" now answers number 10 of the same street in 75004. Of the original 247 cases it passed 239; Nominatim 5.3.2 with January 2026 OSM data passed 227, and the only case Nominatim passed and the index did not is a Dubai boulevard whose street data names a smaller locality than Dubai. Of the 405 address cases (Wikidata landmarks in 90 countries and registry addresses from France, the Netherlands, the United States, Norway, Switzerland and Spain), it passed 286. Most remaining failures are data gaps (streets or villages missing in OSM-only countries) or the service rejecting the index's right first answer on libpostal's parse ("Addis Ababa, Ethiopia" parses as one city name); those critical ones are labelled in `tests/gold.seeds.jsonl`. On the 1,000 address samples, the index answered 827 (Nominatim 855) and 143 at house level (117). Where the sample's country is known, it chose the right country for 97.3% of its answers (Nominatim 97.5%) and claimed a good match in the wrong country 14 times (Nominatim 14). To keep wrong countries down, a lookup answers a street with nothing to place it only when at most three same-named streets with that number exist, all in one country. It answers a place name without a country only on an exact name match, and a bare postcode without a country only when one country uses it. These are single runs on curated and sampled inputs, not production accuracy.

## Geographic evaluation

The `evaluate` binary (`src/bin/evaluate`) measures geographic correctness of the service and its index through the service's RPCs. It is for backend, data and consumer changes; it does not measure throughput. Build it with the service:

```sh
cargo build --locked --release --bin opengeocoding --bin evaluate
```

| Suite | Command | Needs |
| --- | --- | --- |
| PR (CI) | `cargo test --locked` (or `target/release/evaluate check`) | Nothing; scorer, diff gate, corpus and snapshot checks |
| Smoke | `target/release/evaluate run --suite smoke --index out --out eval-results/smoke` | Index, models |
| Pre-release | `target/release/evaluate run --suite gold --index out --out eval-results/gold` | Same |
| Address snapshots | `target/release/evaluate run --suite addresses --index out --out eval-results/addresses --compare tests/snapshots/addresses.tsv` | Same |
| Compare | `target/release/evaluate diff BASELINE CANDIDATE` | Two saved runs or snapshots |
| Re-score | `target/release/evaluate report RUN_DIR --rescore` | A saved run; lists cases whose class changed |

`run` starts `target/release/opengeocoding` (or `SERVICE_BINARY`) on the index in `--index DIR` (or `INDEX_DIR`) with no other environment than `PARSE_CACHE=0`. It sends each case `--repeat` times (default 3) and records the fastest, outside the rate limit and budget; `diff` opens with a speed section comparing those latencies in aggregate: it fails when the total, p50, p90 or p99 is more than 10% slower (not part of `diff`'s exit code; CI reads the section's `<!-- speed: pass|fail|skip -->` line), and skips sides timed with different `--repeat`. Three runs of one binary on a cluster node agreed within 2% in total and 5% at each percentile, single cases only within about 30%, so single cases and the max are not gated; a laptop's totals moved by tens of percent. It rate-limits its RPCs (`--max-rps`, default 20) and stops after a request budget (`--budget`, default 20,000). It counts each case's index searches from the service's `/metrics`, since cases run one at a time. For direct checks (`--no-direct` skips them) it loads the same index and its own libpostal parser, about 4 GB besides the service's. `--rpc URL` targets a running service instead, one request per case; search counts are then unavailable, and direct checks run only with `--index`. Results are checkpointed per case (`--resume`); the process is stopped on exit, including after Ctrl-C. No request is retried by the evaluator.

Each run writes `manifest.json` (a hash of the index's `meta.json`, which names its segments, consumer commit and binary hash, libpostal model and reference-data hashes, cache state and limits), `results.jsonl` (full responses, index search counts, direct-index evidence) and `report.md`. Label the backend with `--backend-label`, e.g. image digest and configuration. A code or configuration comparison requires the same built index; a data or import change is a separate experiment.

### Gold corpus

`tests/gold.jsonl` is generated by `evaluate build-gold` from the curated case list in `tests/gold.seeds.jsonl` (one line per place or construct: queries, cohort, tolerances, must-pass and known-failure labels). Expected coordinates, countries, regions and components come from Wikidata, Base Adresse Nationale, PDOK/BAG, the US Census geocoder, postcodes.io, Kartverket (© Kartverket, CC BY 4.0), swisstopo's building address register (© swisstopo) and CartoCiudad (© IGN/CNIG, CC BY 4.0), never from Nominatim or this service. Sampled cities, towns and regions in about 240 countries and territories (some written as production writes them: "Stratford, CT", or a bare unique name) were picked from GeoNames by population and production weight, sampled registry addresses from Overture, and landmarks from Wikidata items with a street address; GeoNames and Overture only chose the cases, and their expectations come from the sources above. A place in a territory with its own ISO code (Puerto Rico, Réunion, a French overseas department) accepts that code or the sovereign country's.

The `degraded` cohort (929 cases) takes registry-verified addresses and changes one thing in the query: no country, no postcode, a typo in the street or town, a wrong postcode, another town with the right postcode, a neighbouring country, the parts reversed, all capitals or lowercase, or an abbreviated street type. The expected answer stays the registry's address, re-verified by `build-gold`. Edits that leave the address consistent expect goodMatch true; contradictory ones (typos, wrong postcode, town or country) accept either goodMatch value, since whether a repaired match is "good" is not decided. Cases are tagged with their kind (`typo_street`, `wrong_country`, ...). `build-gold` looks each registry reference up once, one registry at a time in parallel, and reads Wikidata in batches of 50; the 2,894 cases took about a minute to regenerate. It covers cities in many scripts, street addresses, postcodes, regions, ambiguous names, rural places, borders, fallbacks, malformed input and no-result input. Each case records an expected country, a resolution range, an area (point and radius, or ISO 3166-2 code), relevant components, a goodMatch expectation, provenance, licence and review status. Variants share a `location` key so they are not counted as independent places. Roughly one in five locations is held out by a stable hash and reported separately; keep it out of tuning. Review status is `source-verified`; human review is pending.

Cases are scored as `pass`, `confidence_mismatch`, `too_coarse` (a fallback never passes an exact-address case), `abstain`, `too_fine`, `wrong_component`, `wrong_region`, `near_miss`, `far_miss` (>100 km), `wrong_country` or `error`. Critical classes are listed individually, flagged when goodMatch was true. For failures the evaluator queries the index directly (raw, structured, reverse, details) to hint whether the backend lacked an acceptable candidate or the consumer rejected one. SearchAddress, ResolveRegion, ListRegions and SuggestAddress are exercised.

Every case with a place, postcode or address to find, except the `degraded` cohort (repairing a damaged address is SearchAddress's job), also types its query into SuggestAddress (`full`): it passes when one of the 5 suggestions is in the expected country, of a type of the expected level (`address` for a house, `street`, `postcode`, `country`, any place type otherwise) and within the expected radius. Where the text before the first comma is the place's English Wikidata label, `build-gold` adds a `suggest` check of that name alone (`name`): the located Wikidata items with that label and more population (P1082) are listed in `larger`, and the place must come no later than right after them (`rank`). So `Paris` must suggest Paris, France first and Paris, Texas no later than second; a name with five or more more-populous namesakes is not checked. Both are classed `pass`, `ranked_low`, `missing` or `error`, reported per cohort with every name out of population order, and add up to two RPCs per case. In one run on a world index (Overture 2026-09-23.1), `full` passed 943 of 1,947 cases (5 of 721 addresses: a complete address with its postcode and country is rarely completed) and `name` 670 of 829. Connect protobuf answers are decoded into the contract type and must equal the Connect JSON answers field for field. grpcurl runs as a known failure and is reported, not skipped.

`known_failure` labels record triaged baseline failures with their attribution. They stay visible in every report, and a label is only removed after a fix is verified. Do not weaken expectations or regenerate them from a run.

### Registry-first samples

The gold seeds above were picked from Overture, the index's own data, and then checked against a registry, so a registry address the index lacks was never sampled and recall cannot be measured. `tests/registry/` holds samples drawn from the registry first. `evaluate check` and `cargo test` validate each `tests/registry/*.jsonl`; `run --corpus tests/registry/us.jsonl` scores one. They are part of the gold suite: `run --suite gold`, `diff` and `report` read `tests/gold.jsonl` and then `tests/registry/*.jsonl` unless `--corpus` names a file, `evaluate gold-corpus --out FILE` writes the suite as one file (the eval image carries it as `/app/tests/gold.jsonl`, so the CI run scores both), and the run manifest hashes the files in that order. `diff` compares only case ids both sides have, so the pull request that adds a corpus is not gated on it. No case is `must_pass`; the known failures are labelled.

**United States** (`us.seeds.jsonl`, `us.jsonl`): the USDOT National Address Database, release r24 (public domain, a work of the federal government; the USDOT disclaims accuracy and the data is not for mailing lists). The 8.5 GB download is a Deflate64 zip that Apple's `unzip` and Python cannot read; use 7-Zip. The text file is 42 GB and 99 million rows, 89.6 million of them plain, live addresses with a house number, street, five-digit ZIP and a city.

```sh
target/release/evaluate sample-us --nad NAD_r24.txt --seed us-2026-10 --out tests/registry/us.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/us.seeds.jsonl --out tests/registry/us.jsonl --nad NAD_r24.txt
```

The draw keeps each row's rank by `sha256(seed|uuid)`, so it does not depend on file order, and rerunning it on the same release gives the same seeds byte for byte (about 4 to 8 minutes; `build-gold` about 80 seconds). `build-gold` finds every sampled UUID in the file again and copies its point, house number and ZIP, and fails if one is gone (a new release needs a new sample). The sample:

- 300 house addresses and 100 units (`Apt 4B`) whose placement the registry states as building, entrance, interior, site, access or parcel centroid, in three density strata (`urban`, `suburban`, `rural`: 10,000 or more, 2,000 or more, fewer address points in the ZIP). Density is an address-count proxy per ZIP, not a census measure, and partly mirrors how completely a state contributed.
- 150 `us_broad` house addresses of any other placement, scored at 0.5 km instead of 0.15 km (0.3 km for a parcel centroid), reported apart: 72% of the release has placement "Unknown", and nearly all of Texas, California and Florida are in it.
- States in turn: the draw keeps up to 3,000 candidates per kind and state, and each stratum takes one location from each state in a seeded order until it is full, so a state's share does not follow how many rows it contributed (New York holds 6.5 million trusted rows, Pennsylvania 0.1 million). Rates are therefore closer to an average over states than over addresses.
- One address per street and at most three per ZIP. A street field holding a unit or only a direction is dropped, as is a unit that is a word ("Office", "Rear", "Suite1") rather than a number or letter.
- Rows that contradict the registry are dropped: a row whose state is not the one holding most of its ZIP's rows (some Virginia counties' addresses carry the neighbouring state's code), and an address the registry places twice, further apart than the case's tolerance.
- The town is the first of the postal city, municipality, community and Census place that names one, written as people write it: "Salcha", not the Census "Salcha CDP"; "Wausau", not "Town of Wausau"; "Atlanta", not the postal code "ATL" or "ADK".
- A damaged variant for half the locations, drawn evenly among the edits that apply to it: no postcode, country added, typo in the street or town, wrong postcode, another town in the same state, parts reversed, upper or lower case, abbreviated street type or direction, no commas, the state spelled out, ZIP+4, and for units `#4B` or `Unit 4B`. Variants share their location key and expect goodMatch true when the edit is consistent, either value when it contradicts itself.

Limits to repeat with any number from it: the 400 trusted-placement locations come from 29 states and the District of Columbia, at most 20 from one, and with the broad sample the 550 locations cover 45 states and the District of Columbia, at most 24 from one; Hawaii, Michigan, Mississippi, Nevada and New Hampshire have no usable rows in r24, and 16 states, Texas, Georgia and Ohio among them, appear only in the broad sample; NAD is aggregated from state and local data of uneven coverage (Florida and Georgia are almost absent); registry rows are public address points of any building, not occupants; and the index itself is built from sources derived from the same state and local registers, so a pass measures fidelity to the register, not independent truth.

**Review.** Done by the assistant that wrote the sampler, not by a person: review status stays `source-verified`, human review pending. Every location's point was checked against the Census 2020 ZCTA gazetteer (public domain): 543 of 545 lie within three ZIP radii or 15 km of their ZIP's centre; the two others are Gray, KY (inside the cloud of NAD points for its ZIP) and Long Beach Township, NJ (on Long Beach Island, 29 km from the ZCTA's internal point, where the index has the house 13 m away); five ZIPs that are not ZCTAs were left alone. Reading the queries and the failures found what the filters now drop: towns named "Unincorporated" or "Denton County", Census suffixes and three-letter postal codes as towns, word units, a street field holding a unit, a bare direction, a direction and type written twice ("West W Red Oak St Street"), and addresses in Lee, Buchanan, Frederick and Clarke counties, Virginia, filed under Tennessee, West Virginia or Maryland with Virginia ZIPs.

**Triage.** `us.labels.jsonl` holds one `{"ref", "query", "known_failure"}` per failing case; `build-gold` attaches it by registry UUID and query, so labels survive a resample and a label that matches no case is an error. Of the 16 failing base locations, none is matching: 10 are index data and 6 reference data. Each was rerun by hand (with the ZIP alone, with the index's locality, without the town):

- **Index data (10):** the house is not in the index (Arlington, WA; Flemington, NJ; Lakeland, FL; Evansville, IN; Hollywood, FL; the Lakeland house is not in OpenAddresses either), or only its street is (Cheshire, MA; Atlanta; Budd Lake, NJ; a Seminole reservation street in Hollywood, FL; Leonard Lane, in Lee County, Virginia, given with a Tennessee town and ZIP).
- **Reference data (6):** three units the register places at their own point, 210 m, 280 m and 1.9 km from the building the index answers (mobile-home lots); a unit 252 m off; a house the index files under Irma, 262 m off; and a house the index has at the register's point in Clarke County, Virginia, which the register gives a West Virginia town and ZIP.

Of the 300 damaged variants, 8 fail and are labelled: 6 share their base case's cause; "410 Welcome Springs Lane, Talladega, AL 35125" has a wrong ZIP on an address the index files under Emauhee, two contradictions, and answers the stated town; and "832 Napoleon Avenue, Baton Rouge, LA 70115", the one critical result, answers Baton Rouge, 120 km from the New Orleans address, because the index holds that house without a postcode and does not know ZIP 70115, so nothing contradicts the stated town.

**Matching fixes found through this sample.** Each failing case was rerun by hand and the cause fixed where it was the service's, not the data's or the register's:

- Comma parts bound the parse in more US shapes: a town swallowed into the road ("4724 3rd Place Northwest, Washingotn, DC"), a road read as a suburb ("2219 Pacific Coast Highway, Lomita"), a county part between street and town ("15 Westervelt Place, Essex, Irvington Township, NJ"), a trailing country part, and "street, STATE ZIP" with no town, where libpostal took the street type for the town ("701 GREENWOOD Avenue, OH 45229") or the state for part of the road.
- A reversed address whose first part is the country ("United States, ND 58104, Fargo, 4122 Osgood Parkway South", "France, 76160 …, 258 Rue …") is resolved in the usual order. The resolver turns the query around itself; `Parser::parse` does not, since the structured re-queries are written country first on purpose.
- A unit left at the end of the road ("#B", "Apt 10A", "Suite 200") is moved out of it; "Ste Foy" keeps its "Ste".
- A two-letter US state code that libpostal took for a country ("Dover, DE 19901" as Germany) is a state when the index knows the ZIP in the US and not in that country.
- A postal town within 10 km of a house whose number, road and ZIP match (`house_despite_town`) is a confident answer: Salcha for a house the data files under Aurora Lodge, Peoria for Bellevue. Further away it stays goodMatch false.
- "Township", "Borough" and "Town" qualify a US place name ("Long Beach Township" is Long Beach, "City and Borough of Juneau" is Juneau), for the resolver's town check and the index's place lookup.
- A misspelt street is checked against every street type or direction within an edit, and against name words too ("southewst" is as near "southeast" as "southwest"; "adlee" is "allee" or "adele"), and the resolver accepts the road the index corrected ("West 54th Srteet").
- A US house with only its state to place it ("1516 RUTH Avenue, OH") is answered when the state holds that number on one street of that name, and with a contradicting ZIP and no town ("7413 Janes Ranch Road, TX 78747") the same lookup answers with goodMatch false. A house number that is also a ZIP ("15411 Woodcliffe Trail, IN") is no longer taken for the ZIP when a state is stated. A road alone no longer outweighs a contradicting state code ("10 Elm Street, MA" is not Newark's), a structured re-query does not place a street that only a state places, and a stated state is not the country of the same code ("MA", Morocco).
- With a stated country that contradicts the rest ("715 Meehan St, Santa Maria, CA 93454, Canada"), the house the query names without the country is answered when its number, road and postcode are the stated ones.
- The evaluator equates a ZIP+4 answer ("50625-8016") with its ZIP, and `build-gold` takes the last four-digit word of a swisstopo reference as the postcode ("Chieberg 1526 3855 Brienz" had house "Chieberg" and postcode 1526).

One local run against a world index built from the October 2026 sources (the gate passes on this corpus with these labels; a second run after the last change showed zero drift): 534 of 550 locations passed (97.1%, Wilson 95% interval 95.3 to 98.2), 386 of 400 with a trusted placement (96.5%, 94.2 to 97.9), 148 of 150 broad (98.7%, 95.3 to 99.6), 121 of 123 held-out base locations, and 292 of 300 damaged variants (97.3%, 94.8 to 98.6). On `main` before these fixes (commit `32db24a`) the same sample passed 527 of 550 and 260 of 300. Across the whole gold suite (curated and registry, 3,759 cases) the fixes made 121 cases pass and none fail or fail worse; the curated US cases went from 384 to 441 of 460, and the 1,000 address samples changed in three answers, all finer or more confident and correct. By hand, the index held the address, or for units the building, for 6 of the 16 failing base locations, so recall is about 98%. These are a development run, not a published accuracy claim.

### Gates

`run` fails on a must-pass failure, an untriaged critical result or a transport regression. `diff` fails on any newly failing case, a worse failure class, a new critical result or a must-pass failure in the candidate. It lists newly passing cases for review and drift in unscored cases. SuggestAddress checks do not gate `run`; in `diff` a check that stops passing, or gets a worse class, fails like a search case. Accept a backend change only when both gates pass on the same corpus, consumer, models, reference data and fresh region cache. Report cohort changes as case counts: with about 250 cases, aggregate rates carry several points of uncertainty and per-country rows are individual evidence, not rates.

The `prepare` and `gold` jobs of `.github/workflows/staging-publish.yml` run the gold suite on every pull request that touches the service, its data or the corpus, and on every `main` commit. After the workflow pushes a commit's images, they send the pull request's merge base and head hashes to an authenticated endpoint of a private cluster, which evaluates both staging images on one full index (building it first from the head image's builder when it has none), then runs `evaluate diff --relative` on the two runs and returns the report (failing and worse cases open, the lists to review and the run details folded, empty sections left out, and a SuggestAddress table with the passing checks on both sides); the workflow comments it on the pull request (also in the job summary) and sets a `gold-full` commit status, and fails on any newly failing or worse case or new critical result. The report's speed section (SearchAddress, and SuggestAddress timings that are reported but not gated) goes into a comment and a commit status of its own, `gold-speed`, which fails, with an error in the job, when SearchAddress is more than 10% slower in total, p50, p90 or p99. `--relative` leaves out must-pass cases that fail on both sides, so the check shows change, not accuracy. To reproduce a run locally, use `evaluate run` on each side, then `evaluate diff BASE HEAD --relative`. No pull-request code runs on GitHub in this workflow, and the cluster holds no GitHub credentials. Pull requests by the owner, members and collaborators use the `gold-trusted` environment and run unattended; every other pull request waits in `gold-approval` for a reviewer, who should read the diff first, since approving it runs that author's code in the cluster. Both environments accept workflows from `main` only and hold the endpoint key (`GOLD_GATEWAY_KEY`) and URL (`GOLD_GATEWAY_URL`). Pushing again to a pull request, or closing or merging it (`gold-cancel.yml`), cancels its staging build and its run, including the one in the cluster. The workflow skips a commit whose merge base has no multi-architecture staging image. A first index build takes hours and can outlast the job's time limit; the run continues in the cluster and later runs reuse its index.

### Address snapshots

`tests/addresses.tsv` holds 1,000 real address and location strings sampled from business addresses, job postings and profile locations. They are spread across 125 countries and stratified by the granularity they target: full address, street, postcode, city, region, country and unparseable. They show how a code or backend change moves everything else; they have no expected answers and measure change, not accuracy. The file holds only the query strings and their geography. Source names, providers, coordinates and identities are removed, and so are imprint addresses and strings naming a company. `tests/snapshots/addresses.tsv` records the accepted behaviour, one row per case: country, level, coordinates (3 decimals, about 100 m), goodMatch, house number, road, postcode, locality, ISO and enriched regions, and the query. A changed answer is one changed row in a PR; `git diff --word-diff` shows just the changed fields. Run metadata (backend version and data date, consumer commit, binary, model and reference-data hashes) is in `addresses.manifest.json` beside it, so it does not clutter the row diff. Full responses stay in the run's `results.jsonl`. Tabs, newlines and other control or separator characters in values are written as backslash escapes.

```sh
target/release/evaluate run --suite addresses --out eval-results/addresses \
  --compare tests/snapshots/addresses.tsv
# after reviewing the changes, accept them
target/release/evaluate snapshot eval-results/addresses tests/snapshots/addresses.tsv
```

`tests/snapshots/gold.nominatim.json` is a reference run of the then 1,280-case gold suite through this service against the Nominatim 5.3.2 deployment it replaces (OSM data of 2026-01-26), taken on 2026-10-13 at 20 requests per second: 888 of 1,280 cases passed (227 of the original 247, 194 of the 286 held out). It holds the run manifest, the class counts and every case's full result except timings and backend traces, one case per line. It is kept for reference and is not regenerated when the gold set changes; compare a run with it using `evaluate diff tests/snapshots/gold.nominatim.json RUN_DIR` (cases added since then show as new). `evaluate snapshot RUN_DIR FILE.json` writes this format.

The diff counts per target granularity which cases gained or lost a result, changed country, became finer or coarser, moved more than 1 km (or less, but visibly in the snapshot), flipped goodMatch, or changed components or regions, and lists each one. `--fail-on-drift` turns any change into a failure. Rerunning an unchanged binary against the same backend produces no drift. A changed answer is not automatically better or worse. Snapshots depend on the backend's data; regenerate the baseline on the target backend before comparing code changes there.

The sampler that queries production data is kept outside this repository; it reuses `opengeocoding::eval` for TSV escaping and hold-out assignment. Other corpora are JSONL in the gold format, or TSV with at least `id` and `query` columns, passed with `--corpus`. Private ones (e.g. replay sets with earlier production geocodes) use the same tooling; keep those and their runs out of this repository.

GitHub-hosted runners have neither a world index nor models, so CI runs only the offline checks in `cargo test`. Run the model-backed suites locally or on a runner with the index and models mounted.

## GHCR publishing

`.github/workflows/docker.yml` runs on `v*` tag pushes, which release-please creates when its Release PR is merged (it needs the `RELEASE_PLEASE_TOKEN` secret, because tags pushed with `GITHUB_TOKEN` trigger no workflows). It runs the existing checks first, then uses Docker's pinned reusable workflow to build on native amd64/arm64 runners, push to `ghcr.io/mokto/opengeocoding`, and assemble and verify the multi-platform manifest. Pull requests do not publish to that image.

Separately, `staging-build.yml` builds the `eval` image on native amd64 and arm64 runners (the runtime image plus the `evaluate` binary and `tests/gold.jsonl`) for every pull request (forks included) and every `main` commit, with no secrets, and `staging-publish.yml` pushes it to `ghcr.io/mokto/opengeocoding-staging` from the default branch without running it: `sha-<commit>-amd64`, `sha-<commit>-arm64` and the multi-platform `sha-<commit>`. These images are untrusted builds for evaluation environments, not releases; set that package's visibility to public after its first push so evaluators can pull it anonymously. The publish workflow only triggers once it exists on `main`, so it cannot be tried from the pull request that adds it. Old tags are not pruned yet.

The repository is `Mokto/opengeocoding`. The Rust crate and service executable are `opengeocoding`, and the GHCR image is `ghcr.io/mokto/opengeocoding`.

A `v1.2.3` Git tag produces `1.2.3` and `latest`; pushes to `main` publish nothing. Publishing uses the workflow's `GITHUB_TOKEN` with `packages: write`, so no personal access token secret is required. OCI labels link the package to this repository. Crates and GitHub releases are not published.

After the image, the same workflow publishes the Helm chart to `oci://ghcr.io/mokto/charts/opengeocoding`, pinned to that image: a `v1.2.3` tag gives chart version and appVersion `1.2.3`.

GHCR can initially create packages as private even for public repositories. After the first successful push, set the [package visibility](https://github.com/users/Mokto/packages/container/opengeocoding/settings) to public if anonymous image pulls are required.

## Reference data and normalization

Country aliases use public Unicode CLDR and GeoNames data. Administrative-region hints use GeoNames; the Wikidata items of the ISO 3166-2 subdivisions Overture lacks (`iso3166_2`, below) come from Wikidata (CC0). Sources, revision and download hashes are recorded in `data/sources.json`; the checked-in data allows offline reference generation during normal builds.

To refresh it, run `cargo run --locked --bin update_reference`. This requires network access and changes the checked-in dataset; review it as a data update. Character classes come from the Unicode Character Database version pinned in the generator (currently 16.0.0), not from the toolchain.

An optional build-time `REFERENCE_DATA` path can supply a custom reference JSON. Search overrides are empty by default. No private deployments, endpoints or address overrides are included.

Transliteration uses BSD-3-Clause `deunicode`, **not Python Unidecode**. Outputs differ for some scripts and symbols. In a migration comparison of 7,067 geographic/sample strings, 55 normalized outputs differed, including Hebrew and Japanese forms. Matching normalizes both sides consistently, but these differences still need real-address accuracy validation before replacing another service.

## License

Project source: **[PolyForm Shield 1.0.0](https://polyformproject.org/licenses/shield/1.0.0)**. Any use is allowed, including commercial use, changes and redistribution, except providing a product that competes with the software or with a product the licensor provides using it; a free product can compete too. There is no later conversion to an open-source license. This is source-available, not OSI open source. Versions published before the switch were licensed under FSL-1.1-ALv2 and keep that license, including its Apache-2.0 grant two years after each publication. See `LICENSE`, `NOTICE` and `LICENSES/` for third-party software and reference-data terms.

libpostal and rustpostal are MIT-licensed; deunicode is BSD-3-Clause. CLDR data is Unicode-3.0; GeoNames data is CC-BY-4.0. Model files are not bundled; their source-data licensing and attribution obligations are separate.
