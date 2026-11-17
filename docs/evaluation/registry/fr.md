# France

[Evaluation guide](../README.md) · [Source notice](../../../tests/registry/fr.NOTICE)

**France** (`fr.seeds.jsonl`, `fr.jsonl`): 505 locations drawn directly from the Base Adresse Nationale CSV export last modified on 2026-10-16 (Licence Ouverte / Open Licence 2.0), independently of Overture and the index. `sample-fr` keeps municipality-certified addresses with an entrance, building or parcel position, takes the smallest seeded SHA-256 hashes in each department, then selects five distinct communes and street names per department. Paris, Lyon and Marseille arrondissements have separate commune codes. All 96 metropolitan and five overseas departments are covered; other collectivities and Monaco are outside this sample. This balances departments, not population or urban/rural density, and favours communes that publish certified points; it is not a representative national accuracy sample.

```sh
target/release/evaluate sample-fr --ban adresses-france-2026-10-16.csv.gz --seed fr-2026-10 --out tests/registry/fr.seeds.jsonl
target/release/evaluate build-gold --seeds tests/registry/fr.seeds.jsonl --out tests/registry/fr.jsonl --ban adresses-france-2026-10-16.csv.gz
target/release/evaluate run --corpus tests/registry/fr.jsonl --index out --out eval-results/fr
```

The compressed export is 938,579,415 bytes, SHA-256 `34bc460429e6b042986ea4296dc8c3f82b308cb3a2ac877e8ee3533d89303680`. Retain that file to reproduce the sample: [the BAN download](https://adresse.data.gouv.fr/data/ban/adresses/latest/csv/adresses-france.csv.gz) moves with each release. Each generated case records the export filename and checksum. `build-gold --ban` re-reads the cited row by its BAN id; no geocoder chooses or verifies the expectation. House number (including its suffix), postcode, street and coordinates come from that row. The tolerance is 150 m, or 300 m for a parcel point. The sample does not infer ISO region codes from postcodes, so it does not check ResolveRegion. Overseas departments accept their own country code or `fr`.

Three locations per department also get one of ten edits, balanced among applicable kinds: no country, no postcode, uppercase, lowercase, no commas, reversed parts, a transposed pair of letters in the street or town, another sampled postcode, or another sampled town in the department. These 303 variants share their base location's expectation and hold-out flag. Consistent edits require goodMatch true; typos and conflicting parts accept either value. The sampler retains only 200 candidates per department before enforcing diversity, so a small department can yield fewer than five locations. No point or query is selected using a service response.

On one local full-index run (builder `0a3e9077e88a1702`, Overture 2026-09-23.1 with the optional sources from the local build), 460 of 505 base locations and 267 of 303 variants passed; 86 of 98 held-out base locations passed. At introduction, six critical results at three locations were triaged in `fr.labels.jsonl`: compound town names lead to Couarde or Romain hundreds of kilometres away despite a correct structured house lookup, and a house BAN certifies in Grand-Santi is indexed within 2 m but under Suriname. Other failures remain visible and unlabelled pending diagnosis. They include spaced house suffixes (`7 bis` falls back to the street while `7bis` finds the house), missing houses or streets, and damaged inputs. Expectations were not relaxed and no resolver behaviour changed. ReverseGeocode returned an accepted country for 504 of 505 base points; SuggestAddress found none of the 505 complete base-address strings. A same-binary, same-index repeat kept every class but changed the wrong house returned for the damaged Châlons-du-Maine query (La Torlière versus La Petite Torlière); the strict zero-drift check therefore failed. That run also failed the then-unresolved Rue de Rivoli must-pass case. These are development observations, not a clean comparison, production accuracy or independent truth: the index's French addresses and BAN share upstream data.

Each case's `places`, the names ReverseGeocode's answer must include one of, are BAN's `nom_commune`.

## Messy inputs

`fr-messy.seeds.jsonl` and `fr-messy.jsonl` (300 cases, 241 locations) write 100 of the `fr_house` locations three ways, from the same BAN export; `tests/registry/fr-messy.py` turns the seeds into queries and `evaluate build-gold` takes the expectation from BAN as for `fr.jsonl`:

- `fr_messy_venue` (100): a venue name before the street ("Salle polyvalente, 42 Rue Hélène Boucher, 28240 La Loupe, France").
- `fr_messy_clutter` (100): a building or estate word between the street and the town ("44 Rue Clos du Haras, Zone d'activités, 14400 Vaucelles, France").
- `fr_messy_abbrev` (100): the street type abbreviated ("28 R. de la Mare", "5 Imp. rudlin": Rue, Avenue, Boulevard, Chemin, Impasse, Route, Place, Allée).

The venue and clutter words come from a short fixed list of generic ones: they stand for the shapes real postal addresses carry, not for facts about the place, and the expectation of a confident answer is left open. The shapes are copied from MESSY STREETS inputs, so this is not a hold-out.

On the production index of 2026-10-21 (SearchAddress `pass` only) 96, 95 and 99 of 100 pass. France's misses in MESSY STREETS therefore do not come from these forms: they are streets the index lacks and venue names with no street, which a registry draw cannot reproduce. The cohort is a regression guard.
