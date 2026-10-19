# Denmark

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/dk.NOTICE)

Source: **Danmarks Adresseregister (DAR)**, maintained by the municipalities and distributed by Klimadatastyrelsen through Datafordeleren, under CC BY 4.0 with attribution. The inputs are official entity exports, not geocoder answers. The earlier access blocker is resolved by a saved offline national snapshot; the preparer needs no credentials and makes no network requests.

## Source snapshot and reproduction

The pinned source is **DAR v4, Current JSON, generation 787**. Retain the original ZIPs and a checksum manifest outside git. All entity files must have the same generation and snapshot point in time. Required entities are Adresse, Husnummer, Adressepunkt, NavngivenVej, NavngivenVejKommunedel, Postnummer and SupplerendeBynavn. The capture also retains NavngivenVejPostnummer and NavngivenVejSupplerendeBynavn, which are not used for the join. The manifest records register, version, generation, type, format, point_in_time and each file's entity/filename/bytes/SHA256; never store authentication parameters in it or the corpus.

Preparation requires Python 3.9+ and an installed DuckDB CLI with its spatial extension. This is an offline development tool, not a runtime dependency.

```sh
python3 -u tests/registry/export-dar-addresses.py --source SOURCE/dk-2026-10-19 --out SOURCE/dar-prepared-787 --seed dk-2026-10
target/release/evaluate sample-dk --dar SOURCE/dar-prepared-787/dar.json --seed dk-2026-10 --out tests/registry/dk.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/dk.seeds.jsonl --out tests/registry/dk.jsonl --dar SOURCE/dar-prepared-787/dar.json
```

Archive names, sizes and SHA256 values are checked before reading. Extraction/import is checkpointed per entity as a projected-column Parquet file, tied to the input hash, SQL and DuckDB version; changed checkpoints require a fresh output directory. Raw ZIPs remain unchanged. The final dar.json is immutable. Large temporary JSON files are removed after successful import; keep the source archives and work receipts.

## Eligibility and coordinates

- Current exports still contain retired logical objects. Select status **3 (gældende)** for addresses, houses, roads and related names, but **8 (i brug)** for address points; the status domains differ. Evaluate both registration and effect intervals at the common source snapshot time. Duplicate/null current UUIDs fail preparation.
- Follow Husnummer.adgangspunkt to Adressepunkt. Never substitute Husnummer.vejpunkt or Adresse.dørpunkt. Require point standards TD (building entrance), TK (road-facing facade) or TN (within building footprint), and accuracy class A (absolute) or B (computed). UF, TA, road points and unknown/unusable positions are outside the eligible frame. These classes are not equivalent precision guarantees.
- The [official object catalogue](https://grunddatamodel.datafordeler.dk/objekttypekatalog/Danmarks%20Adresseregister/Adressepunkt.html) defines position in EPSG:25832. DuckDB spatial transforms it to EPSG:4326 with always_xy=true; original UTM coordinates and quality fields remain in provenance. A 150 m tolerance checks proximity to the published building-access point, not verified floor/door coordinates or surveyed entrance precision.
- Require linked current road and postcode facts, any stated supplementary place, a positive single house number with an optional ASCII letter, and source labels consistent with their linked street/number/postcode/postal district. Unknown/missing links and inconsistent labels are excluded before sampling, not repaired from service answers. Danish ranges and non-ASCII house letters are not validated by this set.

## Sampling and coverage

For known BBR building IDs, group entrances by that ID; otherwise use the GeoDanmark building ID, and fall back to the entrance UUID where neither is supplied. Choose the lowest house UUID in each group, then draw up to 20,000 groups by seeded SHA256. This avoids counting a shared preferred building ID as independent locations; missing/cross-system building links can still split a physical building. No audited BBR–GeoDanmark crosswalk is applied, so these are source-ID groups, not verified independent physical buildings. The selected original entrance's point and number define every variant in its group; representative entrances and registered units are not uniformly sampled.

For each candidate entrance, choose a current registered address preferring a published door, then a floor, then the lowest address UUID. Native strings come from the original adgangsadressebetegnelse and adressebetegnelse. No floors, doors, translations or ASCII place aliases are invented. The catalogue sampler balances the road-managing municipality and mutually exclusive pattern classes, taking up to three different road UUIDs per pool. This administrative code is **the road's managing authority, not necessarily the house's physical municipality**.

Pattern classes are side doors (th/tv/mf), digit-containing doors, other door labels, floor-only, house letter, supplementary place and plain. Tags retain overlapping letter/floor/supplementary evidence. All variants share a building/entrance location-based hold-out. This is a two-stage, pattern-balanced diagnostic sample, not a population-weighted national accuracy estimate. Municipal and record-quality exclusions, building links and limited candidate counts can underrepresent rare/rural formats. Greenland and the Faroes are not covered. Human review remains pending, and shared DAR ancestry in index inputs limits independence.

SearchAddress checks country, house resolution, exact source house number/postcode, source-point proximity and goodMatch. SuggestAddress checks country/type/proximity in the first five, not door identity. The building-only control and published unit form use the same source point; this tests building recovery, not apartment-specific accuracy. ReverseGeocode checks country only; ResolveRegion is unscored. Existing Copenhagen City Hall suggestion examples remain separate.

## Validation

The pinned capture contains 4,168,949 Adresse rows (3,944,832 eligible current rows), 2,704,689 Husnummer rows (2,615,431 current), and 5,411,723 Adressepunkt rows (5,236,948 in use). The quality/link join yields 2,507,560 entrances, all with projectable point geometry; bounds and label/coherence checks retain 2,485,478 entrances in 2,101,133 preferred-source-ID groups. The seeded first-stage capture has 20,000 candidates, each linked to a current registered address. The normalized dar.json is **15,601,674 bytes**, SHA256 **d1247d41a544053bd37cd7feb98e79a557041feacf33b409ea0320d2dc32ddd0**, prepared with DuckDB v1.5.6. Full archive hashes and counts remain in its provenance.

The final corpus has **1,883 queries / 1,358 groups / 272 held-out groups**, covering 99 road-authority codes, not necessarily 99 physical municipalities. Group keys comprise 1,318 BBR, 20 GeoDanmark and 20 entrance-only IDs. Selected points are 164 TD, 589 TK and 605 TN; all 1,358 have accuracy class A. Although B is eligible, this draw does not validate it.

| Pattern | Groups | Queries | SearchAddress passes | Full-string SuggestAddress passes |
|---|---:|---:|---:|---:|
| Plain | 292 | 292 | 291 | 0 |
| House letter | 286 | 286 | 284 | 0 |
| Supplementary place | 255 | 255 | 245 | 0 |
| Floor-only | 217 | 434 | 376 | 145 |
| Side door | 194 | 388 | 294 | 138 |
| Digit-containing door | 107 | 214 | 122 | 10 |
| Other door | 7 | 14 | 10 | 0 |

Overall forward results are **1,622 pass**, 228 too coarse, 14 near misses, 11 abstentions, four confidence mismatches, three wrong-country answers and one far miss. The 1,358 building-only controls pass 1,341 times; the 525 published unit forms pass 281 times. Full-string suggestions pass 293/1,883; **all building-only full-string suggestions fail**. Do not drop postal qualifiers or rewrite published input to hide this limitation. Other noncritical failure causes have not all been isolated. Country-only reverse checks pass 1,882/1,883; Christiansø 97 returns empty. ResolveRegion remains unscored.

Four severe failures remain attributed in dk.labels.jsonl, with unchanged coordinates, 150 m tolerances, numbers/postcodes, levels and goodMatch expectations:

- Published `st. th` forms for Mogensgade 9 in Grenaa, Marienlundsgade 10 in Randers C and Vester Altanvej 26 in Randers C return Thailand's country centroid, about 8,630–8,681 km away. The native parser swallows the address roles and omits the country (Mogensgade labels Denmark as city); the eventual fallback country corresponds to the door abbreviation th despite the supplied Denmark. All three existing building-only controls resolve the correct house/postcode near the source point with goodMatch true. This establishes coverage, not an apartment-coordinate expectation.
- The published `st. th` form of Tjærbyvej 19, Randers NØ swallows the house/unit/postcode into the road and interprets NØ as the city, retaining Denmark. The final locality is No, 115 km away. Its building-only control resolves the correct indexed house. This is lost address/locality qualification, not demonstrated missing-house coverage.

No resolver, matching, index-building, scoring, runtime dependency or wire-contract changes accompany this addition. Formatting, all-target clippy, cargo test, the model-backed synthetic test, release builds and evaluate check pass. Seeds and corpus regenerate byte-for-byte; replaying the full offline source preparation retains the normalized SHA256. The SQL/source regression check is runnable locally with `python3 tests/registry/test-dar-export.py` (DuckDB + spatial required; not installed by CI). It checks status domains, both time intervals, joins, building grouping, native unit selection, projection axis order, input hashes/path rejection, authentication-field minimization and immutability. Rust tests cover sampler ordering, pattern classes, same-location variants, missing references and invalid point status.

A complete **15,006-case** local run passes its absolute quality gate. Against the current-main 11,057-case baseline, shared SearchAddress, SuggestAddress, ResolveRegion and ReverseGeocode answers/scoring have **zero drift**, with no newly failing or worse-class cases; all new-country standalone classifications are unchanged. Re-scoring changes zero classes, including the four still-failing Danish diagnostics. Both sides use identical service binary 71fc5d004964, model digest 65627ea2114b and reference data. These runs use the last-known-good full world index, builder 4fb1fd818b46b2f6 / fingerprint e6df26bed27b; they do not validate a successful rebuild of current builder a7481f0be4727233. The documented fresh-service grpcurl SearchAddress hang remains a known failure even though the runs report it passing.

The local comparison's separate **speed check fails**: shared-request totals are +105% SearchAddress, +81% SuggestAddress and +89% ReverseGeocode. The binaries are identical, and source preparation/compiler checks overlapped the candidate on a non-isolated host, so these are confounded timings, not an established code slowdown or a speed pass. Current-head CI quality and measured speed must be checked separately in the pull request; local accuracy and unit checks do not establish production accuracy or latency.
