# Iceland

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/is.NOTICE)

**Iceland** (`is.seeds.jsonl`, `is.jsonl`): 181 locations in all 61 municipalities, 312 queries, drawn from **Staðfangaskrá**, the access address register of Húsnæðis- og mannvirkjastofnun (HMS) ([dataset page](https://hms.is/stadfangaskra), [metadata](https://geoportal.gis.is/geonetwork/srv/api/records/%7BA879D973-CA98-49D7-AA50-7BC35047E461%7D)): one row per address point, with its address number (HEINUM), land parcel (LANDNR), municipality number (SVFNR), postcode, locator (the street, or the farm or place a numberless address names) in the nominative and dative, house number and letter, suffix, ISN93 and WGS84 point. Municipalities keep the register, which HMS refreshes weekly. The register has no place names, so two other public datasets supply them: the post town of each postcode from **Póstnúmeraskrá**, the postcode register of Byggðastofnun ([page](https://www.byggdastofnun.is/is/postthjonusta/postnumer), [metadata](https://geoportal.gis.is/geonetwork/srv/api/records/22e98d21-a86b-4b62-ad58-a6d17703b612)), and the municipality names from the municipal boundaries of Náttúrufræðistofnun's **IS 50V** (`IS_50V:mork_sveitarf_flakar`).

## Licence

- **Staðfangaskrá**: the metadata's use constraint is the Icelandic government's guidelines on reuse of public information (*Leiðbeiningar varðandi endurnot opinberra upplýsinga*, based on chapter VII of the Information Act 140/2012 and modelled on the UK Open Government Licence 3.0; read on 2026-10-30). The rights holder grants a perpetual, free, worldwide permission to copy, publish, distribute and adapt the information and to use it commercially, combined with other information or products, provided the source is stated, by default as "Byggir á upplýsingum frá" and the rights holder's name; use may not suggest official status or the holder's endorsement. OpenAddresses' `is/countrywide` records the same terms with attribution "HMS".
- **Póstnúmeraskrá**: the metadata's terms (read on 2026-10-30) let anyone copy, reuse and publish the data freely, provided it says the information comes from Byggðastofnun with the text "Byggt á gögnum frá Byggðastofnun".
- **IS 50V**: Náttúrufræðistofnun publishes its open data under CC BY 4.0 in accordance with the Act on the reuse of public sector information 45/2018 ([open data](https://www.natt.is/en/resources/open-data), read on 2026-10-30).

`is.NOTICE` gives all three attributions and the changes made. The corpus records the licence as `IS-Endurnot-Opinberra-Upplysinga AND Byggdastofnun-Postnumer AND CC-BY-4.0`.

## Capture

On 2026-10-30 the dataset page's CSV (`https://hmsstgsftpprodweu001.blob.core.windows.net/fasteignaskra/Stadfangaskra.csv`) served 0 bytes (Last-Modified 2026-10-30 00:40:05 UTC), so the register was read from HMS's WFS, which the metadata lists, as CSV with the same columns, at 18:18 UTC. Its `FID` is a per-request identifier, so a repeated download differs byte for byte; keep the pinned files:

| File | Source | Bytes | SHA-256 |
|---|---|---:|---|
| `stadfangaskra-wfs.csv` | `https://gis.fasteignaskra.is/geoserver/wfs?service=wfs&version=2.0.0&request=GetFeature&typeNames=public:Stadfangaskra&outputFormat=csv` | 38,286,009 | `667e1026b60622e67d3c9f3b076a1dcb0dffeec2b538e5b64f880122717b3c5b` |
| `postnumeraskra_2024.xlsx` | `https://www.byggdastofnun.is/static/files/Postmal/Postnumer/postnumeraskra_2024.xlsx` (Last-Modified 2024-04-18) | 14,317 | `a230577bfdf988cb880121dcdc979b5284a68648be81a254556cc44ab6f079f0` |
| `sveitarfelog-wfs.csv` | `https://gis.lmi.is/geoserver/wfs?service=wfs&version=2.0.0&request=GetFeature&typeNames=IS_50V:mork_sveitarf_flakar&outputFormat=csv&propertyName=objectid,uuid,stjornsyslusvaedi,nrsveitarfelags,sveitarfelag,upphafnotkunar,dagsleidrettingar,heimild,gagnaeigandi,gagnasafn` (IS 50V release 2026-09-24) | 10,592 | `9999896a3153e022595381d5204fd9f7eaef6db5684d9b4a7c8971ffeea8fefe` |

The register has 139,774 rows (one per HNITNUM), the latest corrected on 2026-10-30. `tests/registry/stadfangaskra-csv.sql` joins each row to its post town (every one of the 139,760 rows with a postcode has one in Póstnúmeraskrá) and municipality name, and fails on a postcode or municipality number without one. IS 50V numbers the Borgarbyggð that absorbed Skorradalshreppur on 2026-06-01 with Skorradalshreppur's 3506, while the register keeps Borgarbyggð's 3609 for both (it has no 3506), so the script reads IS 50V's 3506 as 3609. It writes one CSV named after the capture date:

```sh
mkdir -p ../is-data
(
  REPO=$PWD
  cd ../is-data
  curl -fL -o stadfangaskra-wfs.csv 'https://gis.fasteignaskra.is/geoserver/wfs?service=wfs&version=2.0.0&request=GetFeature&typeNames=public:Stadfangaskra&outputFormat=csv'
  curl -fL -o postnumeraskra_2024.xlsx https://www.byggdastofnun.is/static/files/Postmal/Postnumer/postnumeraskra_2024.xlsx
  curl -fL -o sveitarfelog-wfs.csv 'https://gis.lmi.is/geoserver/wfs?service=wfs&version=2.0.0&request=GetFeature&typeNames=IS_50V:mork_sveitarf_flakar&outputFormat=csv&propertyName=objectid,uuid,stjornsyslusvaedi,nrsveitarfelags,sveitarfelag,upphafnotkunar,dagsleidrettingar,heimild,gagnaeigandi,gagnasafn'
  duckdb -f "$REPO/tests/registry/stadfangaskra-csv.sql"
)
target/release/evaluate sample-is --stadfangaskra ../is-data/stadfangaskra-2026-10-30.csv --seed is-2026-10 --out tests/registry/is.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/is.seeds.jsonl --out tests/registry/is.jsonl --stadfangaskra ../is-data/stadfangaskra-2026-10-30.csv
target/release/evaluate run --corpus tests/registry/is.jsonl --index out --out eval-results/is
```

`stadfangaskra-2026-10-30.csv` has 139,774 rows, 26,767,341 bytes, SHA-256 `4b94e0c8173229b907b362f7bddbf6c83b96707fe9fada5061c6074da9a65783` (DuckDB 2.0.0 with its `excel` extension). The adapter checks that every row's published form (`VEF_BIRTING`) is its locator, number, letter, suffix and land parcel joined as the register writes them, which holds for all 139,774. Preparation, sampling and corpus generation each repeated byte for byte.

**Coordinates.** `N_HNIT_WGS84` and `E_HNIT_WGS84` are WGS84 latitude and longitude to eight decimals, `HNIT` the same point in ISN93 (EPSG:3057). The adapter takes the WGS84 pair as published and rejects a point outside 63.0–66.7° N, 24.7–13.3° W, so swapped columns fail loudly. PROJ's EPSG:3057-to-EPSG:4326 transformation of `HNIT` agrees with the WGS84 pair to 0.7 mm at all 139,774 points. `TEGHNIT` says what the point marks (57% "not yet reviewed", then a point within the lot, the estimated centre of the building or its main entrance); no precision beyond the 150 m tolerance is claimed.

## Sample

`sample-is` reads 116,732 eligible addresses, each with a three-digit postcode. It leaves out 12,455 rows with a suffix (`VIDSK`: "lóð", "land", "spennistöð", "8-16"), 882 whose point the register marks for review or without an address number (`YFIRFARID` 2 or 9), 2,739 points of addresses with more than one point, 4,755 addresses written like another in the same postcode (the same locator, number and letter), 1,303 numberless addresses of several words, and 908 others (locators with digits or punctuation such as "Naustabr.12-22 BL", numbers with a leading zero, a letter that is not one, no postcode or no dative). The several-word numberless addresses are more often lots, car parks and road reserves ("Land Bæjarins", "Bílastæði yfir Fífuhvammsbraut") than farms; the property register's land use would tell them apart. An eligible address's class is:

- **street**: a locator and a number ("Bæjarlind 8"): 95,682.
- **letter**: a number with one letter ("Eskihlíð 22A", "Kaupvangsstræti 8b"): 13,200.
- **name**: one word (hyphens allowed) without a number, mostly a farm, a holiday house or a place ("Elliðakot", "Suður-Nýibær", "Kálfatjarnarkirkja"): 7,850.

In each municipality (by `SVFNR`) and class, `sample-is` takes the address with the smallest seeded SHA-256 of its `HNITNUM`: 61 street, 59 letter and 61 name locations (Árneshreppur and Fljótsdalshreppur have no eligible letter address). The sample weights municipalities and classes, not population or address counts: Reykjavíkurborg, with 17% of the addresses, has three locations. It is not a representative national accuracy sample. Of the 181 locations, 32 (51 cases) are held out by the shared location-hash rule.

Queries:

- **street**, **letter** and **name**: the locator in the nominative, number and letter, postcode and post town, with the country: `Bæjarlind 8, 201 Kópavogur, Iceland`, `Kaupvangsstræti 8b, 600 Akureyri, Iceland`, `Elliðakot, 271 Mosfellsbær, Iceland`. The post town is Póstnúmeraskrá's, verbatim: `Sel, 225 Garðabær (Álftanes), Iceland`.
- **dative**: the same with the locator in the dative, as Icelanders write an address on a letter and as HMS writes its own ("Borgartúni 21, 105 Reykjavík"), for the 131 locations whose dative differs from the nominative: `Tónahvarfi 10G, 203 Kópavogur, Iceland`, `Elliðakoti, 271 Mosfellsbær, Iceland`. Post towns stay in the nominative, which Póstnúmeraskrá gives.

Street and letter cases require goodMatch true at house level within 150 m of the register point, with the register's number and letter ("22A") and postcode. A name address has no number, so its cases require goodMatch true at street level or finer within 150 m, with the postcode: the service, like Overture, takes the farm's name for a street. Municipality names are sampling tags, not ISO region expectations, so ResolveRegion is not checked; ReverseGeocode is asked about every point, and 25 m away for the street and letter locations. Each case's `places`, the names ReverseGeocode's answer must include one of, are the post town and the municipality ("Kópavogur", "Kópavogsbær"); a rural postcode's post town is often not the nearest place, so the municipality is the name that holds there.

**Index ancestry.** Overture release 2026-10-21.0 has 139,583 Icelandic addresses, all from Staðfangaskrá. 139,542 of them have a record id that is a `HNITNUM` of this extract, and all 181 sampled points are among them. Against this extract the median distance is 0 m, 139,512 lie within 1 m and 6 farther than 150 m; 139,523 have its number, 139,530 its locator and 139,541 its postcode. Overture keeps only the nominative locator and writes the letter as the unit (13,739 of 14,009 lettered rows: number "22", unit "A"), the suffix too (12,154 rows), and gives each address its postcode's place as Pósturinn names it ("Selfoss, dreifbýli"). OpenAddresses' `is/countrywide` reads the same register, but the builder skips it (its licence is not one the builder recognises), and the chart reads the Iceland OpenStreetMap extract for landmarks only. Selection is independent of the index, but a pass measures fidelity to the register, not independent real-world accuracy, as for RÚIAN in `cz.md`.

## Optional index enrichment

The offline builder can read the **whole pinned HMS WFS capture**, not the sampled corpus or this service's answers:

```sh
build_index OVERTURE_DIR INDEX_DIR \
  --iceland-addresses ../is-data/stadfangaskra-wfs.csv \
  --iceland-addresses-sha256 667e1026b60622e67d3c9f3b076a1dcb0dffeec2b538e5b64f880122717b3c5b
```

Other source flags must stay identical to the baseline build. This input is the raw WFS CSV, not the three-source joined CSV used by `sample-is`. The builder verifies its checksum, required columns, unique `HNITNUM` values and WGS84 coordinate bounds. It attaches the official `HEITI_TGF` spellings to existing `HEITI_NF` street documents using the same containing place and 0.1° geographic cell as the address importer. Shared aliases remain on every matching local street; no word is rewritten globally. It does not create missing streets or change numbered-house import.

For numberless rows without a letter or suffix and not flagged for review (`YFIRFARID` 2 or 9), the document also preserves the official postcode and point. A conventional `Locator, Postcode Town, Iceland` query can use that evidence only when the complete locator matches an indexed spelling, the postcode is stored, and the postal town or geographic locality corroborates the town. A postcode misparsed as a house number cannot veto this verified address. Distinct matching numberless addresses abstain, even when their points coincide; a nearby street, town or postcode is not promoted. Conventional official-alias queries also abstain when the same alias and requested house remain spatially ambiguous after the stated qualifiers. Other input forms retain the normal resolver's limitations.

The index records the input checksum in `iceland-source.json` and the HMS attribution and changes in `ICELAND-NOTICE.txt`; build Jobs also include the source in `build.json` and `ATTRIBUTION.txt`.

For a build Job, set `index.sources.icelandAddresses` to an object with `url` (an immutable, installer-controlled HTTPS URL serving the captured CSV) and `sha256`. Its sealed source mirror is `sources/iceland-addresses/<sha256>`. The builder rechecks the checksum even when the mirror supplied it. The chart leaves this input disabled by default: the request-dependent WFS identifiers mean the pinned bytes cannot reliably be downloaded again from the live WFS URL. Preserve the capture before enabling the input. There are no serving-time downloads.

The source overlap remains deliberate: these HMS addresses also feed Overture. Gains measure fidelity to the published register and its alternative forms, not independent national accuracy.

## Local enrichment comparison (2026-11-01)

Baseline service: `bcdf9d9c6cbaad5bce7dc456279e5f0200ddb913`, binary SHA-256 `df2d6a0938f0bfe4b8d5d16f51c520f8986c8dc63fd2e6e62f804942b1db670e`; candidate binary SHA-256 `05802aa5ca15109d49a392ea983b353dd6e34dc8a213a770aa9e32e95c0d6a71`. Both use the same libpostal models (`65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7`), native library (`9d1a67692bba9f72af1708590666c67be11cb3edfe69e106e964c95d7e137185`) and reference data. Iceland corpus SHA-256: `778b68fd4ba472001c9fa6b8539a31e247d6374015e08a00b3026fe63751ca8c`.

The two full indexes use builders `495ed3bf27ad1ccd` and `c381bdd9d776d281`, Overture **2026-10-21.0**, and the same local OpenAddresses, OSM, Korean-shop and GeoNames captures. A 5,835-file checksum manifest was retained with the local run; source sizes and modification times remained unchanged throughout. Index `meta.json` hashes: baseline `2da4be713dafe7667632475e396f87c3eb111bab59f8283b6cd4946e278abd61`, candidate `79fe318ac0f5b090baf79345522b26026c960257b67d1958e9dbae531967b33b`. The candidate additionally reads the pinned raw HMS capture above. The earlier shared index had no sealed provenance record, so these comparisons use separately built indexes rather than assuming its release from an old build script.

SearchAddress passes (each cohort has one query per location; locations overlap across cohorts):

| Cohort | Training, before → after | Held-out, before → after |
|---|---:|---:|
| Nominative street | 48/48 → 48/48 | 13/13 → 13/13 |
| Nominative letter | 48/49 → 48/49 | 10/10 → 10/10 |
| Nominative name | 21/52 → 52/52 | 4/9 → 9/9 |
| Dative | 25/112 → 107/112 | 5/19 → 18/19 |
| All queries | 142/261 → 255/261 | 32/51 → 50/51 |
| Locations passing **every** variant | 56/149 → 144/149 | 17/32 → 31/32 |

Overall: **174/312 → 305/312 queries**, **73/181 → 175/181 locations passing every variant**. Failing locations fall from 108 to 6. Training failure classes change from 72 too-coarse, 15 confidence-mismatch, 23 near-miss, 8 abstain and 1 far-miss to 6 too-coarse. Held-out failure classes change from 14 too-coarse, 3 confidence-mismatch and 2 near-miss to 1 too-coarse. Held-out individual cases were not inspected for tuning.

A training-only scan of all Icelandic street documents and exact number keys finds every failed nominative address in the baseline. Thirty of the 31 failed name addresses have a street point within 150 m with the right postcode; Miðvangur has the indexed street's centroid 174.6 m away. The 15 training confidence mismatches arise when the parser takes the postcode for a house number. Other name failures mix that misinterpretation, a locator labelled as a building rather than a road, and rejection of a postal town outside the address's geographic locality. The numberless path now uses the source's address point and stored postcode, not proximity or a copied postcode, to establish the address. Dative failures have an indexed nominative address but lack the official alternative spelling; numberless interpretation can be a second obstacle.

Remaining training failures (six queries, five locations):

- `Starrastöðum 2`: the parser merges the locator, number and postcode into a building name.
- `Álaleira 13B` and `Álaleiru 13B`: house `13b` and postcode `780` are parsed together as house number `13b 780`. The indexed house and dative alias work; this is not the letter import fixed in #453.
- `Silfurbotni 3` and `Skagnesi 1`: the locator and real number become a building name, and the postcode becomes the requested house number.
- `Rimum 8`: the same parsing failure, plus postal/geographic corroboration: its postal town is stored as `Selfoss, dreifbýli`, while its geographic locality is Flóahreppur. The precise generic structured-lookup rejection branch remains unconfirmed.

Each requested training house is stored within 0.08 m with its expected postcode; none of these remaining failures is missing house data or a missing official alias. The remaining held-out failure is one dative too-coarse result at one location, left uninspected.

SuggestAddress improves from 213/312 to 310/312; reverse checks stay at 296/312 at the point and 200/203 at the 25 m offset. On the **44,047-case full gold comparison**, SearchAddress gains 131 passes, SuggestAddress 97, with no newly failing case, worse failure class or abstain-to-miss transition. Reverse and region checks do not change. Changes are confined to Iceland; 17 Iceland GLEIF responses change without changing their classes. A code-only comparison on the baseline index has zero response drift, as do unchanged-binary repeats of both Iceland and full gold. The full gold corpus hash is `3dcefbd1fa019468d7c09baeb6a28344516e964662e7529f05ba0f1fa6125e9e`.

Formatting, all-target clippy, unit/integration tests and the separate model-backed synthetic suite pass locally. Standalone full gold runs exit 1 on both sides because of existing baseline gate failures; their comparison exits 0. The model-backed RPC run still fails the documented grpcurl timeout and an oversized-query logging assertion; the logging assertion also fails with the untouched baseline service and index. These failures are not waived or repaired by this change. Laptop timings do not establish a speed improvement, and the opt-in HMS source has not been measured by CI's default-source speed gate.

**This is source-fidelity evidence, not independent national accuracy.** The sampled HMS points also supply Overture; the sample weights municipalities and address classes rather than population.

### Validation after rebasing onto `e12c4f1`

Main's GeoNames enrichment and Australian reverse-house suburbs change the builder too. The rebased comparison rebuilds both full indexes: baseline `060849a64adb798d`, candidate `87ca114eef0ee0e8`. The geographic assigner stays alive until both HMS enrichment and Australian suburb generation finish. All 5,835 source-file SHA-256 digests match the original comparison; models, reference data and corpora are unchanged. The baseline binary was built from `e12c4f1` (SHA-256 `8537eda55fa3d39e01d5978464036db2e905a24e962d6b272c46751d668e2c02`); the candidate binary is `89c48faddf86742f3f39b67921ede07fb24cf7e57f9b170bfeef3c2ca4c2edf6`. Index metadata hashes are `9d92eff962b396bb5a13b0a3be4e88a8d04adeae47d0545480e04c27a6f8ff86` and `cc1f4201ae6fd0ce40a33c9db0a0fcc869ef8d7131c9ae03290317666cd897c1`.

The Iceland results remain **174/312 → 305/312**, with the same suggestion and reverse results. Full gold SearchAddress passes change from **36,713 to 36,844**, and SuggestAddress from **38,022 to 38,119**. There are no newly failing cases, worse failure classes or abstain-to-miss transitions; region and reverse results do not change. The full-gold baseline repeat and code-only comparison both have zero response drift, including the Iceland cases. No individual held-out case was inspected for tuning.

Formatting, all-target clippy, unit/integration tests and the separate libpostal synthetic suite pass again. Standalone full gold still exits 1 on both sides, and RPC checks still fail the grpcurl timeout and oversized-query logging assertion. Local timings vary: an intermediate comparison against `5104ea8` flagged total +40% SearchAddress, +47% SuggestAddress and +41% ReverseGeocode. The latest full-gold timings flag no slowdown over 15%, but the Iceland-only run flags SuggestAddress p50 +26% / p90 +44% and ReverseGeocode total +16% / p90 +23%. These uncontrolled laptop measurements do not establish a performance regression or improvement. CI's measured-speed gate is checked separately, and its default-source configuration still cannot establish latency with the optional HMS capture enabled.

## Historical local baseline (2026-10-30)

On a local full index from builder `2ee0057bc161acd7` (Overture 2026-10-21.0, OpenAddresses and the chart's OSM extracts), SearchAddress passes 174 of 312 queries: all 61 street, 58 of 59 letter, 25 of 61 name and 30 of 131 dative forms (13 of 43 on streets, 17 of 40 lettered, none of 48 numberless), and 32 of the 51 held-out cases. A same-binary repeat has zero response drift. SuggestAddress finds an acceptable full-string result for 212 (61 street, 59 letter, 59 name and 33 dative forms). ReverseGeocode returns the country, a place of the address and, for a numbered address, the house at 296 of 312 points, and a house 25 m away at 200 of 203. One critical result is triaged and labelled in `is.labels.jsonl`.

Builder `2ee0057bc161acd7` joins an Icelandic unit of one letter to the number, as for Spain's extensions (22 with unit "A" is 22A, with a bare 22 kept for the building where the register has no house 22). On builder `56dd7c8cf3af06c0`, before it, "22A" was indexed as a second house 22: no letter form passed (41 answered the street within 150 m of the point, 17 within 1.8 km, one a postcode 13 km away), and ReverseGeocode answered house 22 at 58 of 59 lettered points. The gold suite on both indexes, with one binary, changes only Icelandic cases, none for the worse. The failures:

- **Dative locators (data/matching; 101 of 131 dative forms):** the index has only the nominative, so a dative locator names nothing unless it is one edit from the nominative (the 30 passes: "Hlíðarþúfum", "Lálandi", "Brekku", "Tónahvarfi 10G"). The rest answer the postcode (45 within 5 km, 9 from 5 to 39 km away), the post town or another place (32 within 25 km, 8 from 25 to 60 km away, one far miss) or nothing (6). The far miss is labelled: `Sylgjufelli, 851 Hella` answers the post town Hella, 134 km from the highland hut, goodMatch false; the nominative form finds the hut at its point. The register has the dative (`HEITI_TGF`), but Overture does not carry it.
- **Numberless addresses found with goodMatch false (matching; 18 name forms):** the resolver answers the farm or place at its point (all within 2 m) with its postcode, but goodMatch false: `Grótta, 170 Seltjarnarnes`, `Orustudalur, 803 Selfoss`. 17 of the 18 are in rural postcodes, 8 of them in another municipality than their post town's (`Litlahlíð, 605 Akureyri` is in Eyjafjarðarsveit); the exception, Grótta, is in Seltjarnarnes. Why the resolver withholds goodMatch is not triaged here.
- **Numberless addresses not found (data/matching; 18 name forms):** the answer is the postcode (5 within 5 km, 3 farther, up to 39 km for `Oddastaðavatn, 311 Borgarnes`), the post town or the municipality (4 within 17 km, 3 farther, up to 60 km for `Vesturöræfi, 701 Egilsstaðir`), a street of the same name 174 m from the point (`Miðvangur, 220 Hafnarfjörður`), or nothing (2: `Kambur, 381 Reykhólahreppur`, `Hrútey, 500 Staður`).
- **One letter not found (1 letter form):** `Álaleira 13B, 780 Höfn í Hornafirði` still answers the street, 74 m away, though Overture has 13A to 13D.
- **ReverseGeocode (reverse; 9 points):** near a municipal boundary the answer names the neighbour at 7 points: `Eskihlíð 22A, 105 Reykjavík` and `Urðarhæð 6, 210 Garðabær` are answered in Kópavogur, `Hallarmúli` (Skeiða- og Gnúpverjahreppur) in Hrunamannahreppur. At `Garðavegur 13B, 900 Vestmannaeyjar` it answers another house, and at the highland hut Sylgjufell nothing.

These are diagnostic results on a local index, not a CI gold run.
