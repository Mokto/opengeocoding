# Hungary: BKK Budapest metro landmarks

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/hu.NOTICE)

**Data source: BKK Zrt., CC BY 4.0.** This corpus checks **metro-station landmarks in SuggestAddress**, not house addresses, KCR/EHA, transport platforms or entrances. It does not add Hungary-wide SearchAddress coverage or replace a national address register.

## Source and licence

BKK's official [OpenData source page](https://opendata.bkk.hu/data-sources) links its static GTFS feed and explicitly applies **CC BY 4.0** to the published datasets, permitting use, sharing and processing with the credit above. [The licence](https://creativecommons.org/licenses/by/4.0/) allows commercial and modified redistribution with attribution and disclosure of changes. This is different from public access without a grant: the static dataset is named on the licensed page. Static GTFS downloads require no API key; realtime services are not used.

The operator publishes stop names and GPS coordinates. That is the source of the expectation, not OSM, Wikidata or a geocoder response. The operator's coordinate-collection method and an error bound were not independently established. This is agreement with the operator's published station positions, not a survey-accuracy claim.

## Pinned capture

- Download: `https://go.bkk.hu/api/static/v1/public-gtfs/budapest_gtfs.zip`
- Captured **2026-10-31**; stored as `bkk-2826.20261031.zip`.
- **62,339,598 bytes**, SHA-256 `02fcf63a21e78e2e83f2998a20c41544edf78f60e225a58ca7e3098317fc85f3`.
- `feed_info.txt`: publisher BKK Zrt., Hungarian, version **2826.20261031**, service dates **2026-10-31 through 2026-12-01**.
- `stops.txt`: **6,305 coordinate-bearing records**, including 195 `location_type=1` parents. Those are not 6,305 independent stations: platforms, entrances and path nodes also occur.

The public URL changes every few days. Keep this capture and verify its checksum rather than silently substituting a fresh timetable.

```sh
set -euo pipefail
SOURCE=../gl-hu-data
mkdir -p "$SOURCE"
curl -fL https://go.bkk.hu/api/static/v1/public-gtfs/budapest_gtfs.zip -o "$SOURCE/bkk-2826.20261031.zip"
printf '%s  %s\n' 02fcf63a21e78e2e83f2998a20c41544edf78f60e225a58ca7e3098317fc85f3 "$SOURCE/bkk-2826.20261031.zip" | shasum -a 256 -c -
python3 tests/registry/bkk-metro-jsonl.py "$SOURCE/bkk-2826.20261031.zip" "$SOURCE/bkk-metro-2826.20261031.jsonl"
target/release/evaluate sample-hu-metro --bkk-metro "$SOURCE/bkk-metro-2826.20261031.jsonl" --seed hu-metro-2026-10 --out tests/registry/hu-metro.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/hu-metro.seeds.jsonl --out tests/registry/hu-metro.jsonl --bkk-metro "$SOURCE/bkk-metro-2826.20261031.jsonl"
target/release/evaluate run --corpus tests/registry/hu-metro.jsonl --index out --out eval-results/hu-metro
```

Preparation uses only Python's standard library, offline. It joins `routes.txt` (`route_type=1`, metro) to `trips.txt` and `stop_times.txt`, then takes each served platform's top-level `location_type=1` parent from `stops.txt`. Bus, tram, ferry, trolleybus and suburban rail routes (including type 109) are excluded; a GTFS parent alone does not prove it is a railway/metro station. Missing/duplicate stop ids, unsupported parent relationships and non-Hungarian/swapped coordinates fail rather than guessing. Eligibility means a station is scheduled somewhere in this feed's horizon, not that it is open in realtime.

The output has **48 parent stations**, **22,769 bytes**, SHA-256 `06f46f1c5640c5cf3bb61c0211c1cc56960d115c0d3dc98315320757c6182d1c`. Fields retain the original parent-stop record, published `[latitude, longitude]`, metro lines, feed version/dates and raw archive SHA-256. Coordinates are WGS84/EPSG:4326 by the GTFS specification, copied without geocoding or reprojection. The native names are retained without stripping accents or inventing romanizations.

## Selection and checks

The adapter requires a top-level parent, an M1–M4 service association and agreement between the numeric point and the original GTFS latitude/longitude strings. Duplicate parent ids fail. Same-name groups with different points are excluded; identical-point duplicates keep the lexicographically first id. This capture has **48 distinct, unambiguous names**: 11 M1, 11 M2, 20 M3 and 10 M4 associations, overlapping at interchanges.

The sampler's seeded order has a 64-station ceiling, so this capture includes **all 48** eligible stations: a metro census, not a random sample of Hungarian transport facilities or addresses. Each station has three legitimate contextual query forms, sharing a source-id location key and the common hold-out:

- `Mexikói út, Budapest, Hungary`
- `Budapest, Mexikói út, Hungary`
- `Mexikói út, Budapest, Magyarország`

There are **144 queries**, with **5 held-out stations / 15 queries**. Query forms, selection, top-five rank and tolerance were fixed before seeing service answers. Variants are not independent locations. There are no bus stops, suburban stations, other Hungarian cities or postcode/house expectations in this frame.

A case asks SuggestAddress for landmarks and requires a **station-category landmark among the first five suggestions within 300 m** of the published parent-station point, in Hungary. It repeats the same query with landmarks disabled, where no landmark may appear. Report flag-on success separately from flag-off correctness; combining them is not a station-retrieval accuracy rate. No SearchAddress, ReverseGeocode or ResolveRegion score is generated for these landmark-only cases.

The 300 m tolerance is a diagnostic station-area allowance, not a measured positional-error or entrance bound. The RPC does not expose GTFS ids, and the check does not assert an exact name/id: another station within the radius can pass. The parent coordinate is not a platform, boarding position, entrance or accessibility guarantee.

The index reads Hungarian OSM landmarks, not this BKK feed directly. OSM may reuse BKK transport data; shared ancestry was not ruled out. Selection is independent of the service, but this is not an independent real-world accuracy estimate. The national house-register access/licence blocker remains separate.

## Baseline

On a full index built by `dbec0f0cae254966` with the chart's source configuration, the flag-on station checks pass **92/144**:

| Query form | Passing | Checked |
|---|---:|---:|
| Station first, English country | 46 | 48 |
| City first | 0 | 48 |
| Station first, Hungarian country | 46 | 48 |

Held-out flag-on checks pass **8/15**. No station passes in all three forms. All **144/144 flag-off checks** pass (no landmark leaks with the flag disabled). The combined 236/288 check count is **not** a station-retrieval accuracy rate. SearchAddress is unscored and no reverse/region checks run for this corpus.

The city-first form remains unsupported in this draw, and two station-first names do not retrieve an acceptable station. A non-held-out example, Arany János utca, returns a street rather than the required landmark; the precise missing/indexing/name/ranking attribution remains unresolved. The held-out failing station was not inspected for tuning. All failing variants remain in the corpus without labels or adjusted tolerances.

A repeated run has zero suggestion/scoring drift. Neither run has flag-off failures or RPC errors, and the documented fresh-service grpcurl SearchAddress hang reproduces as a known failure. These are local diagnostics, not nationwide accuracy or speed evidence. No runtime landmark-ranking fix is included.
