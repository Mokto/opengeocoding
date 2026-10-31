# Japan

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/jp.NOTICE)

**Japan** (`jp.seeds.jsonl`, `jp.jsonl`): 184 blocks in all 47 prefectures, 368 queries, drawn from MLIT's **街区レベル位置参照情報** (*block-level location reference information*), 2025年度版, version 24.0a. A point represents a block, not a house, residence or entrance. This corpus measures block-area resolution and does not establish house-level or national address accuracy.

## Source and licence

MLIT's [description](https://nlftp.mlit.go.jp/isj/index.html) says the data covers areas equivalent to urban-planning zones, with a representative point for `○○町△丁目□番`. Where residential addressing is not implemented, a point may instead represent a wider area containing several parcel numbers. This sample excludes that second kind. Place names come from municipal information, GSI maps and other maps; MLIT does not designate them as Japan's standard official names and warns that the data can lag current conditions.

The [Location Reference Information Terms](https://nlftp.mlit.go.jp/isj/agreement.html), revised 2013-02-07 and read 2026-10-31, permit free use, including edited outputs, reproduction and citation, with the required credit **街区レベル位置参照情報 国土交通省** and, where possible, the edition, filename and party responsible for edits. They prohibit relying on it for surveys requiring high accuracy or official certifications, disclaim liability and may change without notice. `jp.NOTICE` gives that attribution and the changes made. The corpus records the terms as `MLIT-ISJ-Terms`, not as an assumed Creative Commons licence.

### Address Base Registry residence-level check

The Digital Agency's [2024-01-15 specification](https://www.digital.go.jp/assets/contents/node/basic_page/field_ref_resources/3d996df3-f87c-4439-b474-c0fbbf3ddad8/31267348/20240115_policies_base_registry_format_01.xlsx), sheet **13住居表示-住居位置参照**, defines a separate residence-position extension: municipality/town/block/residence IDs, `rep_lon`, `rep_lat`, `rep_srid` (example EPSG:6668), map scale and source code. Thus coordinates are not confined to MLIT's block table in the published schema. The [official catalogue](https://dataset.address-br.digital.go.jp/) advertises the residence-position datasets (for example [Tokyo](https://www.arcgis.com/sharing/rest/content/items/e47e9d22d50f4b74bd9827b284292b53?f=json), updated 2025-10-27) under CC BY 4.0 and links the [Digital Agency terms](https://www.digital.go.jp/policies/base_registry_address_tos).

However, `data.address-br.digital.go.jp` returns a CloudFront 403 explicitly blocking downloads from this country, so the residence-position files' actual coordinate completeness was not inspected. The Digital Agency [also states](https://www.digital.go.jp/policies/base_registry_address) that residence/block/position data remains experimental; only town-name text is formally maintained, and the other tables are not guaranteed current. This corpus therefore uses the accessible MLIT block register instead of claiming residence coverage from a schema. The index separately reads OpenAddresses' cached ABR residence export; that is not used to generate these expectations.

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
