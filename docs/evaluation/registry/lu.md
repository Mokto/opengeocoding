# Luxembourg

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/lu.NOTICE)

**Luxembourg** (`lu.seeds.jsonl`, `lu.jsonl`): 240 buildings, one per commune and number class, drawn from the Administration du cadastre et de la topographie's BD-Adresses CSV of 2026-10-19 (CC0). The file is the weekly `addresses.csv` at `https://download.data.public.lu/resources/adresses-georeferencees-bd-adresses/20261019-023118/addresses.csv` (dataset `last_update` 2026-10-19T02:31:47Z). It is 28,199,194 bytes, SHA-256 `5508e2077e0f45ce512df910934eb6977105ffd6ac286bdddb58fa3fbb810b4e`, publisher MD5 `6e438b9f9d544c5b01b4a0f384e49bd0`.

The CSV is semicolon-separated UTF-8 with a leading BOM. Coordinates are the published `lat_wgs84` and `lon_wgs84` columns. The LUREF easting and northing are not used. Points outside latitude 49–51 and longitude 5–7 fail preparation, which rejects a LUREF value read as a WGS84 coordinate.

ACT describes BD-Adresses as a subset of the national locality and street register (CACLR). A point is placed by hand or at the centroid of the cadastral parcel or of the largest building on that parcel. It is not the building entrance. The cadastral reference frame can sit off LUREF. Several addresses can share one coordinate. Not every official address is georeferenced, and ACT does not guarantee completeness. The 150 m check is proximity to that published point, not a surveyed entrance.

```sh
curl -fL -o addresses-2026-10-19.csv \
  https://download.data.public.lu/resources/adresses-georeferencees-bd-adresses/20261019-023118/addresses.csv
target/release/evaluate sample-lu --act addresses-2026-10-19.csv --seed lu-2026-10 --out tests/registry/lu.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/lu.seeds.jsonl --out tests/registry/lu.jsonl --act addresses-2026-10-19.csv
target/release/evaluate run --corpus tests/registry/lu.jsonl --index out --out eval-results/lu
```

## Eligibility and sampling

The pinned file has 179,699 rows. Sixteen rows have an empty `numero` and are ineligible. Every remaining number is a plain integer (`162321`), an integer with an ASCII letter suffix (`17121`, including `A`, `BIS` and `bis`) or a hyphen range (`241`, four of them with a letter on the high number, such as `3-3A`). Postcodes are four digits. `id_caclr_bat`, `id_caclr_rue` and `id_geoportail` are present on every numbered row.

`id_caclr_bat` is the location. 537 groups repeat it, 583 extra rows in all, at most five rows, always the same street and number. Five of those groups list two communes (and two LAU codes) a few metres apart; street, number, localité and postcode still match. The row with the smallest `id_geoportail` is kept, then the smaller latitude and longitude. That leaves 179,100 buildings. A shared coordinate does not merge buildings: three published points are shared, one of them by twelve addresses, and those addresses stay separate locations.

`sample-lu` keeps one building per commune and class, the minimum seeded SHA-256 of `lu-2026-10`, a NUL and `id_caclr_bat`. All 100 communes have a plain number and a suffix; 40 have a range. The sample is 100 plain, 100 suffix and 40 range queries. It weights communes and number classes equally, not population or address counts, so it is not a national accuracy sample. Rare suffixes (`BIS`, doubled letters) are eligible and were not drawn. One sampled range is `3-3A`. Of the 240 locations, 51 are held out by the shared location-hash rule.

The query is the published street, number, postcode and localité, then `Luxembourg`. The commune is a sampling tag. On 81,296 numbered rows the localité and the commune differ; the query still uses the localité. No second wording, `L-` postcode prefix or translated name is invented. Expectations require country `lu`, the published house number and postcode, `goodMatch` true and the source point within 150 m. Commune names are not ISO 3166-2 codes, so ResolveRegion is unscored.

## What a pass measures

Overture's September 2026 address theme lists 179,533 Luxembourg addresses and attributes them to the Luxembourg open data platform, CC0, distributed by OpenAddresses, accessed 2026-09-20. The chart reads the Luxembourg OSM extract for landmarks only. This index's `openaddresses.tsv` has no Luxembourg file: OpenAddresses is added for countries Overture does not already cover. OpenAddresses does publish a validated `lu/countrywide` address layer (batch job 936894, updated 2026-10-16) from this register. A pass measures fidelity to BD-Adresses, not an independent survey. Earlier coverage counts for Luxembourg are about 178,800 indexed houses.

## Validation

On the local world index (`~/opengeocoding-index/out`, builder `1929034e0fce080d`, service data index `210076085b5e`), one `evaluate run --corpus tests/registry/lu.jsonl --repeat 1` scored 238 of 240 SearchAddress cases. Plain numbers passed 100/100 and suffixes 100/100, including the held-out locations (51/51). Ranges passed 38/40. The two misses are street answers with the source postcode and `goodMatch` false: Rue d'Arlon 3-5, Windhof (Koerich), 122 m from the published point, and Rue de Medernach 10-12, Larochette, 277 m away. The drawn lettered range, Krautemergaass 3-3A in Dalheim, passed. These two are not critical and are not labelled. Expectations are unchanged.

SuggestAddress returned no candidate for any of the 240 full strings. That cause was not isolated. ReverseGeocode returned the accepted country at all 240 points and does not check the house. ResolveRegion is unscored. The run's transport checks passed except grpcurl's documented SearchAddress hang, which reproduced. The absolute gate passed. No full-suite drift comparison was run. The consumer was git `71708bdfa6cb` built from a dirty worktree, models `65627ea2114b`. This corpus changes no resolver or builder behaviour. These are development observations, not a production accuracy or speed claim.
