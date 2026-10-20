#!/usr/bin/env python3
"""G-NAF points for the panel's Australian addresses whose OSM node is over 150 m from them.

`find` reads only the manifest's source records and the official G-NAF PSV release, never a
service answer: a record is adjudicated when exactly one current principal G-NAF address, without
unit or level, has its complete number, street and postcode, and an address-level default geocode.
`apply` adds each such point to the replayed cases as an alternative expectation. It is a separate
step because the manifest binds build-addresses.py's hash to the capture.
"""
import argparse
import hashlib
import io
import json
import math
from pathlib import Path
import zipfile

RADIUS_KM = 0.15
# street- and locality-level, gap and unknown geocodes do not place one address
NOT_ADDRESS_LEVEL = {"STL", "LOC", "GG", "UNK"}
STATES = ["ACT", "NSW", "NT", "OT", "QLD", "SA", "TAS", "VIC", "WA"]


def km(a, b):
    lat = math.radians((a[0] + b[0]) / 2)
    return math.hypot((a[0] - b[0]) * 111.195, (a[1] - b[1]) * 111.195 * math.cos(lat))


def norm(text):
    return " ".join(text.upper().split())


def apply(lines, adjudications):
    alternatives = {(a["node"], a["version"]): a for a in adjudications["au"]}
    out, seen = [], set()
    for line in lines:
        case = json.loads(line)
        key = (int(case["source"]["ref"]), case["source"]["version"])
        if case["cohort"] == "global_osm_au" and key in alternatives:
            a = alternatives[key]
            seen.add(key)
            case["expect"] = {"any_of": [case["expect"], {**case["expect"], "point": a["point"]}]}
            case["adjudication"] = {**adjudications["source"], "ref": a["pid"], "geocode_type": a["geocode_type"], "distance_km": a["distance_km"], "role": adjudications["role"]}
        out.append(json.dumps(case, ensure_ascii=False) + "\n")
    if set(alternatives) - seen:
        raise ValueError("adjudication names no case")
    return "".join(out)


def main():
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    find = commands.add_parser("find")
    find.add_argument("--manifest", type=Path, required=True)
    find.add_argument("--gnaf", type=Path, required=True, help="the G-NAF PSV release zip")
    find.add_argument("--release", required=True, help='for example "August 2026"')
    find.add_argument("--out", type=Path, required=True)
    applied = commands.add_parser("apply")
    applied.add_argument("--corpus", type=Path, required=True, help="build-addresses.py --replay output")
    applied.add_argument("--adjudications", type=Path, required=True)
    applied.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "apply":
        adjudications = json.loads(args.adjudications.read_bytes())
        args.out.write_text(apply(args.corpus.read_text().splitlines(), adjudications))
        return
    manifest = json.loads(args.manifest.read_bytes())
    records = next(s for s in manifest["snapshots"] if s["country"] == "AU")["records"]
    h = hashlib.sha256()
    with args.gnaf.open("rb") as stream:
        for chunk in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            h.update(chunk)
    archive = zipfile.ZipFile(args.gnaf)
    names = archive.namelist()

    def rows(suffix):
        name = next(n for n in names if n.endswith("/" + suffix))
        with archive.open(name) as raw:
            lines = io.TextIOWrapper(raw, encoding="utf-8")
            header = next(lines).rstrip("\n").split("|")
            for line in lines:
                yield dict(zip(header, line.rstrip("\n").split("|")))

    suffixes = {r["CODE"]: r["NAME"] for r in rows("Authority_Code_STREET_SUFFIX_AUT_psv.psv")}
    wanted = {norm(r["tags"]["addr:street"]) for r in records}
    streets = {}
    for state in STATES:
        for r in rows(f"{state}_STREET_LOCALITY_psv.psv"):
            full = norm(" ".join(x for x in [r["STREET_NAME"], r["STREET_TYPE_CODE"], suffixes.get(r["STREET_SUFFIX_CODE"], "")] if x))
            if full in wanted and not r["DATE_RETIRED"]:
                streets[r["STREET_LOCALITY_PID"]] = full
    by_key = {}
    for state in STATES:
        for r in rows(f"{state}_ADDRESS_DETAIL_psv.psv"):
            if r["STREET_LOCALITY_PID"] not in streets or r["DATE_RETIRED"] or r["ALIAS_PRINCIPAL"] != "P":
                continue
            if any(r[k] for k in ["FLAT_NUMBER", "FLAT_TYPE_CODE", "LEVEL_NUMBER", "LEVEL_TYPE_CODE", "NUMBER_FIRST_PREFIX", "NUMBER_LAST_PREFIX"]) or not r["NUMBER_FIRST"]:
                continue
            number = r["NUMBER_FIRST"] + r["NUMBER_FIRST_SUFFIX"]
            if r["NUMBER_LAST"]:
                number += "-" + r["NUMBER_LAST"] + r["NUMBER_LAST_SUFFIX"]
            key = (number.upper(), streets[r["STREET_LOCALITY_PID"]], r["POSTCODE"])
            by_key.setdefault(key, []).append(r)
    pids = {r["ADDRESS_DETAIL_PID"] for found in by_key.values() for r in found}
    geocodes = {}
    for state in STATES:
        for r in rows(f"{state}_ADDRESS_DEFAULT_GEOCODE_psv.psv"):
            if r["ADDRESS_DETAIL_PID"] in pids and not r["DATE_RETIRED"]:
                geocodes[r["ADDRESS_DETAIL_PID"]] = r
    adjudicated, counts = [], {"records": len(records), "unique_match": 0, "within_radius": 0, "adjudicated": 0}
    for record in records:
        tags = record["tags"]
        found = by_key.get((tags["addr:housenumber"].upper(), norm(tags["addr:street"]), tags.get("addr:postcode", "")), [])
        if len(found) != 1:
            continue
        detail = found[0]
        geocode = geocodes.get(detail["ADDRESS_DETAIL_PID"])
        if geocode is None or geocode["GEOCODE_TYPE_CODE"] in NOT_ADDRESS_LEVEL:
            continue
        counts["unique_match"] += 1
        point = [round(float(geocode["LATITUDE"]), 6), round(float(geocode["LONGITUDE"]), 6)]
        distance = km(record["point"], point)
        if distance <= RADIUS_KM:
            counts["within_radius"] += 1
            continue
        adjudicated.append({"node": record["id"], "version": record["version"], "pid": detail["ADDRESS_DETAIL_PID"], "geocode_type": geocode["GEOCODE_TYPE_CODE"], "point": point, "distance_km": round(distance, 3)})
    counts["adjudicated"] = len(adjudicated)
    out = {
        "role": "authoritative-alternative-point",
        "rule": "Exactly one current principal G-NAF address without unit or level has the source record's complete number, street and postcode, with an address-level default geocode over 0.15 km from the OSM node; its point is accepted as an alternative.",
        "source": {"name": "G-NAF", "release": args.release, "license": "Open G-NAF EULA (CC BY 4.0 with a mailing restriction)", "url": "https://data.gov.au/data/dataset/geocoded-national-address-file-g-naf", "zip": args.gnaf.name, "zip_sha256": h.hexdigest(), "datum": "GDA2020"},
        "counts": counts,
        "au": adjudicated,
    }
    args.out.write_text(json.dumps(out, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(counts))


if __name__ == "__main__":
    main()
