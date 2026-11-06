#!/usr/bin/env python3
"""District-level gold seeds for Indonesia from Wikidata, checked against BPS's district table.

A district (kecamatan, distrik in Papua) is written after its regency or city as addresses write
it, without and with the labels: "Gubeng, Surabaya, Jawa Timur, Indonesia" and "Kec. Gubeng, Kota
Surabaya, Jawa Timur, Indonesia". Each comes from Wikidata (CC0): an item with a seven-digit
Kemendagri code (P1588), coordinates and one regency -> province chain (P131). It is eligible only
when OCHA's copy of the BPS district table (COD-AB, CC BY 3.0 IGO) has a district of that code and
name, the Wikidata point lies within twice that district's circle-equivalent radius (plus 1 km)
of BPS's centre, and no other Wikidata district of the regency has the name; the tolerance is that
radius doubled, at least 2 km and at most 25 km. Expectations come later from `evaluate
build-gold`, which reads each item from Wikidata again. Districts are picked in seeded-hash order.
Shares the reading of the workbook and Wikidata with sample-villages.py."""
import argparse
import hashlib
import importlib.util
import json
import math
import re
from pathlib import Path

spec = importlib.util.spec_from_file_location("villages", Path(__file__).with_name("sample-villages.py"))
villages = importlib.util.module_from_spec(spec)
spec.loader.exec_module(villages)

DISTRICTS = """SELECT ?item ?code ?point ?label ?kabLabel ?provLabel WHERE {
  ?item wdt:P1588 ?code ; wdt:P625 ?point ; wdt:P131 ?kab .
  FILTER(STRLEN(?code) = 7 && STRSTARTS(?code, "%s"))
  FILTER NOT EXISTS { ?item wdt:P576 [] }
  ?kab wdt:P131 ?prov . ?prov wdt:P31 wd:Q5098 .
  ?item rdfs:label ?label FILTER(LANG(?label) = "id")
  ?kab rdfs:label ?kabLabel FILTER(LANG(?kabLabel) = "id")
  ?prov rdfs:label ?provLabel FILTER(LANG(?provLabel) = "id")
}"""


def read_districts(items):
    """Districts with one code, point, label and regency -> province chain."""
    out = []
    for qid, rows in items.items():
        values = lambda key: {r[key]["value"] for r in rows}
        chains = {(r["kabLabel"]["value"], r["provLabel"]["value"]) for r in rows}
        if any(len(values(k)) != 1 for k in ("code", "point", "label")) or len(chains) != 1:
            continue
        m = re.match(r"Point\(([-\d.eE]+) ([-\d.eE]+)\)", rows[0]["point"]["value"])
        if not m:
            continue
        (kab, prov), = chains
        name = re.sub(r"^(Kecamatan|Distrik)\s+", "", rows[0]["label"]["value"])
        out.append({"qid": qid, "code": rows[0]["code"]["value"], "name": name, "kab": kab, "prov": prov,
                    "lon": float(m.group(1)), "lat": float(m.group(2))})
    return out


def vet(districts, bps):
    """The districts BPS knows by the same code and name, near its centre and alone in their
    regency (the code's first 4 digits) by name, each with its tolerance; and the rejections."""
    by_regency = {}
    for d in districts:
        by_regency.setdefault((d["code"][:4], villages.fold(d["name"])), []).append(d["qid"])
    eligible, rejected = [], {"no_bps_district": 0, "bps_name_differs": 0, "far_from_bps": 0, "ambiguous": 0, "odd_label": 0}
    for d in districts:
        b = bps.get("ID" + d["code"])
        if b is None:
            rejected["no_bps_district"] += 1
        elif villages.fold(b[0]) != villages.fold(d["name"]):
            rejected["bps_name_differs"] += 1
        elif re.search(r"[/\\()]", d["name"] + d["kab"]):
            rejected["odd_label"] += 1
        elif len(by_regency[(d["code"][:4], villages.fold(d["name"]))]) > 1:
            rejected["ambiguous"] += 1
        elif villages.km(d["lat"], d["lon"], b[1], b[2]) > 2 * math.sqrt(b[3] / math.pi) + 1:
            rejected["far_from_bps"] += 1
        else:
            radius = math.sqrt(b[3] / math.pi)
            eligible.append(dict(d, radius_km=round(min(25.0, max(2.0, 2 * radius)), 1)))
    return eligible, rejected


def queries(d):
    labelled_kab, plain_kab = villages.regency(d["kab"], d["code"])
    # Papua's provinces, all coded 9x, call theirs distrik
    label = "Distrik" if d["code"].startswith("9") else "Kec."
    return [
        f"{d['name']}, {plain_kab}, {d['prov']}, Indonesia",
        f"{label} {d['name']}, {labelled_kab}, {d['prov']}, Indonesia",
    ]


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--codab", required=True, help="idn_admin_boundaries.xlsx from HDX's cod-ab-idn")
    ap.add_argument("--count", type=int, default=300)
    ap.add_argument("--seed", default="id-districts-2026-11")
    ap.add_argument("--out", default="tests/gold.id-districts.seeds.jsonl")
    ap.add_argument("--manifest", default="tests/id-villages/districts.manifest.json")
    args = ap.parse_args()

    bps = villages.bps_villages(args.codab, level=3)
    items, hashes = {}, {}
    for prov in sorted({p[2:4] for p in bps}):
        body, rows = villages.sparql(DISTRICTS % prov)
        print(prov, len(rows), flush=True)
        hashes[prov] = hashlib.sha256(body).hexdigest()
        for r in rows:
            items.setdefault(r["item"]["value"].rsplit("/", 1)[1], []).append(r)

    districts = read_districts(items)
    eligible, rejected = vet(districts, bps)
    key = lambda d: hashlib.sha256(f"{args.seed}:{d['qid']}".encode()).hexdigest()
    chosen = sorted(eligible, key=key)[: args.count]
    with open(args.out, "w") as f:
        for d in sorted(chosen, key=lambda d: d["code"]):
            seed = {"kind": "place", "cohort": "district", "qid": d["qid"], "queries": queries(d),
                    "radius_km": d["radius_km"], "tags": ["id-districts"]}
            f.write(json.dumps(seed, ensure_ascii=False) + "\n")

    Path(args.manifest).write_text(json.dumps({
        "seed": args.seed,
        "count": args.count,
        "codab": {"file": Path(args.codab).name, "sha256": hashlib.sha256(Path(args.codab).read_bytes()).hexdigest(), "districts": len(bps)},
        "wikidata": {"items": len(items), "districts": len(districts), "eligible": len(eligible), "rejected": rejected,
                     "selected": len(chosen), "response_sha256": hashes},
    }, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
