# Canada/Spain: registry versus OSM-panel triage

Diagnosis on `main` at `5104ea806b548cdcc9caa501d1fefbb1d59cf356`, 2026-11-01. References [nightly issue #296](https://github.com/Mokto/opengeocoding/issues/296), not a resolution of it. **Runtime, sources, scorer and gold expectations are unchanged.** No national accuracy or speed claim follows.

## Findings

- Current main reproduces the nightly's OSM search/suggest results: Canada **32/100 and 34/100**, Spain **60/100 and 50/100**. The discrepancy is not merely a stale-binary artifact.
- These are different sampling frames, not matched addresses. Canada/Spain OSM extracts deliberately supply **landmarks only** in both the documented local build and chart. Their addresses are not imported. Address coverage comes from Overture and eligible OpenAddresses. An OSM node can therefore disagree with, or be missing from, the address inputs even though its PBF is present.
- The nightly's registry/other country rows combine registry and place/other checks; their reverse column also combines at-point and offset checks. Neither is a national rate.
- The guides' “reverse country only” descriptions are obsolete for current scoring. At-point house checks require the number; offset checks do not. Spain OSM passes **46/100 at point but 100/100 offset**. Combining them hides this distinction.
- Every failing training OSM query/check is retained in the [training ledger](ca-es-osm-training.tsv): **60 locations, 120 queries, 322 failing checks**. Twenty-eight locations remain primarily unresolved. No all-source house absence, erroneous import drop, erroneous duplicate merge or expectation defect is established for those unresolved cases.
- The clearest small runtime proposal concerns Spanish suggestion particles: four training locations/eight failing original suggestion checks. Diagnostic removal of only `del`/`de la` retrieves the house with the other qualifiers retained. This is a proposal, not a fix or a replacement corpus.

## Experiment and provenance

No open PR or separate active triage session covered this investigation at start; an earlier session contained prompt planning only. A new worktree/branch was created from freshly fetched `origin/main`. Builds used a private target directory; no other session's process, capture, target or index was stopped, overwritten or rebuilt.

The documented shared full index `~/opengeocoding-index/out` had builder `75cba2342e68f637`, belonging to the Greenland-building experiment, not main's `eb225d0b385a41f5`. Retained `495ed3bf27ad1ccd` and `4fb1fd818b46b2f6` indexes were older experiments. Rather than change any of them, this investigation built its own full index using the documented local build's inputs and current main's builder.

The [machine-readable manifest](ca-es-osm-manifest.json) records all eight run manifests, aggregate splits, corpus hashes, binary/model/reference hashes and the input-inventory digest. Important identities:

| Item | Version / fingerprint |
|---|---|
| Builder | `eb225d0b385a41f5` |
| Index `meta.json` SHA256 | `81e90c4350cb7752f8ada2a2292f18f614edffaeb2a8f2d037321a63053bf5c3` |
| Service SHA256 | `5d302fe0c783737f753792fc5d0d41af6129b72ff62c749defb12cdc5e1da7ca` |
| Evaluator SHA256 | `3087211e44c0a271de3d9e53efc254da1abc1129c744916e341565e0ccec3ab5` |
| Models, evaluator's digest | `65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7` |
| Reference JSON SHA256 | `9a68f034f3b05f57aecdec9776b06c63a15cf48619703a6fd797bb61ce7cfb01` |
| Overture | Local `2026-09-23.1` division, area, segment, address and place parquet |
| Additional inputs | 432 accepted OA files; 155 address OSM PBFs; landmark-only PBFs; Korean shops; GeoNames populated places, admin1, GB/CA/NL postcodes |

The inventory hashes **847 input files, 207,476,194,757 bytes** individually. It is retained with the local artifacts. This is not the nightly's exact input snapshot: main's chart specifies Overture `2026-10-21.0` and snapshot `2026-10-22`. Equal OSM headline results do not establish identical source data or identical answers in CI.

Both landmark-only PBF hashes equal the panel's captured source hashes:

- Canada: `1d144112007bd5a616ad86f16e9d10fe39305f3ce56d74d71712f5b77bc44f60`, 6,518,798,779 bytes.
- Spain: `f5888cb90e58152a914cd74be214c5242ad402f985e85f5d774581829e565c64`, 1,492,080,272 bytes.

Expectations remain NAR June 2026 (`nar.json` capture SHA256 `5d02a3da1f7b796d1ef2ffe771501416d4eef638eacb84d559af6dc18e0b7910`), CartoCiudad (`2f65fa1a621d1c302eb7b192dc417d50c6d300a92807d93962e1825a660b75d7`) and the checked-in OSM manifest/corpus. Raw registry archives were not found in the searched local source/cache directories; they were not re-downloaded or regenerated. Their captured attributes and provenance remain in the corpus.

The latest nightly was [run 41282935097](https://github.com/Mokto/opengeocoding/actions/runs/41282935097), `b55dd7c`, reported 2026-11-01 at 08:13 Copenhagen time. Subsequent main changes include #479's namesake/hierarchy guards, #455/#492's GeoNames town work, #484's Unicode-number/retry-loop guards and Japanese/Hong Kong/landmark fixes. Earlier #322 repaired Spanish extensions/areas; Canadian unit/suffix/direction repairs are already in main. #435 retains placeless OSM houses and #436 changes building centroids/postcode selection. Neither can import CA/ES addresses from landmark-only extracts, and this panel's CA/ES records are **nodes**, not building ways.

## Comparable results

A pass below means scorer class `pass`, not merely a nonempty answer or `goodMatch=true`. Each panel query has one search, one full-text suggestion, one at-point reverse and one offset reverse check. There are no scored ResolveRegion checks in these four panels. Location counts are stable `location` groups, not audited physical buildings or independent samples.

| Panel / split | Locations | Queries | Search | Suggest | Reverse at point | Reverse offset |
|---|---:|---:|---:|---:|---:|---:|
| CA registry, all | 373 | 550 | 525/550 (95.5%) | 507/550 (92.2%) | 397/550 (72.2%) | 477/550 (86.7%) |
| CA registry, training | 304 | 450 | 431 | 415 | 332 | 396 |
| CA registry, held-out | 69 | 100 | 94 | 92 | 65 | 81 |
| CA OSM, all | 50 | 100 | 32/100 | 34/100 | 28/100 | 68/100 |
| CA OSM, training | 40 | 80 | 24 | 26 | 22 | 52 |
| CA OSM, held-out | 10 | 20 | 8 | 8 | 6 | 16 |
| ES registry, all | 910 | 1,544 | 1,496/1,544 (96.9%) | 1,488/1,544 (96.4%) | 1,410/1,544 (91.3%) | 1,526/1,544 (98.8%) |
| ES registry, training | 724 | 1,239 | 1,197 | 1,192 | 1,124 | 1,221 |
| ES registry, held-out | 186 | 305 | 299 | 296 | 286 | 305 |
| ES OSM, all | 50 | 100 | 60/100 | 50/100 | 46/100 | 100/100 |
| ES OSM, training | 44 | 88 | 54 | 44 | 40 | 88 |
| ES OSM, held-out | 6 | 12 | 6 | 6 | 6 | 12 |

Thus the requested panels contain **9,176 applicable checks per pass**: 2,200 CA-registry, 6,176 ES-registry and 800 OSM. Held-out records were evaluated and aggregated only, never individually probed or used to propose changes. The stored-index inspector rejects held-out locations before opening the index.

### Other country evidence is not the registry

Original, unmodified cases from the default suite outside these two registries and the OSM panel were copied into a separate diagnostic input, selecting expectations containing CA/ES. Suggestion-only cases are excluded from search denominators.

| Other evidence | Location groups / queries | Search | Suggest checks | Point reverse | Offset reverse |
|---|---:|---:|---:|---:|---:|
| Canada, all | 37 / 44 | 42/43 | 61/67 | 38/38 | — |
| Canada, training | 25 / 31 | 29/30 | 44/48 | 25/25 | — |
| Canada, held-out | 12 / 13 | 13/13 | 17/19 | 13/13 | — |
| Spain, all | 107 / 270 | 265/266 | 113/134 | 253/263 | 229/229 |
| Spain, training | 86 / 215 | 212/213 | 92/107 | 203/210 | 183/183 |
| Spain, held-out | 21 / 55 | 53/53 | 21/27 | 50/53 | 46/46 |

Combining current registry and other evidence recreates the misleading-looking country headline: CA search **567/593**, suggest **568/617**, combined reverse **912/1,138**; ES search **1,761/1,810**, suggest **1,601/1,678**, combined reverse **3,418/3,580**. The denominators and source frames differ from the OSM panel, and slightly from the stale nightly.

### Repeat and verification

Each of the four inputs was run twice with the same service, models, reference data and private full index. Comparison of complete saved responses, suggestion order/content/ranks, summaries and classifications, excluding timings, found **zero drift**. All four `evaluate diff ... --relative` commands exited 0. No evaluator retry was added.

`evaluate check` passed: 3,351 curated cases, 40,696 registry/global/suggestion cases and 1,000 address cases validated. The retained `check-evidence.py` also passed: exact 120-query training failure union, stable non-held-out location hashes, 322 unchanged failing checks, and matching ledger/corpus/input-inventory hashes. Connect JSON/protobuf equality and ListRegions passed in all runs. The fresh-service grpcurl search hang reproduced as its documented known failure. CA-registry and other-evidence absolute runs exited 0; ES-registry and OSM absolute runs exited 1 on retained critical failures. These are not overall accuracy passes. No speed sample was taken (`--speed-cases 0`); there is no measured-speed verdict or performance conclusion.

## Actual scorer audit

Checked code: [`score.rs`](../../../src/bin/evaluate/score.rs), especially `score_one`, `agrees`, `suggest_rank`, `reverse_class`; [`run.rs`](../../../src/bin/evaluate/run.rs), `suggestion_rank` and coordinate requests; actual corpus expectations, not just the guides.

| Property | CA registry | ES registry | CA/ES OSM |
|---|---|---|---|
| Point | NAR BG building-associated representative point; may be road access/driveway | Published CartoCiudad address point; per-point positional quality not supplied | Mapped address-node point; not certified entrance/rooftop |
| Search answer | First answer; house; country; 150 m; `goodMatch=true` | Same | Same |
| Number | Complete civic number; 219 unit-related queries accept building or published unit-building composite at the same point | Complete integer + alphabetic extension | Complete source number; no range/unit/suffix stripping |
| Postcode expectation | All 550 queries, including alternatives | None: exporter excludes it because Correos reuse terms remain unresolved | CA 24/100 queries; ES 90/100 |
| Street/locality identity in forward scoring | Street and locality are **not independently compared** | Same | Same |
| Suggestion | First five: address type, country, 150 m | Same | Same |
| Suggest number/postcode/street/`goodMatch` | Not checked | Not checked | Not checked |
| Point reverse | First result: country, house within 150 m, complete expected number, postcode where both nonempty, one expected locality name | Same; area/municipality alternatives | Same house/number/postcode checks, but no `places` locality-name expectation |
| Offset reverse | 25 m offset in a location-stable direction; house within source radius; country/locality; not exact number/postcode | Same | Same, without expected locality names |

Important enforcement details:

- Search compares house number/postcode only when the returned component is nonempty. House level normally supplies a number, but **a missing returned postcode does not fail the postcode expectation**. A conflicting nonempty postcode does. This is an existing scorer limitation, not a proposed relaxation.
- Postcodes compare case-folded letters/digits, so spaces/punctuation do not explain these postcode conflicts. Number comparison uses `number_key` and compact comparison, retaining meaningful suffix/range content.
- Reverse identity checks do not compare street identity either. A same-number house on another street can pass within the radius. For example, Fern Crescent's reverse passes on **132 Avenue**. This is not proof that the expected OSM house was found.
- A 150 m radius does not imply the OSM node is the indexed house's own stored point. A neighbour can be closer. Offset passing does not verify the expected civic identity.
- The registry guides' country-only reverse descriptions and introduction-era reverse rates are historical; their final `places` descriptions and the evaluator guide describe stricter current checks. The global guide's country-only reverse sentence is stale. No scorer correction is made in this investigation.
- The three source families share upstream ancestry: NAR/OA/OSM can draw from municipal sources; Spanish Overture/OA and CartoCiudad share cadastral ancestry; OSM supplies this panel and parser training data, though these PBFs are not address imports here. Selection outside the index is not independent real-world ground truth.

## Composition and like-for-like limits

Geometry is controlled only at the source-family level: **all 100 CA/ES OSM groups are nodes**, NAR groups are building-associated BG points and CartoCiudad groups are published address points. There is no per-point entrance/parcel/driveway classification with which to standardize them further. No independently verified rural/urban labels were available; none were inferred from a road prefix, CSD name or geographic spread.

The frames are not uniform national draws: OSM uses inland quarter-degree-cell sampling; NAR balances provinces and address patterns after BG-point eligibility; CartoCiudad uses clustered feature-ID windows and province/pattern balancing. Registry variants are mailing/geographic forms, abbreviations and official alternatives; OSM variants probe two number orders.

| Full panel stratum | Locations / queries | Search passes | Suggest passes |
|---|---:|---:|---:|
| CA registry, integer first civic alternative | 246 / 361 | 353 | 341 |
| CA registry, other civic form | 127 / 189 | 172 | 166 |
| CA OSM, integer | 49 / 98 | 32 | 34 |
| CA OSM, other number form | 1 / 2 | 0 | 0 |
| CA OSM, postcode present | 12 / 24 | 8 | 10 |
| CA OSM, postcode absent | 38 / 76 | 24 | 24 |
| ES registry, integer | 653 / 1,092 | 1,069 | 1,053 |
| ES registry, extension | 257 / 452 | 427 | 435 |
| ES OSM, integer | 46 / 92 | 56 | 46 |
| ES OSM, other number form | 4 / 8 | 4 | 4 |
| ES OSM, postcode present | 45 / 90 | 52 | 42 |
| ES OSM, postcode absent | 5 / 10 | 8 | 8 |

CA's first civic alternative is a building number even in a unit query; this stratum is not a count of unit-free query strings. Registry pattern counts below preserve that distinction.

Training qualifier strata, with pass counts in search/suggest/point/offset order:

| OSM training stratum | Locations / queries | Pass counts |
|---|---:|---|
| CA, city/town present | 38 / 76 | 24 / 24 / 20 / 48 |
| CA, city/town absent | 2 / 4 | 0 / 2 / 2 / 4 |
| CA, other administrative qualifier present | 9 / 18 | 8 / 8 / 8 / 12 |
| CA, no such qualifier | 31 / 62 | 16 / 18 / 14 / 40 |
| ES, city/town present | 35 / 70 | 46 / 38 / 30 / 70 |
| ES, city/town absent | 9 / 18 | 8 / 6 / 10 / 18 |

All Spanish training nodes lack the additional district/state/province/county tag class above. These are overlapping descriptive strata, not causal estimates. Training registry patterns have search/suggest passes: CA direction 82/76 of 84 queries, plain 99/95 of 99, Quebec 8/8 of 8, suffix 68/62 of 77, unit 174/174 of 182; ES area 295/283 of 309, other-road 200/202 of 203, plain 355/356 of 360, suffix 347/351 of 367. More suffix/unit cases do not automatically mean a harder frame; the existing repairs and shared inputs matter.

### What can be quantified, without claiming a causal decomposition

1. **Top five versus top one explains none of the OSM suggestion gap:** every passing OSM suggestion is already rank 1. The same is true for CA registry; ES registry gains six checks from ranks 2–5.
2. **Forward component/confidence definitions explain little of the observed OSM loss:** a diagnostic projection of original answers to house/country/150 m only yields CA 36 instead of 32 passes and ES still 60. Registry equivalents are CA 526 instead of 525 and ES 1,501 instead of 1,496. This projection is not a rescore or a proposed gold definition.
3. **Reverse check definition matters greatly:** all four panels' at-point answers have the expected country. A country-only projection would give 100% across them while validating no house identity. Under the actual checks the registry-minus-OSM gap is CA **44.2 points at point / 18.7 offset**, ES **45.3 at point / −1.2 offset**. Combining the two checks obscures that distinction.
4. **Sample composition cannot be assigned a defensible percentage of the remaining gap.** These are unmatched locations, point types, source dates, qualifiers and ancestry. There is no common independent location/geometry frame for reweighting. The causal observations below are training-case evidence, not a national or panel-wide attribution model.

## Training classification and evidence

The ledger covers exactly the union of failing training search, suggestion, at-point and offset checks: CA **31 locations/62 queries/196 failing checks**, ES **29/58/126**. Among all training locations, failures by RPC affect CA search 28, suggest 27, point reverse 29, offset reverse 14; ES search 18, suggest 22, point reverse 24, offset reverse 0. Secondary causes overlap.

The following primary classification counts are a bookkeeping partition. “Failing checks at these locations” is **not** a forecast that one repair will recover every check there.

| Primary category | CA locations / failing checks | ES locations / failing checks |
|---|---:|---:|
| House absent from all configured address snapshots, proven | 0 | 0 |
| Source row dropped/altered during import, proven defect | 0 | 0 |
| Incorrect duplicate merging, proven defect | 0 | 0 |
| Source-point disagreement / nearer neighbour | 4 / 12 | 11 / 34 |
| Postcode conflict | 1 / 4 | 0 (secondary at two locations) |
| Number/unit/suffix interpretation | 0 (secondary/unresolved) | 0 (secondary/unresolved) |
| Street alias/direction/abbreviation/punctuation | 1 / 6 | 4 / 12 |
| Postal/geographic locality or hierarchy | 2 / 14 | 0 (secondary) |
| Parser | 5 / 32 | 4 / 20 |
| Candidate ranking/confidence | 0 (secondary, not isolated) | 0 (secondary, not isolated) |
| Evaluation expectation mismatch, proven | 0 | 0 |
| Unresolved | 18 / 128 | 10 / 60 |

Method: training-only `evaluate inspect` reads all street lines within 1 km and their number-FST keys, beyond ranked search probes. A separate pinned-library diagnostic executable saves effective `Parser::parse`, raw `Index::search` candidates and query simplifications. Overture scanning retains all rows in approximately 2.2 km north/south windows (longitude adjusted by latitude), then same-number rows within 2.5 km of the source point; WKB supplies exact source coordinates. All 107 accepted Canadian/Spanish OA files were streamed for nearby rows. Same-number hits on other streets are retained as leads, never asserted to be address identity.

These bounded spatial scans **do not establish absence from every configured snapshot**, nor cover an arbitrarily displaced point. Missing exact keys may reflect a different street/number representation. The importer does not retain a complete per-house source-ID lineage in the final FST, so some merge attribution remains unproved. No arbitrary source was preferred to make the corpus pass.

### Representative complete chains

**CA 1809285736, Upper Lakeville, parser/qualifier (high confidence).** OSM publishes `471 Upper Lakeville Road`, district `2`, Upper Lakeville, Nova Scotia. The original number-first parse makes the road `upper lakeville road 2`; number-last assigns `house_number=2`. Original raw candidates are empty and both searches return locality fallback; suggestions are empty. The stored expected house is within a metre, backed by Overture/OA. A diagnostic query omitting the bare numeric district returns the house. This isolates qualifier contamination, not permission to delete arbitrary administrative qualifiers.

**CA 3619436172, Kitwanga, postcode conflict (high).** OSM expects `1587`, `V0J 2A0`. Overture row `d8a532f2-3ba0-45b5-ba1a-80681becf172` says `Kitwanga RD N`, `1587`, `V0J1G0`; the accepted BC OA row has no postcode. Stored street `5190142`, key `1587`, postcode slot 1 selects `V0J1G0`, approximately 24 m from OSM. Original raw and final house answers retain that postcode; suggestions pass geography; point reverse fails postcode, offset passes. This is source disagreement, not a demonstrated postcode-slot import bug.

**CA 5427192465, Prince Albert, source point (high).** Overture `2995 2nd AVE W` and stored `2995 2 Avenue West` lie about 235 m from OSM. Original raw/final house answers follow that point and fail the unchanged 150 m radius. A nearer reverse neighbour does not adjudicate which source point is right.

**CA 2957495952, Dean River, hierarchy plus point conflict (high).** Original raw candidates contain `1018 Dean River Place` under Area J. The number-first RPC abstains; number-last parse puts `cariboo` in the road and `j` in city, then returns a Quebec J fallback roughly 3,769 km away. OA and the stored expected-number street agree about 390 m from OSM. Even a hierarchy/parser repair would not prove this case passes its point check.

**CA 9865053230 / 907933302 / 1294226930, reverse identity (high).** Search/suggest pass for `108 Jarvis Street`, `11 Wilmac Court` and `550 Chemin de la Pointe-Sphérique`. At-point reverse instead returns `2145 Second Avenue`, `7 Wilmac Court` and `540 Chemin de la Pointe-Sphérique`. The returned neighbours are closer to OSM than the stored requested houses (roughly 11 vs 18 m, 30 vs 41 m, 14 vs 37 m). These are reproduced reverse failures, not passing forward results mislabelled as evaluation errors.

**ES 5722316436 / 6886498386 / 5990648592 / 3646720224, suggestion particles (high for the retrieval gap).** Original searches pass; original suggestions are empty for Plaza de la Constitución 10, Calle del Castillo 2, Carrer del Sagrat Cor 68 and Calle del Chorrillo 28. Source/stored houses exist within approximately 5–39 m. Keeping every other query component and removing only the road particles returns address suggestions through `Index::suggest`. `streets_typed` builds mandatory road-token clauses; absent particles block retrieval. Reverse failures at Castillo/Chorrillo are separate point/number issues, not explained away by a suggestion repair.

**ES 12856440263 / 3806559312, leading civic number (high).** `40 Avenida de Galicia` and `63 Avenida de las Candelas` are parsed as whole roads. Their number-last original variants extract the number and pass search at about 5/25 m. Galicia source row `cc380cd7-52ae-4996-922e-6ba9ab9df880` and stored `AV GALICIA/40` also have postcode `49032`, while OSM expects `49024`; reverse preserves that disagreement. Candelas has OSM locality `Olea` versus indexed `Orea`. Parser evidence is not a licence to overwrite either postcode or locality.

**ES 652579802, Errota (high).** Overture row `9da7efbf-316e-4a0c-813f-30800a7af2f7` stores `ID ERROTA/17`, postcode `20749`, about 10 m from OSM's point/postcode `20740`. Number-first parses the road but returns street-only; number-last moves `errota` to `house`, searches generic `industrialdea`, and returns `ID ESTAZIOKO INDUSTRIALDEA/17` about 1.5 km away with `goodMatch=true`. Point reverse exposes the postcode conflict. Parser, name/qualifier lookup and source postcode are distinct causes.

**ES 2137755650 / 4582778906, suffix coverage unresolved.** Both parsers retain complete `58D` / `13G`; the stored/source same-number lists are empty in the bounded evidence. Reverse returns `56d` / `20`, not the expected house. This does not prove global absence, suffix stripping or an importer regression. The existing Spanish extension repair must not be reimplemented on this evidence.

**ES 620566217, Belakasoro, alternate source points.** Source number 10 has unit A/B points around 96/103 m from OSM; the stored bare-number alias lies between them. Existing Spanish extension/bare-alias handling and equal-priority point aggregation are relevant, but an erroneous merge is not isolated. At-point reverse returns number 2 nearer OSM. Chorrillo similarly distinguishes bare `28` from nearer `28a`; neither suffix nor expectation should be discarded.

## Ranked next experiments, not authorisation to implement

| Rank | Proven cause / confidence | Affected unique training locations / checks | Scope | Smallest safe change and check; missing evidence |
|---|---|---|---|---|
| 1 | Suggestion particle-token mismatch; high | 4 locations, 8 original suggestion failures | Spain-scoped suggestion matching; no index/source change | Propose a literal-first, Spain-scoped road-particle fallback in `country/es.rs`, called by suggestions, preserving number/postcode/locality/country and requiring an unambiguous house. Synthetic source `CALLE CASTILLO/2` must complete `Calle del Castillo 2`; add `de la`, Catalan `del`, conflicting literal streets, wrong postcode/town and prefix negatives. Do not globally delete stopwords or invent aliases. Original-query corpus run plus synthetic tests, then required quality/speed comparisons. No code written. |
| 2 | Leading number absorbed into Spanish road; high | 2 locations, 2 directly contrasted search failures; other locations have additional blockers | Spain parsing | Test a bounded repair only when the street part independently supplies one number/road and the original parsed road wrongly contains that same number. Preserve every outer qualifier and literal-house precedence. Add synthetic number-first/last positives and numeric street-name/range negatives. Determine why existing retries miss these forms before implementation; no promised wider gain. |
| 3 | Bare numeric district contaminates the street/number; high | 1 CA location, 2 search + 2 suggestion failures | Canada qualifier handling; search/suggest require separate paths | First author a synthetic district-`2` case with the house present and a contradictory district control. A fix must preserve/corroborate the district, not silently delete it. Evidence isolates the symptom, but a safe administrative-identity rule is still missing. |
| 4 | Address-source policy excludes both countries' OSM rows; high for policy, unresolved per-house absence | All 84 training OSM locations are outside address-PBF ingestion; 60 have failures, 322 failing checks overall, not a recoverable-gain estimate | Shared import configuration / separate data experiment | If authorised, add the exact already-pinned CA/ES PBFs to an **independent** full-index experiment without replacing baseline. Compare all source conflicts, registry results and duplicate/point choices before recommending admission. ODbL attribution/database obligations and OA source licences require review. Adding an extract to fit its own panel is not independent validation. |
| 5 | Source points/postcodes and nearest-neighbour identity disagree; high on specific chains, truth unresolved | Primary point-disagreement class 15 locations/46 failing checks; postcode additionally CA Kitwanga and ES Galicia/Errota | Source adjudication, not a matching patch | Obtain independent address/point/postcode evidence with usable terms; preserve both source records. Never prefer OSM merely because this corpus uses it, change reverse to guess a requested number, enlarge radii, or suppress suffixes. Correos postcode terms remain a constraint. |
| 6 | Guides and headline aggregation obscure current checks; high | All four panels; no runtime recovery claim | Documentation/evaluator reporting | Label at-point and offset separately, registry-only versus other evidence, geography-only suggestions versus civic-identity search. Separately propose explicit missing-component scorer tests and street-identity limitations. Any stricter evaluator experiment must rescore saved runs and explain every changed class; it must not be mixed with runtime/source changes. |

There is no isolated small importer corruption fix to implement from this triage. Candidate/confidence errors and Canadian electoral-area handling merit follow-up only with house identity/coverage established. Twenty-eight unresolved locations need complete source-record/crosswalk evidence; they are not declared missing or relabelled known failures.

## Reproduction and retained artifacts

Public source attribution remains in [the global notice](../../../tests/global/NOTICE), [CA notice](../../../tests/registry/ca.NOTICE), [ES notice](../../../tests/registry/es.NOTICE) and [project NOTICE](../../../NOTICE). The ledger contains only public geographic source facts and training diagnostics, no private company samples or endpoints. Its `same_number_*_not_identity` columns deliberately show mechanical nearby leads, which can be different streets; use the narrative/source records before asserting identity.

Local artifacts are retained at:

```text
~/Library/Caches/opengeocoding/ca-es-triage-5104ea8/
  target/                     private build outputs
  index/                      private full index, not the shared out/
  build-index.sh, build-index.log, builder.hash.txt
  source-inventory.jsonl      individual input sizes/mtimes/SHA256
  run.sh, run.log, run-{ca,es,osm,other}-{1,2}/
  other-evidence.jsonl        original cases copied verbatim, not new gold
  zero-drift-{ca,es,osm,other}.md
  summarize.py, summary.json, compare.py, composition.json
  osm-training.jsonl, osm-source-training.jsonl, training-failures.jsonl
  stored.jsonl                complete training-only 1 km index inspection
  source-scan.py, source-scan.sql, overture-nearby.jsonl
  oa-scan.py, oa-nearby.jsonl
  probe/                     standalone diagnostic crate, repository lock versions
  probes-controls.jsonl, diagnostic-queries.jsonl
  suggest-probes.jsonl, particle-probes.jsonl
  evidence.py, evidence.jsonl, reviewed-classifications.jsonl
  review-classifications.py, export-note-data.py, check-evidence.py, experiment.json, check.log
```

Core commands, run from the investigation worktree (use a new artifact directory rather than overwrite these):

```sh
git fetch origin main
# origin/main was this SHA at investigation start; pin it for replay.
git worktree add -b docs/ca-es-osm-triage-replay WORKTREE \
  5104ea806b548cdcc9caa501d1fefbb1d59cf356
cd WORKTREE
export R="$HOME/Library/Caches/opengeocoding/ca-es-triage-5104ea8"
export CARGO_TARGET_DIR="$R/target"
RUSTFLAGS='-L native=/opt/homebrew/lib' nice -n 15 cargo build --locked --release \
  --bin opengeocoding --bin evaluate --bin build_index
"$CARGO_TARGET_DIR/release/build_index" hash
pgrep -fl build_index  # wait for an existing builder; never run two together
```

Build into `$R/index` using the documented source set, never run the shared script that removes `out/`:

```sh
RAYON_NUM_THREADS=12 nice -n 15 "$CARGO_TARGET_DIR/release/build_index" \
  "$HOME/geodata/overture/2026-09-23.1" "$R/index" \
  --openaddresses "$HOME/geodata/openaddresses" --osm "$HOME/geodata/osm" \
  --osm-landmarks "$HOME/geodata/osm-landmarks" \
  --korea-shops "$HOME/geodata/korea/sbiz/sbiz.zip" \
  --geonames-postal "$HOME/geodata/geonames/GB_full.txt" \
  --geonames-postal "$HOME/geodata/geonames/CA_full.txt" \
  --geonames-postal "$HOME/geodata/geonames/NL_full.txt" \
  --geonames-places "$HOME/geodata/geonames/populated.txt" \
  --geonames-admin1 "$HOME/geodata/geonames/admin1CodesASCII.txt"
export SERVICE_BINARY="$CARGO_TARGET_DIR/release/opengeocoding"
for pass in 1 2; do
  for cc in ca es; do
    nice -n 15 "$CARGO_TARGET_DIR/release/evaluate" run --suite gold \
      --corpus "tests/registry/$cc.jsonl" --index "$R/index" --no-direct \
      --repeat 1 --speed-cases 0 --workers 4 --out "$R/run-$cc-$pass"
    # Record a nonzero absolute-gate exit; continue the other measurements.
  done
  nice -n 15 "$CARGO_TARGET_DIR/release/evaluate" run --suite gold \
    --corpus tests/global/global-addresses.jsonl --only '^global-osm-(ca|es)-' \
    --index "$R/index" --no-direct --repeat 1 --speed-cases 0 --workers 4 \
    --out "$R/run-osm-$pass"
done
"$CARGO_TARGET_DIR/release/evaluate" check
```

Copy the other CA/ES evidence verbatim from the same pinned suite and run both repeats:

```sh
python3 - <<'PY'
import json, os
from pathlib import Path
paths = [Path('tests/gold.jsonl')]
paths += sorted(p for p in Path('tests/registry').glob('*.jsonl')
                if not p.name.endswith(('.seeds.jsonl', '.labels.jsonl')))
paths += [Path('tests') / p for p in (
    'gold.landmarks.jsonl', 'gold.towns.jsonl', 'gold.gleif.jsonl',
    'gold.people.jsonl', 'gold.suggest.jsonl')]
with (Path(os.environ['R']) / 'other-evidence.jsonl').open('w') as out:
    for path in paths:
        if path.name in ('ca.jsonl', 'es.jsonl'):
            continue
        for line in path.open():
            case = json.loads(line)
            expect = case.get('expect') or {}
            countries = set(expect.get('country', []))
            for alt in expect.get('any_of', []):
                countries.update(alt.get('country', []))
            if countries.intersection(('ca', 'es')):
                out.write(line)
PY
for pass in 1 2; do
  nice -n 15 "$CARGO_TARGET_DIR/release/evaluate" run --suite gold \
    --corpus "$R/other-evidence.jsonl" --index "$R/index" --no-direct \
    --repeat 1 --speed-cases 0 --workers 4 --out "$R/run-other-$pass"
done
for cc in ca es osm other; do
  "$CARGO_TARGET_DIR/release/evaluate" diff "$R/run-$cc-1" "$R/run-$cc-2" --relative
done
```

`--no-direct` prevents automatic per-failure index probes on held-out cases. Separately copy only original cases with `holdout=false` **and** a non-held-out stable location hash into `osm-training.jsonl`, then:

```sh
"$CARGO_TARGET_DIR/release/evaluate" inspect --index "$R/index" \
  --corpus "$R/osm-training.jsonl" --out "$R/stored.jsonl"
python3 "$R/summarize.py"        # full-response zero-drift assertion and training filter
python3 "$R/compare.py"          # descriptive strata and explicitly labelled projections
python3 "$R/evidence.py"         # combines original answers, source rows and stored keys
python3 "$R/check-evidence.py"   # verifies completeness, training isolation and hashes
```

The retained `source-scan.sql` uses DuckDB with four threads/3 GB memory, bounding-box filters around training points, `country IN ('CA','ES')`, and selects `id, street, number, unit, postcode, postal_city, address_levels, bbox, hex(geometry::BLOB), sources`. `source-scan.py` generates those windows from the unchanged training expectations. `oa-scan.py` streams only CA/ES entries marked `read=true` in this index's `openaddresses.tsv`; it retains nearby points and source properties. `probe/` links this exact checkout, uses the repository's dependency versions, rejects held-out locations before opening the index, and records `Parser::parse`, all returned `Index::search` candidates and `Index::suggest` for explicitly labelled diagnostic queries. No diagnostic query becomes a gold case or replaces the original.

The local scripts/checkpoints preserve the full extraction recipe and raw chains; the committed ledger/manifest preserve the reviewable classifications and identities. Reproduction requires the pinned snapshots, not today's `*-latest` downloads. Source acquisition, evaluator corrections and runtime repairs must remain separate experiments. Any later authorised runtime change still requires the repository's local gold comparison and current-head `wait_for_gold` quality **and measured speed** gates.
