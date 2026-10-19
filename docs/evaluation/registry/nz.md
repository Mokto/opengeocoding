# New Zealand

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/nz.NOTICE)

The source is LINZ's authoritative NZ Addresses, CC BY 4.0, accessed through its official public ArcGIS distribution. No API key, postal directory or commercial geocoder is used. The exporter retains source pages, service metadata and checksums outside the repository. Source postcodes and delivery-route qualifiers are not supplied or tested.

The exporter first obtains every current source OBJECTID in disjoint ranges, rejects truncated frames, and checks the total against the source count. It keeps the smallest 10,000 seeded SHA256 hashes of those IDs and fetches the original records with geometry in EPSG:4326. The subsequent Rust draw balances territorial authorities and number-pattern classes, taking at most two different roads per pool. This two-stage, pattern-balanced diagnostic sample is not population-weighted national accuracy. Its source OBJECTID draw is specific to the pinned service snapshot; references and hold-out keys use registry address/road information instead.

A building key hashes road ID and the structured building number (NUL separated); choose the lowest address ID per key. All native/ASCII forms stay together. The original LINZ ASCII string is used, not a generated romanization. Complete ranges remain ranges; endpoints are not expanded into houses. Unit queries accept the published full unit/number or its structured building number at the source point within 150 m; this measures building/geographic recovery, not apartment identity. Distinct properties can share a base number, especially on rural access roads, so the key is a conservative hold-out group, not a claim that all units are one physical building. The point belongs to the selected registry row. No surveyed-entrance precision is claimed.

```sh
python3 -u tests/registry/export-linz-addresses.py --out SOURCE/linz-2026-10-19 --seed nz-2026-10
target/release/evaluate sample-nz --linz SOURCE/linz-2026-10-19/linz.json --seed nz-2026-10 --out tests/registry/nz.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/nz.seeds.jsonl --out tests/registry/nz.jsonl --linz SOURCE/linz-2026-10-19/linz.json
```

Raw pages are checkpointed. An interrupted acquisition must retain the same service snapshot; if it changes, start a new dated directory. An existing final snapshot is never overwritten with different content. Shared LINZ ancestry in index inputs limits independence of coordinate validation.

## Pinned capture and local baseline

The capture's complete current-record frame has **2,430,755 addresses**. Its 10,000 seeded candidates produce **428 sampled location groups and 545 queries**, with 74 held-out groups. Pattern and authority balancing, plus the initial candidate pool, limit conclusions about national accuracy and rare formats. Rural delivery-route qualifiers remain untested; a high house number alone is not a rural label.

| Pattern class | Locations | Queries | SearchAddress passes |
|---|---:|---:|---:|
| Plain | 129 | 129 | 111 |
| House suffix | 124 | 146 | 114 |
| Unit | 82 | 94 | 16 |
| Native/ASCII difference | 83 | 166 | 104 |
| Number range | 10 | 10 | 8 |

Across all classes, native strings pass 275/428 and official ASCII variants 78/117. There are 178 too-coarse results, 13 near misses and one abstention; no critical result is labelled. Missing houses, source ancestry and parser/matching failures have not been isolated for every noncritical failure. SuggestAddress finds an acceptable full-string result for 226/545 queries. All 545 ReverseGeocode country checks pass, including number alternatives sharing a point. These do not verify building/unit identity. ResolveRegion is unscored.

The pinned linz.json is 8,000,474 bytes, SHA256 1571afbe20e9c34f40ed7f6f494fdd01ff6223492b27b6bb7b254ad982ddc178. Its embedded sources list records every raw ID/frame/page file, request and checksum. Retain that entire directory. Editing timestamps in the layer metadata are checked before/after acquisition and against checkpoints; a truncated one-million-ID response is not accepted as the national frame.

## Validation

These are measurements on the documented last-known-good local world index (builder 4fb1fd818b46b2f6, fingerprint e6df26bed27b), not a successful rebuild with the current builder. The unchanged main runtime, identical models and reference data were used on both sides. Two full 11,057-case gold runs have zero response/scoring drift across all four RPCs, including all 8,939 pre-existing cases. There are no newly failing or worse cases. Re-scoring after the triage labels changed zero classes; the labels acknowledge failures rather than passing them. Local absolute gates pass with those labels.

The speed comparator reports no slowdown, but the second identical-binary run was substantially faster on this non-isolated host; no speedup or production-latency claim follows. The documented fresh-service grpcurl hang remains a known failure even though these runs happened to pass that check. Human review of source points remains pending.

Run the offline checks with cargo test --locked and python3 tests/registry/test-registry-exports.py. Run one country with evaluate run --corpus tests/registry/CC.jsonl --index DIR --out RUN_DIR. This source-only addition changes neither resolver behavior nor the builder hash.
