# Canada: OSM extract as an address source

Data experiment for rank 4 of the [Canada/Spain OSM triage](ca-es-osm.md): read the pinned Canadian extract, now landmarks-only, for addresses as well. **Outcome: keep Canada in `osmLandmarks`.** No source configuration, builder, scorer or expectation changes here. The one runtime defect the triage isolated for Canada (a bare `addr:district` part, `471 Upper Lakeville Road, 2, …`) is fixed separately in #509.

## Findings

- **Most Canadian OSM addresses are block-face range ends, not houses.** The extract has 1,337,695 `addr:interpolation` ways, 90.8% tagged `source=NRCan-CanVec`/`CanVec` (another 3.6% StatCan, 5.5% untagged). Their 2,643,584 end nodes carry `addr:housenumber`: **2,598,034 of the 4,127,413 addressed nodes (62.9%)**. They sit at road-segment ends, one per side (`40001`/`40002` and `40131`/`40132` Range Road 281, Lacombe County, 3.2 km apart), not on buildings.
- **The OSM panel samples them.** **29 of the 50 Canadian panel locations are interpolation end nodes** (23 of 40 training, 6 of 10 held-out, counted from node IDs only). No building-point source places a house at a road-segment end, so their at-point and 150 m checks measure agreement with a synthetic position. This explains much of the registry/OSM gap that [#497](ca-es-osm.md) could not attribute.
- **Reading the extract as it is nearly triples Canada's houses**: 13,415,455 → 38,929,703 house keys (+25.5 million). NAR's point frame has 10.7 million locations and 17.4 million address rows, so most of the added keys are numbers interpolated between range ends: civic numbers no source publishes. The panel rises to 80/100 search, but the independent registry gains nothing. It loses one check: a NAR house's reverse lookup now returns `536 Rue Notre-Dame Sud`, interpolated between OSM range ends 510 and 550, instead of the published `540` at the NAR point.
- **Without the interpolation, OSM adds little.** The extract minus interpolation ways and their end nodes adds 318,105 house keys (+2.4%). That recovers the panel's non-endpoint nodes (mostly businesses mapped by contributors) and nothing in NAR or in Canada's other gold evidence. It still fails the gate on OSM's own conflicts (below).
- **Licensing is not the blocker.** OSM is already an ODbL address source for the extracts in `osm`, with attribution in [NOTICE](../../../NOTICE), and CanVec/StatCan content entered OSM under its import terms. Admission was rejected on civic identity and evidence, not on licence.

## Inputs and identities

Everything is identical across the three indexes except the `--osm`/`--osm-landmarks` membership of the Canadian extract.

| Item | Version / fingerprint |
|---|---|
| Code | `main` at `2bccbde`, builder `ce904d2d446c68ba` (= `index.builderHash`) |
| Service / evaluator SHA256 | `01b2abaf…f6f1` / `16415e3c…9f50` |
| Models (evaluator digest) | `65627ea2114bb908…` |
| `data/reference.json` | `9a68f034f3b05f57…` |
| Overture | local `2026-09-23.1`; OpenAddresses, other OSM, GeoNames, Korean shops as in `~/opengeocoding-index/build.sh` |
| Canadian extract | `canada-latest.osm.pbf`, 6,518,798,779 bytes, SHA256 `1d144112007bd5a616ad86f16e9d10fe39305f3ce56d74d71712f5b77bc44f60` (the panel's capture), replication sequence 4943, 2026-10-15T20:21:00Z. Not refreshed |

| Index | Canadian extract read as | OSM rows | CA house keys | CA streets with a number | `meta.json` SHA256 |
|---|---|---:|---:|---:|---|
| Base | landmarks only (as `main`) | 95,491,883 | 13,415,455 | 546,927 | `d20ff09b…` |
| Full | addresses | 130,412,802 | 38,929,703 | 617,218 | `d0ac7ade…` |
| No interpolation | addresses, minus interpolation ways and their end nodes (`osmium removeid`), landmarks from the unmodified extract | 99,244,200 | 13,733,560 | 566,220 | `9ecf6f9c…` |

House keys are final deduplicated FST keys (`evaluate coverage`), not unique buildings. Landmark counts are identical in all three. The no-interpolation extract is a diagnostic: the chart cannot express it, and a builder rule would change the builder hash.

## Results

One request per case, `--no-direct`, same binary on each index. A repeat of the base gold run gave identical classes for all 46,427 cases. The only full-response difference was the tail order (ranks 4–5) of one US suggestion, `Riverside, CT`, whose rank-1 result and class did not change. Counts are search / suggest / reverse at point / reverse offset passes.

| Corpus | Base | Full | No interpolation |
|---|---|---|---|
| NAR registry, 373 locations, 550 queries | 525 / 507 / 397 / 477 | 525 / 507 / **396** / 477 | 525 / 507 / 397 / 477 |
| — training (304 / 450) | 431 / 415 / 332 / 396 | 431 / 415 / 331 / 396 | unchanged |
| — held-out (69 / 100) | 94 / 92 / 65 / 81 | unchanged | unchanged |
| Other CA gold evidence (places, people, GLEIF, towns) | unchanged | unchanged | unchanged |
| OSM panel, 50 locations, 100 queries | 32 / 34 / 28 / 68 | 80 / 90 / 70 / 98 | 52 / 56 / 42 / 80 |
| — training, range ends (23 / 46) | 14 / 14 / 10 / 28 | 38 / 42 / 32 / 44 | 14 / 14 / 10 / 28 |
| — training, other nodes (17 / 34) | 10 / 12 / 12 / 24 | 26 / 30 / 24 / 34 | 26 / 30 / 24 / 34 |
| — held-out, range ends (6 / 12) | 4 / 4 / 0 / 10 | 8 / 10 / 6 / 12 | 4 / 4 / 0 / 10 |
| — held-out, other nodes (4 / 8) | 4 / 4 / 6 / 6 | 8 / 8 / 8 / 8 | 8 / 8 / 8 / 8 |

Outside Canada no case changed. The OSM panel measures agreement with the source being added: these gains are consistency, not accuracy. Held-out cases are aggregated only.

### Gate failures (`evaluate diff`, base → candidate)

Full extract, 5 newly failing and 7 worse:

- NAR `540 RUE NOTRE-DAME S, LOUISEVILLE`: reverse at the NAR point returns `536`, interpolated between OSM range ends `510` and `550`, about 4 m away against the published 540's 6 m. A fabricated civic number displaces a real one.
- NAR `40004B RANGE ROAD 281, LACOMBE COUNTY`: too coarse → near miss. 40004B is in no source (the index has 40004A at the NAR point). OSM range ends give a `Range Road 281` street document in Gull Lake the stated postcode, so the street's middle, 3.8 km away, replaces the postcode's centre, 1.8 km away.
- Panel: `203 Avenue du Quai` reverse now returns `201`, interpolated between range ends 105 and 203; `2020 Highway 631` search now answers another `2020` copy 3 km north (near miss) and reverse returns `2029`; `7 Short Street, Spanish` reverse returns `4 Dugas Street`. `400 Merganser Drive West` is below.

No interpolation, 2 newly failing and 6 worse, all panel:

- `400 Merganser Drive West, Chestermere` (both forms): OSM itself holds two nodes 1 m apart, an ATM at `400 Merganser Drive West, T1X 1K5` and a fuel station at `400 Merganser Drive, T1X 1E1`. Reverse now returns the fuel station's postcode. This is a conflict inside the source, not a resolver defect.
- `2020 Highway 631` (both forms): too coarse → near miss. An OSM-only `Highway 631` street document moves the street answer.
- `2051 Amatal Street, Nisga'a` reverse/offset (both forms): no house → wrong house (`3000 Lisims Avenue`, an OSM business node).

Neither variant qualifies for the OSM-panel trade, because it loses outside the panel or loses through worse classes. Each loss above is a source-identity problem that tuning cannot hide.

## Training failures by cause

The 31 training locations failing on `main` (#497's ledger) and what the experiments establish. Neither experiment changes a held-out record or an expectation.

| Cause | Locations | Evidence |
|---|---:|---|
| Panel point is an interpolation range end | 18 | 10 recorded as unresolved, 3 parser (`Fairview No. 136, M.D. of`, `Cariboo F`, `Bulkley-Nechako F`), 2 locality (`Cariboo J`, `Cariboo I`), 2 reverse identity (`11 Wilmac Court`, `550 Chemin de la Pointe-Sphérique`), 1 alias (`Chemin Landeau Road`). Only importing the range ends themselves changes them. A BC electoral-area alias (`Cariboo J` → `Area J, Cariboo Regional District`) would find only `1018 Dean River Place`, 390 m from the range end: abstain → near miss. Not implemented |
| House only in OSM, as a contributor-mapped node | 6 | `500 Lac Des Mille Lacs Road`, `159 Ellen Road`, `5102 50 Street`, `43 Main Street, Shellbrook`, `4548 93 Highway`, `1101 Alaska Avenue`; all pass with the no-interpolation index. No independent source was available to confirm them |
| Street-name conflict | 1 | `23461 Fern Crescent`: OSM's street; Overture and OpenAddresses put 23461 on 132 Avenue, 18 m away |
| Bare district part (parser) | 1 | `471 Upper Lakeville Road, 2, …`: fixed in #509 |
| Locality qualifier (parser) plus point conflict | 1 | `95059 Highway 734, MD of Bighorn`: the indexed house is 208 m away. With the OSM node indexed, suggestions and reverse pass but search still abstains on `MD of Bighorn` |
| Source point or postcode conflict | 3 | `2995 2 Avenue West` (235 m), `1587 Kitwanga North Road` (Overture `V0J 1G0` vs OSM `V0J 2A0`), `2235 2nd Ave, Dunmore` (house 2.6 km away) |
| Reverse identity at a unit inside a building | 1 | `108 Jarvis Street`: the node is a business tagged `addr:unit=103`. The nearest indexed house is `2145 Second Avenue` |

Points and postcodes stay unadjudicated: NAR raw files were not available locally for these addresses, and no source was preferred to fit the panel.

## Unresolved and next steps

- **Panel frame.** The [global panel guide](../global-addresses.md) excludes interpolation lines, but their end nodes are sampled as addresses. Excluding nodes that end an `addr:interpolation` way would change the sampler and regenerate the corpus for every country. That is an evaluator experiment, kept separate from this one.
- **A narrower source rule.** Reading Canadian OSM without interpolation and its end nodes is the only form that does not fabricate numbers. Its independent benefit is unmeasured: zero in NAR's 550 queries and Canada's other gold evidence. The gate also fails on OSM's own duplicate addresses. It would need a builder rule (new builder hash), a decision for the Canadian nodes that end a contributor-drawn interpolation over real houses, and an independent sample of the OSM-only houses.
- **Not established.** Whether a range end's number is a real civic address, the true point of any conflicting house, and national coverage. These are 50 panel locations and 373 registry locations, not accuracy estimates. Speed was not measured (`--speed-cases 0`).
