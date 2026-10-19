# Belgium

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/be.NOTICE)

The registry-first sample uses FPS BOSA's official BeStAddress CSV exports for all three regions, under CC BY 4.0. These are not the IGN GeoPackage/WMS derivatives marked noncommercial-only. Retain the downloaded ZIPs and hashes. Downloads publish latest rather than an immutable release; the pinned filenames and SHA256 values identify this capture.

Prepare one CSV by concatenating the three ZIP members, retaining one header. The source supplies WGS84 columns directly: use those, not the Lambert columns whose CRS recently changed. Only current rows with paired street/municipality names, a house number and postcode are eligible. A building key hashes region, municipality ID, street ID and house number (NUL separated). Among its rows, choose a box-bearing row before a box-free row, then the lexically smallest address ID. This keeps all variants and source units at one hold-out location, rather than counting apartments separately.

Sampling balances regions and unit/suffix/multilingual/plain classes, with up to 20 different streets per class. Each selected row types every paired official language form and its published box identifier. A translated street is paired with the municipality name in that language; names are never invented. Expectations check the source point within 150 m, country, building number, postcode and goodMatch. They do not verify the box, a postal locality alias or national accuracy. The source's regional registers may also feed Overture/OpenAddresses: independent selection measures registry fidelity, not independent surveyed accuracy.

```sh
# In a separate source directory:
for region in bebru bevlg bewal; do
  curl -fLO https://opendata.bosa.be/download/best/openaddress-$region.zip
done
# Concatenate openaddress-bebru.csv, openaddress-bevlg.csv and openaddress-bewal.csv
# from their ZIPs, checking identical headers and keeping only the first header.
target/release/evaluate sample-be --best SOURCE/best-2026-10-19.csv --seed be-2026-10 --out tests/registry/be.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/be.seeds.jsonl --out tests/registry/be.jsonl --best SOURCE/best-2026-10-19.csv
```

## Pinned capture and local baseline

Captured 2026-10-19: 7,177,814 rows; 139,940 current rows lack a valid local WGS84 point and are excluded before selection. The remaining eligibility/grouping rules produce 4,504,980 building keys. The sample has **220 locations and 384 queries**, including 49 held-out locations; 60 locations are in Brussels, 80 in Flanders and 80 in Wallonia.

| Pattern class | Locations | Queries | SearchAddress passes |
|---|---:|---:|---:|
| Multilingual | 60 | 120 | 120 |
| House suffix | 60 | 81 | 81 |
| Box-bearing building | 60 | 143 | 139 |
| Plain | 40 | 40 | 37 |

There are 60 explicit box queries (56 pass); the other queries at those buildings are box-free language controls. SuggestAddress finds an acceptable full-string result for 319/384 queries. ReverseGeocode passes the country check at all 384 repeated source points; it does not verify the house or box. ResolveRegion is unscored because no ISO expectation is supplied.

The seven forward failures are four too-coarse answers, one near miss, one far miss and one confidence mismatch. The critical Rue du Tordoir 14, Tubize case is attributed in be.labels.jsonl: the parse preserves the complete address, but the raw house and RPC point are 204 km from BeSt and lack ancestry, while the suggested Tubize street is near the reference. This is an index/import coordinate/ancestry defect, not proof of which upstream source or storage stage introduced it. No expectation was weakened.

| File | SHA256 |
|---|---|
| openaddress-bebru.zip | e09d97dc9eeeccf2eb977cbdf62fa16d2ddab579537d40e2047d7a84a03326e0 |
| openaddress-bevlg.zip | 8bba7185862fba2abadb32c7c3c3fd59bb31cbd03bb26f8aafb2a64ceca79f16 |
| openaddress-bewal.zip | 997d80234200583dc88b57adc1f875541485d5b6e0655714fa8e2ac460706195 |
| best-2026-10-19.csv | 5410671b8fc0591e957759c11c411da41db1ce6e593e123ba1ee56c2b7385369 |

## Validation

These are measurements on the documented last-known-good local world index (builder 4fb1fd818b46b2f6, fingerprint e6df26bed27b), not a successful rebuild with the current builder. The unchanged main runtime, identical models and reference data were used on both sides. Two full 11,057-case gold runs have zero response/scoring drift across all four RPCs, including all 8,939 pre-existing cases. There are no newly failing or worse cases. Re-scoring after the triage labels changed zero classes; the labels acknowledge failures rather than passing them. Local absolute gates pass with those labels.

The speed comparator reports no slowdown, but the second identical-binary run was substantially faster on this non-isolated host; no speedup or production-latency claim follows. The documented fresh-service grpcurl hang remains a known failure even though these runs happened to pass that check. Human review of source points remains pending.

Run the offline checks with cargo test --locked and python3 docs/test-registry-exports.py. Run one country with evaluate run --corpus tests/registry/CC.jsonl --index DIR --out RUN_DIR. This source-only addition changes neither resolver behavior nor the builder hash.
