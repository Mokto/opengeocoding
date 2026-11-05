# US reverse-house unincorporated communities

Follow-up to the [government-boundary lookup](us-reverse-municipalities.md). Census-designated places (CDPs) supply geographic community membership, **not municipal status or postal-city identity**.

## Source contract

The [Census Bureau definition](https://www.census.gov/programs-surveys/bas/information/cdp.html) describes CDPs as closely settled, named unincorporated communities. Their statistical boundaries have no legal status. CDPs cannot overlap incorporated places or other CDPs, but may cross county boundaries. A CDP's extent can differ from local understanding or present settlement patterns.

The pin captures the complete TIGERweb January 1, BAS 2026 CDP layer: **12,902 unique source entities**, all `FUNCSTAT=S`, `MTFCC=G4210`. Invalid geometries are excluded without repair (38 entities); 356 valid territory entities are excluded. The remaining **12,508 communities / 13,138 polygon parts** retain full source geometry rounded to 1e-7 degrees, without simplification. `BASENAME` supplies the community name; the technical `CDP` designation is not part of that name. Source IDs use a separate `cdp/<GEOID>` namespace.

[data/us-cdps.sources.json](../../data/us-cdps.sources.json) pins all 29 captured pages/metadata/state-crosswalk files and the prepared JSON/gzip hashes. The approximately 16 MiB gzip is federal public-domain data with [attribution and limitations](../../data/us-cdps.NOTICE). There is no lookup-time network access or write.

## Behavior

Only a **US reverse-house** result without a higher-priority explicit source city is eligible. The lookup uses the selected house's stored point, never the reverse query point. Exactly one distinct CDP must contain it, and the source state must agree with the rendered ISO state. Multiple parts of one source entity deduplicate. Overlaps, holes, state conflicts and numerically uncertain boundaries abstain. A containing incorporated-place government from the separate Census sidecar also prevents correction; county subdivisions may legitimately contain CDPs.

The numerical guard covers the house FST's asymmetric coordinate-encoding cell and half a polygon rounding unit. It is not a claimed Census positional-accuracy bound. Published coordinate precision is not source accuracy or legal boundary authority.

An already matching geographic locality is retained. So is a name that identifies an actual containing, typed Census government, including its unqualified name. The existing locality's own indexed native name variants can also show that its canonical name already represents the CDP; those names are preserved. These checks keep the existing name rather than replace it: names never establish governmental type, introduce aliases into other fields, or create a source-ID crosswalk. The CDP is then added below the kept name, as `village`, or `hamlet` when `village` is taken (a hamlet or village inside a New England, New York or New Jersey town, such as Vineyard Haven in Tisbury or Budd Lake in Mount Olive Township). Nothing is added when both keys are taken.

Otherwise the CDP replaces the primary `city`/`town`/`village` name and that name's display component. With no supplied settlement-size class or population, it uses the existing unclassified-locality fallback field, `village`; this is **not an assertion of legal village incorporation**. Administrative labels that happen to share the old locality's spelling remain in the display.

Selected house layer/identity, complete number, point, road, postcode, country, region and government `municipality` are unchanged. Forward search, suggestions, region resolution and non-house reverse fallbacks do not use CDPs. Existing postal/source-city priorities remain. Old indexes without both optional CDP files retain their previous behavior; partial files or an incorrect source namespace fail opening.

The builder writes `us-cdps.bin`, `us-cdps.json`, source provenance and `US-CDPS-NOTICE.txt`. `--us-cdps JSON.GZ` accepts diagnostic fixtures, with override fingerprints rather than a claim to the embedded pin. The government's independent sidecar and namespace remain unchanged.

## Reproduction and checks

Use the same Python/zlib and DuckDB versions recorded in the manifest. The existing Census preparation helper verifies the complete input inventory, source categories, IDs, raw geometry and quantized geometry before requiring byte-exact prepared fingerprints:

```sh
python3 tests/registry/prepare-us-municipalities.py \
  --manifest data/us-cdps.sources.json \
  --cache ~/geodata/us-census-bas2026-cdps \
  --out data/us-cdps.json.gz
```

A refresh must update the whole capture, manifest, prepared data, notice and builder hash together, never select evaluation locations. The helper's default government-data path still prepares the government pin, not CDPs.

Authored synthetic checks cover correction, a CDP added below a kept town (`village`) or village (`hamlet`), holes, overlaps, state mismatch, numerical boundary cells, incorporated-place conflicts, governing-town names, native-name variants, already-correct names, a locality/admin-name collision, missing/partial sidecars and an offset query on the other side of a boundary from the selected house. The no-CDP index control compares every field except the explicitly changed locality/display. Removing the runtime hook makes the two positive tests fail.

## Controlled local validation

### Follow-up: naming the community below a kept town

The first version dropped the CDP whenever the existing name was kept, so a house in Harmony (Glocester, Rhode Island) or Budd Lake (Mount Olive Township, New Jersey) still had only the town. Adding the CDP below the kept name, on the same local index and consumer models for both binaries (46,511 cases; strict baseline repeat passes), gives **+16 at-point / +13 offset** checks, all US, with no newly failing or worse cases. Across 85,430 reverse records the selected house, its point, number, road, postcode, country and regions are unchanged, no existing name is removed, and 87 records gain exactly one community name. The index files are unchanged; only the builder hash moves because `country/us.rs` is a builder input. CI measures quality and request speed on the full index.

An unconditional replacement prototype was rejected: it gained 58 checks but introduced 20 failures by discarding valid governing-town names and native canonical-name variants. It was not published. The narrower preservation rules above have authored negative checks, rather than case-name exceptions or weakened expectations.

Final base builder `1425772ff671bc82` and candidate/chart `a647b617fa37983d` use one owned APFS-frozen October snapshot: 5,834 files / 215,430,030,305 bytes, inventory SHA256 `e9414a52c6d4ae09a2b328307018f3071eaf6948dafa99dfb76d3980f8a00961`. Shared inputs and indexes were not modified. Both sides use the same consumer, models, reference data and 46,511-case corpus. An unchanged baseline repeat passes the strict scored-answer/class drift check.

Both the full two-index comparison and an isolated candidate-index control with only the two optional CDP runtime files removed pass the relative quality gate: **15 additional at-point / 14 additional offset checks**, confined to the US, no newly failing or worse classes. Other RPC scores and OSM-control pass counts are unchanged. Full absolute suites retain existing baseline failures.

| NAD mode/split | Passing query checks before → after | Passing source groups before → after |
|---|---:|---:|
| At-point, training | 491 → 504 / 656 | 318 → 328 / 427 |
| At-point, held-out aggregate | 147 / 194, unchanged | 94 / 123, unchanged |
| Offset, training | 503 → 517 / 656 | 326 → 337 / 427 |
| Offset, held-out aggregate | 152 / 194, unchanged | 98 / 123, unchanged |

The NAD registry supplies 27 added passing checks; two curated-place checks supply the other at-point gains. Held-out reporting remains aggregate-only. These correlated source-fidelity checks are **not national accuracy estimates**. One at-point `wrong_place` becomes `wrong_house`: the geographic correction exposes an existing house mismatch, with no change to the selected answer. No scoring, expectations, corpus labels or splits change.

Across **85,430 reverse records**, selected house layer/number/point/road/postcode, country, regions and known government names are preserved. Only 60 locality-name lists change. The 128 number FSTs, place and regional files, AU/DK/PL sidecars and existing US government polygon/label files—**135 files**—are byte-identical across builders. One raw suggestion candidate list varies without a score/rank change; unrelated tie variability is not treated as a CDP improvement.

Formatting, all-target clippy with warnings denied, ordinary tests (116 synthetic checks) and the separately invoked model-backed synthetic test pass. The real-index RPC suite reproduces the same two known failures with and without CDPs: grpcurl SearchAddress timeout and the oversized-log assertion. No assertions were weakened or retries added.

Before PR publication, the change was rebased onto `6b72b4b`, retaining the latest house-number suggestion and municipality-bounded settlement-assignment changes. Both indexes were rebuilt from the same frozen source snapshot: main builder `e5a572f9367ece4e`, candidate/chart `542240026746364e`. Fresh 46,511-case two-index and CDP-disabled-control comparisons again pass with **+15 at-point / +14 offset** checks, no quality regressions and the same split table above. The strict baseline repeat passes, identity preservation still covers 85,430 reverse records and 135 files remain byte-identical. Formatting, all-target clippy, ordinary tests (118 synthetic checks) and separately invoked model-backed synthetic checks pass after the rebase; the same two known RPC failures remain.

The CDP polygon sidecar is approximately 56 MiB, plus 451 KiB of labels. **CI quality and measured request speed have not run**; local timing tables do not establish performance. Both CI quality and measured request speed must pass before merge. Coverage omissions, dated statistical boundaries and missing/unreliable settlement membership remain limitations.
