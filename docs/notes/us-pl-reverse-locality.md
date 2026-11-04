# US and Polish reverse-locality investigation, 2026-11-03

Follow-up evidence for [#345](https://github.com/Mokto/opengeocoding/issues/345), not a resolution of it.

## Initial investigation and scope

The initial investigation added only synthetic safeguards and a reproducible baseline, with no runtime repair. Replacing a selected house's locality with its postal town or nearest settlement would lack membership evidence. **The runtime follow-up below instead preserves geographic PRG source evidence at the selected house.** The initial results remain as historical baseline evidence.

The Australian fix [#494](https://github.com/Mokto/opengeocoding/pull/494) uses a house/point-scoped containing-suburb file and fills a missing name. The Danish fix [#507](https://github.com/Mokto/opengeocoding/pull/507) uses independently pinned settlement polygons, an unambiguous locality crosswalk and postal corroboration. Neither establishes US or Polish settlement membership. The Polish case exposes loss of geographic source information, not a reason to equate all postal and geographic names.

The initial safeguards changed no corpus expectations, labels, held-out assignments, numbers, source priorities, builder inputs, builder hash or chart pin. Country-specific runtime changes, if justified later, belong in `us.rs` and `pl.rs` and in separate, non-stacked PRs.

## Baseline identity and provenance limits

Fresh main: `9fe4c84e8087df722087fec53071e1e98512068d`. A separate worktree and its own `target/` were used. Active sessions and open PRs were checked first; no US/Polish locality implementation was open.

The shared `$HOME/opengeocoding-index/out` advertised builder `24e45dcd8295f18b`, not main's `ce904d2d446c68ba`; it was not rebuilt, renamed or deleted. The existing full `out-ce904d2d446c68ba-last-known-good` was selected read-only. Its name advertises the same hash printed by the fresh main builder and chart. That is not a cryptographic producer attestation: this local index has no sealed source-snapshot manifest. All comparisons use the **same directory**, not separately rebuilt indexes.

| Identity | SHA256 / version |
|---|---|
| Service | `24484e03211c3bed028b684156a129c0ce289b899636281f3e9f89a27f06f8a3` |
| Evaluator | `bda4612851bf900b56921a58fce99d1f0c399a9490a16edfa231a1049c44cde3` |
| Index `meta.json` | `11a7c13d7e1827f00f6ffe197cfb8eced65d3cc1d194b0feb22dbbd3d1457ea8` |
| Libpostal models, evaluator fingerprint | `65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7`, default bundle, `data_version=v1` |
| Reference data, evaluator fingerprint | `9a68f034f3b05f57aecdec9776b06c63a15cf48619703a6fd797bb61ce7cfb01` |
| Historical memberships | `16871dd9c5b3dc74b8a9262f7986e2b35abde43231f6f72e9d643cfa4dc665fc` |
| Complete gold corpus fingerprint | `60de35f2ca5d4bbd31248c3b9c7cd18a9b5fb5018b441bfcaf189a9f4ace2b84` |
| US corpus | `5d8f11ac4d60e6cf3b07e330587a4a2b4107e025b7f3d1c874faef49f5fa1b37` |
| Polish corpus | `40acbd45912bc55d37be4efaf6dd459429c8c5d3bc5f2a96a4d2131f1ffffd88` |
| Curated `gold.jsonl` | `b41b2455fb85d1b5e65efdd8bb574dcf1682c36538b2da94c7bb8f17ba968831` |
| Global OSM panel | `c90cfecb5803d410331289d983d05dad75062d9ea49a70bc0f8a88d2a9b78b2b` |
| Retained NAD r24 text | `8a1ed2c2b9ec0f9e41094dc6594b0ff9c6716acfed3c1e7ed0911e3746c8089d` |

Source tracing used the retained Overture `2026-09-23.1` address, division and division-area parquet: 73 files, approximately 31 GiB. The saved relative-path SHA256 inventory itself hashes to `4f549dbdd86a73ac77dffada0209cf0cff91a12f41e93ca441c332f388c754b7`. The available inputs are recorded, not asserted to be a sealed inventory of every source that produced the index. The Polish address rows in these parquet files all identify OpenAddresses' sixteen province datasets, versions `910925`–`910940`, under `LicenseRef-Public-Domain-PL-Geodesy-Law`; the examined division provenance names the OSM `2026-09-16` planet. The independent PRG capture is the 2026-10-19 archive and normalized capture pinned in [Poland's guide](../evaluation/registry/pl.md), not an input newly imported here.

Reproduction, from the checkout root, with `INDEX` set to that full index:

```sh
LIBRARY_PATH=/opt/homebrew/lib nice -n 15 cargo build --locked --release \
  --bin opengeocoding --bin evaluate --bin build_index
nice -n 15 target/release/evaluate run --suite gold --index "$INDEX" \
  --repeat 1 --no-direct --out .pi/reports/us-pl/base
nice -n 15 target/release/evaluate run --suite gold --index "$INDEX" \
  --repeat 1 --no-direct --out .pi/reports/us-pl/repeat
target/release/evaluate diff .pi/reports/us-pl/base .pi/reports/us-pl/repeat \
  --relative --fail-on-drift
```

Both full runs completed 46,407 cases; both absolute gates failed on existing must-pass, untriaged critical, landmark flag-off and transport checks. The repeat diff exits zero with **zero answer or class drift across every RPC**. Standalone `--corpus tests/registry/us.jsonl` and `pl.jsonl` runs both pass their absolute gates and agree with the corresponding full-run answers. `--no-direct` skips the evaluator's forward-index diagnostic probes, not the service RPCs. Training reverse failures were instead inspected through `evaluate inspect`, which rejects held-out cases before opening the index.

## Results: unchanged before and after safeguards

Numbers below are passing **query checks**, not independent locations or national accuracy estimates. At-point and offset are separate. Country filters here require exactly `["us"]` or `["pl"]`; sovereign alternatives for US territories are not silently included. Curated covers every matching case in the remaining gold inputs, not just one cohort.

| Country / evidence | At-point pass / checked | Offset pass / checked |
|---|---:|---:|
| US NAD | 606 / 850 | 626 / 850 |
| US curated | 485 / 496 | 335 / 338 |
| `global_osm_us` | 64 / 100 | 78 / 100 |
| Poland PRG | 730 / 927 | 740 / 927 |
| Poland curated | 12 / 12 | not applicable |
| `global_osm_pl` | 92 / 100 | 100 / 100 |

A location passes below only if **all its checked variants** pass that mode. The NAD source-ID groups number 427 training and 123 held-out; PRG has 430 training and 130 held-out. Global OSM has 40 training and 10 held-out groups per country. These are source groups, not independently surveyed buildings. Different cohorts and modes can reuse a group.

| Evidence / split | At-point passing locations / checked | Offset passing locations / checked |
|---|---:|---:|
| NAD training | 302 / 427 | 312 / 427 |
| NAD held-out | 91 / 123 | 95 / 123 |
| PRG training | 330 / 430 | 334 / 430 |
| PRG held-out | 96 / 130 | 98 / 130 |
| OSM US training | 24 / 40 | 31 / 40 |
| OSM US held-out | 8 / 10 | 8 / 10 |
| OSM Poland training | 38 / 40 | 40 / 40 |
| OSM Poland held-out | 8 / 10 | 10 / 10 |

Held-out material was aggregated only, not inspected for tuning. Raw reports remain ignored; per-case source and ancestry inspection was restricted to training IDs.

### Registry failure classes

Each cell is query checks / unique location keys. Some damaged US queries reuse the corresponding base address, so classes are not independent samples.

| Country / split / mode | wrong_place | wrong_house | wrong_postcode | wrong_region |
|---|---:|---:|---:|---:|
| US training at-point | 174 / 113 | 18 / 12 | 0 | 0 |
| US training offset | 176 / 114 | 0 | 0 | 1 / 1 |
| US held-out at-point | 47 / 28 | 4 / 3 | 0 | 1 / 1 |
| US held-out offset | 46 / 27 | 0 | 0 | 1 / 1 |
| PL training at-point | 139 / 96 | 6 / 3 | 1 / 1 | 0 |
| PL training offset | 139 / 96 | 0 | 0 | 0 |
| PL held-out at-point | 48 / 32 | 3 / 2 | 0 | 0 |
| PL held-out offset | 48 / 32 | 0 | 0 | 0 |

### US training cohorts

Counts are queries / distinct location keys. Location keys overlap between cohorts and between failure classes; do not add them as independent samples.

| Cohort | Mode | Pass / checked queries | Failure class: queries / locations |
|---|---|---:|---|
| address | at-point | 93 / 94 | wrong_house: 1 / 1 |
| address | offset | 86 / 87 | wrong_house: 1 / 1 |
| ambiguous | at-point | 10 / 10 | none |
| border | at-point | 2 / 2 | none |
| city | at-point | 64 / 66 | wrong_place: 2 / 2 |
| degraded | at-point | 174 / 176 | wrong_house: 2 / 1 |
| degraded | offset | 174 / 176 | wrong_house: 2 / 1 |
| fallback | at-point | 6 / 6 | none |
| global_osm_us | at-point | 48 / 80 | wrong_postcode: 8 / 4; wrong_house: 14 / 7; no_house: 10 / 5 |
| global_osm_us | offset | 62 / 80 | wrong_house: 8 / 4; no_house: 10 / 5 |
| people | at-point | 1 / 1 | none |
| postcode | at-point | 1 / 1 | none |
| rural | at-point | 29 / 33 | wrong_place: 4 / 4 |
| rural | offset | 1 / 1 | none |
| us_broad | at-point | 90 / 116 | wrong_place: 26 / 26 |
| us_broad | offset | 90 / 116 | wrong_place: 26 / 26 |
| us_degraded | at-point | 162 / 229 | wrong_place: 61 / 61; wrong_house: 6 / 6 |
| us_degraded | offset | 167 / 229 | wrong_place: 62 / 62 |
| us_house_rural | at-point | 46 / 73 | wrong_place: 26 / 26; wrong_house: 1 / 1 |
| us_house_rural | offset | 47 / 73 | wrong_place: 26 / 26 |
| us_house_suburban | at-point | 49 / 71 | wrong_place: 22 / 22 |
| us_house_suburban | offset | 48 / 71 | wrong_place: 22 / 22; wrong_region: 1 / 1 |
| us_house_urban | at-point | 69 / 98 | wrong_place: 28 / 28; wrong_house: 1 / 1 |
| us_house_urban | offset | 70 / 98 | wrong_place: 28 / 28 |
| us_unit_rural | at-point | 6 / 7 | wrong_house: 1 / 1 |
| us_unit_rural | offset | 6 / 7 | wrong_place: 1 / 1 |
| us_unit_suburban | at-point | 15 / 22 | wrong_place: 4 / 4; wrong_house: 3 / 3 |
| us_unit_suburban | offset | 18 / 22 | wrong_place: 4 / 4 |
| us_unit_urban | at-point | 27 / 40 | wrong_place: 7 / 7; wrong_house: 6 / 6 |
| us_unit_urban | offset | 33 / 40 | wrong_place: 7 / 7 |

### PL training cohorts

Counts are queries / distinct location keys. Location keys overlap between cohorts and between failure classes; do not add them as independent samples.

| Cohort | Mode | Pass / checked queries | Failure class: queries / locations |
|---|---|---:|---|
| border | at-point | 2 / 2 | none |
| city | at-point | 7 / 7 | none |
| global_osm_pl | at-point | 76 / 80 | wrong_house: 2 / 1; wrong_postcode: 2 / 1 |
| global_osm_pl | offset | 80 / 80 | none |
| pl_compound | at-point | 92 / 113 | wrong_place: 21 / 15 |
| pl_compound | offset | 92 / 113 | wrong_place: 21 / 15 |
| pl_other_road | at-point | 90 / 111 | wrong_place: 18 / 13; wrong_postcode: 1 / 1; wrong_house: 2 / 1 |
| pl_other_road | offset | 93 / 111 | wrong_place: 18 / 13 |
| pl_plain | at-point | 106 / 119 | wrong_place: 13 / 9 |
| pl_plain | offset | 106 / 119 | wrong_place: 13 / 9 |
| pl_suffix | at-point | 123 / 131 | wrong_place: 6 / 5; wrong_house: 2 / 1 |
| pl_suffix | offset | 125 / 131 | wrong_place: 6 / 5 |
| pl_village | at-point | 63 / 110 | wrong_place: 47 / 32 |
| pl_village | offset | 63 / 110 | wrong_place: 47 / 32 |
| pl_village_suffix | at-point | 87 / 123 | wrong_place: 34 / 22; wrong_house: 2 / 1 |
| pl_village_suffix | offset | 89 / 123 | wrong_place: 34 / 22 |
| rural | at-point | 1 / 1 | none |

## Training wrong_place triage

These are disjoint **diagnostic buckets by unique training location per mode**, not fully adjudicated causal labels. House mismatch takes priority at-point; an indexed locality-name match takes priority over a different selected number at the offset. A name in a county or region's aliases does not establish settlement identity and was excluded from the variant bucket.

| Observed bucket | US at-point | US offset | PL at-point | PL offset |
|---|---:|---:|---:|---:|
| Complete house number or point mismatch hidden by wrong_place | 9 | not an identity requirement | 3 | not an identity requirement |
| Expected name already occurs on an indexed locality/localadmin in the selected chain | 19 | 20 | 3 | 3 |
| NAD gives only a usable postal city, no usable incorporated municipality, Census place or unincorporated-community name | 43 | 35 | not applicable | not applicable |
| NAD geographic names and selected ancestry disagree; source/boundary conflict not adjudicated | 42 | 32 | not applicable | not applicable |
| PRG settlement absent from selected ancestry, with the expected complete house number | not applicable | not applicable | 90 | 50 |
| Another house selected at offset; its settlement membership unresolved | not applicable | 27 | not applicable | 43 |
| **Total wrong_place locations** | **113** | **114** | **96** | **96** |

For the 90 Polish at-point ancestry disagreements, 89 have the same PRG address UUID in the pinned Overture source and a selected house point within 1 m of that source record. This is strong source-identity evidence for a locality-only defect without moving the house. The remaining UUID is absent from that Overture snapshot and stays a source-vintage uncertainty. The 1 m figure is a tracing diagnostic allowing FST/protobuf quantization, not a proposed runtime assignment radius.

All 96 Polish at-point and 96 offset selected parent localities have land polygons containing the returned house point in the retained division-area input. A containing broad administrative territory is therefore not proof of the source's finer settlement. In the US NAD wrong_place sample, 36 at-point selected localities have such a containing polygon, 71 have no locality land polygon, and 6 have a polygon that does not contain the selected house; offset counts are 37, 71 and 6. Street inheritance can survive beyond an individual house's containing polygon.

The six additional US curated training wrong_place locations are all at-point: one city-name/rendering disagreement and five finer/broader or neighboring-locality disagreements. They remain source/evaluation-membership questions, not an extra runtime-fix claim. No global OSM control failed with wrong_place: these controls do **not** supply a geographic locality expectation, so their pass rates cannot validate a locality repair.

No legitimate offset boundary crossing was independently established for a failing real case. A different number at 25 m is not evidence of a wrong nearest-house selection or of a boundary crossing. No missing authoritative alias was proven absent from the index. Indexed aliases are evidence to investigate, not blanket official equivalences.

## Where the names came from

The 419 registry mode/location traces, plus six curated traces, each identify one stored street document by complete number, road and returned point through the house FST. The trace follows its numeric `place` into `places.tsv`, including primary/English/alternative names and parents. This distinguishes a rendered label from a geographic assertion.

- `src/index/nearby.rs::reverse_houses` selects a house, then calls `house_hit` and `render_reverse`. It does not reassign the chosen house to the nearest locality point.
- `src/index/house.rs::house_hit` sets `city: None`, keeps the stored street/place and the selected number, point and postcode slot, and obtains separate postal metadata.
- `src/index/search.rs::render_with_suburb` renders that ancestry in English, with `house_chain` able to reconcile a conflicting region. For a placed US or Polish house, its postal town is not the geographic name. Neither country's reverse-house path calls the Australian or Danish repair.
- `src/bin/build_index/geo.rs::place_in` prefers a containing locality polygon; without one it can assign a nearby point within its existing 5 km envelope. Thus an inherited label can originate in an offline proximity guess even though runtime reverse-house selection does not use nearest-place assignment.
- `src/index/area.rs` retains county/region/dependency/country polygons for runtime region resolution. Those cannot establish which village lies within the region. Locality land polygons used during construction are not generally retained for runtime settlement membership.

### US: geography is not a mailing address

NAD distinguishes `Post_City`, `Inc_Muni`, `Census_Plc` and `Uninc_Comm`. An incorporated municipality, a Census-designated place and an unincorporated community are not interchangeable; a USPS mailing city proves none of their membership. NAD's `Unincorporated` placeholder is not a municipality name. A Census place is not necessarily incorporated, and a Census suffix omitted from a query does not turn a CDP into a municipality.

The 114 training UUIDs needed for the union of modes were read from retained NAD r24, keeping those columns separate. A postal-only expectation can reject a valid geographic locality. Conversely, the 42 at-point geographic-name disagreements cannot all be excused as postal semantics; municipal/Census boundaries, source vintages and selected-house provenance still need adjudication.

Existing indexed aliases account for some qualified names, but stripping "Town", "Township" or "City of" globally would merge distinct municipal identities, particularly namesakes in different states. Neither a state match nor higher population proves the intended entity. Missing membership evidence is not repaired with population ranking or a new distance cutoff.

### Poland: an exact official relationship is available upstream, not in the runtime index

PRG's `AD_PunktAdresowy.miejscowosc` references a specific `AD_Miejscowosc` through a local GML reference. Street/locality consistency is checked by the preparer. The captured locality includes its own identifier, SIMC, kind and `TERYT` municipality code. Those identify a **miejscowość** and its gmina; a gmina is not a village, and a powiat is not a settlement. Postal locality is a separate concept.

For the 96 training wrong_place source groups, 94 address UUIDs occur in the retained Overture address input. A concrete trace links the same UUID through PRG's explicit locality reference while Overture keeps the settlement's name as its final `address_levels` value. That demonstrates identifier compatibility for those matched address records only. It does not establish a universal Overture-place-to-SIMC or gmina-to-settlement crosswalk.

The builder maps `postal_city`, else the last `address_levels` value, into town metadata and remembers the origin string. All 96 inspected Polish selected street documents have `address_levels`-origin metadata. The geographic name has therefore not always vanished from the input; its **house-level binding and source identity have been lost**. SIMC/GML relationships do not survive into the runtime place table.

In `Sink::add`, a town is associated with a street's postcode when a postcode exists, and only otherwise with its house number. Stored `postal_town` tuples contain postcode, optional number, name and origin, not a house point or PRG UUID. `postal_town` rejects conflicting names but cannot prove that one remaining name belongs to a particular selected house: a second house can contribute it, or the selected source can omit it. Reusing that string as geographic membership would silently cross an unproven boundary.

## Experiments proposed by the initial investigation

1. **Separate evaluator experiment, no expectation changes here.** Report postal-only NAD names apart from geographic membership; test name equivalence only after authoritative entity/state evidence, not global qualifier deletion. For PRG, report verified native/exonym identity separately from settlement membership. Score offset locality against independently established membership of the selected house, or keep the original-town comparison explicitly diagnostic. Re-score saved results and review every changed class before proposing scorer changes.
2. **Polish offline provenance experiment.** Preserve typed geographic locality evidence from the source address record through source-priority selection, number normalization, duplicate handling and final packed house point. Use the existing metadata/sidecar machinery where possible, but bind evidence to the selected house, not its street/postcode. Reject conflicting/missing IDs, ambiguous namesakes and region conflicts. Establish source licences, full-snapshot provenance and identifier compatibility before import; the gold sample must never become runtime lookup data. This requires builder work and compatible indexes from identical snapshots, not a runtime postal substitution.
3. **US membership experiment.** Adjudicate municipal/CDP/community disagreements with appropriate boundaries or exact source address relationships, preserving state/entity identity. Validate any new official input's licence, vintage and IDs in the offline builder. Do not project municipal, CDP or postal semantics globally.

The initial investigation imported no new official input and left the `ce904d2d446c68ba` builder/chart pin unchanged. The follow-up below implements the Polish offline-provenance experiment and updates the builder pin, while leaving US runtime and scoring experiments separate.

## Initial safeguards validation

The authored fixtures in `tests/synthetic/world/us.rs` and `pl.rs` cover a misleading closer locality point, distinct postal/geographic names, a boundary crossed by a 25 m offset, a conflicting source region column, a much larger namesake, complete compound numbers and preservation of the chosen house, road, postcode, country, region and packed coordinates. These pass on main: they guard existing correct behavior and **are not regressions demonstrating a new runtime fix**.

Validation performed:

- `cargo fmt --check`: pass.
- `cargo clippy --locked --all-targets -- -D warnings`: initially blocked by the shell selecting Clippy's Rust 1.97 nightly; pass with `RUSTUP_TOOLCHAIN=homebrew` (Rust 1.99.1), not by weakening warnings or the minimum compiler.
- `cargo test --locked`: pass, including 102 ordinary synthetic tests.
- `cargo test --locked --test synthetic -- --ignored --nocapture`: pass separately, one model-backed test.
- `INDEX_DIR="$INDEX" cargo test --locked --test rpc -- --ignored --nocapture`: the two existing failures reproduce: grpcurl SearchAddress timeout and the search-diagnostics log-truncation assertion. No assertions skipped or relaxed.
- The first release link needed `LIBRARY_PATH=/opt/homebrew/lib`. Initial new-fixture checks caught missing street registration and the use of `hamlet` instead of the rendered `village` key; both fixture mistakes were corrected, without runtime edits.
- Standalone US and PL evaluations: absolute pass. A third full gold run after the safeguards has the **same service binary, corpus, index, model and reference fingerprints**, the same absolute failures and zero answer/class drift against baseline. The relative `--fail-on-drift` diff exits zero. No newly failing, worse-class or abstain-to-miss transitions occur anywhere.

Full-suite passing checks are unchanged: SearchAddress 37,773 / 46,184; SuggestAddress 39,699 / 49,720; ResolveRegion 7,849 / 8,037; reverse at-point 38,484 / 45,806; reverse offset 36,255 / 39,416. House identity and coordinates are unchanged everywhere because the service binary and index are unchanged; the added fixtures check them explicitly. No claim of a locality-accuracy improvement follows.

The initial-stage reports and filtered training evidence remain under ignored `.pi/reports/us-pl/`. That stage created no full index and changed no runtime behavior. Its local timings are not speed evidence; its validation and provenance limitations are recorded rather than counted as passes.

## Runtime follow-up: retain PRG settlement membership at the selected house

The follow-up repairs a loss of geographic information in the builder. It does **not** promote `postal_town` to a settlement. No US runtime rule or evaluator change is included.

### Source contract and offline annotation

The sixteen OpenAddresses source definitions at [commit 76ac33513648f1c050f2d09c2bfdc0376f2514f6](https://github.com/openaddresses/openaddresses/tree/76ac33513648f1c050f2d09c2bfdc0376f2514f6/sources/pl) all map `CITY` from PRG's **miejscowosc**, `ID` from `lokalnyId`, `NUMBER` from `numerPorzadkowy`, and postcode separately from `kodPocztowy`. They describe the PRG-derived 2026-06-03 CSV capture, not a USPS-style mailing city. That earlier source vintage is distinct from the evaluation's October PRG capture. The definition lookup is supporting provenance evidence, not a claim to have independently checksummed the upstream CSV archive.

The pinned Overture input contains 8,584,407 Polish address rows, all with three address levels. For this source contract these are powiat, województwo and miejscowość. The builder requires exactly that shape, a province name consistent with the dataset, one whole-record `open_addresses` provenance entry from a recognized Polish province, its declared Polish-geodesy-law licence, a UUID-shaped source ID, version and update time. Unsupported provenance, missing city levels and inconsistent provinces do not supply membership.

`src/bin/build_index/poland.rs` reads only the installer's existing offline Overture input. It records the geographic name at the source address's packed point and **complete** normalized number, rejecting conflicting names at that key. A source record names the geographic settlement of its own address point; no proximity assertion is involved. After the ordinary number/source-priority pass has chosen houses, the annotation can attach only to an identical final packed point and full number.

A target must be exactly one indexed Polish locality whose primary name matches the source, whose region agrees with the source province, and whose indexed ancestry includes the house's current parent. An ambiguous descendant, sibling, different-province namesake or missing locality is omitted, not ranked by distance or population. A source name already present in a locality's indexed names in the existing chain is left alone. The rule does not invent a SIMC/TERYT-to-Overture entity crosswalk.

The builder writes 1,510,342 annotations into optional `pl-localities.fst`, about 38 MiB. Its house/number/point key reuses the Australian annotation's existing encoding, moved unchanged to `schema::house_point_key`; the Australian output file is byte-identical. `poland-source.json` records the processed datasets, versions and update times. `POLAND-LOCALITIES-NOTICE.txt` retains GUGiK/PRG attribution and the source contract; it and `data/pl-localities.NOTICE` ship with the index/image. No new registry download, dependency, runtime write or startup network request is added.

For Polish reverse **house** results only, `country/pl.rs` checks the exact annotation, descendant relation, country and rendered region code. It replaces unsupported inherited settlement fields with the confirmed indexed locality. It does not retain the old name as an invented suburb or relabel it as a municipality. County, country, region codes, postal metadata, chosen house, road, full number and point remain as they were. Search, suggestions, ResolveRegion, street-only and locality-only reverse do not use the annotation.

### Controlled indexes and versions

The first implementation comparison used main `9fe4c84`. After the Korean import and Greenland/Hungary evaluation changes merged, the branch was rebased onto fresh main **`66e8e81`** before publishing. The final baseline builder is `24e45dcd8295f18b`; the final builder/chart pin is **`06296ece2308263c`**. Both baseline and candidate indexes and all validation were regenerated after the rebase. Both were built against one session-owned APFS copy-on-write snapshot of the same sources used by the local build recipe: Overture 2026-09-23.1, OpenAddresses, OSM address/landmark extracts, Korean shops, GeoNames populated places and GB/CA/NL postcodes. All 5,841 frozen input files were SHA256-inventoried: 214,109,165,285 bytes, relative-path inventory SHA256 `50a8f9de5eeb23460f45311d82e5ee2f72f072b71d9c1befc5ff66e85feaeab5`. No other session's index, sources or builder sidecar was overwritten.

| Final experiment identity | SHA256 |
|---|---|
| Main service | `d42fd63726d886de142b62f00aa4d59b7788e37cc4008f6bc178cbf5a3572181` |
| Candidate service | `65adab1d8f8867031bc055825fbadbd6e52da98dc1fde77f5fc02fc3e6d4df31` |
| Candidate index `meta.json` | `b0005fbd470716d6aa2aca982daa743bc07b61649dbb71e0f0d82120e9f9fd54` |
| Evaluator, same binary on both sides | `c42307b1f568d1d74cf92e1b87b48831aa3a266e7bb187dadf3b6c912536b6e8` |
| Full corpus after upstream evaluation changes | `f3302a6ed5989e2ac14b7c996b1de9c42493071fb7d4afb85e4d98a63f85793b` |
| `pl-localities.fst` | `a656b64a78be28b7413d959a8f24b6456661c30569b7e550da3a08387ee58a75` |
| `poland-source.json` | `5be37ff0faa64b75d240d9fb6d92e1c1e9104ce7bb75109df382e8d6a465b599` |

The final evaluator and corpus fingerprints are in this table, identical on both sides. Models, reference data and the US/Polish corpora retain their initial fingerprints. The upstream Greenland/Hungary scoring/expectation changes are included equally in baseline and candidate; this PR changes no scorer or corpus rows. Both the rebuilt baseline index and final candidate index were evaluated. The main service was also run against the candidate index, followed by the candidate service against **that identical index**, to isolate runtime locality changes.

The unchanged main repeat has zero answer/class drift. Comparing independently built indexes reveals seven street-only tied-road labels with unchanged points/classes, attributable to the existing packed-document tie ordering in `reverse_streets`; there are no changed house selections. These are not locality repairs. The strict same-index runtime comparison removes that build-order noise and changes no result outside Poland. Both the same-index and two-index relative gold diffs exit zero, with no newly failing, worse-class or abstain-to-miss transitions.

All 128 primary/far number FST files are byte-identical across the baseline and final builder, as are `places.tsv`, `areas.bin`, `au-suburbs.fst` and `dk-localities.bin`. On the identical index, an independent comparison of **85,222** reverse/offset records confirms identical answer layer, full number, point, road, postcode, country and region codes. Only locality information changes. No existing passing PRG or curated locality check is renamed; pass-to-pass name changes are confined to the OSM controls, which have no locality-name expectations.

### Before and after

These remain source-fidelity diagnostics with correlated query variants, not national accuracy estimates.

| Corpus / channel | Passing queries before → after | Passing source groups before → after |
|---|---:|---:|
| PRG at-point, all | 730 → 897 / 927 | 426 → 539 / 560 |
| PRG offset, all | 740 → 911 / 927 | 432 → 548 / 560 |
| PRG at-point, training | 561 → 686 / 707 | 330 → 416 / 430 |
| PRG offset, training | 568 → 695 / 707 | 334 → 422 / 430 |
| PRG at-point, held-out aggregate | 169 → 211 / 220 | 96 → 123 / 130 |
| PRG offset, held-out aggregate | 172 → 216 / 220 | 98 → 126 / 130 |
| NAD at-point | 606 → 606 / 850 | unchanged |
| NAD offset | 626 → 626 / 850 | unchanged |
| Curated Poland at-point | 12 → 12 / 12 | unchanged |
| Curated US at-point / offset | 485 → 485 / 496; 335 → 335 / 338 | unchanged |
| OSM Poland at-point / offset | 92 → 92 / 100; 100 → 100 / 100 | unchanged |
| OSM US at-point / offset | 64 → 64 / 100; 78 → 78 / 100 | unchanged |

Held-out information remains aggregate-only. It did not select rules or name mappings.

The original training-location buckets evolve as follows, without reclassifying expectations:

| Initial Polish wrong_place bucket | At-point after | Offset after |
|---|---|---|
| Settlement absent from selected ancestry | 86 pass, 4 wrong_place / 90 | 49 pass, 1 wrong_place / 50 |
| Different house selected at offset; membership unresolved | not applicable | 39 pass, 4 wrong_place / 43 |
| Hidden house/point mismatch | 2 wrong_house, 1 wrong_place / 3 | not an identity requirement |
| Indexed naming variant | 3 wrong_place / 3 | 3 wrong_place / 3 |

The four query checks that become `wrong_house` across training and held-out data expose an existing house mismatch after locality is corrected; their selected houses and numbers do not change. No error is relabelled as a pass to hide it. One held-out check in each channel has a changed name but remains `wrong_place`; its case details were not inspected or used for tuning. All US diagnostic buckets remain unchanged.

Across the complete 46,407-case gold suite after rebasing, passing locality/house at-point checks increase **38,340 → 38,507 / 45,662**, with 144/144 country/area-only checks unchanged, and offsets **36,255 → 36,426 / 39,416**. SearchAddress stays 37,657/46,040 plus 144/144 location-only checks; SuggestAddress stays 39,368/49,361 plus 215/215 flag-off and 144/144 location-only checks; ResolveRegion stays 7,849/8,037. Only Poland changes. The standalone affected-country runs pass their absolute gates. The full suite still fails its pre-existing must-pass, critical, landmark flag-off and transport checks; only the relative quality gate passes.

### Regression checks and remaining limits

Removing the Polish runtime hook makes the authored recovery regression fail with the inherited parent instead of the source-confirmed village; restoring it passes. Additional controls reject postal-only metadata, missing levels, inconsistent provinces, a different-province namesake, ambiguous descendants, conflicting geographic records, an unsupported provider and an averaged point that no source record supplies. An existing correct name is retained. The compound number, source point, road, postcode and all country/region fields are asserted separately.

Final formatting, all-target clippy, `cargo test --locked` (104 ordinary synthetic tests) and the separately invoked model-backed synthetic test pass. The final model-backed RPC command again reproduces the two known baseline failures: grpcurl SearchAddress timeout and the diagnostic-log truncation assertion. Local build/link setup and Cargo queue waits are recorded in the logs, including an interrupted queued release build; no queue was bypassed. The final completed release build and checks use Rust 1.99.1. The Docker manifest stage builds and contains all seventeen explicit builder-hash inputs, including the new Polish stub/notice. Full container checks and multi-architecture CI remain separate verification.

This is deliberately not recovery for sibling/uncrosswalked localities, changed source points, source-vintage conflicts, missing indexed names or unverified aliases. Exact-point annotation gives up recoveries rather than borrowing a neighbor's metadata. It does not certify legitimate real-world offset boundary crossings, building occupancy or rooftop accuracy. USPS mailing-city semantics and unresolved US geographic disagreements still need the separate experiments above. No expectations, failure labels or held-out assignments change.

CI quality and **measured** request speed remain required after publishing. Local warm/cold timings, including apparent large improvements with unchanged forward code, are confounded and support no speed claim. The offline source scan adds a build phase; it is not a request-latency measurement.

### Rebase onto main `ebf889a`

The rebase retains both the Canadian and Polish reverse-locality hooks and both countries' synthetic area/point-only fixtures. No Polish membership rule changes. The builder hash remains `06296ece2308263c`; no new index format or rebuild is needed for the rebase itself.

Formatting, all-target clippy, all ordinary tests (107 synthetic checks) and the separate model-backed synthetic test pass after rebasing. The model-backed RPC command against the shared full index again reproduces the two existing baseline failures. The full-index comparison above remains evidence for its recorded base `66e8e81`, not a newly measured comparison against `ebf889a`. No full gold run was repeated locally for this history-only update. Current-head CI gold quality and measured request speed still need verification.

## Why the first CI pass had no effect, and the October-layout fix

The rebased head `534bd93` passed [CI quality and measured speed](https://github.com/Mokto/opengeocoding/actions/runs/41769559591), but every RPC reported zero changed answers. That was a no-regression result, not evidence that the locality fix worked on the chart's input.

The adapter incorrectly assumed that the province was always the second address level. All **8,628,514** Polish rows in the local copy of Overture **2026-10-21.0** have one supported OpenAddresses source and three levels, but those levels are **[province, gmina, miejscowość]**, rather than September's **[powiat, province, miejscowość]**. The province check rejected every October row. The unchanged builder reproduces this with **zero annotations** on October input.

The corrected adapter requires exactly one of the first two nonempty levels to match the dataset's province, rejects another recognized conflicting province, and keeps the final geographic miejscowość field. It still requires the same supported whole-record provenance/licence/UUID, exact packed source point, complete number, unique descendant and compatible indexed regions. No postal substitution, source-version guess, source-identifier crosswalk, expectation change or held-out tuning was added.

The recovery fixture now uses October ordering; it fails under the old adapter and passes with the fix. The existing-name fixture retains September ordering. A small builder test also covers both orders, diacritics, absent/duplicated province, contradictory provinces and unsupported datasets.

### Controlled October experiment

Old builder `06296ece2308263c` and corrected builder/chart **`3cc9d538e47c5d86`** were built against one owned APFS-frozen input set: October Overture and the same local OpenAddresses/OSM/landmark/Korean-shop/GeoNames inputs used by the documented local recipe. This matches the relevant October layout, not every chart source or CI capture. The 5,834 input files total 215,430,030,305 bytes; the relative-path SHA256 inventory is `4e0ff599e9980030dd99bb2087357a4044fd55f3043447efe93a7719912fb030`.

The corrected builder writes **1,518,986 annotations** instead of zero. Their FST SHA256 is `6b14eeddd09190d464171b2158b31425c59c6255ed32fcecb34778018594bffb`; `poland-source.json` is `6415d390769b8b7776ddd49d9ff65f771ae3c7d785b69ea45203a3c50c7fb8e3`.

Both indexes were evaluated with the **same consumer binary**, models, reference data and corpus. An unchanged baseline repeat has zero answer/class drift. Both the full two-index quality comparison and a separate annotation-only comparison pass without new failures, worse classes or abstain-to-miss transitions. The annotation-only control is a copy-on-write clone of the corrected index with only its FST replaced by the old builder's empty one; it isolates the new geographic evidence from unrelated packed-document tie ordering.

| PRG mode/cohort | Passing query checks before → after | Passing source groups before → after |
|---|---:|---:|
| At-point, all | 738 → 908 / 927 | 431 → 546 / 560 |
| Offset, all | 740 → 911 / 927 | 432 → 548 / 560 |
| At-point, training | 566 → 692 / 707 | 333 → 420 / 430 |
| Offset, training | 568 → 695 / 707 | 334 → 422 / 430 |
| At-point, held-out aggregate | 172 → 216 / 220 | 98 → 126 / 130 |
| Offset, held-out aggregate | 172 → 216 / 220 | 98 → 126 / 130 |

These are correlated source-fidelity checks, not national accuracy estimates. One training check exposes an existing wrong house after its locality is corrected; its house/point/number does not change. Held-out reporting remains aggregate-only.

Across the full suite, locality/house reverse passes increase **38,393 → 38,563 / 45,662** and offsets **36,294 → 36,465 / 39,416**: **+170 and +171**, confined to Poland. Forward search, suggestions, regions, other countries and the curated/OSM control pass counts are unchanged. All 128 house-number FSTs, `places.tsv`, `areas.bin`, and the AU/DK sidecars are byte-identical between builders. The isolated comparison preserves house identity, point, road, full number, postcode, country and regions for all **85,222** reverse records; 392 raw records change locality information.

Formatting, all-target clippy, all ordinary tests (107 synthetic checks), the builder regression and the separately invoked model-backed synthetic test pass. The real-index RPC suite reproduces its two existing baseline failures. Full absolute gold still exits nonzero for existing failures; the relative quality checks pass.

Local speed tables report broad slowdowns even for unchanged forward code using the same binary and index. Those contended, unmatched local timings do not establish request-speed performance. The new head's CI quality and measured speed must both be checked; `534bd93`'s earlier pass does not verify this fix.

