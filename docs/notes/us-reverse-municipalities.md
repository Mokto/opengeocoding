# US reverse-house municipality membership

Follow-up to [#345](https://github.com/Mokto/opengeocoding/issues/345), independent of the Polish [#522](https://github.com/Mokto/opengeocoding/pull/522) source-preservation fix. This does not resolve every US reverse-locality disagreement.

## Source and scope

The pinned input is the U.S. Census Bureau's **2026 TIGER/Line** state Place and County Subdivision shapefiles for the 50 states and DC (legal boundaries and names as of January 1, 2026; released 2026-10-14). Incorporated places are `MTFCC=G4110` and county subdivisions `G4040`. Only `FUNCSTAT=A`, an active government providing primary general-purpose functions, is included. Government status comes from that typed source field, never a name containing “Township”, a postal city or an unclassified Overture locality.

The files hold 16,143 active county subdivisions and 19,468 active incorporated places. GEOS reports every geometry valid, so all **35,611 governments / 50,051 polygon parts** are kept at full source resolution, rounded to 1e-7 degrees and delta-encoded, without simplification. NAD83 coordinates are used without datum transformation, as before. Names are `NAMELSAD` ("Klamath Falls city"). Quantization is checked again for geometric validity. Source GEOIDs retain distinct `cousub/` and `place/` namespaces.

The compressed input is approximately 55 MiB. `tests/registry/prepare-us-municipalities.py` downloads the files and reproduces the pin byte for byte. [The source manifest](../../data/us-municipalities.sources.json) pins every shapefile and the state crosswalk by SHA256, tool versions, counts and prepared-data fingerprints. [The notice](../../data/us-municipalities.NOTICE) records attribution and limitations. This is public-domain federal data; Census requests citation and does not endorse this service.

## Runtime behavior

The builder writes full-resolution `us-municipalities.bin` using the existing slabbed polygon/RTree format, plus `us-municipalities.json`, an independent source-ID/state/name table. These IDs are **not Overture place IDs**; no name-based entity crosswalk is invented. The index includes the source manifest and notice.

Only a US **house** result without an existing `municipality` is eligible. Exactly one distinct government must contain the **selected house's stored point**, and its source state must equal the rendered ISO state. An incorporated place outranks the township or New England town around it (Westfield city inside Westfield Washington township, Indiana); otherwise different overlapping governments abstain, including coextensive records. Population, bounding-box area and postal names never resolve them. Multiple polygon parts of the same government do not create a conflict.

The added value is the source's `NAME`. Existing city/town/village labels, display, house identity, coordinates, complete number, road, postcode, country and regions are not replaced. Forward search, suggestions and region resolution are unchanged. Old indexes lacking both optional US files still open; partial or malformed sidecars fail loading.

A boundary crossing anywhere in the house's **coordinate-encoding cell** prevents enrichment. The guard covers the asymmetric 2^-28 longitude/latitude cell produced by the house FST's flooring and half a 1e-7-degree polygon rounding unit. It queries nearby boundary slabs, not all polygon edges. This is a numerical representation guard, **not** an asserted Census/source positional-accuracy bound.

## Reproduction

The preparation helper needs Python 3 and DuckDB with its spatial extension. The pin was produced with Python 3.9.6, zlib 1.2.12 and DuckDB 2.0.0; different compression/library versions may change bytes and fail verification rather than silently refreshing them.

```sh
duckdb -c 'INSTALL spatial'
python3 tests/registry/prepare-us-municipalities.py \
  --cache ~/geodata/us-census-tiger2026 \
  --out data/us-municipalities.json.gz
```

The helper verifies cached/downloaded files, checks completeness, unique source IDs and that every entity sits in its own state's file, validates raw and quantized geometry, and requires the pinned prepared fingerprints before atomically replacing the output. There are no download/evaluation retries. A changed upstream response is an error, not an automatic new data version. Refresh the complete national capture, manifest, notice and builder hash together; never select gold locations.

The builder normally embeds this pin. `--us-municipalities JSON.GZ` supplies prepared diagnostic fixtures; output provenance records that override's digest rather than claiming the pinned capture. Neither the service nor index opening downloads or writes anything.

## Checks and limits

Authored synthetic cases cover positive membership, state disagreement, holes, overlapping governments, a source point at a quantization-sensitive edge, existing municipality preservation and unchanged selected houses. A separate check opens the same synthetic index without US sidecars and compares every pre-existing field. Removing the runtime hook makes the positive regression fail.

Two naming-only prototypes were rejected before publication: replacing canonical locality labels introduced regressions; deriving municipality type from alternate names lacked administrative evidence. Neither remains in this implementation.

### Controlled full-index validation

Base builder `3cc9d538e47c5d86` and candidate `73dce4e5f7022326` use one owned APFS-frozen source snapshot: October Overture plus the same OpenAddresses, OSM, landmark, Korean-shop and GeoNames inputs. Its 5,834 files total 215,430,030,305 bytes; the relative-path SHA256 inventory hashes to `4e0ff599e9980030dd99bb2087357a4044fd55f3043447efe93a7719912fb030`. Shared indexes and input caches were not rebuilt or modified.

Both sides use the **same consumer binary**, models, reference data and 46,407-case corpus. An unchanged baseline repeat passes the strict scored-answer/class drift check. The full two-index comparison and an isolated control (a copy of the candidate index with only the two optional US runtime files removed) both pass the relative quality gate: **27 additional at-point and 26 additional offset checks**, no newly failing or worse classes, confined to the US. Other RPC scores and OpenStreetMap-control pass counts are unchanged. The standalone US corpus passes its absolute gate; the full suite retains existing absolute-gate failures.

| NAD mode/split | Passing query checks before → after | Passing source groups before → after |
|---|---:|---:|
| At-point, training | 470 → 491 / 656 | 306 → 318 / 427 |
| At-point, held-out aggregate | 142 → 147 / 194 | 91 → 94 / 123 |
| Offset, training | 482 → 503 / 656 | 314 → 326 / 427 |
| Offset, held-out aggregate | 147 → 152 / 194 | 95 → 98 / 123 |

The registry supplies 52 of the added passing checks; a curated city case supplies the remaining at-point check. These are correlated source-fidelity checks, **not national accuracy estimates**. Held-out reporting remains aggregate-only; no scoring, expectations, labels or split assignments changed.

The isolated comparison preserves selected house layer, point, complete number, road, postcode, country, regions and every existing locality name across all **85,222 reverse records**. It enriches 1,638 locality-name lists. All **128 number FSTs**, `places.tsv`, `areas.bin`, and the AU/DK/PL sidecars are byte-identical across the independently built indexes. Two raw suggestion candidate lists vary between the isolated runs without changing scores/ranks; three also vary in the unchanged baseline repeat. This existing tie variability is not a claim that every raw suggestion array is deterministic.

Formatting, all-target clippy with warnings denied, ordinary tests (including 110 synthetic checks) and the separately invoked model-backed synthetic test pass. The real-index RPC suite reproduces the same two failures with and without the US sidecars: the documented grpcurl timeout and the existing oversized-log assertion. No tests or assertions were weakened.

Before PR publication, the change was rebased onto `2db3ed6`, retaining the Canadian and New Caledonian changes. Both indexes were rebuilt from the same frozen inputs: main builder `53ebdb7afad97a81`, candidate/chart `1425772ff671bc82`. The refreshed **46,511-case** two-index and isolated comparisons again pass with exactly **+27 at-point / +26 offset** checks and no quality regressions. The strict baseline repeat passes; 133 files remain byte-identical. Identity/name preservation now covers **85,430 reverse records**, with the same 1,638 enriched lists and unchanged NAD table above. Formatting, all-target clippy, ordinary tests (113 synthetic checks) and separately invoked model-backed synthetic checks pass after the rebase; the same two known RPC failures reproduce.

The new polygon sidecar is approximately 167 MiB, plus a 1.5 MiB label table. CI full-index quality and **measured request speed have not run**; local timing tables do not establish speed performance. Both CI quality and measured request speed must pass before merge.

### Source switch to TIGER/Line

The results above used the TIGERweb BAS 2026 layers, served from Web Mercator storage. Its coordinates carry reprojection noise (`-121.85333799981449` for TIGER/Line's `-121.853338`), and GEOS rejected 1,072 of its governments and 38 of its CDPs, among them Atlanta, Phoenix, Austin, Omaha, Charlotte, Klamath Falls and the Spring Hill, Florida CDP. Those were left out rather than repaired. The 2026 TIGER/Line shapefiles carry the same January 1, 2026 legal boundaries with no invalid geometry, so both pins now come from them, with nothing excluded but the territories (whose files are not downloaded).

The two sources agree on all but a few entities. Among the 50 states and DC, 16,142 county subdivisions, 19,466 places and 12,545 CDPs are in both; one name is spelled differently ("La Grand township" is "LaGrand township" in TIGER/Line). TIGERweb alone has 1 county subdivision, 3 places and 1 CDP, TIGER/Line alone 1, 2 and 12, as with Ithaca, Ohio, now a CDP, and South Orange, New Jersey, now a village. Some shared entities differ in shape, such as Lac La Belle, Wisconsin, which took in most of the Town of Oconomowoc. TIGER/Line, the Census Bureau's published release of the vintage, is the source of record for those.

On one frozen source snapshot (October Overture), main's builder `fa7e87e136675d09` and the candidate were each built in full, and an isolated control served the candidate index with the old sidecars. Over the full 46,511-case gold suite, the scores are unchanged in every RPC, with no newly passing or failing case. The strict baseline repeat has no drift. Control against candidate, every reverse answer, country, region and pass class is unchanged; 259 reverse place lists change, 58 of them held out (aggregate only). Among the 201 training records:

- 178 records (61 locations) gain a `municipality` from a government that TIGERweb had invalid: Atlanta, Sioux Falls, Wichita, Austin, Phoenix, Klamath Falls, Tumwater, Dardenne Prairie and others.
- 19 records (7 locations) lose a township `municipality`. In Indiana and Illinois, a city now valid in TIGER/Line (Westfield, Goshen) lies inside an active township too, and two containing governments abstain. The locality change below names the city instead.
- 4 records (1 location) are named Spring Hill instead of Brookridge: the house is inside the Spring Hill CDP, which TIGERweb had invalid.

The new municipalities are typed names ("Klamath Falls city"), so they do not satisfy the registry's plain `Klamath Falls`, and the wrong locality (Altamont) stays. That is why the score does not move on its own. Separately built indexes also differ in which of two tied streets a few street-level reverse answers name, in the same places with or without this change.

The government sidecar grows to approximately 199 MiB and the CDP one to 57 MiB.

### Locality from the incorporated place

When exactly one incorporated place (`place/`, a city, town, village or borough) holds the selected house, it names the house's locality, without its kind ("Klamath Falls"). The rules, in `reverse_locality`:

- An existing city/town/village of the same name, or one of whose Overture names is that name, stays as it is.
- An existing locality whose own point lies inside the same place (a neighbourhood, a CDP that predates an annexation, Dardenne inside Dardenne Prairie) stays, and the place is added after it under the key of its kind, when that key is free.
- An existing locality that is itself the name of another incorporated place in the state stays and nothing is added: Overture's boundary and the Census one disagree about which place holds the house (Cooper City and Davie, Florida, along Stirling Road).
- Otherwise the existing locality, chosen for being near, is replaced (Altamont by Klamath Falls, Black Eagle by Great Falls, the Town of Osceola by Dresser).
- Places of other kinds ("city and borough", consolidated and metropolitan governments) are left alone, and the CDP rules apply only where no incorporated place holds the house.

On the same frozen snapshot, against main's index, the full gold suite gains **+3 at-point and +7 offset checks**, all US (2,314 → 2,324 of 2,734), with nothing newly failing or worse; the strict baseline repeat has no drift. Every reverse answer, country and region is unchanged, apart from the tied-street build noise above. Held-out cases changed only in aggregate (41 pass to pass, 1 wrong-place to wrong-place). Training changes: 10 checks pass (Klamath Falls, Great Falls, Dresser, Dardenne Prairie offset), 2 move from wrong place to wrong house (Dardenne Prairie, whose house was already the wrong one), and 133 passing records change names, mostly a `municipality` city where the township abstained, plus replaced nearby settlements (Neheb by Oviedo, University of Virginia by Charlottesville) and places added where Overture named none (Mobile). A first version without the other-incorporated-place rule replaced Cooper City by Davie at a Wikidata point 48 m inside Davie by Census boundaries, failing that case; the rule keeps it.
 Formatting, clippy, `cargo test` and the model-backed synthetic test pass; the two known RPC failures reproduce.

Census boundaries are statistical representations, not legal land descriptions or a determination of jurisdictional authority, property ownership or survey accuracy. Positional accuracy depends on source material and is not bounded by the returned coordinate precision. January 2026 boundaries can differ from later annexations. Invalid geometries, ambiguous overlaps, missing governments and non-A statuses are omissions. CDPs and unincorporated communities are not municipalities; postal-only expectations and missing settlement membership remain unresolved.
