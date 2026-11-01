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

The index already contains Greenlandic OpenAddresses/Overture addresses and Greenland OSM data; OSM may also reuse Asiaq mapping. The builder does not directly read these six shapefiles. Independent selection from a licensed source does not establish independent real-world accuracy or absence of shared ancestry.

## Indexed building references

The builder now includes `data/asiaq-buildings.tsv`, the **entire 7,874-reference unambiguous frame**, independently of the sampler, seeds and hold-out labels. Regenerate it by running the preparation SQL above and copying `asiaq-building-refs.tsv` to that path. Its five columns are locality code, town, B-number, latitude and longitude; the file has 466,938 bytes and SHA-256 `88fa6292038887eb0cfd92a43c00e8594460849e08ad4109975a04bc0a62439b`. The source captures and terms remain the ones pinned above; [the data notice](../../../data/asiaq-buildings.NOTICE) also travels with the index.

`build_index` writes `gl-buildings.json` separately from the house-number FSTs, subject to the build bounds and Greenlandic place assignment. On the local full index, **7,870 references in all six towns** get a parent place; four input references lack that assignment and remain unavailable. No cases were removed, and the source frame and assignment rule were not adjusted after evaluation. The service reads the built map without downloads or writes. A `B-123, Town, Greenland` or `Town, B-123, Greenland` lookup needs the exact source reference and folded published town name. It returns `house_name=B-123`, not postal `house_number=123`, with no invented road. Suggestions use type `building` and an empty category, including with landmarks off; a final B-number prefix is completed only with an explicit Greenland filter. Retrieval uses the same immutable-index opaque ID. In a covered town, an absent reference abstains instead of guessing a plain numeric house. Other towns keep the ordinary path. Reverse lookups are unchanged.

The existing GAR-derived OpenAddresses/Overture feed is **not** the source of these identifiers. Its `BNummer`/`HusNummer` fields and licensing uncertainty do not justify treating a numeric street house number as a building reference, or extending the Asiaq basemap grant to GAR. This addition does not resolve that separate permission question.

A full local comparison against main `1988244`, with unchanged ordinary inputs, models and reference data, improves Greenland SearchAddress **0/144 → 144/144** and full-string SuggestAddress **0/144 → 144/144**, at **72 building locations**, including **0/30 → 30/30** held-out queries. Every returned identifier was also checked against the published B-number: `house_name` agrees, and `house_number`/`road` stay empty. The new evaluator level `building` lies between street and house; a building-only response still fails an ordinary postal-house expectation. No gold expectations, radii, query forms, samples or labels changed.

Across **44,047 shared cases**, the two RPCs each gain 144 passing checks and no scored classes regress; ResolveRegion and both reverse scores are unchanged. An additional full run with the **same candidate binary and same new core index but an empty building map** isolates these exact gains to the source addition. Re-scoring saved base answers changes zero classes. The unchanged base rerun has zero scoring drift, but three lower-ranked US suggestion lists vary (Washington PA, Torrance CA and La Plata MD); opaque suggestion IDs also change normally when the index is rebuilt. These are not claimed fixed here.

**These gains measure source fidelity and query handling, not independent positional accuracy.** The new index and the corpus use the same source points, including held-out locations. Hold-outs still protect against query-specific tuning, not shared-source ancestry. A roof point does not establish a residence, entrance or postal address. The corpus remains location-diagnostic rather than an identity-scored postal register, even though these responses expose and were audited for their B-number names. The country-only reverse score remains 144/144 and is not a building-retrieval result.

Formatting, strict all-target clippy, ordinary Rust tests, exporter checks, corpus validation and the separately run model-backed synthetic test pass locally. The ignored RPC tests fail identically on base and candidate release binaries: the documented grpcurl SearchAddress timeout and oversized-query log assertion. The initial debug-binary invocation additionally timed out waiting for readiness. Those failures are not passes; no tests were skipped or weakened. Local timings are highly variable and do not validate speed; CI quality and measured-speed verification are outstanding.

After rebasing onto main `5104ea8`, the builder/chart pin is **`aae8c670e4b3852f`**. A fresh full-index comparison against that main's `eb225d0b385a41f5` index gives the same Greenland-only gains: Search **36,589 → 36,733 / 43,824** and Suggest **37,957 → 38,101 / 47,360**, with no newly failing or worse classes and all other RPC scores unchanged. The same evaluator binary, models and reference data score both sides. A repeated unchanged base has zero scoring drift. Formatting, strict clippy, ordinary tests, the separate model-backed synthetic test and corpus validation pass after the rebase. The original comparison and data-disabled experiment above remain historical evidence, not measurements against this newer main. CI quality and measured speed remain outstanding.

The subsequent rebase onto main `e12c4f1` retains both the Australian and Greenlandic builder inputs and Docker dependency stubs; the combined builder/chart hash is **`bb05cfe6f44d46bd`**. Fresh full indexes from main's actual builder `060849a64adb798d` and the candidate again give only **+144 Search and +144 Suggest** over 44,047 shared cases, with no newly failing or worse classes; region and both reverse scores are unchanged between those two runs. The unchanged base repeat has zero scoring drift. Formatting, strict all-target clippy, ordinary tests, the separate model-backed synthetic test and a native-arm64 runtime Docker build pass. The image's builder hash matches the chart. The Docker-stub regression was verified failing before the missing Greenland file was included. These local checks do not replace the outstanding CI quality and measured-speed reports.

## Original baseline

On a full index built by `dbec0f0cae254966` from Overture 2026-09-23.1, the existing OpenAddresses inputs and chart/landmark OSM extracts, SearchAddress passes **0/144**: **87 too coarse, 57 abstentions**, with no critical results. Held-out queries pass 0/30. Full-string SuggestAddress also passes **0/144**. ReverseGeocode's limited country-only check passes 144/144; that does not establish building recognition. There is no ResolveRegion or house-offset score.

A training-only nearby-index inspection of 57 buildings finds conventional address numbers and some B-prefixed keys, but no B-prefixed key matching these selected identifiers. That is not proof the identifiers are absent everywhere or under another representation. The current road/house lookup does not resolve these bare building-id queries; neither the queries nor expectations were changed to make them pass. No failure labels were added.

A repeated run has zero response/scoring drift. The standalone gate passes because it has no untriaged critical/transport/flag-off failures, **not because building retrieval works**. The known fresh-service grpcurl SearchAddress hang reproduced. Local timings are not speed evidence. No runtime matching fix is part of this source addition.
