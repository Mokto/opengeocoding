#!/usr/bin/env python3
"""Village-level gold seeds for Indonesia from Wikidata, checked against BPS's village table.

Indonesian addresses name the village (desa/kelurahan), district (kecamatan), regency or city
(kabupaten/kota) and province, smallest first. Each village comes from Wikidata (CC0): an item
with a ten-digit Kemendagri code (P1588), coordinates and a single kecamatan -> kabupaten ->
province chain (P131). It is eligible only when OCHA's copy of the BPS village table (COD-AB,
CC BY 3.0 IGO) has a village with that code and the same name, and the Wikidata point lies within
twice that village's circle-equivalent radius (plus 1 km) of BPS's centre; the case's tolerance is
that radius doubled, at least 2 km and at most 25 km. BPS only vets the cases; expectations come
later from `evaluate build-gold`, which reads each item from Wikidata again.

Each village is written four ways: with the usual labels ("Desa Pepe, Kec. Tegowanu, Kab.
Grobogan, Jawa Tengah, Indonesia"), without them ("Pepe, Tegowanu, Grobogan, Jawa Tengah,
Indonesia"), as village and regency ("Pepe, Grobogan, Indonesia"), the last only when no other
eligible-or-not Wikidata village of the regency has the name, and with the labels but the
province shortened as addresses shorten it and no country ("Desa Pepe, Kec. Tegowanu, Kab.
Grobogan, Jateng"). Villages are picked in seeded-hash order."""
import argparse
import hashlib
import json
import math
import re
import time
import unicodedata
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

ENDPOINT = "https://query.wikidata.org/sparql"
USER_AGENT = "opengeocoding-eval/0.1 (https://github.com/Mokto/opengeocoding; test corpus curation)"
# Per province (the first two digits of the code): villages, then their districts with the
# regency and province above them. One query joining both times out for Java's provinces.
VILLAGES = """SELECT ?item ?code ?point ?label ?kec ?type WHERE {
  ?item wdt:P1588 ?code ; wdt:P625 ?point ; wdt:P131 ?kec ; wdt:P31 ?type .
  FILTER(STRLEN(?code) = 10 && STRSTARTS(?code, "%s"))
  FILTER NOT EXISTS { ?item wdt:P576 [] }
  ?item rdfs:label ?label FILTER(LANG(?label) = "id")
}"""
DISTRICTS = """SELECT ?kec ?kecLabel ?kabLabel ?provLabel WHERE {
  ?kec wdt:P1588 ?kc ; wdt:P131 ?kab . FILTER(STRLEN(?kc) = 7 && STRSTARTS(?kc, "%s"))
  ?kab wdt:P131 ?prov . ?prov wdt:P31 wd:Q5098 .
  ?kec rdfs:label ?kecLabel FILTER(LANG(?kecLabel) = "id")
  ?kab rdfs:label ?kabLabel FILTER(LANG(?kabLabel) = "id")
  ?prov rdfs:label ?provLabel FILTER(LANG(?provLabel) = "id")
}"""
# Provinces as addresses shorten them; the others are written out.
SHORT = {
    "Aceh": "NAD", "Sumatera Utara": "Sumut", "Sumatera Barat": "Sumbar", "Sumatera Selatan": "Sumsel",
    "Kepulauan Bangka Belitung": "Babel", "Kepulauan Riau": "Kepri", "Jakarta": "DKI Jakarta",
    "Daerah Khusus Ibukota Jakarta": "DKI Jakarta", "Jawa Barat": "Jabar", "Jawa Tengah": "Jateng",
    "Jawa Timur": "Jatim", "Daerah Istimewa Yogyakarta": "DIY", "Kalimantan Barat": "Kalbar",
    "Kalimantan Tengah": "Kalteng", "Kalimantan Selatan": "Kalsel", "Kalimantan Timur": "Kaltim",
    "Kalimantan Utara": "Kaltara", "Sulawesi Utara": "Sulut", "Sulawesi Tengah": "Sulteng",
    "Sulawesi Selatan": "Sulsel", "Sulawesi Tenggara": "Sultra", "Sulawesi Barat": "Sulbar",
    "Nusa Tenggara Barat": "NTB", "Nusa Tenggara Timur": "NTT", "Maluku Utara": "Malut",
    "Papua Barat": "Pabar",
}
# The word an address puts before the village's name, by its Wikidata type; "Desa" otherwise.
KINDS = {"Q965568": "Kelurahan", "Q4285979": "Gampong", "Q882149": "Nagari"}
NS = {"m": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
REL = "{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id"


def fold(text):
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode().lower()
    return " ".join(re.sub(r"[^a-z0-9]+", " ", text).split())


def bps_villages(xlsx, level=4):
    """adm4_pcode -> (name, lat, lon, area_sqkm) from the COD-AB workbook's idn_admin4 sheet, or
    the districts' from idn_admin3 with `level` 3."""
    with zipfile.ZipFile(xlsx) as z:
        strings = [
            "".join(t.text or "" for t in si.iter(f"{{{NS['m']}}}t"))
            for si in ET.fromstring(z.read("xl/sharedStrings.xml")).findall("m:si", NS)
        ]
        book = ET.fromstring(z.read("xl/workbook.xml"))
        rels = ET.fromstring(z.read("xl/_rels/workbook.xml.rels"))
        target = {r.get("Id"): r.get("Target") for r in rels}
        sheet = next(s for s in book.find("m:sheets", NS) if s.get("name") == f"idn_admin{level}")
        path = "xl/" + target[sheet.get(REL)].lstrip("/").removeprefix("xl/")
        rows = ET.fromstring(z.read(path)).find("m:sheetData", NS)

        def cells(row):
            out = {}
            for c in row:
                col = re.match(r"[A-Z]+", c.get("r")).group()
                v = c.find("m:v", NS)
                if v is None:
                    v = c.find("m:is/m:t", NS)
                text = None if v is None else v.text
                out[col] = strings[int(text)] if c.get("t") == "s" else text
            return out

        it = iter(rows)
        header = {v: k for k, v in cells(next(it)).items()}
        col = lambda name: header[name]
        out = {}
        for row in it:
            c = cells(row)
            out[c[col(f"adm{level}_pcode")]] = (
                c[col(f"adm{level}_name")],
                float(c[col("center_lat")]),
                float(c[col("center_lon")]),
                float(c[col("area_sqkm")]),
            )
        return out


def sparql(query):
    url = ENDPOINT + "?" + urllib.parse.urlencode({"query": query, "format": "json"})
    for attempt in range(5):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
            with urllib.request.urlopen(req, timeout=180) as r:
                body = r.read()
            return body, json.loads(body)["results"]["bindings"]
        except (OSError, ValueError):
            if attempt == 4:
                raise
            time.sleep(10 * (attempt + 1))


def km(a, b, c, d):
    p = math.radians
    h = math.sin(p(c - a) / 2) ** 2 + math.cos(p(a)) * math.cos(p(c)) * math.sin(p(d - b) / 2) ** 2
    return 6371 * 2 * math.asin(math.sqrt(h))


def regency(label, code):
    """Kemendagri numbers a province's cities (kota) from 71."""
    name = re.sub(r"^(Kabupaten|Kota)\s+", "", label)
    return ("Kota " + name, "Kota " + name) if code[2:4] >= "71" else ("Kab. " + name, name)


def queries(v, unique_in_regency):
    labelled_kab, plain_kab = regency(v["kab"], v["code"])
    out = [
        f"{v['kind']} {v['name']}, Kec. {v['kec']}, {labelled_kab}, {v['prov']}, Indonesia",
        f"{v['name']}, {v['kec']}, {plain_kab}, {v['prov']}, Indonesia",
    ]
    if unique_in_regency:
        out.append(f"{v['name']}, {plain_kab}, Indonesia")
    out.append(f"{v['kind']} {v['name']}, Kec. {v['kec']}, {labelled_kab}, {SHORT.get(v['prov'], v['prov'])}")
    return out


def read_villages(items, districts):
    """Villages with one code, point, label and district -> regency -> province chain."""
    villages = []
    for qid, rows in items.items():
        kecs = {r["kec"]["value"] for r in rows}
        chains = set().union(*(districts.get(k, set()) for k in kecs))
        codes = {r["code"]["value"] for r in rows}
        points = {r["point"]["value"] for r in rows}
        labels = {r["label"]["value"] for r in rows}
        if len(chains) != 1 or len(codes) != 1 or len(points) != 1 or len(labels) != 1:
            continue
        (kec, kab, prov), = chains
        r = rows[0]
        m = re.match(r"Point\(([-\d.eE]+) ([-\d.eE]+)\)", r["point"]["value"])
        if not m:
            continue
        types = sorted({r["type"]["value"].rsplit("/", 1)[1] for r in rows})
        villages.append({
            "qid": qid,
            "code": r["code"]["value"],
            "name": r["label"]["value"],
            "kec": kec,
            "kab": kab,
            "prov": prov,
            "lon": float(m.group(1)),
            "lat": float(m.group(2)),
            "kind": next((KINDS[t] for t in types if t in KINDS), "Desa"),
        })
    return villages


def vet(villages, bps):
    """The villages BPS knows by the same code and name, near its centre and unique in their
    district (the code's first 7 digits), each with its tolerance; and the rejections counted."""
    by_district = {}
    for v in villages:
        by_district.setdefault((v["code"][:7], fold(v["name"])), []).append(v["qid"])
    eligible, rejected = [], {"no_bps_village": 0, "bps_name_differs": 0, "far_from_bps": 0, "ambiguous": 0, "odd_label": 0}
    for v in villages:
        b = bps.get("ID" + v["code"])
        if b is None:
            rejected["no_bps_village"] += 1
            continue
        if fold(b[0]) != fold(v["name"]):
            rejected["bps_name_differs"] += 1
            continue
        if re.search(r"[/\\()]", v["name"] + v["kec"] + v["kab"]):
            rejected["odd_label"] += 1
            continue
        if len(by_district[(v["code"][:7], fold(v["name"]))]) > 1:
            rejected["ambiguous"] += 1
            continue
        radius = math.sqrt(b[3] / math.pi)
        if km(v["lat"], v["lon"], b[1], b[2]) > 2 * radius + 1:
            rejected["far_from_bps"] += 1
            continue
        eligible.append(dict(v, radius_km=round(min(25.0, max(2.0, 2 * radius)), 1)))
    return eligible, rejected


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--codab", required=True, help="idn_admin_boundaries.xlsx from HDX's cod-ab-idn")
    ap.add_argument("--count", type=int, default=200)
    ap.add_argument("--seed", default="id-villages-2026-11")
    ap.add_argument("--out", default="tests/gold.id-villages.seeds.jsonl")
    ap.add_argument("--manifest", default="tests/id-villages/manifest.json")
    args = ap.parse_args()

    bps = bps_villages(args.codab)
    provinces = sorted({p[2:4] for p in bps})
    items, districts, hashes = {}, {}, {}
    for prov in provinces:
        for name, query in (("villages", VILLAGES), ("districts", DISTRICTS)):
            body, rows = sparql(query % prov)
            print(prov, name, len(rows), flush=True)
            hashes[f"{prov}-{name}"] = hashlib.sha256(body).hexdigest()
            for r in rows:
                if name == "villages":
                    items.setdefault(r["item"]["value"].rsplit("/", 1)[1], []).append(r)
                else:
                    chain = (r["kecLabel"]["value"], r["kabLabel"]["value"], r["provLabel"]["value"])
                    districts.setdefault(r["kec"]["value"], set()).add(chain)

    villages = read_villages(items, districts)
    eligible, rejected = vet(villages, bps)
    by_regency = {}
    for v in villages:
        by_regency.setdefault((v["code"][:4], fold(v["name"])), []).append(v["qid"])

    key = lambda v: hashlib.sha256(f"{args.seed}:{v['qid']}".encode()).hexdigest()
    chosen = sorted(eligible, key=key)[: args.count]
    with open(args.out, "w") as f:
        for v in sorted(chosen, key=lambda v: v["code"]):
            unique = len(by_regency[(v["code"][:4], fold(v["name"]))]) == 1
            seed = {"kind": "place", "cohort": "village", "qid": v["qid"], "queries": queries(v, unique),
                    "radius_km": v["radius_km"], "tags": ["id-villages"]}
            f.write(json.dumps(seed, ensure_ascii=False) + "\n")

    Path(args.manifest).write_text(json.dumps({
        "seed": args.seed,
        "count": args.count,
        "codab": {"file": Path(args.codab).name, "sha256": hashlib.sha256(Path(args.codab).read_bytes()).hexdigest(), "villages": len(bps)},
        "wikidata": {"items": len(items), "villages": len(villages), "eligible": len(eligible), "rejected": rejected, "selected": len(chosen),
                     "response_sha256": hashes},
    }, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
