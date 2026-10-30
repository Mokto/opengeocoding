# Austria

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/at.NOTICE)

**Austria** (`at.seeds.jsonl`, `at.jsonl`): 78 locations in 78 localities of 71 of the 2,092 municipalities and all nine federal states, 156 queries, drawn from the **Austrian address register** (Österreichisches Adressregister) of the Federal Office of Metrology and Surveying (Bundesamt für Eich- und Vermessungswesen, BEV), as published in its [relational tables of reference-date data](https://www.bev.gv.at/Services/Downloads/Produktbezogene-Downloads/Unentgeltliche-Produkte/Adressregister.html) ("Adresse Relationale Tabellen - Stichtagsdaten", interface description V1.5 of 13.04.2022): one row per address in `ADRESSE.csv`, with the municipality (`GKZ`), locality (Ortschaft, `OKZ`), street (`SKZ`), postcode, house-number parts and access coordinate, and the names in `GEMEINDE.csv`, `ORTSCHAFT.csv` and `STRASSE.csv`. Municipalities keep the register; BEV publishes a snapshot on 1 April and 1 October.

## Licence

The BEV download page states that the Creative Commons Attribution 4.0 licence (CC BY 4.0) applies to "Adressregister Relationale Tabelle Stichtagsdaten". The archive's own `Licensing_Conditions.txt` (862 bytes, SHA-256 `eb948ff734e038fa5a2c0f525b11aec01aa4b27d9d1d01a38d67e9b896437540`; German `Nutzungsbedingungen.txt`, 925 bytes, SHA-256 `30ec2d0dbc1da9f8866a3850cd4c4674ad5ae73bfb55b3ca0ab2cd558feef043`) grants an unlimited right of use on condition that every release and follow-up product quotes "© Österreichisches Adressregister" and the reference date of the extract. Both files are dated June 2026 and still name the reference date 01.04.2026; `at.NOTICE` and every case's `source.attribution` give the reference date of the data used, 01.10.2026. Overture records CC-BY-4.0 for the same register (below). The corpus records the licence as `CC-BY-4.0`.

## Capture

`https://data.bev.gv.at/download/Adressregister/Adresse_Relationale_Tabellen_Stichtagsdaten.zip` was downloaded on 2026-10-29 (`Last-Modified` 2026-10-12): 118,545,233 bytes, SHA-256 `3696f8de70d69847d9f305330500f45836cea3e59884693bdd52fa6220f8f0e1`. Its `Aktualitaetsstand.txt` (186 bytes, SHA-256 `06232d6ec226f98495193d3e1eea447880e6e16e8f48c7fd65711480cbf3d88a`, Latin-1) gives the reference date 01.10.2026. The four tables the adapter reads are UTF-8 with a byte-order mark, separated by semicolons, text in double quotes (doubled inside), CRLF line ends:

| File | Bytes | SHA-256 |
|---|---:|---|
| `ADRESSE.csv` | 327,548,296 | `216691fa084dbb51464740a67b1e053bcc06abe820ff829ff7261cc80c8ffa3a` |
| `STRASSE.csv` | 8,404,183 | `3956ce7f6bf52bbac2d87cba1aaeeb479328040177533be41fd47747e24258e4` |
| `ORTSCHAFT.csv` | 507,160 | `7865294451e6292e02cc943a498f3d325ba61dcf1e6afb25d5f51f71025562f7` |
| `GEMEINDE.csv` | 51,595 | `b4dc3a5ac6a4dc16fbd32a1ae0dcab3f72d2607cd9426b057b7b156e03fe12b0` |

That address always serves the latest snapshot; BEV moves earlier ones to `https://data.bev.gv.at/download/Adressregister/Archiv_Adressregister/Adresse_Relationale_Tabellen_Stichtagsdaten_YYYYMMDD.zip`, where this one is expected after the next. `--bev` takes `ADRESSE.csv` and reads the other three tables and `Aktualitaetsstand.txt` from its directory; the release recorded in every case is named after the reference date (`Adresse_Relationale_Tabellen_Stichtagsdaten_20261001`), with the checksum of `ADRESSE.csv` in `sha256` and those of the three name tables in `tables_sha256`:

```sh
mkdir -p ../at-data
curl -fL -o ../at-data/Adresse_Relationale_Tabellen_Stichtagsdaten_20261001.zip https://data.bev.gv.at/download/Adressregister/Adresse_Relationale_Tabellen_Stichtagsdaten.zip
unzip -d ../at-data ../at-data/Adresse_Relationale_Tabellen_Stichtagsdaten_20261001.zip ADRESSE.csv STRASSE.csv ORTSCHAFT.csv GEMEINDE.csv Aktualitaetsstand.txt
target/release/evaluate sample-at --bev ../at-data/ADRESSE.csv --seed at-2026-10 --out tests/registry/at.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/at.seeds.jsonl --out tests/registry/at.jsonl --bev ../at-data/ADRESSE.csv
target/release/evaluate run --corpus tests/registry/at.jsonl --index out --out eval-results/at
```

**Coordinates.** `RW` and `HW` are an easting and a northing in the Gauss-Krüger zone the row's `EPSG` names: MGI / Austria GK West, Central or East (EPSG:31254, 31255, 31256; central meridian 10°20′, 13°20′ or 16°20′ E, false northing −5,000 km). Since December 2016 the register places an address at its access coordinate (Zugangskoordinate): on the parcel, as close as possible to the entrance or driveway (Adressregisterverordnung 2016, BGBl. II Nr. 51/2016). `QUELLADRESSE` says where the point lies: in a building of the cadastral map (B), in the parcel (G), in a building missing from the map (I), in the parcel as the municipality states (A), or on or within 1 m of the parcel boundary (P); of the 78 sampled locations 51 are G, 24 P, 2 B and 1 I. The adapter converts them with the inverse transverse Mercator on Bessel 1841 and EPSG:1618 "MGI to WGS 84 (3)", the 7-parameter position-vector Helmert transformation (1.5 m) PROJ applies across Austria, which matches PROJ's pipeline to 1e-8° (`mgi_gk_matches_proj`). It does not use BEV's grid (AT_GIS_GRID), which is more precise; no precision beyond the 150 m tolerance is claimed. It rejects an `EPSG` outside the three zones, an `RW` beyond ±150 km or an `HW` outside 130–450 km, so swapped columns fail loudly. Sampling and corpus generation each repeated byte for byte.

## Sample

`ADRESSE.csv` has 2,529,222 addresses, each `ADRCD` once, all with a point. An address is eligible when it is one number (`HAUSNRZAHL1`, without leading zero) and at most one letter (`HAUSNRBUCHSTABE1`, either case), which `HNR_ADR_ZUSAMMEN` writes as the same number; that leaves out 17,553 parcel-number addresses (`GNRADRESSE`), 10,927 ranges ("12-14"), staircase, object or parcel parts ("Stg.", "Obj.", "Parz.") and numbers with a text before them ("bei"). It must also have a four-digit postcode and a street without a name suffix (`STRASSENNAMENZUSATZ`, such as "FS I"; 86,279 addresses). `sample-at` reads 2,416,100 eligible addresses. Its class is:

- **street**: a number without letter on a named street ("Muldenweg 22"): 1,712,331.
- **letter**: a number with a letter ("Göschlgasse 16a", "Siebensterngasse 16A", "Grins 51c"): 213,888.
- **village**: a number without letter whose street is named like its locality, where the locality numbers its houses itself ("Reinsbach 21"): 489,881.

In each federal state (the first digit of `GKZ`) and class, `sample-at` keeps the 200 rows with the smallest seeded SHA-256 of their `ADRCD`, then takes the first from different localities: four street, two letter and three village addresses per state, 36, 18 and 24 locations. Wien has no village addresses. The sample weights states and classes, not population or address counts: Niederösterreich, with 27% of the addresses, has nine locations and Wien six. It is not a representative national accuracy sample. Of the 78 locations, 23 (46 cases) are held out by the shared location-hash rule.

Each location has two queries:

- **street**, **letter** and **village**: the register's postal form, the house then postcode and delivery place (`ZUSTELLORT`), which the register keeps for each street and often abbreviates as Austrian addresses do: `Langegasse 10e, 7061 Trausdorf/Wulka, Austria`, `Bahnhofstraße 45b, 4910 Ried/Innkreis, Austria`, `Reinsbach 21, 3804 Allentsteig, Austria`.
- **municipal**: no postcode; the locality (`ORTSNAME`) if it differs from the municipality and the street is not named after it, then the municipality (`GEMEINDENAME`), each in full: `Muldenweg 22, Holzleiten, Hörsching, Austria`, `Langegasse 10e, Trausdorf an der Wulka, Austria`, `Reinsbach 21, Allentsteig, Austria`. A village that is its own municipality is named once (`Raggal 234, Austria`). Wien, Graz and Klagenfurt name their localities after a district ("Wien,Donaustadt", "Graz,08.Bez.:Sankt Peter"), which the query leaves out: `Aspernstraße 119, Wien, Austria`.

Every case requires goodMatch true at house level within 150 m of the register point, with the register's number and letter and postcode and, unless the street is named after the locality, the street. State names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point. Each case's `places`, the names ReverseGeocode's answer must include one of, are the locality (a district's name alone in Wien, Graz and Klagenfurt) and the municipality.

**Index ancestry.** Overture release 2026-10-21.0 has 2,523,030 Austrian addresses, all from OpenAddresses' `at/countrywide` (updated 2026-10-05, CC-BY-4.0). 2,521,994 of them have a record id that is an `ADRCD` of this file; of those, 2,521,428 have its `HNR_ADR_ZUSAMMEN` as their number, 2,521,259 its street name and 2,521,922 its postcode, and against the points converted as above the median distance is 0.64 m, 96% lie within 1 m, 99.7% within 5 m and 373 farther than 150 m. The builder also reads OpenAddresses' 2021 BEV layers (`at/31254`–`31256`, reference date 2021-04-01) and Tirol's state layer, and skips Vienna's (no licence it recognises); the chart reads the Austria OpenStreetMap extract for landmarks only. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index from builder `aafd258112e50fe9` (Overture 2026-10-21.0, OpenAddresses and the chart's OSM extracts), SearchAddress passes all 156 queries, the 46 held-out cases included. A same-binary repeat has zero response drift. Nothing is labelled. The other checks fail at seven locations, all in the three cities with districts; none is a SearchAddress failure:

- **ReverseGeocode, wrong place (5 locations, 10 cases):** four houses in Wien are answered with the right house, whose places are named "Vienna", an exonym the place check cannot tell from a wrong answer (see the evaluation guide); `Breitenweg 18f, 8042 Graz` (Sankt Peter) is answered with the right house under Tiefental, Bezirk Graz-Umgebung, a locality outside the city.
- **ReverseGeocode, empty, and SuggestAddress, missing (2 locations, 4 cases each):** `Schönbrunner Allee 50A` and `Aspernstraße 119` in Wien are found as houses without country or locality (`50a, Schönbrunner Allee, Austria`), so ReverseGeocode's first result has no country and SuggestAddress's has none to match; SearchAddress takes the country from the query.

ReverseGeocode passes 142 of 156 at the points and 142 of 156 25 m away; SuggestAddress finds an acceptable full-string result for 152 of 156. These are diagnostic results on a local index, not a CI gold run.
