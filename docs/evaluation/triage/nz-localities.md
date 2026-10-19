# New Zealand comma-separated localities

Partial improvement for [#195](https://github.com/Mokto/opengeocoding/issues/195), using the LINZ corpus from [#190 at `69fabda`](https://github.com/Mokto/opengeocoding/tree/69fabda651486f9623e38c5f19e534da5009a112). This is not a unit-number fallback or a new index-data import.

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

## Remaining failures

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

## Exact-road follow-up to #200

The next comparison uses merged main `5e96fbe` as its base. Token lookup can return a longer street name before the requested name: `116 Main Road, Fairfield, Dunedin, New Zealand` selects Main South Road about 4.1 km away, despite an indexed Main Road house within a metre of the LINZ point. `3/638 Maddisons Road, Rolleston, New Zealand` instead selects East Maddisons Road about 6.7 km away.

The follow-up prefers an exact normalized name among the existing bounded street candidates, before looking for the number. This is opt-in for the corroborated NZ comma repair and for three/four-part NZ queries whose first part is a clean house/road/unit parse agreeing with the whole parse. It does not drop qualifiers, change geographic scopes or substitute a building number. An exact street lacking the requested number remains a street fallback, with `goodMatch=false`. Street aliases also prefer an exact normalized display name over an equally overlapping longer alias.

Applying this preference to every parsed road was rejected: the full gate found four passing queries becoming too coarse, and one too-coarse answer becoming a near miss outside NZ. A parsed road can omit a legitimate name prefix or connecting words; a global exact-name rule is unsafe. The final candidate leaves those lookup paths unchanged.

Two complete base runs have zero answer/scoring drift on the same 11,057-case corpus, evaluator, models and last-known-good full index described above. The final comparison has only two changed answers:

- Main Road: near miss to pass, returning house `116` on Main Road at the source-backed point.
- Maddisons Road: near miss to too coarse, returning the correct street instead of East Maddisons Road. The full `3/638` key is still missing; this is not a unit-level pass.

SearchAddress passes rise from 8,511 to 8,512 of 10,978 scored checks. NZ plain rises from 116 to 117/129 and the NZ total from 389 to 390/545; suffix 124/146, unit 19/94, native/ASCII 121/166 and range 9/10 are unchanged. SuggestAddress, region and reverse answers are unchanged. There are no newly failing or worse-class cases, and all 1,000 unscored address responses are unchanged. The older accepted snapshot was not regenerated.

The local speed comparison reports no gated slowdown, but the identical-binary baseline repetitions themselves differ by 31% in forward total time. These runs were not isolated; they establish answer stability, not a speedup or stable latency. CI quality and measured speed remain separate gates.

The synthetic world adds a more prominent East Orchard Road sharing numbers with Orchard Road. Its model-backed check fails on the base, selecting East Orchard Road for an Orchard Road query, and passes with the repair. Checks cover abbreviations, suffixes, complete ranges, exact slash numbers and numbers found only on the longer road. The unit check covers Māori/ASCII aliases. Formatting, Clippy, unit tests and the model-backed synthetic check pass; the model-backed RPC check retains the same CDG landmark failure on both base and candidate. Missing unit keys and source-coordinate disagreements remain unresolved.
