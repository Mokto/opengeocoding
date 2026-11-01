# Greenland: Asiaq numbered-building diagnostics

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/gl.NOTICE)

**Contains data from Asiaq, Greenland Survey, Technical Base Map (BYGNING), obtained 2026-10-31.** This is a building-location corpus, **not a house-address register, GAR extract or entrance benchmark**. B-numbers identify buildings within a locality; they are not ordinary street house numbers.

## Source and permission

Asiaq's [data-distribution page](https://www.asiaq.gl/data-distribution/) explicitly publishes its own technical base maps, including building numbers, for free public/private use. The [download portal](https://kortforsyning.asiaq.gl/files/) links the applicable [Terms of Use for Asiaq Geodata](https://www.asiaq.gl/wp-content/uploads/2026/04/EN_Terms_of_use_for_Asiaq_geodata.pdf). Unlike the unconfirmed terms for the separately published GAR register, this grant applies to **these Asiaq base-map downloads**.

The terms permit worldwide free non-exclusive copying, distribution, publication, modification, combination and commercial/noncommercial use. Credit must include “Contains data from Asiaq, Greenland Survey”, the dataset and acquisition time; recipients must receive the terms or a link. If a service names other sources on its front page, it must name Asiaq there too. No endorsement is implied; Greenlandic law applies, the data are as-is and terms may change. `gl.NOTICE` and each generated case retain the credit and terms link. The terms PDF captured with these files has SHA-256 `59a75ff0156c82add655d5964656f0a5eac1c56927a07048c1dd076ce2ad5cb4`.

The official [GK24 specification](https://www.asiaq.gl/wp-content/uploads/2026/10/V1.1_GK24-Specifikation-for-grundkortlaegning-af-bosteder-i-Gronland_2026.pdf) describes `BYGNING` as roof-outline polygons, including nonresidential buildings; B-number is optional. Other layers include building details and buildings under construction. Only `BYGNING` is used. The source supplies roof geometry and descriptive attributes, not an entrance or surveyed address-access point. Historical base maps are excluded from the free product. Individual objects retain their published creation/edit dates; this is a capture, not a uniform 2026 survey epoch.

## Capture and preparation

Six captures cover **Qaqortoq, Nuuk, Sisimiut, Aasiaat, Ilulissat and Tasiilaq**. [gl.sources.tsv](../../../tests/registry/gl.sources.tsv) pins each official URL, byte count, SHA-256 and CRS. The archives total **215,325,654 bytes**. Retain them: the URLs can be overwritten.

```sh
set -euo pipefail
REPO=$PWD
SOURCE=../gl-hu-data
mkdir -p "$SOURCE/raw"
while IFS="$(printf '\t')" read -r locality town crs url bytes hash; do
  [ "$locality" = locality ] && continue
  zip="$SOURCE/${locality}_TekniskGrundkort_SHP.zip"
  curl -fL "$url" -o "$zip"
  [ "$(wc -c < "$zip" | tr -d ' ')" = "$bytes" ]
  printf '%s  %s\n' "$hash" "$zip" | shasum -a 256 -c -
  mkdir -p "$SOURCE/raw/$locality"
  for part in shp shx dbf prj cpg; do
    unzip -p "$zip" "BYGNING.$part" > "$SOURCE/raw/$locality/BYGNING.$part"
  done
done < tests/registry/gl.sources.tsv
(cd "$SOURCE" && duckdb -bail -f "$REPO/tests/registry/asiaq-buildings.sql")
target/release/evaluate sample-gl-buildings --asiaq-buildings "$SOURCE/asiaq-buildings-4326.jsonl" --seed gl-buildings-2026-10 --out tests/registry/gl-buildings.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/gl-buildings.seeds.jsonl --out tests/registry/gl-buildings.jsonl --asiaq-buildings "$SOURCE/asiaq-buildings-4326.jsonl"
target/release/evaluate run --corpus tests/registry/gl-buildings.jsonl --index out --out eval-results/gl-buildings
```

The shapefiles declare **GR96 / UTM**, with eastings/northings in metres: zone 23N/EPSG:3183 for Qaqortoq, zone 22N/EPSG:3182 for the four west-coast captures, and zone 24N/EPSG:3184 for Tasiilaq. Some carry a GVR2016 vertical reference. Preparation deliberately uses 2D roof geometry and does not evaluate heights. It selects a point-on-surface inside each valid positive-area building outline and transforms that point to WGS84/EPSG:4326 through DuckDB spatial/PROJ with `always_xy=true`. The SQL checks finite Greenlandic coordinates and that each projected anchor is inside its source outline. The adapter also rejects swapped axes, wrong locality/CRS pairings and invalid extents.

The prepared `asiaq-buildings-4326.jsonl` has **12,446 rows**, **4,639,649 bytes**, SHA-256 `b31ac993a8829683473abdb5da57d6b0494c5109e127b08e562baebee0772b90`. Source attributes include `lokal_id`, `lokalitet`, `maalepunkt`, `bygningsty`, `bygningsbr`, `created_da`, `B_nummer` and `last_edit`. The capture includes 12,419 roof-eave (`Tagudhæng`) and 27 ground-reference (`Terræn`) outlines. Types include buildings, outbuildings, water facilities and bus shelters; the frame is not limited to dwellings. Derived points, projected anchors, source CRS and outline bounding-box diagonals are retained in case provenance. No geocoder supplies expectations.

## Frame, selection and checks

The numeric frame requires `B-` followed by one to five ASCII digits, without a leading zero. Missing, compound, differently formatted and zero-padded identifiers are outside the frame; no equivalent spelling is guessed. There are **7,894 eligible numbered-outline records**, **7,884 locality/B-number keys** and **7,874 unambiguous keys** after excluding ten keys whose points or extents conflict. Identical-point/extent repeats keep the lexicographically first source feature id.

The sampler ranks those keys by seeded SHA-256 and takes **12 per captured town: 72 buildings, 144 queries**. Equal town weighting is not population or building-count weighting, and these six towns are not nationwide settlement or rural-building coverage. Building use is not a sampling stratum; public, commercial and unspecified buildings may be included. Source selection and tolerances were fixed before any service answer was seen.

Two forms share each building's location key and the common location-hash hold-out: `B-123, Nuuk, Greenland` and `Nuuk, B-123, Greenland`. The actual draw has **15 held-out buildings / 30 queries**. The examples illustrate the format, not additional cases.

A check requires street level or finer, goodMatch true and a point within the source-derived radius, accepting `gl` or `dk` under the project's territory convention. The radius is `max(50 m, outline bounding-box diagonal + 25 m)`; sampled radii are about **50–106 m**. The full diagonal conservatively covers the mapped outline from an interior anchor; the additional allowance is chosen, not a measured survey-error or entrance bound.

There is **no `house_number` component** and no house-offset reverse check. Neither the RPC nor this expectation identifies a B-number as a separate building-id field. Consequently a nearby different building/address can satisfy the spatial/level/confidence check: a pass is **not proof of B-number identity, a postal address, residence or entrance**. ReverseGeocode checks only the allowed country at the anchor, not the building or municipality. No ResolveRegion score is generated without an expected region. Do not present the country checks as successful building identification.

Twelve of the 72 sampled targets have another sampled building anchor inside their acceptance radius. The source adapter therefore marks these cases `source.scope=building-location-diagnostic`. Reports label their location-only and country/area-only checks explicitly and put them in a separate building-location diagnostics table, excluded from the address/source-fidelity country totals. Standalone reports show the identity limitation prominently. No sample, coordinate, tolerance or failure was changed to resolve this measurement limitation; identity verification requires a future verifiable B-number lookup.

The index already contains Greenlandic OpenAddresses/Overture addresses and Greenland OSM data; OSM may also reuse Asiaq mapping. The builder does not directly read these six shapefiles. Independent selection from a licensed source does not establish independent real-world accuracy or absence of shared ancestry.

## Baseline

On a full index built by `dbec0f0cae254966` from Overture 2026-09-23.1, the existing OpenAddresses inputs and chart/landmark OSM extracts, SearchAddress passes **0/144**: **87 too coarse, 57 abstentions**, with no critical results. Held-out queries pass 0/30. Full-string SuggestAddress also passes **0/144**. ReverseGeocode's limited country-only check passes 144/144; that does not establish building recognition. There is no ResolveRegion or house-offset score.

A training-only nearby-index inspection of 57 buildings finds conventional address numbers and some B-prefixed keys, but no B-prefixed key matching these selected identifiers. That is not proof the identifiers are absent everywhere or under another representation. The current road/house lookup does not resolve these bare building-id queries; neither the queries nor expectations were changed to make them pass. No failure labels were added.

A repeated run has zero response/scoring drift. Re-scoring those saved responses after adding the explicit diagnostic scope changes no outcomes; only their reporting classification changes. This is offline rescoring, not a fresh service/index measurement. The standalone gate passes because it has no untriaged critical/transport/flag-off failures, **not because building retrieval works**. The known fresh-service grpcurl SearchAddress hang reproduced. Local timings are not speed evidence. No runtime matching fix is part of this source addition.
