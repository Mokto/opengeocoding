# Forward-geocoding scorecard

**Diagnostic measurements, not an unbiased production-accuracy estimate.** This is a vendor-run evaluation on this repository's gold corpus. Cases overlap the local index's source data; variants of one location are not independent samples. Do not read these tables as a general accuracy ranking.

**Google remains provisional.** The corrected-adapter pass is complete: normalization failures fell from 54 to 36 worldwide, with none in the US/Canada subset. Remaining failures include insufficient metadata and unsupported native types; they are not proof of incorrect Google locations. No Google response data remains for retrospective inspection. The tables show the completed rerun, not guessed corrections to the initial measurement.

## Worldwide: identical 4,183 queries

Collected 17–18 October 2026; local baseline collected 16 October 2026. Rates use all selected cases, including known failures, abstentions, API rejections and normalization failures.

| Service | Passed / cases | Pass rate | Normalization failures | Status |
|---|---:|---:|---:|---|
| OpenGeocoding | 3,669 / 4,183 | 87.7% | — | Frozen local baseline |
| Google Geocoding API | 3,627 / 4,183 | 86.7% | 36 | Corrected-adapter pass complete; remaining normalization limitations |
| Geoapify | 3,422 / 4,183 | 81.8% | 1 | Complete; one native result has unknown precision |
| OpenCage | 2,766 / 4,183 | 66.1% | 1 | Complete; one result lacks country metadata |
| geocode.maps.co | 2,402 / 4,183 | 57.4% | 0 | Complete |

The Google rerun had 42 errors: 36 normalization failures and six query rejections. A normalization failure is **not proof of an incorrect Google location**. Even an all-36-pass scenario would produce 3,663 / 4,183 (87.6%); this is a generous upper bound for those unresolved cases, not a predicted corrected score. The local raw pass count is higher on this corpus, but the remaining interoperability limitations and corpus bias do not support a general accuracy-superiority claim.

## US/Canada: identical 1,395 queries

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
| Mapbox | Permanent-geocoding access returned HTTP 403 before any results were collected |
| HERE | No collection; applicable evaluation and retention rights need review |
| TomTom | No collection; applicable evaluation and retention rights need review |
| Esri ArcGIS | No collection; the Location Platform agreement requires prior written permission for benchmarking and publication |
| LocationIQ | No collection; retention depends on the applicable plan and terms |

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

The source is [`tests/gold.jsonl`](tests/gold.jsonl) plus [`tests/registry/us.jsonl`](tests/registry/us.jsonl), using the corpus, scorer and non-Google adapters at revision `5d7d9aaf754caec39edc5a57156ba78d73f7516c`, and the Google adapter identified above. Of 4,188 cases, four have no forward expectation and one checks this service's RPC-specific `invalid_argument` contract. The geographic comparison therefore contains 4,183 cases. The region-specific comparison contains 1,395 of those same cases. The scored gold data comes from the licensed authoritative sources described in the [evaluation guide](docs/evaluation.md#gold-corpus) and [NOTICE](NOTICE), never from these competitors' answers.

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

Use [`evaluate provider`](docs/providers.md#offline-competitor-comparisons) with the selected corpus and matching adapter/scorer revision. Google uses `--aggregate-only --fetch --budget N`, never a response cache. Other collected providers replay their authorized private caches; new fetches require an approved budget and applicable rights. Compare against the same saved local binary/index run, not a newly deployed service. Live re-queries can change as provider data changes; they are new measurements, not byte-identical reproductions of these historical responses.

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
