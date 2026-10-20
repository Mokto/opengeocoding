# Japan in the global address panel

Triage of `global_osm_jp` (50 OpenStreetMap address groups, 100 queries; 40 groups train, 10 held out) from [issue #222](https://github.com/Mokto/opengeocoding/issues/222), where Japan passed 0/100 searches and 0/100 suggestions. These are [source-consistency checks](../global-addresses.md), not a Japanese accuracy rate. Only training cases were inspected.

## Index coverage

The chart read Japan's OSM extract for landmarks only (`osmLandmarks`), so Japanese addresses came from Overture and OpenAddresses' Address Base Registry layer. That layer holds residence-indication (住居表示) blocks, filed as a chōme and a block-house number (`鶴田一丁目`, `2-26`), with no postcodes; lot-numbered (地番) addresses, most of rural Japan, are not in it. `evaluate inspect` found a stored house with the node's block-house number within 150 m in only 6 of the 40 training groups, each under the registry's chōme or kanji town name (`帯屋町二丁目` `1-27`, 4 m from a node tagged `帯屋町` `２丁目１−27`; `古川町壱之町` `3-12` for `Furukawacho Ichinomachi` `3-12`). The other 34 had no such house.

Japan is now read for addresses (`osm` instead of `osmLandmarks`). A Japan-only build reads 340,670 OSM address rows beside 22.0 million registry rows; landmarks are read from the same extract as before.

## Measurement

Local full index (Overture 2026-09-23.1 with the local OSM, OpenAddresses and GeoNames GB/CA/NL inputs of `~/opengeocoding-index/build.sh`), builder `9d4006a4eec15abb`, consumer `7a02601`, one request per case. The candidate index differs only by Japan's extract in `--osm`; both use the same landmark files.

| Index | Search pass | Train | Held out | Suggest pass | Critical |
|---|---:|---:|---:|---:|---:|
| Without Japan's OSM addresses | 0/100 | 0/80 | 0/20 | 0/100 | 3 |
| With them | 38/100 | 30/80 | 8/20 | 64/100 | 0 |

Across the full 21,282-case gold suite only Japanese cases changed: SearchAddress 16,856 → 16,894 passes, SuggestAddress 15,013 → 15,077, ReverseGeocode and ResolveRegion unchanged, no newly failing case. One registry case (`7-1, Nagatachō 1-chōme, …`) moved from Chiyoda's locality to its postcode, still too coarse. Two Japanese rows of `tests/addresses.tsv` changed: `5-6 Nampeidai-cho, 150-8511, Tokyo` now answers its postcode instead of nothing, `1-9-2 Higashi-Shinbashi Minato-Ku, 105-0021` its postcode instead of Minato.

Four panel results changed failure class for the worse:

- `帯屋町 ２丁目１−27, 〒780-0841`: abstain → `wrong_component`. The answer is the registry's `1-27` in `帯屋町二丁目`, 4 m from the node: the same building, written in the chōme form. The expectation compares civic strings.
- `日光街道 宇都宮市茂原２丁目5-1022, 321-0145`: abstain → `wrong_component`. The answer is the node itself, 2 m away; its number differs from the expected one only by the ASCII `2` the service writes for every full-width digit.
- `311 長野県道58号長野須坂インター線 (Nagano Suzaka IC line), 須坂市` and its street-first variant: too coarse → near miss. The node states 須坂市, but the street it is indexed on lies in the neighbouring town; the answer is the road, 4.4 km away, with `goodMatch` false. A source-context conflict, not adjudicated here.

## Remaining failures

With the addresses indexed, the training failures fall into:

- **Parser**: an unspaced kanji or romaji name before or after a plain number (`828 今津町福岡`, `Koyodai 258-2`) is cut into unrelated fields, and a separate district part (`肝属郡, 肝付町`) joins the town. Fixed separately from this data change.
- **Source numbers that are not house numbers alone**: a municipality or district in the number (`宇都宮市鶴田1-2-26`, `広島市安佐南区山本３丁目６−1`, `葛巻第5地割170-2`, `山科字前田3399`, `梅ヶ島4269-10`), a block name after it (`739-1 一場御園`), a kana prefix (`オ9`), the kanji `一` as a dash (`１１６一１`, `９０一１`, `１丁目１一５`) and a chōme that contradicts the street (`3 Chome-2-7` on `神楽四条四丁目`). Several resolve to the registry's building; they pass only if the answer repeats the source's string, so neither the expectations nor the resolver were changed for them.
- **Roads as streets**: OSM tags many Japanese addresses with the nearest road (`国道281号`, `県道136号`) rather than the town, which Japanese addresses do not use; a query naming the town finds the registry's houses instead.
