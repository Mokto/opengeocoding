# PMGSY / GeoSadak habitation supplement

The builder can add PMGSY habitation names through `--geosadak-habitations PARQUET --geosadak-habitations-sha256 HASH`. The chart enables the pinned capture with `index.sources.geosadakHabitations`; disabling it omits the source. No service-startup download is introduced.

## Capture and licence

The source is the Ministry of Rural Development's PMGSY Rural Connectivity Datasets (PRCD22), explicitly published under Government Open Data License–India. Commercial reuse is permitted with attribution and non-endorsement conditions; the publisher's additional geospatial provisions remain applicable. See [the retained attribution, terms and exact capture hash](../../data/in-geosadak.NOTICE).

The public mirror labels the habitation data **20 October 2024**. Its pinned Parquet asset was uploaded in March 2026; that upload is not evidence of a new survey. Direct official access returned HTTP 401 during preparation, so this is not presented as a current official export. The Parquet conversion contains 1,045,400 rows, rather than the original GeoJSONL archive's 1,045,428. These are distinct, separately hashed artifacts; the builder consumes the pinned Parquet, not a locally cleaned export. Upstream refreshes require a new capture/hash and review, never a mutable unverified download.

The source has habitation names, points and recorded populations, not postal house numbers. Some names describe colonies, road junctions or nearby facilities. They are not evidence of official village status, current boundaries, entrance coordinates or positional accuracy. PMGSY state/district/block identifiers are not Census or ISO identifiers and do not assign runtime geography.

## Conservative inclusion

The importer rejects invalid points, missing/numeric names, nonpositive populations and every repeated habitation ID. It never repairs coordinates. A name already present under any name of an indexed Indian place is not added, moved or renamed. A repeated source name is omitted when any point is five kilometres or more from its lowest-ID point; otherwise that lowest-ID point represents the cluster. The retained point must be inside a unique indexed Indian region polygon and inside the build bounds.

Additions use `pmgsy_habitation` and the existing isolated small-place fields. Suggestions can fill spare slots after established results and typo completions; ordinary place/street matching does not see their words. The existing exact name/country fallback requires nearby indexed-address corroboration before it can return their point. Neither street assignment nor reverse-locality anchors use them. No fallback ordering or country conflict guard is relaxed.

The index retains `GEOSADAK-NOTICE.txt` and `geosadak-source.json`; a published build Job also includes the notice in `ATTRIBUTION.txt`. Fetching verifies the SHA-256 before sealing and after reading an existing store mirror; the builder verifies it again before parsing.

## Local validation, 14 November 2026

On the existing full world source frame (Overture 2026-09-23.1 and the same local OSM/OpenAddresses/GeoNames inputs), the importer reads 1,045,400 rows:

| Outcome | Rows |
| --- | ---: |
| Invalid/nonpositive population | 30 |
| Valid rows with repeated ID | 6,112 |
| Name already present in the Indian index | 388,814 |
| Distant source namesakes | 170,284 |
| Outside build bounds or without a unique Indian region | 2,580 |
| Nearby copies represented by the lowest-ID point | 5,415 |
| Added habitation points | **472,165** |

These are input/ingestion counts, not distinct independently verified settlements or national coverage percentages. Invalid rows take precedence over duplicate-ID counting.

The full **46,320-case gold comparison has zero changed answers**, including no newly failing or worse failure classes. A base rerun also had zero answer drift. The paired index comparison used the same consumer binary and models. Existing absolute gold failures remain: the comparison measures non-regression, not a claim that every gold case passes. No expectations or failure labels were changed.

A separate **100-location source-fidelity diagnostic**, frozen in seeded source-ID hash order using only the source's unique-ID/name, positive-population and coordinate checks, compared `name, India` queries on the old and supplemented indexes. SuggestAddress agreement within 2 km of the source point rose **14 → 98**, while SearchAddress stayed **10 → 10**. This is shared-origin ingestion evidence, not independent accuracy. No SEDAC evaluation candidates or held-out answers were used to select inclusion rules.

The two remaining suggestion disagreements illustrate deliberate limits: `LUHSI` already names a mapped place about 18 km from the source point, so the importer does not override it; `Near School` is a generic source label for which existing street completions occupy the result slots. Neither disagreement worsened from the base. They were not removed or used to tune special-case filters.

Checks run:

- `cargo fmt --check`: passed.
- Clippy with Rust 1.99, `--locked --all-targets -- -D warnings`: passed. The default `cargo clippy` wrapper initially selected unsupported Rust 1.97-nightly; the installed Homebrew 1.99 Clippy was used explicitly.
- `cargo test --locked`: passed, including the authored source-ingestion/lookup regression and corrupt sealed-mirror rejection. The first synthetic run exposed a missing authored Kerala region polygon; adding that fixture made the unchanged inclusion guard testable.
- `cargo test --locked --test synthetic -- --ignored --nocapture`: passed with provisioned libpostal models, separately from the non-model tests.
- `INDEX_DIR=... cargo test --locked --test rpc -- --ignored --nocapture`: failed on the documented grpcurl timeout and the oversized-query logging assertion. The logging failure also reproduces on the old index without GeoSadak: the unchanged resolver debug statement logs its full parsed query despite the service span truncating it. Both failures remain visible; no retry, assertion change or unrelated fix was added.

Release linking initially needed the installed libpostal library search path (`LIBRARY_PATH=/opt/homebrew/lib` on the validation machine). No dependency was added. No speed conclusion is drawn from local timings; CI's full-index quality and measured-speed gates remain the release checks. This feature adds autocomplete coverage, not demonstrated forward-address accuracy or a house-number dataset.
