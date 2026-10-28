# Hong Kong

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/hk.NOTICE)

**Hong Kong** (`hk.seeds.jsonl`, `hk.jsonl`): 120 locations in all 18 districts, 240 queries, drawn from the **Address Lookup Service** (ALS) of the Digital Policy Office, the Government's aggregated address register. Its bulk download on DATA.GOV.HK, [Addresses from Address Lookup Service](https://data.gov.hk/en-data/dataset/hk-dpo-als_01-als), holds one GeoJSON file per district: building number, street or village, location name, district and region in English and Chinese, a building identifier (CsuId), a GeoAddress and a WGS84 point for each premises. It is updated monthly.

## Licence

DATA.GOV.HK's [Terms and Conditions of Use](https://data.gov.hk/en/terms-and-conditions) (version 1.2, 26 May 2025; the dataset names no other licence) let anyone browse, download, distribute and reproduce the Data, commercially or not, free of charge, on condition that the source is identified clearly, the Government's and the Relevant Organisations' ownership of the intellectual property rights is acknowledged in every copy, proper attribution is given to the Government, the Relevant Organisations and DATA.GOV.HK, and the user indemnifies them against claims arising from that use. `hk.NOTICE` gives the attribution; the corpus records the terms as `DATA.GOV.HK-Terms-1.2`. The terms may change without notice, so re-read them before a new capture. OpenAddresses publishes the same register under the same terms (`hk/countrywide-en`, `hk/countrywide-zh`, attributed to the Hong Kong Digital Policy Office). This is a reading of the published terms, not a grant from the Government.

## Capture and preparation

```sh
curl -fL -o SOURCE/ALS-GeoJSON.zip https://www.als.gov.hk/data/ALS-GeoJSON.zip
unzip -p SOURCE/ALS-GeoJSON.zip 'ALS_GeoJSON_329/als_addresses_(*' > SOURCE/als-2d-2026-10-20.geojson
target/release/evaluate sample-hk --als SOURCE/als-2d-2026-10-20.geojson --seed hk-2026-10 --out tests/registry/hk.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/hk.seeds.jsonl --out tests/registry/hk.jsonl --als SOURCE/als-2d-2026-10-20.geojson
```

The capture of 2026-10-28 is `ALS-GeoJSON.zip` (served with Last-Modified 2026-10-20), 20,777,483 bytes, SHA-256 `4549e4f774224abdef062cb6ffaa7e75f0d8dc02187a64bc6c05434472b6f577`, directory `ALS_GeoJSON_329`. `unzip -p` concatenates its 18 district files, the two-dimensional (building-level) addresses, in archive order into `als-2d-2026-10-20.geojson`, 210,133,459 bytes, SHA-256 `e5683278a8812693a67923f59ed585353e9d4180e44717939c51e1509d79c95b`; `sample-hk` reads those FeatureCollections one after another. The 19th file, flat-level addresses of public rental housing (`als_addresses_3d_(public_rental_housing).geojson`, 621,802,870 bytes), is left out: it repeats the estates' buildings flat by flat. Re-running the extraction, sampling and `build-gold` reproduced every file byte for byte. The ALS does not say what a point marks (building centroid or entrance), so no precision beyond the 150 m tolerance is claimed.

## Sample

The 18 district files hold 182,254 records, 2 of them without a point. A record is eligible when it is one plain building number on one named street (`EngStreet`/`ChiStreet`) or one named village (`EngVillage`/`ChiVillage`) in both languages: the number has one to four digits, no leading zero and at most one capital letter after it, the same in English and Chinese, with no `BuildingNoTo` (a range such as 405–406); the street or village has a name in both languages and the English name has no comma. That rejects estates and buildings known only by name, lots and other numbering (`G3`, `C34E`, `27B1`) and ranges. Records are grouped by English district, street or village, its name, location name and number; several records of one number (an estate's blocks, a school's wings) form one address when they agree in Chinese and lie within 50 m of the first, and its point is that of the record with the smallest GeoAddress. Of 146,482 such addresses, 1,929 are dropped for points farther apart or conflicting Chinese, leaving 144,553.

The draw has one pool per district and kind, **street** (all 18 districts) or **village** (13 districts), and keeps in each pool, by the seeded SHA-256 of the address key, the first four addresses on different streets or villages; Eastern and Wong Tai Sin have two village names only, so 31 pools give 120 locations: 72 street and 48 village (`hk_street`, `hk_village`), 99 plain numbers and 21 with a letter, 57 with a location name. The sample weights districts and kinds equally, not population or address counts; it is not a representative territory-wide accuracy sample. 26 locations are held out by the shared location-hash rule, both queries of a location together.

Each location has two queries, both from the record's own fields in the register's capitals and order, and both require goodMatch true:

- **en**: number, name, location name when there is one, district, region and `HONG KONG`: `27 KO FONG STREET, KWAI CHUNG, KWAI TSING DISTRICT, NEW TERRITORIES, HONG KONG`. The region code is spelled out (`KLN` as `KOWLOON, HONG KONG`, `NT` as `NEW TERRITORIES, HONG KONG`, `HK` as `HONG KONG`).
- **zh**: `香港`, the region unless it is 香港, district, location name, name, number and 號: `香港新界葵青區葵涌高芳街27號`. Location names keep the register's spacing (`大嶼山 梅窩`).

Both expect house level and the building number within 150 m of the ALS point. Hong Kong has no postcodes. Region expectations are left empty, so ResolveRegion is not checked; ReverseGeocode is asked about every point. Cases have no `places`: the register's district names (`CENTRAL & WESTERN DISTRICT`) are not the names the index gives.

**Index ancestry.** All 176,772 Hong Kong addresses in Overture release 2026-09-23.1 come from OpenAddresses' `hk/countrywide-en`, a crawl of this register uploaded on 2026-06-06; the builder also reads `hk/countrywide-zh`, the Chinese layer of the same crawl, which Overture lacks. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local world index from builder `cd477fbcc95de70c` (Overture 2026-09-23.1, OpenAddresses and OSM), the corpus run alone:

| Kind | Form | Queries | SearchAddress passes |
|---|---|---:|---:|
| Street | en | 72 | 41 |
| Street | zh | 72 | 6 |
| Village | en | 48 | 6 |
| Village | zh | 48 | 1 |

Overall 54/240 pass (11/52 held out): 157 abstain, 22 are too coarse, 5 near misses and 2 confidence mismatches, with no critical result. ReverseGeocode passes at 186/240 points; at the other 54 the nearest house is another number (`27-29` for 29 Elgin Street, a range the register also lists), and all 240 checks 25 m away find a house. SuggestAddress finds an acceptable full-string result for 52/240.

The index holds most of these houses: at 150 of the 186 failing queries' points ReverseGeocode returns the same number on the same street or village. Each failing query was retried in shorter forms against the same service; these controls are diagnostics, not cases, and changed no expectation:

- **Chinese prefix (parser, 73 of 113 failing zh):** the street and number alone (`高芳街27號`) pass for 73, but with `香港` before them only 5 do. libpostal reads `香港新界葵青區葵涌高芳街27號` as country 香港, state 新界, postcode 葵 and road 青區葵涌高芳街.
- **`HONG KONG` as the town (matching, 35 of 73 failing en):** `27 KO FONG STREET` passes but `27 KO FONG STREET, HONG KONG` abstains: libpostal reads `HONG KONG` as the city, and a house whose locality is Kwai Chung is not accepted for it. Elgin Street, in the locality Hong Kong, passes. Location names are also merged into the road (`ko fong street kwai`, city `chung`).
- **`&` in a district (parser, 4 en):** all four Central & Western English queries abstain; libpostal takes `CENTRAL &` into the road. With `HONG KONG` instead they pass.
- **Not resolved by the controls (23 en, 40 zh):** 50 of these have the house at the register point in the index, so the failure is in the query path, not the data; the rest were not inspected.

No failure is labelled `known_failure`: none is critical. The controls touched held-out cases only to attribute them; nothing was tuned. These are diagnostic results on a local index, not a CI gold run. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure. Human review of the source points is pending.
