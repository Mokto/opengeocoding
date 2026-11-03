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

The four severe failures of the first draw (the published `st. th` forms of Mogensgade 9 in Grenaa, Marienlundsgade 10 and Vester Altanvej 26 in Randers C, and Tjærbyvej 19 in Randers NØ) were attributed in `dk.labels.jsonl` and are fixed; the file is removed. The native parse of such a query swallows the house, floor, door and postcode into the road and reads Denmark as a city or into the road. A trailing `th` then became a parsed country (Thailand's code) in the resolver's rotated retry, or the postal-direction suffix NØ became the locality No. The cause was the unit part, not the TH or NO country codes, which stay recognised. A floor and door written as its own comma part (`st. th`, `1. tv`, `2. 3`, `kl. 1`) is now set aside and the building's house answers, only when that building resolves to a house; otherwise the query is resolved as written. A door other than th/tv/mf needs Denmark stated, and a part naming another country keeps the query as written. Each of the four now returns the source's house, number and postcode with goodMatch true, as its building-only control does; this is building recovery, not apartment identity, and the unit forms and expectations are unchanged. On the local last-known-good index the four answered an abstention (three) or No (one), not Thailand's centroid; the centroid came from the CI index, which was not available locally, so the Thailand path itself was not reproduced here.

The addition of this corpus came with no resolver, matching, index-building, scoring, runtime dependency or wire-contract change; the unit-form fix above is a later, separate change. Formatting, all-target clippy, cargo test, the model-backed synthetic test, release builds and evaluate check pass. Seeds and corpus regenerate byte-for-byte; replaying the full offline source preparation retains the normalized SHA256. The SQL/source regression check is runnable locally with `python3 tests/registry/test-dar-export.py` (DuckDB + spatial required; not installed by CI). It checks status domains, both time intervals, joins, building grouping, native unit selection, projection axis order, input hashes/path rejection, authentication-field minimization and immutability. Rust tests cover sampler ordering, pattern classes, same-location variants, missing references and invalid point status.

A complete **15,006-case** local run passes its absolute quality gate. Against the current-main 11,057-case baseline, shared SearchAddress, SuggestAddress, ResolveRegion and ReverseGeocode answers/scoring have **zero drift**, with no newly failing or worse-class cases; all new-country standalone classifications are unchanged. Re-scoring changes zero classes, including the four still-failing Danish diagnostics. Both sides use identical service binary 71fc5d004964, model digest 65627ea2114b and reference data. These runs use the last-known-good full world index, builder 4fb1fd818b46b2f6 / fingerprint e6df26bed27b; they do not validate a successful rebuild of current builder a7481f0be4727233. The documented fresh-service grpcurl SearchAddress hang remains a known failure even though the runs report it passing.

The local comparison's separate **speed check fails**: shared-request totals are +105% SearchAddress, +81% SuggestAddress and +89% ReverseGeocode. The binaries are identical, and source preparation/compiler checks overlapped the candidate on a non-isolated host, so these are confounded timings, not an established code slowdown or a speed pass. Current-head CI quality and measured speed must be checked separately in the pull request; local accuracy and unit checks do not establish production accuracy or latency.

## Unit-form fix: local comparison

Same last-known-good index (builder 4fb1fd818b46b2f6, meta fingerprint e6df26bed27b), models and reference data on both sides, `evaluate run --suite gold --repeat 1` of the whole 15,006-case suite, base `7f3e72b` against the change. SearchAddress passes rise from 11,951 to 12,108 of 14,927 scored checks with 158 changed results; SuggestAddress, ResolveRegion and ReverseGeocode have zero drift, and `evaluate diff` lists no newly failing case and no worse failure class. All 158 changed results are in the three Danish door cohorts: 12 empty answers gain a house, 143 become finer (a postcode or street becomes the house), two move to a different house and one only changes its locality label. Danish SearchAddress passes: door-side 294 → 384 of 388, digit doors 122 → 187 of 214, other doors 10 → 12 of 14; plain, house-letter, floor-only and supplementary cases are unchanged. The Danish remainder is 92 too coarse, six near misses, four confidence mismatches and two abstentions, none of them new. The two moved cases (Smålodsvej 21A and Vester Søttrup Vej 36) now return the house on the stated street instead of a house of another street. Local three-request timings are not comparable (the base run overlapped other work); current-head CI quality and speed gates are the reference.

## Remaining Danish forms: local comparison

The same comparison (last-known-good index, whole 15,006-case suite, `main` against the change) after three further changes:

- A floor alone (`1.`), a door alone (`1`, `3b`) and a floor with a word door are set aside like `st. th`, but only with Denmark stated; a lone `th`/`tv`/`mf` part is never touched without it.
- A supplementary place between house and a four-digit postcode ("Pittevej 6, Aidt, 8881 Thorsø, Denmark") breaks libpostal's parse of the whole address. When the address as written has no house, it is retried without that place; the answer must be a house with the stated postcode, and is not a good match unless it names the place.
- `SuggestAddress` ignored a trailing "Denmark" as the country, because the English names of the regions ("Capital Region of Denmark") contain the word. Without a country, a four-digit postcode is not looked up, so every building-only full-string suggestion failed. A region whose name only contains "Denmark" no longer cancels it (for Denmark only: France's overseas departments are addressed "…, France" under other country codes, and treating France as the country dropped them). A place between the road and the postcode's town is tried as the house's place, and a unit part may now be a floor with a numeric or one-letter door, or a side door alone.

Danish SearchAddress passes rise from 1,779 to 1,867 of 1,883 (door-side 384 → 386, digit doors 187 → 210, other doors 12 → 13, floor-only 376 → 434, supplementary 245 → 249; plain and house-letter unchanged). Full-string SuggestAddress passes rise from 293 to 1,838 (building-only controls, which all failed, now pass 285 of 292 plain and 281 of 286 house-letter cases). Shared SearchAddress passes in the whole suite rise from 12,178 to 12,266 and SuggestAddress from 8,232 to 9,791, ResolveRegion and ReverseGeocode do not change, and `evaluate diff` lists no newly failing case and no worse failure class; four supplementary cases move to a better class (too coarse or near miss to confidence mismatch).

What remains, not fixed here:

- Sixteen SearchAddress failures. The seven too-coarse and one near-miss case are mostly houses the index lacks (the building-only control is also street- or postcode-level, e.g. Søndre Tangvej 6, Rynkevangen 8, Skolegården 6F): an importer question. Eight are confidence mismatches: the house, number and postcode are right but the answer names a different locality than the source's supplementary name (Bellinge against the index's Kildemose, DAR's abbreviated "Skovshøjr Mk"), so goodMatch stays false by design.
- 45 full-string SuggestAddress failures, mostly houses whose street is suggested but whose number is absent from the suggestion index.
- The Thailand path itself (the CI index) was not reproduced locally.

Each case's `places`, the names ReverseGeocode's answer must include one of, are the supplementary town name, the postal town and the postal town without its district ("København NV" is also København).

## Initial reverse-locality investigation (2026-11-02)

This initial investigation followed [#345](https://github.com/Mokto/opengeocoding/issues/345), not a resolution of it. No supported geographic-locality replacement was established from the pinned DAR fields or existing index metadata, so the first version changed only tests and documentation. The controls protect geographic membership from both a misleading nearby village point and a distinct postal town, and demonstrate a legitimate locality-boundary crossing at a 25 m offset. The historical results below describe that first version. **The follow-up section adds a runtime repair from a separately sourced official polygon dataset.** Scoring, expectations and known-failure labels remain unchanged.

### Reproduction and versions

The [nightly issue #501](https://github.com/Mokto/opengeocoding/issues/501) reported main `32154ee` and combined several Danish corpora and both reverse channels. Its counts are not the baseline below. This reproduction uses fresh main `37c8fa86fd1ad9c4d3fa7b5d4dfe48b412ec8dd4`, a separate worktree/target, and the shared local full index read-only, with its owner's approval. No index was rebuilt, overwritten or deleted.

| Input | Recorded version |
|---|---|
| Service binary SHA256 | `50f33d6398be548ae027ead1a160b9c5f936cb46b2cd9510c89921da48872349` |
| Evaluator | 1.6.1; binary SHA256 `e04ef8ec0f6f5eea8a9c93efb6258a38511ad359ab1c4f2859c2881eb1a54d6a` |
| Danish corpus SHA256 | `d36546fe0a792048b517fb403c89e09638909782f924b3c518ebc983496ebbdb` |
| Complete gold corpus SHA256 | `3dcefbd1fa019468d7c09baeb6a28344516e964662e7529f05ba0f1fa6125e9e` |
| libpostal | Homebrew 1.1.4; model digest `65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7` |
| Reference data SHA256 | `9a68f034f3b05f57aecdec9776b06c63a15cf48619703a6fd797bb61ce7cfb01` |
| Historical memberships SHA256 | `16871dd9c5b3dc74b8a9262f7986e2b35abde43231f6f72e9d643cfa4dc665fc` |
| Local index | `~/opengeocoding-index/out`; metadata fingerprint `b6d5435392fc`; sidecar builder `bb05cfe6f44d46bd` |
| Current main builder/chart pin | `8c53269491f72efb`, **not** the local index's sidecar hash |

The shared build recipe names Overture 2026-09-23.1, OpenAddresses, OSM address/landmark extracts, Korean shops and GeoNames populated places plus GB/CA/NL postcodes. This local directory has no sealed `MANIFEST` proving every input's checksum; the recipe and builder sidecar are not a complete source receipt. These results therefore do not establish equivalence to the nightly's source snapshots or to a current-builder index. All local comparisons keep this exact directory, models, reference data and corpus unchanged.

```sh
INDEX="$HOME/opengeocoding-index/out"
nice -n 15 target/release/evaluate run --corpus tests/registry/dk.jsonl --index "$INDEX" --repeat 1 --out eval-results/dk-reverse/base
nice -n 15 target/release/evaluate run --corpus tests/registry/dk.jsonl --index "$INDEX" --repeat 1 --out eval-results/dk-reverse/base-repeat
target/release/evaluate diff eval-results/dk-reverse/base eval-results/dk-reverse/base-repeat --fail-on-drift
nice -n 15 target/release/evaluate run --suite gold --index "$INDEX" --repeat 1 --out eval-results/dk-reverse/gold-base
nice -n 15 target/release/evaluate run --suite gold --index "$INDEX" --repeat 1 --out eval-results/dk-reverse/gold-base-repeat
target/release/evaluate diff eval-results/dk-reverse/gold-base eval-results/dk-reverse/gold-base-repeat --relative --fail-on-drift
```

The Danish and full-suite unchanged-binary repeats have zero response/class drift. Local timing changes are not speed evidence: even the identical-binary full-suite repeat flagged reverse total +46%, p50 +30%, p90 +43%, p99 +58%. The absolute full-suite gate fails on pre-existing must-pass/critical results on this older index; the relative comparison has no newly failing or worse cases. Do not describe the absolute suite or measured speed as passing.

### Current Danish results

The DAR corpus has 1,883 query variants at 1,358 source-ID location groups. At-point reverse passes 1,166 variants / 803 locations; 713 variants / 553 locations are `wrong_place`, and four variants / two locations are `wrong_house`. Offset reverse passes 1,168 variants / 805 locations, with 715 variants / 553 locations `wrong_place`. Search passes 1,870/1,883 and suggestions 1,879/1,883. These are source-fidelity diagnostics, not independent population accuracy estimates.

Training comprises 1,495 variants / 1,086 locations. Held-out aggregates, not used to select changes, are 237 passing and 151 `wrong_place` variants in **each** reverse channel: 158 passing and 114 failing locations out of 272. All 388 held-out forward variants pass. An initial unfiltered helper search exposed held-out rows; subsequent case-level probes were guarded to reject held-out locations. No runtime tuning was performed from those rows.

### Training failure triage

Counts below are **variants / unique locations**, identical before and after the test/documentation change. They are disjoint diagnostic buckets, with house failure first, then indexed ancestry conflict, then name equivalence, then unresolved source-name semantics. They are not a count of independently verified geographic errors. Every at-point `wrong_place` has the source street, number and postcode and lies within 3.7 m of the DAR point; it passes the house-only proximity check; locality scoring runs before house scoring, so this was checked separately.

| Diagnostic bucket | At-point | Offset |
|---|---:|---:|
| Rendered county conflicts with the selected house's containing indexed county polygon | 85 / 56 | 87 / 57 |
| Expected name is an indexed alias of a rendered locality | 41 / 28 | 41 / 28 |
| Expected name is an indexed alias of a rendered administration | 15 / 14 | 14 / 13 |
| Only postal-town expectations remain; geographic membership unestablished | 285 / 224 | 287 / 225 |
| Supplementary/postal expectations remain; geographic membership unestablished | 136 / 117 | 135 / 116 |
| Wrong house at the source point | 4 / 2 | 0 / 0 |
| **All failing checks** | **566 / 441** | **564 / 439** |

An ancestry conflict compares the selected house's own point, not the request point, against `Areas::containing` from the same index and the rendered county from its parent chain. This identifies inconsistent index evidence; it does not prove the replacement geographic settlement. Four training locations with passing at-point name checks also have a county conflict, outside the failure table. Name-equivalence buckets require an unambiguous Danish rendered name in `places.tsv`, whose primary or stored language names match an expected name after Danish/Latin folding. They do not add aliases or neighbouring settlements to the corpus. Examples include Helsingør/Elsinore, København/Copenhagen and Nykøbing M/Nykøbing Mors. Administrative aliases are reported separately: agreement with a municipality does not establish a settlement.

Johan Krohns Vej 9 illustrates the ancestry conflict: its house point and stored postal town Valby are correct, its indexed parent names Frederiksberg and Frederiksberg Municipality, but its own point's indexed county polygon names Københavns Kommune. The DAR road-authority code is not evidence of physical municipality, and that county polygon alone cannot establish Valby settlement membership. Replacing the locality with either the postal town or the nearest village would be unjustified.

The two at-point house failures are Søndre Tangvej 6 and Rynkevangen 8, each with two query variants: the nearest indexed houses are number 10, respectively about 10 m and 36 m from the source point. No duplicate-source explanation was established. Keep these house-coverage failures separate from locality identity; offset scoring deliberately permits a different house within tolerance.

For boundary controls, the source point and its 25 m offset fall in different indexed counties at one training location (Hf. Grænsen 32C); the selected houses fall in different counties at two locations (that location and Bispeengen 17). Both checks pass at all three query variants involved. These are indexed-boundary observations, not failures requiring a repair, and not proof of surveyed boundaries. No failing check was certified as a legitimate settlement-boundary crossing: the available evidence cannot settle that membership, so such cases remain unresolved rather than counted as runtime defects.

### Name provenance and the stopping point

- `nearby.rs::reverse_houses` chooses number-FST houses by distance. `house.rs::house_hit` obtains the street document's indexed parent and house-scoped number, coordinates and postcode slot. It does **not** substitute the nearest locality returned by `Index::reverse`.
- `search.rs::render_with_suburb` and `address` render locality and administrative names from that parent chain, preferring English names for reverse. `house_chain` reconciles a conflicting same-country **region** from polygon containment; it does not resolve conflicting counties within the same region or establish a new settlement. `areas.bin` contains county-and-above polygons, not settlement boundaries.
- The builder's `geo.rs::place_in` uses a containing locality polygon when available, otherwise nearby settlement points. That approximation can already be baked into the indexed parent; it is not repaired by running another nearest-point lookup at request time. Street-only reverse has its separate `street_place` refinement; it does not supply house locality names.
- Source `postal_city` (or the last `address_levels` name), OpenAddresses `city` and OSM `addr:city` become scoped postal metadata. `mod.rs::postal_town` rejects conflicting applicable names. Every training first house, including failures, already has raw `address.postal_town` equal to the pinned DAR postal town. Missing postal metadata is not the explanation in this sample.
- The raw index JSON has a distinct `postal_town`; protobuf `AddressDetails` currently has **no** postal-town field and conversion does not expose that raw value. Reusing `city`, `town`, `village` or `suburb` would relabel geographic information. This investigation makes no wire-contract change.
- DAR v4 generation 787 links `Husnummer.postnummer` to `Postnummer.navn` and `Husnummer.supplerendeBynavn` to `SupplerendeBynavn.navn`. Its access point, linked names and building identifiers are authoritative address facts, not settlement polygons or an audited settlement-membership crosswalk. The preparer projects these facts without establishing geographic town membership. Its retained road/postcode/supplementary-name relationship exports do not supply that missing geometry.

[#494](https://github.com/Mokto/opengeocoding/pull/494) uses an Australian house-and-point-scoped containing-suburb annotation with ancestor guards. Denmark has neither that annotation nor demonstrated equivalent authoritative membership in the pinned inputs; no Australian mechanism is copied.

A safe geographic recovery needs independently attributed settlement boundaries or address-to-settlement membership with explicit treatment of postal districts, supplementary names, aliases and conflicts. Until that evidence exists, preserve the selected house and geographic fields, expose no fabricated membership, and keep postal/alias evaluator mismatches distinct. **Actual geography improvements: zero; expectation changes: zero; previously failing checks relabelled as passes: zero.**

### Initial safeguards validation (before the runtime repair)

The candidate service and evaluator hashes are byte-identical to the base. The Danish rerun has no changed answers or classes; the complete 44,047-case gold comparison also has zero drift, no newly failing checks, no worse failure classes and no abstain-to-miss changes. All root-cause buckets and held-out aggregates above are unchanged.

| RPC | Passing before → after | Checked |
|---|---:|---:|
| SearchAddress | 36,735 → 36,735 | 43,824 |
| SuggestAddress | 38,120 → 38,120 | 47,360 |
| ResolveRegion | 7,849 → 7,849 | 8,037 |
| ReverseGeocode at-point | 36,018 → 36,018 | 43,446 |
| ReverseGeocode offset | 33,868 → 33,868 | 37,056 |

Formatting, all-target clippy, `cargo test --locked`, the ordinary synthetic suite and the separately invoked model-backed synthetic suite pass. The initial release link failed to find libpostal; `LIBRARY_PATH=/opt/homebrew/lib` resolved it. The initial clippy command selected an older nightly; invoking the installed Rust 1.99.1 `cargo-clippy` passes without suppressing warnings.

`INDEX_DIR=... cargo test --locked --test rpc -- --ignored --nocapture` was run with models and grpcurl installed. Both tests fail: `rpc_contracts` reaches the documented grpcurl SearchAddress timeout, and `search_diagnostics` fails its oversized-input log-truncation assertion. Neither assertion was changed, skipped or retried. These remain verification limitations, not passes.

The candidate's local speed section reports no slowdown, but the identical-binary repeat's contradictory slowdown above prevents a performance conclusion. This is not a measured CI quality/speed verdict. No runtime performance improvement is claimed.

## Runtime follow-up: official settlement polygons

A further source search found KDS's public **Bebyggelser_2024.zip**, linked from the [Danske Stednavne catalogue](https://dataforsyningen.dk/data/1038), under CC BY 4.0. Its `Byer` layer contains 8,006 polygon parts, including holes, with named settlement codes. [Statistics Denmark's definition](https://www.dst.dk/da/Statistik/dokumentation/statistikdokumentation/byopgoerelsen/indhold) explains that these built-up settlement polygons establish which addresses belong to the settlements. They are not postal districts or the whole of a same-named municipality. The summer-house layer is not imported.

This is new geographic evidence, not a reinterpretation of DAR's postal names. The locally available Overture releases have only 473/8,642 (September) and 461/8,640 (October) Danish localities with polygons; the builder already reads those polygons. A second nearest-point lookup would not fill that gap.

### Source and crosswalk

`data/dk-localities.NOTICE` records the licence, public archive URL and SHA256 `a56addf371a081c75f88e567040d4935ef5f1185ddf2d3d167289d32ee0bd16c`. The layer was dated 2024-03-18 and retrieved 2026-11-02. `data/prepare-dk-localities.py` verifies the archive before reading it, uses DuckDB spatial to transform EPSG:25832 to EPSG:4326 with `always_xy`, and stores all rings at 1e-7-degree precision without simplification or expansion. The delta-encoded gzip is 9,548,013 bytes, SHA256 `bf8bc9b3bd82f213b0764718aa28d13c17b7309770277174e5dd3f4b994e3912`. Regeneration with Python 3.9 and DuckDB v1.5.6 is byte-identical:

```sh
python3 data/prepare-dk-localities.py SOURCE/Bebyggelser_2024.zip /tmp/dk-localities.json.gz
cmp data/dk-localities.json.gz /tmp/dk-localities.json.gz
```

The builder joins a settlement code to an **existing Danish locality** only when the primary names agree after the index's folding and that locality's point lies inside a polygon of the code. More than one candidate omits the code rather than choosing a nearest point. Separate parts of the same code share the unique crosswalk. The world build links 6,160 polygon parts; unmatched/ambiguous settlements remain uncovered. It writes `dk-localities.bin` in the existing `Areas` layout, about 24 MiB, and a source notice. The file is memory-mapped, optional on older indexes, and read-only at runtime. No new places, streets or houses are imported. The default source is bundled and pinned; no serving-time network or new runtime dependency is introduced. `--denmark-localities JSON.GZ` supplies authored diagnostic fixtures in that same delta format.

### Runtime rule and limits

For a Danish reverse **house** result, `country/dk.rs` checks the selected house's own FST point against those polygons. Exactly one linked locality must contain it. Its geographic name must also agree with the separately stored postal town, allowing Danish postal direction districts such as `København NV`. Postal evidence alone never changes the locality. When postal and geographic towns differ, the original rendering remains; those cases need more evidence, not a distance heuristic.

A locality already in the indexed chain is left alone, including its valid English rendering. Otherwise the confirmed locality replaces the unsupported settlement name, with the confirmed indexed place's type and primary name. The old settlement is removed from the display, not retained as an invented suburb. After this repair, a containing indexed Danish county can replace the old county only with the same region code and no finer subdivision code to invalidate. This is not a general county repair: an already-correct settlement can still carry inconsistent county ancestry. County polygons also retain their existing approximately 55 m simplification limitation near borders.

House selection, coordinates, number, road, postcode, country and all region codes remain unchanged. Forward search, suggestions, locality-only reverse, street-only reverse and ResolveRegion do not use the new rule. This is a polygon lookup at request time, not the Australian house/suburb FST mechanism.

The source's 2024 boundary can be stale at newer developments. Rural points outside a polygon, unmatched names, ambiguous crosswalks/overlaps, distinct postal/geographic names and unsupported supplementary-name membership are not repaired. Native/exonym expectation mismatches are not fixed by renaming an already-correct settlement or widening expected names.

### Before and after on identical data

The branch was rebased onto main `018c2ce`. The shared index had meanwhile been rebuilt to main's `8c53269491f72efb`; it was read only. Candidate builds used this session's separate directories, the same local source recipe and no competing builder. The final builder and chart pin are **`ce904d2d446c68ba`**, with index metadata fingerprint `7e1105d1cd42`; its private `builder.hash.txt` is recorded alongside the runs, not written over the shared index's sidecar.

The unchanged base service and the candidate both read the same candidate index, models and corpus. A full unchanged-base rerun has zero drift. Base service SHA256 is `0da25d3074c762b8816e5d8850403ea6aa28c18a43d394d49e73b9b452e90cd6`; candidate is `59f571a3c106dbb27b16966f534fb08a6cef9e1b263565818f0a6c069036af70`. Models, reference data, historical memberships and Danish corpus have the digests recorded above. The expanded current full suite has 44,823 cases, SHA256 `e992bff1986eacb77695ad10790fcb9dcdd7347e67f12078c9287eb951bdeecb`.

| DAR reverse channel | Passing variants before → after | Passing locations before → after |
|---|---:|---:|
| At-point, all | 1,166 → 1,362 / 1,883 | 803 → 957 / 1,358 |
| Offset, all | 1,168 → 1,367 / 1,883 | 805 → 960 / 1,358 |
| At-point, training | 929 → 1,085 / 1,495 | 645 → 769 / 1,086 |
| Offset, training | 931 → 1,090 / 1,495 | 647 → 772 / 1,086 |
| At-point, held-out aggregate | 237 → 277 / 388 | 158 → 188 / 272 |
| Offset, held-out aggregate | 237 → 277 / 388 | 158 → 188 / 272 |

The initial training triage buckets remain diagnostic, not independent ground truth. Failing checks below are **variants / locations**, before → after. A pass gained from an administrative-alias bucket is a newly supported geographic settlement, not a change accepting the municipality's alias. The 41/28 training locality-exonym/alias failures stay unchanged.

| Initial training bucket | At-point failures before → after | Offset failures before → after |
|---|---:|---:|
| Conflicting indexed ancestry | 85/56 → 71/46 | 87/57 → 71/46 |
| Indexed name equivalence (locality + administration) | 56/42 → 43/30 | 55/41 → 43/30 |
| Postal-only, geographic membership unresolved | 285/224 → 179/141 | 287/225 → 179/141 |
| Supplementary/postal, geographic membership unresolved | 136/117 → 113/98 | 135/116 → 112/97 |
| Wrong house | 4/2 → 4/2 | 0/0 → 0/0 |

At-point locality information changes at 204 variants / 162 locations; 196 variants / 154 locations gain passes. Offset locality information changes at 208 variants / 164 locations; 199 variants / 155 locations gain passes. The remaining changed answers still have a name-expectation mismatch. All changed settlement identities are supported by the imported polygon and crosswalk; these are not independently surveyed counts of previously wrong houses or national accuracy. No native/exonym-only case was relabelled as a runtime fix. No expectations or known-failure labels changed.

The full-suite comparison has 196 additional at-point and 199 additional offset passes, with SearchAddress (37,335/44,600), SuggestAddress (38,698/48,136) and ResolveRegion (7,849/8,037) unchanged. No country outside Denmark changes. There are no newly failing checks, worse classes or abstain-to-miss changes. The absolute run still fails existing critical/must-pass cases; only the relative quality gate passes. Local timings are confounded (one comparison flags suggestion total +20%, p99 +36%; the final one flags reverse p99 +19%); CI measured speed is a separate outstanding check, not a local performance conclusion.

Formatting, all-target clippy, ordinary unit/synthetic tests (90 ordinary synthetic tests) and the separately invoked model-backed synthetic test pass. Docker checks and the Linux arm64 runtime image build pass; the image runs as uid/gid 1000 and its builder reports `ce904d2d446c68ba`. amd64 CI remains separate verification. Removing the runtime hook makes the new recovery regression fail; restoring it passes. Added checks cover source checksum/decoding, holes, wrong-country and ambiguous name crosswalks, a misleading village point, distinct postal/geographic names, preserved house fields, containing-county correction and a cross-boundary offset. The model-backed RPC command was run on the private index and again fails the documented grpcurl timeout and the diagnostic-log truncation assertion; neither failure was hidden or skipped.
