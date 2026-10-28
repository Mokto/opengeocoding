#!/usr/bin/env python3
"""Town-level gold seeds from Wikidata for the countries the address corpora leave out.

Each country gets up to --count human settlements: half the most populous, the rest in seeded-hash
order. A settlement is eligible when it has coordinates, an English label no other settlement of
the country shares and that names no country, is not dissolved, is not in the country only through a
unit far away or in another country's, and is not already a gold seed. Expectations come later from
`evaluate build-gold`, which reads each item from Wikidata again."""
import argparse
import hashlib
import json
import math
import time
import urllib.parse
import urllib.request
from pathlib import Path

# Territories whose towns Wikidata files under their own code for some items and also the
# sovereign's for others: all of them accept both, as the rest of the gold corpus does.
SOVEREIGN = {"ck": "nz", "eh": "ma", "mo": "cn", "nu": "nz", "sx": "nl", "tk": "nz", "vg": "gb"}
ENDPOINT = "https://query.wikidata.org/sparql"
# Cities, towns, villages and hamlets (not administrative units, which also subclass human
# settlement) in the country (P17) or, for territories whose items name the sovereign as country,
# located in it (P131+). The item's own country needs an ISO code, which build-gold expects.
QUERY = """SELECT ?item ?label ?pop WHERE {
  ?area wdt:P297 "%s" .
  { ?item wdt:P17 ?area } UNION { ?item wdt:P131+ ?area }
  { VALUES ?type { wd:Q515 wd:Q3957 wd:Q532 wd:Q5084 } ?item wdt:P31/wdt:P279* ?type } UNION { ?item wdt:P31 wd:Q486972 }
  ?item wdt:P625 ?point ; rdfs:label ?label ; wdt:P17/wdt:P297 [] .
  FILTER(isLiteral(?point))
  FILTER(LANG(?label) = "en")
  FILTER NOT EXISTS { ?item wdt:P576 [] }
  OPTIONAL { ?item wdt:P1082 ?pop }
}"""
# Items in the area only through P131+, with the units they are filed under, their country and the
# area's: Wikidata files some towns under a namesake unit abroad (Champdeniers-Saint-Denis, in
# France, under the arrondissement of Saint-Denis, Réunion).
STRAYS = """SELECT ?area ?item ?point ?parent ?unit ?country ?own WHERE {
  ?area wdt:P297 "%s" .
  { ?item wdt:P131 ?parent . ?parent wdt:P131* ?area .
    FILTER NOT EXISTS { ?item wdt:P17 ?area }
    OPTIONAL { ?item wdt:P625 ?point }
    OPTIONAL { ?parent wdt:P625 ?unit }
    OPTIONAL { ?item wdt:P17 ?country } }
  UNION { ?area wdt:P17 ?own }
}"""
# A town further than this from the unit it is filed under is not in it. The area itself is not
# compared: a scattered territory's one point says nothing of its islands (Wake for Navassa).
STRAY_KM = 5000
# Antarctica's settlements are stations, filed under the countries that run them.
OPERATED = {"aq"}


def fetch(cc, cache, query=QUERY, name="{}.json"):
    path = cache / name.format(cc)
    if not path.exists():
        url = ENDPOINT + "?" + urllib.parse.urlencode({"query": query % cc.upper(), "format": "json"})
        request = urllib.request.Request(url, headers={"User-Agent": "opengeocoding-gold/1.0 (https://github.com/Mokto/opengeocoding)"})
        with urllib.request.urlopen(request, timeout=120) as response:
            path.write_bytes(response.read())
        time.sleep(1)
    return json.loads(path.read_bytes())["results"]["bindings"]


def settlements(rows):
    """item -> (label, population or None)."""
    found = {}
    for row in rows:
        qid = row["item"]["value"].rsplit("/", 1)[-1]
        try:
            pop = float(row["pop"]["value"]) if "pop" in row else None
        except ValueError:
            pop = None
        pops = [p for p in (pop, found[qid][1] if qid in found else None) if p is not None]
        found[qid] = (row["label"]["value"].strip(), max(pops) if pops else None)
    return found


def point(wkt):
    lon, lat = (float(v) for v in wkt.removeprefix("Point(").removesuffix(")").split())
    return lat, lon


def km(a, b):
    (lat1, lon1), (lat2, lon2) = (map(math.radians, p) for p in (a, b))
    h = math.sin((lat2 - lat1) / 2) ** 2 + math.cos(lat1) * math.cos(lat2) * math.sin((lon2 - lon1) / 2) ** 2
    return 12742 * math.asin(math.sqrt(h))


def strays(rows, cc=""):
    """Items the area holds only through a unit far from them, or in another country than its own."""
    qid = lambda row, key: row[key]["value"].rsplit("/", 1)[-1]
    own = {qid(row, key) for row in rows for key in ("area", "own") if key in row}
    items = {}
    for row in rows:
        if "item" not in row:
            continue
        points, units, countries = items.setdefault(qid(row, "item"), (set(), set(), set()))
        if row.get("point", {}).get("value", "").startswith("Point("):
            points.add(point(row["point"]["value"]))
        if row.get("unit", {}).get("value", "").startswith("Point(") and qid(row, "parent") not in own:
            units.add(point(row["unit"]["value"]))
        if "country" in row:
            countries.add(qid(row, "country"))
    return {item for item, (points, units, countries) in items.items()
            if (countries and not countries & own and cc not in OPERATED)
            or (points and units and min(km(p, u) for p in points for u in units) > STRAY_KM)}


def select(found, taken_qids, taken_queries, country_name, count, seed, countries=(), cc=""):
    """Seeds for one country, in selection order."""
    names = {}
    for qid, (label, _) in found.items():
        names.setdefault(label.casefold(), []).append(qid)
    eligible = [(qid, label, pop) for qid, (label, pop) in found.items()
                if len(names[label.casefold()]) == 1 and qid not in taken_qids
                and f"{label}, {country_name}".casefold() not in taken_queries and "," not in label
                and label.casefold() not in countries]
    by_pop = sorted((e for e in eligible if e[2]), key=lambda e: (-e[2], e[0]))[:count // 2]
    picked = {e[0] for e in by_pop}
    rest = sorted((e for e in eligible if e[0] not in picked), key=lambda e: hashlib.sha256(f"{seed}\0{e[0]}".encode()).hexdigest())
    chosen = by_pop + rest[:count - len(by_pop)]
    country = {"country": sorted([cc, SOVEREIGN[cc]])} if cc in SOVEREIGN else {}
    return eligible, [{"kind": "place", "cohort": "town", "qid": qid, "queries": [f"{label}, {country_name}"],
                       "radius_km": 10 if (pop or 0) >= 50000 else 5, **country, "tags": ["wikidata-towns"]} for qid, label, pop in chosen]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--countries", nargs="+", required=True, help="ISO 3166-1 alpha-2 codes")
    parser.add_argument("--reference", type=Path, default=Path("data/reference.json"))
    parser.add_argument("--gold-seeds", type=Path, nargs="+", default=[Path("tests/gold.seeds.jsonl")])
    parser.add_argument("--cache", type=Path, required=True, help="raw SPARQL responses, reused when present")
    parser.add_argument("--count", type=int, default=100)
    parser.add_argument("--seed", default="wikidata-towns-2026-10")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()
    names = json.loads(args.reference.read_bytes())["country_names"]
    countries = {n.casefold() for n in names.values()}
    taken_qids, taken_queries = set(), set()
    for path in args.gold_seeds:
        for line in path.read_text().splitlines():
            seed = json.loads(line)
            taken_qids.add(seed.get("qid"))
            taken_queries.update(q.casefold() for q in seed["queries"])
    args.cache.mkdir(parents=True, exist_ok=True)
    seeds, coverage = [], {}
    for cc in sorted(c.lower() for c in args.countries):
        rows = fetch(cc, args.cache)
        stray = strays(fetch(cc, args.cache, STRAYS, "{}.strays.json"), cc)
        found = {qid: town for qid, town in settlements(rows).items() if qid not in stray}
        eligible, chosen = select(found, taken_qids, taken_queries, names[cc], args.count, args.seed, countries, cc)
        seeds += chosen
        taken_qids.update(s["qid"] for s in chosen)
        coverage[cc] = {"settlements": len(found), "eligible": len(eligible), "selected": len(chosen),
                        "strays": len(stray & set(settlements(rows))),
                        "response_sha256": hashlib.sha256((args.cache / f"{cc}.json").read_bytes()).hexdigest(),
                        "strays_sha256": hashlib.sha256((args.cache / f"{cc}.strays.json").read_bytes()).hexdigest()}
        print(cc, coverage[cc], flush=True)
    args.out.write_text("".join(json.dumps(s, ensure_ascii=False) + "\n" for s in seeds))
    args.manifest.write_text(json.dumps({"query": QUERY, "strays_query": STRAYS, "stray_km": STRAY_KM, "seed": args.seed, "count": args.count, "coverage": coverage}, ensure_ascii=False, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
