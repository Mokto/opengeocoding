# Australia

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/au.NOTICE)

Source: Geoscience Australia's **National Address Points**, its official spatial adaptation of the August 2026 G-NAF release. Original attributes and G-NAF persistent IDs are retained. This is not a commercial geocoder's output. The Open G-NAF EULA incorporates CC BY 4.0 **with an additional mailing restriction**: no generating addresses/address compilations for sending mail without independently verifying every address's mail capability. This corpus checks geography, not deliverability. The source-code license does not replace the data terms.

## Capture and reproduction

```sh
python3 -u tests/registry/export-gnaf-addresses.py --out SOURCE/gnaf-2026-10-19 --seed au-2026-10
target/release/evaluate sample-au --gnaf SOURCE/gnaf-2026-10-19/gnaf.json --seed au-2026-10 --out tests/registry/au.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/au.seeds.jsonl --out tests/registry/au.jsonl --gnaf SOURCE/gnaf-2026-10-19/gnaf.json
```

The exporter obtains the layer's count and OBJECTID bounds, draws 20,000 different IDs uniformly from that inclusive integer range using a seeded SHA256 stream with rejection to remove modulo bias, and fetches the original rows through the official ArcGIS feature service. Gaps are omitted, not replaced; every existing source row in the range has the same first-stage inclusion probability. Separate count queries verify each batch, with duplicate, unexpected-ID and transfer-limit checks. Query forms use POST to avoid URL-length truncation. Service edit metadata is checked before and after capture and on resume. A changed layer requires a new capture directory. Retain the original responses, request receipts, item/layer metadata and checksums outside git; an existing final snapshot is never overwritten with different content.

The adapter then balances state/territory and mutually exclusive pattern classes (unit, level, range, suffix, plain), selecting up to 12 different locality/street combinations in each pool by seeded hash. It groups state, locality, street and the complete building number, preferring a unit-bearing row, then a level-bearing row, then the lowest persistent ID. This conservative location/hold-out key does not assert every unit shares one physical building. Every form at a selected group uses the selected original row's point. No service answers influence selection.

## Expectations and limitations

- Only current principal addresses with positive confidence, address-level geocoding, a positive structured street number and an address-level property/building/unit centroid, property access point or frontage/access-point-setback geocode are eligible. Locality/street-only and gap geocodes, lot-only addresses and Other Territories (OT) are excluded. Preserve the geocode type in provenance; the types are not equivalent positional-quality guarantees.
- Geometry is requested from ArcGIS in EPSG:4326. The original GDA2020 latitude/longitude attributes are retained too; do not silently interpret them as a surveyed WGS84 entrance. Property centroids can be distant from buildings on large parcels. A 150 m tolerance checks proximity to the published address point, not rooftop/entrance accuracy.
- Building, explicit flat-type, slash-unit and published level variants are generated from structured source fields. Unit cases accept the source's complete building number or unit/building composite at that same point. This tests geographic/building recovery, **not apartment identity**. Complete ranges stay ranges; no inferred endpoints or interpolated houses.
- Source locality/state/postcode qualifiers are retained. The captured ADDRESS_TYPE is not treated as proof of rural status; rural delivery routes and comprehensive locality-alias pairs remain unvalidated. Native street/locality spelling is preserved.
- The two-stage, pattern-balanced sample is diagnostic, not population-weighted national accuracy. Variants share a location-based hold-out; do not tune on held-out groups. Shared G-NAF ancestry in index inputs limits coordinate independence. Human review remains pending.
- SearchAddress checks country, house resolution, requested number, postcode, source-point proximity and goodMatch. SuggestAddress checks country/type/point among the first five, not unit identity. ReverseGeocode checks country only (same-point number alternatives remain eligible); ResolveRegion is unscored. Reverse passes do not establish building accuracy.

## Validation

The August 2026 service snapshot contains **15,949,543 records**, with OBJECTIDs 1–15,949,543 and edit timestamp 1787625204116. All 20,000 drawn IDs existed. The normalized gnaf.json is 24,932,643 bytes, SHA256 f1ec4aee3c228dfeaa590f62a8153b4096cd5a41ca00cee5dea2282dea6f3c34. Its request/response receipts and layer/item metadata are retained alongside it. The adapter finds 15,546 eligible building groups; the final draw has **326 locations, 522 queries and 61 held-out locations**, covering all eight included state/territory codes.

Local SearchAddress results:

| Pattern class | Locations | Queries | Passes |
|---|---:|---:|---:|
| Plain | 96 | 96 | 95 |
| Suffix | 76 | 76 | 76 |
| Complete range | 54 | 54 | 54 |
| Unit | 96 | 288 | 202 |
| Level | 4 | 8 | 8 |

Overall, 435/522 pass, 82 are too coarse and five are near misses. There are no critical results requiring baseline labels. All 522 full-string SuggestAddress checks fail, while all 522 country-only ReverseGeocode checks pass. For example, `22 PUNYIBAH STREET, CRACE ACT 2911, Australia` resolves to the indexed house within 3 m of the reference but its full-string suggestion is empty. Do not remove state/postcode qualifiers or rewrite the corpus merely to make those suggestions pass. Causes of all noncritical failures have not been isolated. Re-scoring the saved run changed zero classes.

These are diagnostic counts on the documented last-known-good local full world index, builder 4fb1fd818b46b2f6 / fingerprint e6df26bed27b, not validation of a successful current-builder rebuild. Both comparison sides use the identical runtime binary, libpostal models and reference data. No resolver, index builder, runtime dependency or scoring changes are part of this addition. The fresh-service grpcurl SearchAddress hang remains a known failure even though this run reports it passing.

Before the Danish addition, the complete 11,005-case candidate run passed its absolute gate with the eight attributed Spanish baseline failures still failing. Its comparison with the 8,939-case existing-suite baseline has zero answer/scoring drift across SearchAddress, SuggestAddress, ResolveRegion and ReverseGeocode: no newly failing or worse-class cases. Both new corpora's standalone results are unchanged in the full run. The local speed comparator passes, but its identical-binary head is 66–68% faster in total on a non-isolated, differently warmed host; this is timing noise/warm-up, not a speedup claim. See [Denmark's validation](dk.md#validation) for the later three-country run and its failed, confounded local speed comparison. Check current-head CI quality and measured speed separately in the pull request.
