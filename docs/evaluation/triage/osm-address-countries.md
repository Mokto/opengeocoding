# OSM addresses for Chile, Colombia, Mexico, Portugal and Taiwan

Follow-up to [Japan](japan-addresses.md) for [issue #222](https://github.com/Mokto/opengeocoding/issues/222). These five countries were in `osmLandmarks`: their OSM extracts were read for landmarks only, because Overture's address theme lists them. Overture's sources there are partial (cities, some states or a subset of the national register), and the five scored 0–21/100 in the global panel. They are now read for addresses (`osm`).

## What OSM adds

Houses in the index (final house-number keys) in a build of each country's bounding box, with and without its extract; the same build otherwise:

| Country | Without | With | Added |
|---|---:|---:|---:|
| Chile | 3,711,831 | 6,435,743 | +73% |
| Taiwan | 4,291,271 | 5,451,138 | +27% |
| Portugal | 2,366,761 | 2,694,804 | +14% |
| Mexico | 16,181,825 | 16,537,074 | +2% |
| Colombia | 8,550,571 | 8,591,022 | +0.5% |

The panel samples OSM's own address nodes, so its gains partly measure the source against itself. The registry cohorts do not: Taipei's register gains 112 passes, Bogotá's 5.

## Measurement

Local world index (the inputs of `~/opengeocoding-index/build.sh`, Japan already in `osm`), builder `9d4006a4eec15abb`, one request per case, same landmark files on both sides. Base: `main` at `8efeafa` on the index without these extracts; head: this change on the index with them.

| Country | SearchAddress | SuggestAddress |
|---|---:|---:|
| Taiwan | 511 → 654 / 725 | 520 → 652 / 726 |
| Chile | 22 → 93 / 106 | 23 → 91 / 109 |
| Colombia | 19 → 89 / 690 | 32 → 126 / 703 |
| Mexico | 42 → 103 / 126 | 46 → 114 / 140 |
| Portugal | 52 → 105 / 106 | 54 → 106 / 108 |

Across all 21,282 cases: SearchAddress 16,976 → 17,403, SuggestAddress 15,133 → 15,547, ReverseGeocode and ResolveRegion unchanged, no newly failing case. Of the search gain, 35 come from the two changes below on the old index alone.

## Changes that came with the data

- **A house number that is also a postcode.** Taiwan's three-digit district postcodes exist once OSM's postcodes are indexed, and `511 林森路二段, 虎尾鎮` lost its house: the lookup took `511` for the postcode and answered a same-named street without it. A number written in its road's part of the query now stays the house (before, only a US state kept it).
- **Scoring `118號` as 118.** The evaluator compared house numbers as written, so the source's `118號` or `１１８` failed against the service's `118`, the index's documented key (`number_key`: full-width digits as ASCII, no `號`/`号`). House numbers are now compared under that key. Re-scoring the base run changes 27 classes: 19 Chinese and 7 Taiwanese panel cases `wrong_component` → pass, and one Japanese case (`宇都宮市茂原２丁目5-1022`, the right house without `goodMatch`) `wrong_component` → `confidence_mismatch`.

## Remaining worse results

Three Colombian cases changed failure class for the worse; none of them is a wrong place:

- `Carrera 3 # 28-59, Montería 230002`: too coarse → near miss. OSM's addresses give a Carrera 3 street document the postcode 230002, and streets holding the stated postcode come first; that segment is 3.3 km from the gold point. Before, the answer was Montería itself. The `# 28-59` that places the house near Calle 28 is not modelled.
- `Km 6 El Encanto de Guanapalo, San Luis de Palenque, 853039` (both orders): too coarse → near miss. The answer is now the postcode, 6.5 km from the node, instead of the town 19 km away; a postcode's radius is tighter than a town's.
