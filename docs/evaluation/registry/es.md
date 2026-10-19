# Spain

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/es.NOTICE)

Source: the IGN/CNIG **CartoCiudad address collection** through the official IDEE OGC API Features service. These are original harmonized source features, not the output of an address-text geocoder. Each source row identifies its originating authority and modification date. Reuse is under CC BY 4.0 and IGN's geographic-information conditions; retain attribution to IGN/CNIG and the originating providers. The exporter excludes postal-code properties because the relationship between the general address-data license and Correos's visualization-only postcode terms was not conclusively established. Postcodes are neither queried nor scored here.

## Capture and reproduction

```sh
python3 -u tests/registry/export-cartociudad-addresses.py --out SOURCE/cartociudad-2026-10-19 --seed es-2026-10
target/release/evaluate sample-es --cartociudad SOURCE/cartociudad-2026-10-19/cartociudad.json --seed es-2026-10 --out tests/registry/es.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/es.seeds.jsonl --out tests/registry/es.jsonl --cartociudad SOURCE/cartociudad-2026-10-19/cartociudad.json
```

CNIG provincial bulk downloads currently require reCAPTCHA authorization. The anonymous official OGC feature endpoint is used instead. The official WFS province inventory supplies the frame, including the autonomous cities and the source's non-province territory category. For each populated province, take three seeded, contiguous, ID-ordered windows of up to 128 original features, deduplicating overlaps. Verify the province filter, returned/matched counts, unique IDs, point geometries and exclusion of postal-code fields. Re-fetch every selected window at the end and require identical features and population counts. These checks detect selected-window drift, not arbitrary changes elsewhere in the live register; this is not a transactional national snapshot. Keep raw responses, request URLs, metadata, license and checksums outside git, and start a new dated capture if a checkpoint changed. The final normalized snapshot is immutable.

This **clustered candidate frame is not a uniform national address sample**. Adjacent source IDs share roads, places and provider history. The adapter subsequently balances province and pattern classes, choosing at most five different municipality/street pairs per pool. It does not infer nationwide accuracy or adequate coverage of every language/rural-address family.

## Eligibility and variants

- Only positive integer address numbers with empty/alphabetic extensions and nonempty road, municipality, province and provider are used. Unknown/negative sentinel numbers, no-number forms such as s/n, decimal kilometre values and ambiguous extension codes are excluded, not converted into houses. The IGR-Redes de Transporte provider is excluded to avoid treating kilometre markers as numbered houses. Comprehensive kilometre/no-number validation needs its own source-resolution expectations.
- The source number and extension remain one complete house number. Full source road and municipality spellings are preserved. Where the municipality explicitly publishes slash-separated alternatives, each form is also queried; no machine-translated names are invented. CALLE/C/, AVENIDA/Av. and CARRETERA/Ctra. are derived conventional query variants, not additional registry records. This does not validate a comprehensive regional street-alias inventory.
- A differing source address area is retained alongside its municipality, rather than guessed to be a postal town. Pattern classes are suffix, area, other-road-type and plain. The other-road-type class includes CAMINO, LUGAR, ALDEA, BARRIO, DISEMINADO and CARRETERA prefixes; these are lexical proxies, **not verified rural labels**.
- A location key hashes province, municipality, address area, road and complete number. For duplicates choose the lowest feature ID; exclude a whole group if another point is more than 150 m from that selected point. Selection occurs before any service query. All query variants share the stable hold-out group.
- GeoJSON is OGC:CRS84 (longitude, latitude); expectations use latitude, longitude. Source point precision/quality is not surveyed entrance/rooftop accuracy. A 150 m radius checks proximity to the published address point. Shared cadastral/CartoCiudad ancestry with index inputs limits independence; human review remains pending.

SearchAddress checks country, house level, complete number, point proximity and goodMatch; SuggestAddress checks country/type/point among the first five. ReverseGeocode checks **country only**; ResolveRegion is unscored. These checks do not validate units, apartment coordinates, reverse building identity or all postal qualifiers.

## Validation

The 2026-10-19 capture has **19,968 original feature rows** from 156 windows across 52 populated province/city queries; the official inventory's extra non-province category has no matches. The summed province-query population is 15,618,730, while a separate unfiltered official API count is 15,619,055: **325 records are outside this province-name match frame**, for a reason not verified. They are not silently counted as sampled coverage. Retain the separate national-count-audit.json alongside the capture (SHA256 1bd58889c7e858d4ea6ef6267215f12c702a8127603ba8553239a0dc3c3541ba). It is an additional audit response, not part of the normalized snapshot's receipts. The snapshot's populations map records the actual per-province frame.

The normalized cartociudad.json is 12,080,379 bytes, SHA256 2f65fa1a621d1c302eb7b192dc417d50c6d300a92807d93962e1825a660b75d7. Acquisition timed out once; it resumed from the original checkpoints and all 156 selected windows were subsequently re-fetched unchanged, including their population counts. No failed window was omitted. The adapter finds 19,374 eligible groups, excludes zero conflicting-point groups, and selects **910 locations, 1,544 queries and 186 held-out locations**, with cases from all 52 populated province/city names. Seeds and corpus reproduce byte-for-byte from the saved snapshot and labels.

Local SearchAddress results:

| Pattern class | Locations | Queries | Passes |
|---|---:|---:|---:|
| Plain | 251 | 447 | 432 |
| Suffix | 257 | 452 | 0 |
| Address area | 229 | 397 | 220 |
| Other road type | 173 | 248 | 226 |

Overall, 878/1,544 pass; 508 are too coarse, 53 are near misses, 94 abstain, three have a goodMatch mismatch and eight are far misses. **Every suffix query fails**; streets are often found, but suffix-house coverage versus parsing/lookup causes has not been isolated for the entire class. SuggestAddress passes 659/1,544 full-string checks. All 1,544 ReverseGeocode country checks pass; they do not validate the building. Native forms, derived abbreviations and official municipal-name alternatives account for 910, 578 and 56 queries respectively.

### Critical baseline attribution

The first standalone run failed its absolute gate on eight far misses at four locations. Each keeps its original point, 150 m tolerance, house number, goodMatch requirement and failing class. The per-query labels in es.labels.jsonl record observed baseline defects; they are not passes or a resolver fix. Rescoring against those labels changed zero classes.

- Two Llívia-area houses in municipality/province Lleida (four queries) fall back to the other Llívia about 143 km away. The raw original queries have no candidate. The native parse makes Llívia the city and Lleida the state/district. A non-held-out DELS PARTIDORS 2 municipality-only control finds the correct indexed house, proving that dropping the published area would conceal a hierarchy failure. No extra held-out GREALO variants were probed or tuned. The returned namesake also claims Lleida ancestry, although a separate official CartoCiudad record for municipality Llívia near that returned point places it in Girona; the origin of this returned-ancestry defect is not isolated.
- LES ILLES BALEARS 3 in area La Granja, Tarragona (two queries) already has the correct source-point house as raw top-1. Matching rejects it and falls back to La Granja in Segovia, 439 km away. The non-held-out municipality-only control finds the source house; it is diagnostic evidence, not a replacement expectation.
- MAYOR 30 in area Aldehuela, Teruel (two queries) falls back to Aldehuela in Segovia, 263 km away. Its native parse collapses repeated Teruel qualifiers into state_district. The original raw house is already a near miss; the municipality-only control identifies another Calle Mayor 30, not the source house. Correct-house coverage is not established, and removing the area would lose necessary disambiguation.

These are diagnostic results on the documented last-known-good local world index, builder 4fb1fd818b46b2f6 / fingerprint e6df26bed27b, not a successful current-builder rebuild. Existing curated CartoCiudad gold cases remain unchanged. This registry-first addition changes neither resolver/index behavior nor evaluator scoring. The documented fresh-service grpcurl SearchAddress hang remains a known failure despite these runs reporting it passing.

The full candidate run has 11,005 cases. It passes its absolute gate while all eight attributed Spanish far misses remain failures. Comparing it with the 8,939-case existing-suite baseline produces zero answer/scoring drift across all four RPCs, with no newly failing or worse-class cases; both new datasets' standalone classifications are unchanged in the full run. The local speed comparator passes, but the identical-binary head is 66–68% faster in total on a non-isolated, differently warmed host, so no speedup or request-latency claim follows. Current-head CI quality and measured speed still need verification.
