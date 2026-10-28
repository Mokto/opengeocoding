# Slovakia

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/sk.NOTICE)

**Slovakia** (`sk.seeds.jsonl`, `sk.jsonl`): 120 locations in 111 municipalities of 57 of the 79 districts and all 8 regions, 200 queries, drawn from the **Register adries** (address register) of the Ministry of Interior of the Slovak Republic (Ministerstvo vnútra SR), as published in its dataset [Adresy podľa krajov (csv)](https://data.slovensko.sk/datasety/b27f57f1-7e76-45e0-8968-631f9176b2e9): every valid address with its region, district, municipality, municipality part, street, building (súpisné) number, entrance (orientačné) number, postcode and address point. It is part of the [Register adries](https://data.slovensko.sk/datasety/a08562a8-e237-4e7f-a40a-9d0622445c6d) series in the national open-data catalogue and is updated quarterly.

## Licence

The catalogue publishes every distribution of the dataset under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) (author's work, original database and the database maker's special right), and records that it contains no personal data. CC0 asks no attribution; `sk.NOTICE` names the source anyway. OpenAddresses' `sk/countrywide` source records the same licence for the same download.

## Capture

The "všetky kraje" (all regions) distribution, `https://data.slovensko.sk/download?id=d22c42f3-82b5-450d-b8fb-4245d50a31ec`, was downloaded on 2026-10-28. The portal named it `Adresy pod_a krajov UTF8_04_2026.csv`; the dataset was last updated on 2026-08-21. It is 162,404,526 bytes, SHA-256 `312b38f506ee2f635fca0dd9c3a413d0e789506713919b5c3d78a5780a6fdbd3`, a UTF-8 CSV separated by semicolons with CRLF line ends and the header `IDENTIFIKATOR;KRAJ;OKRES;OBEC;CAST_OBCE;ULICA;SUPISNE_CISLO;ORIENTACNE_CISLO_CELE;PSC;ADRBOD_X;ADRBOD_Y`. The download address serves the current release, so keep the pinned file. No preparation is needed: the adapter reads the CSV as published.

```sh
mkdir -p ../ra-data
curl -fL -o ../ra-data/adresy-podla-krajov-utf8-04-2026.csv 'https://data.slovensko.sk/download?id=d22c42f3-82b5-450d-b8fb-4245d50a31ec'
target/release/evaluate sample-sk --ra ../ra-data/adresy-podla-krajov-utf8-04-2026.csv --seed sk-2026-10 --out tests/registry/sk.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/sk.seeds.jsonl --out tests/registry/sk.jsonl --ra ../ra-data/adresy-podla-krajov-utf8-04-2026.csv
target/release/evaluate run --corpus tests/registry/sk.jsonl --index out --out eval-results/sk
```

`ADRBOD_X` and `ADRBOD_Y` are WGS84 longitude and latitude written with a decimal comma ("19,121502;48,6661543"); the adapter rejects a point outside 47.7–49.7° N, 16.8–22.6° E, so swapped or projected coordinates fail loudly. The register does not say whether a point marks the entrance or the building; no precision beyond the 150 m tolerance is claimed. Sampling and corpus generation each repeated byte for byte.

## Sample

The file has 1,739,536 rows. `sample-sk` reads 1,703,024 eligible ones: it leaves out 35,190 without a point, 894 with orientation number 0, 224 without a postcode and 204 with a street but no orientation number or the reverse. An eligible row has a building number without leading zero, an orientation number of digits with at most one capital letter ("2", "131C"), a five-digit postcode, and a region, district and municipality. Identifiers are kept as written: 158,170 end in "A" ("432057A") without a plain twin. Two identifiers appear twice (an entrance row and a building row each); they are left out of the sample, and `build-gold --ra` rejects any duplicated or missing ID.

Each row is in one class: **dual**, a street address with building and orientation numbers ("Banská 150/2"); **dual_letter**, the same with a letter on the orientation number ("Rožňavská 1416/80A"); **village**, a house without a street, numbered on its municipality part, or the municipality when the register names no part ("Rakytovce 4247", "Hranovnica 1065"). In each region and class, `sample-sk` keeps the 200 rows with the smallest seeded SHA-256 of their ID, then takes the first five from different municipalities: 40 locations per class. Ten are in boroughs of Bratislava or Košice, which the register names as municipalities ("Bratislava-Ružinov"). The sample weights regions and classes, not population or address counts; it is not a representative national accuracy sample. Of the 120 locations, 28 are held out by the shared location-hash rule.

Queries are built from the register's fields in the Slovak postal form, the postcode written "976 32":

- **dual** and **dual_letter**: `Bernolákova 5999/28, 974 05 Banská Bystrica, Slovakia`
- **orientation_only**, for each of those 80 locations: `Bernolákova 28, 974 05 Banská Bystrica, Slovakia`, the form a street address is usually written in
- **village**: `Rakytovce 4247, 974 05 Banská Bystrica, Slovakia`

Every case requires goodMatch true at house level within 150 m of the register point, with the register's postcode. A dual-number answer may report the combined number, the building number or the orientation number with its letter, all from the same row, through `any_of`, as for RÚIAN in `cz.md`. Raw region names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point.

**Index ancestry.** All 1,704,346 Slovak addresses in Overture release 2026-10-21.0 come from OpenAddresses' `sk/countrywide`, which reads this same download and writes the number as `SUPISNE_CISLO/ORIENTACNE_CISLO_CELE`. The chart reads the Slovakia OpenStreetMap extract for landmarks only. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index from builder `66c174324c99692f` (Overture 2026-10-21.0, OpenAddresses and OSM), SearchAddress passes 196 of 200: 40/40 dual, 40/40 dual_letter, 79/80 orientation-only and 37/40 village queries, and all 47 held-out cases. There is no critical result, so nothing is labelled. A same-binary repeat has zero response drift. The four failures are too coarse:

- **Number read into the postcode (2, parser):** `Bratislava-Staré Mesto 8157, 811 02 Bratislava-Staré Mesto` answers with postcode "8157 811 02", and `Bratislava-Podunajské Biskupice 25388, 821 06 …` takes the five-digit house number for the postcode "253 88"; both answer the borough. The same queries without the postcode find the house at the register point.
- **Village part with a postcode (1, parser or matching):** `Staré Mesto 2545, 949 01 Nitra` answers the postcode area, 1.8 km away; `Staré Mesto 2545, Nitra, Slovakia` finds the house.
- **Orientation number alone (1, matching):** `Hlavná 28, 082 12 Kapušany` answers the street. The index has the one house 114/28 of that street, and the combined form passes; nine other Hlavná streets within about 20 km also number an entrance 28 (Fintice's 45/28 is 3.5 km away). Most likely more than one of them is a street candidate, and two such keys leave a street answer in `combined_number` (`src/index/country/cz_sk.rs`); this was not traced further.

SuggestAddress finds an acceptable full-string result for 197 of 200: it misses both forms of `Plotová 54/22, 044 13 Valaliky` and `Hoštiná 8, 020 01 Púchov`, houses the index files under a neighbouring municipality (Geča, Klieština). ReverseGeocode returns the country and the house number at all 200 points; 15 answers at 9 locations name a neighbouring municipality instead of the register's, the same misplaced houses among them (Hamuliakovo for Bratislava-Čunovo, Rosina for Žilina). These are diagnostic results on a local index, not a CI gold run.

Each case's `places`, the names ReverseGeocode's answer must include one of, are the municipality part, the municipality and, for a borough of Bratislava or Košice, the city.
