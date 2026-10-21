# New Zealand comma-separated localities

Investigation for [#195](https://github.com/Mokto/opengeocoding/issues/195), using the LINZ corpus from [#190 at `69fabda`](https://github.com/Mokto/opengeocoding/tree/69fabda651486f9623e38c5f19e534da5009a112). The first sections record the comma-locality repair; the unit-column follow-up below recovers exact unit keys from existing index inputs, without a unit-to-building fallback or a gold-data import.

## Confirmed defect and repair

In `number road, suburb/locality, town, New Zealand`, libpostal can absorb the locality into the road or combine both localities into one city. Examples include native Tāhunanui on Parkers Road and the complete range `10-12` on Banksia Place. Their exact requested house-number keys exist in the inspected index; losing the comma boundaries prevents lookup.

The repair parses the first part separately, without changing its requested number or unit. It requires:

- A recognized New Zealand country, without another parsed country or administrative/postcode constraint being discarded.
- A clean house/road parse, with the same house number and unit as the original parse.
- An exact indexed road and number, including the full suffix, range or slash number.
- The stated town as an indexed settlement ancestor, and the locality as either a settlement ancestor or the house's scoped source postal/locality evidence.

Only that corroborated house activates the corrected parse and raw lookup. The locality scopes the lookup but does not replace the returned settlement with a postal-city override. If the house or evidence is missing, the original parse and fallback ordering remain unchanged. The wire contract and SuggestAddress are unchanged. A broader attempt that rewrote the parse before checking the house changed structured fallbacks and failed the local regression gate; it was removed.

## Local comparison

Base: main `b6529ac`. Both sides use the same evaluator, models, reference data and full index. The corpus is #190's pinned 11,057-case suite, including its 545 NZ queries. Two base runs have zero answer/scoring drift across all four RPCs.

| NZ pattern class | Queries | SearchAddress base | SearchAddress candidate |
|---|---:|---:|---:|
| Plain | 129 | 111 | 116 |
| Suffix | 146 | 114 | 124 |
| Unit | 94 | 16 | 19 |
| Native/ASCII difference | 166 | 104 | 121 |
| Number range | 10 | 8 | 9 |
| Total | 545 | 353 | 389 |

SuggestAddress remains 226/545 and reverse country checks 545/545. The full 11,057-case comparison has no newly failing or worse-class cases; all 36 class changes are too-coarse to pass in NZ. Two already-passing cases also lose a fused city label: `Foxton Beach Foxton` becomes `Foxton`, and `Mangawhai Heads Mangawhai` becomes `Mangawhai`, with unchanged house points. Other previously passing responses are unchanged. The 1,000 unscored address cases have zero base/head response drift. Both sides differ from the older accepted snapshot; it was not regenerated. Expectations, tolerances, labels and hold-out groups were not changed. Inspection and diagnostic probes used only non-held-out groups; full-suite scoring includes hold-outs without using them to select a fix.

The NZ sample contains 428 location/hold-out groups, not 545 independent locations. Authority/pattern balancing and shared LINZ ancestry in the index limit interpretation: these counts are diagnostic fidelity checks, not national accuracy or independent coordinate validation.

The index is the documented last-known-good full build `4fb1fd818b46b2f6`, fingerprint `e6df26bed27b`, not a rebuilt current-builder index. The runtime change does not change the builder hash. The final local gold speed comparison fails at SearchAddress p99 (+13%, 3.6 to 4.1 ms); the other gated percentiles/totals pass. Timings on this shared, busy host are not an isolated speed verification or a speedup claim; CI quality and measured speed must be reviewed separately.

## Remaining failures after the comma repair

- **Literal units without indexed slash keys:** `1/19 Benchmark Drive` has no `1/19` key; building `19` is indexed about 3 m from the LINZ point, with Massey as its source locality. This change deliberately does not substitute building `19` or house `1` for the requested unit form.
- **Building/unit point disagreement:** `106/25 Charlesworth Drive` lacks the slash key, and indexed building `25` is about 221 m from the selected LINZ unit point, beyond the unchanged 150 m tolerance. The point discrepancy does not establish which source is defective. The source locality is Allenton; neither LINZ nor that street document supplies a postcode.
- **Unsafe blanket building-number substitution:** diagnostic probes for 75 non-held-out unit queries raised passes from 14 to 65, but introduced three too-coarse-to-near-miss regressions: Charlesworth Drive (221 m), Bob Robertson Drive (157 m) and Coleridge Street (156 m). These are not fixed by dropping the unit. Of those 75 queries, 72 have an accepted indexed full-unit or building key within 150 m; three have accepted building keys farther away. Different native/ASCII queries can share one location group.
- **Ancestry and locality limitations:** a source town is not necessarily an indexed administrative ancestor. The repair declines these cases rather than dropping the town, trusting a county alone, or searching arbitrary comma fragments.
- **Separate candidate-selection defects:** inspected Maddisons Road and Main Road/Fairfield queries can choose East Maddisons Road or Main South Road respectively, despite nearby exact-road houses. These require separate triage; this change does not claim to fix them.

No source-coordinate correction, rural delivery-route support, verified apartment identity or comprehensive NZ coverage is claimed.

## Regression checks

`nz_comma_alignment_keeps_numbers_units_and_geographic_guards` exercises the parse constraints without models. The synthetic world's model-backed resolver check covers Māori/ASCII locality forms, suffixes, complete ranges, full slash numbers, same-road houses in different source localities, missing numbers, and contradictory locality/town/country/region qualifiers. The base service returns only a locality for its three new positive number forms; the candidate recovers the authored houses.

```sh
cargo test --locked --lib nz_comma_alignment
cargo test --locked --test synthetic -- --ignored --nocapture
INDEX_DIR=/path/to/full-index cargo test --locked --test rpc -- --ignored --nocapture
```

Formatting, Clippy, `cargo test --locked` and the model-backed synthetic check pass locally. The model-backed RPC suite has one passing test and one failure: its CDG landmark assertion receives the same `Tunnel Cdgval` street with landmarks on or off. The unchanged base reproduces that failure on this older index; it was not weakened or fixed as part of the NZ change. Native libpostal models are required for the latter checks. The documented fresh-service grpcurl SearchAddress hang remains a known interoperability failure even when an individual evaluator run reports it passing.

## Exact-road follow-up to #200 (initial #204 head)

The next comparison uses merged main `5e96fbe` as its base. Token lookup can return a longer street name before the requested name: `116 Main Road, Fairfield, Dunedin, New Zealand` selects Main South Road about 4.1 km away, despite an indexed Main Road house within a metre of the LINZ point. `3/638 Maddisons Road, Rolleston, New Zealand` instead selects East Maddisons Road about 6.7 km away.

The follow-up prefers an exact normalized name among the existing bounded street candidates, before looking for the number. This is opt-in for the corroborated NZ comma repair and for three/four-part NZ queries whose first part is a clean house/road/unit parse agreeing with the whole parse. It does not drop qualifiers, change geographic scopes or substitute a building number. An exact street lacking the requested number remains a street fallback, with `goodMatch=false`. Street aliases also prefer an exact normalized display name over an equally overlapping longer alias.

Applying this preference to every parsed road was rejected: the full gate found four passing queries becoming too coarse, and one too-coarse answer becoming a near miss outside NZ. A parsed road can omit a legitimate name prefix or connecting words; a global exact-name rule is unsafe. The final candidate leaves those lookup paths unchanged.

Two complete base runs have zero answer/scoring drift on the same 11,057-case corpus, evaluator, models and last-known-good full index described above. The final comparison has only two changed answers:

- Main Road: near miss to pass, returning house `116` on Main Road at the source-backed point.
- Maddisons Road: near miss to too coarse, returning the correct street instead of East Maddisons Road. The full `3/638` key is still missing; this is not a unit-level pass.

SearchAddress passes rise from 8,511 to 8,512 of 10,978 scored checks. NZ plain rises from 116 to 117/129 and the NZ total from 389 to 390/545; suffix 124/146, unit 19/94, native/ASCII 121/166 and range 9/10 are unchanged. SuggestAddress, region and reverse answers are unchanged. There are no newly failing or worse-class cases, and all 1,000 unscored address responses are unchanged. The older accepted snapshot was not regenerated.

The local speed comparison reports no gated slowdown, but the identical-binary baseline repetitions themselves differ by 31% in forward total time. These runs were not isolated; they establish answer stability, not a speedup or stable latency. CI quality and measured speed remain separate gates.

The synthetic world adds a more prominent East Orchard Road sharing numbers with Orchard Road. Its model-backed check fails on the base, selecting East Orchard Road for an Orchard Road query, and passes with the repair. Checks cover abbreviations, suffixes, complete ranges, exact slash numbers and numbers found only on the longer road. The unit check covers Māori/ASCII aliases. Formatting, Clippy, unit tests and the model-backed synthetic check pass; the model-backed RPC check retains the same CDG landmark failure on both base and candidate. At this stage, missing unit keys and source-coordinate disagreements remained unresolved.

## Unit columns and street-cell lookup

The existing Overture `2026-09-23.1` input contains 362,823 NZ rows with nonempty ASCII alphanumeric unit identifiers. The builder projected `number` but omitted `unit`; its OpenAddresses reader likewise omitted `unit`. The same frozen OpenAddresses inputs also contain 1,631 Rotorua rows with units. The input rows for `1/19 Benchmark Drive`, `106/25 Charlesworth Drive` and `3/638 Maddisons Road` already carry the separate number/unit fields and the correct unit points. No LINZ gold snapshot or additional source was imported.

For NZ, the builder now adds `unit/number` alongside the original number in the same input row. This preserves the building keys, street weights and centroid contributions while retaining each unit's source point. It keeps complete suffixes and ranges; already combined numbers, number lists and non-alphanumeric unit labels do not acquire guessed aliases. Other countries keep their original number handling. The format of the index files is unchanged, but the builder identity changes from `a7481f0be4727233` to `9d4006a4eec15abb` and requires a rebuild. On these full inputs, the number-key count rises from 501,588,251 to 501,924,359.

The new synthetic source fixture also exposed a separate SuggestAddress lookup defect. It deduplicated a street's grid cells by name before checking which cell held the number. Suggestions now check the candidate cells first, then deduplicate returned streets/addresses. The existing candidate and geographic bounds remain in place. The fixture uses units hundreds of metres apart, tests separate source number/unit columns as well as a retained building key, and checks full suffix/range forms and missing-unit/endpoint guards.

### Initial separate code and index experiments (11,057 cases)

Both full indexes were rebuilt from the same unchanged Overture, OpenAddresses, OSM and GeoNames files. The base rebuild is fingerprint `4cfc1e4547bd`, the unit-key rebuild `74a1eec034f5`; these are successful current-builder builds, not the older index used above. Two base-runtime repetitions, and two suggestion-fix repetitions on the base index, have zero answer/scoring drift. A single identical runtime binary reads both offline index outputs for the index-only comparison. All runs use the same evaluator, models, reference data and 11,057-case corpus. `--no-direct` disables only the optional evaluator-side index diagnostics; all four RPCs, scoring and transport checks still run.

| Experiment | SearchAddress passes | SuggestAddress passes |
| --- | ---: | ---: |
| Existing runtime, rebuilt base index | 8,510 | 6,744 |
| Suggestion fix, same base index | 8,510 | 6,859 |
| Same fixed runtime, unit-key index | 8,582 | 6,894 |

The code-only gain is 115 full-string suggestions, including 12 NZ queries. The index-only gain is 72 NZ SearchAddress passes and 35 NZ suggestions. Both comparisons and their combined diff have no newly failing or worse-class cases; region and reverse answers are unchanged. The original two issue examples and Maddisons Road now pass with their exact slash numbers, `goodMatch=true`, and points within 2 m of the LINZ expectations. Charlesworth does not require a source-coordinate correction or a wider tolerance: its unit row already had the correct point, unlike the representative building key used by the rejected fallback probe.

NZ SearchAddress rises from 390 to 462/545, including unit 19 to 91/94; plain 117/129, suffix 124/146, native/ASCII 121/166 and range 9/10 are unchanged. NZ suggestions rise from 226 to 273/545, including unit 7 to 42/94. Three unit queries remain too coarse, and other locality/parser/coverage failures remain. One remaining unit query is held out and was not inspected for tuning. Shared LINZ ancestry and the 428 location groups still limit these results to diagnostic fidelity, not national accuracy or independent coordinate validation.

All 1,000 unscored address responses are unchanged between base and candidate. The 864 differences from the older accepted snapshot pre-exist on both sides; the snapshot was not regenerated. No expectations, tolerances, labels, hold-outs or corpus rows changed.

Formatting, Clippy, unit tests and model-backed synthetic checks pass. The model-backed RPC suite retains the same CDG landmark failure on both fresh indexes; `search_diagnostics` passes. Local code/index comparison speed tables pass, but the identical-runtime repetition itself fails speed (SearchAddress p99 +23% on the shared host). These runs establish answer stability, not stable latency or a speedup; CI quality and measured speed must be reviewed separately.

### Rebased 15,006-case gate

After `main` added the Taipei repair and AU/DK/ES corpora, the branch was rebased onto `7f3e72b` and all comparisons were repeated with that 15,006-case suite. The same two frozen-input indexes were reused because neither merge changed the builder. Both complete base runs, and both fixed-runtime runs on the base index, have zero answer/scoring drift.

| Experiment | SearchAddress / 14,927 | SuggestAddress / 15,009 |
| --- | ---: | ---: |
| Main runtime, base index | 11,949 | 7,696 |
| Fixed runtime, same base index | 11,950 | 7,830 |
| Same fixed runtime, unit-key index | 12,022 | 7,865 |

Relative to current main, this is 73 additional SearchAddress passes (the exact-road fix plus 72 unit cases) and 169 suggestions (134 from checking street cells, 35 from unit keys). There are no newly failing or worse-class cases. ResolveRegion remains 3,319/3,338 and reverse country checks 14,515/14,639. NZ SearchAddress is 389 to 462/545, including unit 19 to 91/94; suggestions remain the same 226 to 273/545 improvement. The 1,000-address comparison still has zero changed answers, without replacing the accepted snapshot.

One attempted base repetition lost its local service after 11,027 cases and recorded 3,900 connection errors; no panic was logged, and the process-exit cause was not captured. That failed run was retained, not resumed or counted as an accuracy comparison. A fresh baseline check and the subsequent comparisons ran without overlapping model-backed tests, with a launcher recording native-binary checksums and child exits. All those service processes exited successfully; the baseline's completed answers match its earlier successful full run. No evaluator request retries were added.

The final combined and index-only timing comparisons pass their local thresholds, but the code-only timing comparison fails on this shared host (SearchAddress total +17%, SuggestAddress total +19%, ReverseGeocode total +16%). These inconsistent timings do not establish a speedup or dismiss a slowdown. The updated PR still requires both current-head CI quality and measured-speed reports.

## Global panel follow-up ([#222](https://github.com/Mokto/opengeocoding/issues/222))

The 48-market panel samples OSM address nodes, whose `addr:city` is usually LINZ's town (`town_city`), not the suburb or locality. On a local world index built with GeoNames `allCountries` postcodes (as CI builds it), the base passed 66 of the 100 NZ searches. Of the 14 tuning groups with a failure, 11 already had the requested house within 150 m of the node: those were parsing, matching and suggestion defects, not coverage. Held-out groups were not inspected.

- **Town as well as suburb.** Overture's NZ `address_levels` are `[town_city, suburb_locality]`; the builder kept only the last. It now also records the town for NZ rows (postal-town source `town_city`), so "49 Foster Road, Riverton/Aparima" finds the house LINZ files under Thornbury, and "22 Brackenburn Drive, Ohauiti, Tauranga" one Overture places in Western Bay of Plenty District. The town is the house's postal city in the answer; its suburb and locality are unchanged. This changes the builder hash.
- **Three-part comma addresses.** "7 State Highway 30, Te Teko, New Zealand", where libpostal reads the road across the comma, goes through the same corroborated repair as the four-part form; it applies only when the whole parse disagrees with the head's.
- **Number after the road.** "Church Street 4, Amberley" (libpostal keeps "church street 4" as the road) is resolved again as "4 Church Street, Amberley" when the query as written finds no house, the road ends in a street type, and the turned query gives a good-match house with that number. Restricting it to typed roads and good matches kept "Calle Centenario 505", "Kgapane 13" and "Main Street 988, Springfield, CO" (whose number-first form finds Massachusetts) as they were.
- **Suggestions.** A postcode no NZ street holds no longer empties the result: the town (or, with none, the postcode's own place) places a house whose street holds no postcode. A suburb and town both named by the houses' source ("Ohauiti, Tauranga") scope the street. A contradicting postcode on a street that holds postcodes still blocks the address.

Local comparison: base `main` at `84ee593` on its index, head on the head builder's index, both from the same inputs (`build.sh` plus GeoNames `allCountries`), all 21,282 cases, one request each; two base runs have zero drift. No newly failing or worse-class case.

| | SearchAddress | SuggestAddress |
|---|---:|---:|
| NZ global panel (100) | 66 → 86 | 32 → 84 |
| NZ LINZ registry (545) | 462 → 534 | 273 → 534 |
| All cases | 17,805 → 17,925 / 21,203 | 15,303 → 15,696 / 21,287 |

Outside NZ, the number-after-road retry passes cases in IE, GB, ZA, SA, MY and others, and the postcode retry German and other suggestions on streets without postcodes.

Remaining NZ panel tuning failures: a house absent from the Overture snapshot (970 Maraetotara Road), source points 160 m and 1.1 km from the indexed house with only a postcode to place them (McVicar Road, Cloan Road), and a stated town LINZ does not use (Te Hāpua for a Cape Reinga / Te Kao house). Several LINZ registry misses are addresses missing from the Overture snapshot. Local speed tables are not comparable (the base ran cold on a busy host); CI's measured speed is the gate.
