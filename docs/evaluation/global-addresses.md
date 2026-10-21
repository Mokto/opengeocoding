# Global address source-consistency benchmark

This compact benchmark contains 6,500 mapped address groups across 130 markets (50 per market) and 12,991 distinct query variants. It is **not an independent accuracy gold standard**: OpenStreetMap also supplies index data and libpostal training data. It is included in `evaluate run --suite gold`, the evaluation image and CI gold comparisons, with separate `global_osm_*` cohorts. Do not combine its pass rates with registry-first accuracy evidence or call a country nationally validated because it appears here.

The checked-in [corpus](../../tests/global/global-addresses.jsonl) needs no upstream downloads to run. The [initial measured baseline](global-addresses-baseline.md) retains failing diagnostics and unresolved source conflicts. The [manifest](../../tests/global/manifest.json) contains the selected source records, country coverage, snapshot hashes and sampling parameters. [Attribution and data licence](../../tests/global/NOTICE).

## Markets

US, CA, MX, BR, AR, CL, CO; GB, IE, FR, DE, NL, BE, CH, AT, ES, PT, IT, DK, SE, NO, FI, PL, CZ, GR, HU, RO, RU, TR; ZA, EG, NG; SA, AE, IL; IN, CN, JP, KR, TW, SG, MY, TH, VN, ID, PH; AU, NZ.

Added in October 2026, from the same seed and quota: AL, AM, AO, AZ, BA, BB, BD, BG, BO, BT, BY, BZ, CD, CI, CM, CR, CU, CY, DO, DZ, EC, EE, ET, GA, GE, GH, GT, HR, HT, IQ, IR, IS, JM, JO, KE, KG, KH, KZ, LA, LK, LT, LU, LV, MA, MD, ME, MG, MK, MM, MN, MT, NA, NC, NI, NP, PE, PK, PR, PS, PY, QA, RS, SI, SK, SL, SR, SV, SY, SZ, TJ, TM, TN, TT, TZ, UA, UG, UY, UZ, VE, XK, ZM, ZW.

This is an explicit market panel, not every sovereign country or a claim that omitted countries lack usable data. All 130 meet the 50-group quota; that is address-source coverage, not passing-answer coverage. The October 2026 additions are every remaining country whose Geofabrik extract gave the full quota. The others were left out rather than admitted with fewer groups: 67 had fewer than 50 eligible nodes (24 had none, mostly islands), and in 34 small states and territories the 0.05-degree border exclusion leaves no interior at all. Some additions draw on a small pool or one area: Cameroon, Laos, Cyprus, Haiti, Qatar, Jamaica and Costa Rica had 57 to 90 eligible nodes, and Malta's groups come from one quarter-degree cell, Qatar's from two.

## What the records establish

Each selected record is an OpenStreetMap **address node**, with a complete `addr:housenumber`, `addr:street` or `addr:place`, and a source town/city or postcode. Coordinates are the mapped node's WGS84 point, not a geocoder response, building interpolation or a town representative. Node ID, version and timestamp identify the source record. A 150 m tolerance checks proximity to that mapped point; it does not certify an entrance, surveyed position, apartment or cadastral building.

The forward expectation requires house resolution, the complete source number, country, source postcode when present, point proximity and `goodMatch`. It supplies no independent street-alias crosswalk: a nearby house on a different street with the same number can satisfy these checks. It never strips letters, splits ranges, replaces unit/civic compounds or invents missing source numbers. Numbers are compared as the index keys them: full-width digits as ASCII and without a trailing `號`/`号` ("number"), so `１１８號` is 118. Ranges and multi-value tagging conventions may expose source/representation disagreements rather than resolver defects; investigate before changing either code or expectations.

Up to two deterministic queries use number-before-street and street-before-number orders, followed by the source's available geographic qualifiers and the country's English name; duplicate strings are emitted once. Street/place names and numbers remain in their original script. These are **component-order probes**, not a representative collection of verbatim local mailing formats. No translations, typos or claimed apartment coordinates are generated.

Only address fields are retained. Names, contacts, free-form `addr:full`, contributor usernames and contributor IDs are not copied. Recognizable phone numbers, contact/URL contamination, Plus Codes, placeholder streets and multiple-number lists are rejected at admission, not treated as civic identifiers. Geographic node IDs are retained for provenance. Verification is automated source-record verification, not individual human address or positional review.

## Sampling and limitations

- The target is 50 address groups per market. The manifest reports shortfalls and capture errors rather than backfilling them with place names or weakening expectations.
- Eligibility is decided before evaluating the service, without querying the index or using its answers. Numeric civic identifiers, finite coordinates, bounded printable components and a geographic qualifier are required. Letter-only identifiers, unsupported country-tag aliases, underspecified nodes and overlong constructed queries are excluded.
- Natural Earth's pinned 1:10m country polygons clip source nodes. **All polygon features sharing an ISO code are combined**; otherwise an island such as Clipperton could overwrite mainland France. The masks are eroded inward by **0.05 degrees** before sampling. This deliberately excludes coastal and border-adjacent points: without that exclusion, cartographic detail can put an address on the wrong side of a frontier. The margin is latitude-dependent, not a fixed metric distance. These remain approximate cartographic masks, not legal boundary adjudication; disputed territories and residual positional uncertainty require caution.
- Eligible nodes are grouped into quarter-degree latitude/longitude cells. The 100 lowest seeded cell hashes form the candidate pool; each keeps up to 50 lowest seeded node hashes. Round-robin selection spreads the sample across available retained cells. This is neither population-weighted nor nationally representative. Latitude changes cell area, and small states have fewer cells.
- Only nodes are sampled. Addresses mapped solely on ways, relations, buildings or interpolation lines are outside the frame. Mapping activity and address completeness vary sharply between countries. The manifest includes eligible node/cell counts, not national address totals.
- Duplicate normalized country/number/street/city/postcode groups are suppressed. This is not an audited physical-building crosswalk; duplicate buildings and differing source representations can remain. Each address group shares one stable `location` and the existing SHA256-modulo-five hold-out assignment across both query orders. Keep held-out groups out of tuning.
- Snapshot dates differ by extract and are recorded in each PBF header. Keep source-data changes separate from resolver experiments. Membership in OSM, agreement with this service and a successful source-consistency check are not independent accuracy evidence.
- Suggestions check their first-five type/country/point geography using the existing evaluator, not exact civic identity. Reverse checks country only; this benchmark supplies no ISO-region expectations and does not score ResolveRegion. It is not gRPC interoperability sign-off.

An early admission pilot found phone numbers and Plus Codes misfiled as house numbers, contact text inside address tags, and false country labels near coarse polygon borders. Its measurements are not a valid benchmark baseline. The final sample uses uniform source-only guards and fresh captures; admission never depends on a service answer or pass outcome.

Keep the authoritative registry-first corpora and curated global place tests. This benchmark complements them by revealing missing index coverage, number-role failures and country/format problems across more markets. Do not tune against the hold-out, relabel failures or relax expectations to produce green country scores.

## Registry adjudications

Where a national register places the same address elsewhere, the case also accepts the register's point (`expect.any_of`, the OSM expectation first) and records it under `adjudication`. Number, postcode, house level, `goodMatch` and the 150 m radius stay; only the point differs. Such cases have two expected points, so ReverseGeocode no longer checks them.

So far only Australia, against the August 2026 G-NAF release (`G-NAF` in [the notice](../../tests/global/NOTICE)). `adjudicate-gnaf.py find` reads the manifest's records and the official PSV zip, never a service answer, and admits a record when exactly one current principal G-NAF address without unit or level has its complete number, street and postcode, with an address-level default geocode more than 150 m from the OSM node. Of 50 Australian groups, 43 match one G-NAF address; 27 of those lie within 150 m and 16 are adjudicated, at 0.19 to 13.7 km (mostly rural property and building centroids). Leading zeros, ranges and addresses absent from G-NAF are not adjudicated. See the [Australian triage](triage/global-au.md).

```sh
python3 tests/global/adjudicate-gnaf.py find --manifest tests/global/manifest.json \
  --gnaf SOURCE/g-naf_aug26_allstates_gda2020_psv_110.zip --release "August 2026" \
  --out tests/global/adjudications.json
```

The zip's SHA256 is recorded in `adjudications.json`. A new G-NAF release is a data update: rerun `find`, then the replay and `apply` steps below, and review the corpus diff.

## Run

The default `evaluate run --suite gold --index SOURCE/index --out eval-results/gold` includes this panel alongside the existing corpora. To run only the panel:

```sh
INDEX_DIR=SOURCE/index SERVICE_BINARY=target/release/opengeocoding \
  target/release/evaluate run --suite gold \
  --corpus tests/global/global-addresses.jsonl --index SOURCE/index \
  --repeat 3 --out eval-results/global-addresses

target/release/evaluate report eval-results/global-addresses \
  --corpus tests/global/global-addresses.jsonl
```

`--corpus` replaces the default gold inputs despite the `--suite gold` run label. The default CI gold comparison now includes this panel. Candidate-only cases are reported as added cases on introduction; subsequent comparisons gate newly failing or worse shared cases using the existing relative gate. Source-consistency results remain distinguishable by cohort and provenance, not independent national accuracy. This diagnostic set deliberately has no `must_pass` or `known_failure` labels. A baseline containing untriaged critical results fails the evaluator's absolute gate; retain and investigate those results rather than making them disappear.

## Reproduce

The generator uses Python 3.9+, Shapely 2.0.7/GEOS 3.11.4 for the interior mask, and `osmium-tool` (produced with osmium 1.19.1/libosmium 2.23.1). These are preparation tools, not runtime dependencies. Raw extracts and scratch files stay outside git.

To reproduce the case file byte-for-byte from the checked-in source-record snapshots, without network access or PBFs:

```sh
python3 tests/global/build-addresses.py \
  --replay tests/global/manifest.json --out /tmp/global-address-replay
python3 tests/global/adjudicate-gnaf.py apply \
  --corpus /tmp/global-address-replay/global-addresses.jsonl \
  --adjudications tests/global/adjudications.json --out /tmp/global-address-replay/adjudicated.jsonl
cmp tests/global/global-addresses.jsonl /tmp/global-address-replay/adjudicated.jsonl
```

The second step adds the [registry adjudications](#registry-adjudications); it is separate because the manifest binds the generator's hash to the capture.

To resample the original data, retain every PBF named by the manifest, its exact SHA256, the countryInfo.txt snapshot, and the pinned Natural Earth file. A `-latest` URL changes over time; downloading its current contents is a **new data experiment**, not a reproduction. `urls.txt` beside the PBFs records their public Geofabrik URLs, one per line.

```sh
python3 -u tests/global/build-addresses.py \
  --osm SOURCE/osm SOURCE/osm-landmarks \
  --boundaries SOURCE/natural-earth.json \
  --country-info SOURCE/countryInfo.txt \
  --work SOURCE/global-address-work --out SOURCE/global-address-output \
  --seed global-addresses-2026-10 --count 50
```

`--base tests/global/manifest.json` keeps the countries that manifest already holds as captured and samples only the others, appending their cases; the October 2026 additions were captured this way, so the first 48 countries' snapshots record the generator of commit `7a02601` and the rest this one.

Completed country checkpoints bind the source PBF hash/size, country-mask hash, boundary snapshot, country-label snapshot, generator hash, seed and quota. PBFs are checked again after reading. Changed inputs require a fresh working directory. Completed output is immutable; identical replay does not rewrite it. Four independent country workers stream nodes and checkpoint completed countries; progress is emitted during large scans. Source scanning can take minutes and needs scratch disk space for filtered nodes.

The boundary source is pinned to Natural Earth commit `ca96624a56bd078437bca8184e78163e5039ad19`, SHA256 `239eec57ac17f100a11e2536cffc56752c318b50ae765b0918ff7aab4ce8f255`. Source records and source versions remain reviewable through the OSM API URLs in each case.

## Sources considered

- [OpenAddresses](https://openaddresses.io/): useful government-derived data, but geographic availability and redistribution terms differ by source; it does not supply a uniform, independently licensed global reference.
- [Senzing's libpostal test data](https://github.com/Senzing/libpostal-data): broad multinational parsing tests, not building-coordinate ground truth.
- [MESSY STREETS](https://github.com/EdGaere/MessyStreets) and its [paper](https://arxiv.org/abs/2609.01612): useful verbatim-address work, but 92% North America/Europe in the reported sample, with street existence verified around kilometre-scale cells rather than civic-number/building truth.
- [Wikidata](https://www.wikidata.org/): remains useful independent-source evidence where suitable. The discovery pilot found many road-only descriptions and Plus Codes in address properties; physical-building membership plus a coordinate did not make those strings complete civic addresses. They were not promoted into house-level gold expectations.

These findings are why the panel retains its source-consistency provenance and separate cohorts within gold, rather than being presented as a universally authoritative global dataset.
