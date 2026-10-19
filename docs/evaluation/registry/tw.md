# Taiwan — Taipei City

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/tw.NOTICE)

This municipal diagnostic corpus uses Taipei's official address-point CSV, not a national register. The national catalogue explicitly licenses it under Government Data Open Licence v1. The raw capture is monthly data; retain its checksum and the source metadata beside it. It includes house/floor strings but no people or household records.

Offline preparation projects the metre coordinates from EPSG:3826 to WGS84 with DuckDB spatial / PROJ, always_xy=true. The download's bare X/Y headers do not themselves identify the CRS; the transform is a documented interpretation, checked on the published City Hall address point (市府路1號: 306891.63355369843,2770052.1507132701 → 121.56378381506995,25.037523001993364). This is not survey-grade calibration or a claim of unit-specific coordinate precision.

Eligibility requires a known Taipei district, a street or area, a numeric house number optionally followed by 之 and a number, and a valid local point. Recognized numeric/Chinese floor suffixes end in 樓; other numbering shapes remain outside this frame rather than being guessed. Building keys hash district, street, area, lane, alley and house number with ASCII digits and NUL separators. Buildings whose source rows disagree on their point are excluded before the draw. Native and ASCII-digit forms, plus an original floor suffix at that same point when present, share one location and hold-out key. District names expand official administrative codes, not geocoder output.

The draw balances the 12 districts and suffix/alley/lane/section/plain classes, with at most four different street/area/lane/alley combinations per pool. Classes have precedence in that order; tags and source fields preserve combinations. All cases require a house-level, goodMatch answer in Taiwan within 150 m and the source building number. Floors are not scored as verified units. No romanized address, postcode, national recall or entrance-precision claim is made.

```sh
curl -fL -o SOURCE/taipei.csv 'https://data.taipei/api/frontstage/tpeod/dataset/resource.download?rid=ce76ca0c-7f94-4935-ab47-1d2a41ca2abb'
(cd SOURCE && duckdb -f REPO/docs/taipei-csv.sql)
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

All 240 native building strings and 240 ASCII-digit controls abstain. The 142 published floor forms have one pass, 69 abstentions, 21 too-coarse results and **51 far misses**. Every critical query was parsed and compared with its source-backed building-only control: libpostal isolates the house and floor, but the consumer returns Taiwan's country centroid, 127–138 km away. Fifty raw full-query lookups return only Taiwan; one has no raw candidate. This is attributed to loss of the supplied Taipei hierarchy in matching/fallback, not to failure to parse the floor or proof that the houses are missing. tw.labels.jsonl records each observed case. No tolerance or expectation was changed and no failure was removed. Other failures remain unlabelled.

SuggestAddress finds none of the 622 complete strings. ReverseGeocode passes the country check at all 622 repeated source points, without validating the building, district or unit. ResolveRegion is unscored. The single forward pass is not evidence of general Taiwanese address support.

| File | SHA256 |
|---|---|
| taipei.csv (124,662,109 bytes) | 7212058161f10a13d52ee46e74155526ca4ebf182b151562ba0e533f882a89ab |
| taipei-4326-2026-10-19.csv (166,853,274 bytes) | 0f3caa9072aa441740d17584313b2f684e657e80fcd49a690f9163a474ef4f6c |

## Validation

These are measurements on the documented last-known-good local world index (builder 4fb1fd818b46b2f6, fingerprint e6df26bed27b), not a successful rebuild with the current builder. The unchanged main runtime, identical models and reference data were used on both sides. Two full 11,057-case gold runs have zero response/scoring drift across all four RPCs, including all 8,939 pre-existing cases. There are no newly failing or worse cases. Re-scoring after the triage labels changed zero classes; the labels acknowledge failures rather than passing them. Local absolute gates pass with those labels.

The speed comparator reports no slowdown, but the second identical-binary run was substantially faster on this non-isolated host; no speedup or production-latency claim follows. The documented fresh-service grpcurl hang remains a known failure even though these runs happened to pass that check. Human review of source points remains pending.

Run the offline checks with cargo test --locked and python3 docs/test-registry-exports.py. Run one country with evaluate run --corpus tests/registry/CC.jsonl --index DIR --out RUN_DIR. This source-only addition changes neither resolver behavior nor the builder hash.
