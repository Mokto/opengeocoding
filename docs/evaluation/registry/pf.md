# French Polynesia

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/pf.NOTICE)

**French Polynesia** (`pf.seeds.jsonl`, `pf.jsonl`): 120 locations in the two communes with addressed streets, Papeete and Tumaraa, one query each, drawn from the **Base Adresse Nationale** (BAN) file for French Polynesia, `adresses-987.csv.gz`. The national export that `fr.md` samples leaves the collectivities out. The Gouvernement de la Polynésie française republishes the same addresses as [*Adressage en Polynésie française*](https://services.arcgis.com/L4H4rU26VyhEF588/arcgis/rest/services/Adressage_en_Polyn%C3%A9sie_fran%C3%A7aise_WFL1/FeatureServer/0) ("issu de la Base adresse nationale"), with BAN's columns, so the corpus reads BAN's dated file rather than the live layer.

## Licence

BAN is published under the Licence Ouverte / Open Licence 2.0 (`etalab-2.0`), as is the government's republication. `pf.NOTICE` gives the attribution and the changes made.

## Capture

```sh
mkdir -p ../pf-data
curl -fL -o ../pf-data/adresses-987-2026-10-30.csv.gz https://adresse.data.gouv.fr/data/ban/adresses/latest/csv/adresses-987.csv.gz
target/release/evaluate sample-pf --ban-pf ../pf-data/adresses-987-2026-10-30.csv.gz --seed pf-2026-10 --out tests/registry/pf.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/pf.seeds.jsonl --out tests/registry/pf.jsonl --ban-pf ../pf-data/adresses-987-2026-10-30.csv.gz
target/release/evaluate run --corpus tests/registry/pf.jsonl --index out --out eval-results/pf
```

The file downloaded on 2026-10-31 (Last-Modified 2026-10-30 03:14:59 UTC) is 256,729 bytes, SHA-256 `a80837343b1fadc8957210bd855ef2dfd353020947604f3908473d9cb9e213e3`, with 7,717 rows. The URL moves with each BAN release, so keep the pinned file. Sampling and corpus generation repeated byte for byte. The rows are read by `ban.rs`, as for `fr.md`: the number joins `numero` and `rep` with a space (`12 bis`), the point is BAN's WGS84 `lat` and `lon`, and the tolerance is 150 m, or 300 m for a parcel point.

## Sample

The file covers three of French Polynesia's 48 communes:

- **Papeete** (INSEE 98735, postcode 98714): 5,237 entrance points, none certified by the commune.
- **Tumaraa** (INSEE 98754, postcode 98735): 1,522 points, all certified.
- **Pirae** (INSEE 98736): 958 numbers with no street, postcode or position type; these are not usable as addresses.

A row is eligible when it has a street and a postcode, an entrance, building or parcel position (7 Tumaraa rows placed by the technical service are left out), and no other row has the same street, number and postcode (2 rows are left out). Certification is not required, unlike in `fr.md`, because none of Papeete's points is certified. That leaves 6,750 addresses: 5,235 in Papeete (429 with a suffix) and 1,515 in Tumaraa (65 with a suffix).

The draw has one pool per commune and class: **street** (a plain number) or **suffix** (`bis`, `ter`, `a`–`e`). In each pool it takes, by the seeded SHA-256 of the BAN id, the first addresses on different streets: 48 street and 12 suffix addresses per commune, so 96 `pf_street` and 24 `pf_suffix` locations. 117 points are entrances, 2 parcels and 1 a building. Both communes weigh the same. The sample covers two communes of Tahiti and Raiatea, not the territory: it is not a representative accuracy sample of French Polynesia. 19 locations are held out by the shared location-hash rule.

Each location has one query, from BAN's own fields in BAN's order: number, street, postcode, commune and country: `3 bis Arati’a Paetaha - Rairoa 2, 98735 Tumaraa, French Polynesia`. Street names keep BAN's forms, including the road number of Tumaraa's ring road (`Aroa Tevaitoa (rt 130)`) and numbered lanes (`Arati’a Tenape 7`). Each query requires goodMatch true at house level within the tolerance, with BAN's number, street and postcode. The country may be `pf` or `fr`, as for the overseas departments in `fr.md`. No region is expected. Each case's `places` is BAN's `nom_commune`.

**Index ancestry.** Overture release 2026-09-23.1 has 2,000 French Polynesian addresses, all from OpenAddresses' `pf/countrywide`. That count is the republished layer's page size, so Overture holds one page of it: 478 street-less Pirae points and 1,522 Tumaraa addresses (1,466 with BAN's street and number as written), and none of Papeete's. 53 of the 60 sampled Tumaraa addresses are among them, and none of the Papeete ones. The local index also reads the OpenStreetMap extract of French Polynesia, and Papeete's answers come from it. A Tumaraa pass is largely fidelity to BAN. A Papeete pass compares BAN with another source, which is closer to an accuracy measure, although the OSM data may itself derive from BAN.

## Local baseline

On a local full index from builder `74654cb597c8d104` (Overture 2026-09-23.1, OpenAddresses and OSM), SearchAddress passes 78 of 120 queries: 56 of 60 in Papeete and 22 of 60 in Tumaraa; 61 of 96 street and 17 of 24 suffix addresses; 15 of 19 held out. 41 queries answer too coarsely and 1 is a near miss; there is no critical result, and nothing is labelled. SuggestAddress finds an acceptable full-string result for 112, and ReverseGeocode passes at 112 points, the same at 25 m. The failures:

- **Tumaraa as the town (matching; 38 queries):** with the commune written, the resolver answers the commune itself (`Tumara’a, Tevaitoa, Tumaraa`, locality level, 1.6 km to 10.3 km from the point), with goodMatch false for 34 and, wrongly, true for 4. Overture holds 34 of these 38 houses with the village as their place and no postcode. Asked again as a diagnostic, not a case, without postcode and commune (`5 Arati’a Tenape 7, French Polynesia`), 24 of the 38 find the house, so the commune written is what rejects them. 25 of the 38 are on lanes whose name ends in a number (`Arati’a Tenape 7`); none of those passes with the commune written.
- **Papeete streets (data; 4 queries):** `24 Te aroa 5 no mati 1797` answers the postcode 1.2 km away; `52 Servitude Tepihaa piti` answers `Servitude Tepihaa 1` (*piti* is Tahitian for two); `4 Servitude Richmond` answers another lane in Pīraʻe, 1.2 km away; and `59 Servitude Pékin` is a near miss, `Servitude Papeete Nui`, 2.3 km away.
- **Papeete houses filed under Pīraʻe (data; 8 points):** SearchAddress finds these houses, but ReverseGeocode names Pīraʻe, and SuggestAddress, typed with Papeete, misses them. Overture 2026-09-23.1 has an area polygon for Pīraʻe but none for Papeete, so the index names the nearest locality point.

These are diagnostic results on a local index, not a CI gold run. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure.
