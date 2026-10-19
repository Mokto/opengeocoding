# Provider evaluation guide

[Overview](../README.md) · [Evaluation](evaluation.md) · [Scorecard](../SCORECARD.md)

Run commands from the repository root.

[Offline comparisons](#offline-competitor-comparisons) · [Aggregate-only mode](#aggregate-only-evaluations)

## Offline competitor comparisons

[SCORECARD.md](../SCORECARD.md) is the single aggregate comparison record, including unresolved normalization and reproducibility limitations. Its corrected-adapter Google pass is complete, but the measurements remain provisional because some responses still lack required metadata or a supported native type.

`evaluate provider NAME` evaluates a competitor's first forward-geocoding result against the same corpus. Adapters live in `src/bin/evaluate/providers/{provider}.rs`; `providers/mod.rs` shares caching, request budgets, scoring and paired reports. It is **offline by default**: a cache miss fails before any network access. Use `--corpus FILE` or `--only REGEX` to select cases; otherwise it reads the gold suite, including registry-first samples. `evaluate mapbox` remains an alias for `evaluate provider mapbox`, with the same cache keys and paths.

| Provider | API | Credential environment variable | Offline-storage gate |
| --- | --- | --- | --- |
| `mapbox` | Geocoding v6 | `MAPBOX_ACCESS_TOKEN` | Always requests permanent geocoding |
| `google` | Google Geocoding v3 | `GOOGLE_MAPS_API_KEY` | Requires `--storage-authorized` |
| `here` | HERE Geocoding and Search v7 | `HERE_API_KEY` | Requires `--storage-authorized` |
| `tomtom` | TomTom Search v2 Geocode | `TOMTOM_API_KEY` | Requires `--storage-authorized` |
| `geoapify` | Geoapify Geocoding v1 | `GEOAPIFY_API_KEY` | Storage allowed with attribution |
| `opencage` | OpenCage Geocoding v1 | `OPENCAGE_API_KEY` | Storage allowed; retain data licences |
| `mapsco` | geocode.maps.co Search | `MAPSCO_API_KEY` | Result export allowed; retain OSM attribution and licence |
| `locationiq` | LocationIQ Search v1 | `LOCATIONIQ_ACCESS_TOKEN` | Requires `--storage-authorized`; plan-dependent retention |
| `geocodio` | Geocodio API v2 | `GEOCODIO_API_KEY` | Requires `--storage-authorized`; verify dataset terms |
| `arcgis` | Esri ArcGIS World Geocoding | `ARCGIS_API_KEY` | Requires `--storage-authorized`; always requests `forStorage=true` |

Mapbox [allows indefinite storage only for permanent geocoding](https://docs.mapbox.com/api/search/geocoding/#storing-geocoding-results). Its fetches always set `permanent=true`; the account needs a valid payment method or an enterprise contract permitting permanent geocoding. Caching avoids repeat charges, not the initial cost.

Other providers' storage rights are **not** inherited from Mapbox. [Google's standard policy restricts response storage](https://developers.google.com/maps/documentation/geocoding/policies); [HERE](https://www.here.com/terms/here-platform-terms) and [TomTom](https://developer.tomtom.com/terms-and-conditions) also have contract-specific restrictions. For these adapters, use `--storage-authorized` only after verifying that your applicable agreement permits retaining raw responses and normalized reports for this offline evaluation. The flag grants no rights, supplies no automatic expiry or deletion, and does not turn a temporary-cache allowance into permission for indefinite storage. Observe any retention deadline yourself. Confirm benchmarking and use terms too.

[Geoapify permits storage with attribution](https://www.geoapify.com/geocoding-api/) and [OpenCage permits retaining results](https://opencagedata.com/faq#caching); retain their source attributions and licences. [geocode.maps.co permits exporting and using results](https://geocode.maps.co/terms) subject to source-data licences; retain its OpenStreetMap attribution. Its adapter uses GeocodeJSON's address levels and explicit primary postcode tags, never inherited address components, to determine precision. LocationIQ retention depends on the plan or active subscription, so it is storage-gated. Geocodio is also conservatively gated pending review of the applicable dataset terms. ArcGIS always requests [stored geocoding](https://developers.arcgis.com/rest/geocode/find-address-candidates/#forstorage) (`forStorage=true`), which requires a token with the corresponding privilege and may consume credits.

Keep caches and reports private, under the ignored `eval-results/` directory or outside the checkout. Never commit provider responses or credentials. Provision the credential through your secret manager in the process environment, not a command-line argument or committed file:

```sh
# Inspect cache misses without a token or network access (fails on misses).
target/release/evaluate provider mapbox --cache eval-results/mapbox-cache --out eval-results/mapbox

# Fetch missing queries. Replace N with an approved request budget.
target/release/evaluate provider mapbox --cache eval-results/mapbox-cache --out eval-results/mapbox \
  --fetch --budget N

# Re-score offline against a saved local or competitor run directory or JSON snapshot.
target/release/evaluate provider mapbox --cache eval-results/mapbox-cache --out eval-results/mapbox \
  --compare eval-results/local-gold
```

For another adapter, replace `mapbox` in the command and paths; add `--storage-authorized` where the table requires it. A provider is never fetched automatically as part of another provider's run. A budget smaller than the missing-request count fails before sending any. Fetches are serialized at no more than five requests per second (one per second for OpenCage, LocationIQ and maps.co), without retries or redirects. Each HTTP 200 JSON response (including an empty result) and HTTP 422 response is checkpointed before scoring. OpenCage, Geoapify and maps.co HTTP 400 invalid-query responses are also cached and counted as errors, not empty geocodes; an empty query therefore does not stop collection of the other cases. Authentication, rate-limit and server failures stop the fetch. LocationIQ HTTP 404 `Unable to geocode` responses are cached as empty results; 404s from other providers still stop the fetch. For a successful API response, missing country metadata, invalid coordinates and unsupported result types are recorded as per-case `invalid_response` errors; collection continues, never treating them as abstentions or inferring missing facts from the query. Provider API errors inside HTTP 200 still stop the run and are retained for diagnosis, not scored as empty answers; after fixing the account or key, explicitly remove that failed cache entry before fetching again. A transport failure may already have been billed. A cache lock prevents concurrent fetches against the same directory; do not fetch into separate caches for the same experiment.

Cache keys include the exact query, provider endpoint and settings, never the credential or expected answer; different providers cannot share an entry. Raw responses, attribution and collection times are retained. Requests use English (`en-US` for TomTom; Geocodio uses its native language defaults), without country filters or proximity hints; Mapbox also uses `autocomplete=false` and `worldview=us`. Adapters request one result where supported and score only the first. Fixed settings and storage authorization are recorded in the manifest. Provider defaults can still bias answers; these are not optimized country-by-country configurations. Native result types determine granularity: a city is not promoted to a house because its context has an address. Google administrative short names and HERE state abbreviations are not assumed to be ISO 3166-2 codes. Google locality handling includes postal towns and all five sublocality levels; explicit political colloquial areas without a more specific native type are conservatively region-level. A malformed country short name can be resolved from the explicit native country name using the existing reference aliases, never from the query or expectation; genuinely missing country metadata remains an error. OpenCage `postal_city` results are localities, not postcode matches; `subregion` results remain at administrative-region precision. Mapbox secondary addresses are evaluated at building level, not for unit correctness. Geocodio is a regional provider and its native API defaults to the US, not worldwide search; compare relevant-country subsets separately. Its `place` accuracy type conflates ZIP and city centroids and is conservatively scored at locality level. Nearest-rooftop Geocodio matches and out-of-range ArcGIS street addresses are not credited as exact house matches.

`report.md` includes outcomes by cohort, expected country and holdout. `--compare` re-scores both sides against the current corpus, ignoring `goodMatch`; native confidence scores are not equated with it. The paired table counts both-pass, baseline-only, competitor-only and neither-pass cases, and lists different failure classes. Service-specific error-contract cases are excluded. Known-failure labels do not turn failures into passes. No latency comparison is made from cached responses. The curated corpus remains diagnostic, not an unbiased accuracy estimate: many cases were selected from index inputs, and registry expectations can share upstream sources with providers. Adapter tests use made-up responses; they establish neither live interoperability nor competitive accuracy.

### Aggregate-only evaluations

`--aggregate-only` makes one forward request per scored case and retains only aggregate outcome counts, paired pass counts, cohort/holdout and US/Canada groups, request accounting and reproducibility metadata. It never reads or writes a response cache, raw or normalized provider responses, per-case outcomes, coordinates, address summaries or response diagnostics. The output contains `aggregate.json`, `report.md` and an empty lock file, not `results.jsonl`. It requires `--fetch --budget N` and rejects `--cache` and `--storage-authorized`. Use a new output directory.

```sh
target/release/evaluate provider google --aggregate-only --fetch --budget N \
  --corpus cases.jsonl --out eval-results/google-aggregate --compare eval-results/local-gold
```

There are no automatic retries. `--resume` continues from aggregate checkpoints without repeating completed cases; keep the same binary, corpus, filter, baseline and total request budget. Attempts are checkpointed before network access. An interrupted in-flight request remains counted against the budget and may be repeated on explicit resume. Authentication and quota failures stop the run. Google HTTP 400 and `INVALID_REQUEST` responses count as query errors, not empty results. Aggregate-only mode discards declared HTTP query-error bodies without requiring JSON, so an HTML rejection of malformed input does not stop collection. Normalization failures count as adapter errors without retaining response details. Google failures also have fixed, whitelisted aggregate categories (for example, missing country or unsupported type); arbitrary diagnostic text is never retained. Cases without geographic expectations and service-specific error-contract cases are excluded.

No provider responses remain for offline inspection or re-scoring. This option minimizes retained data; it does **not** establish that benchmarking, aggregate retention or publication is permitted under the applicable provider agreement. Review those terms separately, including Google's applicable EEA or non-EEA terms. Never use provider answers as gold labels or to train the service.
