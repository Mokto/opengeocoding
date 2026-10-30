# Latvia

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/lv.NOTICE)

**Latvia** (`lv.seeds.jsonl`, `lv.jsonl`): 122 locations in all 42 municipalities (35 novadi and the 7 state cities), 244 queries, drawn from the **State Address Register** (Valsts adrešu reģistrs) of the State Land Service (Valsts zemes dienests, VZD), as published in its [open data](https://data.gov.lv/dati/lv/dataset/varis-atvertie-dati) (Valsts adrešu reģistra atvērtie dati) on the Latvian Open Data Portal: one row per address of a building or of a land unit intended for building, with its parent object, number or name, postcode, full address and point (`AW_EKA.CSV`), and the streets, villages (ciemi and mazciemi), parishes (pagasti), towns (pilsētas) and municipalities (novadi) the parent codes name. VZD refreshes the files daily.

## Licence

The dataset page gives the licence as CC-BY-4.0, and each resource's metadata (`aw_eka_metadata.json`) as `https://creativecommons.org/licenses/by/4.0/`. VZD's data use terms ([Par datu izmantošanas noteikumiem](https://www.vzd.gov.lv/lv/par-datu-izmantosanas-noteikumiem), published 2021-12-08, read on 2026-10-30) say that open data carry no reuse restrictions beyond Creative Commons Attribution 4.0: they may be copied, changed and distributed, commercially or not, without VZD's permission, provided a product cites the source (for example "Izmantoti Valsts adrešu reģistra informācijas sistēmas dati"), links the licence and states the changes made, and does not suggest that VZD approved it. `lv.NOTICE` gives all three. OpenAddresses' `lv/countrywide` records the same licence and attribution ("Valsts zemes dienests") for the register's spatial files. The corpus records the licence as `CC-BY-4.0`.

## Capture

The files were downloaded on 2026-10-30 (04:26 UTC) from `https://data.gov.lv/dati/dataset/6b06a7e8-dedf-4705-a47b-2a7c51177473/resource/RESOURCE/download/NAME`; each address serves the latest daily refresh, so keep the pinned files:

| File | Resource | Last-Modified | Bytes | SHA-256 |
|---|---|---|---:|---|
| `aw_eka.csv` | `a510737a-18ce-400f-ad4b-04fce5228272` | 2026-10-29 04:39:49 | 141,763,271 | `9200b666f2c8bb4a2a4a06948ae95e54bf65636e1f9a8389ddc23da8340ea3ef` |
| `aw_iela.csv` | `3c4ab802-76cf-433c-9c1c-89215e28d833` | 2026-10-29 04:39:55 | 3,334,932 | `2414b22560dfdb15ea26383720b411b92680ade596f9dd11d487c664589e1f99` |
| `aw_ciems.csv` | `0d3810f4-1ac0-4fba-8b10-0188084a361b` | 2026-10-29 04:39:25 | 1,676,792 | `a6c6bd22b6b377edd068afe3c03e5a066689682f8037d2da12f59745a613323c` |
| `aw_pagasts.csv` | `6ba8c905-27a1-443a-b9c6-256a0777425b` | 2026-10-29 04:39:59 | 84,227 | `b39d3c7c24fd8060635bd4f6d1c8b54499ff31f291f2f91336fe0ca268354201` |
| `aw_pilseta.csv` | `ee02baa4-2bc3-4f77-a6cb-5427a3e9befe` | 2026-10-29 04:40:01 | 11,788 | `ceee08e463f2aaeb23d2bb1b1d1e9b53c6b155dcc9703fed463d0988ec0be34e` |
| `aw_novads.csv` | `c62c60bb-58d4-4f26-82c0-5b630769f9d1` | 2026-10-07 07:24:01 | 22,019 | `3bb10fdb6db1f71d29bbe7cfbd33eb318c1852a2a9e364ab29c37fc765b41847` |
| `aw_eka_metadata.json` (structure) | `27ab50ae-95ae-46be-88c8-d54d4b4c70b5` | | 8,111 | `33e24a00fceda477fd05f556477e86dd826453dbf9e3b59df143cdace22da1ae` |

The CSVs are UTF-8 with a byte-order mark (the metadata says ISO-8859-1; the bytes are UTF-8), separated by commas, every field quoted. `tests/registry/varis-csv.sql` keeps the addresses that exist (`STATUSS` EKS), are approved (`APSTIPR` Y) and are not planned (`PLAN_ADR` N), 545,885 of the 611,219 rows, and joins each to its street and to the existing town, village, parish and municipality above it; every one of them has its whole chain. It writes one CSV named after the data date:

```sh
mkdir -p ../lv-data
(
  REPO=$PWD
  cd ../lv-data
  B=https://data.gov.lv/dati/dataset/6b06a7e8-dedf-4705-a47b-2a7c51177473/resource
  curl -fL -o aw_eka.csv $B/a510737a-18ce-400f-ad4b-04fce5228272/download/aw_eka.csv
  curl -fL -o aw_iela.csv $B/3c4ab802-76cf-433c-9c1c-89215e28d833/download/aw_iela.csv
  curl -fL -o aw_ciems.csv $B/0d3810f4-1ac0-4fba-8b10-0188084a361b/download/aw_ciems.csv
  curl -fL -o aw_pagasts.csv $B/6ba8c905-27a1-443a-b9c6-256a0777425b/download/aw_pagasts.csv
  curl -fL -o aw_pilseta.csv $B/ee02baa4-2bc3-4f77-a6cb-5427a3e9befe/download/aw_pilseta.csv
  curl -fL -o aw_novads.csv $B/c62c60bb-58d4-4f26-82c0-5b630769f9d1/download/aw_novads.csv
  duckdb -f "$REPO/tests/registry/varis-csv.sql"
)
target/release/evaluate sample-lv --varis ../lv-data/varis-addresses-2026-10-29.csv --seed lv-2026-10 --out tests/registry/lv.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/lv.seeds.jsonl --out tests/registry/lv.jsonl --varis ../lv-data/varis-addresses-2026-10-29.csv
target/release/evaluate run --corpus tests/registry/lv.jsonl --index out --out eval-results/lv
```

`varis-addresses-2026-10-29.csv` has 545,885 addresses, 113,924,211 bytes, SHA-256 `fd9d833bcf57a9eec7672e1c73febf24e4c598851aa4630349bb509e6ab00dee`. The adapter checks that every row's full address (`STD`) is its components joined as the register writes them (a street and number, or a name in quotes; village or town; parish; municipality; postcode), which holds for all 545,885. Preparation, sampling and corpus generation each repeated byte for byte.

**Coordinates.** On 2026-10-07 VZD moved the register's coordinates from LKS-92 TM to LKS-2020 TM. In this release `KOORD_X` and `KOORD_Y` are LKS-2020 TM (EPSG:10306) northing and easting, and `DD_N` and `DD_E` WGS84 latitude and longitude to six decimals, as the metadata describes them. The adapter takes the WGS84 pair as published and rejects a point outside 55.6–58.1° N, 20.9–28.3° E, so swapped columns fail loudly. PROJ 9.9's EPSG:10306-to-EPSG:4326 transformation of the LKS-2020 pair agrees with the published WGS84 pair to 5e-7° (the six-decimal rounding) at all 545,885 points. The point is the centroid of the addressed building or land unit; no precision beyond the 150 m tolerance is claimed. A release before 2026-10-07 has LKS-92 TM (EPSG:3059) in `KOORD_X` and `KOORD_Y`; record the CRS with any other release.

## Sample

`sample-lv` reads 481,821 eligible addresses, each with an `LV-NNNN` postcode. An eligible address's class is:

- **street**: on a street, a number without leading zero or letter ("Puķu iela 19"): 247,657.
- **letter**: on a street, a number with one capital letter ("Zolitūdes iela 71D"): 55,851.
- **name**: a house name without digits or quotes, in a village, a parish or, rarely, a town without a street, the address of most rural Latvian farms (`"Riņņi", Vecates pag.`): 178,313.

It leaves out 15,146 street addresses with a building number ("Promenādes iela 4 k-1"), 1,251 other street numbers ("12/14", "41B k-8-14", a name on a street), 42,237 names with digits ("Ezītis 88", "Silmači 1"), 5,214 numbers in villages without streets (`"21A", Maļutki`), 206 names with quotes inside them and 10 with a trailing space.

In each municipality and class, `sample-lv` takes the address with the smallest seeded SHA-256 of its `KODS`: one street, one letter and one name address per municipality, 42, 42 and 38 locations. The state cities other than Jūrmala, Liepāja and Ventspils have no eligible name address. The sample weights municipalities and classes, not population or address counts: Rīga, with 9% of the addresses, has two locations. It is not a representative national accuracy sample. Of the 122 locations, 31 (62 cases) are held out by the shared location-hash rule.

Each location has two queries:

- **street**, **letter** and **name**: the register's full address, the form Latvian post and official documents use, with the country: `Puķu iela 19, Cēsis, Cēsu nov., LV-4101, Latvia`, `Uzvaras iela 8, Uzvara, Gailīšu pag., Bauskas nov., LV-3931, Latvia`, `"Silvas", Anna, Annas pag., Alūksnes nov., LV-4341, Latvia`, `Vīnogu iela 12A, Daugavpils, LV-5413, Latvia`.
- **spelt**: the same without postcode, with parish and municipality written out and the house name without quotes: `Uzvaras iela 8, Uzvara, Gailīšu pagasts, Bauskas novads, Latvia`, `Silvas, Anna, Annas pagasts, Alūksnes novads, Latvia`, `Vīnogu iela 12A, Daugavpils, Latvia`.

Every case requires goodMatch true at house level within 150 m of the register point, with the register's number and letter (or, for a name address, the name as the number), postcode and, on a street, the street with its type ("Puķu iela"). Municipality names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point. Each case's `places`, the names ReverseGeocode's answer must include one of, are the village or town and the parish and municipality, each abbreviated and in full ("Anna", "Annas pag.", "Annas pagasts", "Alūksnes nov.", "Alūksnes novads").

**Index ancestry.** Overture release 2026-10-21.0 has 550,686 Latvian addresses, all from OpenAddresses' `lv/countrywide`, which reads this register's spatial files (`AW_SHP.zip`, `Ekas.shp`). 545,739 of them have a record id that is a `KODS` of this extract; of those, all have its postcode, 545,446 its number or name, and against its points the median distance is 0.03 m, 545,522 lie within 1 m and 3 farther than 150 m. OpenAddresses' street function fails on these rows: 307,963 carry the whole register address as the street ("Saules iela 15, Liepāja, LV-3401"), 11,624 the same with the number's digits cut off the end of the postcode ("Amatas iela 11, Liepāja, LV-34"), and the 225,913 name addresses no street, with the name as the number and address levels such as "Ģibuļu pag. Talsu nov., LV-3251", the last of which the builder takes as the street. The builder also reads OpenAddresses' `lv/city_of_riga` (48,753 address points of Rīga's own open data, street and number); the chart reads the Latvia OpenStreetMap extract for landmarks only. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index from builder `aafd258112e50fe9` (Overture 2026-10-21.0, OpenAddresses and the chart's OSM extracts), SearchAddress passes 102 of 244 queries: 54 of the 122 register forms (30 of 42 street, 24 of 42 letter, none of 38 name) and 48 of the 122 spelt forms (25 street, 23 letter, no name), and 26 of the 62 held-out cases. A same-binary repeat has zero response drift. SuggestAddress finds an acceptable full-string result for 97 (18 street, 18 letter, no name and 61 spelt forms). ReverseGeocode returns the country, a place of the address and the house at all 244 points and a house 25 m away at all 244. Two critical results are triaged and labelled in `lv.labels.jsonl`. The failures:

- **Every house its own street (data; 30 register and 36 spelt forms on streets):** because the index's Latvian streets are whole register addresses, each holds one house, and the resolver answers a neighbour's: `Jēkabpils iela 30A, Jaunjelgava` finds the street "Jēkabpils iela 6, Jaunjelgava, Aizkraukles nov., LV-5134" (too_coarse, 0.4 km). Of the register forms, 25 answer another house's street, two find the house at the register point with goodMatch false (its road does not agree with the query's "Vidzemes iela"), one answers a Saules iela in another village 3.7 km away, one the postcode and one abstains. Of the spelt forms, 18 answer the village, parish or city (a state city's centre, 4–8 km away), 15 another house's street, two a near miss (`Raiņa iela 64A, Vaiņode` answers Raiņa iela 68 in Durbe, 34 km away, goodMatch true) and one abstains. The passes are the houses whose own street string the query matches best. In Rīga, whose streets `lv/city_of_riga` also names properly, one of the four queries passes.
- **Name addresses (data; 38 register and 38 spelt forms):** the farm is in the index only as a number of the pseudo-street its last Overture address level names ("Ģibuļu pag. Talsu nov., LV-3251"), which no query writes. The register forms answer the postcode (36; 8 of them over 5 km away, at most 9.5 km), the village (1) or its village's pseudo-street (1). The spelt forms answer the parish or village (29), the municipality (4), Latvia (1) or a place of the farm's name in another municipality (4): `Franči, Vecpils pagasts, Dienvidkurzemes novads` answers Franči in Saldus novads, 47 km away, and `Lejiņas, Birznieki, Ādažu pagasts` Birznieki in Aizkraukle, 84 km away. The two over 100 km are labelled (parser/matching): libpostal joins parish and municipality into one state_district or reads "pagasts" as the city, so the stated municipality does not rule out `Mežvidi` in Saldus novads (210 km, goodMatch true) or `Amatnieki` in Ropažu novads (113 km, goodMatch false).

These are diagnostic results on a local index, not a CI gold run.
