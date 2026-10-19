# Performance methodology and Nominatim comparison

[Documentation](../README.md) · [Build prerequisites](../guides/development.md#build-and-test) · [Evaluation](../evaluation/README.md)

Run commands from the repository root.

The README combines a historical world-index build with a measured query benchmark of deployed release 0.14.0. The build and request figures come from different machines and runs; they must not be presented as one combined deployment measurement.

## World-index build

The [landmark build record](../guides/api.md#landmarks) reports a 2026-10-17 build taking **409 seconds**, with a **32.2 GB peak memory footprint**, on an **18-core Apple M5 Max with 64 GB RAM**. The resulting world index was roughly **16 GB**, including administrative polygons, street-reverse files, GeoNames places and landmarks. Source inputs included Overture, OpenAddresses, GeoNames and OSM extracts, including landmark-only extracts.

This times index construction from local sources, not first installation: source downloads, libpostal model provisioning, application compilation, artifact upload and deployment are outside the figure. The index size excludes models, source mirrors and build scratch. It is a single observed build, not an isolated before/after experiment or a memory ceiling.

## Forward-search throughput, p95 and memory

On **2026-10-19**, release **0.14.0** was benchmarked against **one deployed Kubernetes pod**. The [aggregate measurement record](benchmarks/2026-10-19-search.json) preserves all three client summaries, software/image identifiers, corpus hash and resource observations. It contains no addresses or private endpoints.

| Run | Measured requests | Queries/s | p95 | p99 | Errors |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 20,000 | 2,675.0 | 18.45 ms | 34.69 ms | 0 |
| 2 | 20,000 | 2,656.2 | 19.27 ms | 35.87 ms | 0 |
| 3 | 20,000 | 1,773.9 | 29.86 ms | 60.62 ms | 0 |

The README reports the range across all three runs, including the slower third run. Percentiles are computed per run, not pooled across runs.

**Server and resources.** The host CPU was an **Intel Core i5-13500**, with **14 physical cores and 20 logical CPUs**, on a shared node. The serving container could use CPUs 0–19, requested **1 CPU and 6 GiB memory**, had **no CPU quota**, and had a **16 GiB memory limit**. Cgroup CPU counters showed **4.09–5.03 CPU cores on average over each invocation**, including preliminary queries and warm-up. This is not throughput on one reserved or capped CPU.

Process RSS sampled roughly every 1–2 seconds peaked at **6.29–6.30 GiB** during the runs. Container-charged memory peaked at **5.23–5.24 GiB**. Process RSS and cgroup-charged memory use different accounting for mapped pages and must not be substituted for one another. No server CPU throttling was recorded. The client used about 0.10–0.12 CPU cores over its invocations, with no CPU throttling; it was on a different node with a 4-CPU limit.

**Workload and transport.** The existing `src/bin/benchmark.rs` was compiled unchanged as a standalone client using the direct dependency versions recorded in the JSON. Each run used **16 concurrent workers** and **20,000 measured requests**, cycling through the **1,000 queries** in the `query` column of `tests/addresses.tsv`. Each invocation first made one preliminary request per query, then 160 connection-warming requests, outside the timed batch. The deployed parse cache was enabled at its default 65,536 entries; no page-cache reset was performed. This is a warm, repeating-corpus measurement, not unique-query or cold-cache performance.

The client called the selected pod IP directly, over Connect JSON/HTTP keep-alive from another node in the same namespace. Ingress, TLS, authentication and the public API's metering proxy were bypassed. The target was image `ghcr.io/mokto/opengeocoding:0.14.0`, with builder `a7481f0be4727233`, index `2026-09-23.1-2026-10-ff34388b4830` and model version `v1`; the JSON records the immutable image digest. No deployment, cache or resource settings were changed.

To repeat the request phase with the extracted one-query-per-line corpus and a reachable single pod:

```sh
for round in 1 2 3; do
  target/release/benchmark --queries queries.txt \
    --service "pod=http://${POD_IP:?Set a single pod IP}:11121" \
    --requests 20000 --rounds 1 --concurrency 16
done
```

Run the client on a different node and verify that namespace network policies permit access. The corpus and client source hashes in the JSON identify the exact inputs; use those revisions when reproducing the historical run.

**Limits.** There were **60,000 measured requests with zero HTTP, transport or JSON-decoding errors**, no retries, no pod restarts, and successful readiness checks after every round. Requests returning valid JSON are not necessarily geographically correct; no accuracy evaluation was performed. The timed batches lasted about 7.5–11.3 seconds each on a live instance on a shared node; this is not a sustained-capacity test or an isolated explanation of the third-run slowdown. Preliminary connectivity probes were excluded from the measurements. Raw per-request latency samples were not retained. The temporary client pod was removed afterwards.

The older 2,850 queries/s result and its incomplete hardware/latency record remain in the [lookup optimization history](../notes/lookup-history.md#lookups), not the README headline. That older run used a different index, an 808-query corpus and disabled parse caching; it cannot be used as a speed comparison with this run.

## What the Nominatim figures mean

Source: [Nominatim 5.3 installation prerequisites](https://nominatim.org/release-docs/5.3/admin/Installation/#hardware), checked 2026-10-19. For a planet import it strongly recommends **128 GB RAM or more**; for a planet installation it requires **at least 1 TB disk**, preferably NVMe. It describes a planet import taking **around 2.5 days** on a well-configured machine, or **4–5 days** on traditional SSDs.

These are Nominatim's published planning figures, not measurements from the OpenGeocoding benchmark machine. Nominatim imports an OSM planet into PostgreSQL/PostGIS; OpenGeocoding builds a different, compact serving index from already-processed geographic inputs. Neither the data coverage nor the import work is equivalent. Import-memory recommendations cannot be compared with measured serving-process RSS as a RAM-saving ratio, and a final index size is not comparable to total installation/scratch requirements as a disk-saving ratio.

There is no matched Nominatim throughput, request-p95 or serving-memory result in this record. A numeric speedup or resource-saving percentage would require both services on identified hardware, documented source snapshots and coverage, the same queries and concurrency, explicit cache/warm-up settings, request/error counts, latency distributions, and separate build/serving memory and disk accounting. Measure both under the same protocol or clearly account for adapter overhead, and evaluate geographic correctness separately. The benchmark below speaks Connect JSON; it cannot call a stock Nominatim HTTP endpoint without an adapter.

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

`queries.txt` holds one address per line, for example the `query` column of `tests/addresses.tsv`. A service URL without a route uses `opengeocoding.OpenGeocoding`; to benchmark old clients, use `--service legacy=http://127.0.0.1:18121/geolocation.Geolocation` or `.../nominatim_geolocation.NominatimGeolocation`. All three routes use the same resolver. Results include the client, HTTP transport, parsing, index lookups and the RPC framework on one machine; they are not production throughput or accuracy guarantees.
