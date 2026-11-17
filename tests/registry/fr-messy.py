#!/usr/bin/env python3
"""Seeds for `fr-messy.jsonl`: BAN addresses written the way the MESSY STREETS paper's French inputs are.

  python3 tests/registry/fr-messy.py tests/registry/fr.seeds.jsonl > tests/registry/fr-messy.seeds.jsonl
  evaluate build-gold --seeds tests/registry/fr-messy.seeds.jsonl --out tests/registry/fr-messy.jsonl --ban adresses-france-2026-10-16.csv.gz

The locations are the `fr_house` ones of `fr.seeds.jsonl`; only the query text differs, and every
expected answer still comes from BAN. The venue and clutter words are generic filler, not facts
about the place: they stand for the venue names, buildings and estates real postal addresses carry.
"""
import hashlib
import json
import re
import sys

SEED = "fr-2026-11-messy"
SIZE = 100
VENUES = ["Salle polyvalente", "Mairie", "Espace associatif", "École primaire", "Gymnase", "Médiathèque"]
CLUTTER = ["Bâtiment B", "Résidence du Parc", "Zone d'activités", "BP 12", "Appartement 4", "Escalier 2"]
ABBREVIATIONS = {
    "Rue": "R.", "Avenue": "Av.", "Boulevard": "Bd", "Chemin": "Ch.",
    "Impasse": "Imp.", "Route": "Rte", "Place": "Pl.", "Allée": "All.",
}


def key(what, ref):
    return int(hashlib.sha256(f"{SEED}|{what}|{ref}".encode()).hexdigest()[:16], 16)


def variant(line, cohort, query, tag):
    out = dict(line, cohort=cohort, queries=[query], tags=line["tags"] + ["variant", "messy", tag])
    out["good_match"] = None
    return out


def main():
    houses = [l for l in map(json.loads, open(sys.argv[1])) if l["cohort"] == "fr_house"]
    out = []
    for name, tag in (("venue", "venue_first"), ("clutter", "clutter_after_street")):
        for c in sorted(houses, key=lambda c: key(name, c["ref"]))[:SIZE]:
            street, rest = c["queries"][0].split(", ", 1)
            pick = lambda words: words[key(name + "w", c["ref"]) % len(words)]
            query = (f"{pick(VENUES)}, {street}, {rest}" if name == "venue"
                     else f"{street}, {pick(CLUTTER)}, {rest}")
            out.append(variant(c, f"fr_messy_{name}", query, tag))
    typed = [c for c in houses if re.match(r"^\d+\S* (?:bis |ter )?(%s) " % "|".join(ABBREVIATIONS), c["queries"][0])]
    for c in sorted(typed, key=lambda c: key("abbrev", c["ref"]))[:SIZE]:
        query = c["queries"][0]
        for word, short in ABBREVIATIONS.items():
            query = re.sub(r"^(\d+\S* (?:bis |ter )?)%s " % word, r"\g<1>%s " % short, query)
        if query != c["queries"][0]:
            out.append(variant(c, "fr_messy_abbrev", query, "abbreviated_type"))
    for line in out:
        print(json.dumps(line, ensure_ascii=False, sort_keys=True))


main()
