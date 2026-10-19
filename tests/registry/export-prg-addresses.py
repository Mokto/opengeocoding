#!/usr/bin/env python3
"""Prepare Polish PRG address points from an offline national GML ZIP."""

import argparse
from collections import Counter
from datetime import date
import csv
import hashlib
import heapq
import io
import json
import math
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET
import zipfile

URL = "https://opendata.geoportal.gov.pl/prg/adresy/PRG-punkty_adresowe.zip"
NS = {"p": "https://geoportal.gov.pl/schemas/prgad/1.0", "g": "http://www.opengis.net/gml/3.2"}
XLINK = "{http://www.w3.org/1999/xlink}href"
GML_ID = "{http://www.opengis.net/gml/3.2}id"
PROVINCES = {f"{i:02}" for i in range(2, 33, 2)}
GUID = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")
NUMBER = re.compile(r"[1-9][0-9]*(?:[ A-Za-z0-9/-]*)")
POSTCODE = re.compile(r"[0-9]{2}-[0-9]{3}")


def encode(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def text(element, path):
    value = element.findtext(path, default="", namespaces=NS).strip()
    if any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise ValueError(f"control character in {path}")
    return value


def link(element, path):
    item = element.find(path, NS)
    if item is None:
        return ""
    href = item.get(XLINK, "")
    if not href.startswith("#") or len(href) < 2:
        raise ValueError(f"non-local or missing {path} reference")
    return href[1:]


def identity(element):
    guid = text(element, "p:idIIP/p:AD_IdentyfikatorIIP/p:lokalnyId")
    namespace = text(element, "p:idIIP/p:AD_IdentyfikatorIIP/p:przestrzenNazw")
    if not GUID.fullmatch(guid) or namespace != "PL.PZGIK.200":
        raise ValueError("unexpected PRG identifier namespace or UUID")
    return {"id": guid, "gml_id": element.attrib[GML_ID], "namespace": namespace,
            "version": text(element, "p:idIIP/p:AD_IdentyfikatorIIP/p:wersjaId"),
            "valid_from": text(element, "p:poczatekWersjiObiektu"),
            "valid_to": text(element, "p:koniecWersjiObiektu")}


def projected_point(element):
    point = element.find("p:georeferencja/g:Point", NS)
    if point is None or point.get("srsName") != "urn:ogc:def:crs:EPSG::2180" or point.get("srsDimension") != "2":
        raise ValueError("PRG requires an EPSG:2180 two-dimensional address point")
    values = text(point, "g:pos").split()
    if len(values) != 2:
        raise ValueError("PRG point coordinate count")
    north, east = map(float, values)
    if not all(math.isfinite(v) for v in (north, east)):
        raise ValueError("non-finite PRG point")
    return {"north": north, "east": east, "gml_pos": values}


def prepare_member(stream, province, snapshot, seed, size):
    if size <= 0:
        raise ValueError("candidates per province must be positive")
    counts = Counter()
    localities, streets, points = {}, {}, []
    seen_gml, seen_points = set(), set()
    iterator = ET.iterparse(stream, events=("start", "end"))
    _, root = next(iterator)
    if root.tag != f"{{{NS['g']}}}FeatureCollection":
        raise ValueError("PRG requires a GML FeatureCollection")
    for event, element in iterator:
        if event != "end" or element.tag != f"{{{NS['g']}}}featureMember":
            continue
        if len(element) != 1:
            raise ValueError("PRG featureMember cardinality")
        feature = element[0]
        kind = feature.tag.removeprefix(f"{{{NS['p']}}}")
        if kind not in {"AD_Miejscowosc", "AD_UlicaPlac", "AD_PunktAdresowy", "AD_Uchwala"}:
            raise ValueError(f"unknown PRG feature: {kind}")
        counts[kind] += 1
        if counts[kind] % 100000 == 0:
            print(f"PRG {province}: {kind} {counts[kind]}", flush=True)
        gml_id = feature.attrib[GML_ID]
        if gml_id in seen_gml:
            raise ValueError(f"duplicate PRG feature: {gml_id}")
        seen_gml.add(gml_id)
        if kind != "AD_Uchwala":
            row = identity(feature)
            if not row["valid_from"] or row["valid_from"][:10] > snapshot or row["valid_to"]:
                counts["closed_or_future_features"] += 1
            elif kind == "AD_Miejscowosc":
                row.update(name=text(feature, "p:nazwa"), municipality=text(feature, "p:TERYTGminy"), simc=text(feature, "p:identyfikatorSIMC"), kind=text(feature, "p:rodzaj"))
                if not row["name"] or not re.fullmatch(province + r"[0-9]{5}", row["municipality"]):
                    counts["ineligible_locality_attributes"] += 1
                else:
                    localities[gml_id] = row
            elif kind == "AD_UlicaPlac":
                places = feature.findall("p:miejsce", NS)
                if any(not item.get(XLINK, "").startswith("#") or len(item.get(XLINK, "")) < 2 for item in places):
                    raise ValueError("PRG street has a non-local place reference")
                row.update(name=text(feature, "p:nazwaPelna"), kind=text(feature, "p:rodzaj"), ulic=text(feature, "p:identyfikatorULIC"),
                           teryt_name1=text(feature, "p:TERYTNazwa1"), teryt_name2=text(feature, "p:TERYTNazwa2"),
                           places=[item.get(XLINK)[1:] for item in places])
                if not row["name"] or not row["places"]:
                    counts["ineligible_street_attributes"] += 1
                else:
                    streets[gml_id] = row
            else:
                if row["id"] in seen_points:
                    raise ValueError("duplicate PRG address UUID")
                seen_points.add(row["id"])
                row.update(number=text(feature, "p:numerPorzadkowy"), postcode=text(feature, "p:kodPocztowy"), granted=text(feature, "p:dataNadania"),
                           locality_ref=link(feature, "p:miejscowosc"), street_ref=link(feature, "p:ulica2"))
                if not NUMBER.fullmatch(row["number"]) or not POSTCODE.fullmatch(row["postcode"]) or row["granted"] > snapshot or not row["locality_ref"]:
                    counts["ineligible_point_attributes"] += 1
                else:
                    row.update(projected_point(feature))
                    if not (100000 <= row["north"] <= 900000 and 100000 <= row["east"] <= 900000):
                        counts["points_outside_projected_frame"] += 1
                    else:
                        counts["eligible_point_frame"] += 1
                        rank = int.from_bytes(hashlib.sha256(f"{seed}\0{row['id']}".encode()).digest(), "big")
                        item = (-rank, row["id"], row)
                        if len(points) < size:
                            heapq.heappush(points, item)
                        elif item[:2] > points[0][:2]:
                            heapq.heapreplace(points, item)
        root.clear()
    addresses = []
    for _, _, row in sorted(points, key=lambda item: item[1]):
        locality = localities.get(row["locality_ref"])
        street = streets.get(row["street_ref"]) if row["street_ref"] else None
        if locality is None or (row["street_ref"] and (street is None or row["locality_ref"] not in street["places"])):
            counts["sampled_points_with_unresolved_or_conflicting_reference"] += 1
            continue
        row.update(province=province, locality=locality, street=street)
        addresses.append(row)
    counts["sampled_points"] = len(points)
    counts["candidate_points"] = len(addresses)
    return {"counts": dict(sorted(counts.items())), "addresses": addresses}


def project(addresses):
    with tempfile.TemporaryDirectory(prefix="prg-projection-") as directory:
        path = Path(directory) / "points.csv"
        with path.open("w", newline="") as stream:
            writer = csv.writer(stream)
            writer.writerow(["id", "east", "north"])
            writer.writerows((row["id"], row["east"], row["north"]) for row in addresses)
        escaped = str(path).replace("'", "''")
        sql = f"""SET autoinstall_known_extensions=false; SET autoload_known_extensions=false; LOAD spatial;
SELECT id, ST_Y(point) AS lat, ST_X(point) AS lon FROM (
SELECT id, ST_Transform(ST_Point(east, north), 'EPSG:2180', 'EPSG:4326', always_xy := true) AS point
FROM read_csv('{escaped}', columns={{'id':'VARCHAR','east':'DOUBLE','north':'DOUBLE'}}, header=true)) ORDER BY id;"""
        result = subprocess.run(["duckdb", "-csv", "-c", sql], check=True, capture_output=True, text=True)
    points = {}
    for row in csv.DictReader(io.StringIO(result.stdout)):
        if row["id"] in points:
            raise ValueError("duplicate projected point")
        points[row["id"]] = [float(row["lat"]), float(row["lon"])]
    if set(points) != {row["id"] for row in addresses}:
        raise ValueError("projection lost address IDs")
    return points


def capture(source, out, seed, size):
    receipt = json.loads(source.with_suffix(".zip.receipt.json").read_text())
    digest = hashlib.sha256()
    with source.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    if receipt["url"] != URL or source.stat().st_size != receipt["bytes"] or digest.hexdigest() != receipt["sha256"]:
        raise ValueError("PRG archive receipt mismatch")
    duckdb_version = subprocess.run(["duckdb", "--version"], check=True, capture_output=True, text=True).stdout.strip()
    signature = {"archive": digest.hexdigest(), "script": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "duckdb": duckdb_version, "seed": seed, "size": size}
    work = out.with_suffix(".work")
    work.mkdir(parents=True, exist_ok=True)
    captured = []
    members = []
    with zipfile.ZipFile(source) as archive:
        seen = set()
        for name in sorted(archive.namelist()):
            match = re.fullmatch(r"([0-9]{2})_[a-z-]+_([0-9]{2}\.[0-9]{2}\.[0-9]{4})_[0-9]{2}\.[0-9]{2}\.[0-9]{2}\.gml", name)
            if not match or match[1] not in PROVINCES or match[1] in seen:
                raise ValueError(f"unexpected or duplicate PRG province member: {name}")
            province = match[1]
            seen.add(province)
            day, month, year = map(int, match[2].split("."))
            snapshot = date(year, month, day).isoformat()
            checkpoint = work / f"{province}.json"
            binding = dict(signature, member=name)
            if checkpoint.exists():
                saved = json.loads(checkpoint.read_bytes())
                if saved["input"] != binding:
                    raise ValueError(f"checkpoint changed; use a fresh output directory: {checkpoint}")
                result = saved["data"]
                if saved["sha256"] != hashlib.sha256(encode(result)).hexdigest():
                    raise ValueError(f"checkpoint checksum mismatch: {checkpoint}")
            else:
                with archive.open(name) as stream:
                    result = prepare_member(stream, province, snapshot, seed, size)
                temporary = checkpoint.with_suffix(".tmp")
                temporary.write_bytes(encode({"input": binding, "data": result, "sha256": hashlib.sha256(encode(result)).hexdigest()}))
                temporary.replace(checkpoint)
            captured.extend(result["addresses"])
            members.append({"file": name, "snapshot_day": snapshot, "counts": result["counts"]})
            print(f"PRG {province}: {result['counts']}", flush=True)
        if seen != PROVINCES:
            raise ValueError("incomplete PRG province frame")
    if not captured or len({row["id"] for row in captured}) != len(captured):
        raise ValueError("empty or duplicate PRG candidate IDs")
    points = project(captured)
    for row in captured:
        row["point"] = points[row["id"]]
        if not (48 <= row["point"][0] <= 56 and 14 <= row["point"][1] <= 25):
            raise ValueError("projected PRG point outside Poland")
    value = {"capture": {"geometry_crs": "EPSG:4326", "source_crs": "EPSG:2180", "gml_axis_order": "northing,easting", "always_xy": True,
                         "seed": seed, "candidates_per_province": size, "selection": "lowest SHA256(seed NUL address UUID) per province among open-version, numbered/postcoded points; no backfill after reference joins",
                         "source": {key: receipt[key] for key in ["url", "bytes", "sha256"]}, "duckdb": duckdb_version, "preparer_sha256": signature["script"], "members": members},
             "addresses": sorted(captured, key=lambda row: row["id"])}
    data = encode(value)
    out.parent.mkdir(parents=True, exist_ok=True)
    try:
        with out.open("xb") as stream:
            stream.write(data)
    except FileExistsError:
        if out.read_bytes() != data:
            raise ValueError("refusing to overwrite a different final capture")
    print(f"{out}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--seed", required=True)
    parser.add_argument("--candidates-per-province", type=int, default=2000)
    args = parser.parse_args()
    capture(args.source, args.out, args.seed, args.candidates_per_province)
