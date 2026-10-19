# Reviewed historical membership observations

`data/historical_memberships.json` is a hand-reviewed reference file, compiled into the binary. It is not a raw Wikidata import or a complete worldwide history. Its format and matching code are country-neutral; initial evidence covers one settlement in the Philippines and one municipality in Norway. Adding another country requires reviewed data, not a country branch in the resolver.

SearchAddress consults these observations **only after its existing lookups and retries have abstained**. A successful current interpretation retains precedence. Recovery accepts exactly three comma-separated parts: an exact indexed settlement name, the reviewed historical qualifier, and a recognized country. The settlement may use an existing indexed alternate name. Partial names, numeric town/qualifier parts, additional address fields and qualifiers naming a recognized country are excluded. A separately recognized foreign country in the settlement part cannot be overridden.

A recovered answer has the settlement's **current** indexed coordinates, address components and region codes. `good_match=true` means this complete place query is corroborated by the reviewed observation; no requested street, house or postcode was discarded. This does not offer historical coordinates, an as-of-date RPC, historical street matching or historical-country interpretation. SuggestAddress, ReverseGeocode, ResolveRegion and ListRegions do not use these observations.

## File and review contract

Each record asserts that the entire listed membership path held at `observed_on`. This is a point observation, not a claim that it held continuously between that date and today. Facts from separate dates must not be combined into an invented chain.

| Field | Meaning |
|---|---|
| `id` | Unique, stable observation identifier. |
| `country` | Uppercase present-day country code; all bound indexed places must agree. |
| `observed_on` | Gregorian `YYYY-MM-DD` at which the reviewed path is supported. |
| `reviewed_on` | UTC review date, no earlier than the observation or source retrieval and no later than the server's UTC day. |
| `path` | Settlement first, any unchanged lower ancestors next, historical ancestor last. Nodes have a source-backed `name` and Wikidata identity. |
| `sources` | Public evidence URLs, retrieval dates, optional publication dates and the zero-based path edges they support. Every edge needs evidence. |
| `review` | Why these sources establish the common dated path and identity links, and what they do not establish. |

For `[settlement, municipality, province, former region]`, edge `0` is settlement → municipality, edge `1` municipality → province, and edge `2` province → former region. A Wikidata identifier identifies an entity; it is not by itself proof of its historical membership.

Binding happens at startup against existing place rows. The settlement must be a locality/local administration with the recorded exact name in the recorded country. Indexed Wikidata IDs must agree with the reviewed ones. A settlement lacking an indexed ID requires **two adjacent, named and identified lower ancestors**, in the same country, matching the reviewed path. An absent middle ancestor, changed lower chain, explicit conflicting identity, malformed/cyclic current chain or multiple matching place rows prevents recovery. The historical last ancestor need not exist in today's index. Distinct or ambiguous bindings for the requested name and qualifier abstain, even when one candidate is larger or an alternate spelling appears unique. Repeated observations of the same uniquely identified place do not create multiple answers.

This does not attach a province's old parent to every current descendant. A newly created or later-arriving town without its own reviewed observation gets no history from this file. A known-ID settlement can have a two-node observation directly naming a former ancestor; an unlinked settlement cannot use that shortcut. Relocations or changed lower ancestry of an unlinked settlement require new identity evidence, not relaxed binding.

The file is versioned and reviewed with code. Invalid schemas, dates, IDs or incomplete evidence fail loading. Missing or ambiguous identities in a regional/different source snapshot simply remain unavailable for recovery. There is no runtime file override, network access, download or write. The evidence is neither merged into settlement aliases nor written into index ancestry. No builder/index format or builder hash changes; older indexes can be used if their existing identities bind.

The evaluator records `historical_memberships_sha256` alongside its existing reference fingerprint. This hashes the evaluator's compiled reviewed file, not an RPC attestation of a separately supplied `SERVICE_BINARY`; binary/data control experiments must also record what each executable embedded. Hashing embedded bytes needs no additional data file in evaluation images. Treat additions as data updates: inspect primary sources, retain provenance/attribution, review scoped identities and dates, and compare the effect separately from runtime changes. A reference URL, statement rank or parse success alone is not approval. Raw candidate extracts remain outside this reviewed file.

## Initial observations

### Caliling, Cauayan, Negros Occidental, Western Visayas — July 2021

The [Philippine government report published 25 July 2021](https://www.ntfelcac.gov.ph/post/news-pna-1148234) explicitly locates Barangay Caliling in Cauayan, Negros Occidental. Its event-day caption is inconsistent with its publication date; the evidence used is the geographic membership described in July 2021, not an inferred exact day of the event.

[EO 38](https://elibrary.judiciary.gov.ph/thebookshelf/showdocs/5/78545) restored Negros Occidental to Region VI in 2017. Section 7 makes newspaper publication the effectivity condition; the Supreme Court catalogue records 10 August 2017 publication. [Western Visayas (Q13665)](https://www.wikidata.org/wiki/Q13665) is Region VI. This supports the 2021 chain without assuming that today's descendants all belonged to that region then.

[Caliling Q31528302 revision 2090431456](https://www.wikidata.org/w/index.php?title=Q31528302&oldid=2090431456) identifies Caliling and GeoNames 1709625. The existing index row has no Wikidata ID, so binding requires the recorded Cauayan Q195287 → Negros Occidental Q13862 lower chain, its exact source-backed names and a unique settlement. The query's Kaliling spelling is already an indexed alternate name; this file does not introduce it.

[EO 183](https://elibrary.judiciary.gov.ph/thebookshelf/showdocs/5/62394) had established NIR in 2015 before its 2017 revocation. [RA 12000](https://elibrary.judiciary.gov.ph/thebookshelf/showdocs/2/97490) re-established NIR in 2024, with effectivity dependent on complete publication. Neither the Act's approval date nor Wikidata's 13 June endpoint is substituted for a verified legal transition date. The observation claims no exact endpoint or uninterrupted membership interval. Outputs retain today's Negros Island Region/province ancestry and codes.

### Drammen municipality, Viken — 1 January 2021

Statistics Norway's [date-scoped KLASS correspondence](https://data.ssb.no/api/klass/v1/classifications/131/correspondsAt?targetClassificationId=104&date=2021-01-01) explicitly maps municipality `3005` Drammen (classification `131`) to county `30` Viken (classification `104`). These are historical classification identifiers, not timeless codes.

The reviewed entity links are Drammen **municipality** Q26525 and Viken Q56407178, not the separately identified urban settlement. The municipal identity must exist in the index; a name-only match cannot substitute the urban settlement. Viken need not still exist in the index. Current interpretations retain precedence, so an already answered Drammen query is not replaced by this path.

The historical lookup is live and can receive corrections. The response inspected on 19 October 2026 has SHA-256 `3ee6af63350e4216b5225c66ff0a7a74f06221469bf8e6a1b8d520accefebfea`. Statistics Norway's [CC BY 4.0 reuse policy](https://www.ssb.no/en/diverse/lisens) applies; attribution and the reduction/linking of its data are recorded in `NOTICE`. This observation does not establish unchanged boundaries or full membership endpoints.

## Checks

`cargo test --locked --release --lib index::history` checks authored facts, not this service's answers: exact alternate names; different countries; removed historical parents; changed lower ancestry; later/unattested settlements; conflicting source IDs; ambiguous bindings and namesakes; cycles; and malformed dates/evidence. `tests/synthetic/main.rs` also checks that unreviewed places and unsupported address shapes cannot acquire a historical qualifier. The model-backed gold suite checks the unchanged Caliling expectation against public source geography.

Full-index quality, repeat stability and measured-speed comparisons remain required for a matching change. Compilation and fixture checks do not establish production accuracy or worldwide coverage.

### Local comparison

Clean main `097e167` and this implementation used the same full index `e6df26bed27b`, models `65627ea2114b`, references `5cebf4b7a553` and consumer over 8,939 shared gold cases. The existing known-good index has builder `4fb1fd818b46b2f6`; the independently failed newer rebuild is not validated by these runs. This change alters no builder input/hash. The branch was subsequently rebased over the 0.14.0 release and documentation-only main changes; neither changes geographic behavior. The expanded-suite rebase is recorded below. Each new head needs fresh CI quality and speed results.

| Experiment | SearchAddress passing | Changed responses |
|---|---:|---:|
| New code with an empty reviewed file versus main | 7,744 → 7,744 | 0 |
| Enable the two reviewed observations on the new code | 7,744 → 7,745 | 1 |
| Combined versus main | 7,744 → 7,745 | 1 |

Only Kaliling changes, from abstention to a source-consistent Caliling result with `goodMatch=true`, Negros Island Region in its current address and `PH-NEC` from the existing index. Its geographic, region, level and confidence expectations are unchanged. All eleven original place-recovery training cases pass. There are no newly failing or worse failure classes. SuggestAddress (6,201 passes), ResolveRegion (3,319) and ReverseGeocode (8,449) responses are unchanged. Independent main/head reruns have zero response drift. The 1,000 unscored address cases have zero base/head response changes; their accepted snapshot was not regenerated.

The empty control embedded `[]` (SHA-256 `37517e5f3dc66819f61f5a7bb8ace1921282415f10551d2defa5c3eb0985b570`); the normal binary embedded the reviewed file. The common evaluator's reviewed-file fingerprint is the same across these control runs; the executable hashes distinguish the actual embedded inputs.

The new response adds one reported `region_level_mismatch:PH-NEC:4!=6` flag: the current index exposes the province code and typed level-6 region, but lacks NIR's level-4 code. This is not presented as complete region coverage or hidden by changing the expectation. The current formatting/enrichment behavior is unchanged.

Passed: formatting, release Clippy over all targets with warnings denied, the full release Rust suite, and ignored model-backed synthetic checks. The ignored RPC suite fails identically on both binaries at `tests/rpc.rs:752`, where the local index lacks the expected CDG landmark and returns Tunnel Cdgval (one test passed, one failed each). Gold reproduces the separately documented grpcurl hang; interoperability is not signed off.

The combined **local speed gate fails**: SearchAddress p50 +17%, SuggestAddress p90 +12%, ReverseGeocode p90 +12%. Unchanged-binary controls also fail (main search p50 +16%; head reverse p99 +17%). The isolated data and unscored-address timing comparisons pass, but those do not override the failed combined measurement. These host timings support no speedup or safe-overhead claim; the controlled CI quality and speed reports remain required.

### Expanded-suite rebase

Rebased onto `96dc059` after the documentation reorganization and the Belgian, New Zealand, Taipei and Bogotá registry additions. The matching implementation and reviewed evidence are unchanged. The historical guide and four newly added registry guides now follow the documentation layout expected by the catalogue check; only their relative links changed.

Fresh base/head runs on the same known-good index, models and references cover **11,057 shared cases**. SearchAddress passes **8,475 → 8,476**; Kaliling is still the only changed response. There are no newly failing or worse classes. SuggestAddress (6,746), ResolveRegion (3,319) and ReverseGeocode (10,567) passing counts/responses are unchanged. Independent base/head reruns have zero response drift, as do the 1,000 unscored address comparisons. No corpus expectations, labels or accepted snapshots were changed.

Formatting, release Clippy over all targets, the full Rust suite, registry-exporter checks, model-backed synthetic checks and relative links in the affected guides pass. The known local CDG fixture failure remains identical on base/head. The address-run wrapper timed out after writing all 1,000 head records; explicit checkpoint resume completed the remaining probes/report without repeating completed cases.

Local speed **fails again**, including unchanged-binary controls: combined search total +25%, suggest total +39% and reverse total +37%; the unchanged head's search total varies +60%. These non-isolated timings establish no overhead bound. Fresh CI quality and measured-speed results are required after updating the PR.

The expanded-suite CI run for `b8a704a` passed quality (8,471 → 8,472 forward passes; only Kaliling changed) but **failed speed**: search total +11%, suggestion p99 +21%, reverse p99 +20%. That is not a verified performance pass. Its executable fingerprints match the earlier passing CI comparison (`74faed3d83c7` base, `f64a685ea9c7` head), while its corpus and index fingerprints differ; those runs alone do not isolate the cause of the timing difference.

During that run, main advanced with New Zealand and Taipei matching fixes. The branch was rebased again onto `8055894`, retaining the reviewed evidence and historical-recovery implementation unchanged. Upstream now contains the registry-guide/helper relocations, so those changes no longer appear in this feature's diff. The new combination requires its own quality and speed comparison; the failed head was not blindly rerun. Fresh local 11,057-case runs on the same index/models have **9,016 → 9,017** forward passes, with Kaliling the only change and no newly failing or worse cases. Other RPC outcomes and the 1,000 unscored-address responses are unchanged; both independent repeat runs have zero response drift. All unit, exporter and model-backed synthetic checks pass. The CDG fixture failure remains identical on base/head. Local timing still fails badly on the non-isolated host (combined totals +127% search, +130% suggest and +139% reverse; unchanged controls also fail), so it supplies no performance clearance.
