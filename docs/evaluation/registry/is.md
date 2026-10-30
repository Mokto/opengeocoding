# Iceland

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/is.NOTICE)

**Iceland** (`is.seeds.jsonl`, `is.jsonl`): 181 locations in all 61 municipalities, 312 queries, drawn from **Staðfangaskrá**, the access address register of Húsnæðis- og mannvirkjastofnun (HMS) ([dataset page](https://hms.is/stadfangaskra), [metadata](https://geoportal.gis.is/geonetwork/srv/api/records/%7BA879D973-CA98-49D7-AA50-7BC35047E461%7D)): one row per address point, with its address number (HEINUM), land parcel (LANDNR), municipality number (SVFNR), postcode, locator (the street, or the farm or place a numberless address names) in the nominative and dative, house number and letter, suffix, ISN93 and WGS84 point. Municipalities keep the register, which HMS refreshes weekly. The register has no place names, so two other public datasets supply them: the post town of each postcode from **Póstnúmeraskrá**, the postcode register of Byggðastofnun ([page](https://www.byggdastofnun.is/is/postthjonusta/postnumer), [metadata](https://geoportal.gis.is/geonetwork/srv/api/records/22e98d21-a86b-4b62-ad58-a6d17703b612)), and the municipality names from the municipal boundaries of Náttúrufræðistofnun's **IS 50V** (`IS_50V:mork_sveitarf_flakar`).

## Licence

- **Staðfangaskrá**: the metadata's use constraint is the Icelandic government's guidelines on reuse of public information (*Leiðbeiningar varðandi endurnot opinberra upplýsinga*, based on chapter VII of the Information Act 140/2012 and modelled on the UK Open Government Licence 3.0; read on 2026-10-30). The rights holder grants a perpetual, free, worldwide permission to copy, publish, distribute and adapt the information and to use it commercially, combined with other information or products, provided the source is stated, by default as "Byggir á upplýsingum frá" and the rights holder's name; use may not suggest official status or the holder's endorsement. OpenAddresses' `is/countrywide` records the same terms with attribution "HMS".
- **Póstnúmeraskrá**: the metadata's terms (read on 2026-10-30) let anyone copy, reuse and publish the data freely, provided it says the information comes from Byggðastofnun with the text "Byggt á gögnum frá Byggðastofnun".
- **IS 50V**: Náttúrufræðistofnun publishes its open data under CC BY 4.0 in accordance with the Act on the reuse of public sector information 45/2018 ([open data](https://www.natt.is/en/resources/open-data), read on 2026-10-30).

`is.NOTICE` gives all three attributions and the changes made. The corpus records the licence as `IS-Endurnot-Opinberra-Upplysinga AND Byggdastofnun-Postnumer AND CC-BY-4.0`.

## Capture

On 2026-10-30 the dataset page's CSV (`https://hmsstgsftpprodweu001.blob.core.windows.net/fasteignaskra/Stadfangaskra.csv`) served 0 bytes (Last-Modified 2026-10-30 00:40:05 UTC), so the register was read from HMS's WFS, which the metadata lists, as CSV with the same columns, at 18:18 UTC. Its `FID` is a per-request identifier, so a repeated download differs byte for byte; keep the pinned files:

| File | Source | Bytes | SHA-256 |
|---|---|---:|---|
| `stadfangaskra-wfs.csv` | `https://gis.fasteignaskra.is/geoserver/wfs?service=wfs&version=2.0.0&request=GetFeature&typeNames=public:Stadfangaskra&outputFormat=csv` | 38,286,009 | `667e1026b60622e67d3c9f3b076a1dcb0dffeec2b538e5b64f880122717b3c5b` |
| `postnumeraskra_2024.xlsx` | `https://www.byggdastofnun.is/static/files/Postmal/Postnumer/postnumeraskra_2024.xlsx` (Last-Modified 2024-04-18) | 14,317 | `a230577bfdf988cb880121dcdc979b5284a68648be81a254556cc44ab6f079f0` |
| `sveitarfelog-wfs.csv` | `https://gis.lmi.is/geoserver/wfs?service=wfs&version=2.0.0&request=GetFeature&typeNames=IS_50V:mork_sveitarf_flakar&outputFormat=csv&propertyName=objectid,uuid,stjornsyslusvaedi,nrsveitarfelags,sveitarfelag,upphafnotkunar,dagsleidrettingar,heimild,gagnaeigandi,gagnasafn` (IS 50V release 2026-09-24) | 10,592 | `9999896a3153e022595381d5204fd9f7eaef6db5684d9b4a7c8971ffeea8fefe` |

The register has 139,774 rows (one per HNITNUM), the latest corrected on 2026-10-30. `tests/registry/stadfangaskra-csv.sql` joins each row to its post town (every one of the 139,760 rows with a postcode has one in Póstnúmeraskrá) and municipality name, and fails on a postcode or municipality number without one. IS 50V numbers the Borgarbyggð that absorbed Skorradalshreppur on 2026-06-01 with Skorradalshreppur's 3506, while the register keeps Borgarbyggð's 3609 for both (it has no 3506), so the script reads IS 50V's 3506 as 3609. It writes one CSV named after the capture date:

```sh
mkdir -p ../is-data
(
  REPO=$PWD
  cd ../is-data
  curl -fL -o stadfangaskra-wfs.csv 'https://gis.fasteignaskra.is/geoserver/wfs?service=wfs&version=2.0.0&request=GetFeature&typeNames=public:Stadfangaskra&outputFormat=csv'
  curl -fL -o postnumeraskra_2024.xlsx https://www.byggdastofnun.is/static/files/Postmal/Postnumer/postnumeraskra_2024.xlsx
  curl -fL -o sveitarfelog-wfs.csv 'https://gis.lmi.is/geoserver/wfs?service=wfs&version=2.0.0&request=GetFeature&typeNames=IS_50V:mork_sveitarf_flakar&outputFormat=csv&propertyName=objectid,uuid,stjornsyslusvaedi,nrsveitarfelags,sveitarfelag,upphafnotkunar,dagsleidrettingar,heimild,gagnaeigandi,gagnasafn'
  duckdb -f "$REPO/tests/registry/stadfangaskra-csv.sql"
)
target/release/evaluate sample-is --stadfangaskra ../is-data/stadfangaskra-2026-10-30.csv --seed is-2026-10 --out tests/registry/is.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/is.seeds.jsonl --out tests/registry/is.jsonl --stadfangaskra ../is-data/stadfangaskra-2026-10-30.csv
target/release/evaluate run --corpus tests/registry/is.jsonl --index out --out eval-results/is
```

`stadfangaskra-2026-10-30.csv` has 139,774 rows, 26,767,341 bytes, SHA-256 `4b94e0c8173229b907b362f7bddbf6c83b96707fe9fada5061c6074da9a65783` (DuckDB 2.0.0 with its `excel` extension). The adapter checks that every row's published form (`VEF_BIRTING`) is its locator, number, letter, suffix and land parcel joined as the register writes them, which holds for all 139,774. Preparation, sampling and corpus generation each repeated byte for byte.

**Coordinates.** `N_HNIT_WGS84` and `E_HNIT_WGS84` are WGS84 latitude and longitude to eight decimals, `HNIT` the same point in ISN93 (EPSG:3057). The adapter takes the WGS84 pair as published and rejects a point outside 63.0–66.7° N, 24.7–13.3° W, so swapped columns fail loudly. PROJ's EPSG:3057-to-EPSG:4326 transformation of `HNIT` agrees with the WGS84 pair to 0.7 mm at all 139,774 points. `TEGHNIT` says what the point marks (57% "not yet reviewed", then a point within the lot, the estimated centre of the building or its main entrance); no precision beyond the 150 m tolerance is claimed.

## Sample

`sample-is` reads 116,732 eligible addresses, each with a three-digit postcode. It leaves out 12,455 rows with a suffix (`VIDSK`: "lóð", "land", "spennistöð", "8-16"), 882 whose point the register marks for review or without an address number (`YFIRFARID` 2 or 9), 2,739 points of addresses with more than one point, 4,755 addresses written like another in the same postcode (the same locator, number and letter), 1,303 numberless addresses of several words, and 908 others (locators with digits or punctuation such as "Naustabr.12-22 BL", numbers with a leading zero, a letter that is not one, no postcode or no dative). The several-word numberless addresses are more often lots, car parks and road reserves ("Land Bæjarins", "Bílastæði yfir Fífuhvammsbraut") than farms; the property register's land use would tell them apart. An eligible address's class is:

- **street**: a locator and a number ("Bæjarlind 8"): 95,682.
- **letter**: a number with one letter ("Eskihlíð 22A", "Kaupvangsstræti 8b"): 13,200.
- **name**: one word (hyphens allowed) without a number, mostly a farm, a holiday house or a place ("Elliðakot", "Suður-Nýibær", "Kálfatjarnarkirkja"): 7,850.

In each municipality (by `SVFNR`) and class, `sample-is` takes the address with the smallest seeded SHA-256 of its `HNITNUM`: 61 street, 59 letter and 61 name locations (Árneshreppur and Fljótsdalshreppur have no eligible letter address). The sample weights municipalities and classes, not population or address counts: Reykjavíkurborg, with 17% of the addresses, has three locations. It is not a representative national accuracy sample. Of the 181 locations, 32 (51 cases) are held out by the shared location-hash rule.

Queries:

- **street**, **letter** and **name**: the locator in the nominative, number and letter, postcode and post town, with the country: `Bæjarlind 8, 201 Kópavogur, Iceland`, `Kaupvangsstræti 8b, 600 Akureyri, Iceland`, `Elliðakot, 271 Mosfellsbær, Iceland`. The post town is Póstnúmeraskrá's, verbatim: `Sel, 225 Garðabær (Álftanes), Iceland`.
- **dative**: the same with the locator in the dative, as Icelanders write an address on a letter and as HMS writes its own ("Borgartúni 21, 105 Reykjavík"), for the 131 locations whose dative differs from the nominative: `Tónahvarfi 10G, 203 Kópavogur, Iceland`, `Elliðakoti, 271 Mosfellsbær, Iceland`. Post towns stay in the nominative, which Póstnúmeraskrá gives.

Street and letter cases require goodMatch true at house level within 150 m of the register point, with the register's number and letter ("22A") and postcode. A name address has no number, so its cases require goodMatch true at street level or finer within 150 m, with the postcode: the service, like Overture, takes the farm's name for a street. Municipality names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point, and 25 m away for the street and letter locations. Each case's `places`, the names ReverseGeocode's answer must include one of, are the post town and the municipality ("Kópavogur", "Kópavogsbær"); a rural postcode's post town is often not the nearest place, so the municipality is the name that holds there.

**Index ancestry.** Overture release 2026-10-21.0 has 139,583 Icelandic addresses, all from Staðfangaskrá. 139,542 of them have a record id that is a `HNITNUM` of this extract, and all 181 sampled points are among them. Against this extract the median distance is 0 m, 139,512 lie within 1 m and 6 farther than 150 m; 139,523 have its number, 139,530 its locator and 139,541 its postcode. Overture keeps only the nominative locator and writes the letter as the unit (13,739 of 14,009 lettered rows: number "22", unit "A"), the suffix too (12,154 rows), and gives each address its postcode's place as Pósturinn names it ("Selfoss, dreifbýli"). OpenAddresses' `is/countrywide` reads the same register, but the builder skips it (its licence is not one the builder recognises), and the chart reads the Iceland OpenStreetMap extract for landmarks only. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index from builder `aafd258112e50fe9` (Overture 2026-10-21.0, OpenAddresses and the chart's OSM extracts), SearchAddress passes 99 of 312 queries: all 61 street, none of 59 letter, 25 of 61 name and 13 of 131 dative forms (13 of 43 on streets, none of 40 lettered and none of 48 numberless), and 20 of the 51 held-out cases. A same-binary repeat has zero response drift. SuggestAddress finds an acceptable full-string result for 135 (61 street, no letter, 59 name and 15 dative forms). ReverseGeocode returns the country, a place of the address and, for a numbered address, the house at 200 of 312 points, and a house 25 m away at 198 of 203. One critical result is triaged and labelled in `is.labels.jsonl`. The failures:

- **Letters (data; 59 letter and 40 dative forms):** the builder keeps an Overture unit as part of the number only for a few countries' registers (Italy, Spain, Taiwan, New Zealand), so "22A" is indexed as a second house 22 and the resolver, asked for 22A, answers the street: 41 within 150 m and 17 within 1.8 km of the point, one a postcode 13 km away, all with goodMatch false. At the register point ReverseGeocode answers house 22 of the same street (58 wrong_house). Joining a single-letter Icelandic unit to the number, as for Spain's extensions, would change the builder hash.
- **Dative locators (data/matching; 118 of 131 dative forms):** the index has only the nominative, so a dative locator names nothing unless it is one edit from the nominative (the 13 passes: "Hlíðarþúfum", "Lálandi", "Brekku"). The rest answer the postcode (55 within 5 km, 11 from 5 to 39 km away), the post town or another place (36 within 25 km, 9 from 25 to 60 km away, one far miss) or nothing (6). The far miss is labelled: `Sylgjufelli, 851 Hella` answers the post town Hella, 134 km from the highland hut, goodMatch false; the nominative form finds the hut at its point.
- **Numberless addresses found with goodMatch false (matching; 18 name forms):** the resolver answers the farm or place at its point (all within 2 m) with its postcode, but goodMatch false: `Grótta, 170 Seltjarnarnes`, `Orustudalur, 803 Selfoss`. 17 of the 18 are in rural postcodes, 8 of them in another municipality than their post town's (`Litlahlíð, 605 Akureyri` is in Eyjafjarðarsveit); the exception, Grótta, is in Seltjarnarnes. Why the resolver withholds goodMatch is not triaged here.
- **Numberless addresses not found (data/matching; 18 name forms):** the answer is the postcode (5 within 5 km, 3 farther, up to 39 km for `Oddastaðavatn, 311 Borgarnes`), the post town or the municipality (4 within 17 km, 3 farther, up to 60 km for `Vesturöræfi, 701 Egilsstaðir`), a street of the same name 174 m from the point (`Miðvangur, 220 Hafnarfjörður`), or nothing (2: `Kambur, 381 Reykhólahreppur`, `Hrútey, 500 Staður`).
- **ReverseGeocode places (reverse; 7 points):** near a municipal boundary the answer names the neighbour: `Eskihlíð 22A, 105 Reykjavík` and `Urðarhæð 6, 210 Garðabær` are answered in Kópavogur, `Hallarmúli` (Skeiða- og Gnúpverjahreppur) in Hrunamannahreppur. At the highland hut Sylgjufell ReverseGeocode returns nothing.

These are diagnostic results on a local index, not a CI gold run.
