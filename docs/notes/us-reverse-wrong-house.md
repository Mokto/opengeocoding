# US reverse geocoding: wrong selected houses, 2026-11-06

Follow-up to the [locality work](us-pl-reverse-locality.md), which left the selected house unchanged. This note looks only at the house a ReverseGeocode answer selects. It does not cover locality names: #532, #537, #539 and #543 are not reopened.

## Inputs

Base: main `8af6538`, builder `fb1f7989c99c19e4`. Candidate builder: `ca1194896cfafd5d`. Both indexes were built from the same Overture `2026-09-23.1`, OpenAddresses, OSM, landmark, Korean shop and GeoNames inputs (`build.sh`'s input list). The candidate read an APFS clone of those inputs. Every number FST, `places.tsv`, `areas.bin` and every sidecar is byte-identical between the two indexes. Only `reverse.bin` and `streets.bin` differ, and `streets.bin` differs only by document addresses. One evaluator binary, model set and reference data were used for every run. A repeat of the base run had zero answer or class drift on all 47,898 cases.

Every training reverse case listed below was traced with `evaluate inspect`, which now also lists each street's far copies of its numbers, and against NAD r24 and the Overture rows near its point. Held-out cases are reported as aggregates only.

## What the US wrong-house cases are

These are unique training locations, at-point checks. Offset checks have no US wrong-house failure other than missing houses.

| Cause | NAD | OSM panel | Curated |
|---|---:|---:|---:|
| **The house is in the index, but ReverseGeocode could not see it** (far copy, below) | 1 (Houlton, ME) | 0 | 0 |
| Several numbers at one source point: NAD itself places 2106/2206/2306 Cherry Blossom Drive (Dardenne Prairie), 5104/5112/5120 Oaks of St Clair Circle (Moody), eleven numbers of 1278–1290 Beacon Street (Brookline) and 14/16 Boylston Street (Pittsfield) at one point each. The selected house stands at the expected point, and the tie goes by street id and number. | 4 | 0 | 0 |
| A unit's own point is nearer another building's number than its own. The builder averages a number's unit points within about 300 m into one point. The index has no unit points (Asheboro, Tumwater, Liberty, Park Hill, Centralia, Manchester, Niagara Falls, Evansville). | 8 | 0 | 0 |
| The house is missing from the index (Lakeland, Hollywood ×2, Arlington, Flemington; no_house controls). | 5 | 2 + 5 no_house | 0 |
| Sources disagree on the point or the street: Overture's older NAD copy has 19406 Dewberry Drive at NAD r24's point for 19432 (Bristol, VA); a parcel point 180 m from NAD's rooftop (Santa Barbara); OSM nodes 10–370 m from the register's copy (Greeneville, Russia, Uvalde, Mansfield); and OSM "401 West 2nd Street" against the register's "401 West 1st Street" (Clarendon). | 2 | 5 | 1 (12140 US Highway 377, 974 m from the Census point) |

Of these, only the far copy is a selection defect. The co-located numbers are an expectation that the source point cannot support: any rule picking 2206 over 2106 at their shared point would be fitted to the sample. Unit averaging is a deliberate representation (`schema.rs`: "the middle of them beats an edge"), and the [registry guide](../evaluation/registry/us.md) already records it as reference data. In all of these cases the nearest stored house is the one answered. A mailing city alone was not counted as a wrong house.

## The fix: far copies of a number

When one source has a street's number twice, more than about 300 m apart (two villages' "Calle Mayor 3", or a stale and a current register row), the builder keeps the second copy in `numbers-far-NN.fst`. Search already answers it. ReverseGeocode did not. `reverse.bin` posted only the first copy's grid cell, and `reverse_houses` read only the first FST. In Houlton, NAD has 63 Green Street both as a 2022 row 734 m away and as units 1 and 2 at the case's point. The index kept the far one first, so the point answered 61 Court Street, 36 m away.

- `schema.rs` now posts the far copy's cell too, which changes the builder hash.
- `reverse_houses` reads both FSTs.
- A far copy ranks 1 m behind the stored houses. On the first run without this slack, every training far copy that won by less than 0.5 m was a stored house under another key at the same point: č.ev. 30's bare 30, 85a's bare 85, "233" for "233-247", a place-named street ("3 Bruges" on 3 Jagersdreef). That run also had one held-out regression with the same 0 m shape. Every training far copy that won by 6.9 m or more was a different house.

## Results

On the full gold suite (same evaluator, models and inputs), at-point reverse goes from 40,035 to 40,087 of 47,153 passing checks and offset from 36,801 to 36,819 of 39,520. Nothing is newly failing or in a worse class. The other RPCs are unchanged. Most gains are global OSM-panel and registry cases in other countries (Kuwait, Egypt, Iceland, Spain, Poland, Germany, ...). In the US, NAD training at-point goes from 333 to 334 of 427 locations (Houlton, two checks). US held-out aggregates do not change: NAD 94/123 at-point and 98/123 offset, OSM 8/10, curated 23/23. Across all countries, held-out reverse changes are 12 checks to pass at-point (10 wrong_house, 2 no_house) and 4 at offset. No held-out check gets worse.

Pass-to-pass answer changes:

- A strictly nearer far copy at offset points (Houlton now answers 63 Green Street there).
- Three street-only labels at tied points. The base service on the candidate index reproduces these, so they come from independent builds' document order, not from this change.

The selected house's locality naming is unchanged. A house newly found through its far copy is named from its own point, like any other house.

`tests/synthetic/world/us.rs` builds a street holding 12 twice, about 540 m apart, with a neighbour 20 m from the far copy. At the far copy the answer must be 12 at its own point. The first copy must still answer at its own point. A point 5 m from the neighbour must answer the neighbour. A stored house 0.3 m from another far copy must answer there. Dropping the posting, the reverse read or the slack each fails this test.

## Not fixed here

- Co-located source numbers: they need an evaluator decision (accept any number at the source point, or sample differently), not a runtime rule.
- Unit points: answering a unit's own point would need unit points in the index, which is a size and representation change.
- Missing houses and source conflicts belong to the source data, not to the runtime.
