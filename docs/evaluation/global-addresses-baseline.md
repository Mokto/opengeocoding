# Initial global address diagnostic baseline

These are **source-consistency counts, not national accuracy estimates**. The panel contains 50 address groups per market, not 100 independent buildings; the two component orders are deliberate probes, not universally native mailing formats. See [method, sources and exclusions](global-addresses.md).

The resolver, scorer and index builder are unchanged by this dataset addition. On the pinned full index, the new panel has **2,827/4,796 forward passes**, **2,913/4,796 first-five suggestion geography matches**, and **4,794/4,796 country-only reverse matches**. ResolveRegion is unscored. Forward results include **59 critical diagnostics** (54 far misses and five wrong-country results). The benchmark's absolute gate **fails**; no failure exemptions or weaker expectations were added. These diagnostics require source/index/parser/matching attribution before fixes.

A second run with the same immutable binaries, models and index had **zero answer or class changes** across all 4,796 cases, excluding timings. Each run used one request per operation: this is not a latency comparison, production throughput estimate or performance claim. Before this panel was added to the default gold inputs, the existing 16,483-case suite passed its absolute quality gate locally. The combined suite now has 21,279 cases and retains the panel's untriaged failures; inclusion does not make its absolute gate pass.

## Country panel

`Primary` is number-before-street; `alternate` is street-before-number. Identical query strings occur only once. `Suggest` tests first-five country/type/point geography, not exact civic identity. `Critical` counts forward critical diagnostics, not confirmed resolver defects.

| Market | Forward pass / queries | Primary / 50 | Alternate / queries | Suggest / queries | Critical |
| --- | ---: | ---: | ---: | ---: | ---: |
| AE | 47/99 | 12/50 | 35/49 | 83/99 | 2 |
| AR | 81/100 | 37/50 | 44/50 | 88/100 | 2 |
| AT | 91/100 | 45/50 | 46/50 | 0/100 | 0 |
| AU | 32/100 | 30/50 | 2/50 | 0/100 | 4 |
| BE | 86/100 | 42/50 | 44/50 | 78/100 | 0 |
| BR | 32/100 | 15/50 | 17/50 | 28/100 | 0 |
| CA | 25/100 | 16/50 | 9/50 | 29/100 | 8 |
| CH | 91/100 | 45/50 | 46/50 | 98/100 | 0 |
| CL | 16/100 | 8/50 | 8/50 | 16/100 | 5 |
| CN | 42/100 | 22/50 | 20/50 | 86/100 | 2 |
| CO | 0/99 | 0/50 | 0/49 | 0/99 | 6 |
| CZ | 76/100 | 37/50 | 39/50 | 82/100 | 0 |
| DE | 91/100 | 46/50 | 45/50 | 38/100 | 0 |
| DK | 96/100 | 48/50 | 48/50 | 98/100 | 0 |
| EG | 31/100 | 3/50 | 28/50 | 12/100 | 0 |
| ES | 34/100 | 16/50 | 18/50 | 32/100 | 3 |
| FI | 78/100 | 39/50 | 39/50 | 50/100 | 0 |
| FR | 78/100 | 40/50 | 38/50 | 76/100 | 0 |
| GB | 55/100 | 43/50 | 12/50 | 90/100 | 0 |
| GR | 91/100 | 46/50 | 45/50 | 82/100 | 0 |
| HU | 99/100 | 49/50 | 50/50 | 100/100 | 0 |
| ID | 70/100 | 34/50 | 36/50 | 68/100 | 2 |
| IE | 58/100 | 39/50 | 19/50 | 80/100 | 0 |
| IL | 95/100 | 48/50 | 47/50 | 94/100 | 0 |
| IN | 38/99 | 25/50 | 13/49 | 40/99 | 1 |
| IT | 90/100 | 44/50 | 46/50 | 94/100 | 0 |
| JP | 0/100 | 0/50 | 0/50 | 0/100 | 3 |
| KR | 62/100 | 21/50 | 41/50 | 45/100 | 0 |
| MX | 19/100 | 9/50 | 10/50 | 14/100 | 1 |
| MY | 47/100 | 28/50 | 19/50 | 64/100 | 0 |
| NG | 41/100 | 31/50 | 10/50 | 56/100 | 7 |
| NL | 97/100 | 48/50 | 49/50 | 98/100 | 0 |
| NO | 95/100 | 45/50 | 50/50 | 100/100 | 0 |
| NZ | 38/100 | 34/50 | 4/50 | 32/100 | 0 |
| PH | 49/99 | 33/50 | 16/49 | 67/99 | 0 |
| PL | 89/100 | 45/50 | 44/50 | 91/100 | 0 |
| PT | 21/100 | 11/50 | 10/50 | 22/100 | 0 |
| RO | 72/100 | 24/50 | 48/50 | 100/100 | 0 |
| RU | 89/100 | 44/50 | 45/50 | 92/100 | 4 |
| SA | 62/100 | 34/50 | 28/50 | 90/100 | 1 |
| SE | 94/100 | 46/50 | 48/50 | 94/100 | 0 |
| SG | 66/100 | 45/50 | 21/50 | 62/100 | 0 |
| TH | 34/100 | 21/50 | 13/50 | 44/100 | 0 |
| TR | 79/100 | 39/50 | 40/50 | 87/100 | 2 |
| TW | 2/100 | 1/50 | 1/50 | 12/100 | 0 |
| US | 38/100 | 34/50 | 4/50 | 59/100 | 1 |
| VN | 54/100 | 25/50 | 29/50 | 47/100 | 5 |
| ZA | 56/100 | 47/50 | 9/50 | 95/100 | 0 |

## Default-gold integration check

A full 21,279-case default-gold run includes all 4,796 panel cases. The relative quality comparison passes across the existing 16,483 shared cases, with unchanged pass counts for all RPCs and no newly failing or worse classes. Every panel answer/class matches the standalone baseline. One unscored fifth suggestion for `Washington, PA` varies between runs of the same immutable service binary; its first result and pass outcome are unchanged.

The combined absolute gate still fails on the panel's 59 untriaged critical diagnostics. The local one-repeat speed comparison also fails (SearchAddress total +14%, SuggestAddress +46%, ReverseGeocode +33%); the run overlapped a Docker build and used the same service binary, so this is not evidence of a code-induced slowdown or a speed pass. The built evaluation image exports exactly 21,279 cases, including 4,796 panel cases, with the same combined-corpus hash as the local evaluator and the ODbL notice present.

## Source and boundary cautions still requiring adjudication

- `global-osm-ae-5505788221-{0,1}` reproduces a published node whose point is in the UAE but whose address tags say `Passiebloemweg`, `Utrecht`, `3452 CZ`. This is a source-content conflict, not evidence by itself that the resolver should return the UAE. The published record and expectations remain unchanged; do not tune the resolver to satisfy it without independent adjudication.
- `global-osm-ru-8450680627-{0,1}` has a Russia label from Natural Earth's mask while the index's country-only reverse result is `ua`. This is a boundary-convention disagreement requiring adjudication, not geopolitical sign-off from this benchmark.
- Zero forward passes for Japan/Colombia and two for Taiwan are diagnostic findings on these input variants and this index. They do not establish national failure rates, prove every case is a parser bug or justify removing compound civic identifiers. Japan's are [triaged](triage/japan-addresses.md): its OSM addresses were not indexed. So were Colombia's and Taiwan's, with Chile's, Mexico's and Portugal's ([triage](triage/osm-address-countries.md)).
- The evaluator's grpcurl probe reported a pass, but the separately documented fresh-service SearchAddress hang remains a known failure. This run does not establish full gRPC interoperability.

## Measurement identity

- Corpus SHA256: `f3004e5cb1c5e5d919841461106e080dda2f790b5cf18adfb6f444ecbbb1344e`.
- Consumer revision: `9b3294106c6ac35c39b80714792a528fc8a870fc`; dataset-only dirty worktree, no runtime logic edits.
- Service binary SHA256: `2b96f97099907fa68127c783047dc1eda374dbe7e6f54348b4214de0a823a66d`.
- libpostal models SHA256: `65627ea2114bb908b63d38a959853438fa097f39bb00a45a2139897bf713a4c7`.
- Index fingerprint: `index 74a1eec034f5`; builder `9d4006a4eec15abb`.
- Evaluator: `0.15.0`; local macOS arm64, one repeat, measured on 2026-10-20.
- Full runs stay outside version control; regenerate with the documented `evaluate run --corpus` command and the same index/model/source snapshots.
