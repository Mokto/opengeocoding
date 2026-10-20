# Taiwan: Taiwan Province and numbers after a road

Part of [issue #222](https://github.com/Mokto/opengeocoding/issues/222), after [Taiwan's OSM addresses were indexed](osm-address-countries.md). Only the non-held-out panel groups were read.

## What failed

Most of Taiwan's remaining panel failures were abstentions, and nearly all of them wrote `台灣省` (Taiwan Province) between the county and `Taiwan`. Taiwan Province exists in name only. No place of the index has it, and libpostal takes it for the town, so nothing matched. The same query without it finds the house: `廈門街 21號, 屏東市, 屏東縣, Taiwan` is house 21 at the panel's point.

libpostal also mishandles two forms of number:

- A number written after its road with a space (`中正路63巷 22號, 甲仙區, 高雄市`) is read as part of the road.
- A number after a village's name (`佳崙 386號`) is read as a unit.

## Change

- **Taiwan Province.** When a query that writes Taiwan Province (`台灣省`, `臺灣省`, `台湾省` or `Taiwan Province`) as one of its parts finds nothing, it is resolved again without that part. That answer is kept only when it is confident (`goodMatch`). A part that is only the province stays, and so does a name merely containing it (`台灣省政府`).
- **Numbers.** In the parse, a word of digits ending in 號/号 ("number"), optionally with 之 (`8之3號`), becomes the house number when the parse has none. This applies when the word is the first or last word of the road, or when it was read as a unit. 號 says only that the word is a number.

## Measurement

The index was the local world index (builder `1929034e0fce080d`) with the OSM addresses of Japan, Taiwan, Chile, Colombia, Mexico and Portugal added, as the chart indexes them. Base was `main` at `2121761`, with one request per case over all 21,282 gold cases.

| | Base | Change |
|---|---:|---:|
| SearchAddress, Taiwan | 654 / 725 | 707 / 725 |
| SearchAddress, all | 18,229 | 18,282 |

No other answer changed, no case is newly failing or worse, and the other RPCs are unchanged. The evaluator's local speed section reported no slowdown over 10%.
