# Lithuania

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/lt.NOTICE)

**Lithuania** (`lt.seeds.jsonl`, `lt.jsonl`): 100 locations in 96 settlements of 48 of the 60 municipalities and all 10 counties, 200 queries, drawn from the **Address Register** (Adresų registras) of the State Enterprise Centre of Registers (VĮ Registrų centras), as published in its [raw data](https://www.registrucentras.lt/atviri-duomenys-ir-statistika/adresu-registro-pirminiai-duomenys-raw-data) (Adresų registro pirminiai duomenys): one row per address of a land plot or building with its municipality, settlement and street codes, number, building (korpusas) number and postcode (`adr_stat_lr.csv`); one point per address (`adr_gra_adresai_LT.zip`); and the streets, settlements, municipalities and counties the codes name. Registrų centras refreshes the files monthly.

## Licence

The raw data page opens the data under Creative Commons Attribution 4.0 (CC BY 4.0): the data may be used and shared without limit, provided the authorship of the opened data is given, a link to the licence is included, and any changes are stated ("Duomenis galima naudoti ir dalintis neribotai, tačiau privaloma nurodyti atvertų duomenų autorystę, įdėti nuorodą į licenciją bei nurodyti, ar buvo padaryta pakeitimų"; page as its content service served it on 2026-10-29, last updated 2026-02-04). `lt.NOTICE` gives all three. OpenAddresses' `lt/countrywide` records the same licence and attribution for geoportal.lt's database built from the same register. The corpus records the licence as `CC-BY-4.0`.

## Capture

The files were downloaded on 2026-10-29 from `https://www.registrucentras.lt/aduomenys/?byla=NAME` (no `Last-Modified`; the same names serve each month's refresh, so keep the pinned files):

| File | Bytes | SHA-256 |
|---|---:|---|
| `adr_stat_lr.csv` | 56,920,911 | `ec13ee64a49fa8909fe2492b89f06a245cd76b682d349649031d63a9dfffcf97` |
| `adr_gra_adresai_LT.zip` | 62,462,648 | `a050d4712ec81bfef5f74a4b90eebe6400e83dbf387e84319ffe5911a1ec5cbc` |
| ↳ `adr_gra_adresai_LT.json` (2026-10-01 in the archive) | 443,584,849 | `13358033932776e673f77bcc3a8a9fb94a402337e3dca9da585f96fa8844e710` |
| `adr_gatves.csv` | 2,735,105 | `cd346d1ff1fd36da5848f1313e2bf1bb418991ab00529a16738d3a5a967f2ceb` |
| `adr_gyvenamosios_vietoves.csv` | 1,206,241 | `be849eb96c135e8d7f93e65b4ce13c287312077a88a32b0e561e1a4b5e95702d` |
| `adr_savivaldybes.csv` | 3,050 | `8c20342de678d4b310f7ff690494a00ee624b24c2c449f144736b8a8df9f67b3` |
| `adr_apskritys.csv` | 445 | `a68ba78f09d8edff11771ea1bea742ab26033cbe3fb4df212c82e10f27aeb209` |
| `adr_stat_meta.xlsx` (structure) | 9,100 | `ccbb3484ab0ac6fea4c1f7fbdaafe28ca5c89eb01a1586f93537608ce0c672fe` |
| `adr_gra_adresu_taskai_meta.xlsx` (structure) | 11,018 | `a909690d24d2522a3c8ffb60d8aed4ae9a043c8e01cf757daf70cfd673a8f3da` |

The CSVs are UTF-8, separated by `|`, without quotes, with LF line ends. The address points are a GeoJSON FeatureCollection whose every feature has `formav_dat` 2026-10-01, the date the data were formed; the latest registration date in `adr_stat_lr.csv` is 2026-10-02. `tests/registry/adr-csv.sql` joins them by their codes into one CSV, named after the points' date:

```sh
mkdir -p ../lt-data
(
  REPO=$PWD
  cd ../lt-data
  for f in adr_stat_lr.csv adr_gra_adresai_LT.zip adr_gatves.csv adr_gyvenamosios_vietoves.csv adr_savivaldybes.csv adr_apskritys.csv; do
    curl -fL -o "$f" "https://www.registrucentras.lt/aduomenys/?byla=$f"
  done
  unzip adr_gra_adresai_LT.zip
  duckdb -f "$REPO/tests/registry/adr-csv.sql"
)
target/release/evaluate sample-lt --adr ../lt-data/adr-addresses-2026-10-01.csv --seed lt-2026-10 --out tests/registry/lt.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/lt.seeds.jsonl --out tests/registry/lt.jsonl --adr ../lt-data/adr-addresses-2026-10-01.csv
target/release/evaluate run --corpus tests/registry/lt.jsonl --index out --out eval-results/lt
```

`adr-addresses-2026-10-01.csv` has 1,131,379 addresses, 167,662,825 bytes, SHA-256 `e77b843b3935d82ea4389674e36ca801fd944a8c29f2a651909c50d06eb0cd45`. The script keeps an address of `adr_stat_lr.csv` (1,131,504 rows, each `AOB_KODAS` once) only where it has a point (117 have none) whose settlement and street codes are its own (8 points name another settlement), and names its settlement (type, abbreviation, genitive and nominative name), street (abbreviation and genitive name), municipality and county. Preparation, sampling and corpus generation each repeated byte for byte.

**Coordinates.** Each point carries LKS-94 (EPSG:3346, the GeoJSON's `crs`) `X_KOORD` and `Y_KOORD` in whole metres and WGS84 `N_KOORD` and `E_KOORD`, as the structure file describes them. The adapter takes the WGS84 pair as published and rejects a point outside 53.8–56.5° N, 20.9–26.9° E, so swapped columns fail loudly. PROJ's EPSG:3346-to-EPSG:4326 transformation of the LKS-94 pair (DuckDB spatial) agrees with the published WGS84 pair to 1.4e-9° at all 1,131,398 points. The point is the register's address point on the plot or building; no precision beyond the 150 m tolerance is claimed.

## Sample

`sample-lt` reads 1,127,430 eligible addresses: a number without leading zero and at most one capital letter, no building (korpusas) number, and an `LT-NNNNN` postcode. It leaves out 3,731 addresses with a building number ("Pagubės g. 63, korpusas 5") and 218 without a postcode. An eligible address's class is:

- **street**: on a street, without letter ("Taikos g. 4"): 912,860.
- **letter**: with a letter, on a street or in a settlement without streets ("Putinų g. 68M", "Viršurodukio k. 1L"): 150,196.
- **village**: without letter, in a settlement (a village, a homestead or a railway station) that has no streets and numbers its houses itself ("Juočiškių k. 7"): 64,374.

In each county and class, `sample-lt` keeps the 200 rows with the smallest seeded SHA-256 of their `AOB_KODAS`, then takes the first from different settlements: four street, three letter and three village addresses per county, 40, 30 and 30 locations. The sample weights counties and classes, not population or address counts: Vilnius county, with 23% of the addresses, has ten locations and Vilnius city one. It is not a representative national accuracy sample. Of the 100 locations, 14 (28 cases) are held out by the shared location-hash rule.

Each location has two queries:

- **street**, **letter** and **village**: Lietuvos paštas's order. In a municipality's centre (a city whose genitive name is the municipality's), the house, then postcode and city in the nominative: `Punios g. 6, LT-62268 Alytus, Lithuania`. Elsewhere the settlement after a street (a town or small town in the nominative, a village with its type: `Taikos g. 4, Dusetos`, `Agrastų g. 12, Kaniūkų k.`), then postcode and municipality: `Agrastų g. 12, Kaniūkų k., LT-64104 Alytaus r. sav., Lithuania`, `Juočiškių k. 7, LT-62190 Alytaus r. sav., Lithuania`.
- **register**: the register's components in its order, municipality, settlement, street and number, without postcode: `Alytaus r. sav., Kaniūkų k., Agrastų g. 12, Lithuania`, `Alytaus m. sav., Alytaus m., Punios g. 6, Lithuania`, `Alytaus r. sav., Juočiškių k. 7, Lithuania`.

Every case requires goodMatch true at house level within 150 m of the register point, with the register's number and letter, postcode and, on a street, the street with its type abbreviation ("Putinų g."). County names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point. Each case's `places`, the names ReverseGeocode's answer must include one of, are the settlement in the nominative and with its type ("Alytus", "Alytaus m.") and the municipality in full and abbreviated ("Alytaus miesto savivaldybė", "Alytaus m. sav.").

**Index ancestry.** All 1,130,062 Lithuanian addresses in Overture release 2026-10-21.0 come from OpenAddresses' `lt/countrywide`, which reads geoportal.lt's database built from this register's open data (`RC_adresai_atviri_duomenys.zip`; settlement as "Vaškių k.", street with its type, number, postcode; no building numbers). The builder also reads OpenAddresses' `lt/city_of_vilnius` (the city's own address points); the chart reads the Lithuania OpenStreetMap extract for landmarks only. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index from builder `1a879843c757fda0` (Overture 2026-10-21.0, OpenAddresses and the chart's OSM extracts), SearchAddress passes 92 of 200 queries: 88 of the 100 postal forms (all 40 street, 28 of 30 letter, 20 of 30 village) and 4 of the 100 register forms, and 16 of the 28 held-out cases. A same-binary repeat has zero response drift. SuggestAddress finds an acceptable full-string result for 57 (35 street, 22 letter, no village or register form). ReverseGeocode returns the country, a place of the address and the house at 194 of 200 points and a house 25 m away at 194. One critical result is triaged and labelled in `lt.labels.jsonl`. The failures (parser/matching unless said otherwise):

- **Register form (96):** libpostal joins the comma-separated municipality, settlement and street into one road ("alytaus r. sav. kaniūkų k. agrastų g.") or reads the municipality as a venue, so 93 abstain and 2 answer only the country (too_coarse, 110 and 158 km). The labelled far_miss `Pagėgių sav., Užbalių k. 1` reads "užbalių k." as the road with nothing to place it; eight villages are named Užbaliai, and the answer is house 1 of the one in Prienai District, 129 km away, goodMatch true. The postal form of 84 of these 96 locations passes.
- **Postcode and municipality read as a road (10 postal):** in `Mikališkės k. 8, LT-67312 Lazdijų r. sav.` libpostal reads "lt-67312 lazdijų r." as a second road and "sav." as the city, losing the postcode; the raw lookup has the house first, but the resolver accepts no candidate (9 village houses and one with a letter, `Mažeikiškio k. 1A`; all abstain). All 20 village forms that pass are read with the postcode on its own ("lt-62190").
- **Village with a numeral (1 postal):** `Kryžiaukos II k. 10` is read as venue "kryžiaukos ii" and road "k.", and answers house 10 of Čirkai, 1.3 km away (near_miss, goodMatch true).
- **Street written with a first name (1 postal, data):** the register names the street `Vlado Rozmano g.`; geoportal.lt's database, and so Overture, abbreviates the first name to `V. Rozmano g.`. The query answers another street named in full, in the next village (too_coarse, 1.3 km). 882 of the register's street names (21,508 addresses, about 2% of those on streets) are not in Overture as written, 867 of them only with initials ("Jono Basanavičiaus g." as "J. Basanavičiaus g.").
- **ReverseGeocode (6 at 3 locations, data):** the answer is the house, but its places are the nearest locality point's (Geisčiūnai for a house of Gečialaukis), and some municipalities are rendered with their English name ("Alytus District Municipality", which `places` does not list); one house near a border is answered in Šiaulių rajono savivaldybė, not Akmenės.

These are diagnostic results on a local index, not a CI gold run.
