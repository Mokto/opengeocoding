# Italy

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/it.NOTICE)

**Italy** (`it.seeds.jsonl`, `it.jsonl`): 307 locations in 286 municipalities across all 20 regions, 320 queries, drawn from the national address list (*indirizzario*) of **ANNCSU**, the Archivio nazionale dei numeri civici delle strade urbane kept by Agenzia delle Entrate and Istat from data the municipalities certify. The list is published as open data under CC BY 4.0 ([dati.gov.it entry](https://www.dati.gov.it/view-dataset/dataset?id=stradari-ed-indirizzari-regionali-e-nazionali-da-anncsu), [ANNCSU open-data page](https://www.anncsu.gov.it/it/consultazione-dellarchivio/open-data/)). The monthly national and regional ZIPs download anonymously; the authenticated bulk download in the 2024 technical specifications is no longer needed. Municipality names, province abbreviations and regions come from Istat's [SITUAS](https://situas.istat.it/) list of municipalities on the same date (© Istat, CC BY 4.0), because the address list carries only municipality codes.

## Capture and preparation

The 2026-09-15 release has **27,415,954 civic numbers**, one row per external access, each with a unique national access ID (`PROGRESSIVO_ACCESSO`). Georeferencing is optional and done by each municipality, but no longer rare: **20,731,065 rows (76%) from 5,493 of the 7,891 municipalities have coordinates**, in every region. Trieste province has a single georeferenced row, so the corpus has no case there. The sample comes only from georeferenced rows, so municipalities that do not geocode are not represented. The `METODO` field says how the municipality placed each point: 1 surveyed, better than 5 m (1,714,163 rows); 2 surveyed, 5 m or worse (290,652); 3 derived from a territorial database, better than 5 m (7,125,819); 4 derived, 5 m or worse (10,894,269); 5 placed through the Agenzia's municipal portal, accuracy not stated (706,162). Coordinates are ETRF2000 (epoch 2008.0) latitude/longitude, used as WGS84 without transformation.

Prepare the CSV with DuckDB (1.5.6 for this sample), an offline curation tool, not a service dependency:

```sh
REPO="$PWD"
mkdir -p ../anncsu-data
(
  cd ../anncsu-data
  curl -fL -o INDIR_ITA.zip 'https://anncsu.open.agenziaentrate.gov.it/age-inspire/opendata/anncsu/getds.php?INDIR_ITA'
  unzip INDIR_ITA.zip   # INDIR_ITA_20260915.csv
  curl -fL -o situas-comuni-2026-09-15.json 'https://situas-servizi.istat.it/publish/reportspooljson?pfun=61&pdata=15/09/2026'
  duckdb -f "$REPO/tests/registry/anncsu-csv.sql"
)
target/release/evaluate sample-it --anncsu ../anncsu-data/anncsu-addresses-2026-09-15.csv --seed it-2026-10 --out tests/registry/it.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/it.seeds.jsonl --out tests/registry/it.jsonl --anncsu ../anncsu-data/anncsu-addresses-2026-09-15.csv
target/release/evaluate run --corpus tests/registry/it.jsonl --index out --out eval-results/it
```

The `getds.php` link always serves the latest month, so retain the ZIP. It is 336,089,695 bytes, SHA-256 `5d1632be13ae951b215649a28b043a3da37c15efc8018e29ac1012ee1661966d`; its `INDIR_ITA_20260915.csv` is 2,206,082,816 bytes, SHA-256 `00d28bec814ba747d2380b545241aa8cfce84960ac53fb8cac93d03f7fb241dc`. The SITUAS JSON is 3,406,994 bytes, SHA-256 `69ffc40375c706c883463a1ec9462f785c04945f62f0cb09fc323f54aca35314`, identical on two downloads. The prepared CSV is 2,270,127,216 bytes, SHA-256 `db6cedfd4efeb9e0d8fb3333a3eb19d94865b94462791971319000798c65b736`. Preparation, sampling and corpus generation each repeated byte for byte.

The SQL joins each row to its municipality on the cadastral code (`CODICE_COMUNE` = `COD_CATASTO`), which matches all 7,891 register municipalities; the register still carries Sardinia's pre-2026 Istat codes, so joining on `CODICE_ISTAT` would lose 376 of them. It keeps rows with coordinates and counts, over the whole register, the rows sharing a municipality, street ID, civic number, exponent, specificity, metric number and no-number marker.

## Sample

`sample-it` reads 19,771,416 eligible rows and puts each in a class: **bilingual** (the register gives the street a name in a minority language), else **exponent** (a civic number with an exponent, "12/A"), else **locality** (the row names a *località* or *frazione*), else **plain**. In each region and class it keeps the smallest 200 seeded SHA-256 hashes of access IDs, then takes up to five different municipalities. That gives 100 plain, 100 exponent and 95 locality locations (Valle d'Aosta has no eligible locality row), and 12 bilingual ones: 5 in Trentino-Alto Adige, 4 in Friuli-Venezia Giulia and 3 in Valle d'Aosta, where the 200 candidates span fewer than five municipalities. The sample weights regions and classes, not population or address counts; it is not a representative national accuracy sample. Of the 307 locations, 59 are held out by the shared location-hash rule; every query of a location shares its access ID and hold-out assignment.

Eligibility:

- A civic number that is a positive integer, with an exponent that is empty, one capital letter, a number below 100, or BIS, TER or QUATER. Other exponents ("B1", "A/4", "C-2"), civic number 0, metric (kilometre) numbers and accesses without a civic number (SNC) are excluded.
- No specificity, or N/NERO: Genoa's and Florence's red commercial numbers ("12 rosso") repeat the black ones on the same street, and a plain "12" there means the black one. Red and other numbering systems are excluded.
- An address the register lists more than once (86 georeferenced rows) is excluded rather than resolved; so is a street name with fewer than two letters or containing a comma (the register writes some as "VIA GANDHI, MONDHAS K").

The query is the register's street name as written, in capitals with apostrophes for accents ("LOCALITA'"), the civic number, the locality when it differs from the municipality, the Italian municipality name with its province abbreviation in parentheses, and the country: `VIA PRAGA 3, POSSIDENTE, Avigliano (PZ), Italy`. A bilingual location also gets a query in each other language the register gives (German in South Tyrol, Slovene in Gorizia and Udine, French in Valle d'Aosta, Ladin as the second language), with the municipality's other official name from Istat when it has one ("Bozen", "Doberdob") and without the locality, which the register names in Italian only. These are consistent address forms and require goodMatch true. The register has no postcode, so none is queried or scored.

An exponent is written "12/A" and accepted as "12/A" or "12A" through `any_of`; a numeric exponent only as "12/1", since "121" is another number. Every case keeps the registry point, a 150 m tolerance and house level. Raw region names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every case's point.

**Index ancestry.** Overture's Italian addresses (26,841,754 rows in release 2026-09-23.1) all come from OpenAddresses' `it/countrywide`, a copy of this register's georeferenced list of 2026-07-03, and OpenAddresses' regional Italian sources are municipal and regional address registers. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy.

## Local baseline

On a local world index from builder `ce9fa3d2362ead46` (Overture 2026-09-23.1, OpenAddresses and OSM), the corpus run alone:

| Class | Locations | Queries | SearchAddress passes |
|---|---:|---:|---:|
| Plain | 100 | 100 | 100 |
| Exponent | 100 | 100 | 100 |
| Locality | 95 | 95 | 91 |
| Bilingual | 12 | 25 | 20 |

Overall 311/320 pass; 8 are too coarse and one is a near miss, with no critical result. By placement method, SearchAddress passes 27/27 surveyed points better than 5 m, 4/4 surveyed worse than 5 m, 114/116 and 143/146 derived better and worse than 5 m, and 23/27 portal-placed points. All 320 ReverseGeocode country checks pass. SuggestAddress finds an acceptable full-string result for 314/320; the six it misses are houses SearchAddress misses too.

When the corpus was added (builder `7ab597c193ca4bf7`) 186/320 passed, 7/100 exponents and 61/95 localities among them, with one far miss. Two changes fixed most of that:

- **Exponents:** Overture carries the register's exponent in its `unit` field, which `build_index` used to drop for Italy, so "35 A" was indexed as a bare "35". It now keys the house "35/A". A house lookup also tries the other spellings of a letter addition ("35/a", "35a", "35 a"), so "35A" finds the register's "35/A" and "12/A" the "12 A" that Tuscany's and Piedmont's regional sources write.
- **Localities:** libpostal reads a *frazione* between street and municipality into the road ("via praga 3 possidente"), takes it for the road, fuses it with the municipality ("felegara medesano"), takes it for the town or drops it. The resolver then searches the query without it (`country/it.rs`), unless the parse keeps it a place of its own and the municipality the town. The far miss ("VIA FRATELLI COVIELLO 17, SAN CATALDO, Bella (PZ)", answered with San Cataldo in Sicily, 390 km away) was one of these.

SuggestAddress found 91/320 after those changes. It read the province's sigla as a word the town must contain, so "Cordovado (PN)" matched nothing; it now takes the sigla for the province's ISO 3166-2 code, and for the provinces the index has no code for (Aosta, Friuli-Venezia Giulia's four, Sassari) for the province's name. A frazione the houses do not name is left out when the full text finds no house, as SearchAddress does.

The remaining failures:

- **Locality (4):** no such house in the index for Piscopio and Cerese; the index's house 11 on Strada Montegrazie is 2.1 km from the register's; and "LUNGOMARE MESU'E TURRIS 48" finds nothing where the index spells the street "Mesu 'e Turris" (written "MESU E" it is found).
- **Bilingual (5):** every native Italian form passes, and 8/13 of the register's other-language forms. The others are names the index has as streets without the house (Srebrničeva ulica, Rue des Clementines, Streda Dorives), as a hamlet (Podlak) or not at all (Murates).

These are diagnostic results on a local index, not a CI gold run. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure. Human review of the source points is pending.

Each case's `places`, the names ReverseGeocode's answer must include one of, are ANNCSU's `localita`, the municipality and its name in the area's other official language.
