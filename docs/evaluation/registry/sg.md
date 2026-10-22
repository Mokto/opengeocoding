# Singapore

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/sg.NOTICE)

**Singapore** (`sg.seeds.jsonl`, `sg.jsonl`): 181 locations in 55 of the 82 postal sectors, 362 queries, from postcodes drawn uniformly from the postal code space and looked up in **OneMap**, the Singapore Land Authority's national map, through its [Search API](https://www.onemap.gov.sg/apidocs/search): postal code, block number, road name, building name and WGS84 point for each address.

## Licence and terms

OneMap's [API Terms of Service](https://www.onemap.gov.sg/legal/apitermsofservice.html) let anyone call the API commercially or not, and put the datasets it returns under the [Singapore Open Data Licence 1.0](https://www.onemap.gov.sg/legal/opendatalicence.html): use, copy, distribute, modify and adapt, commercially or not, with a conspicuous notice naming the source and linking the licence, and without suggesting official status or endorsement. `sg.NOTICE` is that notice; the corpus records the licence as `SG-ODL-1.0`. The API terms add three conditions the capture keeps: use the access credentials issued to you (the Search API needs an account's token, and without one answers with an `error` the exporter refuses), do not mask the client's identity, and do not disrupt the service. The exporter sends one request at a time with a 0.5 s pause, under the 250 calls a minute OneMap allows a token. The website's own [Terms of Use](https://www.onemap.gov.sg/legal/termsofuse.html), which forbid storing or republishing site material, cover the website; the API terms send datasets fetched through the API to the Open Data Licence instead. OpenAddresses republishes a full OneMap crawl under the same licence and attribution. This is a reading of the published terms, not a grant from the SLA.

## Capture

Singapore has no bulk address download, so the frame is the postcode space itself. Every address has a six-digit postal code whose first two digits are its postal sector, 01 to 82, usually one code per building. `export-onemap-postcodes.py` orders the 820,000 codes 010000 to 829999 by the seeded SHA-256 of the code and searches the first 1,200, every page of each answer. A postcode that exists is drawn with the same probability as any other, so the sample follows the register's own distribution of postcodes (Housing and Development Board blocks, condominiums, landed houses, offices), without a list from any geocoder or from the index's inputs.

```sh
ONEMAP_TOKEN=... python3 -u tests/registry/export-onemap-postcodes.py --out SOURCE/onemap-2026-10 --seed sg-2026-10
target/release/evaluate sample-sg --onemap SOURCE/onemap-2026-10/onemap.json --seed sg-2026-10 --out tests/registry/sg.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/sg.seeds.jsonl --out tests/registry/sg.jsonl --onemap SOURCE/onemap-2026-10/onemap.json
```

Each response page is checkpointed under `responses/` and never refetched, so an interrupted capture (a 429 or a network error stops it) resumes where it stopped; the token is read from the environment and stored nowhere. `onemap.json` holds every drawn postcode with its results, found or not, and each page's request URL and SHA-256. OneMap publishes no release or edit date, so the snapshot is the set of responses: keep the whole directory.

The capture of 2026-10-22 drew 1,200 postcodes and found **186** (15.5%; OpenAddresses' crawl has 123,883 postcodes, 15.1% of the space). Every answer fit on one page and no result carried another postcode. `onemap.json` is 370,944 bytes, SHA-256 `b5ca7edc6087946dffe8f25558f0c9dfff9010cc0754e5d7e37f330677efc527`, over 1,200 response pages; re-running the exporter offline reproduced it byte for byte. OneMap does not document how it places a point (building centroid, entrance or postal delivery point), so no precision beyond the 150 m tolerance is claimed.

## Sample

`sample-sg` keeps a found postcode when all its results name one block number and one road, lie within 50 m of each other, and the block is a number of one to four digits with at most one letter after it. Five postcodes fail, all because one code covers two numbers of one building (45 and 45A Neil Road, 19 and 19A Jalan Mas Puteh, 148 and 148A Sims Avenue, 19 and 19A Lorong 8 Geylang, 43 and 43A Nanyang View); 181 remain: 144 plain block numbers and 37 with a letter (`sg_plain`, `sg_suffix`), 115 with a building name and 66 without. The point is the first result's.

Each location has two queries in OneMap's own capitals, which require goodMatch true: the `postcode` form, `640 ROWELL ROAD, SINGAPORE 200640`, and the `street` form without the postcode, `640 ROWELL ROAD, SINGAPORE`. Both expect house level, the block number and the postcode, within 150 m of OneMap's point. Building names are kept in the provenance but not queried. Region expectations are left empty, so ResolveRegion is not checked; ReverseGeocode is asked about every point. 42 locations are held out by the shared location-hash rule, both queries of a location together.

The sample is uniform over postcodes, not over people or addresses: a block of 100 flats counts as one postcode, like a landed house. It does not cover unit numbers (`#05-12`), addresses without a block number, or the postcode-only and building-name searches common in Singapore.

**Index ancestry.** All 142,210 Singapore addresses in Overture release 2026-09-23.1 come from OpenAddresses' `sg/countrywide`, a OneMap crawl uploaded on 2026-06-04; the index also reads the Malaysia, Singapore and Brunei OpenStreetMap extract. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Local baseline

On a local world index from builder `17bc2fab0087eddb` (Overture 2026-09-23.1, OpenAddresses and OSM), the corpus run alone:

| Class | Form | Queries | SearchAddress passes |
|---|---|---:|---:|
| Plain | postcode | 144 | 143 |
| Plain | street | 144 | 143 |
| Suffix | postcode | 37 | 36 |
| Suffix | street | 37 | 36 |

Overall 358/362 pass, all 84 held-out queries among them; 2 are too coarse and 2 have a wrong component, with no critical result. All 362 ReverseGeocode country checks pass; SuggestAddress finds an acceptable full-string result for 361/362. The failures:

- **Wrong postcode (2, index data):** 325 BALESTIER ROAD answers the house at 2 m from OneMap's point but with postcode 329699. OpenStreetMap tags the City Suites building at 235 Balestier Road (postcode 329699) as number 325, and the index gives OneMap's house that postcode.
- **Locality (2, matching):** both forms of 624C PUNGGOL CENTRAL answer a Punggol Central street 691 m away. The index holds 624C at 1 m from OneMap's point, and `624C PUNGGOL CENTRAL` without `, SINGAPORE` finds it, so naming the city loses the house.

Neither is labelled `known_failure`; both are noncritical. These are diagnostic results on a local index, not a CI gold run. The documented fresh-service grpcurl SearchAddress hang reproduced as a known failure. Human review of the source points is pending.
