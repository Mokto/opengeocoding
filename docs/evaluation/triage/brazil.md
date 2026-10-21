# Brazil

Follow-up to the nightly pass rate by country ([#256](https://github.com/Mokto/opengeocoding/issues/256)), where Brazil passed 46 of 116 SearchAddress and 55 of 135 SuggestAddress checks. 100 of those searches are the `global_osm_br` panel (50 groups).

## Cause

Brazil's OpenStreetMap extract was in `osmLandmarks`, so it was read for landmarks only, and the index's Brazilian houses came from Overture and OpenAddresses. Most failing panel cases found the street but not the house (`too_coarse`). It now moves to `osm`. In the local world index, Brazil gains 1,774,267 houses (54,554,868 → 56,329,135, +3.3%, `evaluate coverage`) and 31,048 numbered streets.

The panel samples OSM's own address nodes, so this gain partly measures the source against itself. No Brazilian registry cohort exists.

## Changes that came with the data

- **A town-wide postcode does not refuse a house.** OSM gives many small towns' houses the municipality's single CEP (`47820-000`, São Desidério), and other sources give the same house a finer one (`47821-030`). With both on one street, the rule that refuses a house whose postcode contradicts the requested one (#156, "10 Rue de Rivoli, 75001 Paris") dropped the house and answered the postcode's centre, 42 km away. The rule now applies only when the requested postcode is centred within 3 km of the house, the radius at which the lookup already gives a house the requested postcode. Rivoli's 75001 is centred about 2 km from number 10. The house is answered with its own postcode and without `goodMatch` (`wrong_component`, as before the data).
- **CEPs without the old dot.** Some sources write a CEP in its older form, `45.160-000`. The index keys it as `45160000` either way, but displayed it as stored. A postcode of that exact shape is now shown as `45160-000`. Before, `matching::postcodes_compatible` also took the two forms for different postcodes, which is how "20 Rua Jerusalém, Belo Campo, 45160-000" passed before the data.

## Results

Local world index (the inputs of `~/opengeocoding-index/build.sh` with the chart's `osm` and `osmLandmarks` lists), builder `e8c4a1473b15ae4a`, one request per case. Base: `main` at `d17f3b5` on the index with Brazil in `osmLandmarks`; head: this change on the index with Brazil in `osm`.

| RPC | Base | Head |
|---|---:|---:|
| SearchAddress, Brazil | 46 / 116 | 100 / 116 |
| SuggestAddress, Brazil | 55 / 135 | 97 / 135 |
| SearchAddress, all | 18,865 / 21,203 | 18,919 / 21,203 |
| SuggestAddress, all | 16,169 / 21,287 | 16,211 / 21,287 |

ResolveRegion and ReverseGeocode are unchanged, and only Brazilian cases changed. No case is newly failing or got a worse failure class. Two got a better one: "3670 Rua Joinville, Itália, São José dos Pinhais, 83020-000" (both orders) is now the postcode, 1.5 km from the node, instead of house 3670 under another postcode 74 m away (`wrong_component` → `too_coarse`).

## Still failing

Brazil's 16 failing searches, none triaged further:

- `near_miss` (5): two houses indexed 306 and 359 m from the node (both orders of each), and a postcode 13.6 km away.
- `too_coarse` (4): Rua Joinville's postcode (both orders), and two towns.
- `wrong_component` (2): São Desidério's house 170 under its other postcode, above.
- `abstain` (3).
- Two Wikidata addresses in São Paulo (`Avenida Paulista, nº 1824`, `Rua Álvares Penteado, 49`) answer a street point 7.6 and 13.5 km away (`near_miss`).
