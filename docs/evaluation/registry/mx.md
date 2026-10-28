# Mexico

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/mx.NOTICE)

**Mexico** (`mx.seeds.jsonl`, `mx.jsonl`): 160 locations in 138 municipalities of all 32 states, 320 queries, drawn from INEGI's **Directorio Estadístico Nacional de Unidades Económicas (DENUE)**, release 05_2026: every active establishment in the country with its street type and name, exterior number and letter, settlement, postcode, locality, municipality, state and coordinates ([catalogue entry](https://www.inegi.org.mx/rnm/index.php/catalog/979), [bulk download](https://www.inegi.org.mx/app/descarga/?ti=6)). It is a register of businesses, not of addresses: the corpus measures business addresses, and keeps only their address fields, never a business name, activity, phone, e-mail or web site.

## Why DENUE

Mexico has no open national address register. INEGI's nearest product, the *números exteriores* layer of its cartography (*Información vectorial de localidades amanzanadas y números exteriores*, a point per numbered plot), is published per locality from INEGI's map library, without a single dated national file, and it is already in the index: Overture's 30,726,687 Mexican addresses in release 2026-10-21.0 are its `mx/countrywide` source, AddressForAll's copy of that layer, 15,017,167 of them with a number. A sample of it would only measure fidelity to the index's own source. DENUE is a national, dated bulk download with a point per establishment, and the sample below keeps out the establishments the index already reads from it.

## Licence

INEGI publishes DENUE under its *Términos de Libre Uso de la Información del INEGI* ([terms](https://www.inegi.org.mx/inegi/terminos.html), [PDF](https://www.inegi.org.mx/contenidos/inegi/doc/terminos_info.pdf)), the licence its metadata names. They allow copying, publishing, adapting, extracting part of the information and commercial use, on condition of crediting INEGI as "Fuente: INEGI, <product>" with its date, telling users of any transformation, and not implying INEGI's endorsement. `mx.NOTICE` gives that credit and says the queries were constructed from the register's fields. The corpus records the licence as `INEGI-Terminos-de-Libre-Uso`. OpenAddresses also republishes DENUE (its 2015 state files) with attribution to INEGI.

## Capture and preparation

The 33 state ZIPs of release 05_2026 ("Directorio Estadístico Nacional de Unidades Económicas (DENUE) 05_2026", modified 2026-05-20; Sinaloa's was republished on 2026-07-01) were downloaded on 2026-10-28 from `https://www.inegi.org.mx/contenidos/masiva/denue/`; the State of Mexico comes in two parts. Together they are 574,199,079 bytes:

| File | Bytes | SHA-256 |
|---|---:|---|
| denue_01_csv.zip | 6,797,877 | `3679a407c7da2657f353ee57aa37f58084cf7522bff0dd03f43aca5eb16a17bc` |
| denue_02_csv.zip | 13,720,130 | `07f33c0d289f3218990d52246f8edf6705a2a6580c636ed3e3736fcd2f9d8fe7` |
| denue_03_csv.zip | 3,893,069 | `51ada17868f8eeda25559d8b3350970fa2bcab792d68a63af5cad038c76c13b4` |
| denue_04_csv.zip | 3,845,263 | `7e1690e55aee8cbed558fbecab72f8e81b3b32f19b33d8d1e525c5c38cbe6ad6` |
| denue_05_csv.zip | 12,113,938 | `7df6cf18dd6105fb2b96d9fbf27cc658d3aa1a54054559e7b0a6489734065828` |
| denue_06_csv.zip | 3,788,726 | `948e292e567ce16a8b7485f15846ddaaf9c9cda6fda9be07334bb6afc5480af6` |
| denue_07_csv.zip | 21,392,585 | `be4799488ecc135ebcf18d49167711d16186c81c7d502d297572e83deaac0bd4` |
| denue_08_csv.zip | 13,749,250 | `d946462adfa8fc4a5697e0f99ccf42a86bca88ee77bf1c14b5c24ba612c72e05` |
| denue_09_csv.zip | 45,439,249 | `ae608f30118f6313e9d537ea22918b2c9e9a3d3f19c2ddf5907f55dbb1642b19` |
| denue_10_csv.zip | 6,911,602 | `b2a84170f379bcbcea6f18c5d65ac097ad5f2902b3c9b1d17ba654b4bac7a9b2` |
| denue_11_csv.zip | 27,962,614 | `56e1755701a421b73d877b501ccd895f1ed85c8efe30e003604d846bd91b8fcb` |
| denue_12_csv.zip | 15,554,083 | `a9fa160d9faac810d80a39303528729b6df0bf4879adbacef091909cf1f57f09` |
| denue_13_csv.zip | 15,059,273 | `d0747958bc364177ea8d281047334cffd4d3505247f43967e08da2ca9c1d9ca5` |
| denue_14_csv.zip | 39,432,220 | `5a3ee180edecaa85be195c0efaa1bd918ffbd3dfb406fab0cbf0c450635e8a30` |
| denue_15_1_csv.zip | 50,572,588 | `23021fc24a2046e1aa3ad8c21137ca57988ded26b6f293ec0c929f59dd2cb1ae` |
| denue_15_2_csv.zip | 30,236,017 | `ad53e5fb65f15b491e140a80e79b207b95eb0320e4a69822d5a3cefb7652bc20` |
| denue_16_csv.zip | 26,873,186 | `c6351ac9a953928cfa2bb06920eb5f00c1265ec1ca9319988d5b85856da8748b` |
| denue_17_csv.zip | 9,975,747 | `87aa074040907d273f7b086125f00daff316d96736f1bb0ef570dc3b6297067c` |
| denue_18_csv.zip | 6,601,995 | `eeee9d5037f47f3832af2ad643934a9b17601041082d8252cbe6d92550e0c64e` |
| denue_19_csv.zip | 21,475,515 | `ba9963bd99f27b65220f1e249384ab74c744cc665ca134347040ccbd8860ae0f` |
| denue_20_csv.zip | 24,928,890 | `3dd10addafc615b50f4d2867ec61258c412cfabfae2b7769e9f020190c3cb4d1` |
| denue_21_csv.zip | 37,421,731 | `814aea797a6fa3d9ed0307d14eeda516806ea765fcaa4fb4f7192c40029210af` |
| denue_22_csv.zip | 10,382,206 | `2ea1e298086f109cdbdb6a036d6cd3ecfbdfe26123b34248d73b1d06c201304a` |
| denue_23_csv.zip | 5,926,665 | `264dfef280b9905f5e70950dbada6da85aa2b2551e681708c4e0dc302d382d60` |
| denue_24_csv.zip | 11,881,223 | `6e611e177ab4483d6777301bb39829ac04a5bbc820046f64f5bbf110d8947269` |
| denue_25_csv.zip | 12,902,619 | `f0f64d43554d9b956aa77ac46e3c8b975a38617da1a4cf76e17f89d212906004` |
| denue_26_csv.zip | 12,405,139 | `332bbcd4cc79a5806cbde9e9e4be0522648171d48f04d4e13b1500dce5192fdc` |
| denue_27_csv.zip | 8,348,151 | `bd0b889eea608e4d1d3214e96b367f34af2f1650c259d4b65f06d35c65e5b2ed` |
| denue_28_csv.zip | 13,747,403 | `56b07316d59cc1227bcae766ce2e7660df58a79e2f57a21c1ac8ddac7b3f3e9a` |
| denue_29_csv.zip | 8,840,434 | `2efedf9fca28c7611322d5cfa1e81a8f4535aaaf3d610c6f56edbb91dcfc910e` |
| denue_30_csv.zip | 34,453,409 | `3295a498b87bb37a426c34b77470458af5374ffd613b6bad38a5f6e6335f69f5` |
| denue_31_csv.zip | 10,832,360 | `a47d4054f2cdeb5ebfce0da8ce1934cb830417174c504182b81964a329556536` |
| denue_32_csv.zip | 6,733,922 | `d5cb1d0a9b05b4f45f539adf9d426d0cf0aaf52fa5f430d59b38bf9ee78983dd` |

The address in the URL serves the current release, so keep the pinned files. Each holds one CSV of 42 columns in ISO-8859-1 with a handful of UTF-8 sequences (three rows have mojibake such as "MÃ\u0081RQUEZ"). Convert each to UTF-8 as ISO-8859-1, which maps every byte, and prepare one CSV with DuckDB (2.0.0 for this sample), an offline curation tool, not a service dependency:

```sh
REPO="$PWD"
mkdir -p ../denue-data/csv
(
  cd ../denue-data
  B=https://www.inegi.org.mx/contenidos/masiva/denue
  for s in 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15_1 15_2 16 17 18 19 20 21 22 23 24 25 26 27 \
           28 29 30 31 32; do
    curl -fL -o "denue_${s}_csv.zip" "$B/denue_${s}_csv.zip"
    unzip -p "denue_${s}_csv.zip" 'conjunto_de_datos/*.csv' | iconv -f ISO-8859-1 -t UTF-8 > "csv/denue_inegi_${s}.csv"
  done
  duckdb -f "$REPO/tests/registry/denue-csv.sql"
)
target/release/evaluate sample-mx --denue ../denue-data/denue-addresses-05-2026.csv.gz --seed mx-2026-10 --out tests/registry/mx.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/mx.seeds.jsonl --out tests/registry/mx.jsonl --denue ../denue-data/denue-addresses-05-2026.csv.gz
target/release/evaluate run --corpus tests/registry/mx.jsonl --index out --out eval-results/mx
```

`tests/registry/denue-csv.sql` keeps the identifier and the address fields of each of the 6,138,075 establishments, the staff band, fixed or semi-fixed, whether it is in a shopping centre and the month it entered DENUE, and adds, over every establishment of the same state, municipality, locality, street and exterior number, how many there are, how many postcodes they give and how far apart their points are. Names, activities and contact details are dropped. The result, `denue-addresses-05-2026.csv.gz`, is 228,445,574 bytes, SHA-256 `4f438bfcda2a9945f76998d3054287bc473e93ca5d1625bd75539a6a534c267f`; preparation, sampling and corpus generation each repeated byte for byte. `latitud` and `longitud` are WGS84 degrees of the establishment; the adapter rejects a point outside 14.3–32.8° N, 118.5–86.6° W, so swapped coordinates fail loudly. DENUE does not say whether a point marks the entrance or the building; no precision beyond the 150 m tolerance is claimed.

## Sample

`sample-mx` reads 162,823 eligible establishments. In order, it keeps fixed establishments (5,964,036), with six or more staff (754,330), which entered DENUE in 2016 or later (387,006), outside a shopping centre or market (335,911), with an exterior number of one to five digits without leading zero and at most one capital letter (201,684), a five-digit postcode not starting "00" (198,345), a named street that is not a highway or "other" (188,394), and an address all of whose establishments give one postcode with points within 30 m (162,892); 69 more have a comma or mojibake in a name or an empty place field. The staff rule keeps out the smallest businesses (up to five staff), often run from a home, so the sample holds workplaces, not likely homes. The date rule keeps out the establishments in the 2015 DENUE files OpenAddresses republishes, which the index reads. Mexico City's 14,640 four-digit postcodes, which lost a leading zero, are left out rather than repaired.

Each establishment is in one class: **seat**, in a locality coded 0001, a municipality's seat (in Mexico City, the borough itself); **locality**, any other locality. In each state and class, `sample-mx` keeps the 200 establishments with the smallest seeded SHA-256 of their DENUE id, then takes the first from different municipalities: three seats and two other localities per state, 96 and 64 locations. 19 numbers carry a letter. The sample weights states and classes, not population or establishment counts; it is not a representative national accuracy sample, and it measures business addresses of workplaces with six or more staff. Of the 160 locations, 34 (68 cases) are held out by the shared location-hash rule.

Queries are built from the register's fields:

- **full**, in the order of INEGI's address standard (*Norma Técnica sobre Domicilios Geográficos*): `CALLE INDEPENDENCIA 305-A, COLONIA CALVILLO CENTRO, 20800 Calvillo, Aguascalientes, Mexico`, naming the municipality after the locality where they differ (`... 21960 Ciudad Morelos, Mexicali, Baja California, Mexico`)
- **short**, as commonly written: `CALLE INDEPENDENCIA 305-A, Calvillo, Aguascalientes, Mexico`

DENUE writes street and settlement names in capitals, mostly without accents, and places in mixed case. The street type is left out where the name already starts with it ("CALLE DEL OLMO"), and the settlement type where it is "OTRO" or repeated; a settlement named after the locality is left out. A number on a numbered street is written after "#" (`CALLE 67 #10`). A bracketed alternative name is dropped from a street, settlement or locality ("Ciudad Morelos (Cuervos)" is written "Ciudad Morelos", "La Noria [Parque Industrial]" "La Noria"). A letter is joined with a hyphen ("305-A"); the scorer compares numbers without spaces and hyphens, as the index's "305A".

Every case requires goodMatch true at house level within 150 m of the DENUE point, with its number and postcode. Raw state names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point. Each case's `places`, the names ReverseGeocode's answer must include one of, are the locality, with and without its bracketed alternative, the municipality and, in Mexico City, the city.

**Index ancestry.** The chart's index reads three Mexican address sources: Overture's `mx/countrywide` (INEGI's *números exteriores*, above), OpenAddresses' 2015 DENUE state files (on the local index 31 of 32: Michoacán's export has no source definition there, so its licence is unknown and the builder skips it) and the OpenStreetMap extract `mexico`. Selection reads only DENUE, before any service answer. DENUE and the números exteriores layer come from the same institute and share its geostatistical framework, so the corpus is not independent of the index's main source: 62 of the 160 sampled numbers have a números exteriores point with the same number within 150 m, 45 a 2015 DENUE point (an earlier establishment at that address), 85 one or the other. The other 75 locations have neither.

## Local baseline

On a local full index from builder `66c174324c99692f` (Overture 2026-10-21.0 and OpenAddresses, without the Mexico OpenStreetMap extract the chart reads), SearchAddress passes 55 of 320: 22/96 full and 19/96 short seat queries, 8/64 full and 6/64 short locality queries, 9 of the 68 held-out cases. Every pass is at one of the 85 locations whose number an index source has; the 75 others never pass. A same-binary repeat has zero response drift. The failures:

- **too_coarse (194):** 116 answer the street, 27 the postcode area and 51 the locality; mostly the index lacks the number on that street.
- **near_miss (39):** a street (16), locality (10) or postcode-area (6) answer more than 150 m from the point, or a house of the same number elsewhere (7).
- **abstain (12)** and **wrong_component (3)**, where the index's house carries another postcode than DENUE's.
- **confidence_mismatch (1):** `CALLE 25 #901, Chihuahua, Chihuahua, Mexico` finds the house, 9 m away, with goodMatch false.

The 16 critical results (far_miss beyond 100 km, all with goodMatch false) are labelled in `mx.labels.jsonl` with their attribution:

- **The stated state does not constrain a fallback (12, matching):** when the house is not found, the resolver answers a namesake locality or street in another state, though the query names the state: "Solidaridad, Quintana Roo" answers Solidaridad in Temixco, Morelos (Overture's divisions call the municipality Playa del Carmen); "Benito Juárez, Quintana Roo" and "Cancún, Benito Juárez" answer Nuevo León; "El Oasis, Coahuila de Zaragoza" answers Chihuahua even as a bare place. In `CALLE MIGUEL HIDALGO 425, La Pila, San Luis Potosí` the raw top candidate is the register's house, and the answer is a street of that name in Hidalgo. Six of these 12 are short forms; the six full forms name a municipality (Los Cabos, Carmen, Solidaridad, Benito Juárez) that has a namesake elsewhere.
- **Namesake in the same state (3, data or query):** the short form names no municipality, and the index has no place of the register's name there (Ciudad Morelos in Mexicali, Aquiles Serdán in Gómez Palacio) or answers a street in a namesake town (Villa González Ortega); the full forms land within 60 km.
- **El Paraíso, Guerrero (1):** answers a subdivision in Jalisco; not traced further.

SuggestAddress finds an acceptable full-string result for 45 of 320. ReverseGeocode returns the house at 50 of 320 points and a house within 150 m of a point 25 m away at 244; 64 answers name another place than the register's locality or municipality. These are diagnostic results on a local index, not a CI gold run.
