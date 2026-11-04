# New Caledonia: address inputs and postal evidence

The global OSM panel measures agreement with mapped address records, not national
accuracy. Its NC cohort has 50 locations, two query orders each, with eight locations
held out by the existing stable hash. Expectations, tolerances and that split are
unchanged.

## Proven import defects

The chart previously read `new-caledonia` for landmarks only. Its addressed nodes
therefore contributed no house-number keys. A missing ranked search result alone
was not used as proof: training records were checked in the original PBF, the
Overture and BAN-derived rows, and all stored street/number keys within 1 km.

Overture's NC address rows use `NC`, while BAN's OpenAddresses exports use `FR`.
An additional experiment merged their country keys, but full gold rejected that
approach: it changed postal-evidence scopes and lost existing matches. That
attempt was reverted. Country representation, territory guards and shared
recovery policies remain unchanged. No new locality or administrative membership
is inferred.

The chart now reads NC's OSM addresses as a supplement. The existing Overture,
OpenAddresses, OSM source-priority order is unchanged. Landmarks are still read by
the normal OSM pass. The source snapshot label is not refreshed: the fetcher can
reuse an extract already sealed in the other input list.

## Contradictory postcodes

[OPT-NC's postal-code reference](https://www.opt.nc/particuliers/courrier-et-colis/les-codes-postaux-de-nouvelle-caledonie)
lists five-digit codes beginning with `988`, including distinct home-delivery and
post-office-box codes. For example, it lists `98859` for Koné home delivery and
`98860` for its boxes. Neither code is rewritten into the other.

An address declared as NC, or assigned to an NC place, cannot contribute a house
point or a postcode representative when its nonempty source postcode contradicts
that namespace. Its street, original postcode and postal-town evidence remain
indexed so conflicting geography is not silently erased. Missing source postcodes
are allowed. Other countries' postcodes, including mainland France's `30500`, are
unaffected. Full civic numbers, suffixes and route digits are not shortened.

This excludes the house point of the training record tagged `saint-brès, 30500`
at an NC coordinate. It does **not** change that panel case's house expectation or
label it away. Reading an inconsistent source record must not be mistaken for
independently verifying its address.

## Controlled comparison

Both sides used the same consumer binary and libpostal models. Two full baseline
runs had zero answer drift. The postcode guard alone changed no answers across
the 46,407-case local gold suite; the address-input experiment then used exactly
the same pinned source files with only NC's input role changed.

| NC panel operation | Passing queries, before → after | Locations passing both orders, before → after |
| --- | ---: | ---: |
| SearchAddress | 58/100 → 92/100 | 29/50 → 45/50 |
| SuggestAddress | 58/100 → 92/100 | 29/50 → 46/50 |
| Reverse at point, number/postcode | 50/100 → 88/100 | 25/50 → 44/50 |
| Reverse 25 m away, proximity | 72/100 → 98/100 | 36/50 → 49/50 |

Held-out locations, reported only as aggregates: search and suggestions **5/8 →
8/8**, at-point reverse **4/8 → 7/8**, offset reverse **5/8 → 8/8**. Number-first
searches pass 47/50, street-first 45/50; suggestions pass 46/50 for either order.
The contradictory `30500` record deliberately remains a failing house case.
At-point reverse checks compare number, postcode and radius, not an independent
street/building identity crosswalk.

The full local comparison gains 34 search, 34 suggestion, 38 at-point reverse and
26 offset reverse checks, with no newly failing checks, worse failure classes or
abstain-to-miss transitions. Only the NC cohort changes. Absolute gold still fails
existing main failures; this is a clean **relative quality** comparison, not an
absolute pass or a speed claim. CI must check its own pinned index and measured
speed separately.

## Evidence and remaining limitations

The reproduced baseline is main `bce81e3`, builder `24e45dcd8295f18b`, on the
standard local Overture `2026-09-23.1` source set. A repeat and an independently
rebuilt control had zero NC answer drift. The stale issue #501 nightly is not the
before side of this comparison.

The pinned NC index-input extract is Geofabrik replication 4940, 2026-10-15,
SHA256 `c47cdc3f0af03b28de2b320784d50f31849281c0e9d1dfb92bacdcfbab21b064`.
The panel's replication-4945 extract, 2026-10-20, has SHA256
`bfe5c51a5df99ca72d66129233f6695be33dabd84659a63c53f579e232f18317`.
The 42 training nodes have identical versions, address tags and points in both.
The input extract has no interpolation ways; no interpolation anchors were
reintroduced into the corpus.

The source audit found 744 normalized OSM street/full-number keys absent from the
inspected BAN inputs and Overture, using either the September release or the
chart's October `2026-10-21.0` release. These are incremental keys, not 744 verified
physical buildings: aliases and duplicate-address points can remain. There are
282 repeated OSM keys, with 383 additional nodes. Seven of the 16,001 numbered
nodes carry a nonempty postcode outside the NC namespace. The built index gains
2,359 house-number FST keys after applying the guard; this is not a count of new
physical buildings. The namespace check does not validate allocation, delivery
capability or municipality membership. OSM's ODbL attribution and
share-alike obligations remain; the address-input switch does not change them.
See the existing `NOTICE` and `tests/global/NOTICE`.

Two training records disagree with BAN on postcode or position; three others use
different street/number representations without a verified alias crosswalk.
Three training houses already passing search have OSM/registry offsets of roughly
39–63 m: their nearest reverse result need not be the requested house. A passing
25 m offset check establishes nearby-house proximity, not that house's identity.
Do not move higher-priority source coordinates to satisfy the panel.

Untyped `Néméara`, a compound `Lot 302A Farino` house identifier, and trailing
`bis` query orders expose parser/representation limitations. This change does not
invent aliases, split those identifiers or add generic lookup fallbacks.
The deliberate shared country-repair behavior also remains unchanged; enabling
more addresses does not make `goodMatch` an independently calibrated accuracy
measure.

Independently authored cases in `tests/synthetic/world/nc.rs` cover foreign NC
postcodes, retained contradictory street evidence, unaffected French postcodes,
complete numbers, numeric routes, duplicate-source positions, missing-postcode
import eligibility, and explicit conflicting geography. Removing the
house-eligibility guard makes the foreign-postcode test fail. The attempted
country-key normalization and its extra confidence test were reverted together;
no corpus expectations or labels were changed.
