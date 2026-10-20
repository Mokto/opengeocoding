# India: houses the parse loses

Part of [issue #222](https://github.com/Mokto/opengeocoding/issues/222). India's OSM addresses were already indexed (`osm`), so the failures came from handling the queries, not from missing data. Only the non-held-out panel groups were read.

## What failed

Of 81 non-held-out Indian panel queries, 35 passed. The index held the house in most of the failures, and `SuggestAddress` often listed it first, but libpostal misread Indian civic identifiers. `D-112 Shastri Nagar` and `CK43/103 Dalmandi Road` parse as one road. `001 madhosinghana, sirsa, v.p.o madhosinghana` parses as a house name. With no road, SearchAddress fell back to the postcode.

## Change

When the answer has no house, SearchAddress now asks the completion lookup (which reads no parse) for the query's houses. It accepts the house only when the query states the house's number as one of its words, contains every word of the house's street, and contains the house's postcode or town (city, town, village, municipality, suburb, city district or hamlet; a county is not enough). The house must also be in the answer's country, or in the country the query names when there is no answer. A stated postcode that differs from the house's rules it out. Each answer considers up to five suggestions; if those that pass these checks lie more than 200 m apart, no house is chosen. The fallback only runs when the query has a word containing a digit besides the answer's postcode.

## Measurement

Local world index, builder `1929034e0fce080d`, base `main` at `70cb37b`, one request per case, all 21,282 gold cases:

- SearchAddress 17,790 → 18,077 passes. SuggestAddress, ReverseGeocode and ResolveRegion are unchanged, and no case is newly failing.
- India 72 → 88 / 143 checks. Other markets: UAE +32, Romania +27, Saudi Arabia +18, Thailand +17, Nigeria +15, Ireland +14, Malaysia +13. The registry cohorts also gain (Spain, New Zealand, Canada, Switzerland, France and Poland).
- **Three cases have a worse failure class** (too coarse → near miss): `2 Avenue West 2995, Prince Albert`, `Calle Centenario 505, Calvillo` and `255 중앙로, 순천시`. Each answer now names the same house that its other-order query already answered on the base. The index places that house 0.2–0.5 km from the OSM node. This is a disagreement between sources, not a different house, but it still fails the gate.
- The local speed section is not evidence: SuggestAddress and ReverseGeocode, which this change does not touch, slowed as much as SearchAddress while other jobs ran on the machine.

## Remaining Indian failures

The largest remaining group is house numbers the source stores as several words: `Plot No.14`, `Survey No.49`, `GAT No. 720`, `flat 1`, `1st floor`, `Ration Shop 54`. Malaysia (`Lot 25`, `PT 421`), Thailand (`367 หมู่ 1`), the Philippines (`Purok 6`) and Indonesia (`RT 02/04`, `Blok I4`) show the same pattern. A whole-word number check does not match these numbers, and the completion lookup does not suggest them. Some of them are local identifiers, while floors and shop labels are unit information that the source tags as house numbers. They are left for a separate change.

Pakistan is not in the panel, and no other corpus covers it, so this change makes no claim about Pakistan.
