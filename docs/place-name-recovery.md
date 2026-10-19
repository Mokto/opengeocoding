# Supported place names and administrative qualifiers (#166)

## Reproduction

Base: `a728336`. Full local index: `f2e491ae4811`, builder `83ef195a69f42f93`. Default libpostal models: `65627ea2114b`; reference data: `5cebf4b7a553`. All eleven listed training cases fail at this base: one `far_miss`, five `near_miss`, five `abstain`. The eleven supplied training cases drove the investigation. No held-out aliases, location-specific rules or expectation changes were added; the full-suite regression gate checks held-out cases too.

The table records the base parser fields (C = city, H = house, R = road, S = state, CR = country_region, I = island; the recognized trailing country is omitted unless parsing lost it). Names and chains are from `places.tsv`, not aliases inferred from responses. Country parents are shown by code. The names column lists the relevant existing spellings, not every translation of the ancestors.

| Training case | Base parse | Existing settlement names; parent chain | Exact loss/rejection |
| --- | --- | --- | --- |
| `city-moyynkum-zhambyl-kazakhstan` | R=moyynkum; C=zhambyl | Мойынқұм / Moiynkum / Moyınqum; Moyınqum District → Jambyl Region (also Zhambyl) → KZ | `geonames::add_places` deduplicates GeoNames 1524265 through Мойынқұм but discards its primary Moyynkum. `Index::search` instead searches the parsed town Zhambyl. Even with the spelling retained, the qualifier must not remain the parsed town. |
| `city-mizdah-jabal-al-gharbi-libya` | H=mizdah; S=jabal al gharbi | ⵎⵣⴷⴰ مزدة / Mizda / مزدة; Nafusa Mountains (also Jabal al Gharbi) → LY | Deduplication discards primary Mizdah (2214827). The first exact indexed place-like part is the region, which answers instead. |
| `rural-kaliling-western-visayas-philippines` | C=kaliling; CR=western visayas | Caliling; Cauayan → Negros Occidental → Negros Island Region → PH | Deduplication through Caliling discards primary Kaliling (1709625). The raw region hit fails the resolver's city corroboration. Western Visayas is **not** the indexed parent; retaining a spelling does not validate that old qualifier. |
| `rural-hlathikhulu-shiselweni-eswatini` | H=hlathikhulu shiselweni eswatini; no country field | Hlatikulu; Inkhundla Mtsambama → Shiselweni → SZ | Deduplication through Hlatikulu discards primary Hlathikhulu (935081). `Index::search`'s first-comma-part/trailing-country recovery finds no spelling; no raw candidate survives. |
| `rural-az-zall-q-southern-governorate-bahrain` | H=az zallāq; S=southern governorate | الزلاق / Zallaq; Southern Governorate → BH | Deduplication through Zallaq discards Az Zallāq / Az Zallaq (290296). Only the governorate is an exact raw hit. Independently, the index returns BH-14 while the corpus expects BH-17; recovering the town does not fix that region-code disagreement. |
| `rural-kamyanyuki-brest-belarus` | H=kamyanyuki; C=brest | Каменюкі / Kamieniuki; Камянюцкі сельскі Савет → Kamenets District → Brest Region → BY | Deduplication through the shared names discards primary Kamyanyuki (627659). Search then finds Brest, and the resolver corroborates the parser's wrong town rather than the requested village. |
| `city-baynjiwayn-sulaymaniyah-iraq` | H=baynjiwayn; C=sulaymaniyah | پێنجوێن / Penjwen; Penjwen Central Subdistrict → Penjwen District → Sulaymaniyah Governorate → IQ | Deduplication through Penjwen discards primary Baynjiwayn (98012). Sulaymaniyah becomes the first exact hit and passes city corroboration. |
| `city-massy-jalal-abad-region-kyrgyzstan` | C=massy; S=jalal-abad region | Масы / Ması; Nooken District → Jalal-Abad Region → KG | Deduplication through Масы discards primary Massy (1527689). The region alone fails city corroboration in raw and structured tiers. |
| `rural-london-village-line-islands-kiribati` | R=london village line; I=islands | Ronton / London → London → Kiritimati → Line Islands → KI | Two nearby Overture localities answer to London, so no new alias is assigned to either. Village is their kind, not an indexed name token; the parser fuses it with Line and search answers Line Islands instead. |
| `city-al-khawr-al-khor-qatar` | C=al khawr al khor | الخور / Al Khor; 74 → Al Khor and Al Thakhira (also Al Khor) → QA | Deduplication through Al Khor discards primary Al Khawr (289962). The fused city matches neither settlement spelling; the partial region hit fails city corroboration. |
| `city-us-fl-gainesville` | H=us-fl-gainesville | Gainesville; Alachua County → Florida → US | One unknown hyphenated token reaches `places_named`/its all-terms lookup. No separate country, state or town exists to corroborate. Intentionally unchanged: interpreting a producer-specific slug is not alternate-name recovery. |

## Mechanism and boundaries

- Deduplication still uses shared names, country and the existing 10 km distance bound. Only a **unique matching settlement of the same subtype** gains GeoNames' primary name and ASCII form, in both its searchable names and its persisted name details. Its coordinates, ranking and parents do not change. Multiple nearby matching settlements, a match only to an administrative area, and a different country cannot acquire the names.
- GeoNames' unrestricted alternate-name list remains identity evidence only. It contains district labels and cannot safely become searchable aliases wholesale.
- For exactly three comma-delimited parts (town/administration/country), recovery requires an exact indexed settlement name in the recognized country before parsing can be aligned. A matching administrative ancestor corroborates the qualifier, including its names in other languages. An alternate-name interpretation with a known but contradictory administrative qualifier abstains before fragment fallbacks can discard that constraint. Existing primary/English-name handling of unmatched qualifiers remains unchanged; the repair is not a general tightening of every legacy region heuristic. House-number and postcode parses are not rewritten. An unqualified homonym is not reassigned.
- A trailing English Village can act as a type hint only for an indexed village corroborated by the written administration and country. It does not become an unqualified alias or a general free-text expansion.
- Suggestions also accept an ancestor's exact indexed name in another language as a qualifier. An alias that also names a foreign country needs separate country corroboration; it cannot override that recognized country by itself. Prefix matching of qualifiers still uses the primary and English names; it is not widened to every alias.
- Existing administrative-word normalization remains in use; no fuzzy free-text expansion, location-specific rule, new alias source, country override or fallback reordering is added.

The authored synthetic case supplies a Cyrillic primary name, its ASCII form and an existing indexed name for one settlement, plus a larger distant namesake. It checks the persisted aliases, regional scope, contradictory region/country inputs, unqualified ranking, exclusion of untrusted alternate labels, and an exact out-of-region namesake beside a merely partial in-region name. An authored province alias also collides with a recognized foreign country: it cannot scope the settlement alone, but works when the actual country separately corroborates the interpretation. The name and ancestor evidence must belong to the same exact candidate; a lookup's aggregate “has an exact hit” flag is not evidence for its other hits. Builder tests also check ambiguous nearby matches and administrative-only/cross-country matches.

The rebuilt place table has the same 4,711,280 rows as the base. A complete row-by-row comparison verifies that 13,206 rows gain only appended names: identities, coordinates, parents, codes, populations, original names and Wikidata items are unchanged.

## Full-index validation

The final comparison uses main `f7a6727` after rebasing over the postal-town and Norwegian/Dutch parsing fixes, the same models and references, and 8,934 shared gold cases. The baseline index is `358185ba7395` (builder `9547c5f06d71e087`); the rebuilt candidate is `161361fffd3e` (builder `ec8e06ca98c9516c`). Runtime and import changes are separate experiments:

| Experiment | SearchAddress passing | SuggestAddress passing | ResolveRegion / ReverseGeocode changes |
| --- | --- | --- | --- |
| Runtime only, unchanged index | 7,654 → 7,665 (+11) | 6,055 → 6,123 (+68) | None |
| Import only, unchanged candidate binary | 7,665 → 7,683 (+18) | 6,123 → 6,136 (+13) | None |
| Combined | 7,654 → 7,683 (+29) | 6,055 → 6,136 (+81) | None |

No newly failing or worse failure classes occur. Eight of the eleven training cases pass. Kaliling retains its unsupported historical qualifier; Gainesville retains its unsupported producer slug. Zallaq now resolves to the settlement but remains `wrong_region` because of the BH-14/BH-17 disagreement. Expectations and known-failure labels are unchanged. The other improved failure classes are Assomada and Narsaq reaching their expected geography but retaining confidence mismatches, and Miquelon's name-only suggestion becoming rank 2 instead of missing. They are not counted as passes.

Independent repeated base and candidate runs have zero response drift across all four RPCs. The final runtime, import and combined comparisons meet the local 10% speed budget. Identical-binary timing comparisons nevertheless exceed it (base reverse p99 +54%; candidate search total +18%), and earlier pre-rebase comparisons also failed it, including unchanged reverse code. These unstable host timings do not support a speedup claim; the controlled CI comparison remains necessary.

The 1,000-case unscored address comparison has four changed responses. Repeated Mozambique now follows the existing repeated-country/town rule through the source-backed town name; that ambiguous input is not an accuracy result. The county-labelled Longsheng query reaches Longsheng instead of distant Zhuangcun, without establishing county-level semantics. Repeated Torrejón de Ardoz reaches that settlement instead of Madrid, and Medina retains its point with changed confidence. The accepted address snapshot is not regenerated; it already differs from the current base in 864 responses, which is unrelated baseline drift rather than this branch's four changes.

Checks: `cargo fmt --check`; release Clippy over all targets with warnings denied; `cargo test --locked --release`; the ignored model-backed synthetic test; release service/evaluator builds; full gold comparisons and repeat checks; address drift comparison. Current ignored RPC checks fail identically on base and candidate at their missing CDG landmark fixture (one passed, one failed each). Earlier checks on a landmark-complete index passed the other contracts before reproducing `grpcurl timed out`; the final gold evaluator still records that known transport failure. No assertions, retries or expectations were weakened to hide it.

These are source-import and query-interpretation checks, not production accuracy or semantic proof for every old query. Canonical proximity alone is not a full re-score.

## Remaining-case follow-up

The original implementation is on main as #178. This follow-up separates a supported input form from a corpus correction; it does not import aliases or change the builder.

### Gainesville: bounded ISO-code syntax

SearchAddress accepts `<country>-<subdivision suffix>-<settlement>`, case-insensitively, when the country is recognized, the complete ISO 3166-2 subdivision exists in the index, and the complete settlement name is indexed under that subdivision's ancestor chain. Hyphens between name words become spaces under the existing exact-name normalization. The name contains letters and hyphens only, not house numbers, postcodes or additional comma-separated fields. Known country prefixes with an unknown subdivision or an incompatible/missing settlement abstain without fragment fallbacks. This is not arbitrary slug splitting or fuzzy free-text matching.

Authored synthetic cases cover French/US namesakes, numeric subdivision codes, multiword Cyrillic and transliterated alternate names, a larger distant namesake, and contradictions in country, subdivision and complete versus partial names. Invalid syntax stays on the pre-existing path. Gainesville's country, point, radius, region and confidence expectations are unchanged; its known-failure label is removed only after the full-index lookup passes.

### Zallaq: independently adjudicated region

The legacy expectation follows [Zallaq, Wikidata Q4117781 revision 2113024205](https://www.wikidata.org/w/index.php?title=Q4117781&oldid=2113024205), whose P131 names Northern Governorate (Q840445, `BH-17`). That conflicts with the written Southern Governorate qualifier and the government's own records:

- [Southern Governorate, Zallaq Residents Hall Project](https://www.southern.gov.bh/en/article/zallaq-residents-hall-project), published 2019-04-01, identifies the governorate's project at Al Zallaq.
- [Southern Governorate, National Day festivities reach Zallaq](https://www.southern.gov.bh/en/article/national-day-festivities-reach-zallaq22), published 2022-12-11, records its governor and deputy governor attending the governorate's celebrations there.
- [Southern Area Municipality](https://www.mun.gov.bh/newportal/en/southern-area-municipality/about-municipality) independently includes Al Zallaq landmarks in the southern area.
- [Southern Governorate, Wikidata Q838532 revision 2529394607](https://www.wikidata.org/w/index.php?title=Q838532&oldid=2529394607) supplies the CC0 ISO code `BH-14`, as in the [Isa Town adjudication](adjudications/168.md).

Inspected 2026-10-19. The seed uses the existing `region_qid` mechanism to source the current governorate separately from the town's identity/point. Generated expectations change only `BH-17` to `BH-14`; the WGS84 point (26.05, 50.4833), 6 km radius, query, country, locality level and `good_match=true` stay unchanged. No government prose, geometry or service-derived expectation is imported. A regression check still rejects `BH-17`.

Regenerate the two affected seeds through `evaluate build-gold --seeds FILE --out FILE` and merge generated rows by unchanged case ID. The merge was checked to permit only Zallaq's region/review, Gainesville's verified label removal, and retrieval dates. Full regeneration remains `evaluate build-gold` against the original seeds; generated rows are never hand-edited.

Re-scoring saved base/head runs changes Zallaq's forward class from `wrong_region` to `pass` on both binaries and re-scores its reverse/region checks. No service response changes for this correction; Gainesville's forward `abstain → pass` is a separate runtime change. No other class changes in the re-score.

### Kaliling: historical geography remains unresolved

[Republic Act No. 12000, section 3](https://www.lawphil.net/statutes/repacts/ra2024/ra_12000_2024.html), approved 2024-06-11, puts Negros Occidental, including its cities, municipalities and barangays, in the Negros Island Region. The indexed Caliling in Cauayan, Negros Occidental therefore does not have Western Visayas as its current ancestor. The named settlement spelling is already preserved by #178; the remaining rejection is the administrative contradiction, not a missing alias.

The old query and its expectations remain untouched and its failure remains visible. Supporting a former region requires dated, source-backed administrative relationships kept distinct from the current parent chain; making Western Visayas a name of the settlement or dropping that qualifier would hide the contradiction. This follow-up does neither.

### Follow-up validation and limits

A clean baseline compiled from main `589c018` and the candidate used the same full index `e6df26bed27b` (builder `4fb1fd818b46b2f6`), models `65627ea2114b`, reference data `5cebf4b7a553`, evaluator and corrected 8,934-case corpus. SearchAddress passes change **7,702 → 7,703**, with Gainesville the only changed response. SuggestAddress (6,139), ResolveRegion (3,314) and ReverseGeocode (7,684) passing counts and responses are unchanged. There are no newly failing or worse-class cases. With Zallaq independently re-scored on both sides, **10 of the 11 training cases pass** on the candidate; Kaliling remains an abstention. Independent repeated baseline and candidate runs each have zero response drift across all four RPCs.

The 1,000-case unscored address comparison also has zero base/head response changes. The accepted snapshot is not regenerated: its 864 differences from the current baseline are unrelated existing snapshot drift.

Checks passed: formatting, release Clippy over all targets with warnings denied, `cargo test --locked --release`, and the ignored model-backed synthetic suite. The ignored RPC suite fails identically on both binaries at the absent CDG landmark fixture (one passed, one failed each), after the preceding JSON/protobuf, legacy-path and address checks pass. Gold's passing gRPC probes do not clear the separately documented SearchAddress hang.

Local gold timings **fail** the 10% budget, including comparisons of unchanged binaries: baseline SearchAddress total +19%, and candidate SearchAddress p99 +74%. The code comparison is slower across all three measured RPCs, including unchanged suggestions and reverse lookups; the later unscored address comparison instead reports SearchAddress total -46%. These host measurements do not establish a speedup or a safe overhead bound. The CI speed gate remains necessary.

A separate current-main index rebuild is **not validated**. The existing build command completed with builder `5aa58df2c766f429`, but its output `44bfb597e74e` returns wrong house coordinates (Dam 1 in Amsterdam at 151.29181, -33.4466) and only 1,508 forward passes. A clean `589c018` binary reproduces this, ruling out the initial stale-executable hypothesis. The builder reported zero memory headroom and a 1 GB writer arena; causality is not established. The failed output and logs are preserved, and the last-known-good index was restored and backed up for the comparisons above. This branch changes no builder input/hash and does not hide that independent rebuild failure by changing matching, tests or expectations.
