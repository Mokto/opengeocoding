# Brazil

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/br.NOTICE)

**Brazil** (`br.seeds.jsonl`, `br.jsonl`): 158 locations in 149 municipalities across all 27 federative units, 316 queries, drawn from **CNEFE**, the Cadastro Nacional de Endereços para Fins Estatísticos of IBGE, as published from the 2022 Demographic Census with coordinates for every address ([CNEFE page](https://www.ibge.gov.br/estatisticas/sociais/populacao/38734-cadastro-nacional-de-enderecos-para-fins-estatisticos.html), [download folder](https://ftp.ibge.gov.br/Cadastro_Nacional_de_Enderecos_para_Fins_Estatisticos/Censo_Demografico_2022/Arquivos_CNEFE/CSV/UF/)). Municipality names come from IBGE's [localidades API](https://servicodados.ibge.gov.br/api/docs/localidades), because the register carries only municipality codes.

## Licence

IBGE publishes the files anonymously and states no licence for them. Two rules of Brazilian law cover them: Decree 8.777/2016 (federal open-data policy), art. 4 as amended by Decree 9.903/2019, makes data published by the federal Executive, IBGE included, free for society to use and authorizes free use of databases whose rights belong to the Union; and Law 9.610/1998 (copyright), art. 8, V, excludes *cadastros* (registers) from copyright. The decree's definition of open data (art. 2, III) limits conditions to crediting the author or source, so `br.NOTICE` credits IBGE. The corpus records the licence as `BR-Decree-8777-2016`. OpenAddresses also republishes the register with attribution to IBGE. This is a reading of public law, not a licence grant from IBGE.

## Capture and preparation

The register has **111,102,875 rows**, one per *espécie* (household, establishment, building under construction) at an address, each with latitude and longitude. Census enumerators captured the points on handheld devices during the 2022 census (August 2022 to May 2023), so their precision varies with the device and the site. `NV_GEO_COORD` says how each point was placed: 1 original census coordinate; 2 modified, apartments sharing a street number moved apart; 3 estimated, for addresses without or with invalid coordinates; 4 to 6 the block face, locality or census tract. Counting each row at the highest level among its street number's rows, 97,682,637 rows are at level 1, 7,261,800 at 2, 4,461,244 at 3 and 1,697,194 at 4 to 6. Coordinates are SIRGAS 2000, used as WGS84 without transformation. Street names, numbers and localities are in capitals without accents ("RUA GOIAS").

The 27 state ZIPs (dated 2024-05-20 on the server, Rondônia's 2024-06-12) are 3,922,077,434 bytes together; their SHA-256s:

| File | Bytes | SHA-256 |
|---|---:|---|
| 11_RO.zip | 16,310,880 | `24af73415134f23fed8bb587f1ae64721510e9efea546706b18f4c3afac5f36a` |
| 12_AC.zip | 7,023,748 | `bdbf3163f5c330f6fcfd9fe3b5881b2e8c28248f3f7cec42c11cb1a9c929667b` |
| 13_AM.zip | 29,620,766 | `74cb2d806cf19324e3897d4a106de874a6bf7fdcc812141b96f726855c86cefc` |
| 14_RR.zip | 4,520,527 | `97e52c09e4107069746afba3063bfc2a9732ac28f46e139148e55f5c05606d58` |
| 15_PA.zip | 103,050,645 | `8e1d971c53af3ad32c0e43e00d34e6d221c9d594cf84197acaa2dadc683616bd` |
| 16_AP.zip | 5,454,312 | `074f778510848a75aa3d156a1764643a4811431cf8ffec52f678a9dcd67c9c95` |
| 17_TO.zip | 15,412,488 | `71ac3af348b484cea7ac072af0aa3c9ebfefb73085684a43a490fee880b4f194` |
| 21_MA.zip | 77,694,765 | `6e2433cc017c9bed6102e4cf8018c0152e75c96c4ef94b42dee5a09aa2acd858` |
| 22_PI.zip | 34,838,556 | `984ae0afb1f06dfbd9b24b86c194641109698ccb64155c4ed159edf88c7c7e79` |
| 23_CE.zip | 138,747,815 | `46d614871858029201da71147bd4309dae88f8734c8b31e6cbe4fefa1edf51fb` |
| 24_RN.zip | 34,231,013 | `1e76a715a68b1b99ab8407925fdfcb083df11ba5901c7ff80e4f39e5be0cc710` |
| 25_PB.zip | 40,796,565 | `7104f6e001f5b7613b7d7e1169fcfb7d8253f839ff7feab649edf6e9f2d2f78d` |
| 26_PE.zip | 151,432,961 | `6ba1c047acf18d5754220d8a993571ac0f03c60c28cf167f9405b0df9f91fcde` |
| 27_AL.zip | 29,180,557 | `9dc46c082d806b8d360135fe644ca94d8d0d88f7f076c54a3d0cd1e944edbf8e` |
| 28_SE.zip | 20,632,480 | `736df79493fbf1e0bc88d18aa4e7aa2acc42b8f07a22895b1bf2389df52f9e0d` |
| 29_BA.zip | 398,181,853 | `3a6aa3ec58a87374d389ce2f0f255454be8cb845498a54d14d4fa89a54019685` |
| 31_MG.zip | 537,151,309 | `dbbbbcf46a739d9e43d5095af3e87928d34a5ad348e3b586917514a82fe4f5ad` |
| 32_ES.zip | 41,567,902 | `31e58a51ae993862e9d99afcda995d8305c86d7aac5e4afcce07bf52723f0ade` |
| 33_RJ.zip | 338,096,849 | `5f7695904b80456eb9854b51e7317ee95141332773ce84a630a7f20edd9f7f9b` |
| 35_SP.zip | 1,082,015,432 | `1949c8370a64cea539fa2b3ae87b094a072b61f86499df017ac11373a127d6fc` |
| 41_PR.zip | 242,066,031 | `6a0ed65da17b01e4043269ba1d585bd722dd1235b25d2182e2c09b08e54908f9` |
| 42_SC.zip | 118,607,932 | `0508cc192c0d4485cdec264b88e12aee011f4ab1a72b0567f5be87fd953935bd` |
| 43_RS.zip | 247,249,924 | `b09866937f8dde7eb2a3efd24fe09cf3adb6c33544c6866e78af1c808b79128b` |
| 50_MS.zip | 29,787,554 | `bb1781ee1a6f63e3bb6616b4ac6ca1b55c9fb53ef5adcc891dddbc6a1668204f` |
| 51_MT.zip | 37,772,175 | `f5c8d632e7b7464a6330380458d1d7793ee315d9a7eb8e4ba1db36424d3c1637` |
| 52_GO.zip | 120,818,046 | `844c5611ba76f48c1bc9cdac4e80d02283416190b09517a889619af12ecbf075` |
| 53_DF.zip | 19,814,349 | `53a564b31adb94c46d5d7dbe00cc30aed911176ea6bb6e0f4f25ea652a96587a` |

The municipality list fetched on 2026-10-22 is 2,470,036 bytes, SHA-256 `86ecdccdf97d72e7e5e46f0854cfcea8bacc28154cc1bc4ed0e6110e3a6c9c02`; it matches all 5,570 register municipalities. The API always serves the current list, so retain the file.

Prepare the CSV with DuckDB (1.5.6 for this sample), an offline curation tool, not a service dependency. The unpacked CSVs take about 15 GB, so each state is streamed from its ZIP:

```sh
REPO="$PWD"
mkdir -p ../cnefe-data
(
  cd ../cnefe-data
  B=https://ftp.ibge.gov.br/Cadastro_Nacional_de_Enderecos_para_Fins_Estatisticos/Censo_Demografico_2022/Arquivos_CNEFE/CSV/UF
  for f in 11_RO 12_AC 13_AM 14_RR 15_PA 16_AP 17_TO 21_MA 22_PI 23_CE 24_RN 25_PB 26_PE 27_AL 28_SE \
           29_BA 31_MG 32_ES 33_RJ 35_SP 41_PR 42_SC 43_RS 50_MS 51_MT 52_GO 53_DF; do
    curl -fL -o "$f.zip" "$B/$f.zip"
    unzip -p "$f.zip" | duckdb -f "$REPO/tests/registry/cnefe-uf.sql"
    mv cnefe-uf.parquet "cnefe-$f.parquet"
  done
  curl -fL -o ibge-municipios-2026-10-22.json 'https://servicodados.ibge.gov.br/api/v1/localidades/municipios'
  duckdb -f "$REPO/tests/registry/cnefe-csv.sql"
)
target/release/evaluate sample-br --cnefe ../cnefe-data/cnefe-addresses-2022.csv.gz --seed br-2026-10 --out tests/registry/br.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/br.seeds.jsonl --out tests/registry/br.jsonl --cnefe ../cnefe-data/cnefe-addresses-2022.csv.gz
target/release/evaluate run --corpus tests/registry/br.jsonl --index out --out eval-results/br
```

`cnefe-uf.sql` groups a state's rows into street numbers: the rows sharing a municipality, street type, title and name, number and modifier. For each it keeps the lowest `COD_UNICO_ENDERECO` as the reference and that row's point, and counts the CEPs, localities, rows and address units (codes), the lowest and highest geocoding level, the rows outside urban blocks (`NUM_QUADRA` 0) and the spread of the rows' points. `cnefe-csv.sql` joins the municipality names and state abbreviations and writes **55,932,637 street numbers**, sorted by reference. The prepared CSV is 933,669,470 bytes, SHA-256 `4f930c7d40257a0e187d51af4ed303d042165bca390d613d3fcc65e772aa405a`. Re-preparing a state, the final join, sampling and corpus generation each repeated with identical output.

## Sample

`sample-br` reads 42,672,333 eligible street numbers and classes each **urban** (every row in an urban block) or **rural** (every row outside one). In each state and class it keeps the smallest 200 seeded SHA-256 hashes of references, then takes up to four (urban) or two (rural) different municipalities. Brasília is the Federal District's only municipality, so DF has one location of each class: 105 urban and 53 rural locations in all. The sample weights states and classes, not population or address counts; it is not a representative national accuracy sample. Of the 158 locations, 28 are held out by the shared location-hash rule; both queries of a location share its reference and hold-out assignment.

Eligibility:

- A number that is a positive integer of up to five digits, with no modifier. Modifiers are mostly not letters but "SN" (no number), "KM", or a utility's pole or meter label ("ENERGISA", "POSTE"), so numbers written with one are excluded.
- One CEP and one locality for the whole street number; a number found in two bairros or under two CEPs is two addresses.
- Every row placed at level 1 or 2 (an address coordinate), and the rows' points within 30 m of each other.
- Every row urban or every row rural; a street type and name, without commas.

The register names streets by type ("RUA", "ESTRADA", "SITIO", "POVOADO") rather than only roads, so rural cases include farms, hamlets and rivers written as the street. The `full` query is the register's street, number, locality (left out when it repeats the municipality name), the municipality with its state, the CEP and the country: `RUA MAJOR GUALTER, 225, SOBRAL, Rio Branco - AC, 69912-130, Brazil`. The `short` query keeps street, number, municipality and state: `RUA MAJOR GUALTER, 225, Rio Branco - AC, Brazil`. Both require goodMatch true. Every case keeps the register point, a 150 m tolerance, house level, the number and the CEP. Region expectations are left empty, so ResolveRegion is not checked; ReverseGeocode is asked about every case's point.

**Index ancestry.** All 89,887,635 Brazilian addresses in Overture release 2026-09-23.1 come from OpenAddresses' `br/<state>/statewide` sources, AddressForAll's processing of this register; OpenAddresses also has municipal registers for twelve cities, and Brazil's OpenStreetMap extract is read too. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy.

## Local baseline

On a local world index from builder `ce9fa3d2362ead46` (Overture 2026-09-23.1, OpenAddresses and OSM), the corpus run alone:

| Class | Form | Queries | SearchAddress passes |
|---|---|---:|---:|
| Urban | full | 105 | 102 |
| Urban | short | 105 | 102 |
| Rural | full | 53 | 46 |
| Rural | short | 53 | 52 |

Overall 302/316 pass; 6 are too coarse and 8 near misses, with no critical result. All 316 ReverseGeocode country checks pass; SuggestAddress finds an acceptable full-string result for 139/316 (306/316 on a world index from builder `17bc2fab0087eddb` once a municipality followed by a CEP is read without its state and a number may follow a numbered road, up from 196 on that index). The failures:

- **Locality and CEP (7, parsing or matching):** the `full` forms of RUA URACU 3 (Rio Branco), RAMAL 6 VICINAL 7, SITIO SERRA DO COITE 5, FAZENDA RANCHO ALEGRE 7, RAMAL DOS BURITIS 5, RUA JOAO PAULO I 15 and LUGAREJO ROCADO 8 answer the CEP's centre, up to 29 km away, though Overture has each house at the register point and every `short` form passes. The bairro or rural locality between number and municipality loses the house, as Italy's *frazioni* did. Since the municipality before a CEP is read without its state, RAMAL 6 VICINAL 7, SITIO SERRA DO COITE 5, RAMAL DOS BURITIS 5 and RUA JOAO PAULO I 15 pass on the builder `17bc2fab0087eddb` index.
- **Number without its modifier (4, index data):** RUA DURVAL VIEIRA 10 (Jaguaré) and ESTRADA ESTRELA DALVA 15 (Santa Rita do Novo Destino) answer, 177 m and 2.6 km away, the register's "10" with modifier "2" and its "15 KM", which the OpenAddresses copy lists as plain "10" and "15".
- **No such house (2, index data):** Overture has no RUA NOSSA SENHORA DO ROSARIO 1593 in Fazenda Rio Grande, so both forms answer the street, 151 m away.
- **Brasília (1, matching):** "EDF QNM 19 CONJUNTO G, 45, Brasília - DF" answers Brasília though the raw search's first candidate is the house; the `full` form, which names CEILANDIA, passes.

These are diagnostic results on a local index, not a CI gold run. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure. Human review of the source points is pending.
