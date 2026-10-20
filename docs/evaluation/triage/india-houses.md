# India: houses the parse loses

Part of [issue #222](https://github.com/Mokto/opengeocoding/issues/222). India's OSM addresses were already indexed (`osm`), so the failures came from handling the queries, not from missing data. Only the non-held-out panel groups were read.

## What failed

Of 81 non-held-out Indian panel queries, 35 passed. The index held the house in most of the failures, and `SuggestAddress` often listed it first, but libpostal misread Indian civic identifiers:

- `D-112 Shastri Nagar` and `CK43/103 Dalmandi Road` parse as one road.
- `001 madhosinghana, sirsa, v.p.o madhosinghana` parses as a house name.
- Numbers of several words, such as `Plot No.14`, `Survey No.49` or `GAT No. 720`, are split.

With no road, SearchAddress fell back to the postcode. Malaysia (`Lot 25`, `PT 421`), Thailand (`367 หมู่ 1`), the Philippines (`Purok 6`) and Indonesia (`RT 02/04`) show the same patterns.

## Change

When the answer has no house, SearchAddress looks for the house the query names:

- **Answer is a postcode:** the completion lookup, which reads no parse. If that finds nothing, the postcode's streets are searched for a number of several words written in the query.
- **Answer is a street:** the same-named streets that hold the postcode. When the parse read no house number, same-named streets within 5 km are searched too.
- **Answer is a town, or there is no answer:** the completion lookup, but only when the parse read no house number.

Town answers whose parse found a number get no lookup. They are the most frequent answers without a house and the least often recovered, so leaving them out keeps the cost low.

A house is accepted only when all of these hold:

- Its number is written in the query, as one to four whole words of one part.
- Every word of its street is in the query.
- The query contains its postcode or town (city, town, village, municipality, suburb, city district or hamlet; a county is not enough).
- The words beside the number belong to its street or its places, and at least one belongs to the street. `Heyrovského č.p. 470, Olomouc` is not house č.p. 470 of a street named Olomouc, and `Dublin 4` is a district, not house 4.
- Any other number in the query is explained: it is the postcode or part of the street.
- It is in the answer's country.

A stated postcode that differs from the house's rules the house out. If the houses found are not one house within 200 m, no house is chosen. A number of several words is not searched for when its words belong to the answer's road, or when it contains the postcode (`TX 75460`).

**Scoring change.** The evaluator now compares a Czech house number with RÚIAN's type mark (`č.ev. 346`) as the number (`346`), just as it already compares `118號` as 118. Re-scoring the base run changed no class. On the change's run, 64 `cz_evidential` cases moved from `wrong_component` to pass, all within 150 m of the registry point.

## Measurement

The index was the local world index (builder `1929034e0fce080d`) with the OSM addresses of Japan, Taiwan, Chile, Colombia, Mexico and Portugal added, as the chart indexes them. Base was `main` at `70cb37b`, with one request per case over all 21,282 gold cases.

- SearchAddress passed 18,226 → 18,669 cases. SuggestAddress, ReverseGeocode and ResolveRegion are unchanged, and no case is newly failing.
- India 72 → 102 of 143 checks. Elsewhere: Czechia +52, Malaysia +40, UAE +31, Philippines +31, Thailand +30, Romania +23, Japan +20, Nigeria +20, Saudi Arabia +18, Vietnam +11, Colombia +10, Indonesia +10, China +9, Mexico +9.
- **Two cases have a worse failure class** (too coarse → near miss):
  - `Calle Centenario 505, Calvillo`
  - `255 중앙로, 순천시`

  Each now names the house its other-order query already answered on the base. The index keeps one point for each house number of a street, merging copies from different sources. That point lies 0.2–0.5 km from the OSM node. This is a disagreement between sources, not a different house, but it still fails the gate.
- **Speed.**
  - CI, without the town lookup: SearchAddress −0% total, −1% p90 (gold-speed passed).
  - A first version that used the completion lookup for every answer without a house measured +17% p90 in CI.
  - Locally, on answers without a house that contain a digit, the town lookup adds 0.054 ms on average, against 0.035 ms without it and 0.137 ms for the first version.

## Remaining Indian failures

- Numbers whose label is also a word of the street.
- Floors and shop labels that the source tags as house numbers (`1st floor`, `Ration Shop 54`).
- Answers at town level whose parse found a number, where no lookup runs.
- Streets the index lacks.

Pakistan is not in the panel, and no other corpus covers it, so this change makes no claim about Pakistan.
