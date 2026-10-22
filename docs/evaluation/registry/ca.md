# Canada

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/ca.NOTICE)

Source: **Statistics Canada National Address Register (NAR), June 2026**, under the Statistics Canada Open Licence, with information also licensed under Open Government Licence – Yukon 2.0. These terms require attribution and prohibit linking the information to identify an individual person, business or organization. Only address components and geographic/source identifiers are retained. The sample tests geography, not mail deliverability, and implies no endorsement by Statistics Canada.

## Snapshot and reproduction

The official [product](https://www150.statcan.gc.ca/n1/pub/46-26-0002/462600022022001-eng.htm) links the [June 2026 ZIP](https://www150.statcan.gc.ca/n1/pub/46-26-0002/2022001/202606.zip). The [user guide](https://www150.statcan.gc.ca/n1/pub/46-26-0002/462600022026001-eng.htm) defines the entities, fields, coordinates and licenses. Retain the ZIP outside git and its `202606.zip.receipt.json`, containing the official `url`, completed `bytes`, `sha256` and HTTP `last_modified` (or null). The preparer checks the URL, size and checksum before reading. The acquired archive is **1,665,944,959 bytes**, SHA256 **c87df8d39537b75146ab4b38b9b1ca1f820d72e0f74ee2b3722059851a564364**, with 55 ZIP members. Every member's CRC was verified. A late transfer timeout was recovered only after verifying the unchanged ETag, matching saved prefix and exact HTTP 206 Content-Range; partial responses were not blindly appended.

Python 3.9+ standard library is sufficient; the preparer is offline and is not a runtime dependency.

```sh
python3 -u tests/registry/export-nar-addresses.py --source SOURCE/ca/202606.zip --out SOURCE/ca/nar.json --seed ca-2026-10
target/release/evaluate sample-ca --nar SOURCE/ca/nar.json --seed ca-2026-10 --out tests/registry/ca.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/ca.seeds.jsonl --out tests/registry/ca.jsonl --nar SOURCE/ca/nar.json
```

Each province/territory is checkpointed in `nar.work/`, bound to the archive hash, preparer hash, seed, candidate count and province; the checkpoint's content checksum is verified on replay. Changed inputs require a fresh output directory. A different final capture is never overwritten. No credentials, request headers, personal/business names or land-survey/delivery details are exported.

## Selection and address forms

The source separates `Locations/Location_*.csv` from `Addresses/Address_*.csv`, with large provinces split across parts. All thirteen province/territory file families are required. Stream every location and select up to 2,000 lowest SHA256(seed NUL LOC_GUID) values per province among records with a province-matching seven-digit CSD and finite Canadian-range **BG_LATITUDE/BG_LONGITUDE**. Duplicate location UUIDs within a province fail. Missing building points are excluded; **BF_REPPOINT blockface centroids are never substituted**.

Scan every address file and join selected locations by LOC_GUID. Eligible rows have a positive structured civic number, an empty/letter/fraction civic suffix, a source apartment label in the supported alphanumeric/space/slash/hyphen grammar, matching province and mailing-province codes, and a populated mailing street/place and Canadian-shaped postal code. Prefer a published unit-bearing row, then the lowest ADDR_GUID. Missing/unusable addresses are omitted without backfilling the first-stage location sample. Candidate-address UUID duplicates fail, as do duplicate final location IDs across provinces.

The Rust adapter balances province and mutually exclusive classes: unit, civic suffix, street direction, other Quebec addresses, and plain. It selects up to eight different mailing-locality/street combinations per pool by seeded location hash. This is a diagnostic stratified sample, not a uniform national draw or population-weighted accuracy estimate. The Quebec class means geography, not a proof that every name is French. Rural delivery routes and comprehensive bilingual street aliases are not covered.

Queries include the building mailing form, a source unit/civic hyphen form when an apartment is published, and a distinct geographic form using official street components and the source CSD name (French for Quebec, English otherwise). Recognised French street designators are placed before names, except ordinal street names; source spelling is preserved, not translated. CSD names are geographic qualifiers, not claimed postal-town aliases. Every form shares the source location and hold-out assignment. LOC_GUID is a provider building-location identifier, not an independently audited physical-building crosswalk.

## Expectations and limitations

- NAR includes addresses validated by at least two sources, but is a statistical register with its own inclusion and georeferencing coverage. Excluding missing BG coordinates further changes the frame.
- The BG coordinates are building-associated representative points in EPSG:4326. The guide explicitly allows a road access point or driveway; they are **not surveyed rooftop or entrance coordinates**. No per-point positional-quality classification is supplied here. A 150 m tolerance checks proximity to the published point, not production building accuracy.
- Forward search requires Canada, house resolution, the complete civic number, source postcode, source-point proximity and goodMatch. A unit row accepts either its complete building number or published unit-building composite, at the same point; this does not test apartment identity.
- Full-string suggestions check country/type/point among the first five, not unit identity. Reverse checks country only; ResolveRegion is unscored.
- Municipal/provincial address data can occur in NAR, OpenAddresses, OSM and the index's other inputs. Shared ancestry limits independence. Hold-outs group source locations, not every possible duplicate physical building.
- No resolver, index builder, scorer, runtime dependency or wire-contract changes belong to this addition. Compilation alone establishes no geographic accuracy.

## Unit/civic resolution

SearchAddress and SuggestAddress try the literal house number first, preserving indexed compound numbers and ranges. If no house is found, a leading alphanumeric unit/civic hyphen form can retry the civic number as the building number. Recovery requires an unambiguous comma-separated Canadian country qualifier, retains the street/locality/province/postcode, and accepts only an indexed Canadian house with the requested civic number. Forward recovery also requires the building result's existing goodMatch checks; a missing requested building is not a good match.

This returns the building, not a verified apartment or unit coordinate. A bare `CA` qualifier is ambiguous with California and does not enable recovery; the suggestion country filter alone is not sufficient either. Spaced fractional civic numbers remain an autocomplete limitation when the building-only form itself is unsupported; synthetic checks retain that baseline failure. Forward search can recover supported fractional building forms.

## Civic suffixes and street directions

Overture's Canadian address rows write about 98,000 suffixed civic numbers as a float and the suffix (`55.0 A`, `29.0 1/2`); the builder stores them as `55A` and `29 1/2`, the forms NAR and Canada Post use. Plain `55` rows stay separate houses: a suffix is never stripped or substituted.

French street directions fold like English ones (`Ouest`/`O`, `Est`/`E`, `Nord`/`N`, `Sud`/`S`), so `1455 Boulevard de Maisonneuve Ouest` finds Overture's `boul. De Maisonneuve O`. The folding is shared by all text, so English and French direction words of any place name now match each other (`South Province` and `Province Sud`).

Two SearchAddress retries run only when the query as written finds no house. A suffix glued to a number the parser reads as part of the road (`82A AV MERCIER`) is retried spaced, and answers only with that exact number. A street direction the source omits (`31 THATCHER DR E` for Overture's `Thatcher DR`) is retried without it, and answers only with a Canadian house of the same number whose postcode equals the one stated; without a postcode in the query there is no retry.

Remaining failures include suffixed buildings the sources lack (several Whitehorse ones), `Robert's RD` against a query's `ROBERTS RD`, and duplicated house numbers on same-named streets of one city.

## Corpus-introduction validation

The archive contains **11,795,284 location rows and 17,362,476 address rows**. Requiring matching CSDs excludes 11,471 locations; another 1,068,910 lack usable BG coordinates, leaving a **10,714,903-location point frame**. The first-stage draw contains 26,000 locations across all thirteen provinces/territories. Of these, 4,247 have no eligible joined address and are not replaced; the normalized capture has **21,753 candidates**, **13,575,893 bytes**, SHA256 **5d02a3da1f7b796d1ef2ffe771501416d4eef638eacb84d559af6dc18e0b7910**. This is not a count of all nationally eligible addresses.

The final corpus has **550 queries, 373 source building-location groups, and 69 held-out groups (100 queries)**. Source preparation replays to the same capture hash; seeds and corpus regenerate byte-for-byte. The standalone source run on the full local index (builder **a7481f0be4727233**, fingerprint **4cfc1e4547bd**) passes its absolute gate with no critical results or added baseline labels:

| Pattern class | Locations | Queries | Search passes | Full-string suggestion passes |
|---|---:|---:|---:|---:|
| Direction | 80 | 98 | 89 | 71 |
| Plain | 96 | 120 | 120 | 95 |
| Other Quebec | 8 | 10 | 10 | 6 |
| Civic suffix | 85 | 103 | 27 | 20 |
| Unit | 104 | 219 | 107 | 88 |

SearchAddress passes **353/550**, with 188 too coarse, eight near misses and one abstention. Building mailing controls pass **297/373** and distinct geographic forms **56/73**. All **104 unit/civic-hyphen forms fail both forward search and suggestions**; their own building-only controls pass **98/104**, establishing indexed building recovery for those controls but not isolating every failure's cause. Training-only parser probes retain `100-18107`, `602-1531` and `204-625` as entire house numbers; the corresponding unprefixed `18107`, `1531` and `625` controls resolve correctly at the source points. This identifies a unit/building-number interpretation gap for those examples, not a justification for globally splitting hyphenated numbers. Do not remove the units or relax the number/point checks to conceal this gap. Full-string suggestions pass **280/550**. Country-only reverse checks pass **550/550**; ResolveRegion remains unscored. Noncritical source/index/point discrepancies have not all been individually adjudicated, and human review remains pending.

The complete **16,483-case candidate** passes its absolute gate. Against the **15,006-case baseline**, shared answers and classes are unchanged across all four RPCs: no newly failing or worse-class cases. New-country standalone and complete-run answers/classes/suggestion results agree. Both sides use the same service binary (**031c161c67d72e17f53f2e2a28d4edeef421f8552bbf17963e1524dc5a75869a**), models (**65627ea2114b**), reference data and full index. Hardening immutable capture writes and strict CSV validation changed only the preparer fingerprint/provenance: fresh raw-source preparation preserved every source record/point, seed, query, expectation and hold-out; rescoring all 16,483 saved results with the final corpus changed zero classes.

**The local speed comparison fails:** SuggestAddress total +12% and p99 +16%, ReverseGeocode p50 +12% and p90 +11%, beyond the 10% limit. This is a shared, non-isolated host, with identical runtime binaries and other test activity during part of the comparison; it is neither a speed pass nor an established runtime slowdown. Current-head CI quality and measured speed are still required. The fresh-service grpcurl SearchAddress hang remains a documented known failure even though the evaluator prints it as passing; this corpus does not sign off gRPC interoperability.

Each case's `places`, the names ReverseGeocode's answer must include one of, are the mailing municipality (`MAIL_MUN_NAME`) and the census subdivision name.
