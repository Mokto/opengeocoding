# Forward-geocoding scorecard

**Diagnostic measurements, not an unbiased production-accuracy estimate.** This is a vendor-run evaluation on this repository's gold corpus. Cases overlap the local index's source data; variants of one location are not independent samples. Do not read these tables as a general accuracy ranking.

The newest measurement is the OpenGeocoding 1.7.0 comparison below. The 4,183-query tables after it are an earlier, separate measurement; do not compare percentages across tables. Earlier comparisons, of OpenGeocoding 1.5.0 on 24,350 queries, 1.0.1 on 18,129 and 0.18.0 on 11,671 matched queries, are in this file's history ([1.5.0](https://github.com/Mokto/opengeocoding/blob/e1fb39ac6c13e22be80bc0749b88270c9641425c/SCORECARD.md), [1.0.1](https://github.com/Mokto/opengeocoding/blob/4b49315f1bf21fcd4b72275ed34639ae3fa7280b/SCORECARD.md), [0.18.0](https://github.com/Mokto/opengeocoding/blob/290f5226acb14a990d1094771c504399b530f8e9/SCORECARD.md)).

## OpenGeocoding 1.7.0 against cached provider responses (5 November 2026)

Google was not measured in this round. Provider responses were collected 17 October to 5 November 2026 and re-scored offline; OpenGeocoding 1.7.0, released on 4 November 2026, ran locally on 4 November. 1.7.0 is the release the hosted API runs, as of 5 November 2026. Every provider listed answered every case. Rates use all selected cases, including known failures, abstentions, API rejections and normalization failures. Mapbox, LocationIQ and TomTom collections are not published; see [Providers not measured](#providers-not-measured).

### Full forward selection: identical 29,013 queries

| Service | Passed / 29,013 | Pass rate | Held-out pass rate (5,804) | Only OpenGeocoding 1.7.0 passed | Only this provider passed |
|---|---:|---:|---:|---:|---:|
| **OpenGeocoding 1.7.0** | 24,830 | 85.6% | 86.4% | — | — |
| Geoapify | 20,447 | 70.5% | 71.2% | 5,554 | 1,171 |
| OpenCage | 14,894 | 51.3% | 52.6% | 10,768 | 832 |
| geocode.maps.co | 12,411 | 42.8% | 43.4% | 12,891 | 472 |

### US/Canada: identical 1,947 queries

| Service | Passed / 1,947 | Pass rate | Held-out pass rate (414) | Only OpenGeocoding 1.7.0 passed | Only this provider passed |
|---|---:|---:|---:|---:|---:|
| **OpenGeocoding 1.7.0** | 1,903 | 97.7% | 97.8% | — | — |
| Geocodio | 1,738 | 89.3% | 87.2% | 194 | 29 |
| Geoapify | 1,577 | 81.0% | 78.7% | 349 | 23 |
| OpenCage | 989 | 50.8% | 51.4% | 927 | 13 |
| geocode.maps.co | 754 | 38.7% | 34.5% | 1,158 | 9 |

### By cohort

The cohorts differ in difficulty and in level (house, street or town), so the full-corpus rate depends on their mix. The first row is the selection of the 1.0.1 comparison.

| Cohort | Cases | OpenGeocoding 1.7.0 | Geoapify | OpenCage | geocode.maps.co |
|---|---:|---:|---:|---:|---:|
| Curated gold and the first 22 registry samples | 18,129 | 95.8% | 82.1% | 59.2% | 49.9% |
| Registries added in October 2026 | 1,961 | 87.8% | 57.1% | 25.7% | 24.9% |
| Registries added in November 2026 | 4,660 | 62.1% | 31.3% | 16.5% | 14.7% |
| GLEIF company addresses, town level | 976 | 92.4% | 75.7% | 74.4% | 31.5% |
| Towns in countries without a registry (Wikidata) | 3,240 | 58.4% | 68.4% | 66.4% | 57.3% |
| Place strings as people profiles write them | 44 | 100.0% | 56.8% | 45.5% | 38.6% |
| Curated gold cases added in November 2026 | 3 | 100.0% | 0.0% | 0.0% | 33.3% |

### By registry

Each address-registry sample in the selection, in country-code order: cases passed out of the cases asked. [registry-comparison-1.7.0.tsv](docs/evaluation/registry-comparison-1.7.0.tsv) adds held-out counts and, for each provider, the cases only one side passed with the exact two-sided sign-test p-value on them. Geocodio answered only the United States and Canada. Against every provider it answered, OpenGeocoding 1.7.0 passed significantly more cases (p < 0.05) in 35 of the 40 registries. In Austria, Berlin and Liechtenstein it is ahead of OpenCage and geocode.maps.co but level with Geoapify. In Serbia it is ahead of Geoapify but level with OpenCage and geocode.maps.co. In the Faroe Islands every provider is significantly ahead: OpenGeocoding passed 116 of 136, against 131 to 134.

| Country | Registry | Scope | Cases | OpenGeocoding 1.7.0 | Geoapify | OpenCage | geocode.maps.co | Geocodio |
|---|---|---|---:|---:|---:|---:|---:|---:|
| Austria | bev | national | 156 | 156 | 153 | 132 | 121 | — |
| Australia | gnaf-atlas-json | national | 522 | 519 | 486 | 298 | 91 | — |
| Belgium | best-csv | national | 384 | 381 | 365 | 294 | 258 | — |
| Brazil | cnefe | national | 316 | 310 | 251 | 5 | 2 | — |
| Canada | nar-json | national | 550 | 532 | 376 | 28 | 25 | 503 |
| Switzerland | swisstopo-csv | national | 780 | 778 | 732 | 615 | 559 | — |
| Colombia | bogota-plates | Bogotá | 567 | 564 | 273 | 9 | 12 | — |
| Czechia | ruian-csv | national | 567 | 556 | 480 | 344 | 329 | — |
| Germany | berlin-hko | Berlin | 192 | 192 | 187 | 180 | 180 | — |
| Denmark | dar-json | national | 1,883 | 1,878 | 1,760 | 1,611 | 1,246 | — |
| Estonia | ads | national | 240 | 240 | 102 | 27 | 17 | — |
| Spain | cartociudad-features-json | national | 1,544 | 1,499 | 1,052 | 117 | 92 | — |
| Finland | ryhti | national | 169 | 166 | 145 | 41 | 36 | — |
| Faroe Islands | adressur | national | 136 | 116 | 134 | 133 | 131 | — |
| France | ban-csv | national | 808 | 801 | 754 | 716 | 263 | — |
| Greenland | asiaq-buildings | national | 144 | 144 | 0 | 4 | 0 | — |
| Hong Kong | als | national | 240 | 233 | 113 | 0 | 0 | — |
| Iceland | stadfangaskra | national | 312 | 305 | 149 | 80 | 74 | — |
| Italy | anncsu | national | 320 | 311 | 177 | 54 | 49 | — |
| Japan | abr-residences | national | 512 | 506 | 0 | 0 | 0 | — |
| Japan | mlit-blocks | national | 368 | 362 | 1 | 0 | 0 | — |
| South Korea | korean-schools | national | 2,794 | 795 | 677 | 294 | 268 | — |
| Liechtenstein | llv | national | 132 | 131 | 129 | 120 | 116 | — |
| Lithuania | adr | national | 200 | 200 | 52 | 0 | 0 | — |
| Luxembourg | act-csv | national | 240 | 239 | 213 | 186 | 186 | — |
| Latvia | varis | national | 244 | 244 | 161 | 60 | 89 | — |
| Mexico | denue | national | 320 | 94 | 53 | 4 | 2 | — |
| Netherlands | bag-csv | national | 1,026 | 1,026 | 981 | 840 | 932 | — |
| Norway | kartverket-csv | national | 1,071 | 1,068 | 1,024 | 896 | 897 | — |
| New Zealand | linz-addresses | national | 545 | 543 | 484 | 471 | 453 | — |
| French Polynesia | ban-pf | national | 120 | 119 | 101 | 0 | 0 | — |
| Poland | prg-json | national | 927 | 926 | 720 | 708 | 739 | — |
| Serbia | rgz | national | 296 | 280 | 247 | 273 | 269 | — |
| Singapore | onemap | national | 362 | 358 | 211 | 73 | 62 | — |
| Slovenia | gurs | national | 192 | 192 | 168 | 107 | 92 | — |
| Slovakia | ra | national | 200 | 197 | 172 | 132 | 131 | — |
| Taiwan | tainan-addresses | Tainan | 350 | 332 | 241 | 76 | 0 | — |
| Taiwan | taipei-addresses | Taipei | 622 | 620 | 574 | 196 | 0 | — |
| United States | nad | national | 850 | 835 | 709 | 527 | 347 | 776 |
| Uruguay | dgu | national | 206 | 205 | 138 | 100 | 102 | — |


### Read with these limits

- **The selection is frozen at corpus revision `bce81e3`,** the cases providers were asked: the curated gold cases, 40 address-registry samples, and the towns, GLEIF and people cohorts. It leaves out the OpenStreetMap global-address panel (OpenStreetMap is an input to this index and the data behind some providers), landmark and suggestion-only cases, and RPC error contracts.
- **Most of these cases were available during development.** Releases include fixes aimed at these registries, many of them shipped in the weeks each registry was added. The held-out column scores the fifth of locations kept out of tuning by a stable hash; it is closer to, but not, an independent sample, since the registries are also index inputs. Many Wikidata settlements were imported from GeoNames, which the index also reads.
- **The local index is close to production, not identical.** It was built with the 1.7.0 builder (`24e45dcd8295f18b`) from Overture 2026-10-21.0, the OSM extracts the chart lists, OpenAddresses, Korea's commercial-building addresses, Ryhti's Swedish-only addresses, DENUE establishments registered before 2025, Uruguay's lettered doors, Iceland's address register, Asiaq's Greenland building references and GeoNames, including its full populated-places list, from local snapshots of the sources that publish only "latest". The Mexico cases sample establishments registered from 2025, so they are not index inputs. Two complete runs returned identical responses for all 29,013 cases.
- **Per-registry results need the same care.** Many of these registries, or datasets built from them, are also index inputs for OpenGeocoding, so a pass there measures fidelity to the register more than independent accuracy. That holds most strongly for Greenland: the builder reads the same Asiaq building references the cases are drawn from, which providers do not carry, so 144 against 0 to 4 shows that, not geocoding skill. Japan's and Iceland's registers reach the index through OpenAddresses and Overture, and providers rarely answer Japanese residence-level addresses at house precision. The samples are not nationally representative: each registry's guide describes its draw, and some are targeted (Korean school sites, Mexican business establishments, one Luxembourg building per commune and number class). The Berlin, Taipei, Tainan and Bogotá rows cover one city each and do not speak for their countries. Samples of 130 to 2,800 cases are small, so read a difference as a real gap only when its sign-test p-value is low. Most registries ask two or more query forms of each location, and the test counts cases, not locations; those variants are not independent, which makes the p-values optimistic, and with up to four providers per registry some p-values below 0.05 are expected by chance.
- **Weak areas remain.** Towns in the 78 countries and territories without a registry sample pass 58.4%, behind Geoapify (68.4%) and OpenCage (66.4%); most failures answer only the country, because the settlement is missing from the index or nothing nearby corroborates GeoNames' entry. Of 2,794 Korean school addresses, 795 pass (Geoapify 677). The Faroe Islands pass 116 of 136, behind every provider. Of 151 street and house cases in Asian countries without a registry sample, 43 pass, against 61 for Geoapify. House-number coverage in most of Asia and Africa is still thin; see the [1.7.0 coverage table](docs/evaluation/house-number-coverage-1.7.0.tsv).

| Artifact | Value |
|---|---|
| Corpus revision | `bce81e3dc06df1feba3174711be1701c7523d4a9` |
| Full 29,013-case forward selection / held-out, JSONL SHA-256 | `30d7a386d4df4b1f99fa5603aab37904842dd62599af97c6cb30b91029a28379` / `d8a4b4b282b7539a15cd6a118d015acb6afc9ba875eb28815de78318b1017226` |
| US/Canada subset / held-out, JSONL SHA-256 | `2a6eb60eca782589e128d3c7943758344a52b8e175ccd3958c24ac34255568c2` / `3d6cf1a7131811a41a05abf7f52c3eaf48ac7c83f7963532c3296c10da05603f` |
| Evaluator and provider adapters | release 1.7.0, binary SHA-256 `c457e9a6fa9cde67fea3812a1ab9993b0abe8d3ff8cfd9157869485e0907d277` |
| OpenGeocoding 1.7.0 service binary | release commit `7ce6dae`, SHA-256 `fa0be44291c7a85c2da59ff4dc257d2bc19a829ad95284fbab325666d6772fc9` |
| 1.7.0 index | builder `24e45dcd8295f18b`, `meta.json` SHA-256 `870fb053417afd6e800d76770246e89c2576c722b10ebbba662f082d847ac84e` |
| libpostal models | `65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7` |

The forward selection keeps cases with a forward expectation, as described above. Request settings are those listed under Corpus and reproducibility; scoring uses the 1.7.0 evaluator, whose pass decisions on these responses match the 1.0.1, 1.4.0 and 1.5.0 evaluators'.

**Google (earlier measurement only) remains provisional.** The corrected-adapter pass is complete: normalization failures fell from 54 to 36 worldwide, with none in the US/Canada subset. Remaining failures include insufficient metadata and unsupported native types; they are not proof of incorrect Google locations. No Google response data remains for retrospective inspection. The tables show the completed rerun, not guessed corrections to the initial measurement.

## Earlier measurement — worldwide: identical 4,183 queries

Collected 17–18 October 2026; local baseline collected 16 October 2026. Rates use all selected cases, including known failures, abstentions, API rejections and normalization failures.

| Service | Passed / cases | Pass rate | Normalization failures | Status |
|---|---:|---:|---:|---|
| OpenGeocoding | 3,669 / 4,183 | 87.7% | — | Frozen local baseline |
| Google Geocoding API | 3,627 / 4,183 | 86.7% | 36 | Corrected-adapter pass complete; remaining normalization limitations |
| Geoapify | 3,422 / 4,183 | 81.8% | 1 | Complete; one native result has unknown precision |
| OpenCage | 2,766 / 4,183 | 66.1% | 1 | Complete; one result lacks country metadata |
| geocode.maps.co | 2,402 / 4,183 | 57.4% | 0 | Complete |

The Google rerun had 42 errors: 36 normalization failures and six query rejections. A normalization failure is **not proof of an incorrect Google location**. Even an all-36-pass scenario would produce 3,663 / 4,183 (87.6%); this is a generous upper bound for those unresolved cases, not a predicted corrected score. The local raw pass count is higher on this corpus, but the remaining interoperability limitations and corpus bias do not support a general accuracy-superiority claim.

## Earlier measurement — US/Canada: identical 1,395 queries

This subset is selected before provider responses: the union of expected-country alternatives must be nonempty and confined to the US or Canada. It includes all eligible cases, not just each provider's successful results. Do not compare these percentages directly with the worldwide table.

| Service | Passed / cases | Pass rate | Status |
|---|---:|---:|---|
| OpenGeocoding | 1,359 / 1,395 | 97.4% | Frozen local baseline |
| Google Geocoding API | 1,313 / 1,395 | 94.1% | Corrected-adapter pass complete; no normalization failures |
| Geocodio | 1,235 / 1,395 | 88.5% | Complete; regional provider |
| Geoapify | 1,201 / 1,395 | 86.1% | Complete |
| OpenCage | 960 / 1,395 | 68.8% | Complete |
| geocode.maps.co | 728 / 1,395 | 52.2% | Complete |

Geocodio's Canadian full postcodes may be echoed from the request rather than independently verified. Its `place` type conflates city and ZIP centroids and is conservatively credited at locality level, not postcode level.

## Providers not measured

A missing result is not a failed geographic test and receives no score.

| Service | Status |
|---|---|
| Mapbox | Permanent-geocoding access returned HTTP 403; responses collected with temporary geocoding are not published |
| HERE | No collection; applicable evaluation and retention rights need review |
| TomTom | Collected; not published while evaluation and publication rights are reviewed |
| Esri ArcGIS | No collection; the Location Platform agreement requires prior written permission for benchmarking and publication |
| LocationIQ | Collected; not published while retention and publication terms are reviewed |

## What a pass means

- Score the **first forward-geocoding result**, not a best-of-many answer. There is no request-latency comparison.
- Ignore native confidence and OpenGeocoding's `goodMatch`; these have different meanings across services.
- Require an allowed country, geographic tolerance and minimum/maximum resolution from the existing independent expectation. Region codes are checked when available. Missing ISO region codes are flagged, not fabricated, and may fall back to a distance check; this is not a polygon-containment test.
- Requested house-number and postcode components must agree when returned, with the documented ZIP+4 exception. Missing metadata is not always a component failure; missing house precision cannot pass a house-level case.
- Preserve known-failure labels and all selected expectations and tolerances. A known failure still counts as a failure.
- Count query rejections and unusable responses as errors, not empty geocodes. Report normalization limitations separately rather than attributing every such error to provider accuracy.

This measures compatibility with these geographic and address-precision requirements, not verified postal deliverability, entrance accuracy, unit correctness, confidence calibration, reverse geocoding or autocomplete. Some landmark cases have a 1 km tolerance and measure coarse coverage.

## Google normalization status

The initial measurement used revision `5d7d9aaf754caec39edc5a57156ba78d73f7516c`; the completed rerun uses the Google adapter in revision `9207fda`. Its evaluator binary SHA-256 is `ad3202718dccb4211cc91184c9727a66e2148b2adad0256b83dcde8979c17b1e`. The changes were:

- Recognize native `sublocality_level_3` through `sublocality_level_5` without promoting inherited house/street context.
- Accept a native postal town or other locality component for address context, and a street number even when road/city metadata is absent.
- Resolve a missing or malformed two-letter country short name from the **explicit native country name**, using the existing reference aliases. Never infer it from the request or gold answer, or override a valid native two-letter country code.
- Treat an explicitly political colloquial area without a more specific native type conservatively as a region, not a city or address. Genuinely absent country metadata remains an error.
- Retain only fixed, whitelisted aggregate failure categories. Unknown diagnostic text and provider addresses/coordinates are never written.

The complete 4,183-query rerun consumed 4,184 attempts, including one blocked attempt; the run resumed after the quota setting was updated. Completed cases were preserved on resume. The 36 remaining failures are categorized by the first failing normalization check:

| Category | Cases | Interpretation |
|---|---:|---|
| Missing address context | 16 | No number, street or supported locality component sufficient for the current normalized address contract |
| Unsupported `plus_code` result | 12 | Encoded location reference; not automatically a verified street or house identity |
| Missing country component | 7 | Country cannot be invented from the query, gold answer or location |
| Unsupported `political` result | 1 | No supported native primary precision established |

These are first-failure categories, not proof that every other field is valid. All remain in the denominator as errors. Tuning-only schema probes are not an additional accuracy sample. The Google rows remain provisional while these interoperability limits persist. This rerun is a new live provider snapshot: since historical Google responses were discarded, individual class changes cannot be retrospectively audited or attributed solely to the adapter changes.

## Corpus and reproducibility

The source is [`tests/gold.jsonl`](tests/gold.jsonl) plus [`tests/registry/us.jsonl`](tests/registry/us.jsonl), using the corpus, scorer and non-Google adapters at revision `5d7d9aaf754caec39edc5a57156ba78d73f7516c`, and the Google adapter identified above. Of 4,188 cases, four have no forward expectation and one checks this service's RPC-specific `invalid_argument` contract. The geographic comparison therefore contains 4,183 cases. The region-specific comparison contains 1,395 of those same cases. The scored gold data comes from the licensed authoritative sources described in the [evaluation guide](docs/evaluation/README.md#gold-corpus) and [NOTICE](NOTICE), never from these competitors' answers.

| Artifact | SHA-256 |
|---|---|
| Source `tests/gold.jsonl` | `f5469237a5b452bd9732b0bf8d837de977d1f5faabc1b790a2511ffe413b60bb` |
| Source `tests/registry/us.jsonl` | `6c4cbe8ef098eea7855212473c2bd08d2b52c002eb687cc04f9a3696651f94b7` |
| Selected worldwide JSONL | `43e9169f0af02912fce8a21cb470a79ef861a52e0a17db1d8764726ea5de07ef` |
| Selected US/Canada JSONL | `4d7c1c542260a523e0612a512ca9774e31dadc48a334aa9c716d46c6f30b27c2` |
| Frozen local service binary | `ee5c78eba675129ec549fc7a47534430eb132612c1cd39f7a48ce7c5b0586ac5` |
| Local libpostal models | `65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7` |
| Local reference data | `5cebf4b7a553e651b2d9ea610bc3f373d588fceb36972fd9a635a0bdb1628683` |

The local manifest identifies index metadata as `13d14f5234c6` and source revision `5d495a65f7d4cfe1e601773b6d6dd85e7c4ca025`, with a dirty working tree. This is a historical saved baseline, **not a clean-release or current-deployment measurement**. Its executable/model hashes identify the run, but a commit alone does not reconstruct the dirty build. Exact historical baseline reproduction remains a limitation.

At that measurement revision, reconstruct the selected input without contacting any provider:

```sh
mkdir -p eval-results
python3 - <<'PY'
import json
from pathlib import Path
files = [Path('tests/gold.jsonl'), Path('tests/registry/us.jsonl')]
cases = [json.loads(line) for p in files for line in p.read_text().splitlines() if line.strip()]
cases = [c for c in cases if c.get('expect') is not None and c['expect'].get('error') is None]
assert len(cases) == 4183
Path('eval-results/scorecard-corpus.jsonl').write_text(''.join(json.dumps(c, ensure_ascii=False) + '\n' for c in cases))
PY
```

Use [`evaluate provider`](docs/evaluation/providers.md#offline-competitor-comparisons) with the selected corpus and matching adapter/scorer revision. Google uses `--aggregate-only --fetch --budget N`, never a response cache. Other collected providers replay their authorized private caches; new fetches require an approved budget and applicable rights. Compare against the same saved local binary/index run, not a newly deployed service. Live re-queries can change as provider data changes; they are new measurements, not byte-identical reproductions of these historical responses.

| Service | API and fixed request settings |
|---|---|
| Google Geocoding API | Geocoding v3; free-text address, `language=en`; score first result |
| Geoapify | Geocoding v1; free-text text, `lang=en`, `format=json`, `limit=1` |
| OpenCage | Geocoding v1; free-text q, `language=en`, `limit=1`, `no_annotations=1`, `no_record=1` |
| geocode.maps.co | Search; free-text q, `accept-language=en`, `format=geocodejson`, `addressdetails=1`, `limit=1`, `extratags=0`, `namedetails=0` |
| Geocodio | API v2; free-text q, `limit=1`, no data appends, native language/US defaults |

No country or proximity hints are supplied. Configurations are not optimized per country. Different native response schemas and default worldviews can affect results. Provider data was collected across multiple dates. The small earlier 200/2,500-query pilots are not extra independent samples or additional rows in this scorecard.

## Retention, attribution and publication

Only aggregate Google statistics and our own input/configuration metadata are published here: no Google response JSON, normalized addresses, coordinates, place IDs or per-case Google outcomes. Google collection used one-pass in-memory scoring and aggregate checkpoints. Other providers' native responses remain private; retain the licences and attribution supplied with those responses, including © OpenStreetMap contributors / ODbL where applicable.

Product names identify the APIs measured; no affiliation, sponsorship or endorsement is implied. Google's [EEA §6.5](https://cloud.google.com/terms/maps-platform/eea) and [general §3.2.4](https://cloud.google.com/maps-platform/terms) address public benchmark disclosure and require information needed to replicate the test. This record's normalization and baseline-reproduction limitations must not be hidden. Aggregate-only collection is data minimization, **not legal clearance** for every use or publication. Review the applicable agreement and comparative-advertising requirements before turning these diagnostic measurements into website marketing claims.
