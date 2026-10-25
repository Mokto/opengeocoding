# Serbia

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/rs.NOTICE)

**Serbia** (`rs.seeds.jsonl`, `rs.jsonl`): 148 locations in 143 settlements of 106 of the 168 municipalities, 296 queries, drawn from the house-number list of the **Address Register** (Адресни регистар) kept by the Republic Geodetic Authority (Републички геодетски завод, RGZ): street and house number in Cyrillic and Latin script, settlement, municipality and point for every house number in Serbia, Kosovo excluded. It is published on the [Open Data Portal](https://data.gov.rs/sr/datasets/adresni-registar/) as CSV and GeoPackage, updated weekly.

## Licence

The portal publishes the register under its [data licence](https://data.gov.rs/sr/terms/) (Лиценца за коришћење података, registered in the EU vocabulary as SODL 1.0; the corpus records `SODL-1.0`): any natural or legal person may copy, distribute, make available to third parties, adapt and combine the data, commercially or not, without charge, without limit of time or territory. On each reuse the user must name the source, including the public body that made the data available, state the download date and the address the data can be downloaded from, and mark clearly any changes. `rs.NOTICE` does that. This is a reading of the published terms, not a grant from the RGZ.

## Capture and preparation

The download address the portal links, `download.geosrbija.rs`, answers only Serbian networks: from abroad the TCP connection opens and the TLS handshake is never answered (OpenAddresses records the same block in [issue 7199](https://github.com/openaddresses/openaddresses/issues/7199)). This corpus therefore uses the copy of the CSV export an OpenAddresses contributor downloaded on 2025-06-10 and posted in that issue; it is not a checksum-verified official release. `kucni_br_csv.zip` is 132,385,796 bytes, SHA-256 `6170abc37eb96226196ee8eb74b57d50b54130eaaf3a5069fe0c877c97f033f6`; its `kucni_broj.csv` (dated 2025-06-10 in the ZIP, latest `modificationdate` 2025-06-07) is 767,021,018 bytes, SHA-256 `4b11fb3bccf845e61275fd1c7a30c8645363bd0e1118294ac4651707f2a3e9ac`. It has 2,464,753 house numbers, all with status `АКТИВАН` and none retired, in 168 municipalities and 4,716 settlements; 2,242,724 are typed `УТВРЂЕНИ КБр` (established) and 222,029 carry no type. From a Serbian network the [CSV resource](https://download.geosrbija.rs/download-api/opendata-proxy/export?category=ar&layer=kucni_broj_ar&geometry=true&fileName=kucni_br_csv&format=csv) gives the current week's export in the same format; the adapter reads either.

Prepare the CSV with DuckDB spatial (1.5.6 for this sample), an offline curation tool, not a service dependency:

```sh
REPO="$PWD"
mkdir -p ../rgz-data
(
  cd ../rgz-data
  curl -fL -o kucni_br_csv.zip https://bogdanupload.blob.core.windows.net/openaddresses/kucni_br_csv.zip
  unzip kucni_br_csv.zip   # kucni_broj.csv
  duckdb -f "$REPO/tests/registry/rgz-csv.sql"
)
target/release/evaluate sample-rs --rgz ../rgz-data/rgz-addresses-4326-2025-06-10.csv --seed rs-2026-10 --out tests/registry/rs.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/rs.seeds.jsonl --out tests/registry/rs.jsonl --rgz ../rgz-data/rgz-addresses-4326-2025-06-10.csv
target/release/evaluate run --corpus tests/registry/rs.jsonl --index out --out eval-results/rs
```

The points are WKT in ETRS89 / UTM zone 34N (EPSG:25834); the SQL transforms them to EPSG:4326 with PROJ, and all of them land within 42.24–46.18° N, 18.86–22.96° E. The register does not say what a point marks (building, entrance or parcel), so no precision beyond the 150 m tolerance is claimed. The SQL also writes the place each query names and counts, over the whole register, the addresses sharing that place, Latin street and Latin number (`copies`). The prepared CSV is 487,394,460 bytes, SHA-256 `08bf75f4ff99caebb269d1320a27eacd6812c027519df55a9e1795d57dba6b1c`. Preparation, sampling and corpus generation each repeated byte for byte.

## Sample

`sample-rs` reads 2,449,504 eligible rows: a house number of one to four digits, no leading zero, with at most one letter written as one Latin capital A–Z and one Cyrillic capital (the digraph letters "158Њ"/"158NJ" and letters Č, Ć, Š, Ž, Đ in Latin are excluded), street names with two letters or more and no comma, and a query that fits one address in the register (`copies` = 1). Each row is in one class: **village**, a house numbered on a "street" named after its own settlement, the register's form for villages without streets; else **letter**, a number with a letter; else **plain**. In each class it keeps, for each municipality, the row with the smallest seeded SHA-256 of its register ID (`kucni_broj_id`), and of those the 60 smallest: 60 plain, 60 letter and 28 village locations (only 28 municipalities number village houses that way), 19 of them in the City of Belgrade and 21 without the "established" type. The sample weights municipalities and classes, not population or address counts; it is not a representative national accuracy sample. Of the 148 locations, 27 are held out by the shared location-hash rule; both queries of a location share its ID and hold-out assignment.

Each location has two queries, both from the register's own fields in its capitals, and both require goodMatch true:

- **cyrillic**: `ЦАРА ЛАЗАРА 61, СРЕМСКА КАМЕНИЦА, Србија`
- **latin**: `CARA LAZARA 61, SREMSKA KAMENICA, Serbia`

The place is the settlement; a village-class query names the municipality instead (`ЛОНЧАНИК 83, УБ`), since the settlement is already the street. Places are written without the register's parenthesised qualifier: "БЕОГРАД (ЗЕМУН)" and "НИШ (ПАЛИЛУЛА)" become БЕОГРАД and НИШ, as addresses are written, and "ДОЛАЦ (НАСЕЉЕ)" becomes ДОЛАЦ; the `copies` rule keeps only addresses that are still the only one of their street and number in a place of that name anywhere in Serbia. No transliteration is made: the Latin form is the register's own. Every case expects house level within 150 m of the register point; a lettered number is accepted in either script ("27А" or "27A") through `any_of`, both from the same row. The register has no postcode, so none is queried or scored. Region expectations are empty, so ResolveRegion is not checked; ReverseGeocode is asked about every point.

**Index ancestry.** All 2,681,230 Serbian addresses in Overture release 2026-09-23.1 come from OpenAddresses' `rs/countrywide`, a copy of this register posted on 2025-07-31, in Latin script only. The chart also reads Geofabrik's Serbia OpenStreetMap extract, where the community imported the register: 3,581,701 of the 3,618,953 objects with a house number in the extract of 2026-10-20 carry its ID (`ref:RS:kucni_broj`), with Cyrillic street names and house-number letters in Latin ("10a"). Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

The local world index from builder `17bc2fab0087eddb` (Overture 2026-09-23.1, OpenAddresses and OSM) lacks the Serbia extract the chart reads, so it holds Serbian houses only under Latin street names: SearchAddress passes 143/296, all 148 Latin forms but 5 and no Cyrillic form. The table is a variant of that index built with the same builder and the Serbia extract of 2026-10-20 added (240,994,225 bytes, SHA-256 `b0ee047694202cc0bb6cbcdeaa3084276d16b446e129da54bc66c2deedbf4ed3`), as the chart builds it:

| Class | Form | Queries | SearchAddress passes |
|---|---|---:|---:|
| Plain | cyrillic | 60 | 56 |
| Plain | latin | 60 | 55 |
| Letter | cyrillic | 60 | 0 |
| Letter | latin | 60 | 58 |
| Village | cyrillic | 28 | 22 |
| Village | latin | 28 | 28 |

Overall 219/296 pass, 38 of the 54 held-out queries among them; 69 are too coarse, 6 near misses and 2 far misses. SuggestAddress finds an acceptable full-string result for 231/296, none of the Cyrillic lettered numbers. All 296 ReverseGeocode country checks pass. A same-binary repeat has zero response drift. The failures:

- **Cyrillic house letter (63, matching):** every Cyrillic query with a lettered number ("КАПЕТАНА ДЕРОКА 10А") answers the street. Both sources key the house with a Latin letter ("10a"), and the house lookup does not try the Cyrillic letter's Latin form; the Latin query finds the house.
- **Raw house not accepted (5, matching):** the raw top candidate is the register's house, but the resolver rejects it: for Belgrade houses the index places in Zemun or Čukarica with the stated city БЕОГРАД (VIKTORA STARČIĆA 14A, both forms of RADOJA PRVULOVIĆA 67), for MILENKA PEVCA 93V in Stari Banovci, and for ЖЕЛЕЗНИЧКА 60, ДОЛАЦ, whose locality is "Dolac (naselje)": the answer is the namesake Dolac near Kraljevo, 137 km away (labelled `known_failure`; the Latin form passes).
- **Parser (2):** libpostal splits the settlements VELIKO ORAŠJE and BAČKI JARAK into a road and a city ("veliko", "orašje"), so SKADARSKA 6 answers the village Orašje near Vlasotince, 178 km away (labelled `known_failure`; the Cyrillic form passes), and IVE LOLE RIBARA 31 Jarak near Sremska Mitrovica, 51 km away.
- **Same number in another settlement (4, matching or index):** GENERALA PAVLA ILIĆA 8, NEGOTIN and KRATKA 11, BAČKA TOPOLA answer, in both scripts, the same street and number in a village of that municipality (Aleksandrovac, Bajša), 16 km and 7 km away, goodMatch true.
- **Cyrillic village name (3, index data):** БЕРЛОВИНЕ 2, ОЈКОВИЦА 256 and БОЖИЦА 388 find no house under the Cyrillic name; their Latin forms pass.

On the index without the Serbia extract there is one more critical result, ЗАСЕЛАК ВЕРДИНЂА 1, ТЕКИЈА answered with Tekija near Paraćin, 119 km away; it passes with the extract and is not labelled. Only the two critical results that remain with the extract carry `known_failure` labels; the others stay visible and unlabelled. These are diagnostic results on local indexes, not a CI gold run. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure. Human review of the source points is pending.

Each case's `places`, the names ReverseGeocode's answer must include one of, are the place the query names (in Latin and Cyrillic), the settlement and the municipality.
