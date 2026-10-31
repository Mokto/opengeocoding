# Street evidence and metropolitan locality ambiguity

Investigation of the remaining queries in [#461](https://github.com/Mokto/opengeocoding/issues/461) and [#462](https://github.com/Mokto/opengeocoding/issues/462), after [#469](https://github.com/Mokto/opengeocoding/pull/469). These are observed responses and source checks, not new corpus expectations or an accuracy estimate.

## Controlled inputs

Base: `0d14abc7dbbe`, fresh main on 2026-10-31. Main's builder hash is `dbec0f0cae254966`. The available default index had builder `a41c249ca7b2cb79` and predates main's schema; a search failed with `Field "name" is not indexed`. It was not modified.

Comparisons instead used the existing, schema-compatible full-world snapshot with builder `15509783cc6dd90d`, evaluator index fingerprint `d0b7527decee`. Its build reads Overture `2026-10-21.0`, OpenAddresses, OSM, landmarks, Korean shops and GeoNames. This is not an exact-builder-hash validation of main. Neither that index nor other sessions' indexes were rebuilt or changed.

Base and candidate used the same index, evaluator source, reference data (`9a68f034f3b0`), historical memberships (`16871dd9c5b3`) and libpostal models (`65627ea2114b`). Separate worktrees had separate targets. The unchanged-base rerun had zero response drift across 43,244 shared gold cases and all five scored RPC/check groups. The final candidate comparison likewise had zero response drift against the base. Timings from these busy-machine runs are not speed evidence.

## North Conway

Query: `2760 White Mtn Highway, North Conway, United States`.

Libpostal returns `house_number=2760`, `road=white mtn highway`, `city=north conway`, `country=united states`. The index's folded street tokens retain `mtn`; the indexed street is `White Mountain Highway`, whose tokens retain `mountain`. This is not a parser country error or a one-edit typo. USPS [Publication 28, Appendix C1](https://pe.usps.com/text/pub28/28apc_002.htm) gives `MTN` for `MOUNTAIN`.

The exact North Conway candidates are:

| Name | Indexed type | Ancestry |
|---|---|---|
| North Conway | locality / hamlet | Conway → Horry County → South Carolina → United States |
| North Conway | macrohood | Conway → Carroll County → New Hampshire → United States |

The neighbourhood is not the leading settlement candidate. The settlement fallback therefore chose South Carolina; it omitted the requested house and road, so `goodMatch=false`.

The abbreviation is only the first cause: spelling out `White Mountain Highway` still produced the same wrong settlement fallback on base. The street lookup finds a Conway, NH street near North Conway, but renders its enclosing town as Conway, not the requested neighbourhood. The resolver rejects that street's locality corroboration. No indexed house 2760 was found there.

Inspection also found highway segments under Ossipee, Milton, Tamworth, Albany, Wakefield, Bartlett and Madison, NH. A partial-name result, South White Mountain Highway in Show Low, AZ, is not the requested exact street. Of the two exact North Conway candidates' enclosing-town scopes, only New Hampshire's Conway contains the exact highway. South Carolina's Conway does not. The index retains administrative polygons, not a North Conway neighbourhood polygon that would establish a house's membership.

The fix retries an otherwise unmatched US `mtn` spelling as `mountain`, without changing the builder or replacing an existing literal match. For a simple numbered street/city/country query with competing exact namesakes, an exact street in just one enclosing-town scope can support the named neighbourhood. It returns that neighbourhood, **not** a street or invented house inside it. Additional parsed qualifiers prevent this shortcut; it does not discard their evidence. Candidate-scan saturation or multiple supported namesakes prevents recovery. No population preference or generic namesake abstention is added.

After: `North Conway, Conway, Carroll County, New Hampshire, United States`, `goodMatch=false`. House 2760 remains unresolved.

## Brisbane

Query: `121 King Street, Brisbane`.

Libpostal returns `house_number=121`, `road=king street`, `city=brisbane`. There is no stated postcode, suburb, state or municipal/metropolitan qualifier. `King Street` agrees exactly after normal street-type folding.

The broad Brisbane locality (`a04b219a-98e1-45e1-bf62-80bb3ee87aff`, Wikidata Q34932) has `Greater Brisbane` as an alias. Woody Point (`fc4cb3ee-a681-4ba7-b879-b266f844ee9d`) is filed below it, then Queensland and Australia. The municipality is a separate county, City of Brisbane (`9f70d143-ce73-4ad0-9bf1-230e075f9217`, Q917682), absent from that locality ancestry. City of Moreton Bay is county `3ba9bee4-20b7-4292-9a50-d1f28020567b`.

The local Overture address rows contain:

| House | Source locality | Postcode | Approximate point | Containing county |
|---|---|---|---|---|
| 121 King Street | Caboolture | 4510 | −27.08464, 152.94379 | City of Moreton Bay |
| 121 King Street | Woody Point | 4019 | −27.24968, 153.09743 | City of Moreton Bay |

Their Overture IDs are `82365b5f-3370-4197-87fe-432f8f93c8fa` and `543db76d-6ac0-436c-91a6-d09d329f2e70`. No 121 King Street row in the central Brisbane/Fortitude Valley area was found in this snapshot. Central King Street's existence does not establish that requested house.

Raw lookup leads with Caboolture's house. #469's outer-locality distance guard rejects its Brisbane corroboration. The suggestion-based house recovery then selects Woody Point, whose rendered ancestry names Brisbane. Its road and number match, its city label explains the query, and the existing 30 km rule does not reject it. Recovery marks that house `goodMatch=true`. The containing Moreton Bay polygon was not consulted for that decision.

The fix does not move the radius. For Australian street candidates, an existing unique `City of <stated city>` county in the containing region supplies a conflicting municipal interpretation when its polygons do not contain the candidate. Without matching postal evidence, neither an ordinary candidate nor suggestion-based house recovery can use that candidate as an answer to the ambiguous city query. The existing fallback returns the named city with `goodMatch=false`.

This does **not** infer that every mention of Brisbane means its municipality. It declines house precision when the available interpretations conflict. A matching requested postcode, a source postal town named by the city/suburb fields, an explicitly named outer suburb, or an explicit metropolitan name can retain the house. A house inside the municipality is not rejected because it is far from the centre. Missing municipal geometry supplies no new constraint.

After: `Brisbane, Queensland, Australia`, `goodMatch=false`. Probes for `121 King Street, Woody Point, Australia`, `121 King Street, Brisbane QLD 4019, Australia` and `121 King Street, Greater Brisbane, Australia` retain the Woody Point house with `goodMatch=true`. The unqualified query still lacks the evidence needed to identify a central-city house.

## Regression checks and limitations

Synthetic facts cover the US abbreviation, a literal `Mtn` street that must not borrow another street's `Mountain` house, a neighbourhood versus an ordinary namesake town with a larger enclosing city's population, a real indexed house in each town, and explicit state corroboration. Australian facts put an outer house about 26 km from the city centre outside its municipality, and a legitimate metropolitan house about 41 km away inside it. Controls cover matching and conflicting postcodes, an explicit suburb, suburb plus metropolitan city, an explicit Greater-city name and source postal-town corroboration.

An initial broader neighbourhood recovery regressed Metford and two Egyptian cases and coarsened several other answers. It was replaced with the simple-query, competing-namesake constraint above. Those regressions are absent from the final full comparison; no expectations or known-failure labels changed.

Host formatting, Clippy, unit/integration tests, separately run synthetic libpostal tests, and Docker checks/runtime builds passed. Model-backed RPC checks were run on both base and candidate. Both have the same two failures: `Mt. Gretna, PA 17064` does not satisfy the RPC assertion on this source snapshot, and the oversized-input diagnostics log assertion fails. These assertions were not changed or bypassed. Standalone gold runs also retain baseline must-pass/untriaged failures; only the relative comparison passes. The documented grpcurl SearchAddress hang remains a known failure despite this run reporting success.

CI's full-index quality and measured-speed comparison is a separate gate. Local compilation, synthetic facts and a zero-drift relative comparison do not establish general production accuracy or speed.
