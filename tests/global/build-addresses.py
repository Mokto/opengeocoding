#!/usr/bin/env python3
import argparse
import concurrent.futures
import hashlib
import heapq
import json
import math
import re
from pathlib import Path
import subprocess
import time
import unicodedata
import xml.etree.ElementTree as ET

COUNTRIES = dict(zip(
    "US CA MX BR AR CL CO GB IE FR DE NL BE CH AT ES PT IT DK SE NO FI PL CZ GR HU RO RU TR ZA EG NG SA AE IL IN CN JP KR TW SG MY TH VN ID PH AU NZ".split(),
    "us canada mexico brazil argentina chile colombia united-kingdom ireland-and-northern-ireland france germany netherlands belgium switzerland austria spain portugal italy denmark sweden norway finland poland czech-republic greece hungary romania russia turkey south-africa egypt nigeria gcc-states gcc-states israel-and-palestine india china japan south-korea taiwan malaysia-singapore-brunei malaysia-singapore-brunei thailand vietnam indonesia philippines australia new-zealand".split(),
))
# Added in October 2026: the countries whose OSM extracts gave the full quota of 50 groups.
COUNTRIES.update(zip(
    "AL AM AO AZ BA BB BD BG BO BT BY BZ CD CI CM CR CU CY DO DZ EC EE ET GA GE GH GT HR HT IQ IR IS JM JO KE KG KH KZ LA LK LT LU LV MA MD ME MG MK MM MN MT NA NC NI NP PE PK PR PS PY QA RS SI SK SL SR SV SY SZ TJ TM TN TT TZ UA UG UY UZ VE XK ZM ZW".split(),
    "albania armenia angola azerbaijan bosnia-herzegovina central-america bangladesh bulgaria bolivia bhutan belarus belize congo-democratic-republic ivory-coast cameroon costa-rica cuba cyprus haiti-and-domrep algeria ecuador estonia ethiopia gabon georgia ghana guatemala croatia haiti-and-domrep iraq iran iceland jamaica jordan kenya kyrgyzstan cambodia kazakhstan laos sri-lanka lithuania luxembourg latvia morocco moldova montenegro madagascar macedonia myanmar mongolia malta namibia new-caledonia nicaragua nepal peru pakistan puerto-rico israel-and-palestine paraguay gcc-states serbia slovenia slovakia sierra-leone suriname el-salvador syria swaziland tajikistan turkmenistan tunisia central-america tanzania ukraine uganda uruguay uzbekistan venezuela kosovo zambia zimbabwe".split(),
))
# Added later in October 2026 with --loose: the remaining countries that fill the quota once
# coasts are kept and addresses mapped on buildings count.
COUNTRIES.update(zip(
    "AD AF AW AX BF BH BJ BM BN BW CG CV CW FJ FK FO GL GN GU GY HK HN IM JE KW KY LB LY MH ML MU MW MZ OM PA PF RW SD SN TC VI YE".split(),
    "andorra afghanistan central-america finland burkina-faso gcc-states benin bermuda malaysia-singapore-brunei botswana congo-brazzaville cape-verde central-america fiji falklands faroe-islands greenland guinea american-oceania guyana china honduras isle-of-man guernsey-jersey gcc-states central-america lebanon libya marshall-islands mali mauritius malawi mozambique gcc-states panama polynesie-francaise rwanda sudan senegal-and-gambia central-america us-virgin-islands yemen".split(),
))
# Territories with their own ISO code also answer as their sovereign country, as in tests/gold.jsonl.
SOVEREIGN = {"pr": "us", "nc": "fr", "aw": "nl", "ax": "fi", "bm": "gb", "cw": "nl", "fk": "gb", "gu": "us", "hk": "cn", "ky": "gb", "pf": "fr", "tc": "gb", "vi": "us"}
BOUNDARY_URL = "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/ca96624a56bd078437bca8184e78163e5039ad19/geojson/ne_10m_admin_0_countries.geojson"
BOUNDARY_SHA = "239eec57ac17f100a11e2536cffc56752c318b50ae765b0918ff7aab4ce8f255"
FIELDS = ["addr:housenumber", "addr:street", "addr:place", "addr:city", "addr:town", "addr:suburb", "addr:district", "addr:state", "addr:province", "addr:postcode", "addr:country"]


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()


def immutable(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        if path.read_bytes() != data:
            raise ValueError(f"refusing to overwrite different output: {path}")
        return
    with path.open("xb") as stream:
        stream.write(data)


def normalize(text):
    return " ".join(unicodedata.normalize("NFKC", text).casefold().split())


def holdout(location):
    return int(sha(location.encode()), 16) % 5 == 0


def record(attributes, tags, country, country_name, iso3):
    tags = {k: v.strip() for k, v in tags.items() if k in FIELDS and v.strip()}
    number = tags.get("addr:housenumber", "")
    street = tags.get("addr:street") or tags.get("addr:place", "")
    city = tags.get("addr:city") or tags.get("addr:town", "")
    postcode = tags.get("addr:postcode", "")
    if not number or not any(c.isdecimal() for c in number) or len(number) > 20 or not street or not (city or postcode):
        return None
    if any(c in number for c in "+@;,") or re.search(r"\d{8,}", number) or normalize(street) in {"-", ".", "unknown", "none", "n/a"}:
        return None
    if re.fullmatch(r"\d{2,4}[ .]\d{2,4}[ .]\d{2,4}", number) or re.fullmatch(r"\d{10,}", postcode):
        return None
    for key, value in tags.items():
        if re.search(r"(?i)(?:https?://|www\.|\S+@\S+|\b(?:phone|telephone|tel|mobile|whatsapp)\s*[:+])", value):
            return None
        if key != "addr:postcode" and re.search(r"\d{8,}", value):
            return None
        if re.search(r"(?i)\b[23456789CFGHJMPQRVWX]{4,8}\+[23456789CFGHJMPQRVWX]{2,3}\b", value):
            return None
    if tags.get("addr:country") and normalize(tags["addr:country"]) not in {country.lower(), iso3.lower(), normalize(country_name)}:
        return None
    if any(len(v) > 200 or any(unicodedata.category(c).startswith("C") for c in v) for v in tags.values()):
        return None
    try:
        lat, lon = float(attributes["lat"]), float(attributes["lon"])
        node_id, version = int(attributes["id"]), int(attributes["version"])
    except (ValueError, KeyError):
        return None
    kind = attributes.get("type", "node")
    if kind not in {"node", "way"}:
        return None
    if not all(map(math.isfinite, [lat, lon])) or not -90 <= lat <= 90 or not -180 <= lon <= 180 or (lat == 0 and lon == 0) or node_id <= 0 or version <= 0:
        return None
    geography = []
    for key in ["addr:suburb", "addr:district", "addr:city", "addr:town", "addr:state", "addr:province", "addr:postcode"]:
        v = tags.get(key)
        if v and normalize(v) not in {normalize(x) for x in geography}:
            geography.append(v)
    geography.append(country_name)
    queries = []
    queries.extend([number + " " + street + ", " + ", ".join(geography), street + " " + number + ", " + ", ".join(geography)])
    queries = list(dict.fromkeys(q for q in queries if len(q) <= 200))
    if not queries:
        return None
    address_key = "\0".join(normalize(x) for x in [country, number, street, city, postcode])
    return {**({"type": kind} if kind != "node" else {}), "id": node_id, "version": version, "timestamp": attributes.get("timestamp", ""), "country": country.lower(), "point": [round(lat, 6), round(lon, 6)], "tags": tags, "queries": queries, "location": "osm-address:" + sha(address_key.encode())}


class Draw:
    def __init__(self, seed, count):
        self.seed, self.count = seed, count
        self.cells = {}
        self.seen_cells = set()
        self.eligible = 0

    def add(self, r):
        self.eligible += 1
        cell = tuple(math.floor(p * 4) for p in r["point"])
        known = cell in self.seen_cells
        self.seen_cells.add(cell)
        if known and cell not in self.cells:
            return
        rank = int(sha((self.seed + "\0" + str(cell)).encode()), 16)
        if cell not in self.cells:
            if len(self.cells) >= 100:
                worst = max(self.cells, key=lambda c: self.cells[c][0])
                if rank >= self.cells[worst][0]:
                    return
                del self.cells[worst]
            self.cells[cell] = (rank, [])
        bucket = self.cells[cell][1]
        key = int(sha((self.seed + "\0" + r.get("type", "")[:1] + str(r["id"])).encode()), 16)
        entry = (-key, r["id"], r)
        if len(bucket) < self.count:
            heapq.heappush(bucket, entry)
        elif entry[:2] > bucket[0][:2]:
            heapq.heapreplace(bucket, entry)

    def selected(self):
        buckets = [sorted(v[1], key=lambda x: (-x[0], x[1])) for _, v in sorted(self.cells.items(), key=lambda x: x[1][0])]
        selected, locations = [], set()
        for at in range(self.count):
            for bucket in buckets:
                if at < len(bucket):
                    r = bucket[at][2]
                    if r["location"] not in locations:
                        locations.add(r["location"])
                        selected.append(r)
                        if len(selected) == self.count:
                            return selected
        return selected


def cases(records, proof):
    out = []
    for r in records:
        number = r["tags"]["addr:housenumber"]
        components = {"house_number": number}
        if r["tags"].get("addr:postcode"):
            components["postcode"] = r["tags"]["addr:postcode"]
        kind = r.get("type", "node")
        ref = ("w" if kind == "way" else "") + str(r["id"])
        for n, query in enumerate(r["queries"]):
            out.append({"id": f"global-osm-{r['country']}-{ref}-{n}", "query": query, "cohort": "global_osm_" + r["country"], "location": r["location"], "holdout": holdout(r["location"]), "must_pass": False,
                        "expect": {"country": sorted({r["country"], SOVEREIGN.get(r["country"], r["country"])}), "level": "house", "max_level": "house", "point": r["point"], "radius_km": 0.15, "region": [], "good_match": True, "components": components},
                        "source": {"name": f"openstreetmap-address-{kind}", "license": "ODbL-1.0", "url": f"https://www.openstreetmap.org/api/0.6/{kind}/{r['id']}/{r['version']}", "ref": str(r["id"]), "version": r["version"], "timestamp": r["timestamp"], "snapshot_sha256": proof["source_sha256"], "evaluation_role": "source-consistency-not-independent-accuracy"},
                        "review": {"status": "source-record-verified", "note": "Mapped address-node coordinate and complete number, not surveyed entrance accuracy. No human positional review." if kind == "node" else "Centre of a mapped building or way and its complete number, not surveyed entrance accuracy. No human positional review."}})
    return out


def country_features(data):
    features = {}
    for feature in data["features"]:
        cc = feature["properties"]["ISO_A2_EH"]
        geometry = feature["geometry"]
        if geometry["type"] not in {"Polygon", "MultiPolygon"}:
            raise ValueError("country mask is not polygonal")
        polygons = [geometry["coordinates"]] if geometry["type"] == "Polygon" else geometry["coordinates"]
        target = features.setdefault(cc, {"type": "Feature", "properties": {"ISO_A2_EH": cc}, "geometry": {"type": "MultiPolygon", "coordinates": []}})
        target["geometry"]["coordinates"].extend(polygons)
    return features


def interior(geometry, neighbours, loose):
    """The country minus a 0.05-degree margin: from its whole outline, or with `loose` only from
    other countries' land, so coasts and islands keep their addresses."""
    import shapely

    if not loose:
        return geometry.buffer(-0.05)
    if neighbours:
        geometry = geometry.difference(shapely.unary_union(neighbours).buffer(0.05))
    return shapely.MultiPolygon([g for g in shapely.get_parts(geometry) if g.geom_type == "Polygon"])


def capture(args, country, feature, country_info, urls, neighbours=()):
    import shapely
    from shapely.geometry import mapping, shape

    mask = interior(shape(feature["geometry"]), [shape(n) for n in neighbours], args.loose)
    feature = {**feature, "geometry": mapping(mask)}
    if not feature["geometry"]["coordinates"]:
        raise ValueError("no country interior after border exclusion")
    name, iso3 = country_info[country]
    source = next((d / (COUNTRIES[country] + "-latest.osm.pbf") for d in args.osm if (d / (COUNTRIES[country] + "-latest.osm.pbf")).is_file()), None)
    if source is None:
        raise ValueError(f"no PBF snapshot for {country}: {COUNTRIES[country]}")
    source_url = urls.get(source.name)
    if source_url is None:
        raise ValueError("PBF has no recorded public source URL")
    proof = {"source_url": source_url, "source_sha256": file_sha(source), "source_bytes": source.stat().st_size, "boundary_url": BOUNDARY_URL, "boundary_sha256": BOUNDARY_SHA, "country_mask_sha256": sha(encoded(feature)), "border_exclusion_degrees": 0.05, **({"border_exclusion_of": "other-countries", "buildings": True} if args.loose else {}), "shapely_version": shapely.__version__, "geos_version": shapely.geos_version_string, "country_info_sha256": file_sha(args.country_info), "seed": args.seed, "count": args.count, "script_sha256": file_sha(Path(__file__))}
    path = args.work / (country + ".snapshot.json")
    if path.exists():
        saved = json.loads(path.read_bytes())
        if saved["proof"] != proof:
            raise ValueError(f"checkpoint inputs changed: {path}")
        return saved
    header = json.loads(subprocess.check_output(["osmium", "fileinfo", "-j", str(source)]))["header"]
    directory = args.work / country
    directory.mkdir(parents=True, exist_ok=True)
    polygon = directory / "country.geojson"
    immutable(polygon, encoded(feature))
    nodes = directory / "nodes.osm.pbf"
    if nodes.exists():
        nodes.unlink()
    print(country, "filtering", source.name, flush=True)
    if args.loose:
        return scan_buildings(args, country, source, nodes, mask, proof, header, path, name, iso3)
    subprocess.run(["osmium", "tags-filter", str(source), "n/addr:housenumber", "--omit-referenced", "-o", str(nodes)], check=True)
    process = subprocess.Popen(["osmium", "extract", "-s", "simple", "-p", str(polygon), str(nodes), "-f", "osm"], stdout=subprocess.PIPE)
    draw, scanned = Draw(args.seed + "\0" + country, args.count), 0
    try:
        for _, element in ET.iterparse(process.stdout, events=("end",)):
            if element.tag != "node":
                continue
            scanned += 1
            tags = {x.get("k"): x.get("v", "") for x in element if x.tag == "tag"}
            r = record(element.attrib, tags, country, name, iso3)
            if r is not None:
                draw.add(r)
            element.clear()
            if scanned % 100000 == 0:
                print(country, "scanned", scanned, "eligible", draw.eligible, flush=True)
        if process.wait() != 0:
            raise ValueError("osmium country extraction failed")
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
        nodes.unlink(missing_ok=True)
    if file_sha(source) != proof["source_sha256"]:
        raise ValueError("PBF changed during sampling")
    selected = draw.selected()
    saved = {"country": country, "country_name": name, "proof": proof, "header": header, "scanned": scanned, "eligible": draw.eligible, "eligible_cells": len(draw.seen_cells), "records": selected}
    immutable(path, encoded(saved))
    print(country, "selected", len(selected), "of", draw.eligible, "eligible nodes", flush=True)
    return saved


def scan_buildings(args, country, source, filtered, mask, proof, header, path, name, iso3):
    """Address nodes and ways (buildings, at their centre) whose point lies in `mask`."""
    import shapely
    from shapely.geometry import shape

    shapely.prepare(mask)
    subprocess.run(["osmium", "tags-filter", str(source), "nw/addr:housenumber", "-o", str(filtered)], check=True)
    process = subprocess.Popen(["osmium", "export", "-f", "geojsonseq", "-x", "print_record_separator=false", "-a", "type,id,version,timestamp", "--geometry-types=point,polygon", str(filtered)], stdout=subprocess.PIPE, text=True)
    draw, scanned, seen = Draw(args.seed + "\0" + country, args.count), 0, set()
    try:
        for line in process.stdout:
            if not line.strip():
                continue
            feature = json.loads(line)
            p = feature["properties"]
            if "addr:housenumber" not in p or (p.get("@type"), p.get("@id")) in seen:
                continue
            seen.add((p.get("@type"), p.get("@id")))
            point = shape(feature["geometry"]).centroid
            if point.is_empty or not mask.contains(point):
                continue
            scanned += 1
            attributes = {"type": p.get("@type"), "id": str(p.get("@id")), "version": str(p.get("@version")), "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(int(p["@timestamp"]))) if "@timestamp" in p else "", "lat": repr(point.y), "lon": repr(point.x)}
            r = record(attributes, {k: v for k, v in p.items() if not k.startswith("@")}, country, name, iso3)
            if r is not None:
                draw.add(r)
        if process.wait() != 0:
            raise ValueError("osmium export failed")
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
        filtered.unlink(missing_ok=True)
    if file_sha(source) != proof["source_sha256"]:
        raise ValueError("PBF changed during sampling")
    selected = draw.selected()
    saved = {"country": country, "country_name": name, "proof": proof, "header": header, "scanned": scanned, "eligible": draw.eligible, "eligible_cells": len(draw.seen_cells), "records": selected}
    immutable(path, encoded(saved))
    print(country, "selected", len(selected), "of", draw.eligible, "eligible addresses", flush=True)
    return saved


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--osm", type=Path, nargs="+")
    parser.add_argument("--boundaries", type=Path)
    parser.add_argument("--country-info", type=Path)
    parser.add_argument("--work", type=Path)
    parser.add_argument("--replay", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--countries", nargs="+", choices=list(COUNTRIES), default=list(COUNTRIES))
    parser.add_argument("--count", type=int, default=50)
    parser.add_argument("--seed", default="global-addresses-2026-10")
    parser.add_argument("--loose", action="store_true", help="keep coasts (exclude only near other countries' land) and sample addresses mapped on buildings too")
    parser.add_argument("--base", type=Path, help="earlier manifest: its countries are kept as captured and only the others are sampled")
    args = parser.parse_args()
    if args.replay:
        manifest = json.loads(args.replay.read_bytes())
        corpus = [c for s in manifest["snapshots"] for c in cases(s["records"], s["proof"])]
        immutable(args.out / "global-addresses.jsonl", "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in corpus).encode())
        immutable(args.out / "manifest.json", encoded(manifest))
        return
    if not all([args.osm, args.boundaries, args.country_info, args.work]):
        parser.error("capture requires --osm, --boundaries, --country-info and --work")
    if not 1 <= args.count <= 100:
        parser.error("count must be between 1 and 100")
    base = json.loads(args.base.read_bytes()) if args.base else {"coverage": {}, "errors": {}, "snapshots": []}
    if args.base and (base["seed"] != args.seed or base["errors"] or any(c["target_locations"] != args.count for c in base["coverage"].values())):
        parser.error("base manifest has a different seed or quota, or errors")
    if file_sha(args.boundaries) != BOUNDARY_SHA:
        parser.error("boundary snapshot checksum differs")
    features = country_features(json.loads(args.boundaries.read_bytes()))
    countries = {}
    for line in args.country_info.read_text().splitlines():
        if line and not line.startswith("#"):
            row = line.split("\t")
            countries[row[0]] = (row[4], row[1])
    urls = {}
    for directory in args.osm:
        for line in (directory / "urls.txt").read_text().splitlines():
            if line.startswith("https://download.geofabrik.de/") and line.endswith(".osm.pbf"):
                urls[line.rsplit("/", 1)[-1]] = line
    args.work.mkdir(parents=True, exist_ok=True)
    neighbours = {}
    if args.loose:
        import shapely
        from shapely.geometry import mapping, shape

        shapes = {cc: shape(f["geometry"]) for cc, f in features.items()}
        for country in args.countries:
            x0, y0, x1, y1 = shapes[country].bounds
            box = shapely.box(x0 - 0.1, y0 - 0.1, x1 + 0.1, y1 + 0.1)
            neighbours[country] = [mapping(shapely.clip_by_rect(g, *box.bounds)) for cc, g in shapes.items() if cc != country and g.intersects(box)]
    snapshots, errors = [], {}
    with concurrent.futures.ProcessPoolExecutor(max_workers=4) as pool:
        jobs = {pool.submit(capture, args, country, features[country], countries, urls, neighbours.get(country, ())): country for country in args.countries if country not in base["coverage"]}
        for job in concurrent.futures.as_completed(jobs):
            country = jobs[job]
            try:
                snapshots.append(job.result())
            except Exception as error:
                errors[country] = f"{type(error).__name__}: {error}"
                print(country, "FAILED", errors[country], flush=True)
    snapshots.sort(key=lambda s: args.countries.index(s["country"]))
    snapshots = base["snapshots"] + snapshots
    coverage = {}
    corpus = []
    for snapshot in snapshots:
        country = snapshot["country"]
        rows = cases(snapshot["records"], snapshot["proof"])
        corpus.extend(rows)
        coverage[country] = {"target_locations": args.count, "locations": len(snapshot["records"]), "queries": len(rows), "held_out_locations": sum(holdout(r["location"]) for r in snapshot["records"]), "eligible": snapshot["eligible"], "eligible_cells": snapshot["eligible_cells"], "status": "quota-met" if len(snapshot["records"]) == args.count else "shortfall"}
    manifest = {"evaluation_role": "source-consistency-not-independent-accuracy", "seed": args.seed, "coverage": coverage, "errors": errors, "snapshots": snapshots}
    immutable(args.out / "global-addresses.jsonl", "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in corpus).encode())
    immutable(args.out / "manifest.json", encoded(manifest))
    print("TOTAL", len(corpus), "queries;", sum(v["locations"] for v in coverage.values()), "locations;", len(coverage), "countries;", len(errors), "errors", flush=True)
    if errors:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
