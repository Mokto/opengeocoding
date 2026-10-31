# Liechtenstein

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/li.NOTICE)

**Liechtenstein** (`li.seeds.jsonl`, `li.jsonl`): 132 locations in all 11 municipalities, one query each, drawn from the **Offizielles Verzeichnis der Gebäudeadressen**, the official directory of building addresses of the Liechtensteinische Landesverwaltung, kept by the Amt für Tiefbau und Geoinformation ([opendata.swiss](https://opendata.swiss/de/dataset/offizielles-verzeichnis-der-gebaudeadressen-liechtenstein), [geocat.ch](https://www.geocat.ch/datahub/dataset/50b86c0b-242c-4be0-9bad-659aed6fe147)). It lists every official building address: the federal identifiers (EGAID, EGID, ESID), street, house number, municipality number, postcode and postal locality, status, and the address point in LV95 and WGS84. It is updated continuously.

## Licence

The download encloses the Landesverwaltung's *Nutzungsbedingungen für die offiziellen Verzeichnisse der Strassen, der Gebäudeadressen und der Ortschaften* (`AbiOGDLizenz.pdf`, ABI/OGD-Version 2 vom 5.2.2021; read on 2026-10-31), and opendata.swiss lists the dataset as "open use, must provide the source". The data may be copied, passed on free of charge, enriched, processed, combined and used commercially. Every redistribution must enclose or link the terms, and every publication must state the source "Liechtensteinische Landesverwaltung". `li.NOTICE` gives the attribution and the changes made and links the terms. The corpus records them as `LLV-ABI-OGD-2`. This is a reading of the published terms, not a grant from the Landesverwaltung.

## Capture

```sh
mkdir -p ../li-data
curl -fL -o ../li-data/oia-2026-08-18.zip 'https://service.geo.llv.li/download/getfile.php?theme=officialAdress'
unzip -p ../li-data/oia-2026-08-18.zip dataset.csv > ../li-data/oia-2026-08-18.csv
target/release/evaluate sample-li --llv ../li-data/oia-2026-08-18.csv --seed li-2026-10 --out tests/registry/li.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/li.seeds.jsonl --out tests/registry/li.jsonl --llv ../li-data/oia-2026-08-18.csv
target/release/evaluate run --corpus tests/registry/li.jsonl --index out --out eval-results/li
```

The download of 2026-10-31 (`oia.zip`) is 1,306,445 bytes, SHA-256 `ea8592ccc2ef85a7db73473e5a21ac62be5a3b39bba6f16dcea597636983bcea`. Its `dataset.csv` is dated 2026-08-18, with the latest `MODIFIED` on 2026-08-10. It is 2,049,756 bytes, SHA-256 `744f46b8ad22b08d4d5ef1bb554d6f627fabd21b890801aca10961fe19712edc`, and has 12,966 rows. The URL always serves the current directory, so keep the pinned files. The CSV is ISO-8859-1 (the shapefile's `.cpg` says 1252; the file has no byte that the two decode differently), and the adapter rejects any byte 0x80–0x9F. Sampling and corpus generation repeated byte for byte.

**Coordinates.** `WGS84_N` and `WGS84_E` are WGS84 latitude and longitude to six decimals, and `EAST` and `NORTH` are the same point in LV95 (EPSG:2056). The adapter takes the WGS84 pair as published and rejects a point outside 47.0–47.3° N, 9.4–9.7° E, so swapped columns fail loudly. PROJ's EPSG:2056-to-EPSG:4326 transformation of the LV95 pair agrees with it to within 7 cm at all 12,966 points. The directory does not say whether a point marks an entrance or the building, so no precision beyond the 150 m tolerance is claimed.

## Sample

An address is eligible when it is `real`, `OFFICIAL` and `RELIABLE`, with a number of one to four digits without a leading zero and at most one lowercase letter, a street and a four-digit postcode, and no other eligible address has the same street, number and postcode. That leaves out 233 planned addresses and 40 marked unofficial or unreliable. Every remaining row is eligible: 12,693 addresses, 11,951 with a plain number and 742 with a letter (`42b`).

The draw has one pool per municipality (`COM_FOSNR`) and class: **street** (a plain number) or **letter**. In each pool it takes, by the seeded SHA-256 of `ADR_EGAID`, the first addresses on different streets: 8 street and 4 letter addresses per municipality, so 88 `li_street` and 44 `li_letter` locations. Every municipality weighs the same: Planken, with 214 addresses, has as many locations as Schaan, with 1,749. The sample is therefore not a representative national accuracy sample. 29 locations are held out by the shared location-hash rule.

Each location has one query: street, number, postcode, postal locality and country, as the directory writes them: `Palduinstrasse 49a, 9496 Balzers, Liechtenstein`. It requires goodMatch true at house level, within 150 m of the directory point, with the directory's number (`49a`), street and postcode.

The directory has municipality numbers but no municipality names. The adapter gives each number its name and ISO 3166-2 code from a fixed table of the eleven: Vaduz 7001 (LI-11), Triesen 7002 (LI-09), Balzers 7003 (LI-01), Triesenberg 7004 (LI-10), Schaan 7005 (LI-07), Planken 7006 (LI-05), Eschen 7007 (LI-02), Mauren 7008 (LI-04), Gamprin 7009 (LI-03), Ruggell 7010 (LI-06) and Schellenberg 7011 (LI-08). The numbers and names are those of swisstopo's official locality directory (`AMTOVZ_CSV_LV95`, 2026-09-30), and the codes those of ISO 3166-2:LI. An unknown number fails the run. The municipality's code is the region that SearchAddress, ReverseGeocode and ResolveRegion must return. Each case's `places`, the names ReverseGeocode's answer must include one of, are the postal locality and the municipality. They differ for 18 locations: Nendeln and Schaanwald are villages of Eschen and Mauren, and the postal locality Gamprin-Bendern also covers houses in Eschen.

**Index ancestry.** Overture release 2026-09-23.1 has 12,966 Liechtenstein addresses, all from OpenAddresses' `li/countrywide`, which reads this directory. All 12,966 rows of this extract match one of them by street, number and postcode within 1 m. Overture keeps the letter in the number (`42b`). The local index reads no OpenStreetMap extract and no OpenAddresses file for Liechtenstein. The sample is selected independently of the index, but a pass measures fidelity to the directory, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index from builder `0805a94fbb0deede` (Overture 2026-09-23.1, OpenAddresses and OSM), SearchAddress passes 131 of 132 queries, all 44 letter and 87 of 88 street addresses, and all 29 held-out queries. SuggestAddress finds an acceptable full-string result for all 132. ReverseGeocode at the point and 25 m away, and ResolveRegion, pass at 131 of 132.

The one failure, `Eschner Strasse 40, 9487 Gamprin-Bendern`, finds the house at its point with goodMatch true, but in Gamprin (LI-03). The directory files it under Eschen (LI-02), and so does Overture's Eschen polygon, 7 m inside its border with Gamprin. The index answers regions from polygons simplified to 0.0005° (`src/index/area.rs`), about 38 m east–west here, so a house this near a border can be placed in its neighbour. This is a reverse-region limitation of the index (`wrong_region` in all three checks), not a parse or matching error, and it is not labelled.

These are diagnostic results on a local index, not a CI gold run. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure.
