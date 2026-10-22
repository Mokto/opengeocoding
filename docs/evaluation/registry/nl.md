# Netherlands

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/nl.NOTICE)

**Netherlands** (`nl.seeds.jsonl`, `nl.jsonl`): 684 locations and 342 damaged variants drawn directly from Kadaster's BAG GeoPackage of 2026-10-01, distributed by PDOK under CC0-1.0. The sampling frame contains 8,320,999 in-use residential objects with a point and postcode. `sample-nl` ranks the main-address number-designation IDs by seeded SHA-256, keeps the smallest 200 per municipality and suffix class, and selects one plain number and one with a house letter or addition on different streets in each of all 342 municipalities. There are 302 sampled numbers containing letters and 85 with additions (some have both). This weights municipalities equally and deliberately oversamples suffixes, not population or address counts; it is not a representative national accuracy sample. Of the 684 base locations, 129 are held out by the shared location-hash rule.

Download and retain [the BAG GeoPackage](https://service.pdok.nl/kadaster/bag/atom/bag.xml): the URL moves every month. The file used here is 7,818,600,448 bytes, SHA-256 `b56b3a1a9156d2c7b1d9a6a15cb227f5d126116ae6556cbad668964e988dff0f`. Prepare a CSV with DuckDB's spatial extension (1.5.6 for this sample); this is offline corpus preparation, not a service dependency:

```sh
curl -fL https://service.pdok.nl/kadaster/bag/atom/downloads/bag-light.gpkg -o bag-light-2026-10-01.gpkg
duckdb <<'SQL'
INSTALL spatial;
LOAD spatial;
SET threads = 4;
COPY (
  SELECT nummeraanduiding_hoofdadres_identificatie AS id,
         openbare_ruimte_naam AS street, huisnummer AS number,
         huisletter AS letter, toevoeging AS addition, postcode,
         woonplaats_naam AS city, bronhouder_identificatie AS municipality,
         round(ST_Y(point), 8) AS lat, round(ST_X(point), 8) AS lon
  FROM (
    SELECT *, ST_Transform(geom, 'EPSG:28992', 'EPSG:4326', always_xy := true) AS point
    FROM ST_Read('bag-light-2026-10-01.gpkg', layer := 'verblijfsobject')
    WHERE status = 'Verblijfsobject in gebruik'
      AND list_contains(string_split(gebruiksdoel, ','), 'woonfunctie')
      AND geom IS NOT NULL AND postcode IS NOT NULL
  )
) TO 'bag-addresses-4326-2026-10-01.csv' (HEADER);
SQL
target/release/evaluate sample-nl --bag bag-addresses-4326-2026-10-01.csv --seed nl-2026-10 --out tests/registry/nl.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/nl.seeds.jsonl --out tests/registry/nl.jsonl --bag bag-addresses-4326-2026-10-01.csv
target/release/evaluate run --corpus tests/registry/nl.jsonl --index out --out eval-results/nl
```

The prepared CSV is 672,460,226 bytes, SHA-256 `37bc9a5318c48cefff8f117a5dcac73e93843dd27876cb43c479600a9e162cdb`. Each generated case records its filename, checksum and coordinate conversion. `build-gold --bag` re-reads every sampled ID from that CSV, rejects a missing or duplicate ID, and copies its house number (letter attached, addition after a hyphen), street, postcode and point. No geocoder selects or verifies an expectation. Points are rounded to six decimal places and checked within 150 m. Municipality codes are sampling tags, not inferred ISO regions, so these cases do not check ResolveRegion.

One location per municipality also gets an edit, balanced among eleven applicable kinds: no country, no postcode, uppercase, lowercase, no commas, reversed parts, a space in the postcode, a street or town typo, or another sampled postcode or town from that municipality. Variants retain their location and hold-out assignment. Consistent edits require goodMatch true; typos and contradictory parts accept either value. The GeoPackage publishes only one address per addressable object; secondary addresses, moorings, pitches, non-residential objects and objects not yet in use are outside this frame. BAG covers the European Netherlands, not the Caribbean Netherlands. Dutch index addresses share BAG upstream data, so passes measure registry fidelity, not independent real-world accuracy.

Two same-binary runs on the full world index (builder `83ef195a69f42f93`, Overture 2026-09-23.1 with the local optional sources) returned identical results for all three checked RPCs. SearchAddress passed all 684 base locations and 337 of 342 variants; the five failures were too-coarse answers, four to street typos and one to a query without a postcode. There were no critical results and no new known-failure labels. SuggestAddress passed 676 of 684 full base strings and 185 of 342 variants. ReverseGeocode returned the accepted country for all 1,026 points; this does not check the house or locality it names. The standalone Dutch gate passes, and grpcurl's documented known failure reproduced. A full-suite comparison kept all 6,919 existing cases' answers unchanged across all checked RPCs; both absolute runs still fail the pre-existing Rue de Rivoli must-pass case. Its same-consumer timing comparison flagged slowdowns above the 10% budget for all three timed RPCs, so there is no clean speed result. No service behaviour changed. These are development observations, not production accuracy or speed claims.

Each case's `places`, the names ReverseGeocode's answer must include one of, are BAG's `woonplaats` (`city`).
