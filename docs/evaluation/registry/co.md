# Colombia — urban Bogotá

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/co.NOTICE)

The reference is UAECD's official Placa Domiciliaria point layer, distributed through IDECA / Datos Abiertos Bogotá under CC BY 4.0. It covers urban Bogotá, not Colombia nationally. The catalogue describes a 1:1000 map product; plate points are not a guarantee of surveyed entrance accuracy. The service is requested in EPSG:4326, regardless of the catalogue's separately stated native reference system.

The acquisition frame consists of principal plates (PDOTIPO=1) whose snapshot OBJECTID modulo 5,000 is 137, 1,871 or 3,023. These fixed systematic offsets are chosen independently of this service and all its answers. They need not be spatially random because publisher OBJECTIDs may cluster. Retain each complete response and checksum: OBJECTIDs can change on republishing, while corpus references use the actual PDOCODIGO. Requests reject truncation, duplicate IDs and records outside the frame. This is a pinned diagnostic frame, not an estimate over every eligible national or municipal address. The complete municipal principal-plate population was not enumerated.

Within that frame, eligibility requires a principal plate with a numeric crossing-road/distance form, an official road type and a local point. MJ, IN and other nonstandard plate text is not treated as a numbered house. The Rust draw balances road types and directional/suffix/plain classes with up to 20 different roads per pool, selecting each cadastral lot at most once. Source-native abbreviation, expanded road type, and hash-free punctuation forms share one registry reference and hold-out key. CL/KR/DG/TV/AC/AK expand to Calle/Carrera/Diagonal/Transversal/Avenida Calle/Avenida Carrera; source numeric/letter/direction strings are retained, not translated or repaired.

Expectations retain the composite crossing-road/building number, source point within 150 m, country Colombia and goodMatch true. They do not verify a unit, postcode or ISO region. Shared municipal data upstream of an index can make this registry fidelity rather than independent coordinate verification.

```sh
python3 -u tests/registry/export-bogota-plates.py --out SOURCE/bogota-2026-10-19
target/release/evaluate sample-co --bogota SOURCE/bogota-2026-10-19/bogota.json --seed co-2026-10 --out tests/registry/co.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/co.seeds.jsonl --out tests/registry/co.jsonl --bogota SOURCE/bogota-2026-10-19/bogota.json
```

Direct acquisition timed out from the investigation host, but the same public publisher queries were accessible through the web-fetch tool. The capture used those responses, not an unofficial mirror. Place the retained response JSON files in the source directory and use --offline to regenerate the identical prepared snapshot without network access.

## Pinned capture and local baseline

The three complete source responses contain 213, 221 and 201 principal plates, **635 candidates** in total. Eligibility and street/lot diversity produce **189 locations and 567 queries**, including 34 held-out locations. Seventy-seven locations carry directional qualifiers, 61 suffix letters and 51 plain numbering; those classes take precedence in that order.

| Source road type | Locations | Queries | SearchAddress passes |
|---|---:|---:|---:|
| AC | 10 | 30 | 0 |
| AK | 7 | 21 | 0 |
| CL | 60 | 180 | 0 |
| DG | 19 | 57 | 0 |
| KR | 60 | 180 | 0 |
| TV | 33 | 99 | 0 |

All 567 forward queries are too coarse; all full-string SuggestAddress checks fail. These results do not establish that the index lacks every corresponding house: composite-number representation, parsing and lookup coverage still need separate investigation. No baseline label converts these failures into passes. ReverseGeocode passes the country check at all 567 repeated source points, not house or plate identity. ResolveRegion is unscored.

### Plate and avenue forms

The houses were in the index all along, from OpenAddresses' Bogotá layer, keyed as the register writes them: plate `86B 18` on `AC 43 S`. Queries write the plate `# 86B-18` and often the road `Avenida Calle 43 S`, and neither matched. A Colombian plate is now also looked up in the register's form, and `Avenida Calle`/`Avenida Carrera` also as `AC`/`AK` (and back). On the local world index (builder `9d4006a4eec15abb`, same index both sides), SearchAddress passes 561 of the 567 queries (CL 174/180, every other road type all), held-out locations included, against 0 before. SuggestAddress, which does not use this lookup, is unchanged.

### Same-named roads in another locality

The six remaining failures were two locations, `CL 59BIS S # 48G-60` and `CL 22BIS S # 9C-18 E`. The parse and the house keys were right (`48g 60` and `9c 18 e` are indexed 1 m from the plate points). The miss came from scoping. Overture files Bogotá's localities partly under the `Bogotá ciudad` locality (Kennedy, Bosa) and partly directly under the `Bogotá, D.c.` county (San Cristóbal, Ciudad Bolívar). The town scope found a same-named road in Kennedy or Bosa without the house and stopped before the county scope. Elsewhere that order is deliberate: a province's other towns must not lend a house (Via Brera 28 in Milan). In Colombia the county is the municipality (1,122 counties, no `localadmin`), so when the town's road lacks the house, a same-named road under the county that holds it now answers. On the local world index (builder `1929034e0fce080d`, same index, models and evaluator both sides), SearchAddress passes all 567 Bogotá queries (CL 180/180). Across the full gold suite only these six results changed, and SuggestAddress, ResolveRegion and ReverseGeocode are unchanged.

The prepared bogota.json is 154,724 bytes, SHA256 080d89ae89ca4e607f22a0c9571d3b9f545c93949ae5d60cb8a3fb381f3db524. Its sources list pins the three request predicates, URLs and response checksums. The source response count, not a national population, is the eligible acquisition-frame denominator.

## Validation

These are measurements on the documented last-known-good local world index (builder 4fb1fd818b46b2f6, fingerprint e6df26bed27b), not a successful rebuild with the current builder. The unchanged main runtime, identical models and reference data were used on both sides. Two full 11,057-case gold runs have zero response/scoring drift across all four RPCs, including all 8,939 pre-existing cases. There are no newly failing or worse cases. Re-scoring after the triage labels changed zero classes; the labels acknowledge failures rather than passing them. Local absolute gates pass with those labels.

The speed comparator reports no slowdown, but the second identical-binary run was substantially faster on this non-isolated host; no speedup or production-latency claim follows. The documented fresh-service grpcurl hang remains a known failure even though these runs happened to pass that check. Human review of source points remains pending.

Run the offline checks with cargo test --locked and python3 tests/registry/test-registry-exports.py. Run one country with evaluate run --corpus tests/registry/CC.jsonl --index DIR --out RUN_DIR. This source-only addition changes neither resolver behavior nor the builder hash.
