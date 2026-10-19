# Postal-town recovery

Follow-up to [#163](https://github.com/Mokto/opengeocoding/issues/163) and the source-metadata preservation in [#176](https://github.com/Mokto/opengeocoding/pull/176). No source, gold expectation, alias table or index format changes. The builder hash remains `5aa58df2c766f429`; no index rebuild is required by this change.

## Two different remaining mechanisms

The six postcode-conflict training cases already retain the correct `address.postal_town` on their raw house candidates. Corroboration previously required the requested postcode to agree before that town could count. A source-backed postal town can now corroborate a conflicting postcode on the raw lookup, but only when:

- The house number and road agree, the country is recognized and agrees, and the stated region is consistent.
- The postcode is indexed in that country, or it is a correctly shaped numeric code differing from the house's own postcode by exactly one substitution.
- No indexed street matching the candidate's road terms in that country holds the requested postcode or a compatible postcode area.
- Every occurrence of the requested house number on those streets is within 100 m of the candidate. A search reaching its 1,000-street cap declines recovery rather than claiming uniqueness.

The house keeps its own source postcode and settlement/admin fields. `goodMatch` stays false because the input contradicts the source postcode. The existing nearby-source reconciliation, shared postcode-area compatibility, competing-postcode candidate preference and same-street postcode conflict guard remain in place. Unknown nonnumeric postcodes, ambiguous house locations, wrong source towns and country/region conflicts do not qualify.

The Hanoi training case has no source postal town or postcode on its OSM building. There is other existing source evidence: the index's locality and administrative-region records identify the same Wikidata entity, in the same country and ISO region. Startup records those administrative representations of settlements; an ancestor's exact indexed name can then corroborate the house without inventing a postal town or changing its returned ancestry. A same-named county/region with a different or missing identity does not qualify, nor does an identity in a different country or ISO region. A supplied postcode on this path requires the house's own compatible postcode, not merely an absence of conflicting postcode data.

Both mechanisms judge corroboration separately from rendering, preserving the original settlement and administrative fields and existing fallback ordering. Neither copies an alias or geographic expectation from gold.

## Reproduction and geographic scope

The baseline service was built from `0491e8f`; the feature branch starts at `1c67d6b`, whose intervening change is release version metadata, not resolver behavior. Baseline and candidate used the same evaluator binary, models (`65627ea2114b`), reference data and frozen full-index contents (builder `5aa58df2c766f429`, index fingerprint `b8fff11e1de8`). Unchanged-binary repeats of both baseline and candidate over 8,934 gold cases had zero response or class drift across SearchAddress, SuggestAddress, ResolveRegion and ReverseGeocode.

| Check | Baseline | Candidate |
| --- | ---: | ---: |
| Listed training cases passing | 14 / 21 | 21 / 21 |
| SearchAddress passing | 7,691 / 8,855 | 7,700 / 8,855 |
| SuggestAddress passing | 6,138 / 8,936 | 6,138 / 8,936 |
| ResolveRegion passing | 3,313 / 3,333 | 3,313 / 3,333 |
| ReverseGeocode passing | 7,683 / 7,801 | 7,683 / 7,801 |

The postcode-only experiment changed exactly the six targeted postcode-conflict cases to pass, all with `goodMatch=false`. Adding shared source-entity corroboration changed Hanoi and two Korean city-region cases from locality to house answers. The complete gold diff had nine newly passing cases, no newly failing or worse-class cases, and no abstain-to-wrong-answer changes. Held-out cases were not inspected or used to author either rule or its fixtures.

The 1,000-case unscored address corpus has two locality-to-house changes, for Ottawa and Cologne, corroborated by the shared source-entity mechanism. Those changes are not independent accuracy evidence. The stored snapshot was compared, not regenerated.

Authored synthetic checks cover a postal town that is also a county, a known wrong postcode and an unknown one-substitution numeric postcode, a postcode held elsewhere on the same road, duplicate houses several kilometres apart, missing numbers, country/state conflicts and a city/region entity whose house is assigned to a different settlement. A separate loader check requires matching nonempty identity, country and ISO region; equal names alone do not count.

These are corpus diagnostics, not production accuracy or latency estimates. Local timings were collected during concurrent builds and failed the speed threshold; even the unchanged-binary repeat exceeded the reverse p50 threshold by 13%. The local runs establish response stability and geographic changes, not a comparable performance result. The isolated CI quality and measured speed checks must both pass before completion. The documented grpcurl SearchAddress hang remains a known limitation.
