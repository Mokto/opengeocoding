# Germany

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/de.NOTICE)

**Germany** (`de.seeds.jsonl`, `de.jsonl`): 192 Berlin addresses drawn from the house-coordinate subset of Adressen Berlin, extract date 2026-03-23 (© Amt für Statistik Berlin-Brandenburg and Senatsverwaltung für Stadtentwicklung, Bauen und Wohnen Berlin, dl-de/zero-2.0). There is no open national house-coordinate register. The national HK-DE product is distributed by the Zentrale Stelle Hauskoordinaten und Hausumringe (ZSHH). This corpus is Berlin only. It is not a national sample.

dl-de/zero-2.0 permits any use without restrictions or conditions, including copying, editing and republishing a sample. Attribution is not required; `de.NOTICE` names the publishers anyway.

## What other states publish

Checked from the publisher in this change:

| State | Open house coordinates | Licence | In this corpus |
|---|---|---|---|
| Berlin | Hauskoordinaten text file, with postcode. Pinned below. | dl-de/zero-2.0 | yes |
| Nordrhein-Westfalen | [Gebäudereferenzen](https://www.bezreg-koeln.nrw.de/geobasis-nrw/produkte-und-dienste/liegenschaftskataster/aktuelles-liegenschaftskataster/gebaeudereferenzen): street, house number, municipality and an ETRS89/UTM32 point. The publisher says this file has no postal fields, and that postal Hauskoordinaten (HK-DE) come from ZSHH. The open ASCII zip listed on 2026-10-21 was `gebref_EPSG25832_ASCII.zip`, 97,015,231 bytes, directory timestamp 2026-07-28. The OGC API collection then reported 4,511,573 features with data stand 2026-09-01. | dl-de/zero-2.0 | no |
| Sachsen | [Statewide CSV download](https://www.geodaten.sachsen.de/downloadbereich-hauskoordinaten-4172.html) (`hk_sn_ascii.zip`, about 20 MB, quarterly). The download page does not state a licence. | not confirmed here | no |
| Brandenburg | Gazetteer WFS includes Hauskoordinaten, updated monthly. | dl-de/by-2-0 | no |
| Thüringen | The [catalogue record](https://geomis.geoportal-th.de/geonetwork/srv/api/records/666deaca-27ec-4fdf-91b0-253bff94b738) describes Hauskoordinaten and says the postcode is not filled yet. The download page itself was not retrieved. | dl-de/by-2-0 on that record | no |
| Sachsen-Anhalt | The [open-data page](https://www.lvermgeo.sachsen-anhalt.de/de/gdp-open-data.html) publishes Gebäudereferenzen (June 2026, about 16.6 MB) and says they have no postal address, unlike Amtliche Hauskoordinaten. | not confirmed here | no |

Hamburg, Bremen, Hessen, Niedersachsen, Rheinland-Pfalz, Saarland, Schleswig-Holstein, Baden-Württemberg, Bayern and Mecklenburg-Vorpommern were not opened to a dated file and a licence statement in this change.

## Berlin extract

The pinned file is [HKO_EPSG25833.zip](https://gdi.berlin.de/data/adressen_berlin/atom/HKO_EPSG25833.zip) from the [ATOM feed](https://gdi.berlin.de/data/adressen_berlin/atom/0.atom). HTTP `Last-Modified` was Wed, 25 Mar 2026 08:32:06 GMT. The zip is 10,034,237 bytes, SHA-256 `461e2e832e7ad5e8e4cd3e5536538dea9f5e704efa939aefb581e128a2953aec`. Its member `adressen-be_mitPLZ.txt` is 57,917,875 bytes, SHA-256 `39e723b9b7c5b6cf3c1e4943f6b0df868c034737db358887530e1c867a5e30ab`, zip timestamp 2026-03-24 12:57, and `info-be.txt` says 394,933 coordinates, extract date 2026-03-23, EPSG:25833. The portal's stated coverage through 2026-06-30 does not match that extract date. The pin is the file.

The format description distributed beside it is ZSHH's Hauskoordinaten Deutschland v5.2 (Stand 2025-05-02), 212,153 bytes, SHA-256 `095b8823687b1ed4745c260aa419ffa24846bca8cc849d13aabcd9aa655e2e3c`. Berlin's file follows that layout with three differences: there is no header, the record-kind field is empty, and the coordinates are UTM zone 33 rather than the description's zone 32. A trailing field holds the extract date. Quality `A` is an official house number whose point is inside the recorded building outline (373,048 rows). Quality `B` is an official house number whose point is inside the parcel when a building is not certain (21,885 rows). This extract has no quality `C`. Additions are empty or one letter. The three postal-name fields are empty; the postcode is filled on all but 3 rows. Eleven further rows in Treptow-Köpenick carry a 155xx postcode. Those 14 rows are excluded, so a query does not pair "Berlin" with a missing or Brandenburg postcode. The eligible frame is 394,919 addresses. This is not a survey-grade entrance check.

Prepare a WGS84 CSV with DuckDB spatial (1.5.6 for this sample), offline, not a service dependency. The SQL reads `adressen-be_mitPLZ.txt` in the current directory:

```sh
REPO="$PWD"
mkdir -p ../berlin-hko
(
  cd ../berlin-hko
  curl -fLO https://gdi.berlin.de/data/adressen_berlin/atom/HKO_EPSG25833.zip
  unzip HKO_EPSG25833.zip
  duckdb -f "$REPO/tests/registry/berlin-csv.sql"
)
target/release/evaluate sample-de --berlin ../berlin-hko/berlin-addresses-4326-2026-03-23.csv --seed de-2026-10 --out tests/registry/de.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/de.seeds.jsonl --out tests/registry/de.jsonl --berlin ../berlin-hko/berlin-addresses-4326-2026-03-23.csv
target/release/evaluate run --corpus tests/registry/de.jsonl --index out --out eval-results/de
```

The prepared CSV is 40,680,435 bytes, SHA-256 `2f8502c3b585e34c1c68128d74d8e4c348cfdd0201994496932f89cfa6e01e36`. `build-gold --berlin` re-reads every sampled id and rejects a missing or duplicate id. Each case records the prepared filename, checksum, coordinate conversion, borough, locality part, quality and registry link. No geocoder selects or verifies an expectation.

`sample-de` keeps, in each of the 12 boroughs and in each number class (plain, or a one-letter addition), the addresses with the smallest seeded SHA-256 of the registry id, then takes up to eight different street keys. The draw has 96 plain numbers and 96 letters, 16 addresses in each borough, of which 181 are quality A and 11 are quality B. Boroughs and number classes are weighted equally, not by address count or population. Of the 192 locations, 47 are held out by the shared location-hash rule. The query is the street, the house number with the register's letter attached, the postcode and "Berlin, Germany". Expectations require country `de`, that house number and postcode, the source point within 150 m, and goodMatch true. Borough names are sampling tags, not ISO region codes, so these cases do not check ResolveRegion.

The builder does not read this file. German addresses in the index come from Overture, which carries OpenStreetMap, and from OpenAddresses layers Overture does not already include. The only German source in `openaddresses_reviewed.tsv` is Cologne's city address file under the same zero licence, not this extract. Where OpenStreetMap copied the official point, a pass measures registry fidelity rather than an independent survey.

One local run (`evaluate run --corpus tests/registry/de.jsonl --index ~/opengeocoding-index/out --repeat 1`, release binary, libpostal models at `/opt/homebrew/share/libpostal`) scored this corpus on the index whose `meta.json` fingerprint is `210076085b5e`. That directory has no `build.json`. `builder.hash.txt` records `1929034e0fce080d`; this tree's `build_index hash` is `e8c4a1473b15ae4a`. The corpus does not change the builder hash. These figures are a development observation on that index, not a production-accuracy claim.

A later run of the same index, after lookup treats a glued one-letter addition as the spaced key OpenStreetMap stores, passes SearchAddress **192/192**, including the held-out locations. `Straße 47 7A` is house 7A of Straße 47. The four outer plain houses (`Hamburger Straße 29`, `Schubertstraße 37`, `Florastraße 97`, `Dahmestraße 20`) are confident: the stated city is the state, Berlin, while the house is filed under its borough. Full-string suggestions pass **96/96** letter queries and **95/96** plain queries. The plain miss is still `Dahmestraße 20`, which suggests the street. ReverseGeocode passes the country check at all 192 source points and does not check the house. ResolveRegion is unscored. There are no critical results and no `de.labels.jsonl`. Expectations are unchanged. grpcurl's documented SearchAddress hang reproduced.
