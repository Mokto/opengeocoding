# Deployment guide

[Documentation](../README.md) · [API](api.md) · [Index building](indexing.md)

Run commands from the repository root. Serving requires both libpostal models and a compatible built index; neither is bundled in the image. For an automated source/model fetch and build, use the Helm chart or its `build_index job` command below. World builds need substantial memory, disk and initial downloads; regional builds are smaller.

[Docker](#run-with-docker) · [Kubernetes](#deploy-on-kubernetes) · [Configuration](#configuration)

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

`latest` is the newest release. Models and indexes are not included in the image. For repeatable deployments, pin a release tag or image digest. If building an index with local Cargo, build the runtime image from the same checkout instead of pairing it with an unrelated published image.

Download libpostal models explicitly into a writable directory. Downloads are large; nothing downloads automatically at import or startup:

```sh
mkdir -p models
docker run --rm --entrypoint libpostal_data \
  -v "$PWD/models:/data" opengeocoding download all /data
```

The resulting directory is `models/libpostal`. Keep it for subsequent runs.

Build an index into `out/` using the [index-building guide](indexing.md), or use `build_index job` and `pull` as described under [Deploy on Kubernetes](#deploy-on-kubernetes). Use the same image for building and serving: indexes are tied to the builder version. The models and index must be readable by the container's uid 1000. Mount both read-only:

```sh
docker run --rm \
  -p 127.0.0.1:11121:11121 -p 127.0.0.1:8000:8000 \
  -v "$PWD/models/libpostal:/data/libpostal:ro" \
  -v "$PWD/out:/data/index:ro" \
  -e INDEX_DIR=/data/index \
  opengeocoding
```

The service derives its administrative regions from the index at startup. It needs no database or separate geocoder.

**Deploy behind a trusted network or authenticated proxy.** The RPCs do not implement authentication. Do not expose them directly to the public internet.

Example request:

```sh
curl http://localhost:11121/opengeocoding.OpenGeocoding/SearchAddress \
  -H 'Content-Type: application/json' \
  -H 'Connect-Protocol-Version: 1' \
  -d '{"addressString":"Copenhagen, Denmark"}'
```

## Deploy on Kubernetes

The Helm chart in `manifests/` builds the index in the cluster and serves it. It needs an S3-compatible bucket (AWS S3, MinIO, SeaweedFS, Ceph, R2 and the like) that already exists: nothing here creates it, and the leader exits when it can't list it.

```sh
kubectl create secret generic opengeocoding-s3 \
  --from-literal=AWS_ACCESS_KEY_ID=... --from-literal=AWS_SECRET_ACCESS_KEY=...
helm install geocoding oci://ghcr.io/mokto/charts/opengeocoding \
  --set store.url=s3://my-bucket/opengeocoding \
  --set store.endpoint=http://seaweedfs-s3.seaweedfs:8333 --set store.allowHttp=true \
  --set store.credentialsSecret=opengeocoding-s3
```

`store.endpoint` is only for stores other than AWS S3; any other `AWS_*` setting, such as a web identity role, goes in `store.extraEnv`. See [`manifests/values.yaml`](../../manifests/values.yaml) for every setting.

- **Leader.** The chart installs no build Job and no serving Deployment of its own. A small Deployment (`leader`, two replicas, one holding a Lease) runs the same loop every 30 seconds with no state beyond the cluster and the bucket: if the bucket has no sealed `indexes/<id>/_COMPLETE` for the index the chart names, it makes sure one build Job exists, replacing a failed Job after `build.retryDelaySeconds` (the Job itself never retries its pod); once the index is sealed, it deletes the build Jobs and server-side applies the serving Deployment, whose rolling update is the rollout. Helm only renders both workloads into a ConfigMap (`<name>-workloads`). No serving pod exists, or waits in an init container, before the first index does. That ConfigMap owns the Deployment and the Jobs the leader creates (owner references), so removing the release removes them, and a GitOps tool shows them under it (Argo CD neither tracks nor prunes them). The leader moves a build Job an earlier chart's leader created (owned by the leader's Deployment, or tracked by Argo CD) under the ConfigMap the same way. `build.enabled: false` keeps the leader from building: it then serves an index only once something else has published it.
- **Build Job.** `build_index job` exits at once if the bucket already holds the index the chart names. Otherwise it fetches the sources and libpostal's models, builds, opens the result as the service would and checks a sample of its places, then publishes it. Each source is copied into the bucket as it arrives, so a later build or a retried pod reads the bucket instead of upstream. A world build needs about 16 CPUs, 32 GiB of memory and 250 GiB of scratch (`build.*`), and about 200 GB of downloads the first time, including the places theme and 42 GB of landmark-only OSM extracts (`osmLandmarks`). Scratch is deleted with the pod unless `build.scratch.existingClaim` names a volume you keep: a later build then copies from the bucket only the files that do not already match it, and deletes the earlier build's source files its sources no longer name (another Overture release's, a dropped extract's or layer's).
- **Serving pods.** An init container (`build_index pull`) downloads it and the models to the pod's own disk and checks every file's SHA-256. The service then opens them read-only. The service itself never downloads anything. Each pod needs about 6 GiB of memory (16 GiB keeps a world index's page cache), and space for the index and models. A later world index with landmarks was roughly 16 GB before models; budget additional space for upgrades. These are observed sizes, not resource guarantees.
- **Index names.** The chart names the index by `index.builderHash`, `index.modelsVersion` and `index.sources`, and passes the same name to the Job and the pods. `builderHash` hashes the builder's code (`build_index hash`), except the code that fetches sources and talks to the store (`fetch.rs`, `store.rs`): sources are sealed in the bucket and reused as they are, so that code can change without rebuilding. A fix to how sources are selected therefore takes effect only with a new `snapshot` label, or for Overture (stored by release) after deleting its directory in the bucket. `cargo test` fails until `manifests/values.yaml` matches the hash, so a builder change is a visible chart change. The Job refuses an image built from other builder code, and the service refuses an index whose `build.json` names another builder. `index.sources.snapshot` labels one fetch of the sources that only publish "latest" (OSM, OpenAddresses, GeoNames); a new label fetches them again. Within one label an extract can move between `osm` and `osmLandmarks`, and is taken from the other list's sealed mirror; an extract neither mirror holds is downloaded alone and sealed in `sources/<snapshot>/geofabrik/<file>`, so it is newer than the rest of the label's fetch, and later builds with that label reuse it. `index.sources.osmLandmarksSnapshot`, when set, labels the `osmLandmarks` fetch instead (`sources/<label>/osm-landmarks`), so a new `snapshot` does not download the landmark extracts (about 42 GB from Geofabrik) again. A daily workflow (`.github/workflows/source-refresh.yml`) opens a pull request setting both `overtureRelease` and `snapshot` when Overture publishes a release, and a new `snapshot` alone once the current one is 30 days old and OSM, OpenAddresses or GeoNames changed since; closing it declines that release, or the snapshot refresh for 30 days. It keeps `osmLandmarksSnapshot` until that label is 90 days old. A snapshot refresh merges itself once the gold check finds no regressions and no slowdown against the latest `main`: a branch behind `main` is rebased and checked again, and one with commits from anyone but the workflow is left to a person. An Overture release always waits for a person, and so does the release that deploys either.
- **Upgrades.** A change that names a new index makes the leader start a new Job (a build still running for another index is deleted: one build at a time). The serving Deployment keeps its old template until the new index is sealed, so the old index keeps serving during the build; its rollout then replaces pods one at a time (`maxSurge: 1`, `maxUnavailable: 0`). Upgrading a release installed with an earlier chart, which rendered the Deployment itself: the leader takes the Deployment over on its first round, but a GitOps tool that still tracks the old Deployment (for example Argo CD's `argocd.argoproj.io/tracking-id` annotation) will prune it once the chart stops rendering it, so remove that annotation first. Rolling back the values rolls back to the old index, which is still in the bucket. Nothing deletes old sources or indexes.
- **Regional builds.** `index.sources.bbox: [min lon, min lat, max lon, max lat]` builds only that area. Overture is then read only where its row groups meet the box (copied unchanged, without decoding), OpenAddresses layers are picked by their bounds, GeoNames by row, and the builder drops everything outside the box. Pick OSM extracts in `index.sources.osm` by hand. A regional index answers queries about other places with whatever it holds: "Paris, France" against a Liechtenstein index answered a Paris in Lombardy, with goodMatch true.

Bucket layout, under the URL's prefix: `sources/overture/<release>/<extent>/<type>/`, `sources/<snapshot>/{osm,osm-landmarks,openaddresses/<extent>,geonames/<extent>/<file>,geonames-populated/<extent>}/`, `models/libpostal/<version>/` and `indexes/<id>/`. Every directory is sealed by `MANIFEST` (paths, sizes, SHA-256) and then `_COMPLETE`; readers use nothing unsealed.

The same commands work without Kubernetes, against a bucket or a local directory: `build_index job --store file:///srv/geocoding --index NAME --builder-hash "$(build_index hash)" --sources sources.json --scratch /tmp/scratch --models /data/libpostal`, where `sources.json` holds `index.sources` as JSON, then `build_index pull --store file:///srv/geocoding --index NAME --to /srv/index --models /data/libpostal`. A local directory store hard-links the index and models into place instead of copying them when they share a file system, so nothing may write those files in place; `pull` still checks every file against `MANIFEST`.

Several stores can share one copy of the sources. `build_index fetch --store URL --sources sources.json --scratch DIR` only fetches the sources into a store, as `job` does before it builds. Given `--sources-mirror URL`, `job` and `fetch` take every source directory sealed there and missing from `--store` from that store, read-only, and fetch the rest from upstream into `--store`. So a CI run whose sources a trusted `fetch` has already put into the shared bucket needs only read access to it.

The leader has unit tests for its build decisions and has not yet run against a cluster. The earlier chart, with a Job and an init container that waited, was tested on a single-node kind cluster with SeaweedFS, with a Liechtenstein box and the `liechtenstein` OSM extract. The first build took 3 minutes, including libpostal's 2 GB of models from GitHub, and produced a 4.7 MB index. Two pods pulled it and answered house-level queries. Changing the sources built a new index from the bucket in under 20 seconds and rolled the pods one at a time. A client polling once a second through the rollout got 150 answers out of 150, after a 10-second `preStop` delay was added; before it, 3 of 240 requests failed. A world build has not yet been run through the Job; the world numbers above come from building by hand.

## Configuration

| Variable | Default |
| --- | --- |
| `INDEX_DIR` | Required; a `build_index` output directory, read-only |
| `RPC_PORT` / `METRICS_PORT` | `11121` / `8000` |
| `PARSE_CACHE` | `65536`; address parses kept across requests (0 keeps none). A request parses each text once either way; libpostal's lock is shared, so a repeated query skips it |
| `RUST_LOG` | `info` |
| `SENTRY_DSN` / `ENV` | Optional |

For temporary SearchAddress diagnostics, set `RUST_LOG=info,opengeocoding::address_log=debug` (or add `RUST_LOG` to the chart's `serving.env` list). This logs the input (at most 256 characters, with its original byte length), parsed components, candidate-selection tiers, the final response (including coordinates and `good_match`), RPC status and elapsed milliseconds. Events share a process-local `request_id` within a `SearchAddress` span, including on the blocking resolver thread; include the pod identity when correlating across replicas. Candidate selections are intermediate decisions; the `SearchAddress result` event is the final enriched answer. Invalid input and errors have a completion event but no result. Debug formatting escapes control characters.

**Address diagnostics contain potentially personal data.** They are off at the default `info` level; broad `debug`/`trace` filters also enable them. Use only for a bounded debugging window with restricted log access and short retention, then restore `RUST_LOG=info`. Do not publish raw samples in issues or pull requests. Request domain and organization metadata are not included in these diagnostic events.

The service loads the index into its own process at startup: one process parses, looks addresses up and serves the RPCs, with no internal HTTP hop. Startup fails if `INDEX_DIR` holds no `build_index` output. The index files stay memory-mapped; the place table (about 2 GB for the world) and libpostal's models (about 2 GB) are read into memory, which took about 10 seconds from a warm disk. `INDEX_URL`, `NOMINATIM_HOST`, `NOMINATIM_URL` and `NOMINATIM_AUTHORITY` are no longer supported; there is no Nominatim fallback. Legacy RPC paths and metric prefixes remain for compatibility, not backend selection.

Overture's `region` column carries an ISO 3166-2 code for its regions only. Subdivisions it does not code (Ugandan, Czech and Nauruan districts, Palestinian governorates, overseas territories such as `US-PR` and `FR-971`; 180 in all) are named in `data/reference.json` under `iso3166_2`, by the Wikidata item Overture gives the division. The list of codes is a policy kept in that file, one entry per subdivision that consumers expect; `update_reference` refreshes only their items. The builder writes each place's Wikidata item as the last `places.tsv` column and the service gives the matching place that code: it becomes a region row and the address's `ISO3166-2-lvl4` code, ahead of the code Overture gives the places inside it (`UG-108` rather than `UG-C`). A code Overture supplies itself wins, and so does a coded region inside the coded place: Wallis and Futuna is `FR-WF`, but a place in Uvea carries Uvea's `WF-UV` (Overture's code for the district, with its polygon), not `FR-WF`. Still without regions: Cyprus, Monaco, Madagascar and Singapore (Overture has no divisions with those codes' Wikidata items) and 29 Vietnamese provinces that Overture does not carry. Regions come from the index, not from a database: at startup the service builds one row per ISO 3166-2 code the index names a region or subdivision for (about 4,600 for the world), the same row a search for `<cc>, <code>` returns, and serves them from `ListRegions` (`RegionsResponse`, ordered by country and admin code). The rows are fixed for the life of the process and need no writable directory. A place also carries the subdivision below its region when it lies in a county that Wikidata gives an ISO 3166-2 code: `ISO3166-2-lvl6` (French départements `FR-68`, Italian and Spanish provinces `IT-MI`, `ES-BI`, Belgian provinces `BE-WLX`, Czech districts `CZ-806`, Lithuanian municipalities, Philippine provinces, Bangladeshi and Moroccan districts, 854 codes in all, from the `iso3166_2_lvl6` list in `data/reference.json`, which is kept and refreshed like `iso3166_2`). `ResolveRegion` and `addressDetails.regions` then return the region (level 4) first and the subdivision (level 6) after it, and `ListRegions` holds both, the subdivision's row naming its region in `parent_admin_code` (`FR-GES`). The two lists differ: `iso3166_2` replaces a place's level-4 code (a Ugandan district stands for the region), `iso3166_2_lvl6` adds a second one. Only counties are matched, so an address in a place whose parents skip the county, or in a county Overture does not carry, has a region only; in one comparison with Nominatim 5.x, 114 of 500 `ResolveRegion` points returned extra codes there, and at the six points of the issue that prompted this, this service now returns each extra code (`FR-68`, `FR-75C`, `IT-MI`, `ES-BI`, `BE-WLX`, `CZ-806`) but Alsace's `FR-6AE`, a collectivity that no county stands for. The index cannot give levels 3, 7 or 8, which Nominatim sometimes returns, nor level 5 outside Ireland. Overture's Irish regions are counties, so an Irish address also carries its province (`ISO3166-2-lvl5`, `IE-L`), and the service makes a place for each of the four provinces at startup from its counties (at the mean of their points, named in English and Irish): it has a region row at level 5, and a search for `ie, <code>` or for its name with Ireland stated ("Leinster, Ireland", "Laighin, Ireland") finds it. It is in no other place's chain, so a name without the country ("Leinster") or under another country ("Ulster, United Kingdom") does not, and a place's stated province ("Ramelton, Ulster, Ireland") is not checked against it. SuggestAddress does not offer the provinces, and `ResolveRegion` at a point of Ulster in Northern Ireland answers `GB-NIR`: `IE-U` covers only the province's three counties in Ireland. A search for `<cc>, <code>` finds the subdivision's place, as it does a region's (`tests/resolve_region.rs`, with `INDEX_DIR`, checks every listed code does, and the issue's points). The subdivision codes take no part in the stated-state checks: `ES-MA` (Málaga) does not make "MA" agree with a Spanish candidate. A region code an address carries but the index has no region for is logged and left out of the answer. Earlier versions kept rows in ClickHouse (`CLICKHOUSE_*`, `REGION_MAP_PATH`); those settings are ignored.

`/healthz` reports readiness after initialization. Metrics are exposed at `/metrics` on port 8000.

Coordinates use protobuf float32. Postcode compatibility deliberately includes shared outward/area codes for supported national schemes; it is not a promise of an exact street-level postcode match.
