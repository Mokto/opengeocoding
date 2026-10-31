# Uruguay

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/uy.NOTICE)

**Uruguay** (`uy.seeds.jsonl`, `uy.jsonl`): 103 locations in all 19 departments, 206 queries, drawn from **Direcciones Geográficas del Uruguay**, the national address base the Infraestructura de Datos Espaciales (IDE Uruguay) publishes on the [Catálogo de Datos Abiertos](https://catalogodatos.gub.uy/dataset/ide-direcciones-geograficas-del-uruguay) from information of the Correo Uruguayo: one CSV per department with every address's access point (latitude, longitude and the same point as WKB), postcode, street code and name, door number and letter, kilometre, block (manzana) and lot (solar), building name, census locality and department. The catalogue says the base holds official addresses (assigned by the departmental governments) and unofficial ones (existing on the ground), and refreshes the files every 30 days.

## Licence

The catalogue gives the licence as "Licencia de DAG de Uruguay" (`odc-uy`) and links the text, [Licencia de Datos Abiertos – Uruguay, version 0.1](https://www.gub.uy/agencia-gobierno-electronico-sociedad-informacion-conocimiento/sites/agencia-gobierno-electronico-sociedad-informacion-conocimiento/files/documentos/publicaciones/licencia_de_datos_abiertos_0.pdf) (read on 2026-10-30; `Last-Modified` 2020-02-17, 155,676 bytes, SHA-256 `ee78c585910e1222af58f2c1ccb0a21a1492e01b84cecfaf09b03c4059e54497`). It lets open data be reproduced, distributed, published, adapted and transformed, combined with other data into new datasets and built into products, commercially or not. Every use must cite the provider, the "Licencia de Datos Abiertos – Uruguay" and the dataset used, and mention changes and modifications in that note of origin. `uy.NOTICE` gives all four. OpenAddresses' `uy/*/regionwide` sources record the same licence and the attribution "Infraestructura de Datos Espaciales (IDE Uruguay), Correo Uruguayo". The corpus records the licence as `Licencia-de-Datos-Abiertos-Uruguay-0.1`.

## Capture

The files were downloaded on 2026-10-30 (18:41 UTC) from `https://catalogodatos.gub.uy/dataset/85ca8574-3bb3-4329-b005-aed19883e19a/resource/RESOURCE/download/NAME`. Each address serves the latest refresh; this one is of 2026-10-01 (`Last-Modified` 06:00 to 06:02 UTC on every file), so keep the pinned files. Every file's MD5 matches the catalogue's `hash`.

| File | Resource | Bytes | SHA-256 |
|---|---|---:|---|
| `artigas.csv` | `e2bb1efb-1e57-47d7-bc40-2ff8a5fa2d7b` | 3,981,589 | `71b01ae82900573bec28158bfb4a807a0ae32177dc9e9bf71f43d566b13e4b64` |
| `canelones.csv` | `4cd4cf75-b44a-4637-a43d-2cc5a0294ca5` | 28,029,449 | `10c59206476e94aa5cca82db72081c63d85f64c24955f2399a412afd27c2afa4` |
| `cerro_largo.csv` | `64af2cac-cac4-44ee-b6be-09edf1536ed8` | 4,708,832 | `298af27e811a27c3eb34a8e1823a4bbc57fe63602b2eceac36b9e9b7b9a77803` |
| `colonia.csv` | `11b10088-e286-4626-bd06-c45043045dc1` | 7,968,314 | `fc57968c8442fd8a1f11af1d81dedea52e24f833f077e1bce3dd80b269f5e2e9` |
| `durazno.csv` | `66a8e451-96ca-4c4d-afc4-1a32abc2295e` | 3,202,715 | `f92844fe8eed99ed050c1b138f2f367beee68550091fd38dfd9fbd91dd8d00d7` |
| `florida.csv` | `7306e8e1-27c0-473b-915f-30cc919ca765` | 3,828,092 | `625f8f9347e7670023109ed8f34f2b7aa73ded56b0b6d0939da4d020c76d6338` |
| `flores.csv` | `7d572335-106e-4282-8265-ca23e2f34baf` | 1,612,393 | `c2f191a3a5a1b5d625a75ae3fa589d906f2ceb7abf07a6ccb4b6b304fb995721` |
| `lavalleja.csv` | `0d5a007d-d01b-4fa6-9bdd-7fd08c68fb3d` | 3,510,298 | `346038cc77372be40ff8d02b2fef7cd8c2d216baf3b783610a49a0db248764db` |
| `maldonado.csv` | `42eef510-214c-479b-a580-09805c7c29c1` | 9,366,508 | `464eb1fb05946f84b5030eec15807b9b270f84f626973b4c07c4774314b7dfcc` |
| `montevideo.csv` | `c8e3e713-e755-4112-bfe3-fa01b20dcda1` | 64,531,354 | `8dc67b32cf62d6991e81a7952d9bf225827c79b6f33fd83d9f38299b06de5a16` |
| `paysandu.csv` | `e5f583c8-c240-4fc3-9170-ff701cc1ae14` | 6,115,289 | `c453d142aba600b4e5de0eee1f296f9ee002f5c714e3d130b659dc8a5529bf96` |
| `rio_negro.csv` | `558cadb0-4cb6-4fcc-9c24-6720e7a05912` | 2,982,713 | `7dc22ed68ccb6741e8057263e0dc5e6572a954d025c73aa1d0b3790b8e827933` |
| `rivera.csv` | `da91a1a9-7149-4281-8ee8-53dbd29bab77` | 5,642,549 | `62a806083e3f40b46de2f25deae4f7b3a2c8b4e3d9ee8ee72bbf53bff0f79553` |
| `rocha.csv` | `baa0c351-5a7c-4240-b54e-bfa8db442559` | 4,996,997 | `2d22eb93c2d97f1afb9857b5d08da2f64158d439b5c13a163e6b0fbd70d61239` |
| `salto.csv` | `fba901e2-8918-4795-8249-0368d8c0586d` | 5,223,067 | `9076f185124df05d02e4ef5e1a3656d016f8ad77244e3dc65f27e962a9f62ef3` |
| `san_jose.csv` | `ff75c363-a8a9-4105-9086-335f52beae50` | 6,011,085 | `6c52faaff2d437d1adcbecd77dd4a50886c71196fc9fdf4e326cf65208b222b4` |
| `soriano.csv` | `26820104-d0ee-4823-9b9a-ab7b441a6c94` | 4,366,314 | `a5460549da2d034664e012e1d4dcc1cd50f3965d3d4c1a29539ad5b0941681d6` |
| `tacuarembo.csv` | `c4827bfe-ceea-42dd-88a0-7043aa3fa779` | 5,100,021 | `cfb82953edaaf87b2debb4b6bce6e1f49ff5f72a3fb6a7c1bb1e67701e499c66` |
| `treinta_y_tres.csv` | `ccf4a98f-7223-4d9f-9bf5-061500623f18` | 2,762,799 | `d7645c8be05802c3be630152a82ab170bc09d1021c3b91f38ea4142cdacc2384` |
| `metadatos_datos_ide.json` (attributes) | `e16361e4-93dd-461d-83da-71b0228cad44` | 3,913 | `bfb7273bb70726fb94fa836efc7dac05f45d9a82a1bc5d9ccca00bc47b8191c0` |

The CSVs are Windows-1252 (six rows carry stray UTF-8 bytes or a replacement character, in a letter field the rules below drop or in a building name no query uses), comma-separated with CRLF line ends, quoting only a field that holds a comma or a quote (doubled inside); a missing value is the text `N/A`. Each file is converted to UTF-8 with `iconv`, and `tests/registry/dgu-csv.sql` (DuckDB 2.0.0) joins the 19 files, drops the WKB column and counts, for every row, the rows of its department, locality, street name, door number and letter (`copies`). It writes one CSV named after the data date:

```sh
mkdir -p ../uy-data/utf8
(
  REPO=$PWD
  cd ../uy-data
  B=https://catalogodatos.gub.uy/dataset/85ca8574-3bb3-4329-b005-aed19883e19a/resource
  while read -r id name; do curl -fL -o "$name" "$B/$id/download/$name"; done <<'EOF'
e2bb1efb-1e57-47d7-bc40-2ff8a5fa2d7b artigas.csv
4cd4cf75-b44a-4637-a43d-2cc5a0294ca5 canelones.csv
64af2cac-cac4-44ee-b6be-09edf1536ed8 cerro_largo.csv
11b10088-e286-4626-bd06-c45043045dc1 colonia.csv
66a8e451-96ca-4c4d-afc4-1a32abc2295e durazno.csv
7306e8e1-27c0-473b-915f-30cc919ca765 florida.csv
7d572335-106e-4282-8265-ca23e2f34baf flores.csv
0d5a007d-d01b-4fa6-9bdd-7fd08c68fb3d lavalleja.csv
42eef510-214c-479b-a580-09805c7c29c1 maldonado.csv
c8e3e713-e755-4112-bfe3-fa01b20dcda1 montevideo.csv
e5f583c8-c240-4fc3-9170-ff701cc1ae14 paysandu.csv
558cadb0-4cb6-4fcc-9c24-6720e7a05912 rio_negro.csv
da91a1a9-7149-4281-8ee8-53dbd29bab77 rivera.csv
baa0c351-5a7c-4240-b54e-bfa8db442559 rocha.csv
fba901e2-8918-4795-8249-0368d8c0586d salto.csv
ff75c363-a8a9-4105-9086-335f52beae50 san_jose.csv
26820104-d0ee-4823-9b9a-ab7b441a6c94 soriano.csv
c4827bfe-ceea-42dd-88a0-7043aa3fa779 tacuarembo.csv
ccf4a98f-7223-4d9f-9bf5-061500623f18 treinta_y_tres.csv
EOF
  for f in *.csv; do iconv -f WINDOWS-1252 -t UTF-8 "$f" > "utf8/$f"; done
  duckdb -f "$REPO/tests/registry/dgu-csv.sql"
)
target/release/evaluate sample-uy --dgu ../uy-data/dgu-addresses-2026-10-01.csv --seed uy-2026-10 --out tests/registry/uy.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/uy.seeds.jsonl --out tests/registry/uy.jsonl --dgu ../uy-data/dgu-addresses-2026-10-01.csv
target/release/evaluate run --corpus tests/registry/uy.jsonl --index out --out eval-results/uy
```

`dgu-addresses-2026-10-01.csv` has 1,059,256 rows, 121,021,523 bytes, SHA-256 `fdd9474269136f2330311b95232c7e1536b34d331974445c8b97e3a8fde5ddc0`. Preparation, sampling and corpus generation each repeated byte for byte.

**Coordinates.** The metadata gives `latitud` and `longitud` as the access point's coordinates, and the catalogue says the addresses use EPSG:4326. `punto_wkb` is the same point in WGS 84 / UTM zone 21S (EWKB, SRID 32721): DuckDB spatial's transformation of it to EPSG:4326 agrees with the published pair to 1e-13° at all 1,059,256 rows. All points lie within 34.98–30.16° S, 58.43–53.25° W; the adapter rejects a point outside 35.1–30.0° S, 58.5–53.0° W, so swapped columns fail loudly. No precision beyond the 150 m tolerance is claimed.

## Sample

The register has no address identifier. A case's reference is the locality code, street code, door number and letter (`844-16173-673`, `366-18199-109-BIS`), which is unique among eligible rows because an eligible row is the only one of its department, locality, street, number and letter. `sample-uy` reads 826,023 eligible rows, in 557 localities. It leaves out, in this order:

- 2,171 rows with door number 0 or none (rural addresses by kilometre, buildings by name only);
- 7,145 with a letter field other than none, one capital A–Z or `BIS` ("DU", "CC", "01", "S/N", "TR"; one empty);
- 168,592 that share their department, locality, street, number and letter with another row (170,220 rows in all, 1,560 of them repeating another row exactly), so the query names more than one point;
- 1 with a kilometre as well as a number;
- 19,786 on streets whose name is a placeholder (`UYCALDS_SN_003`) or holds quotes, brackets or a slash;
- 193 in localities named `N/A` or with a bracketed or slashed alternative ("18 DE JULIO (PUEBLO NUEVO)");
- 29,674 in the census's rural areas ("CANELONES RURAL"), which are not places an address names;
- 5,671 in localities the register files under two departments ("LA PAZ" and "COLONIA NICOLICH" in Canelones and Montevideo, "CERRO CHATO", "MASOLLER").

An eligible row's class is **plain** (no letter, 804,579), **bis** ("675 BIS", 9,571) or **letter** (one capital written after the number, "6413A", 11,873; 98% of them in Montevideo). A row is in the department's **capital** setting when its locality is the department's seat: MELO, COLONIA DEL SACRAMENTO, TRINIDAD, MINAS, FRAY BENTOS, SAN JOSE DE MAYO and MERCEDES, elsewhere the locality named as its department (CANELONES, not the larger LAS PIEDRAS); otherwise it is **interior**.

`sample-uy` takes, by the seeded SHA-256 of the reference, the two smallest plain rows in each department and setting, and the smallest bis and letter row in each department: 74 plain (38 capital, 36 interior: Montevideo has no eligible interior locality), 19 bis and 10 letter locations (nine departments have no eligible letter row). The sample weights departments, settings and classes, not population or address counts: Montevideo, with 44% of the eligible rows, has four locations. It is not a representative national accuracy sample. Of the 103 locations, 17 (34 cases) are held out by the shared location-hash rule.

Each location has two queries, as the register writes the parts, in capitals and without accents:

- **plain**, **bis** and **letter**: street and door number, postcode and locality, department, country: `SARANDI 673, 85000 TRINIDAD, FLORES, Uruguay`, `GENERAL JOSE GERVASIO ARTIGAS 109 BIS, 55100 BELLA UNION, ARTIGAS, Uruguay`, `BULEVAR JOSE BATLLE Y ORDOÑEZ 6413A, 12900 MONTEVIDEO, Uruguay`. A locality named as its department (MONTEVIDEO, SALTO) is not repeated.
- **no_postcode**: the same without postcode: `SARANDI 673, TRINIDAD, FLORES, Uruguay`.

Every case requires goodMatch true at house level within 150 m of the register point, with the register's door number and letter (`109 BIS` and `109bis` agree), its postcode and its department's ISO 3166-2 code (`UY-FS`), which ResolveRegion is also asked for. ReverseGeocode is asked about every point; each case's `places` are the locality and the department.

**Index ancestry.** Overture release 2026-10-21.0 has 1,052,295 Uruguayan addresses, all from OpenAddresses' 19 `uy/*/regionwide` sources (jobs of 2026-10-09), which read these department CSVs. Their conform takes the number from `num_puerta` alone (`^([0-9]+)$`), so no Uruguayan address in Overture has a letter: a register door "109 BIS" is an Overture house "109" at the BIS point. 957,900 of the register's 1,059,256 rows have an Overture address with the same street, number and postcode at the same point (to 1e-6°), and all 103 sampled locations do, within 1 m. The chart's build Job therefore also reads the register itself (`index.sources.dgu`, see `docs/guides/indexing.md`): it downloads the 19 department CSVs and writes their 22,659 doors with a letter (files of 2026-10-01) as the OpenAddresses layer `uy_dgu-letters`. It skips OpenAddresses layers that Overture carries, `uy/*/regionwide` among them, and reads Geofabrik's Uruguay OpenStreetMap extract. `uy/mo/montevideo` (the Intendencia de Montevideo's access points, which keep the letter) is not read: its source definition states no licence the builder recognises. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local full index from builder `fd5091031cb03b1a` (this change on `main` at `3d753ab`) with the `uy_dgu-letters` layer (Overture 2026-10-21.0, OpenAddresses, the chart's OSM extracts and GeoNames), SearchAddress passes 205 of 206 queries, the 34 held out included, and SuggestAddress finds an acceptable full-string result for all 206. ResolveRegion returns the department at all 206 points. ReverseGeocode passes at 170 of 206 points and finds a house 25 m away at all 206. Nothing is labelled.

On builder `56dd7c8cf3af06c0`, without the layer, SearchAddress passed 143 (plain numbers 143 of 148, no bis or letter door), SuggestAddress 148 and ReverseGeocode 150. The changes:

- **Doors with a letter:** OpenAddresses' Uruguay layers drop the letter (see Index ancestry), so the index held no "109 BIS" or "283A", and those queries answered the street or locality with goodMatch false; four landed 2.7–40 km away. The build Job now reads them from the register (`index.sources.dgu`). All 58 bis and letter queries pass.
- **Postcode 20000 shown as "20.000":** one OpenStreetMap object in Maldonado tags `addr:postcode=20.000`. The builder files postcodes by their letters and digits and showed each in its smallest written form, and "20.000" sorts before "20000", so every house of postcode 20000 answered "20.000", and a query with "20000" did not match it: `CERRO SAN ANTONIO 2428` answered with goodMatch false and `AVENIDA SANTA TERESA 1008` answered a street 1 km away. A postcode is now shown in its smallest form without a dot. Only forms with a dot change; phone codes tagged as postcodes ("+251", "+85620") keep showing as before.

The gold suite on the same local index changes only Uruguayan cases and one DRC postcode now shown "BP 202" instead of "B.P. 202" (still failing, same class): SearchAddress +60, SuggestAddress +58 and ReverseGeocode +20 checks, no newly failing or worse case.

Remaining failures:

- **ReverseGeocode at lettered doors (18 points, 36 checks):** Overture's letterless copy of each door ("109" at the "109 BIS" point, 0.0–0.3 m away) is still in the index, and ReverseGeocode answers it at 16 of the 19 bis and 2 of the 10 letter points. Attribution: backend/data, OpenAddresses' conform. It goes once `uy/*/regionwide` join `letra_puerta`, after which the `dgu` source can be dropped.
- **Ciudad del Carmen (1 case):** `RIVERA 1010, CIUDAD DEL CARMEN, DURAZNO, Uruguay` answers a street of the department's capital 50 km away; the index knows the town as Carmen and Villa del Carmen, and the structured backend query finds the house. With the postcode the query passes. Attribution: parser, the locality's "Ciudad del" qualifier.

These are diagnostic results on a local index, not a CI gold run.
