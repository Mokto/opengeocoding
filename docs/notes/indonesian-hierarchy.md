# Indonesian administrative hierarchy supplement

The builder includes a pinned, country-wide names/code table from
[cahyadsn/wilayah](https://github.com/cahyadsn/wilayah), under MIT. It follows
Kepmendagri No. 300.2.2-2138 Tahun 2025; the upstream README also cites the
2430/2025 amendment. The snapshot contains 38 provinces, 514 regencies/cities,
7,285 districts and 83,762 villages/kelurahan, with unique codes and complete
parent links. These are source-table counts, not counts of newly indexed places.

Only `db/wilayah.sql`'s administrative names and codes are used. The source file
itself grants MIT licensing. Other upstream files credit Google Maps for some
coordinates: none of those coordinates, polygons or postal data is included.
The service never fetches this source, and needs no PHP runtime or SQL database.

## Preparation

The complete source is pinned by commit and SHA-256 in
[`prepare-id-hierarchy.py`](../../data/prepare-id-hierarchy.py). Regenerate with:

```sh
python3 data/test_prepare_id_hierarchy.py
python3 data/prepare-id-hierarchy.py --source /path/to/pinned/wilayah.sql
```

Omit `--source` to download that exact revision during preparation. The script
parses tuples as text; it does not execute SQL. It validates the source digest,
national counts, unique codes and parent existence, then writes deterministic
gzipped name/code pairs and their provenance manifest. No evaluator cases,
expected coordinates or seed lists participate in preparation.

The compressed table, manifest, notice and shared Indonesian name rules are
part of the builder hash. Refresh the complete source frame and metadata
together, never select rows from gold failures. Ministry of Home Affairs codes
are not interchangeable with BPS/COD-AB P-codes: punctuation removal is not a
valid crosswalk between them.

## Conservative enrichment

The builder runs the supplement after Overture and GeoNames place loading:

1. Require one unambiguous Indonesian country ancestor and no foreign ancestors.
   Crosswalk provinces and counties by compatible names, existing ancestry and
   consistent region codes. Preserve the city/regency distinction; a bare county
   name is usable only when unique within its source province.
2. Consider only existing village/hamlet records and GeoNames locality gap
   fillers whose immediate parent is a county. That county must uniquely contain
   the original point in the existing Overture polygons. Missing, conflicting or
   overlapping county polygons abstain.
3. Join the village's names to the complete source table within that county.
   Require uniqueness in both directions. Conflicting aliases, repeated source
   village names and duplicate indexed points are unresolved, not merged.
4. Keep existing district ancestry untouched. Reuse an unambiguously matched
   `localadmin` record, or add a names-only district parent. A locality alias that
   mentions a district does not establish administrative-entity identity. Never
   infer a district from a nearby or same-named town, borrow a town's coordinates,
   replace an original point, or resolve a cyclic/ambiguous parent.

Names-only districts retain the source's unprefixed names and their district
subtype; no `Kecamatan` or `Distrik` name is invented. They have no coordinates,
no place search document and no standalone autocomplete result. Render them as
administrative `district` fields, not municipal settlement evidence; hierarchy
alone must not make an early structured fallback claim a city match. Selection
retrieval rejects them. They supply
ancestor names and semantic district qualifiers for their linked villages; they
do not claim district-centre coverage or a district polygon. Original
`places.tsv` rows retain their IDs, coordinates, names and country/region
columns. The supplement supplies no street or house coordinates. Retrieval IDs
remain index-scoped. Adding parent IDs also updates the village's builder ancestry
so street lookup postings and runtime parent chains agree.

`--indonesia-hierarchy JSON.GZ` replaces the input for authored diagnostic
fixtures. The builder writes `id-hierarchy-source.json`, a complete
`id-hierarchy-links.tsv` audit of accepted links, and
`INDONESIA-HIERARCHY-NOTICE.txt` into the index.

## Limits and verification

A unique name/county match corroborates a source hierarchy; it is not a surveyed
identity check or proof of containment in a district boundary. The supplement
contains neither district boundaries nor village coordinates. Unindexed
villages remain unindexed, towns are not promoted to districts, and existing
hierarchy conflicts remain visible. The table follows a 2025 decree, not a claim
that every administrative change through today is covered.

Unit checks cover complete source parentage, the pinned digest, literal names,
source/runtime duplicates, foreign or ambiguous country ancestors, province/region
and city/regency conflicts, existing parents, non-villages, geographic rejection and preservation
of coordinates. Authored synthetic facts exercise both reused and names-only
districts through the real builder, autocomplete, selection and model-backed
search. Full-index comparisons must use frozen geographic sources and the same
consumer/models; compilation and source counts do not establish geographic
accuracy or measured speed. Corpus expectations and failure labels must remain
unchanged.

## Local comparison

On frozen Overture `2026-09-23.1` inputs, with the same consumer and models,
builders `7c97127b13cc31f0` and `99a4a5120d5f8e62` were compared across 46,320
shared gold cases. All 6,247,884 original `places.tsv` rows retained every field
except the parent of 36,137 Indonesian localities. The supplement added 4,703
coordinate-free district rows. Rebuilt-baseline and consumer-only controls had
zero response or score drift.

| RPC | Passing before | Passing after | Checked |
|---|---:|---:|---:|
| SearchAddress | 38,753 | 38,754 | 45,953 |
| SuggestAddress | 40,382 | 40,610 | 49,730 |
| ReverseGeocode | 38,565 | 38,640 | 45,575 |

There were no newly failing checks or worse failure classes. Only Indonesian
answers changed; ResolveRegion, offset reverse checks and flag-off integrity
were unchanged. The added search pass is an OSM-panel house, not a demonstrated
new district centre. The independently selected Indonesian place panel gained
226 suggestion checks and 75 reverse checks, with search unchanged. These are
checks, not independent locations or production accuracy estimates; the corpus
still has existing failures and geographic-radius passes do not verify exact
administrative identity.

Local timings are not the CI speed gate. Model-backed RPC tests still failed
identically on the baseline and candidate: grpcurl timeout and the oversized-log
assertion (0/2 on each). Compilation, authored-fact checks and these corpus
comparisons do not establish gRPC interoperability or a full ordinary-suite
pass; the separately observed baseline POI-test stall remains unresolved.

See the [attribution and MIT notice](../../data/id-hierarchy.NOTICE) and
[pinned provenance](../../data/id-hierarchy.sources.json).
