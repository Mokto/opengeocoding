# Finland in the global OSM address panel

Triage of the Finnish rows in [#222](https://github.com/Mokto/opengeocoding/issues/222) (search 78/100, autocomplete 50/100 on #219's head), on the 100 `global_osm_fi` cases (50 groups, 8 held out). Inspection (`evaluate inspect`) and probes used only the 42 tuning groups; held-out groups were scored, not inspected.

## Autocomplete at 50/100 was fixed earlier

On main `2121761`, before this change, SuggestAddress already passes 78/100, the same cases as search. #219's head did not yet include #215, which stopped a region named with the country's name as a later word ("Southwest Finland", "Central Finland") from discarding a trailing "Finland" (the defect behind Austria's 0/100, see [Austria](global-at.md)), nor #243.

## Cause of the remaining failures

Every Finnish house in the index came from one source, Overture's `fi/countrywide` addresses (3.66 million rows). Finland's OpenStreetMap extract was read for landmarks only (`osmLandmarks`). Of the 10 failing tuning groups (20 queries):

| Class | Groups | Evidence |
| --- | ---: | --- |
| Number not in the index | 7 | The street is indexed within 150 m, without the number: Ylä-Satakunnantie 12 (11, 13 and 15 are), Kuusamontie 150 (152 is 19 m away), Muurlantie 340 A (340 is 5 m away), Niemeläntie 3 (3a–3c), Vaarannivantie 35 (35a, 35e), Maaninkavaarantie 3 (3a–3c) and Aarnitie 1 (a street without houses). Search answers the street (`too_coarse`, `near_miss` for Vaarannivantie's 7 km street point). |
| Point disagreement | 2 | Haravamäentie 16 is indexed 220 m from the OSM node, Saukontie 23 162 m away, both rural. |
| Source postcode | 1 | Lauttijärventie 1, Siikainen: the OSM node has postcode 29180, which GeoNames gives to Peränkylä (Eurajoki), 60 km away; Siikainen's is 29810, which the index's house 1 (20 m from the node) has. Search answers that house without `goodMatch` (`wrong_component`). Likely transposed digits in OSM, not adjudicated against Posti's register. |

Letters are not stripped and nearby numbers not substituted; 3 is not 3a.

## Changes

- **OSM addresses for Finland.** Its extract moves from `osmLandmarks` to `osm`. It holds 1.13 million address rows. In the local world index, Finland gains 66,110 houses (2,151,647 → 2,217,757, `evaluate coverage`) and 3,759 numbered streets. Åland, which Overture's addresses miss, goes from 0 to 1,851 houses. OSM's `addr:city` is the postal town, so a house filed under its municipality also answers its postal town ("Hennalantie 10, Pertunmaa" is in Mäntyharju).
- **Suggestions for a letter after a space ([#248](https://github.com/Mokto/opengeocoding/pull/248)).** OSM keys some houses as written, `340 A`, where Overture has `3c`. SuggestAddress tried `340 A` only as `340a`, so it suggested nothing for "Muurlantie 340 A, Muurla, 25130, Finland" although search found the house. When no other reading of the text finds a street, it now also tries the number with its space, and still tries `340a` when that finds no house. 17,466 of the house numbers in Finland's extract are written so (a number, a space and one letter), and Sweden's extract, already read for addresses, has them too.

The panel samples OSM's own address nodes, so the coverage gain partly measures the source against itself. No Finnish registry cohort exists yet.

## Results

Local world index, builder `1929034e0fce080d` (Overture 2026-09-23.1, OpenAddresses, OSM, GeoNames), one request per case. Base: main `2121761` on index `210076085b5e`. The index with Finland's extract read for addresses is `8cf16e8b2571`; it differs only by that extract, which it also reads for landmarks (production already does; no landmark case changed). The data change and the code change were measured separately.

| `global_osm_fi` | Base | Finland's OSM addresses | and the suggestion fix |
| --- | ---: | ---: | ---: |
| SearchAddress, tuning | 64/84 | 78/84 | 78/84 |
| SearchAddress, held out | 14/16 | 14/16 | 14/16 |
| SuggestAddress, tuning | 64/84 | 76/84 | 78/84 |
| SuggestAddress, held out | 14/16 | 14/16 | 14/16 |
| ReverseGeocode country | 100/100 | 100/100 | 100/100 |

Across the 21,282-case gold suite:

- Finland's extract: SearchAddress +14 and SuggestAddress +12, all Finnish panel cases, with no newly failing or worse case. Four passing cases name the postal town instead of the municipality, and one Norwegian postcode point near the border moves about 0.1 km.
- The suggestion fix, on the base index: SuggestAddress +2 (Sweden's "Finntorp 6 A, GYTTORP, 713 72"), nothing else changed. On the index with Finland's extract it adds the two Muurlantie queries as well.
- For all Finnish cases, search goes from 87 to 101 of 109 checks and suggestions from 90 to 104 of 112.

Local timings were taken while another index was building and are not a speed measurement.

## Native address forms

Finnish addresses are written `Haarlantie 1, 25500 Perniö`, without the country. Five such forms of each tuning group, built from the same source tags (with and without the country, without the comma, without the postcode, without the town): 163 queries.

| | Base | After both changes |
| --- | ---: | ---: |
| SearchAddress | 119/163 | 150/163 |
| SuggestAddress | 121/163 | 151/163 |

Besides the 3 groups above, this leaves:

- "Leipojantie 1, Ii": libpostal reads `1, Ii` as house number `1 ii`, a Roman numeral, so nothing answers. With the postcode or the country it is found.
- "Muurlantie 340 A 25130 Muurla", without commas: SuggestAddress does not see where the town starts after a letter. Search finds the house.

## Remaining panel failures

The 3 tuning groups (6 queries) above, the point disagreements and the source postcode, are source conflicts, not resolver defects. One held-out group (2 queries) also fails, and was not inspected.
