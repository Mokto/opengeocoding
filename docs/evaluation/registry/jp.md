# Japan

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/jp.NOTICE) · [ABR residence supplement](#abr-residence-level-supplement)

**Japan** (`jp.seeds.jsonl`, `jp.jsonl`): 184 blocks in all 47 prefectures, 368 queries, drawn from MLIT's **街区レベル位置参照情報** (*block-level location reference information*), 2025年度版, version 24.0a. A point represents a block, not a house, residence or entrance. This corpus measures block-area resolution and does not establish house-level or national address accuracy.

## Source and licence

MLIT's [description](https://nlftp.mlit.go.jp/isj/index.html) says the data covers areas equivalent to urban-planning zones, with a representative point for `○○町△丁目□番`. Where residential addressing is not implemented, a point may instead represent a wider area containing several parcel numbers. This sample excludes that second kind. Place names come from municipal information, GSI maps and other maps; MLIT does not designate them as Japan's standard official names and warns that the data can lag current conditions.

The [Location Reference Information Terms](https://nlftp.mlit.go.jp/isj/agreement.html), revised 2013-02-07 and read 2026-10-31, permit free use, including edited outputs, reproduction and citation, with the required credit **街区レベル位置参照情報 国土交通省** and, where possible, the edition, filename and party responsible for edits. They prohibit relying on it for surveys requiring high accuracy or official certifications, disclaim liability and may change without notice. `jp.NOTICE` gives that attribution and the changes made. The corpus records the terms as `MLIT-ISJ-Terms`, not as an assumed Creative Commons licence.

### Address Base Registry residence-level check

The Digital Agency's [2024-01-15 specification](https://www.digital.go.jp/assets/contents/node/basic_page/field_ref_resources/3d996df3-f87c-4439-b474-c0fbbf3ddad8/31267348/20240115_policies_base_registry_format_01.xlsx), sheet **13住居表示-住居位置参照**, defines a separate residence-position extension: municipality/town/block/residence IDs, `rep_lon`, `rep_lat`, `rep_srid` (example EPSG:6668), map scale and source code. Thus coordinates are not confined to MLIT's block table in the published schema. The [official catalogue](https://dataset.address-br.digital.go.jp/) advertises the residence-position datasets (for example [Tokyo](https://www.arcgis.com/sharing/rest/content/items/e47e9d22d50f4b74bd9827b284292b53?f=json), updated 2025-10-27) under CC BY 4.0 and links the [Digital Agency terms](https://www.digital.go.jp/policies/base_registry_address_tos).

However, `data.address-br.digital.go.jp` returns a CloudFront 403 explicitly blocking downloads from this country, so the residence-position files' actual coordinate completeness was not inspected. The Digital Agency [also states](https://www.digital.go.jp/policies/base_registry_address) that residence/block/position data remains experimental; only town-name text is formally maintained, and the other tables are not guaranteed current. This corpus therefore uses the accessible MLIT block register instead of claiming residence coverage from a schema. The index separately reads OpenAddresses' cached ABR residence export; that is not used to generate the MLIT expectations. The separate residence-level supplement below uses that export and must not be conflated with the block corpus.

## Capture and preparation

The 47 archives are dated 2025年度版, version 24.0a. They were captured on **2026-10-31**, total **128,320,377 bytes**. [jp.sources.tsv](../../../tests/registry/jp.sources.tsv) pins each archive's official URL, size and SHA-256. The CSV entries are dated 2026-07-02, except Osaka (27) and Fukuoka (40), dated 2026-09-11. Their metadata calls them 2025年度版 and declares **JGD2000 / (B, L)**. The year is the source edition, not the download year; a versioned URL can still be replaced, so retain the pinned files.

```sh
REPO=$PWD
SOURCE=../jp-data
mkdir -p "$SOURCE/raw"
while IFS="$(printf '\t')" read -r pref url bytes hash; do
  [ "$pref" = prefecture ] && continue
  zip="$SOURCE/$pref-2025.zip"
  curl -fL "$url" -o "$zip"
  [ "$(wc -c < "$zip" | tr -d ' ')" = "$bytes" ] || exit 1
  printf '%s  %s\n' "$hash" "$zip" | shasum -a 256 -c - || exit 1
  unzip -p "$zip" "${pref}_2025.csv" | iconv -f CP932 -t UTF-8 > "$SOURCE/raw/${pref}_2025.csv"
done < tests/registry/jp.sources.tsv
(cd "$SOURCE" && duckdb -bail -f "$REPO/tests/registry/mlit-csv.sql")
target/release/evaluate sample-jp --mlit "$SOURCE/mlit-blocks-2025-4326.csv" --seed jp-2026-10 --out tests/registry/jp.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/jp.seeds.jsonl --out tests/registry/jp.jsonl --mlit "$SOURCE/mlit-blocks-2025-4326.csv"
target/release/evaluate run --corpus tests/registry/jp.jsonl --index out --out eval-results/jp
```

The 47 Shift-JIS CSVs decode as CP932, with the same 14-column header and **19,589,219 rows**. `mlit-csv.sql` requires all 47 files, filters the eligibility flags and numeric block signs below, converts the geographic coordinates from EPSG:4612 to EPSG:4326 using DuckDB spatial / PROJ with `always_xy=true`, and writes a deterministically ordered UTF-8 file. The prepared input, `mlit-blocks-2025-4326.csv`, has **1,127,815 rows**, **132,032,559 bytes**, SHA-256 `e60b0405584a426d3558d3dc05f073f546218162566948233604abda5b20e02f`. Preparation, sampling and corpus generation each repeated byte for byte.

### Coordinate limitations

The expectation comes only from the published geographic **緯度 / 経度** columns, transformed from their declared JGD2000 datum, not from the service, OpenAddresses, Overture or another geocoder. Points are supplied to six decimal places. A latitude outside 20–46° N or longitude outside 122–154° E fails the adapter rather than silently accepting swapped axes.

The auxiliary projected columns are **X = northing**, **Y = easting**, with a Japanese plane-rectangular zone number 1–19. Reprojecting that pair in the corresponding JGD2000 zone agrees with the geographic point within 1 m for 151 of the 184 sampled blocks, but disagrees by over 300 m for 33 (up to 202 km). For example, the geographic point of a Saga block is west of the zone origin while its Y is positive; the two columns cannot both describe that point as the format specifies. These inconsistencies are disclosed, not repaired by guessing a missing sign. The geographic columns are used consistently for every case, and no sampled block is dropped based on this cross-check. This is not survey calibration or a claim that the geographic columns are error-free.

The **300 m radius** is a diagnostic block-area tolerance, fixed before any service answer was seen. No block polygons or measured extent/error bounds are supplied, so it is not guaranteed to contain every block. A pass cannot validate a residence, an entrance, or every point of the block.

## Eligibility and sample

Only `住居表示フラグ=1` (residential addressing implemented), `代表フラグ=1` (the source's chosen representative) and `更新後履歴フラグ!=3` (not deleted) are eligible. The block sign must have one to four ASCII digits, without a leading zero. Prefecture, municipality and town names must be nonempty. Parcel-number areas, non-representative points, deleted rows and alphabetic/directional block signs (`南2`) are outside the frame. The representative flag is an administrative choice among points, not a promise of a geometric centroid.

The SQL frame has 1,127,815 records; one lacks a town name, leaving 1,127,814. Address keys are the SHA-256 of prefecture code, prefecture name, municipality, town, small-area name and block sign, NUL-separated. They form 1,127,540 keys; 180 with conflicting geographic points are excluded, leaving **1,127,360 unambiguous keys**. Identical-point repeats keep the lexicographically smallest source record, so input order does not choose provenance.

Within each prefecture, `sample-jp` ranks keys by seeded SHA-256 and takes the first block from each of four different municipality names where available. The source includes ward names in designated cities and district names for towns. Tottori and Tokushima have eligible blocks in two municipalities each, so each contributes two, and the other 45 prefectures contribute four: **184 locations**. Equal prefecture weighting is not population or address-count weighting. Coverage is limited to the source's urban-planning/residential-addressing frame, not all Japan.

Each block has two source-field query forms:

- `北海道札幌市東区北三十七条東二十二丁目5番, Japan`
- `日本北海道札幌市東区北三十七条東二十二丁目5番`

The prefecture, municipality, town and any small-area name retain the register's spelling and order; `番` identifies the numeric block sign. No residence number, postcode, romanization, street name or floor is invented. Both forms share a location and the shared location-hash hold-out: **31 locations, 62 queries**, held out among 184 locations and 368 queries. The draw was frozen before querying the service.

### What the expectation checks

Each query requires goodMatch true, the source prefecture (`JP-01` through `JP-47`) and a point within 300 m, at street level or finer. There is no `house_number` component because the source supplies a **block**, not a residence, and the RPC has no block-number field. Consequently a passing nearby house is not proof that its block identifier is correct, and the pass count below is only this geography/level/confidence test.

ResolveRegion and ReverseGeocode check the prefecture at the source point. Unlike a house case, ReverseGeocode does **not** verify the block, house, municipality name or distance to a residence, and no 25 m house-offset check is generated. These checks must not be presented as 368 correct block or house identifications.

**Index ancestry.** Overture 2026-09-23.1 holds 19,587,926 Japanese addresses, all from the 47 OpenAddresses prefectural `jp/...` sources, which read these MLIT archives. All 184 sampled geographic points occur in Overture at exactly the source position, matching the prefecture, municipality and town and the first part of the indexed number. The indexed numbers are not the bare source block signs: examples include `5-12` for source block 5 in coordinate zone 12. OpenAddresses also omits the small-area field used in one sampled address. None of these index representations supplies an expectation. The local index additionally reads OpenAddresses' ABR residence export (2026-10-02 job, joining the town, residence and residence-position tables) and the chart's Japanese OSM extract. Selection is independent of those imports, but agreement measures fidelity to the register, not independent real-world accuracy.

## Initial local baseline (#475)

On the local full index built by `dbec0f0cae254966` (Overture 2026-09-23.1, OpenAddresses including ABR residences and the chart's OSM extracts), the baseline is:

| Query form | Pass | Too coarse | Confidence mismatch | Abstain | Near miss | Far miss |
|---|---:|---:|---:|---:|---:|---:|
| Country suffix (`…, Japan`) | 0 | 174 | 0 | 10 | 0 | 0 |
| Country prefix (`日本…`) | 11 | 79 | 46 | 5 | 42 | 1 |
| Total | 11 | 253 | 46 | 15 | 42 | 1 |

Held-out queries pass 2/62. None of the 184 locations passes in **both** forms. SuggestAddress finds no acceptable full-string result (0/368). ResolveRegion and the limited ReverseGeocode prefecture checks pass 368/368. A second run of the same binary has zero answer/scoring drift across all four RPCs. Local timing variations are not speed evidence; CI's gold-speed is the gate.

The initial critical result was inspected and recorded in `jp.labels.jsonl` when the corpus was added, without removing it or giving it a different expectation:

- **Parser/matching:** `日本大分県中津市沖代町一丁目5番` returns **Tsu, Mie (JP-24)**, 505 km from the MLIT block in Nakatsu, Oita (JP-44), goodMatch false. Native libpostal reads country `日本`, state `大分県中`, city `津市`, suburb `沖代町一丁目`, house number `5番`: it splits 中津市 into the wrong city and corrupts the prefecture. ReverseGeocode and ResolveRegion locate the source point in Nakatsu/Oita, and a training-only index inspection finds the expected road and block-5 residence numbers nearby. Its country-suffix control returns Okidaimachi 1-chome in Nakatsu within 139 m, but at locality level, so that control remains too coarse. That initial label preserved the `far_miss` class and all coordinates, tolerance, level and confidence requirements; re-scoring the saved run after adding it changes **zero classes**.

Other failures remain unlabelled: 253 locality fallbacks (45 m–14 km from the point), 46 nearby street results with goodMatch false, 42 wrong nearby streets/houses or places (307 m–53 km), and 15 abstentions. Examples inspected only in the non-held-out set include 函館市青柳町10番, which finds the street within 211 m but lacks confidence, and 仙台市若林区沖野二丁目1番, which answers another Sendai street. The source's block points are present in the index; these results expose query parsing/matching/representation limitations, not a corpus expectation derived from service output. There is no runtime fix, tolerance increase or removal of failing cases in this change.

These are local diagnostic results, not national accuracy or house coverage. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure. Human review of source points is pending.

## Native administrative-boundary correction

The subsequent parser correction keeps the written prefecture, city and ward for complete chōme/block/house forms whose country is recognized as Japan and whose parse already contains a house number. It does not change the parsed road or number, override a conflicting country, or extend that alignment to block-only queries. Municipality splitting keeps 市川市 and 四日市市 intact. Independently, a result contradicting an indexed prefecture explicitly named by a Japanese query yields to a prefecture-compatible weak locality fallback, or abstains if none is available.

On a fresh full index with builder `dbec0f0cae254966`, base and candidate use the same directory, sources and local models; a repeated base run has zero response drift. Across the 43,244-case gold suite, five SearchAddress results change, only in Japan: four improved failure classes and one finer unresolved street fallback. Passing counts do not change. No passing case is lost, no failure class worsens, and no other RPC changes. The absolute runs still fail inherited non-Japanese must-pass/critical cases; this is a clean relative accuracy comparison, not a claim that the full suite passes absolutely. Local timings are not speed evidence.

The former 505 km Nakatsu→Tsu result becomes **Okidaimachi 1-chome, Nakatsu, Oita (JP-44), 139 m from the source point, goodMatch false**. It remains `too_coarse` because it does not resolve the block. Its obsolete wrong-city failure note was removed and the corpus regenerated from the same pinned MLIT input: this removes only `known_failure`, without changing any expectation or tolerance. Re-scoring both saved runs changes zero classes. The other block changes are the two Hatsukaichi forms (abstain→too_coarse) and the Yokohama form (near_miss in Ageo→too_coarse in Yokohama).

The block corpus remains weak: **11/368 pass**, 257 too coarse, 46 confidence mismatches, 13 abstentions, 41 near misses and no far miss on this index. SuggestAddress remains 0/368. A general block-aware lookup is still needed. Broadly aligning block-only forms was rejected before publication: the index's existing ambiguity between chōme/block and block/house readings promoted wrong nearby houses. Those results are not retained or counted as improvements. Synthetic cases retain the exact-house and missing-house checks, plus the wrong-prefecture block fallback that failed before the guard.

## ABR residence-level supplement

`jp.abr.seeds.jsonl` and `jp.abr.jsonl` are a separate **residence-level** panel: 256 locations in 210 municipality/ward codes across all 47 prefectures, 512 queries. They do not replace, refine or reuse the MLIT block expectations above.

### Source, access and licence

The source is the Digital Agency's **住居表示-住居マスター** joined to **住居表示-住居マスター位置参照拡張**, with town/city names and available postcodes from the town and city masters. The official CDN explicitly returns a country-blocking CloudFront 403 from Denmark. Instead, this panel uses the public [OpenAddresses export uploaded 2026-10-01](https://data.openaddresses.io/cache/uploads/jeffdefacto/2026-10-01-tkes5/jp_abr_residential.csv.zip), not a geocoder's responses. Its [preparation script at commit 5172acb847e0127cab772a75f92b43444cac5db0](https://github.com/openaddresses/openaddresses/blob/5172acb847e0127cab772a75f92b43444cac5db0/scripts/jp/abr_residential.py) joins the source IDs and copies `rep_lat`/`rep_lon`; it does not call a geocoder. The export contains **21,984,066 rows**.

The [official Tokyo residence-position catalogue metadata](https://www.arcgis.com/sharing/rest/content/items/e47e9d22d50f4b74bd9827b284292b53?f=json) explicitly names CC BY 4.0. The [ABR-specific terms](https://www.digital.go.jp/policies/base_registry_address_tos) apply [PDL1.0](https://www.digital.go.jp/resources/open_data/public_data_license_v1.0), whose section 1.7 explicitly permits use under CC BY 4.0, including commercial reuse and redistribution. The required Digital Agency attribution, OpenAddresses join and OpenGeocoding edits are recorded in `jp.NOTICE` and each case. This panel uses residential-address tables, not the parcel masters subject to additional Ministry of Justice terms. This is a reading of the published terms, not legal advice.

**Limitations of the export:** it selects the smallest longitude/latitude pair where a residence has multiple position records, keeps one town record per key and drops per-record `rep_srid`, map scale and coordinate-source codes. The mirror therefore cannot establish that all original position records agree, or support source-method/precision stratification. The original raw tables were not downloaded locally. Residence positions remain experimental and are not guaranteed current; the upload date is not an asserted date for every source record. This source is sufficient for a disclosed registry-consistency diagnostic, not a surveyed entrance benchmark.

### Pinned capture and reproduction

```sh
SOURCE=../jp-data
mkdir -p "$SOURCE"
curl -fL -o "$SOURCE/jp_abr_residential.csv.zip" 'https://data.openaddresses.io/cache/uploads/jeffdefacto/2026-10-01-tkes5/jp_abr_residential.csv.zip'
printf '%s  %s\n' 5f9a914bfbf53d5fc4a9ddd10ea217b4872cb56c36456489deffa9d78984d655 "$SOURCE/jp_abr_residential.csv.zip" | shasum -a 256 -c -
unzip -p "$SOURCE/jp_abr_residential.csv.zip" jp_abr_residential.csv > "$SOURCE/jp_abr_residential.csv"
printf '%s  %s\n' 9ae62879562f5990ba483a0b9e925af6fa10bff7f7c4b1a7a205f6dfdef56fc3 "$SOURCE/jp_abr_residential.csv" | shasum -a 256 -c -
target/release/evaluate sample-jp-abr --abr "$SOURCE/jp_abr_residential.csv" --seed jp-abr-2026-11 --out tests/registry/jp.abr.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/jp.abr.seeds.jsonl --out tests/registry/jp.abr.jsonl --abr "$SOURCE/jp_abr_residential.csv"
nice -n 15 target/release/evaluate run --corpus tests/registry/jp.abr.jsonl --index "$HOME/opengeocoding-index/out" --out eval-results/jp-abr
```

Captured 2026-11-01: ZIP **349,857,474 bytes**, uncompressed CSV **2,111,961,652 bytes**. Retain both files. Each generated case records the CSV checksum, release filename, original nine fields, export URL, pinned join script and licence/attribution. Sampling and `build-gold` each repeated byte for byte. No expectations are hand-edited.

### Eligibility, sampling and expectations

The adapter accepts only the export's exact nine-column schema. An eligible residence has a numeric source ID whose first two digits identify prefecture 01–47, nonempty prefecture/city/town names, a complete two- or three-part number with one to four ASCII digits per part and no zero/leading-zero parts, coordinates, and an empty or seven-digit postcode. A bare block number, nonnumeric or incomplete number is not a residence candidate. Malformed IDs, column counts, nonfinite or out-of-Japan coordinates fail the input. The coordinate bounds, 20–46° N and 122–154° E, also reject swapped axes. **21,979,778 rows** meet the frame; 4,288 do not.

Per prefecture and number class, `sample-jp-abr` keeps the smallest 200 seeded SHA-256 hashes of source IDs, then takes up to four different municipality/ward codes for two-part numbers and two for three-part numbers. Diversity is limited to those 200 candidates, not an exhaustive search of every municipality. The result has **177 two-part and 79 three-part locations**. Equal prefecture/class targets and oversampling secondary numbers are diagnostic choices, not population or address-count weights. Sampling does not inspect the index or service answers.

Before writing seeds, a second lookup rejects repeated selected IDs and a full-source scan rejects any other eligible ID with exactly the same complete address text. `build-gold` repeats those checks and rejects missing/ineligible IDs. This detects ambiguity present in the export, not records the upstream exporter already discarded.

Both queries preserve source spelling, full-width digits, small-area names when present and every number part:

- `北海道釧路市貝塚３丁目7-22-54, Japan`
- `日本北海道釧路市貝塚３丁目7-22-54`

No residence part is silently reinterpreted as a unit or dropped. Both forms share a source ID, location and held-out assignment: **55 locations / 110 queries** held out. All sampled small-area fields are empty; this panel does not demonstrate small-area coverage. It adds no romanized, floor or damaged-input variants.

Each expects house level, goodMatch true, the complete source number, the source prefecture (`JP-01`–`JP-47`, from the municipality-code prefix) and a point within **150 m**. When the source supplies a postcode (61 locations), it is checked with the usual Japanese hyphen punctuation. Source JGD2011 geographic coordinates (EPSG:6668, as the OpenAddresses source declares) are used as WGS84 without transformation and rounded to six decimals. That approximation is for this 150 m diagnostic tolerance, not survey accuracy; the export lacks the original per-record CRS and error estimates. The tolerance was fixed before querying the service and is not a certified bound.

Road names remain in provenance but are not scored without an alias crosswalk, so a nearby different road with the same house number can satisfy the component checks. There is no bilingual municipality-name crosswalk: ReverseGeocode checks the source number, available postcode, radius and prefecture, not the municipality's identity. ResolveRegion checks the prefecture. The 25 m offset check only asks for a nearby house, not the same residence.

**Index ancestry:** the local index already reads this same OpenAddresses ABR export as a supplement. No source was imported or changed for this panel. Selection is independent of index coverage, but agreement measures fidelity to a shared source, not independent real-world accuracy. It is reported separately from the MLIT block panel.

### Initial local baseline

On the rebuilt local full index from builder `dbec0f0cae254966`, with `main`'s service at `0d14abc` and the new evaluator:

| Number class | Locations | Queries | SearchAddress passes |
|---|---:|---:|---:|
| Two-part | 177 | 354 | 235 |
| Three-part | 79 | 158 | 3 |

Overall **238/512** pass, 57/110 held-out queries. Other classes: 192 too coarse, 56 wrong component, 13 abstain, 12 near miss and one confidence mismatch. There is no critical result and no new known-failure label. Both forms pass for 110/256 locations; none of the three-part locations passes in both forms. These are registry-consistency observations on a deliberately biased panel, not national accuracy estimates.

Full-string SuggestAddress passes **0/512**. ResolveRegion passes **512/512**. At the source point, ReverseGeocode passes **498/512** (all 14 failures have the wrong house number); the 25 m house-offset check passes **512/512**. A same-binary repeat has zero answer/scoring drift. The documented grpcurl SearchAddress hang reproduces as a known failure. Human review of source points remains pending.

The three-part queries expose truncated numbers: for example, `日本青森県十和田市東四番町5-6-3` returns nearby house `5-6`, while reverse at the source point finds `5-6-3`. Two-part failures include source houses present in raw candidates but returned only as locality fallbacks. These are diagnostic examples, not repairs or grounds to drop cases, relax numbers or increase tolerances. No parsing, matching, index, scoring or existing-corpus expectation was changed.

The full comparison retains **43,244 shared cases with zero changed answers or classes across all RPCs**, and reports the 512 added cases separately. Both absolute full runs still fail inherited untriaged critical cases outside this panel; the relative diff passes. No existing label or expectation was changed to silence them. Local runs used no timed speed sample and provide no speed sign-off. The report and nightly issue now disclose that their existing "Independent sources" grouping means selection outside the index, not independence of upstream registry inputs; counting and scoring are unchanged.

### Complete residence-number lookup

The subsequent runtime correction preserves an explicit chōme as part of the indexed road and every residence-number part. It reads digits inside road names without mistaking them for house numbers and shares the complete-address lookup between search and autocomplete, without calling libpostal for autocomplete. A confident direct answer needs an exact indexed road and number, an indexed prefecture, and the complete written municipality hierarchy in that prefecture. Comma-separated city/postcode qualifiers must corroborate the same municipality hierarchy. A stated postcode must match the house's stored postcode for the shortcut; other cases retain the generic postcode-area guards. Conflicting countries are not overridden. Distant copies remain ambiguous; co-located copies of the same road/number within roughly 100 m collapse. This does not distinguish nearby buildings sharing an address; building-level disambiguation needs source identities in the index.

Block-only forms retain their existing chōme/block representations and do not enter the confident residence shortcut. Neither the corpora, tolerances, known-failure labels, scoring nor index inputs change.

Against the `ecd8645` baseline on the same full index (`dbec0f0cae254966`, data digest `c831e5894669`) and models:

| Check | Before | After |
|---|---:|---:|
| SearchAddress, two-part | 235/354 | 296/354 |
| SearchAddress, three-part | 3/158 | 132/158 |
| SearchAddress, total | 238/512 | 428/512 |
| Full-string SuggestAddress | 0/512 | 416/512 |
| Held-out SearchAddress | 57/110 | 94/110 |
| Held-out SuggestAddress | 0/110 | 94/110 |

After this correction, the remaining residence search failures were 74 too coarse, eight abstentions and two near misses. Many ward-qualified addresses have their streets stored under the city, not the ward; the direct lookup does not discard that qualifier to claim a match. This change does not establish general half-typed native-script completion, romanized address coverage or building/entrance accuracy. The weak block panel remains **12/368** for search and **0/368** for full-string suggestions; a block-aware lookup is still needed.

A 1,005-case Japan comparison includes the two registries, the OSM panel and the other Japanese gold checks. Repeated baseline and candidate runs have zero answer/scoring drift. The full **44,047-case** comparison gains 191 SearchAddress passes and 416 SuggestAddress passes, all in Japan, with no newly failing check or worse failure class. ResolveRegion and both reverse checks are unchanged. Absolute runs still fail inherited critical results; the relative comparison passes. Expectations were not weakened. Development used the non-held-out residence panel; held-out counts are validation, not tuning targets. These are shared-source consistency checks, not national accuracy estimates. Local runs use `--repeat 1 --speed-cases 0`; only CI can provide the measured speed gate.

### Ward-qualified residence lookup

The next correction uses the already-stored source municipality/ward attached to a house number or its own postcode when the geographic hierarchy files the street under the parent city. It requires the full written municipality and ward in the requested prefecture, an exact road and complete residence number, and an unambiguous source municipality/ward match. A different indexed district, a second distant copy of the number, conflicting source municipalities or an unverified ward prevents this shortcut. A source association keyed only by postcode cannot disambiguate two wards sharing that postcode in the same street document. Houses already found within the indexed ward take precedence over this source-backed fallback.

Municipality lookup tries complete suffixes instead of splitting a proper ward name at every administrative-looking character: `中村区` must remain intact even though it contains `村`. The entire written hierarchy still has to match; no municipality or ward is silently discarded. The original query is checked after resolver rewrites as well, so an unsupported ward cannot become a confident city-only house answer. A verified ward can instead be returned as a structured fallback with goodMatch false. The source-backed house's stored geographic hierarchy may still display the parent city; this change does not rewrite the index geography or establish ward-level reverse coverage.

Compared with `bcdf9d9` on the same full index (unchanged builder `495ed3bf27ad1ccd`, data digest `696ca51d63f3`) and models:

| Residence-panel check | Before | After |
|---|---:|---:|
| SearchAddress | 428/512 | 506/512 |
| Full-string SuggestAddress | 416/512 | 502/512 |
| Held-out SearchAddress | 94/110 | 108/110 |
| Held-out SuggestAddress | 94/110 | 108/110 |

The non-held-out panel improves from 334 to 398/402 search passes and 322 to 394/402 suggestion passes; held-out cases were reserved for validation. Remaining search failures are four abstentions and two too-coarse responses. The full **44,047-case** comparison gains 78 search passes and 86 suggestion passes, all within this residence panel, with no newly failing check or worse class. The three other changed search answers remain passing at the same house coordinates and number: two display Sapporo rather than Teine Ward as the city, and one displays Niigata rather than Nishi Ward and normalizes postcode punctuation. ResolveRegion, both reverse checks, the block panel and all other countries are unchanged. Full baseline/head repeats have zero answer/scoring drift. Absolute full runs retain inherited critical failures; the relative diff passes.

No corpus, expectation, tolerance, failure label, scoring, source input or builder hash changes. Authored synthetic checks cover number- and postcode-specific source associations, conflicting wards/countries/prefectures, missing/ambiguous source associations, distant duplicate numbers, proper ward names containing `村`, preservation of an existing geographic-ward match, and the resolver rewrite guard. These remain shared-source consistency measurements, not national accuracy or entrance-level accuracy. Local runs have no timed speed sample; measured speed remains a CI check.
