# Taiwan — Taipei City

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/tw.NOTICE)

This municipal diagnostic corpus uses Taipei's official address-point CSV, not a national register. The national catalogue explicitly licenses it under Government Data Open Licence v1. The raw capture is monthly data; retain its checksum and the source metadata beside it. It includes house/floor strings but no people or household records.

Offline preparation projects the metre coordinates from EPSG:3826 to WGS84 with DuckDB spatial / PROJ, always_xy=true. The download's bare X/Y headers do not themselves identify the CRS; the transform is a documented interpretation, checked on the published City Hall address point (市府路1號: 306891.63355369843,2770052.1507132701 → 121.56378381506995,25.037523001993364). This is not survey-grade calibration or a claim of unit-specific coordinate precision.

Eligibility requires a known Taipei district, a street or area, a numeric house number optionally followed by 之 and a number, and a valid local point. Recognized numeric/Chinese floor suffixes end in 樓; other numbering shapes remain outside this frame rather than being guessed. Building keys hash district, street, area, lane, alley and house number with ASCII digits and NUL separators. Buildings whose source rows disagree on their point are excluded before the draw. Native and ASCII-digit forms, plus an original floor suffix at that same point when present, share one location and hold-out key. District names expand official administrative codes, not geocoder output.

The draw balances the 12 districts and suffix/alley/lane/section/plain classes, with at most four different street/area/lane/alley combinations per pool. Classes have precedence in that order; tags and source fields preserve combinations. All cases require a house-level, goodMatch answer in Taiwan within 150 m and the source building number. Floors are not scored as verified units. No romanized address, postcode, national recall or entrance-precision claim is made.

```sh
curl -fL -o SOURCE/taipei.csv 'https://data.taipei/api/frontstage/tpeod/dataset/resource.download?rid=ce76ca0c-7f94-4935-ab47-1d2a41ca2abb'
(cd SOURCE && duckdb -f REPO/tests/registry/taipei-csv.sql)
target/release/evaluate sample-tw --taipei SOURCE/taipei-4326-2026-10-19.csv --seed tw-2026-10 --out tests/registry/tw.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/tw.seeds.jsonl --out tests/registry/tw.jsonl --taipei SOURCE/taipei-4326-2026-10-19.csv
```

## Pinned capture and local baseline

The raw CSV has **1,157,763 rows**. The declared number/coordinate rules and exclusion of conflicting-point building groups leave **251,108 eligible keys**. The sample contains **240 locations and 622 queries**, including 43 held-out locations. Each of the five precedence classes has 48 locations.

| Pattern class | Queries | SearchAddress passes |
|---|---:|---:|
| Alley | 127 | 0 |
| Lane | 127 | 0 |
| Plain | 131 | 1 |
| Section | 123 | 0 |
| House suffix | 114 | 0 |

All 240 native building strings and 240 ASCII-digit controls abstain. The 142 published floor forms have one pass, 69 abstentions, 21 too-coarse results and **51 far misses**. Every critical query was parsed and compared with its source-backed building-only control: libpostal isolates the house and floor, but the consumer returns Taiwan's country centroid, 127–138 km away. Fifty raw full-query lookups return only Taiwan; one has no raw candidate. This is attributed to loss of the supplied Taipei hierarchy in matching/fallback, not to failure to parse the floor or proof that the houses are missing. The original 51 critical cases were recorded in tw.labels.jsonl. The introduction changed no tolerance or expectation and retained every failure; verified label changes after the runtime repair are described below. Other failures remain unlabelled.

At this baseline, SuggestAddress found none of the 622 complete strings. ReverseGeocode passed the country check at all 622 repeated source points, without validating the building, district or unit. ResolveRegion is unscored. The single forward pass was not evidence of general Taiwanese address support.

| File | SHA256 |
|---|---|
| taipei.csv (124,662,109 bytes) | 7212058161f10a13d52ee46e74155526ca4ebf182b151562ba0e533f882a89ab |
| taipei-4326-2026-10-19.csv (166,853,274 bytes) | 0f3caa9072aa441740d17584313b2f684e657e80fcd49a690f9163a474ef4f6c |

## Validation

These are measurements on the documented last-known-good local world index (builder 4fb1fd818b46b2f6, fingerprint e6df26bed27b), not a successful rebuild with the current builder. The unchanged main runtime, identical models and reference data were used on both sides. Two full 11,057-case gold runs have zero response/scoring drift across all four RPCs, including all 8,939 pre-existing cases. There are no newly failing or worse cases. Re-scoring after the triage labels changed zero classes; the labels acknowledge failures rather than passing them. Local absolute gates pass with those labels.

The speed comparator reports no slowdown, but the second identical-binary run was substantially faster on this non-isolated host; no speedup or production-latency claim follows. The documented fresh-service grpcurl hang remains a known failure even though these runs happened to pass that check. Human review of source points remains pending.

Run the offline checks with cargo test --locked and python3 tests/registry/test-registry-exports.py. Run one country with evaluate run --corpus tests/registry/CC.jsonl --index DIR --out RUN_DIR. The corpus introduction was source-only; the query-time follow-ups below do not change the builder hash.

## Runtime follow-ups

[The forward-search repair (#201)](https://github.com/Mokto/opengeocoding/pull/201) resolves 506/622 queries locally and removes all 51 country-centroid answers: 45 now pass and six return an unconfident street fallback. Its expanded full-index CI comparison confirms 505 additional forward passes, no new or worse failures, and passing measured speed gates. These remain municipal diagnostic results, not national accuracy.

The next query-time repair routes complete native building strings through that same exact road/house hierarchy for SuggestAddress, without libpostal. It preserves recognized floors as unverified suffixes on the building's unchanged retrieval ID and point, rather than claiming a verified floor or apartment. Unrecognized prefixes, extra qualifiers and number ranges are not discarded to force a house match. Empty normalized place prefixes return no candidates instead of panicking.

On the same last-known-good local index, with identical evaluator, models and reference data:

| Pattern class | Queries | SearchAddress after #201 | SuggestAddress candidate |
|---|---:|---:|---:|
| Alley | 127 | 127 | 127 |
| Lane | 127 | 127 | 127 |
| Plain | 131 | 129 | 128 |
| Section | 123 | 123 | 123 |
| House suffix | 114 | 0 | 0 |
| Total | 622 | 506 | 505 |

Before this suggestion repair, all 622 suggestion checks failed. The candidate passes 191/240 native building strings, 191/240 ASCII-digit strings and 123/142 published floor strings, all at rank one. Every non-held-out, non-suffix query passes. Three plain held-out suggestion queries remain unresolved; two also abstain in SearchAddress. They were not inspected or used to choose the repair. The early typing of an unspaced native address before its complete `號` form remains outside this repair.

The full 11,057-case local comparison has 505 newly passing suggestion checks, with zero SearchAddress, ReverseGeocode or ResolveRegion response changes and no newly failing or worse cases. A fresh baseline rerun has zero answer/scoring drift. The latest local measured-speed comparator passes, but timings also shift substantially for unchanged RPCs on this non-isolated host; this is not a speedup or production-latency claim. Full-index CI must independently pass quality and measured speed for the suggestion change.

### Label reconciliation and remaining coverage

The official raw capture was downloaded again and matched its pinned checksum. The documented projection produced the byte-identical pinned WGS84 CSV; regenerating the unchanged seeds and labels first reproduced the entire corpus byte for byte. The 45 labels whose SearchAddress fixes were verified by #201 were then removed, and the six remaining labels re-attributed to the observed indexed-key gap. Rebuilding through `evaluate build-gold` changes only those labels: every query, source field, expectation, tolerance and hold-out group is unchanged. Re-scoring the saved main run changes zero classes.

For all 38 non-held-out suffix-building groups, a 1 km source-point inspection found the exact road and base house number but neither the requested `之` key nor its hyphen spelling on the inspected matching street segments. None is borrowed from its base house. The six remaining labelled floor cases belong to these inspected groups; their current forward failure is `too_coarse`, not a country-centroid miss. This bounded evidence does not prove upstream or global index absence. Any source/import work is a separate experiment and must preserve the corpus's authoritative expectations.

The other 10 suffix location groups remain evaluation-only. No new failure labels, source import, tolerance increase, case deletion or evaluator retry was used. The authored synthetic checks cover exact numbers and hierarchy, country conflicts, full-width forms, missing suffixes, rejected ranges/prefixes, and unchanged building retrieval with unverified floors. The model-backed RPC suite retains the previously reproduced CDG landmark fixture failure on this older index; the documented grpcurl SearchAddress hang remains a known failure.
