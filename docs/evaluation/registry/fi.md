# Finland

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/fi.NOTICE)

**Finland** (`fi.seeds.jsonl`, `fi.jsonl`): 152 locations in 127 of the 308 municipalities and all 19 regions, 169 queries, drawn from the building addresses of **Ryhti**, the Built Environment Information System (Rakennetun ympäristön tietojärjestelmä) of the Finnish Environment Institute (Suomen ympäristökeskus, Syke), as published in its dataset [Rakennetun ympäristön tietojärjestelmän rakennusten osoitetiedot](https://avoindata.suomi.fi/data/fi/dataset/rakennetun-ympariston-tietojarjestelman-rakennusten-osoitetiedot): one row per address and building, with the Finnish and Swedish street names, house number and letter, postcode, Finnish and Swedish post office, municipality number and the building's point.

## Why Ryhti

The issue named DVV's *Rakennusten osoitetiedot – koko Suomi* (building addresses from the Population Information System). DVV announced that its distribution would end on 14 March 2025, and the national catalogue no longer lists it. Ryhti took over those addresses: its dataset page says that building addresses created before Ryhti were brought over from the Population Information System, and that municipalities may still report through it until 2028. OpenAddresses' `fi/countrywide-fi` and `fi/countrywide-sv` now read this layer; its regional sources, which read the DVV files, are retired.

## Licence

The catalogue entry gives the licence as CC BY 4.0 for the open part of the data and the credit as "Lähde: Syke Ryhti"; Syke's [use licence](https://www.syke.fi/en/environmental-data/use-license-and-responsibilities) licenses its open data under CC BY 4.0, provided the licensor and dataset are named. The catalogue records no access constraints. `fi.NOTICE` gives that credit, links the licence and says what was changed. The municipality and region names come from Statistics Finland's classification service, whose open data is CC BY 4.0 ([terms](https://stat.fi/en/about-us/get-to-know-statistics-finland/legislation/terms-of-use)), credited in `fi.NOTICE` too. The corpus records the licence as `CC-BY-4.0`.

## Capture and preparation

Ryhti publishes no file, only OGC API Features, WFS and WMS services. The WFS layer `ryhti_building:open_address` serves at most 100,000 features per request, so the capture pages on the address key; `srsName=CRS:84` has the server write WGS84 longitude and latitude instead of the stored EPSG:3067 metres. On 2026-10-28 it returned 3,867,619 rows (the count the OGC API reported), one per address key: `ryhti-open-address-2026-10-28.csv`, 1,059,221,651 bytes, SHA-256 `b98a1d87e22d27211e95a9139d3b066235451326f033d899772997260a2e2c89`. The service serves the current data, so keep the pinned file. Statistics Finland's 2026 municipality-to-region key (`kunta-maakunta-2026.json`, 506,907 bytes, SHA-256 `9f77b9db693960d128426be506cb004bffea082b52b09dc7c637472ee8c29815`) and Swedish municipality names (`kunta-2026-sv.json`, 173,248 bytes, SHA-256 `51f0a15588cbc88a4e118a7b80d656fa35df5a693bd91a9d353d50b454fc4016`) name every municipality number in it. Prepare one CSV with DuckDB (2.0.0 for this sample), an offline curation tool, not a service dependency:

```sh
REPO="$PWD"
mkdir -p ../fi-data
(
  cd ../fi-data
  B='https://paikkatiedot.ymparisto.fi/geoserver/ryhti_building/wfs?service=WFS&version=2.0.0&request=GetFeature&typeNames=ryhti_building:open_address&outputFormat=csv&sortBy=address_key&count=100000&srsName=CRS:84'
  out=ryhti-open-address-2026-10-28.csv last=
  : > "$out"
  while :; do
    url=$B; [ -n "$last" ] && url="$B&cql_filter=address_key%3E'$last'"
    curl -fsS "$url" > page.csv
    [ -s "$out" ] || head -1 page.csv > "$out"
    [ "$(wc -l < page.csv)" -gt 1 ] || break
    tail -n +2 page.csv >> "$out"; last=$(tail -1 page.csv | cut -d, -f2)
  done
  rm page.csv
  C=https://data.stat.fi/api/classifications/v2
  curl -fsS -o kunta-maakunta-2026.json "$C/correspondenceTables/kunta_1_20260101%23maakunta_1_20260101/maps?content=data&meta=min&lang=fi"
  curl -fsS -o kunta-2026-sv.json "$C/classifications/kunta_1_20260101/classificationItems?content=data&meta=min&lang=sv"
  duckdb -f "$REPO/tests/registry/ryhti-csv.sql"
)
target/release/evaluate sample-fi --ryhti ../fi-data/ryhti-addresses-2026-10-28.csv.gz --seed fi-2026-10 --out tests/registry/fi.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/fi.seeds.jsonl --out tests/registry/fi.jsonl --ryhti ../fi-data/ryhti-addresses-2026-10-28.csv.gz
target/release/evaluate run --corpus tests/registry/fi.jsonl --index out --out eval-results/fi
```

The register has one row per building: an address on several buildings (a farmhouse and its outbuildings) has several rows and points. `tests/registry/ryhti-csv.sql` leaves out the 141,536 rows without an address and makes one row per address (municipality, Finnish and Swedish street, number and letter, second number and letter, postcode): the first building's key, point and post offices by address key, how many buildings it has and how far apart their points are, and the municipality's Finnish and Swedish names and region from Statistics Finland. The result, `ryhti-addresses-2026-10-28.csv.gz`, has 1,968,361 addresses, 140,238,786 bytes, SHA-256 `1ae2105da18b8a7e80bf096c9af1cc1f0c1271e13133f7cb0fe862a85c867ea2`; preparation, sampling and corpus generation each repeated byte for byte. The adapter rejects a point outside 59.5–70.2° N, 19.0–31.7° E, so swapped coordinates fail loudly. A point is a building's, not an entrance's, as the dataset page says; no precision beyond the 150 m tolerance is claimed.

## Sample

`sample-fi` reads 1,700,445 eligible addresses. It leaves out 69,231 without a house number (a named farm or island), 10,152 with a second number, 236 with a number of more than five digits or a leading zero, 187,902 whose buildings are more than 50 m apart, and 395 with a missing postcode, a letter that is not one of a–z, a comma in a name, or no post office for either language's name. A second number is a range ("Peurantie 7-9") or a number on a side road numbered from a main road ("Uusi Porvoontie 849-15"), which the register does not tell apart.

Each address is in one class: **plain**, a number alone ("Kurukatu 6"); **letter**, a number with a building letter ("Tellervonkatu 49a"). In each region and class, `sample-fi` keeps the 200 addresses with the smallest seeded SHA-256 of their address key, then takes the first from different municipalities: five plain and three letter addresses per region, 95 and 57 locations. The sample weights regions and classes, not population or address counts; it is not a representative national accuracy sample. Of the 152 locations, 34 (38 cases) are held out by the shared location-hash rule.

Queries follow Posti's address form, the post office in capitals as the register writes it:

- **plain** and **letter**: `Tellervonkatu 49a, 53100 LAPPEENRANTA, Finland`, in Finnish; an address the register names only in Swedish (seven in Åland, one in Ostrobothnia) is asked in Swedish: `Tärnegränd 5, 22100 MARIEHAMN, Finland`.
- **swedish**, for the 17 locations whose Swedish street or post office differs from the Finnish one: `Hagavägen 10a, 67700 KARLEBY, Finland`.

Every case requires goodMatch true at house level within 150 m of the register point, with the register's number and postcode. An Åland address (postcode 22xxx) also accepts the country code `ax`, as an overseas department accepts its own code in `fr.md`. Raw region names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point. Each case's `places`, the names ReverseGeocode's answer must include one of, are the municipality's Finnish and Swedish names and the address's Finnish and Swedish post offices.

**Index ancestry.** All 3,670,323 Finnish addresses in Overture release 2026-10-21.0 come from OpenAddresses' `fi/countrywide-fi`, and the builder also reads OpenAddresses' `fi/countrywide-sv`, which Overture does not carry, and the Finland OpenStreetMap extract; both OpenAddresses sources read this WFS layer. With `index.sources.ryhti`, the build Job also reads this layer's Swedish-only addresses directly, which those sources publish without a number (see the [indexing guide](../../guides/indexing.md)). Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index matching the chart's sources (Overture 2026-10-21.0, the chart's OpenStreetMap extracts, OpenAddresses, GeoNames and Ryhti's Swedish-only addresses) from this change's builder, SearchAddress passes 166 of 169: 93/95 plain, 56/57 letter and 17/17 Swedish queries, and 37 of 38 held-out cases. There is no critical result, so nothing is labelled. The three failures are one register inconsistency:

- **One address under two postcodes (3, register):** the register gives `Kenttäkangas 13` in Posio the postcode 97820 PERÄ-POSIO on one building and 97840 LEHTINIEMI on another 17 m away, and likewise `Seiväsrannantie 4b` in Suomussalmi (89600/89760, 10 m) and `Selkjärventie 131` in Hämeenlinna (16900/16970, 49 m). The sample drew the first postcode of each; the index keeps one copy of a house number within 300 m, and the service answers the house at the register point with the second (wrong_component). These were found after sampling and are kept rather than redrawn.

Without Ryhti's Swedish-only addresses, eight more fail: the seven Åland addresses the register names only in Swedish (`Tärnegränd 5, 22100 MARIEHAMN`) and `Ketovägen 9a, 66520 VEIKARS` answer the street or nothing. OpenAddresses' `fi/countrywide-fi` takes the street from `address_name_fin` and `fi/countrywide-sv` the number from `address_fin`, both empty for these rows (16,748 of Åland's 19,154 addresses), so neither gives them a numbered house. Reading them also changes ten checks of the OpenStreetMap panel in Åland (`tests/global/`), where the register and OpenStreetMap disagree: ReverseGeocode at OpenStreetMap's `1087 Sundsvägen` now answers the register's 1078, 6 m away, and likewise `2A Smedjegränd` (the register's 2, 1 m), `70 Ringvägen` (70b, 4 m) and `258 Pålsbölevägen` (284, 3 m; the register's 258 is 270 m away, the point SuggestAddress now answers).

SuggestAddress finds an acceptable full-string result for 166 of 169, missing the same three. ReverseGeocode returns the country and the house at 163 of 169 points and a house within the radius 25 m away at all 169: the misses are the three two-postcode addresses (wrong_postcode) and three neighbours nearer than the register's building (`Koivuniementie 150` answers 148). These are diagnostic results on a local index, not a CI gold run.
