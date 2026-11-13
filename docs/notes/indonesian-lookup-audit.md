# Indonesian lookup audit after hierarchy enrichment

## Scope

This audit follows the names-only [hierarchy supplement](indonesian-hierarchy.md).
It uses the frozen Overture `2026-09-23.1` index with builder
`99a4a5120d5f8e62`, plus the same other inputs and libpostal models. Diagnosis
covers only the training subset: 155 village and 247 district locations, grouped
by their stable location keys rather than counting query variants as places.
Held-out cases participate only in aggregate validation.

| Training locations with any failing check | Before | After lookup fixes |
|---|---:|---:|
| Village SearchAddress | 46 | 42 |
| Village full-query SuggestAddress | 59 | 55 |
| District SearchAddress | 84 | 84 |
| District full-query SuggestAddress | 165 | 163 |

Before the lookup fixes, the 59 village suggestion-failure locations included
four with a nearby named point and the requested district ancestry, 24 without
the requested district ancestry, 12 without an indexed name, 17 without a nearby
named locality and two with a matching names-only administrative node but no
nearby named locality. The 165 district suggestion-failure locations included
66 with names-only administrative nodes but no nearby matching point, 92 with
nearby settlements but no declared district point, three with only small
GeoNames points, two without an indexed name and two with point and ancestry
present. These are diagnostic name/radius categories, not surveyed identities;
a names-only administrative node is not a settlement or a district centre.

## Reproduced lookup failures

The four village locations with point and ancestry present were `Nicah Awe` /
`Nicahawe`, `Uram Jalan` / `Uramjalan`, `Babah Krueng` / `Babahkrueng` and
`Lubuk Raya` / `Lubukraya`. Their small indexed villages were absent from the
population-ranked completion FST. Ordinary term lookups required the spaced
words, so a later joined-name comparison never saw the intended candidate.
Street abbreviation folding also changes the separate word `Jalan`, unlike
that substring inside `Uramjalan`.

The labelled district completions were `Kec. Bogor Barat` and
`Kec. Cibeunying Kaler`. Indexed road names containing administrative text filled
the requested suggestion slots before the administrative rewrite ran. Neither
failure required adding a point or relaxing an ancestor qualifier.

## Changes and bounds

- Search can use an indexed joined village name only after checking the stated
  county and any stated district. An existing scoped match as typed wins.
  Explicit district labels do not become villages. Multiple matching villages
  abstain; the chosen native district spelling remains the parse's scope.
- Uniqueness is checked against the full bounded posting list, not the ordinary
  ten-result place lookup. A single folded term with at most 400 global postings
  bounds every matching Indonesian place document. Larger or multi-term lists
  abstain. An eleven-namesake authored fixture catches the truncated-list error:
  nine high-ranked matches outside the district must not hide a second lower-
  ranked match inside it.
- The regency fallback's name corroboration accepts complete joined/spaced
  equivalence without changing its geographic guard or confidence rules.
- Autocomplete tries at most the unlabelled and joined forms, nonrecursively,
  with Indonesian context and the original country, ancestor and level checks.
  Existing named suggestions retain priority; matching administrative places
  are considered before unrelated streets consume the remaining slots.

No source coordinates, administrative memberships, corpus expectations or
known-failure labels are changed. Unqualified name-only input does not acquire
an inferred Indonesian country. Missing points, conflicting memberships and
unrecognized or ambiguous names remain coverage limitations, not reasons to
ignore qualifiers or borrow another settlement's point.

## Consumer-only validation

On the same enriched full index, 46,320 shared gold cases compared with a
zero-drift baseline control gained 20 SearchAddress checks and 20 SuggestAddress
checks, with no newly failing checks or worse failure classes. Only Indonesian
answers changed. Reverse, region, offset reverse and flag-off checks were
unchanged. The independently selected Indonesian place panel changed from
972 to 992 of 1,394 search checks and 1,121 to 1,141 of 1,853 suggestion checks.
These are checks, not independent locations or production accuracy estimates;
geographic-radius agreement does not establish exact administrative identity.
Local timings are not the CI request-speed gate.

The shared Indonesian module is a builder-hash input even for runtime-only
lookup changes, so the chart pins `a91b74e2bc3f2088`. The rebuilt full index's
`places.tsv` and administrative-link audit are byte-identical to the baseline,
and a same-consumer full-suite comparison has zero response or score drift.
Comparing the baseline consumer/index against the candidate consumer/rebuilt
index preserves the gains above with no regressions. This is not another
geographic-data import.

Targeted authored-fact search and autocomplete tests, library/matching/chart
checks and all-target clippy passed. The eleven-namesake test fails if the
truncated lookup is restored and passes with the exhaustive bounded lookup.
Model-backed RPC integration still failed identically on baseline and candidate
(0/2): grpcurl timeout and the oversized-log assertion. No interoperability or
full ordinary-suite pass is claimed; the baseline POI-test stall remains
unresolved.
