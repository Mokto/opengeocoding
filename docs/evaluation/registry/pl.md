# Poland

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/pl.NOTICE)

Source: **GUGiK Państwowy Rejestr Granic (PRG)**, official national address-point GML export, acquired 19 October 2026. The [official page](https://www.geoportal.gov.pl/pl/dane/panstwowy-rejestr-granic-prg/) explicitly permits free use for any purpose; [Geoportal terms, Article 3](https://www.geoportal.gov.pl/en/about-geoportal/terms-and-conditions/) permit publication with source attribution. The evaluator uses the source-specific identifier `GUGiK-PRG-open-data`; this is not a claim of CC BY licensing or blanket worldwide public-domain status.

## Snapshot and reproduction

Retain the [national ZIP](https://opendata.geoportal.gov.pl/prg/adresy/PRG-punkty_adresowe.zip) outside git, with `PRG-punkty_adresowe.zip.receipt.json` containing `url`, completed `bytes` and `sha256`. The acquired file is **804,116,673 bytes**, SHA256 **0b052fb76a26996a040514cb126fe1a8e1a9a9801a84e500daff447a4f7fccff**. All sixteen members' CRCs were verified. The national URL is mutable: exact regeneration requires the retained archive matching this checksum, not a fresh download. The files were generated on 19 October 2026, with filename times 13:28:44–13:36:25; their timezone is unspecified. These are per-province exports, not a transactional nationwide snapshot at one instant. Municipal record vintages vary.

The [PRGAD 1.0 schema](https://opendata.geoportal.gov.pl/prg/schemas/prgad/1.0/PRGAD.xsd), dated 22 February 2024, defines localities, streets/squares, address points, versions and local GML references. Python 3.9+ and an installed DuckDB CLI with its spatial extension are development prerequisites, not runtime dependencies. Extension autoinstall/autoload are disabled; the preparer makes no network requests.

```sh
python3 -u tests/registry/export-prg-addresses.py --source SOURCE/pl/PRG-punkty_adresowe.zip --out SOURCE/pl/prg.json --seed pl-2026-10
target/release/evaluate sample-pl --prg SOURCE/pl/prg.json --seed pl-2026-10 --out tests/registry/pl.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/pl.seeds.jsonl --out tests/registry/pl.jsonl --prg SOURCE/pl/prg.json
```

Archive URL, size and hash are checked before reading. Exactly one member for each of the sixteen province codes is required. Each province has an atomic JSON checkpoint in `prg.work/`, bound to the archive and preparer hashes, member, DuckDB version, seed and candidate count, with a checked content hash. Changed inputs require fresh output; a different final capture is never overwritten. Raw XML is streamed from the ZIP, not extracted to arbitrary paths, and external GML references are not fetched.

## Selection and projection

Read every feature, reject duplicate GML feature IDs, and retain open-version localities/streets/points whose version starts no later than the member's snapshot day. Closed or future records are excluded conservatively. A locality needs a populated name and province-matching seven-digit TERYT municipality. Streets need their published full names and local locality references; street polygons and locality representative points never supply address coordinates.

Eligible point records have a positive leading civic number in the supported ASCII number/letter/space/slash/hyphen grammar, a five-digit `NN-NNN` source postcode, a nonfuture grant date when present, local references, and a finite two-dimensional EPSG:2180 address point in the broad projected-coordinate frame. Select the lowest 2,000 SHA256(seed NUL source address UUID) values per province. Join the selected points to locality and optional street through exact source GML references, checking the street's locality membership. Missing/conflicting references are omitted without backfill. Candidate IDs duplicated across provinces fail.

GML's declared EPSG:2180 axis order is **northing, easting**. Preserve the original position and pass **easting, northing** to DuckDB `ST_Point`, then `ST_Transform` to EPSG:4326 with `always_xy=true`. Retain the input position, declared CRS, projection settings and DuckDB version in provenance. The actual DuckDB 1.5.6 transformation was checked against the EPSG:2180 projection origin: easting 500,000 / northing −5,300,000 maps to longitude 19° / latitude 0°. The offline regression tests separately check GML axis interpretation and the SQL's easting/northing order. Projected points outside the broad Poland WGS84 frame fail preparation; no service/index answer calibrates or selects coordinates.

The adapter balances province and mutually exclusive classes: complete compound numbers, village suffix, village, street suffix, nonstandard street type, and plain. It selects up to six distinct municipality/road combinations per pool, using the village name when there is no road. Native queries preserve the complete source number and street/locality spelling. An ASCII variant uses the existing BSD-3-Clause deunicode dependency when it changes the query; this is an input variant, not an invented registered alias. A slash/hyphen compound is scored as the entire source number, never inferred as an apartment or split into convenient endpoints.

## Expectations and limitations

- The PRGAD export has **no building-occupancy status or per-point positional-quality field**. An address point does not prove a completed building, surveyed entrance or rooftop. Do not attribute every missing house to parsing; planned buildings, source/index coverage, ancestry and point differences need separate investigation.
- Locality names can denote villages, city parts or districts; the source name is not replaced by an invented postal town. Postcodes are reproduced from the official PRG export under its stated reuse terms.
- A 150 m tolerance checks the published address point, not a positional-accuracy guarantee. Forward search also requires Poland, house resolution, complete source number/postcode and goodMatch. Full-string suggestions check country/type/point among the first five. Reverse checks country only; ResolveRegion is unscored.
- Variants share the source address-point ID and location-based hold-out. These are **source-ID groups**, not verified independent physical buildings; multiple points/addresses may describe one building.
- The two-stage, province/pattern-balanced selection is diagnostic, not population-weighted national accuracy. PRG data can also feed OSM/OpenAddresses/index inputs, so agreement is not independent accuracy evidence. Keep held-out locations out of tuning.
- No matching, parsing, fallback, builder, runtime dependency, scoring or wire-contract changes are included.

## Validation

The national archive contains **68,332 locality, 305,768 street/square and 8,622,284 address-point features**. There are **8,619,713 eligible numbered/postcoded points** before selected-point reference joins; 2,571 point records are excluded by the source attribute criteria. One locality has a blank name and cannot supply a query. Of 32,000 hash-selected points, three have unresolved or conflicting references and are not replaced. The normalized capture has **31,997 candidates**, **36,930,942 bytes**, SHA256 **fa29a9cc662ffc77cae3f15d87d36f11e56a883c13d2729b2ce76b94a64685ff**. Preparation, including the projection, replays to the same hash.

The final corpus has **927 queries, 560 source address-point groups, and 130 held-out groups (220 queries)**, across all sixteen provinces. Seeds and corpus regenerate byte-for-byte. The standalone source run on the full local index (builder **a7481f0be4727233**, fingerprint **4cfc1e4547bd**) passes its absolute gate with no critical results or added baseline labels:

| Pattern class | Locations | Queries | Search passes | Full-string suggestion passes |
|---|---:|---:|---:|---:|
| Complete compound number | 86 | 143 | 141 | 135 |
| Other street type | 90 | 153 | 147 | 144 |
| Plain | 96 | 157 | 154 | 143 |
| Street suffix | 96 | 174 | 170 | 161 |
| Village | 96 | 143 | 140 | 135 |
| Village suffix | 96 | 157 | 141 | 134 |

SearchAddress passes **893/927**, with 20 too coarse, 13 near misses and one component mismatch. Native forms pass **538/560**; ASCII forms pass **355/367**. Full-string suggestions pass **852/927**, and country-only reverse checks pass **927/927**. ResolveRegion is unscored. For `Rynek 26, 59-100 Polkowice, Poland`, the service returns the house within about 1 m but reports postcode **59-101**, disagreeing with PRG's **59-100**. The source expectation is retained; the currently correct postal assignment has not been independently adjudicated. Other noncritical failures are not all root-caused, and human review remains pending.

The complete 16,483-case candidate passes its absolute gate and has zero shared answer/class drift against the 15,006-case baseline across all four RPCs, with no newly failing or worse-class cases. The standalone new-country answers/classes/suggestions agree with the complete run. Capture validation/output hardening changed only capture fingerprints/provenance: fresh source records/points, seeds, queries, expectations and hold-outs are unchanged, and rescoring the final corpus changes zero classes. See [Canada's validation](ca.md#validation) for the shared binary/index/model identities and the **failed, non-isolated local speed comparison**. Current-head CI quality and measured speed remain required. The fresh-service grpcurl SearchAddress hang remains a documented known failure despite the evaluator's passing message.
