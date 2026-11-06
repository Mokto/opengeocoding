# Reverse house candidate retention, 2026-11-06

## Scope and frozen inputs

The baseline is `3cf43f8`, after #549 and #550. Both sides use one locally built full index, builder `7c97127b13cc31f0`, from the existing Overture `2026-09-23.1`, OpenAddresses, OSM, landmark, Korean-shop and GeoNames inputs of the local build script. This is **not** the chart's newer `2026-10-21.0` source snapshot. No source, model, corpus, reference data or index-format change is part of this experiment.

The index occupies 17,108,581,219 bytes. Its files were SHA-256 inventoried; source paths, sizes and modification times were recorded separately. The evaluator records:

- Corpus: 46,320 cases, SHA-256 `98f7adb64387d0cab28cef0a4f78b082a5800d04635e65ef3c87f86e461c38a2`.
- Baseline service: SHA-256 `92dc54a830895ed477921c102c0424bf566631c2d6a196e1a3605a042d4dce9c`.
- libpostal models: SHA-256 `65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7`.
- Reference data: SHA-256 `9a68f034f3b05f57aecdec9776b06c63a15cf48619703a6fd797bb61ce7cfb01`.

Local runs use release builds, serialized libpostal, `PARSE_CACHE=0`, and the evaluator's unchanged defaults: 2,000 hash-selected timed cases, fastest of three requests. Timing excludes startup and suggestion warm-up. These laptop measurements support attribution, not deployed latency or throughput claims.

## Baseline variability and outstanding attribution

Three runs of the same baseline give these totals and p99s, in milliseconds:

| RPC | Total, runs 1 / 2 / 3 | p99, runs 1 / 2 / 3 |
|---|---|---|
| SearchAddress | 908.0 / 1021.3 / 945.8 | 4.081 / 4.466 / 3.971 |
| SuggestAddress | 418.9 / 458.0 / 442.4 | 1.062 / 1.153 / 1.160 |
| ReverseGeocode | 258.1 / 275.8 / 279.2 | 0.428 / 0.501 / 0.501 |

The unchanged-binary comparison itself fails reverse p99 at +17%. This does not invalidate or relax the CI gate.

#549's measured CI pair was `60d3be2` → `23045e8`; the latter directly descends from the former. Its SearchAddress p90 +25% and SuggestAddress p99 +22% remain failed measurements. An ABBA local replay of those original binaries, using the same current frozen index and corpus on both sides, gives SearchAddress p90 0.777 → 0.955 → 0.872 → 0.774 ms. That is consistent with added search cost, but does not reproduce the CI conditions or establish its exact attribution. SuggestAddress p99 is 1.036 → 1.180 → 1.088 → 1.083 ms, insufficient to distinguish its reported slowdown from local variability.

The added post-town and village retries are reachable only from SearchAddress. Its range spelling lookup also reaches SuggestAddress, after an exact house miss. ReverseGeocode cannot reach those #549 paths. Profiles of current main show the new search fallbacks, but range-alternative work is small in the sampled suggestion workload. No speculative change to those paths is included.

#550's earlier speed pass covers `9895ccf`, not its rebased merged head `35b44a8` or the final builder hash. This experiment supplies a current-main local baseline; it does not retroactively certify that merge.

## Profile and change

Separate 20-second macOS `sample` captures exercised SearchAddress, SuggestAddress and ReverseGeocode after warm-up. The fixed workload combines seeded training cases, training slow tails, British cases and partial prefixes. Held-out cases were not inspected or used to select requests.

Search samples concentrate in Tantivy lookups and libpostal, suggestion samples in Tantivy lookups, document decompression and qualification. Reverse samples concentrate in number-FST enumeration, candidate sorting and allocation/copying. The reverse stable-sort comparator alone accounts for 548 exclusive samples. Serialization is separately visible; startup readiness took roughly 5–7 seconds in these instrumented runs and is not an optimization result.

Previously each reverse grid ring collected every in-radius house, allocated every number, sorted the collection and discarded everything beyond the requested size. The change still reads every previously read cell, street and number. It retains the sorted prefix as candidates arrive, allocating a number only when it enters that prefix. Distance, far-copy slack, street-id/number ordering, stable equal-key order, radius checks and ring termination are unchanged.

There is no cache, parallelism, new dependency or persistent memory. Candidate storage falls from all houses of a ring to at most `size + 1` temporarily; RPC size is at most 10. Insertion moves O(size) entries, so a heap would be preferable if much larger result sizes were introduced. Startup work is unchanged.

## Local validation

For 262 fixed training reverse requests, each requested with sizes 1, 3 and 10, three alternating base/candidate pairs give:

| Result size | Base total range | Candidate total range | Median p99, base → candidate |
|---|---:|---:|---:|
| 1 | 41.30–41.96 ms | 36.94–43.56 ms | 0.905 → 0.784 ms |
| 3 | 45.99–46.07 ms | 41.93–49.14 ms | 0.958 → 0.795 ms |
| 10 | 61.95–62.30 ms | 58.99–65.74 ms | 1.190 → 1.051 ms |

All 2,358 paired complete reverse responses are identical. One candidate run was slower in total; all repetitions are retained rather than selecting the fastest run.

The full gold relative comparison has no newly failing or worse-class checks. A separate sequential replay compares complete JSON responses, including IDs, components, coordinates, confidence and reverse distances, rather than only scores:

| RPC | Training requests | Held-out requests | Changed responses |
|---|---:|---:|---:|
| SearchAddress | 36,983 | 9,337 | 0 |
| SuggestAddress | 40,019 | 10,070 | 1 training |
| ReverseGeocode, including house offsets | 66,715 | 16,946 | 0 |

The suggestion difference is the fifth result for the training query `Washington, PA`. Unchanged baseline starts already alternate this result between countries: `regions_coded` collects `*-PA` regions from a randomized HashMap, and street completion privileges the first region. The complete replay observed a Pennsylvania street instead of a Paysandú street; a baseline repeat also observed a Pará street. The first four results and scores are unchanged. This pre-existing nondeterminism is independent of reverse lookup and is not fixed here. Baseline runs 1 and 2 have identical saved answers; run 3 changes three suggestion tails.

Synthetic coverage checks the independently authored 16-house street's exhaustive sorted prefix for sizes 1–10, three query points and three radii. Existing tests also cover far copies and locality/street fallbacks. The absolute gold run fails on the baseline as well as the candidate; a clean relative comparison is not an accuracy claim. Model-backed RPC checks on both binaries reproduce the documented grpcurl timeout and the address-log truncation assertion failure; those failures are not skipped or weakened.

Current-head CI `gold-full` and **measured** `gold-speed`, on the cluster's own full source snapshot, remain the acceptance gate. No sampling, repetition, threshold, scorer or retry setting was changed.
