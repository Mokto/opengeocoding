# Forward-geocoding scorecard

**Diagnostic measurements, not an unbiased production-accuracy estimate.** This is a vendor-run evaluation on this repository's gold corpus. Cases overlap the local index's source data; variants of one location are not independent samples. Do not read these tables as a general accuracy ranking.

The newest measurement is the OpenGeocoding 1.5.0 comparison below. The 4,183-query tables after it are an earlier, separate measurement; do not compare percentages across tables. Earlier comparisons, of OpenGeocoding 1.0.1 on 18,129 queries and 0.18.0 on 11,671 matched queries, are in this file's history ([1.0.1](https://github.com/Mokto/opengeocoding/blob/4b49315f1bf21fcd4b72275ed34639ae3fa7280b/SCORECARD.md), [0.18.0](https://github.com/Mokto/opengeocoding/blob/290f5226acb14a990d1094771c504399b530f8e9/SCORECARD.md)).

## OpenGeocoding 1.5.0 against cached provider responses (31 October 2026)

Google was not measured in this round. Provider responses were collected 17–31 October 2026 and re-scored offline; OpenGeocoding 1.5.0, released on 30 October 2026, ran locally on 31 October. 1.5.0 is the release the hosted API runs, checked on 31 October 2026. Every provider listed answered every case. Rates use all selected cases, including known failures, abstentions, API rejections and normalization failures. Mapbox, LocationIQ and TomTom collections are not published; see [Providers not measured](#providers-not-measured).

### Full forward selection: identical 24,350 queries

| Service | Passed / 24,350 | Pass rate | Held-out pass rate (4,847) | Only OpenGeocoding 1.5.0 passed | Only this provider passed |
|---|---:|---:|---:|---:|---:|
| **OpenGeocoding 1.5.0** | 21,744 | 89.3% | 90.2% | — | — |
| Geoapify | 18,987 | 78.0% | 78.7% | 3,708 | 951 |
| OpenCage | 14,126 | 58.0% | 59.3% | 8,427 | 809 |
| geocode.maps.co | 11,724 | 48.1% | 48.9% | 10,492 | 472 |

### US/Canada: identical 1,947 queries

| Service | Passed / 1,947 | Pass rate | Held-out pass rate (414) | Only OpenGeocoding 1.5.0 passed | Only this provider passed |
|---|---:|---:|---:|---:|---:|
| **OpenGeocoding 1.5.0** | 1,897 | 97.4% | 97.6% | — | — |
| Geocodio | 1,738 | 89.3% | 87.2% | 191 | 32 |
| Geoapify | 1,577 | 81.0% | 78.7% | 346 | 26 |
| OpenCage | 989 | 50.8% | 51.4% | 921 | 13 |
| geocode.maps.co | 754 | 38.7% | 34.5% | 1,152 | 9 |

### By cohort

The cohorts differ in difficulty and in level (house, street or town), so the full-corpus rate depends on their mix. The first row is the selection of the 1.0.1 comparison.

| Cohort | Cases | OpenGeocoding 1.5.0 | Geoapify | OpenCage | geocode.maps.co |
|---|---:|---:|---:|---:|---:|
| Curated gold and the first 22 registry samples | 18,129 | 95.7% | 82.1% | 59.2% | 49.9% |
| Registries added in October 2026 (below) | 1,961 | 80.7% | 57.1% | 25.7% | 24.9% |
| GLEIF company addresses, town level | 976 | 91.7% | 75.7% | 74.4% | 31.5% |
| Towns in countries without a registry (Wikidata) | 3,240 | 57.7% | 68.4% | 66.4% | 57.3% |
| Place strings as people profiles write them | 44 | 100.0% | 56.8% | 45.5% | 38.6% |

| Registry added in October 2026 | Cases | OpenGeocoding 1.5.0 | Geoapify | OpenCage | geocode.maps.co |
|---|---:|---:|---:|---:|---:|
| Hong Kong (ALS) | 240 | 233 | 113 | 0 | 0 |
| Estonia (ADS) | 240 | 240 | 102 | 27 | 17 |
| Lithuania (Registrų centras) | 200 | 200 | 52 | 0 | 0 |
| Slovakia (RA) | 200 | 196 | 172 | 132 | 131 |
| Slovenia (GURS) | 192 | 192 | 168 | 107 | 92 |
| Finland (Ryhti) | 169 | 166 | 145 | 41 | 36 |
| Austria (BEV) | 156 | 156 | 153 | 132 | 121 |
| Latvia (VZD) | 244 | 105 | 161 | 60 | 89 |
| Mexico (INEGI DENUE) | 320 | 94 | 53 | 4 | 2 |

### By registry

Each address-registry sample in the selection, in country-code order: cases passed out of the cases asked. [registry-comparison-1.5.0.tsv](docs/evaluation/registry-comparison-1.5.0.tsv) adds held-out counts and, for each provider, the cases only one side passed with the exact two-sided sign-test p-value on them. Geocodio answered only the United States and Canada. Against every provider it answered, OpenGeocoding 1.5.0 passed significantly more cases (p < 0.05) in 26 of the 31 registries. In Austria, Berlin and South Korea it is ahead of OpenCage and geocode.maps.co but level with Geoapify (South Korea: 118 against 110). In Serbia it is ahead of Geoapify but level with OpenCage and geocode.maps.co. In Latvia it is ahead of OpenCage, level with geocode.maps.co, and behind Geoapify, which is significantly ahead (161 against 105).

| Country | Scope | Cases | OpenGeocoding 1.5.0 | Geoapify | OpenCage | geocode.maps.co | Geocodio |
|---|---|---:|---:|---:|---:|---:|---:|
| Austria | national | 156 | 156 | 153 | 132 | 121 | — |
| Australia | national | 522 | 519 | 486 | 298 | 91 | — |
| Belgium | national | 384 | 381 | 365 | 294 | 258 | — |
| Brazil | national | 316 | 310 | 251 | 5 | 2 | — |
| Canada | national | 550 | 526 | 376 | 28 | 25 | 503 |
| Switzerland | national | 780 | 778 | 732 | 615 | 559 | — |
| Colombia | Bogotá | 567 | 564 | 273 | 9 | 12 | — |
| Czechia | national | 567 | 551 | 480 | 344 | 329 | — |
| Germany | Berlin | 192 | 192 | 187 | 180 | 180 | — |
| Denmark | national | 1,883 | 1,878 | 1,760 | 1,611 | 1,246 | — |
| Estonia | national | 240 | 240 | 102 | 27 | 17 | — |
| Spain | national | 1,544 | 1,498 | 1,052 | 117 | 92 | — |
| Finland | national | 169 | 166 | 145 | 41 | 36 | — |
| France | national | 808 | 801 | 754 | 716 | 263 | — |
| Hong Kong | national | 240 | 233 | 113 | 0 | 0 | — |
| Italy | national | 320 | 311 | 177 | 54 | 49 | — |
| South Korea | national | 414 | 118 | 110 | 39 | 5 | — |
| Lithuania | national | 200 | 200 | 52 | 0 | 0 | — |
| Luxembourg | national | 240 | 239 | 213 | 186 | 186 | — |
| Latvia | national | 244 | 105 | 161 | 60 | 89 | — |
| Mexico | national | 320 | 94 | 53 | 4 | 2 | — |
| Netherlands | national | 1,026 | 1,026 | 981 | 840 | 932 | — |
| Norway | national | 1,071 | 1,068 | 1,024 | 896 | 897 | — |
| New Zealand | national | 545 | 543 | 484 | 471 | 453 | — |
| Poland | national | 927 | 926 | 720 | 708 | 739 | — |
| Serbia | national | 296 | 280 | 247 | 273 | 269 | — |
| Singapore | national | 362 | 358 | 211 | 73 | 62 | — |
| Slovenia | national | 192 | 192 | 168 | 107 | 92 | — |
| Slovakia | national | 200 | 196 | 172 | 132 | 131 | — |
| Taiwan | Taipei | 622 | 620 | 574 | 196 | 0 | — |
| United States | national | 850 | 835 | 709 | 527 | 347 | 776 |

### Read with these limits

- **The selection is frozen at corpus revision `142508e`,** the cases providers were asked: the curated gold cases, 31 national address-registry samples, and the towns, GLEIF and people cohorts. It leaves out the OpenStreetMap global-address panel (OpenStreetMap is an input to this index and the data behind some providers), landmark and suggestion-only cases, and RPC error contracts. The 1.0.1 comparison's text said its selection held every forward case of its revision; it lacked the towns, GLEIF and OpenStreetMap cohorts.
- **Most of these cases were available during development.** Releases include fixes aimed at these registries, several of them shipped between 1.0.1 and 1.5.0 for the registries added in October. The held-out column scores the fifth of locations kept out of tuning by a stable hash; it is closer to, but not, an independent sample, since the registries are also index inputs. Many Wikidata settlements were imported from GeoNames, which the index also reads.
- **The local index is close to production, not identical.** It was built with the 1.5.0 builder (`0d6237d777e33303`, unchanged since 1.4.0) from Overture 2026-10-21.0, the OSM extracts the chart lists, OpenAddresses, Korea's commercial-building addresses, Ryhti's Swedish-only addresses, DENUE establishments registered before 2025 and GeoNames, from local snapshots of the sources that publish only "latest". The Mexico cases sample establishments registered from 2025, so they are not index inputs. Two complete runs returned identical responses for all 24,350 cases.
- **Per-registry results need the same care.** Many of these registries, or datasets built from them, are also index inputs for OpenGeocoding, so a pass there measures fidelity to the register more than independent accuracy; the Mexico sample is the exception, drawn from establishments registered after the index's cut-off. The samples are not nationally representative: each registry's guide describes its draw, and some are targeted (Korean school sites, Mexican business establishments, one Luxembourg building per commune and number class). The Berlin, Taipei and Bogotá rows cover one city each and do not speak for Germany, Taiwan or Colombia. Samples of 150 to 1,900 cases are small, so read a difference as a real gap only when its sign-test p-value is low. Most registries ask two or more query forms of each location, and the test counts cases, not locations; those variants are not independent, which makes the p-values optimistic, and with up to four providers per registry some p-values below 0.05 are expected by chance.
- **Weak areas remain.** Towns in the 78 countries and territories without a registry sample pass 57.7%, behind Geoapify (68.4%) and OpenCage (66.4%); most failures answer only the country, because the settlement is missing from the index. Latvia passes 105 of 244 (Geoapify 161) and Mexico 94 of 320. Of 414 Korean registry addresses, 118 pass (Geoapify 110). Of 151 street and house cases in Asian countries without a registry sample, 40 pass, against 61 for Geoapify. House-number coverage in most of Asia and Africa is still thin; see the [1.4.0 coverage table](docs/evaluation/house-number-coverage-1.4.0.tsv).

| Artifact | Value |
|---|---|
| Corpus revision | `142508e7ee8e96935ae58d75909ac795508a5ab7` |
| Full 24,350-case forward selection / held-out, JSONL SHA-256 | `e7faae5d81188181645c8e386cea6042441654e70072e9b94d39e3aa9d701f7f` / `fb8586e628c0e0d56d22ba016e620c234f446b59b7c16a04a58e2d527d917a36` |
| US/Canada subset / held-out, JSONL SHA-256 | `2a6eb60eca782589e128d3c7943758344a52b8e175ccd3958c24ac34255568c2` / `3d6cf1a7131811a41a05abf7f52c3eaf48ac7c83f7963532c3296c10da05603f` |
| Evaluator and provider adapters | release 1.5.0, binary SHA-256 `1b10b3451ba21bbe033573a9bf5121ef12737b35ab19854d46d92e25c91bdfcb` |
| OpenGeocoding 1.5.0 service binary | release commit `ef58300`, SHA-256 `1aa3dee63967363c1a75a0068a524996ff0a12c61ec91a7804cf11bbfb6a2781` |
| 1.5.0 index | builder `0d6237d777e33303`, `meta.json` SHA-256 `bebfec8f94865bb88e8aae3c711ff07698dcf66787367d533b7e7f4e8d942cce` |
| libpostal models | `65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7` |

The forward selection keeps cases with a forward expectation, as described above. Request settings are those listed under Corpus and reproducibility; scoring uses the 1.5.0 evaluator, whose pass decisions and failure classes on these responses match the 1.0.1 evaluator's.

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
