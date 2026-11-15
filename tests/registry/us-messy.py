#!/usr/bin/env python3
"""Seeds for `us-messy.jsonl`: US addresses written the way the MESSY STREETS paper's inputs are.

  evaluate sample-us --nad NAD_r24.txt --seed "us-2026-11-messy*8" --out candidates.jsonl
  python3 tests/registry/us-messy.py candidates.jsonl lacking.txt > tests/registry/us-messy.seeds.jsonl
  evaluate build-gold --seeds tests/registry/us-messy.seeds.jsonl --out tests/registry/us-messy.jsonl --nad NAD_r24.txt

`lacking.txt` is one NAD UUID per line: the draw's house addresses the index of 2026-10-21 did not
hold at their number (see docs/evaluation/registry/us.md). Every expected answer comes from NAD,
as in `us.jsonl`; only the query text and the choice of locations differ.
"""
import hashlib
import json
import sys

SEED = "us-2026-11-messy"
TYPES = ["Street", "Avenue", "Road", "Drive", "Boulevard", "Lane", "Court", "Circle", "Place"]
SIZE = 100


def key(what, ref):
    return int(hashlib.sha256(f"{SEED}|{what}|{ref}".encode()).hexdigest()[:16], 16)


def with_query(line, cohort, query, tags, good_match=...):
    out = dict(line, cohort=cohort, queries=[query], tags=line["tags"] + tags)
    out.pop("label", None)
    out.pop("good_match", None)
    if good_match is not ...:
        out["good_match"] = good_match
    return out


def main():
    cands = [json.loads(l) for l in open(sys.argv[1])]
    lacking = {l.strip() for l in open(sys.argv[2]) if l.strip()}
    houses = [c for c in cands if c["cohort"].startswith(("us_house", "us_broad"))]
    out = [
        with_query(c, "us_no_house", c["queries"][0], ["variant", "index_lacked_house"])
        for c in houses
        if c["ref"] in lacking
    ]
    rest = [c for c in houses if c["ref"] not in lacking and c["cohort"].startswith("us_house")]
    for c in sorted(rest, key=lambda c: key("unit", c["ref"]))[:SIZE]:
        number, street_rest = c["queries"][0].split(" ", 1)
        letter = "ABCD"[key("letter", c["ref"]) % 4]
        street, tail = street_rest.split(", ", 1)
        out.append(with_query(c, "us_messy_unit", f"{number} {street} {letter}, {tail}",
                              ["variant", "stray_unit"], None))
    typed = [c for c in rest if c["queries"][0].split(", ")[0].split(" ")[-1] in TYPES]
    for c in sorted(typed, key=lambda c: key("type", c["ref"]))[:SIZE]:
        street, tail = c["queries"][0].split(", ", 1)
        words = street.split(" ")
        others = [t for t in TYPES if t != words[-1]]
        words[-1] = others[key("other", c["ref"]) % len(others)]
        out.append(with_query(c, "us_messy_type", f"{' '.join(words)}, {tail}",
                              ["variant", "wrong_street_type"], None))
    for line in out:
        print(json.dumps(line, ensure_ascii=False, sort_keys=True))


main()
