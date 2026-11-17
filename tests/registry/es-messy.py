#!/usr/bin/env python3
"""Seeds for `es-messy.jsonl`: CartoCiudad addresses written the way the MESSY STREETS paper's Spanish inputs are.

  python3 tests/registry/es-messy.py tests/registry/es.seeds.jsonl > tests/registry/es-messy.seeds.jsonl
  python3 tests/registry/es-messy-capture.py tests/registry/es.jsonl tests/registry/es-messy.seeds.jsonl OUT/cartociudad.json
  python3 tests/registry/es-messy.py tests/registry/es.seeds.jsonl OUT/changed.txt > tests/registry/es-messy.seeds.jsonl
  (repeat the last two steps, adding each OUT/cartociudad.json.missing to OUT/changed.txt, until none is listed)
  evaluate build-gold --seeds tests/registry/es-messy.seeds.jsonl --out tests/registry/es-messy.jsonl --cartociudad OUT/cartociudad.json

The locations are the `es_plain` and `es_other_road` ones of `es.seeds.jsonl` whose street has no
digit; only the query text differs, and every expected answer still comes from CartoCiudad.
CartoCiudad has no postcode, so none of these queries has one.
"""
import hashlib
import json
import re
import sys

SEED = "es-2026-11-messy"
SIZE = 100
ABBREVIATIONS = {
    "CALLE": ["C.", "C/"], "AVENIDA": ["Avda.", "Avd."], "PLAZA": ["Pza.", "Plza."],
    "CARRETERA": ["Ctra."], "CAMINO": ["Cno."],
}
FLOORS = ["Bajo", "1º B", "2º A", "Entlo.", "Ático", "3º C"]


def key(what, ref):
    return int(hashlib.sha256(f"{SEED}|{what}|{ref}".encode()).hexdigest()[:16], 16)


def variant(line, cohort, query, tag):
    out = dict(line, cohort=cohort, queries=[query], tags=line["tags"] + ["variant", "messy", tag])
    out["good_match"] = None
    return out


SMALL = {"de", "del", "la", "las", "los", "el", "y", "en", "a", "i", "d"}


def title(text):
    words = text.lower().split()
    return " ".join(w if k and w in SMALL else w[:1].upper() + w[1:] for k, w in enumerate(words))


def parts(line):
    """(road type, street name, number, the rest) of a base query, or None."""
    m = re.match(r"^(C/|[A-ZÁÉÍÓÚ]+|Av\.|Ctra\.) ([^,\d]+) (\d+), (.+)$", line["queries"][0])
    return m.groups() if m else None


def main():
    gone = {l.strip() for l in open(sys.argv[2])} if len(sys.argv) > 2 else set()
    seen, bases = set(gone), []
    for line in map(json.loads, open(sys.argv[1])):
        if line["cohort"] in ("es_plain", "es_other_road") and line["ref"] not in seen and parts(line):
            seen.add(line["ref"])
            bases.append(line)
    out = []
    for name, tag in (("abbrev", "abbreviated_type"), ("number", "numero_sign"),
                      ("floor", "floor_word"), ("comma", "comma_before_number")):
        pool = bases
        if name == "abbrev":
            pool = [b for b in bases if parts(b)[0] in ABBREVIATIONS or parts(b)[0] == "C/"]
        for c in sorted(pool, key=lambda c: key(name, c["ref"]))[:SIZE]:
            kind, street, number, rest = parts(c)
            full = {"C/": "CALLE", "Av.": "AVENIDA", "Ctra.": "CARRETERA"}.get(kind, kind)
            name_ = title(street)
            if name == "abbrev":
                options = ABBREVIATIONS.get(full)
                if not options:
                    continue
                query = f"{options[key('w', c['ref']) % len(options)]} {name_} {number}, {rest}"
            elif name == "number":
                query = f"{title(full)} {name_} nº {number}, {rest}"
            elif name == "floor":
                floor = FLOORS[key("f", c["ref"]) % len(FLOORS)]
                query = f"{title(full)} {name_} {number} {floor}, {rest}"
            else:
                query = f"{title(full)} {name_}, {number}, {rest}"
            out.append(variant(c, f"es_messy_{name}", query, tag))
    for line in out:
        print(json.dumps(line, ensure_ascii=False, sort_keys=True))


main()
