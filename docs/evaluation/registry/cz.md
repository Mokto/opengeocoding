# Czechia

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/cz.NOTICE)

**Czechia** (`cz.seeds.jsonl`, `cz.jsonl`): 329 locations in 234 municipalities across all 14 regions, drawn directly from ČÚZK's RÚIAN address and administrative-hierarchy exports of 2026-09-30 (© Český úřad zeměměřický a katastrální, CC BY 4.0). The national address ZIP contains 6,258 municipality CSVs and 3,022,397 address rows. Preparation keeps the 3,021,489 rows with coordinates, from 6,256 municipalities, joins their municipality to its region from the hierarchy export, and transforms the points to WGS84. The positive S-JTSK CSV columns `Y, X` become `(-Y, -X)` in EPSG:5514 before PROJ converts them; treating them as longitude/latitude or keeping their signs is wrong. This is not a survey-grade coordinate check.

The sample targets different numbering systems rather than conventional damaged addresses: 61 descriptive/orientation pairs, 58 pairs with an orientation letter, 70 descriptive numbers on named streets, 70 descriptive numbers without a street (the municipality part names the address), and 70 evidential numbers, written with `č.ev.`. In each region and class, `sample-cz` keeps the smallest 200 seeded SHA-256 hashes of registry address IDs, then takes up to five different municipalities or city districts. Some regions have fewer than five places with dual numbers. The sample weights regions and numbering classes, not population or address counts; it is not a representative national accuracy sample. It excludes invalid or absent number/postcode fields, orientation numbers without a street, and evidential numbers that also have an orientation number. Of the 329 locations, 89 are held out by the shared location-hash rule.

Each of the 119 dual-number locations also gets an orientation-only query and an explicitly marked `č.p.` descriptive-only query, for 567 cases total. These are consistent address forms and require goodMatch true, not damaged inputs. A dual-number answer may report the combined number, the descriptive number, or the orientation number including its letter: all three come from that same registry row and retain its country, postcode, point and 150 m tolerance, through the existing `any_of` expectation format. No geocoder selects or verifies an expectation. The same registry ID gives every form the same location and hold-out assignment. The evaluator currently skips reverse checks for `any_of` cases, even when their points coincide; only the 210 other cases check ReverseGeocode. Raw region codes are sampling tags, not inferred ISO region expectations, so none checks ResolveRegion.

Retain both pinned exports from the [address](https://atom.cuzk.gov.cz/RUIAN-CSV-ADR-ST/RUIAN-CSV-ADR-ST.xml) and [hierarchy](https://atom.cuzk.gov.cz/RUIAN-CSV-HIE-ST/RUIAN-CSV-HIE-ST.xml) feeds. Prepare the CSV with `iconv` and DuckDB spatial (1.5.6 for this sample), offline curation tools, not service dependencies:

```sh
REPO="$PWD"
mkdir -p ../ruian-data
(
  cd ../ruian-data
  curl -fLO https://vdp.cuzk.gov.cz/vymenny_format/csv/20260930_OB_ADR_csv.zip
  curl -fLO https://vdp.cuzk.gov.cz/vymenny_format/csv/20260930_strukt_ADR.csv.zip
  unzip -p 20260930_OB_ADR_csv.zip 'CSV/*.csv' | iconv -f WINDOWS-1250 -t UTF-8 > 20260930_addresses-utf8.csv
  unzip -p 20260930_strukt_ADR.csv.zip 'strukturovane-CSV/vazby-cr.csv' > 20260930_hierarchy.csv
  duckdb -f "$REPO/tests/registry/ruian-csv.sql"
)
target/release/evaluate sample-cz --ruian ../ruian-data/ruian-addresses-4326-2026-09-30.csv --seed cz-2026-10 --out tests/registry/cz.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/cz.seeds.jsonl --out tests/registry/cz.jsonl --ruian ../ruian-data/ruian-addresses-4326-2026-09-30.csv
target/release/evaluate run --corpus tests/registry/cz.jsonl --index out --out eval-results/cz
```

The address ZIP is 63,460,492 bytes, SHA-256 `fad0022ec08a56ccbf2f32e5c005409a8f8bc47f2f8bf665b8e2eec4245ce516`; the hierarchy ZIP is 34,209,961 bytes, SHA-256 `7132a9df018c733fc89006f13a757d38d1bd4b701a6b97659aca5922414e2344`. The prepared CSV is 277,066,884 bytes, SHA-256 `f1a11ee8600c9b5ff9992e0b90226c39b8e2e101f75d3e325ddf3a9e00dc2dd9`. Its preparation and sampling both repeat byte for byte. Each generated case records the prepared filename, checksum, coordinate conversion, raw numbering fields and registry-record link. `build-gold --ruian` re-reads every sampled ID and rejects a missing or duplicate ID. RÚIAN also supplies Overture's Czech addresses and has been imported into OSM: independent selection avoids index-conditioned sampling, but passes measure registry fidelity, not independent real-world accuracy.

On one local full index (builder `83ef195a69f42f93`, Overture 2026-09-23.1 with the local optional sources), SearchAddress passes 251 of 329 base locations and only 1 of 238 single-number variants: 70/70 descriptive street numbers, 60/61 dual numbers, 53/58 dual numbers with letters, 68/70 village numbers, 0/70 explicit `č.ev.` queries, 1/119 orientation-only forms and 0/119 explicit `č.p.` forms. The latter tests conflate the single-number lookup with handling the `č.p.` marker; they do not establish that every unmarked descriptive-only query fails. SuggestAddress passes 247 of 567 strings; ReverseGeocode returns the accepted country at 209 of 210 checked points and nothing at the other, without checking the house or locality name.

The evaluator compares an answer's house number written with its type, `č.ev. 346` or `č.p. 470`, as the registry's number (`346`, `470`); the 150 m point check still tells the two series apart. See the [India triage](../triage/india-houses.md), which made such answers possible.

Seven critical results at three Prague locations are triaged in `cz.labels.jsonl`: the exact combined-number or village house is within 1 m of the registry point, but the street has no assigned place (`u32::MAX`), and the raw index and RPC omit country and region ancestry. Scoping a direct index query with `cz` restores the country, not the ancestry. Single-number variants also fall back to those countryless streets. These are missing-country outputs, not points abroad; expectations remain unchanged. Other failures stay visible and unlabelled. Re-scoring after attaching the labels changed no class. A same-binary repeat has zero response drift; a full-suite comparison leaves all 7,945 existing cases' answers unchanged across all checked RPCs, with no relative accuracy regression. Both absolute full runs still fail the existing Rue de Rivoli must-pass case, and grpcurl's documented known failure reproduced. The Czech gate passes with its attributed baseline labels. No resolver, scorer or builder behaviour changed; these are development observations, not production accuracy or speed claims.

