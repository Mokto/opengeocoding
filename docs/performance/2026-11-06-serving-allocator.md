# Serving allocator, 2026-11-06

## Scope and inputs

The baseline is `da5b321`, after #554. Only the serving executable's Rust global allocator changes, from the system allocator to the already-installed `mimalloc` dependency used by the builder. Native libpostal allocations, parsing, matching, ranking, concurrency, caches, wire contracts, reference data and index format are unchanged. There is no new dependency or builder hash.

An owned fresh-main worktree and target built an owned full index with builder `7c97127b13cc31f0`, using the existing local Overture `2026-09-23.1`, OpenAddresses, OSM, landmark, Korean-shop and GeoNames inputs. These are not CI's newer source snapshot. All 296 index files (17,121,744,470 bytes) were SHA-256 inventoried; all hashes and 5,847 source file size/mtime records remained unchanged after comparison. The shared local index was stale and was neither reused nor replaced.

Both sides use the same evaluator, corpus, models and reference data:

- Corpus: 46,320 cases, SHA-256 `98f7adb64387d0cab28cef0a4f78b082a5800d04635e65ef3c87f86e461c38a2`.
- Baseline service: SHA-256 `c4939b13f2a9701a8939e9308de5f15c1846b9d2dc524037052bd9f8366a6c20`.
- Candidate service: SHA-256 `3ba62742bc8951599ebf1aeeaee0e264c7e9c42996ee27f637c2814a600dd429`.
- libpostal models: SHA-256 `65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7`.
- Reference data: SHA-256 `9a68f034f3b05f57aecdec9776b06c63a15cf48619703a6fd797bb61ce7cfb01`.

Heavy commands use the local Cargo queue, niceness 15 and 12 Rayon threads. Gold retains its defaults: parse cache disabled, 2,000 hash-selected timed cases, fastest of three requests, then untimed correctness checks. Startup and suggestion warm-up are excluded from request timings. These are ARM64 macOS laptop measurements, not Linux request-latency or throughput guarantees.

## Attribution and rejected experiment

The unresolved SearchAddress speed failures of #549 and #554 are not dismissed as noise. #554 improved reverse lookup but failed SearchAddress p90 at +18%; its head ran immediately after a full index build, before the base. Declining node load/I/O pressure is an attribution lead, not proof of an environmental cause. #550's earlier pass did not cover its final rebased builder. This experiment establishes another fresh-main baseline; it does not retroactively certify those merges.

Three initial unchanged-binary gold runs give SearchAddress totals of 891.0 / 957.9 / 922.5 ms, p90 of 0.857 / 0.940 / 0.872 ms and p99 of 4.154 / 3.956 / 4.137 ms. Suggest totals are 414.5 / 453.6 / 427.7 ms; reverse totals are 240.6 / 268.3 / 249.1 ms. Small sequential deltas are not enough to attribute an optimization.

Separate 20-second `sample` profiles exercised each RPC after warm-up, using seeded training requests and training slow tails, never held-out requests for tuning. Search spends substantial time in postings and native parsing, but also allocation/freeing: the baseline's collapsed exclusive samples include 625 in `_xzm_free`, 242 in `_xzm_xzone_malloc` and 152 in `_malloc_zone_malloc`. The candidate has 129 in `mi_free`, 100 in `mi_malloc_aligned` and 13 in `_xzm_free`. Samples are attribution evidence, not normalized CPU benchmarks.

A diagnostic found 100 repeated raw native parses among 1,169 calls over 274 training requests. Request-local raw-parse memoization was tried and reverted: three interleaved comparisons showed under 1% total improvement and no consistent tail benefit. It is not part of the change.

## Request measurements

Two warmed services receive each of the same 274 training requests in ABBA order, with one warm-up request and 12 measured requests per side per case. The table uses each case's median, not a selectively chosen fastest run. All three process-pair repetitions are included:

| Pair | Sum of medians, base → candidate (ms) | p90 (ms) | p99 (ms) |
|---|---:|---:|---:|
| 1 | 250.09 → 226.72 | 3.075 → 2.681 | 7.264 → 6.569 |
| 2 | 250.85 → 227.70 | 3.109 → 2.830 | 7.217 → 6.563 |
| 3 | 249.77 → 228.70 | 2.997 → 2.734 | 7.201 → 6.596 |

All 21,372 responses in those repetitions are identical. The unchanged full gold workload was also run in candidate/base/base/candidate order. The adjacent base/candidate comparisons have Search totals 917.3 → 819.2 and 917.8 → 840.8 ms; Suggest totals 430.4 → 390.3 and 435.6 → 411.1 ms; reverse totals 251.4 → 236.5 and 254.3 → 248.6 ms. These local differences support the allocator experiment, but CI's unchanged 15% speed gate remains authoritative.

## Memory and startup trade-off

The allocator retains different amounts of memory. This is not a free speedup. Paired-service startup/readiness plus suggestion warm-up took 4.18–4.30 seconds on the baseline and 3.98–4.40 seconds on the candidate. The warmed paired processes usually showed about 5.15 GiB baseline RSS versus 5.99–6.02 GiB candidate RSS before requests.

During the full gold workload, 250 ms sampling observed peak service RSS of 11.76 / 11.79 GiB on the baseline and 12.05 / 12.05 GiB on the candidate. After the separate sequential complete-response replay, `vmmap` reported physical footprint 4.0G (peak 5.0G) versus 4.4G (peak 5.3G). RSS includes mapped index pages and is not interchangeable with private heap or physical footprint. No CPU or memory limits are raised; the chart's serving memory limit remains 16 GiB. Linux memory behavior still needs observation and these short runs establish no sustained-memory ceiling.

## Answer and regression checks

A separate sequential replay compares complete JSON responses, including IDs, ordering, components, coordinates, confidence and reverse distances:

| RPC | Training | Held out | Changed responses |
|---|---:|---:|---:|
| SearchAddress | 36,983 | 9,337 | 0 |
| SuggestAddress | 40,019 | 10,070 | 1 training |
| ReverseGeocode, including offsets | 66,715 | 16,946 | 0 |
| ResolveRegion | 36,505 | 9,214 | 0 |

The one changed response, `Riverside, CT`, is existing suggestion-tail nondeterminism: a baseline replay includes a Solomon Islands street while other starts of the same baseline return Connecticut streets. `regions_coded` traverses a randomized HashMap and street completion privileges its first region. Baseline-only gold repetitions also change tails for `Torrance, CA` and `La Plata, MD`; candidate starts vary too. No scoring or held-out result changes, and no unrelated determinism fix, expectation change or new failure label is included. The first two initial baseline runs have identical saved answers.

Full gold relative comparisons have no newly failing or worse-class checks. Absolute runs fail existing baseline checks. `cargo fmt --check`, strict all-target clippy, `cargo test --locked` and the separately invoked ignored model-backed synthetic resolver check pass. The actual baseline and candidate serving executables both fail the model-backed RPC suite on the documented grpcurl timeout and the existing address-log truncation assertion; neither failure is skipped or weakened. The runtime replay exercises the allocator-enabled binary, whereas library-only synthetic tests do not select that binary's allocator.

Current-head CI quality and measured speed must both pass before acceptance. Missing, skipped or unmeasured speed remains a blocker. Compilation, synthetic tests and these local timings do not establish production accuracy, native thread safety, gRPC interoperability or deployed performance.
