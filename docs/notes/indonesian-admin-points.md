# Indonesian administrative points

## Sources and preparation

The ordinary GeoNames populated-place loader reads feature class `P`, not
administrative features. The names-only [hierarchy supplement](indonesian-hierarchy.md)
adds ancestry but cannot supply missing district coordinates. This supplement
uses GeoNames' own `ADM3` points; it does not borrow a town's coordinates or
calculate a centroid.

The complete cached GeoNames `allCountries.zip` from 2026-10-27 is pinned by
SHA-256 `efb8e27e66ba86d8269b8f57718d5872447d6f675249194c07152990d8577740`.
It contains 38 Indonesian ADM1, 514 ADM2, 7,113 ADM3 and 82,394 ADM4 records.
The [preparation manifest](../../data/id-admin-points.sources.json) pins both
inputs, output hash, crosswalk counts and abstention reasons. GeoNames is
CC BY 4.0; the names/code source retains its MIT terms. See the full
[attribution](../../data/id-admin-points.NOTICE) and existing license texts.

```sh
python3 data/test_prepare_id_admin_points.py
python3 data/prepare-id-admin-points.py /path/to/pinned/allCountries.zip
```

Preparation uses the standard library only. It scans the complete snapshot,
not evaluation-selected names. Administrative-code fields identify GeoNames'
own parents; they are never treated as Kemendagri or BPS codes. Names and
aliases must map to one source unit, and each source unit to one GeoNames row.
County matching preserves `Kota` versus `Kabupaten`. A supplied county code
must resolve consistently. Without a county code, a district name must be
unique within its matched province. Non-Latin letters are not silently deleted
to create a Latin-name match. Duplicate identifiers, parent-code ambiguity,
wrong-level labels and aliases spanning source units are rejected.

This produces 6,572 district candidates. Two share identical coordinates with
another district candidate; both are excluded, leaving 6,570 pinned candidates.
These are candidate identities, not 6,570 new index points.

ADM4 has much weaker parent evidence: 94 records crosswalk with explicit district
references, while 82,293 have no usable parent crosswalk. None is imported as a
populated settlement. A village administrative point alone does not establish
where people live or justify a nearest-district assignment.

## Builder and serving semantics

After the existing hierarchy enrichment, the builder requires a unique mapped
province/county and the candidate point's unique containment in that same county
polygon. Conflicting or ambiguous native district mappings are rejected.
Existing district records with coordinates are preserved. A fresh names-only
parent may receive the independently supplied point at its existing ID; otherwise
a typed district row is appended. Existing village coordinates, names and parent
references are not rewritten by this point step. Accepted links retain GeoNames
IDs, government codes, source names and match evidence in
`id-admin-point-links.tsv`; `id-admin-point-summary.json` records rejection counts.
The index also carries the pinned manifest and notice.

Rows have class `id_geonames_admin`, subtype `localadmin`, zero population and no
invented bounding box. Rendering and autocomplete label them as `district`, not
`municipality`, town or village. They rank as supplementary points, not populous
settlements. Existing polygon-based street assignment does not use their points;
reverse locality fallback still uses settlement points, not these districts.

The points occupy a reserved `id_admin|` term namespace, separate from both
ordinary place names and small populated places. It is deliberately not a
two-letter country prefix. Ordinary SearchAddress settlement lookup does not
use these records. No parser fallback, `city_agrees` rule or confidence threshold
changes. Independently supplied administrative coordinates are not evidence of
a populated centre.

Autocomplete considers the supplement only after all existing completion and
rewrite paths, and only for spare slots. Every qualifier is checked against the
candidate's own ancestry, and the original query's head must retain compatible
administrative intent. Village, regency and province labels cannot acquire
district points. The supplement cannot displace an existing completion; added
districts precede landmarks only when there is room for both. A separate same-name
ADM3 fixture checks that the native Mekar Harum completion retains priority.

A custom `--indonesia-hierarchy` disables embedded points unless an authored
`--indonesia-admin-points` file is supplied too. This keeps diagnostic source
snapshots separate. The service performs no preparation, download or write.

## Local build audit

The frozen-input comparison starts with 6,252,587 rows from the preceding
Indonesian lookup build. Exactly 4,188 existing `id_hierarchy` district rows gain
a class marker and coordinates; all their other fields remain byte-identical.
Another 370 district rows are appended. The other 6,248,399 original rows are
byte-identical, including every original village row. The complete hierarchy
link audit is unchanged. These are import counts, not accuracy gains.

Of the 6,570 candidates, 1,951 existing districts are preserved, 30 fail unique
county containment, 29 lack a mapped native county, and two have ambiguous native
district mappings. The remaining 4,558 receive points as described above.

## Rejected prototype

The initial full-index comparison gained 208 SuggestAddress checks but lost two
SearchAddress passes. It failed the gate with four newly failing checks and eight
worse failure classes. New exact administrative names could crowd existing
settlement lookups, and the attempted structured point fallback did not establish
settlement confidence or sufficient positional precision. Lower suggestion weight
alone also failed to preserve all native completions.

That SearchAddress fallback was removed. Separate term storage and a final
spare-slot completion pass replace the interfering lookup paths. The corpus,
expectations, failure labels and scoring are unchanged. The prototype index was
deleted after recording the failed comparison; it was not promoted or published.

## Local validation

On the same frozen full-index inputs and models, the final comparison covered
46,320 cases. Both the immutable baseline comparison and the same-consumer,
old-index/new-index comparison had no newly failing checks or worse failure
classes. The candidate consumer on the old index had zero response/score drift;
a repeat of the baseline consumer also had zero drift.

| RPC | Baseline passing | Candidate passing | Responses changed |
|---|---:|---:|---:|
| SearchAddress | 38,774 / 45,953 | 38,774 / 45,953 | 0 |
| SuggestAddress | 40,630 / 49,730 | 40,861 / 49,730 | 243 |
| ReverseGeocode | 38,640 / 45,575 | 38,640 / 45,575 | 0 |
| ResolveRegion | 9,236 / 9,424 | 9,236 / 9,424 | 0 |

Only Indonesia changed. Its SuggestAddress checks improved from 1,225 to 1,456
of 1,953; the independent places panel improved from 1,141 to 1,372 of 1,853.
Checks are not independent locations. Among the fixed 402 non-holdout locations,
districts with any full-query completion failure fell from 163 to 32 of 247.
Village completion failures stayed at 55 of 155. Search failures stayed at
84 district and 42 village locations. Held-out cases were aggregate validation,
not individual diagnosis or tuning input.

Release builds, all-target clippy, library tests (125), matching tests (67), chart
tests (5), the six hierarchy/point-import tests, authored suggestion and
model-backed SearchAddress checks, evaluator corpus checks and the five stdlib
preparation tests passed. Regeneration reproduced both the point gzip and manifest
byte-for-byte. The final build's place rows were also byte-identical to the
rejected prototype: lookup isolation, not altered point data, removed the losses.
Builder hash: `cdcdb8aab27d1c3d`.

RPC integration remained 1/2 on both baseline and candidate: the oversized-query
log assertion failed. The documented intermittent grpcurl hang is not cleared by
the passing contract check. The complete ordinary suite and local Docker images
were not verified. Local timing comparisons varied; they do not establish request
performance or CI speed clearance. No current-head CI result is claimed.

## Limits

County geometry and unique names corroborate identity; they do not establish
ADM3 boundary containment, a surveyed administrative seat or current legal
membership. GeoNames' dates do not guarantee currency, and the pinned hierarchy
has its own documented legal-version limitations. Missing or ambiguous parents,
conflicts, duplicated points and incomplete geographic scope abstain rather than
borrowing another unit's point. Search and autocomplete counts are not independent
locations or production accuracy estimates.
