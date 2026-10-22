#!/usr/bin/env python3
"""Town-level gold seeds from GLEIF legal addresses for the countries without a registry corpus.

The query is an entity's registered address as GLEIF publishes it; the expected point is the
Wikidata settlement its city names, so the answer is checked against a source independent of
both GLEIF and OpenStreetMap. Only active, issued, fully corroborated records of entities that
are not sole proprietors are read. A city matches when exactly one settlement of the country
carries that name (English label or alias, or one in the records' own languages) and its
Wikidata population is at least --min-population: a nameless or tiny namesake is how a town's
name lands on a hamlet across the country when the town itself is typed otherwise ("urban area
in Sweden"). An address whose lines name another settlement is dropped too: there the city is
the postal town or metro, not the place. Unmatched addresses are counted in the manifest. Addresses are deduplicated and kept only when at
least --min-entities entities share them: an office, not the home of a one-person company. They
are then picked city by city in seeded-hash order, at most --per-city from one settlement and
--count per country.
Expectations come later from `evaluate build-gold`, which reads each settlement from Wikidata."""
import argparse
import collections
import csv
import hashlib
import importlib.util
import io
import json
import math
import re
import time
import unicodedata
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

spec = importlib.util.spec_from_file_location("towns", Path(__file__).parent.parent / "towns" / "sample-towns.py")
towns = importlib.util.module_from_spec(spec)
spec.loader.exec_module(towns)

# The settlements of tests/towns/sample-towns.py, with every name in the given languages.
QUERY = """SELECT ?item ?name ?pop WHERE {
  ?area wdt:P297 "%s" .
  { ?item wdt:P17 ?area } UNION { ?item wdt:P131+ ?area }
  { VALUES ?type { wd:Q515 wd:Q3957 wd:Q532 wd:Q5084 } ?item wdt:P31/wdt:P279* ?type } UNION { ?item wdt:P31 wd:Q486972 }
  ?item wdt:P625 ?point ; wdt:P17/wdt:P297 [] .
  FILTER(isLiteral(?point))
  FILTER NOT EXISTS { ?item wdt:P576 [] }
  { ?item rdfs:label ?name } UNION { ?item skos:altLabel ?name }
  FILTER(LANG(?name) IN (%s))
  OPTIONAL { ?item wdt:P1082 ?pop }
}"""
# Every located item of the country with one of the given exact labels and a population: a town,
# municipality or urban area, not the river or mountain a town is named after.
NAMESAKES = """SELECT ?item ?name ?point WHERE {
  hint:Query hint:optimizer "None" .
  VALUES ?name { %s }
  ?item rdfs:label|skos:altLabel ?name ; wdt:P17/wdt:P297 "%s" ; wdt:P625 ?point ; wdt:P1082 [] .
}"""
# Which (settlement, namesake) pairs are a town inside the area named after it (Salzburg, Limerick).
CONTAINS = """SELECT ?q ?o WHERE {
  hint:Query hint:optimizer "None" .
  VALUES (?q ?o) { %s }
  ?q wdt:P131+ ?o .
}"""
A = "Entity.LegalAddress."
CARE_OF = re.compile(r"^\s*(c/o|c\.o\.|care of|attn\.?|attention)\b", re.I)
PO_BOX = re.compile(r"\b(p\.?\s*o\.?\s*box|post\s*box|postfach|apartado|casilla|caixa postal|b\.?\s*p\.?\s*\d|pmb\s*\d|private bag)", re.I)
MAX_QUERY = 160
KEEP = {"LEI", "Entity.EntityStatus", "Entity.EntityCategory", "Registration.RegistrationStatus", "Registration.ValidationSources"}


# letters NFKD leaves whole, as cities are often typed without them ("Hamrun" for Ħamrun)
UNDECOMPOSED = str.maketrans({"ħ": "h", "ł": "l", "ø": "o", "đ": "d", "ð": "d", "þ": "th", "æ": "ae", "œ": "oe", "ı": "i"})


def fold(text):
    text = unicodedata.normalize("NFKD", text.casefold()).translate(UNDECOMPOSED)
    text = "".join(c for c in text if not unicodedata.combining(c)).replace("'", "").replace("\u2019", "")
    return " ".join(re.sub(r"[^\w]+", " ", text).split())


def address(row):
    """(lines, city, postcode) of a record's legal address, or None when it can't be typed:
    a care-of line is dropped (it names a company), a separate house number is not placed."""
    if row[A + "AddressNumber"].strip() or row[A + "AddressNumberWithinBuilding"].strip():
        return None
    lines = []
    for key in ("FirstAddressLine", "AdditionalAddressLine.1", "AdditionalAddressLine.2", "AdditionalAddressLine.3"):
        line = " ".join(row[A + key].split())
        if CARE_OF.match(line):
            # "C/O Agent Ltd, 9 Some Street": the agent is a name, the rest is the address
            line = line.split(",", 1)[1].strip() if "," in line else ""
        if line and fold(line) not in {fold(l) for l in lines}:
            lines.append(line)
    city = " ".join(row[A + "City"].split())
    if not lines or not city:
        return None
    return lines, city, " ".join(row[A + "PostalCode"].split())


def read(path, countries):
    """country -> {folded query parts: record}, deduplicated; and each country's record languages."""
    found = collections.defaultdict(dict)
    langs = collections.defaultdict(collections.Counter)
    with zipfile.ZipFile(path) as z:
        [name] = [n for n in z.namelist() if n.endswith(".csv")]
        with z.open(name) as raw:
            reader = csv.reader(io.TextIOWrapper(raw, encoding="utf-8", newline=""))
            header = next(reader)
            wanted = [(k, i) for i, k in enumerate(header) if k.startswith(A) or k in KEEP]
            for values in reader:
                row = {k: values[i] for k, i in wanted}
                cc = row[A + "Country"].lower()
                if (cc not in countries or row["Entity.EntityStatus"] != "ACTIVE"
                        or row["Registration.RegistrationStatus"] != "ISSUED"
                        or row["Registration.ValidationSources"] != "FULLY_CORROBORATED"
                        or row["Entity.EntityCategory"] in ("SOLE_PROPRIETOR", "")):
                    continue
                parsed = address(row)
                if not parsed:
                    continue
                lines, city, postcode = parsed
                key = fold(" ".join(lines + [city, postcode]))
                lei = row["LEI"]
                record = found[cc].get(key)
                if not record or lei < record["lei"]:
                    found[cc][key] = {"lei": lei, "lines": lines, "city": city, "postcode": postcode,
                                      "entities": record["entities"] if record else 0}
                found[cc][key]["entities"] += 1
                if row[A + "xmllang"]:
                    langs[cc][row[A + "xmllang"].split("-")[0].lower()] += 1
    return found, langs


def languages(counter):
    """English plus every language of at least a tenth of the country's records."""
    total = sum(counter.values()) or 1
    return sorted({"en"} | {l for l, n in counter.items() if n * 10 >= total and re.fullmatch(r"[a-z]{2,3}", l)})


def sparql(query, path):
    """The query's bindings, cached in path."""
    if not path.exists():
        data = urllib.parse.urlencode({"query": query, "format": "json"}).encode()
        request = urllib.request.Request(towns.ENDPOINT, data=data, headers={"User-Agent": "opengeocoding-gold/1.0 (https://github.com/Mokto/opengeocoding)"})
        with urllib.request.urlopen(request, timeout=180) as response:
            body = response.read()
        time.sleep(1)
        # a query that times out mid-stream still answers 200, with the error after the partial JSON
        json.loads(body, strict=False)
        path.write_bytes(body)
    return json.loads(path.read_bytes(), strict=False)["results"]["bindings"]


def literal(value, lang):
    return json.dumps(value, ensure_ascii=False) + "@" + lang


def far_names(rows, matched, km=25):
    """Folded name -> the items of the country more than km from its settlement that share one of
    its exact labels: a town typed otherwise (Bucharest, "urban area in Sweden") that the
    settlement query missed, or the area named after the town (see CONTAINS)."""
    points = {}
    for row in rows:
        m = re.fullmatch(r"Point\(([-\d.eE]+) ([-\d.eE]+)\)", row["point"]["value"])
        if m:
            points.setdefault(row["item"]["value"].rsplit("/", 1)[-1], set()).add((float(m[2]), float(m[1])))
    by_label = collections.defaultdict(set)
    for row in rows:
        by_label[fold(row["name"]["value"])].add(row["item"]["value"].rsplit("/", 1)[-1])
    far = collections.defaultdict(set)
    for name, qid in matched.items():
        for other in by_label[name] - {qid}:
            if any(distance(a, b) > km for a in points.get(qid, ()) for b in points.get(other, ())):
                far[name].add(other)
    return dict(far)


def distance(a, b):
    (la1, lo1), (la2, lo2) = (map(math.radians, p) for p in (a, b))
    h = math.sin((la2 - la1) / 2) ** 2 + math.cos(la1) * math.cos(la2) * math.sin((lo2 - lo1) / 2) ** 2
    return 12742 * math.asin(math.sqrt(h))


def settlements(rows):
    """folded name -> {qid: population or None}, and folded name -> {(name, language)}."""
    names = collections.defaultdict(dict)
    labels = collections.defaultdict(set)
    for row in rows:
        qid = row["item"]["value"].rsplit("/", 1)[-1]
        try:
            pop = float(row["pop"]["value"]) if "pop" in row else None
        except ValueError:
            pop = None
        name = fold(row["name"]["value"])
        if name:
            old = names[name].get(qid)
            names[name][qid] = max((p for p in (old, pop) if p is not None), default=None)
            labels[name].add((row["name"]["value"], row["name"]["xml:lang"]))
    return names, labels


def select(records, names, country_name, count, per_city, seed, cc="", min_entities=3, min_population=1000):
    """(unmatched city count, seeds) for one country: cities in seeded order, one address each per round."""
    by_city = collections.defaultdict(list)
    unmatched = 0
    for r in records.values():
        if r["entities"] < min_entities:
            continue
        match = names.get(fold(r["city"]), {})
        [(qid, pop)] = match.items() if len(match) == 1 else [(None, None)]
        if (pop or 0) < min_population:
            unmatched += 1
            continue
        # GLEIF's city is often the postal town or metro of a place the lines name (Bellville, Cape Town)
        if any(qid not in names.get(fold(part), {qid: 0}) for line in r["lines"] for part in line.split(",")):
            unmatched += 1
            continue
        query = ", ".join(r["lines"] + [f"{r['city']} {r['postcode']}".strip(), country_name])
        if len(query) <= MAX_QUERY:
            by_city[qid].append((hashlib.sha256(f"{seed}\0{r['lei']}".encode()).hexdigest(), query, r, pop))
    order = sorted(by_city, key=lambda q: hashlib.sha256(f"{seed}\0{q}".encode()).hexdigest())
    for qid in order:
        by_city[qid].sort()
    picked = []
    for round_ in range(per_city):
        for qid in order:
            if len(picked) < count and round_ < len(by_city[qid]):
                picked.append((qid, *by_city[qid][round_][1:]))
    country = {"country": sorted([cc, towns.SOVEREIGN[cc]])} if cc in towns.SOVEREIGN else {}
    seeds = []
    for qid, query, r, pop in picked:
        po_box = any(PO_BOX.search(l) for l in r["lines"])
        seed = {"kind": "place", "cohort": "gleif", "qid": qid, "source": "gleif", "ref": r["lei"], "queries": [query],
                "level": "locality", "radius_km": 20 if (pop or 0) >= 1e6 else 10 if (pop or 0) >= 5e4 else 5,
                "good_match": None, **country, "tags": ["gleif", cc] + (["po_box"] if po_box else [])}
        # "FI-47400" is 47400, but Latvia and Lithuania write their code into the postcode ("LV-1010")
        postcode = r["postcode"] if cc in ("lv", "lt") else re.sub(rf"^{cc}[- ](?=\d)", "", r["postcode"], flags=re.I)
        if postcode and not po_box:
            seed["postcode"] = postcode
        seeds.append(seed)
    return unmatched, seeds


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--gleif", type=Path, required=True, help="GLEIF Golden Copy LEI2 CSV zip")
    parser.add_argument("--countries", nargs="+", help="ISO 3166-1 alpha-2 codes (default: every country without tests/registry/CC.jsonl)")
    parser.add_argument("--reference", type=Path, default=Path("data/reference.json"))
    parser.add_argument("--cache", type=Path, required=True, help="raw SPARQL responses, reused when present")
    parser.add_argument("--count", type=int, default=50)
    parser.add_argument("--per-city", type=int, default=10)
    parser.add_argument("--min-entities", type=int, default=3)
    parser.add_argument("--min-population", type=float, default=1000)
    parser.add_argument("--seed", default="gleif-2026-10")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()
    names = json.loads(args.reference.read_bytes())["country_names"]
    countries = {c.lower() for c in args.countries} if args.countries else {
        cc for cc in names if not Path(f"tests/registry/{cc}.jsonl").exists()}
    found, langs = read(args.gleif, countries)
    args.cache.mkdir(parents=True, exist_ok=True)
    seeds, coverage = [], {}
    for cc in sorted(found):
        shared = sum(r["entities"] >= args.min_entities for r in found[cc].values())
        coverage[cc] = {"addresses": len(found[cc]), "shared": shared}
        if shared:
            lang = languages(langs[cc])
            try:
                path = args.cache / f"{cc}-{'-'.join(lang)}.json"
                cities, labels = settlements(sparql(QUERY % (cc.upper(), ", ".join(f'"{l}"' for l in lang)), path))
                matched = {}
                for r in found[cc].values():
                    match = cities.get(fold(r["city"]), {})
                    if r["entities"] >= args.min_entities and len(match) == 1 and (next(iter(match.values())) or 0) >= args.min_population:
                        matched[fold(r["city"])] = next(iter(match))
                literals = sorted({literal(v, l) for name in matched for v, l in labels[name]})
                rows, paths = [], [path]
                for start in range(0, len(literals), 200):
                    chunk = " ".join(literals[start:start + 200])
                    paths.append(args.cache / f"{cc}-namesakes-{hashlib.sha256(chunk.encode()).hexdigest()[:16]}.json")
                    rows += sparql(NAMESAKES % (chunk, cc.upper()), paths[-1])
                namesakes = far_names(rows, matched)
                pairs = sorted(f"(wd:{matched[name]} wd:{other})" for name, others in namesakes.items() for other in others)
                inside = set()
                for start in range(0, len(pairs), 200):
                    chunk = " ".join(pairs[start:start + 200])
                    paths.append(args.cache / f"{cc}-contains-{hashlib.sha256(chunk.encode()).hexdigest()[:16]}.json")
                    inside |= {(r["q"]["value"].rsplit("/", 1)[-1], r["o"]["value"].rsplit("/", 1)[-1])
                               for r in sparql(CONTAINS % chunk, paths[-1])}
                far = {name for name, others in namesakes.items() if any((matched[name], o) not in inside for o in others)}
            except Exception as e:  # a SPARQL timeout leaves the country out, recorded here
                coverage[cc]["error"] = str(e)
            else:
                unmatched, chosen = select(found[cc], {k: v for k, v in cities.items() if k not in far},
                                           names[cc], args.count, args.per_city, args.seed, cc,
                                           args.min_entities, args.min_population)
                seeds += chosen
                coverage[cc] |= {"unmatched_city": unmatched, "far_namesake": sorted(far), "selected": len(chosen),
                                 "settlements": len({s["qid"] for s in chosen}), "languages": lang,
                                 "response_sha256": [hashlib.sha256(p.read_bytes()).hexdigest() for p in paths]}
        print(cc, coverage[cc], flush=True)
    args.out.write_text("".join(json.dumps(s, ensure_ascii=False) + "\n" for s in seeds))
    args.manifest.write_text(json.dumps({
        "gleif": {"file": args.gleif.name, "sha256": hashlib.sha256(args.gleif.read_bytes()).hexdigest(), "size": args.gleif.stat().st_size},
        "query": QUERY, "namesakes": NAMESAKES, "contains": CONTAINS, "seed": args.seed, "count": args.count, "per_city": args.per_city,
        "min_entities": args.min_entities, "min_population": args.min_population, "coverage": coverage,
    }, ensure_ascii=False, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
