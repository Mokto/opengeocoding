# Estonia

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/ee.NOTICE)

**Estonia** (`ee.seeds.jsonl`, `ee.jsonl`): 120 locations in 110 settlements of 55 of the 79 municipalities and all 15 counties, 240 queries, drawn from the **Estonian Address Data System** (Aadressiandmete süsteem, ADS) of the Estonian Land and Spatial Development Board (Maa- ja Ruumiamet), as published in its [CSV extract of addresses with postcodes](https://geoportaal.maaruum.ee/est/ruumiandmed/aadressiandmed/aadressid-ja-posti-sihtnumbrid-p582.html): every valid address version with its ID (ADR_ID), postcode (SIHTNUMBER), short address (LAHIAADRESS), full address (TAISAADRESS) and reference point. ADS holds only official addresses; Omniva assigns the postcodes. The extract is regenerated on the 5th of every month.

## Licence

The extract page names no licence, but the Board's data FAQ ([Kas maa- ja ruumiameti andmete ja teenuste kasutamiseks on vaja litsentsi?](https://geoportaal.maaruum.ee/est/abi-ja-juhised/andmed/kas-maa-ja-ruumiameti-andmete-ja-teenuste-kasutamiseks-on-vaja-litsentsi/82)) says that all spatial data the Board collects, address data included, are open data under its [open data licence](https://geoportaal.maaruum.ee/avaandmete-litsents) ([English](https://geoportaal.maaruum.ee/opendata-licence)), and the Board's metadata calls that licence the equivalent of CC BY 4.0. The licence of 01.01.2025 (PDF, 85,098 bytes, SHA-256 `167719a3834cd61e7d03e9437bd823559cf2cb7f515e24dc3a90f1795d20282e` on 2026-10-29) lets the licensee make derivatives, combine the data with other data, use it commercially and redistribute it, provided the licensor's name, the dataset's name and the data's age or extract date are given and the licence text or a link to it accompanies the data. `ee.NOTICE` gives all four. OpenAddresses' `ee/countrywide` records the same licence for the ADS extract it reads. The corpus records the licence as `MaRu-Open-Data-Licence-2025`.

## Capture

`https://geoportaal.maaruum.ee/docs/aadress/sihtnumbrid.zip` was downloaded on 2026-10-29: 29,591,636 bytes, SHA-256 `3af4d373fb82e745f6cf429b30ae23d1b708618854082f17c67f16a7192ea58d`. It holds one file, `sihtnumbrid.csv`, dated 2026-10-05 in the archive: 148,313,792 bytes, SHA-256 `66a3d18c344bbf0cfd3b38fe7e7a43cca9cc4085780bcb9041a1f16947f1da51`, a UTF-8 CSV separated by semicolons, without quotes, with CRLF line ends and the header `ADR_ID;SIHTNUMBER;LAHIAADRESS;TAISAADRESS;VIITEPUNKT_X;VIITEPUNKT_Y`. The address serves the current month's extract, so keep the pinned file. The adapter reads the CSV as published, renamed after its extract date:

```sh
mkdir -p ../ee-data
curl -fL -o ../ee-data/sihtnumbrid-2026-10-29.zip https://geoportaal.maaruum.ee/docs/aadress/sihtnumbrid.zip
unzip -p ../ee-data/sihtnumbrid-2026-10-29.zip sihtnumbrid.csv > ../ee-data/sihtnumbrid-2026-10-05.csv
target/release/evaluate sample-ee --ads ../ee-data/sihtnumbrid-2026-10-05.csv --seed ee-2026-10 --out tests/registry/ee.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/ee.seeds.jsonl --out tests/registry/ee.jsonl --ads ../ee-data/sihtnumbrid-2026-10-05.csv
target/release/evaluate run --corpus tests/registry/ee.jsonl --index out --out eval-results/ee
```

The reference point is in L-EST97 (EPSG:3301). As the page says, `VIITEPUNKT_X` is the six-digit "mathematical X", the easting, and `VIITEPUNKT_Y` the seven-digit northing; the adapter rejects an easting outside 300–800 km or a northing outside 6,300–6,700 km, so swapped columns fail loudly. It converts the point to WGS84 with the projection's own Lambert conformal conic on GRS80, which matches PROJ's EPSG:3301-to-EPSG:4326 transformation to 1e-9° (`lest97_matches_proj`); the WGS84 points the In-ADS gazetteer serves differ from both by about 0.4 m. ETRS89 and WGS84 are treated as the same. ADS puts a point on the most important object an address names (the dwelling, if a parcel and its buildings share the address); no precision beyond the 150 m tolerance is claimed. Sampling and corpus generation each repeated byte for byte.

## Sample

The file has 1,369,879 rows, all with a postcode and a point, each ADR_ID once. `sample-ee` reads 573,945 eligible ones. An eligible row's full address is its county, municipality and at most one settlement unit (küla, alevik, alev or linn) or Tallinn city district (linnaosa), followed by its short address, which is one of:

- **street**: a street whose last word is one of tn, tänav, tee, mnt, maantee, pst, puiestee, põik or allee (or one word ending in -tee, "Kesktee"), and a number without leading zero ("Pargi tn 2"): 226,691 rows.
- **letter**: the same with one lower-case letter ("Pargi tn 2b"): 41,832.
- **name**: in a village (küla), one word without digits that does not end in -tee, the name of a farm or other property, which rural Estonia addresses instead of a street ("Antsu"): 305,422.

It leaves out 624,877 apartments and other building parts ("Saue tn 13-5"), 16,890 numbers of a building within a plot ("Sillutise tn 3/1"), 18,965 streets without a number, 18,535 parcels numbered in a forest district ("Narva metskond 143"), 11,573 names of more than one word (many are substations, fields or roads: "Sadama alajaam"), 11,154 one-word names outside villages, and 93,940 other numbered forms (plots of gardening cooperatives and small places "AÜ", "vkt", road kilometre ranges, a property name with a building number "Kreisi/2", and street parcels "Urkna tee L2").

In each county and class, `sample-ee` keeps the 200 rows with the smallest seeded SHA-256 of their ADR_ID, then takes the first from different settlements: three street, two letter and three name addresses per county, 45, 30 and 45 locations. The sample weights counties and classes, not population or address counts: Harju county, with 37% of the addresses, has eight locations and Tallinn one. It is not a representative national accuracy sample. Of the 120 locations, 27 (54 cases) are held out by the shared location-hash rule.

Each location has two queries:

- **street**, **letter** and **name**: Omniva's postal form, the postcode before the town or county. A town that is its own municipality is named without "linn": `Pargi tn 2b, 76607 Keila, Estonia`, `Mustamäe tee 51, 10621 Tallinn, Estonia` (Tallinn's district is left out). Elsewhere the settlement and municipality come first: `Antsu, Ruhnu küla, Ruhnu vald, 93001 Saare maakond, Estonia`, `Aia tn 6a, Haljala alevik, Haljala vald, 45301 Lääne-Viru maakond, Estonia`.
- **register**: the register's own full address, county first, with the country: `Harju maakond, Tallinn, Haabersti linnaosa, Räime tn 27, Estonia`.

Every case requires goodMatch true at house level within 150 m of the register point, with the register's number (or, for a name address, the name as the number) and postcode. Raw county names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point. Each case's `places`, the names ReverseGeocode's answer must include one of, are the settlement or city district and the municipality, each also without its type word ("Ruhnu küla", "Ruhnu").

**Index ancestry.** All 2,277,177 Estonian addresses in Overture release 2026-10-21.0 come from OpenAddresses' `ee/countrywide`, which reads a mirror of the ADS extract with address components (not this file) and writes no postcode; a name address there has the name as its number and no street, and the builder files it under its village. The chart reads the Estonia OpenStreetMap extract for landmarks only. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`. Because the index's Estonian houses have no postcode, an answer repeats the query's postcode (a house asked with postcode 99999 answers 99999), so the postcode expectation catches only a contradicting one.

## Local baseline

On a local full index from builder `7e8df68716d10def` (Overture 2026-10-21.0, OpenAddresses and the chart's OSM extracts), SearchAddress passes 88 of 240: 45/45 street, 28/30 letter, 0/45 name and 15/120 register queries, and 21 of the 54 held-out cases. A same-binary repeat has zero response drift. One critical result is triaged and labelled in `ee.labels.jsonl`. The failures:

- **Name addresses (45, parser/matching):** no postal-form name query finds the house: 42 answer the village's postcode area and one a street of the village (too_coarse), two a street 2 km away (near_miss; `Nurme, Kapu küla` answers Nurme in Koeru). The index has these houses: ReverseGeocode at the register point answers 43 of them by name ("vanaveski" on "Vaidasoo küla"). The farm name is not looked up as a number of the village.
- **Register form, name addresses (45, parser/matching):** 43 answer the municipality (near_miss, 3–36 km) and 2 the village or town.
- **Register form, street and letter addresses (60 of 75, parser/matching):** libpostal reads county-first text as one road ("oela küla suislepa tee"). 52 abstain although the raw lookup finds the house, 3 answer the town or municipality, and 1 answers the house with goodMatch false (Võhma). Four take the street's name for a town elsewhere: `Võru tn 12` in Põlva answers Võru (24 km), `Muru tn 15` in Jõhvi Muru küla (48 km), `Viljandi tn 13` in Türi Viljandi (50 km), and the labelled far_miss `Suislepa tee 9` in Oela küla Suislepa küla in Viljandi County, 125 km away, although the query states Rapla maakond. The 15 passes are all in towns: a city municipality ("Narva linn") or a town of a rural municipality.
- **Letter with a postcode (2, matching):** `Karjamaa tn 1a, 80020 Pärnu` answers the town, while `Karjamaa tn 1a, Pärnu` and `Karjamaa tn 1, 80020 Pärnu` find the house; `Tallinna mnt 44f, Juuru alevik, Rapla vald, 79401 Rapla maakond` answers the postcode area, while the same with "maantee" or without the postcode finds 44f. Not traced further.

SuggestAddress finds an acceptable full-string result for 69 of 240: 41/45 street and 28/30 letter queries (it misses six addresses of city municipalities, such as `Kuu tn 29, 21055 Narva`), and no name or register query. ReverseGeocode returns the country, a place of the address and the house at 228 of 240 points, and a house within the radius 25 m away at all 240. The 12 misses at 6 locations answer a nearer neighbour, three of them a road parcel the index holds as a streetless number ("viljandi tänav l1" on "Türi linn"). These are diagnostic results on a local index, not a CI gold run.
