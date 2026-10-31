# Taiwan — Taipei and Tainan

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/tw.NOTICE)

Two municipal registers, not a national sample: Taipei (`tw.jsonl`, `sample-tw --taipei`) and [Tainan](#tainan-city) (`tw-tainan.jsonl`, `sample-tainan --tainan`). Both are included automatically in the nightly gold suite. Each retains its own seeds and preparation command.

## Taipei City

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

All 240 native building strings and 240 ASCII-digit controls abstain. The 142 published floor forms have one pass, 69 abstentions, 21 too-coarse results and **51 far misses**. Every critical query was parsed and compared with its source-backed building-only control: libpostal isolates the house and floor, but the consumer returns Taiwan's country centroid, 127–138 km away. Fifty raw full-query lookups return only Taiwan; one has no raw candidate. This is attributed to loss of the supplied Taipei hierarchy in matching/fallback, not to failure to parse the floor or proof that the houses are missing. The original 51 critical cases were recorded in tw.labels.jsonl, since removed (see below). The introduction changed no tolerance or expectation and retained every failure; verified label changes after the runtime repair are described below. Other failures remain unlabelled.

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

### OSM suffix numbers and the last labels

[Indexing Taiwan's OSM addresses (#233)](https://github.com/Mokto/opengeocoding/pull/233) supplied the missing `之` keys; it changed import sources only, not query logic. The OSM extract keys about 1.4 million Taiwanese house numbers with `之`, most as `N之M號`, which `number_key` stores as `N之M`. On a Taipei-bbox index from the current builder (1929034e0fce080d) with the chart's Taiwan extract, using the same evaluator, models and reference data:

| Pattern class | Queries | SearchAddress | SuggestAddress |
|---|---:|---:|---:|
| Alley | 127 | 127 | 127 |
| Lane | 127 | 127 | 127 |
| Plain | 131 | 129 | 128 |
| Section | 123 | 123 | 123 |
| House suffix | 114 | 112 | 112 |
| Total | 622 | 618 | 617 |

There are no critical results. Two runs of the same binary have zero answer drift. Full-index CI on #233 also reported the labelled floor cases moving from too_coarse to pass. All six labels are verified fixed and removed, so tw.labels.jsonl no longer exists. Regenerating with `build-gold`, dated on the capture's 2026-10-19 retrieval, first reproduced the corpus byte for byte; removing the labels changed only those six cases. Re-scoring the saved run changes zero classes.

Remaining failures:

- **Missing data (suffix, non-held-out):** 雙和街31巷20弄31之4號, both digit forms. The Taiwan extract has 31, 31之1 and 31之2 on that alley, but not 31之4. SearchAddress returns the alley with goodMatch=false (too_coarse) instead of borrowing another house, and suggestion has no candidate.
- **Evaluation-only (plain, held-out):** 園區街3號, both digit forms, abstains in SearchAddress; three plain held-out suggestion queries are missing. These were not inspected.

The local world index in `~/opengeocoding-index` is built from `~/geodata/osm`, which lacks the five extracts #233 moved, so its Taipei suffix results (0/114) understate the chart's index. These numbers measure this municipal sample, not national Taiwan accuracy, floor or apartment coverage.

### Reverse district expectations (#345)

Both adapters originally left `Entry::places` empty, despite already expanding district codes for forward query generation. Taipei's 622 cases now require the named district; Tainan's 350 cases still have no district check. Tainan uses a different seven-digit address code and its own official-name table; its recent #470 corpus is intentionally not regenerated here.

Taipei's existing 12-name mapping is verified against the National Land Surveying and Mapping Center (內政部國土測繪中心), **代碼服務－鄉鎮市區清單(戶政)** ([catalogue and licence](https://data.gov.tw/dataset/102011), [API specification](https://maps.nlsc.gov.tw/demo/COM_004_代碼服務_鄉鎮市區清單(戶政)_OAS2_V1.yml)). The catalogue explicitly licenses this code service under Government Data Open Licence v1, https://data.gov.tw/license. The unmodified API response is committed as `tests/registry/taipei-districts.xml`: captured 2026-10-31, **1,590 bytes**, SHA-256 `28f02758973901261c7d2c6b3733ed06a196fa64250a1deca7fc08564629157d`. Each generated case cites that capture separately from the address export.

The [county list](https://api.nlsc.gov.tw/other/ListCounty) identifies `A` as 臺北市 / `63000`. `/other/ListTown1/A` returns its 12 **districts**, not villages, postal towns or the city itself. Join `鄉鎮市區代碼` to the exact eight-character `towncode` (`63000010` → 松山區, `63000100` → 內湖區); do not join the separate `towncode01` (`A01`, `A14`). All 12 codes in the pinned 2026-10-19 address capture occur in this list, and every name agrees with the adapter's pre-existing query mapping. The API response is undated, not a historical boundary release; this verifies compatibility with this snapshot, not arbitrary older exports.

Codes remain strings: zeros are significant. Taipei codes have no initial zero, so neither adding an initial zero nor removing an internal zero is accepted; seven-digit codes such as Tainan's are not interchangeable. Unknown eligible address codes fail preparation rather than assigning a default district. The runnable adapter test checks all 12 mappings against the pinned official XML, distinct `63000010`/`63000100`, malformed zero padding, the alternate identifier and an unknown code.

Reproduction (reuse the pinned address input above; do not refresh it). Compare the code-list checksum before accepting a changed capture:

```sh
curl -fL 'https://api.nlsc.gov.tw/other/ListTown1/A' -o SOURCE/taipei-districts.xml
sha256sum SOURCE/taipei-districts.xml tests/registry/taipei-districts.xml
target/release/evaluate build-gold --seeds tests/registry/tw.seeds.jsonl --out tests/registry/tw.jsonl --taipei SOURCE/taipei-4326-2026-10-19.csv
cargo test --locked --release --bin evaluate registry::taipei
```

Only `expect.places` (one official Chinese district name), `source.district_names` and the verification date `source.retrieved` change in the 622 generated rows. The original address input is byte-identical to its pinned SHA-256 above. Query IDs, query text, locations, hold-outs, coordinates, radii, numbers, regions, labels and all other expectations are unchanged. No city-wide alternative is added: accepting 臺北市 alone would not check the district.

#### Saved-run re-scoring

The full saved run at consumer `ce977aa3f9647f7a9424c83f5500a83e6a7b838f`, index fingerprint `fc7a4814f632` (2026-10-31), includes all 622 Taipei cases with reverse place names, house answers and offset answers. Copies were re-scored using `evaluate report RUN_DIR --corpus FILE --rescore`, once against the old expectations regenerated from the pinned input and once against the new corpus. The old-expectation re-score changes zero classes. Original run artifacts are untouched.

| Check | Old classes | New classes |
|---|---|---|
| ReverseGeocode | 608 pass, 14 wrong_house | 622 wrong_place |
| ReverseGeocode offset | 622 pass | 622 wrong_place |
| SearchAddress | 620 pass, 2 abstain | unchanged |
| SuggestAddress | 619 pass, 3 missing | unchanged |
| ResolveRegion | not applicable (no expected region) | unchanged |

All newly exposed place failures hit the **name-comparison rule**; this is not evidence that all these houses are in the wrong district. Saved answers name English districts (for example `Songshan District`), whereas the official list gives `松山區`. The existing scorer's deunicode folding does not equate `區` with `District`; even `北投區` does not equal the saved `Beitou`. It returns `wrong_place` before checking the house, so the 14 existing `wrong_house` failures also change to `wrong_place`; they are not fixed. There are 1,230 newly failing checks and 14 worse-class checks across the two reverse requests, on the same 240 locations. No response, runtime, scoring rule, radius or failure label changes. No English aliases are invented from the answers or guessed transliterations. An official bilingual alias source or preservation/comparison of native names would be separate work; these failures stay visible.

The name mismatch does not rule out genuine district disagreements. For example, all three forms of 信義區市民大道六段38號 (`tw_section-taiwan-96ec6b`, `tw_section-taiwan-4c809d`, `tw_section-taiwan-5c5b7d`) return `Songshan District` at the offset point, rather than the source's 信義區. The saved answer is 八德路四段498之1號. This is a potential cross-boundary offset limitation as well as a language mismatch; no boundary, runtime or tolerance change is made to conceal it.

The [first full-index CI comparison](https://github.com/Mokto/opengeocoding/actions/runs/41237357672), head `2979628`, confirms the same 608 at-point and 622 offset passes lost, with 622 changed classifications for each reverse check. SearchAddress, SuggestAddress and ResolveRegion have zero changes; the service binary, models and index are identical on both sides. Measured speed passes all three RPC budgets. Quality **fails** on the stronger expectations. Its downloaded `report.md` ends mid-case inside Newly failing (55,037 bytes), omitting later sections including Worse failure class; the available report cannot establish a complete per-case CI review. The local saved-run re-score above supplies the complete class counts, but is not a substitute for the missing CI sections.

## Tainan City

The Bureau of Civil Affairs of Tainan City Government publishes [臺南市門牌坐標資料-僅供參考](https://data.tainan.gov.tw/DataSet/Detail/3b5a799b-2ba8-4ac2-84ec-336c8e329962), an annual address-point export from the household registration offices. This sample uses the [ROC114 (2025) resource](https://data.tainan.gov.tw/Resource/af44f904-2f4c-49b2-aaf8-1a64dce09bd4), published 2026-01-14 and updated 2026-07-16. It has 847,103 rows across all 37 districts: county and district codes, village, neighbourhood, street/section, locality, lane, alley, number and TWD97 easting/northing. No people or household records are included.

### Licence and other available registers

The resource explicitly names the **Open Government Data License v1** ([text](https://data.gov.tw/license)). It permits worldwide reproduction, distribution, adaptation and sublicensing for any purpose, provided the original agency, year, dataset/version and licence are attributed. `tw.NOTICE` and the corpus's source fields give that attribution. This is a reading of the published licence, not a grant from the agency.

The source check for #281 also found the same licence on these municipal registers, not sampled in this PR:

| Municipality | Official dataset | Format / caveat |
|---|---|---|
| Taoyuan | [門牌位置坐標資料](https://opendata.tycg.gov.tw/datalist/ec47dbd5-9ed8-4c8d-8ce1-ccb63b1b72e6) | Monthly CSV with the same components and TWD97 two-degree coordinates; manual point placement, errors acknowledged. |
| Kaohsiung | [ROC114 門牌坐標資料](https://data.gov.tw/dataset/172400) | CSV, same components and TWD97 coordinates. |
| New Taipei | [門牌位置資料](https://data.ntpc.gov.tw/datasets/d7b568ab-3819-40c8-a6e7-a6b199443101) | Monthly ZIP/API, `x_3826`/`y_3826`; a full bulk capture remains to be pinned. |
| Taichung | [門牌坐標資料](https://data.gov.tw/dataset/177460) | TWD97 and longitude/latitude columns; the catalogue points to a separate download listing. |

These are options for later coverage, not a claim that every city/county publishes this schema. The present addition is Tainan alone, chosen before seeing any service answer. It does not establish national recall.

### Capture and coordinate interpretation

```sh
mkdir -p ../tainan-data
curl -fL --get 'https://data.tainan.gov.tw/File/DirectDownload/af44f904-2f4c-49b2-aaf8-1a64dce09bd4' \
  --data-urlencode 'fileName=114年臺南市門牌坐標資料' -o ../tainan-data/tainan-114.csv
(cd ../tainan-data && duckdb -f REPO/tests/registry/tainan-csv.sql)
target/release/evaluate sample-tainan --tainan ../tainan-data/tainan-114-4326.csv --seed tw-tainan-2026-10 --out tests/registry/tw-tainan.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/tw-tainan.seeds.jsonl --out tests/registry/tw-tainan.jsonl --tainan ../tainan-data/tainan-114-4326.csv
target/release/evaluate run --corpus tests/registry/tw-tainan.jsonl --index out --out eval-results/tainan
```

The capture of 2026-10-31 is pinned below. The resource ID identifies the annual release, but its contents can be replaced, so retain these files. Preparation, sampling and gold generation each repeated byte for byte.

| File | Bytes | SHA-256 |
|---|---:|---|
| `tainan-114.csv` | 70,278,778 | `93dd7780f59a7d70f01bf08e9e4ad3c2e0458e1a4968671fabea64fae7f121c2` |
| `tainan-114-4326.csv` | 101,154,871 | `dda2dfaf86f0c220fefac327cfeaaa8afda428268e747eb36e24b1aa5e17c6cd` |

The page says **TWD97**, but does not specify the projection zone (its CRS metadata is the placeholder `0`). `tainan-csv.sql` interprets the metre coordinates as **EPSG:3826**, TWD97 / TM2 zone 121, and converts to WGS84 with DuckDB spatial / PROJ, `always_xy=true`, as for Taipei. A check against the [Labour Affairs Bureau's employment-service locations](https://data.tainan.gov.tw/Resource/0e2e6c3d-1ff1-44b6-8373-02edc5b8c09d) finds its published longitude/latitude within 1.7 m of the 忠政里 record of 民治路36號 (`179912.73, 2578949.67` → `23.31156039853972, 120.31473211844106`), but 231 m from 永華路二段6號. The employment dataset does not explicitly declare WGS84; this is a plausibility check on the interpretation, not survey calibration. Several 民治路36號 records themselves disagree by over 1 km and are excluded as ambiguous below.

The address-register page warns that household offices manually placed the points relative to other addresses, that they are not accurate positions, and that a point does not prove a building/doorplate exists. The **300 m tolerance** was set before running the service to allow for that warning; it is not a measured bound on source error. Even a passing point is not verified at entrance or building precision.

### Eligibility and draw

The adapter reads only source rows whose number is one to five ASCII/full-width digits, without a leading zero, optionally followed by `之` and one to four digits, then `號`. The source spelling is kept in the native query. Floors, `97號之1`, prefixes such as `建`, ranges and other numbering are outside this frame; `97號之1` is not silently treated as `97之1號`. A street or locality must be present. One record has projected coordinates `3422483.03, 9441669.63`, far outside Tainan, and is excluded by the projected bounds (150,000–220,000 E, 2,530,000–2,600,000 N); every eligible converted point must also lie within 22.8–23.5° N, 120.0–120.7° E, so swapped axes fail loudly.

Of 847,103 rows, 550,697 match the number shape and 550,696 also pass the coordinate bounds. They make 550,653 address keys after ASCII-digit normalization of district, street, locality, lane, alley and number, NUL-separated and SHA-256 hashed. A key is the address as queried, without a village/neighbourhood; 17 keys whose source rows disagree on the point are excluded, leaving **550,636 unambiguous keys**. Identical-point repeats use the lexicographically smallest source record, so input order does not choose the answer.

District names expand the seven-digit source codes (`6700100` → 新營區) with the city's [臺南市各區代碼](https://data.tainan.gov.tw/Resource/5b389f60-dee6-425a-8a40-f5cb2f949cf1) (Research, Development and Evaluation Commission, updated 2025-07-25), whose eight-digit codes use `67000010` for the same district. The adapter's fixed 37-name table follows that list; an unknown source code fails rather than guessing. The downloaded table is 1,192 bytes, SHA-256 `5e1c23f9d1f7c459a77861e2907f047af4ff3ab698f2d9069d683b4f15f7a13e`.

Each district/category pool contributes the address with the smallest seeded SHA-256 of its address key. Categories have precedence: suffix (`之`), alley, lane, section (`段`), locality/area without a street, then plain street. Pools that do not exist contribute nothing:

| Category | Locations | Queries |
|---|---:|---:|
| Suffix | 37 | 74 |
| Alley | 28 | 56 |
| Lane | 28 | 56 |
| Section | 22 | 44 |
| Locality/area | 31 | 62 |
| Plain street | 29 | 58 |
| Total | 175 | 350 |

All 37 districts contribute. Each location has a native query (`臺南市新營區太子路５３巷１弄３２號, Taiwan`) and an ASCII-digit control at the same point. The draw balances districts and address shapes, not population or address counts; it is not a representative citywide accuracy sample. **42 locations (84 queries)** are held out by the shared location-hash rule, both forms together. No service answer selects or supplies an expectation.

Cases require house level, goodMatch true, the source number including any `之` suffix, and a point within 300 m in Taiwan and Tainan (`TW-TNN`). The input has no postcode. ReverseGeocode checks the house number and region at the point and 25 m away; ResolveRegion checks Tainan. No district-level `places` expectation is asserted for Tainan: its adapter currently does not propagate the mapped district name to `Entry::places`. This remains outside the Taipei-only #345 update above.

**Index ancestry.** Overture 2026-09-23.1 holds 847,064 addresses from OpenAddresses' `tw/tnn/regionwide`, whose source definition cites this register and an uploaded 2025 extract (2026-06-04). All 175 sampled addresses match its district, street/locality/lane/alley and reconstructed number within 7 cm (median 4 cm). OpenAddresses splits the number's remainder into the unit; the builder's Taiwan import keeps `之` subnumbers. The local index has no separate Tainan OpenAddresses file, but also reads the chart's Taiwan OSM extract. This measures fidelity to the register, not independent real-world accuracy.

### Local baseline

On a full local index from builder `dbec0f0cae254966` (Overture 2026-09-23.1, OpenAddresses and the chart's OSM extracts including Taiwan), **SearchAddress and SuggestAddress each pass 332/350**, or 166/175 locations. Held-out SearchAddress passes 82/84 queries (41/42 locations). The other 18 queries abstain, with no critical result or label. ReverseGeocode at the point and 25 m away and ResolveRegion each pass 350/350. A same-binary repeat has zero response/scoring drift across all these RPCs. Preparation and sampling changed no Taipei case.

Eight non-held-out failing locations were inspected; all have their exact number on their expected road at the register point in ReverseGeocode. Five are in 新市區 (蓮潭南一巷15弄, 勝利街35巷, 富強路, 環東路一段, 豐華), two in 左鎮區 (光和, 中正), and one in 中西區 (協進市場). Diagnostic queries without the district also abstain; an explicitly comma-separated number/road/district/city form still abstains for the five 新市區 locations and answers too coarsely for the other three. This attributes the failures to the query/lookup hierarchy path, not missing houses; the exact parser versus matching cause remains untriaged. The one held-out failing location was not inspected. No runtime repair, tolerance increase or label was used.

These are local diagnostic results, not a CI gold run or a national accuracy claim. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure. Repeated local timings differed even with identical answers and binary, so no speed conclusion follows; CI's gold-speed is the speed gate. Human review of source points is pending.
