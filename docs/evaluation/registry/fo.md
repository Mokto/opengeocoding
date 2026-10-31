# Faroe Islands

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/fo.NOTICE)

**Faroe Islands** (`fo.seeds.jsonl`, `fo.jsonl`): 136 locations in all 29 municipalities, one query each, drawn from the address register (*adressur*) of the Faroese Environment Agency, **Umhvørvisstovan**, as its ArcGIS layer of house numbers (`us_adr_husanr`) serves it. Each record has an address number (`address_id`), street, number with its letter (`house_no`, also split into `house_digit` and `house_letter`), postcode and its village or town (`city`), municipality in the genitive (`Tórshavnar`, as in Tórshavnar kommuna), cadastral references, a building number and the point.

## Licence

Umhvørvisstovan's [terms and conditions](https://www.foroyakort.fo/um-foeroyakort/terms-and-conditions-in-english) for data from foroyakort.fo (read on 2026-10-31) grant a worldwide right to copy, distribute and publish the data, modify and combine it with other data, and use it commercially and non-commercially. The use must not appear to be authorised, endorsed, recommended or promoted by Umhvørvisstovan, and must follow Faroese law and source reference practice. OpenAddresses' `fo/countrywide` reads the same layer under the same terms. `fo.NOTICE` gives the attribution and the changes made, and the corpus records the terms as `Umhvorvisstovan-Terms`. This is a reading of the published terms, not a grant from Umhvørvisstovan.

## Capture

```sh
mkdir -p ../fo-data
curl -fsS -G 'https://gis.us.fo/arcgis/rest/services/adressur/us_adr_husanr/MapServer/0/query' \
  --data-urlencode 'where=1=1' --data-urlencode 'outFields=*' --data-urlencode 'returnGeometry=true' \
  --data-urlencode 'outSR=4326' --data-urlencode 'orderByFields=objectid' \
  --data-urlencode 'resultRecordCount=30000' --data-urlencode 'f=json' -o ../fo-data/fo-adressur-2026-10-31.json
target/release/evaluate sample-fo --adressur ../fo-data/fo-adressur-2026-10-31.json --seed fo-2026-10 --out tests/registry/fo.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/fo.seeds.jsonl --out tests/registry/fo.jsonl --adressur ../fo-data/fo-adressur-2026-10-31.json
target/release/evaluate run --corpus tests/registry/fo.jsonl --index out --out eval-results/fo
```

The layer is live, with no dated release. The query of 2026-10-31 at 11:06 UTC returned all 26,529 features in one response (the layer allows 30,000), 18,866,512 bytes, SHA-256 `443c10319b4b00bce0f0ed068415be306620a851383cf27bd3efde6f39fc0809`. A second query a few minutes later was byte-identical. The latest `collection_date` is 2026-10-23. The URL always serves the current register, so keep the pinned file. Sampling and corpus generation repeated byte for byte.

**Coordinates.** `latitude` and `longitude` are WGS84 fields of each record. The query also asks for the point's geometry in WGS84 (`outSR=4326`; the layer stores it in EPSG:5316). The two agree to within 1 cm at 26,521 records and differ by 2.8 m to 31 m at 8. The register does not say which is right, so the adapter leaves those 8 out, takes the fields, and rejects a point outside 61.3–62.5° N, 7.8–6.1° W so swapped axes fail loudly. No precision beyond the 150 m tolerance is claimed.

## Sample

Every record has a number and a postcode. `house_no` is always `house_digit` followed by `house_letter`, and the address field is always the street and `house_no`. No two records share a street, number and postcode. Each of the 117 postcodes has one village or town. A record is eligible when its number has one to four digits without a leading zero and at most one capital letter, its street has no comma, and its fields agree with its geometry: 26,521 of 26,529 records.

The draw has one pool per municipality and class: **street** (a plain number) or **letter** (`47B`; 1,443 records, in 20 municipalities). In each pool it takes, by the seeded SHA-256 of `address_id`, the first addresses on different streets: 4 street and 1 letter address per municipality, so 116 `fo_street` and 20 `fo_letter` locations. Every municipality weighs the same: Fugloy (57 addresses) has as many street locations as Tórshavn (9,180). The sample is not a representative national accuracy sample. 34 locations are held out by the shared location-hash rule.

Each location has one query: street, number, postcode, village or town and country, as the register writes them: `Gerðisvegur 27C, 470 Eiði, Faroe Islands`. It requires goodMatch true at house level, within 150 m of the register point, with the register's number (`27C`), street and postcode. The Faroe Islands have no ISO 3166-2 subdivisions, so no region is expected. Each case's `places`, the names ReverseGeocode's answer must include one of, are the village or town and, for the seven postcodes whose name adds the island or town that tells namesakes apart (`Nes, Vágur`, `Syðradalur, Kalsoy`), also the bare village name. That rule was added after the first baseline run showed `Á Nesi 36, 925 Nes, Vágur` answered as Nes, and it applies to all seven names; the full name is still accepted.

**Index ancestry.** Overture release 2026-09-23.1 has 26,428 Faroese addresses, all from OpenAddresses' `fo/countrywide`, which reads this layer. 26,426 records of this capture match one of them by street, number, letter and postcode, 26,417 within 1 m and none farther than 150 m. Overture splits the letter off as the unit (number `47`, unit `B`). The local index reads no OpenAddresses file and no OpenStreetMap extract for the Faroe Islands. The sample is selected independently of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index from builder `74654cb597c8d104` (Overture 2026-09-23.1, OpenAddresses and OSM), SearchAddress passes 116 of 136 queries: all 116 street addresses and none of the 20 letter addresses; 28 of 34 held out. SuggestAddress finds an acceptable full-string result for the same 116. ReverseGeocode passes at 102 of 136 points; 25 m away it finds a house at 120. There is no critical result, and nothing is labelled. The failures:

- **Letters (data; all 20 letter queries):** the builder keeps an Overture unit as part of the number only for some countries' registers (#453 added Iceland's letters), so `27C` is indexed as a further house 27. The resolver, asked for 27C, answers the street (`too_coarse`, goodMatch false, 31 m to 613 m from the point), and at the register point ReverseGeocode answers house 27 of the same street (18 `wrong_house`). Joining a single Faroese letter to its number, as for Iceland, would change the builder hash and needs its own pull request.
- **ReverseGeocode places (reverse; 16 points):** the house is right, but the place named is a neighbouring village: Skopun's houses are answered in Sandur, Runavík's in Toftir, Stykkið's in Kvívík, and Tvøroyri's in Froðba. Overture has all of these villages as localities; the index names the nearest locality point where no polygon decides.

These are diagnostic results on a local index, not a CI gold run. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure.
