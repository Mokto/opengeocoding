# Australia in the global OSM address panel

Triage for [#222](https://github.com/Mokto/opengeocoding/issues/222). The issue's Australian counts (search 32/100, autocomplete 0/100) were measured on #219's head, which did not yet include #215 (Australian units, state codes and suburbs) or #213. On main `7a02601` with the same local full index (builder `9d4006a4eec15abb`), the 100 Australian queries pass **58/100 searches and 60/100 suggestions**; reverse country passes 100/100 and there are no critical results.

## Defect fixed

libpostal reads a street written before its number ("Panorama Place 3, Maleny", "Railway Terrace 9, Mount Pleasant") as a venue. The index tried the village rule first, which looks the town up as a street and accepts any street the name search returns: "Maleny" found house 3 on Maleny Vista, a good match at the wrong street. The venue rule now runs first, a venue that ends in a street type gives up its trailing number, and the village rule only accepts a street named exactly as the town (or an unspaced name such as 帯屋町二丁目). With the fix the panel passes 60/100 searches and 60/100 suggestions; across the whole gold suite 10 cases newly pass, one wrong house becomes a locality, and nothing gets worse.

## Remaining failures (40 queries, 20 address groups)

Each was checked against the August 2026 G-NAF release. Held-out groups were adjudicated but not used to choose the fix.

| Class | Groups | Queries | Evidence |
| --- | ---: | ---: | --- |
| Index answers G-NAF's point; the OSM node is elsewhere | 16 | 32 | The exact number and street are found at G-NAF's default geocode, 0.19 to 13.7 km from the OSM node. Most are rural numbers on highways and long roads, where G-NAF's point is the property or building centroid (`PC`/`BC`). One alternate-order query (Sturt Highway 14382) answers the street, not the house. |
| Number not in G-NAF | 2 | 4 | 18 Rose Street, Manjimup and 3871 Old Charleville Road, Quilpie: no such address in G-NAF. The street fallback is the best available answer. |
| Range in the source | 1 | 2 | 155 Horace Street, Sea Lake: Vicmap holds 151-157; G-NAF's 155 has confidence -1 (in no contributor dataset). Matching a number inside a range is not done. |
| Leading-zero source number | 1 | 2 | OSM "01692" East West Road: G-NAF's 1692 is 4.1 km from the OSM node, so even an exact match would miss the 150 m radius. |

The 16 point disagreements are source conflicts, not resolver defects: the panel's 150 m radius around an OSM node does not hold for rural Australian addresses whose authoritative point is the parcel. One held-out case, 1 Manuka Road, Logan Village, answers the point G-NAF gives its 200 units (192 m) rather than the building record without a unit (71 m); it was not tuned.
