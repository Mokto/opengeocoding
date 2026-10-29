# Slovenia

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/si.NOTICE)

**Slovenia** (`si.seeds.jsonl`, `si.jsonl`): 96 locations in 93 settlements of 66 of the 212 municipalities and all 12 statistical regions, 192 queries, drawn from the **Register of Addresses** (Register naslovov, RN) of the Surveying and Mapping Authority of the Republic of Slovenia (Geodetska uprava Republike Slovenije, GURS), as published on [OPSI](https://podatki.gov.si/dataset/register-naslovov) and exported by its [Javni geodetski podatki](https://ipi.eprostor.gov.si/jgp/data) application ("Register naslovov za območje Slovenije"): one row per house number and one per apartment or business unit, with the municipality, settlement, street (where the settlement has streets), postal area, house number and letter, the address centroid inside the building's footprint, and the identifiers of the spatial units the centroid lies in. GURS refreshes the export weekly.

## Licence

OPSI lists the dataset under Creative Commons Attribution 4.0 (CC BY 4.0), a high-value dataset without access restrictions. GURS's [general terms for the use of geodetic data](https://www.e-prostor.gov.si/fileadmin/struktura/General_terms.docx) (01.04.2023; 52,246 bytes, SHA-256 `69f5480b9466045c06b88ea4f2a515f57da30db1502cec12112e8a842494232a` on 2026-10-29) say the same: commercial and non-commercial use, sharing and adaptation are allowed, provided every publication names "Surveying and Mapping Authority of the Republic of Slovenia, the type of data and the time to which the data refer and/or the date of the database". `si.NOTICE` gives all three. OpenAddresses' `si/countrywide` records the same licence for the same register. The corpus records the licence as `CC-BY-4.0`.

## Capture

The export was downloaded on 2026-10-29 through the application's API (it answers with a signed, short-lived link): `RN_SLO_NASLOVI_20261025.zip`, 108,528,093 bytes, SHA-256 `bfc93a49e295b01408af03d1de4cb159c21b2e26fe71f5f4e451006c099b98e3`. It holds one file, `RN_SLO_NASLOVI_register_naslovov_20261025.csv`, the database state of 2026-10-25: 511,124,897 bytes, SHA-256 `577f61a2a453a27145c18cbc75585a7214b1f3e9b1c04bf8d96d52cfdff340c9`, UTF-8 without a byte-order mark, comma-separated, quoted where a field holds a comma ("Novo Polje, cesta VII"), CRLF line ends, the 39 columns `Struktura_RN.xlsx` describes (23,601 bytes, SHA-256 `ab8ec4a9eafd7bbb22748234d6b714858d0615b68323cd6ce9847acb7638a2ea`). The application serves only the latest export, so keep the pinned file. The adapter reads the CSV as published:

```sh
mkdir -p ../si-data
curl -fsSL "$(curl -fsS 'https://ipi.eprostor.gov.si/jgp-service-api/display-views/groups/120/composite-products/281/file?filterParam=DRZAVA&filterValue=1' | jq -r .url)" -o ../si-data/RN_SLO_NASLOVI_20261025.zip
unzip -d ../si-data ../si-data/RN_SLO_NASLOVI_20261025.zip
target/release/evaluate sample-si --gurs ../si-data/RN_SLO_NASLOVI_register_naslovov_20261025.csv --seed si-2026-10 --out tests/registry/si.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/si.seeds.jsonl --out tests/registry/si.jsonl --gurs ../si-data/RN_SLO_NASLOVI_register_naslovov_20261025.csv
target/release/evaluate run --corpus tests/registry/si.jsonl --index out --out eval-results/si
```

**Distribution system.** This is the renovated distribution (Javni geodetski podatki, "prenovljen sistem distribucije" on OPSI), the only one left: the earlier e-Geodetski podatki portal (`egp.gu.gov.si`), which OpenAddresses' 2022 building layer cites, no longer resolves. House numbers that differ between the two systems cannot occur in this corpus.

**Coordinates.** `E` and `N` carry no reference system in the CSV or in `Struktura_RN.xlsx`. They are D96/TM (EPSG:3794): the spatial-units shapefiles of the same export name it in their `.prj`, OpenAddresses reads the register with `EPSG:3794`, and the converted points of 580,152 house numbers lie within 5 mm of Overture's for the same identifiers (below). The adapter converts them with the projection's transverse Mercator (central meridian 15° E, scale 0.9999, false northing −5,000 km) on GRS80, which matches PROJ's EPSG:3794-to-EPSG:4326 transformation to 1e-8° (`d96tm_matches_proj`), and rejects an easting outside 370–630 km or a northing outside 25–200 km, so swapped columns fail loudly. D96 (ETRS89) and WGS84 are treated as the same. The centroid lies inside the building; no precision beyond the 150 m tolerance is claimed. Statistical region names come from the register of spatial units of the same date (`RPE_SLO_PROSTORSKE_ENOTE_statisticne_regije`), since the address file has only their identifiers. Sampling and corpus generation each repeated byte for byte.

## Sample

The file has 1,103,112 rows: 580,322 house numbers (rows without `ST_STANOVANJA`, each `EID_HISNA_STEVILKA` once) and 522,790 apartment and business units, which repeat their house's point and are left out. Every house number is eligible: each has a number without leading zero, at most one lower-case letter, a four-digit postcode, a settlement, a municipality and a point. Its class is:

- **street**: a house on a named street, without letter ("Delavska cesta 71"): 199,899.
- **letter**: a house with a letter, on a street or numbered by its settlement ("Zaloška cesta 236c", "Bodešče 12a"): 146,212.
- **settlement**: a house without letter in a settlement that has no streets and numbers its houses itself ("Davča 78"): 234,211.

In each statistical region and class, `sample-si` keeps the 200 rows with the smallest seeded SHA-256 of their `EID_HISNA_STEVILKA`, then takes the first from different settlements: three street, two letter and three settlement addresses per region, 36, 24 and 36 locations. The sample weights regions and classes, not population or address counts: Osrednjeslovenska, with 20% of the house numbers, has eight locations and Ljubljana two. It is not a representative national accuracy sample. Of the 96 locations, 19 (38 cases) are held out by the shared location-hash rule.

Each location has two queries:

- **street**, **letter** and **settlement**: Pošta Slovenije's order, the house then postcode and post office as the register names it (bilingual posts included): `Delavska cesta 71, 4000 Kranj, Slovenia`, `Davča 78, 4228 Železniki, Slovenia`, `Vanganelska cesta 25f, 6000 Koper - Capodistria, Slovenia`. A street's settlement is added before the postcode where it differs from the post office's name (or the part of it before " - "): `Koroška cesta 6, Koroška Bela, 4270 Jesenice, Slovenia`. Pošta writes the settlement only where a post office has several streets of one name; it writes a letter after a space ("11 a"), the register and this corpus without one.
- **municipal**: no postcode; the street's settlement, if it differs from the municipality, then the municipality: `Davča 78, Železniki, Slovenia`, `Koroška cesta 6, Koroška Bela, Jesenice, Slovenia`, `Delavska cesta 71, Kranj, Slovenia`. A settlement that is its own municipality is named once.

Every case requires goodMatch true at house level within 150 m of the register point, with the register's number and letter, postcode and, on a street, the street. Region names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point. Each case's `places`, the names ReverseGeocode's answer must include one of, are the settlement and the municipality and, in the bilingual areas, their Italian or Hungarian names (`NASELJE_NAZIV_DJ`, `OBCINA_NAZIV_DJ`).

**Index ancestry.** Overture release 2026-10-21.0 has 580,213 Slovenian addresses, all with source dataset `GURS` (updated 2026-10-14). 580,152 of them share an `EID_HISNA_STEVILKA` with this file; all have its street or settlement name and postcode, all but two its number, and their points lie within 5 mm of the converted register points. The builder also reads OpenAddresses' `si/countrywide` (573,634 rows, a 2024 run over the same register); the chart reads the Slovenia OpenStreetMap extract for landmarks only. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index from builder `5009a82d974f39f5` (Overture 2026-10-21.0, OpenAddresses and the chart's OSM extracts), SearchAddress passes 175 of 192: all 96 postal-form queries and 79 of 96 municipal ones, and 35 of the 38 held-out cases. A same-binary repeat has zero response drift. SuggestAddress finds an acceptable full-string result for all 192, and ReverseGeocode returns the country, a place of the address and the house at all 192 points and a house 25 m away at all 192. One critical result is triaged and labelled in `si.labels.jsonl`. The 17 municipal failures (parser/matching; the raw lookup has an acceptable candidate for 15):

- **A settlement numbered by its own name, in another municipality (12):** libpostal reads `Postaja 19a, Tolmin` as a venue or road "postaja" and the city "tolmin". The house is filed under its settlement, not the municipality, so 9 answer the municipality's town (too_coarse, 2–15 km), `Cesta 18, Trebnje` answers `Rimska cesta 18` in Trebnje (near_miss, 10 km), `Prelesje 3, Šentrupert` the village Šentrupert in Braslovče (near_miss, 32 km), and the labelled far_miss `Mala Varnica 22, Videm` answers house 22 of the village Videm in Dobrepolje, 108 km away, goodMatch true: Videm is also a municipality, the one the query means.
- **Street, settlement and municipality (4):** libpostal joins the settlement and municipality into one city ("vrtojba šempeter-vrtojba", "petišovci lendava"), takes the settlement for the road (`Vidova pot 24, Bizeljsko, Brežice` is read as road "bizeljsko") or keeps only the first word of a hyphenated street ("Prade - cesta VIII" as "prade"); each answers a place.
- **Street in a municipality's seat (1):** `Vavpča vas 50, Semič` is read as venue "vavpča" and road "vas".

**Other samples.** Four more samples drawn with other seeds (`si-check-a` to `si-check-d`, 384 locations, 768 queries, none in the corpus, not committed) pass all 384 postal-form queries and 305 of 384 municipal ones, without a critical result (71 too_coarse, 5 abstain, 3 near_miss). Over both, the municipal form passes 185 of 258 settlement-numbered houses outside their municipality's seat, 6 of 6 in it, 39 of 56 street houses outside the seat and 154 of 160 in it; four of the six misses in a seat are Ljubljana streets that answer the city (`Komacova ulica 4c, Ljubljana`), not traced further. These are diagnostic results on a local index, not a CI gold run.
