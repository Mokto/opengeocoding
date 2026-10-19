# House-number coverage

[Documentation](../README.md) · [Evaluation](README.md) · [Index building](../guides/indexing.md)

This historical audit records coverage counts and source decisions for specific builds, not current worldwide address completeness.

`evaluate coverage --index DIR [--population countryInfo.txt] [--out FILE]` reads a built index (no service, no libpostal; 12 seconds for the world) and writes one row per country: house numbers, streets, streets with at least one number, and, with GeoNames' `countryInfo.txt` (CC-BY-4.0), population, houses per 1,000 people and an estimated gap, the houses missing to 0.3 per person. The rows are ordered by that gap. 0.3 is a round figure between the indexed French addresses (0.38 per person) and German addresses (0.26); countries of large apartment blocks or large households need fewer addresses per person, so their gap is overstated, and a country can exceed it (Argentina's interpolated block ranges give 1.0). It ranks countries; it is not a completeness measure. Counts are final deduplicated FST keys, not input rows or unique buildings; interpolation counts too. A “street” is an index document (name, place and 0.1° cell), not a physical road. Ratios are rounded. Countries in the population file with no streets are included with zero counts.

The largest gaps in one world build (Overture 2026-09-23.1 with the OSM, OpenAddresses and GeoNames [source inputs](../guides/indexing.md#overture-address-index)), before and after Overture's places were added. [The full measurement](house-number-coverage.tsv) records every country's before/after counts (base builder `4f714061845f4003`, candidate `0a3e9077e88a1702`; the `trial` column records the rejected wider import). Population: [GeoNames countryInfo.txt](https://download.geonames.org/export/dump/countryInfo.txt), retrieved 2026-10-16, SHA-256 `c55f638cfaafb4fe45a9aa15120a0f1f78e8a34a8e1847d7b9320cf740fbc9ab`.

| Country | Population | Houses | Per 1,000 people | Streets with a number | Estimated gap |
|---|---:|---:|---:|---:|---:|
| India | 1,464 M | 85,596 | 0 | 13% | 439.1 M |
| China | 1,407 M | 48,642 | 0 | 3% | 421.9 M |
| Indonesia | 286 M | 68,272 | 0 | 5% | 85.6 M |
| Pakistan | 255 M | 96,391 | 0 | 21% | 76.5 M |
| Nigeria | 238 M | 7,276 | 0 | 8% | 71.3 M |
| Bangladesh | 176 M | 18,323 | 0 | 17% | 52.7 M |
| Ethiopia | 135 M | 425 | 0 | 6% | 40.6 M |
| Philippines | 117 M | 64,285 | 1 | 9% | 35.0 M |
| DR Congo | 113 M | 8,584 | 0 | 13% | 33.8 M |
| Egypt | 118 M | 2,323,776 | 20 | 70% | 33.2 M |
| Russia | 144 M | 11,603,910 | 81 | 70% | 31.5 M |
| Vietnam | 102 M | 73,267 → 293,395 | 3 | 24% | 30.2 M |
| Iran | 92 M | 12,231 | 0 | 2% | 27.7 M |
| Turkey | 86 M | 195,318 | 2 | 10% | 25.6 M |
| Mexico | 132 M | 16,182,651 | 123 | 60% | 23.4 M |
| Thailand | 72 M | 43,169 | 1 | 12% | 21.4 M |
| Tanzania | 71 M | 16,032 | 0 | 14% | 21.1 M |
| South Africa | 65 M | 133,160 | 2 | 8% | 19.3 M |
| Kenya | 58 M | 1,132 | 0 | 4% | 17.3 M |
| Myanmar | 55 M | 4,995 | 0 | 12% | 16.5 M |

For comparison, the United States has 329 houses per 1,000 people (112.4 million), France 385, Germany 256, Japan 307 and the United Kingdom 85 (5.29 million before, 5.90 million after). The first rollout adds 616,360 houses in the United Kingdom and 220,128 in Vietnam (four times as many there), for 501.7 million houses and the same 39.2 million streets. These are single-build counts, not recall estimates.

A wider trial added 1.19 million houses in 166 countries, with further large additions in South Africa (+163,000), Malaysia (+57,000), the Philippines (+40,000), Ireland (+13,000), India (+12,000) and Nigeria (+11,500). It failed the gold gate on two reverse lookups: Valencia, Venezuela stopped naming Valencia and named Miguel Peña instead; a Lagos address got a nearer house across the Ogun State border. Those regressions were not relabelled or accepted. The broader rollout is deferred; its potential counts remain in the measurement's `trial` columns. Thailand's potential gain is only 5,000. Some listings and segments use different scripts (for example `Rama IV Rd` versus Thai), and an address without a matching road is dropped.

Sources considered for the largest gaps, October 2026 (licence findings are a reading of the publishers' terms, not legal review):

- **Adopted: [Overture's places theme](https://docs.overturemaps.org/guides/places/)**, [CDLA-Permissive-2.0](https://cdla.dev/permissive-2-0/), in the same bucket and release as the rest of Overture, so the build Job fetches it with no new credentials. This review's usable bulk candidate reaching the largest gaps; the first rollout takes only its two largest measured country additions, the UK and Vietnam. Every further country needs its own full-index gold comparison. The September 2026 files' `sources.license` also names Apache-2.0 for Foursquare and CC0-1.0 for AllThePlaces. CDLA allows commercial use, modification and redistribution with its agreement text; Apache requires its licence, retained notices and a change notice. The builder includes both licence texts, Foursquare's copyright and the changes made in `PLACES-LICENSES.txt`, for hand-built and Job-built indexes. These permissive licences add no share-alike restriction conflicting with ODbL: the combined index still carries its OSM-derived database obligations; permissive inputs do not remove those. Listings are typed by businesses, so only the plainest shape on a known road is read.
- **India, China, Indonesia, Pakistan, Nigeria, Bangladesh, Ethiopia, DR Congo, Iran, Turkey, Thailand, Myanmar, Russia, Egypt:** this review did not establish a downloadable national address register with coordinates and redistribution permission. That is not proof that none exists; OpenStreetMap, already read, remains the main source. Russia's FIAS/GAR is an address register, not a replacement for geocoded house points. The OpenAddresses countrywide files for South Africa, Saudi Arabia and Egypt are uploads with no stated provenance or terms and stay refused (`openaddresses_reviewed.tsv`).
- **United Kingdom:** AddressBase is licensed by Ordnance Survey and not open; [OS Open UPRN](https://www.ordnancesurvey.co.uk/products/os-open-uprn) (OGL) has 40 million points but no address text, and Code-Point Open only postcodes. Nothing to add beyond OSM and places.
- **South Korea:** the [road-name address database with building entrances](https://www.data.go.kr/data/15050410/fileData.do) (juso.go.kr, about 6.4 million buildings) is listed on data.go.kr, but each download needs an application with identity verification, which a build Job cannot do; the 2017 OpenAddresses copy came from someone else's application and stays refused.
- **Sweden:** Lantmäteriet offers [CC0 open geodata](https://www.lantmateriet.se/en/geodata/our-products/open-data/), but its [address download](https://www.lantmateriet.se/sv/geodata/vara-produkter/produktlista/belagenhetsadress-nedladdning-inspire/) is subject to legal review and special terms. Do not infer that the address product is unrestricted from the general CC0 page. A follow-up candidate after confirming redistribution rights and obtaining access (Sweden has 99 houses per 1,000 people).
- **Bavaria:** its open data (CC BY 4.0) has building outlines, while the [house-coordinate service](https://geodatenonline.bayern.de/geodatenonline/seiten/wfs_hauskoor) requires an agreement and credentials; the OSM extract stays.
- **Ireland:** GeoDirectory and Eircode are licensed, not open.

The UK/Vietnam rollout passes the relative gold gate on the same 4,188-case suite, consumer binary and models: no newly failing or worse cases. SearchAddress goes from 3,655 to 3,656 passes (Belvedere Road in London); SuggestAddress (1,873), ResolveRegion (2,531) and ReverseGeocode (3,784) are unchanged. The new 200 cases pass 27 before and 28 after, including 7 and 8 of the 78 house cases. A baseline rerun showed zero response drift. The absolute run still fails the pre-existing Rue de Rivoli must-pass case, and grpcurl's documented timeout reproduced in the RPC integration test. The 1,000 unscored address samples change in 7 results, all to houses in the same country; their accepted snapshot was not replaced. These are coarse, landmark-biased coverage checks, not an accuracy or speed claim.

Gold independence: the national address registers used for the United States, Netherlands, France, Norway, Switzerland, Spain and Czechia are already index inputs for those countries, through Overture's address theme (NAD for the United States, BAG for the Netherlands, BAN for France per département, Kartverket, swisstopo, CartoCiudad and RÚIAN) and OpenAddresses (BAN, BAG, CartoCiudad and Kartverket again). A registry case there measures fidelity to the register, not independent accuracy. Overture's places are read in none of those countries, since Overture covers them. The Korean school-directory set is selected independently of the index, but independence of its upstream coordinates from the index's contributors has not been established.
