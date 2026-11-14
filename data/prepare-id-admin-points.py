#!/usr/bin/env python3
import argparse
import collections
import csv
import gzip
import hashlib
import io
import json
import math
from pathlib import Path
import re
import unicodedata
import zipfile

ARCHIVE_SHA256 = "efb8e27e66ba86d8269b8f57718d5872447d6f675249194c07152990d8577740"
HIERARCHY_SHA256 = "162207b92490aafb5cb4b0e95f4ab3f6f5a73918b7068c970c9608c221c6c506"
LABELS = {
    1: r"^(?:provinsi|prov)[. ]+",
    2: r"^(?:kabupaten|kab|kota)[. ]+",
    3: r"^(?:kecamatan|kec|distrik)[. ]+",
    4: r"^(?:desa|kelurahan|gampong|nagari)[. ]+",
}


def key(name, level):
    text = "".join(c for c in unicodedata.normalize("NFKD", name).lower() if not unicodedata.combining(c))
    if any(c.isalpha() and not c.isascii() for c in text):
        return None
    label = re.match(LABELS[level], text)
    kind = ""
    if level == 2 and label:
        kind = "kota:" if text.startswith("kota") else "kabupaten:"
    return kind + re.sub("[^a-z0-9/]", "", re.sub(LABELS[level], "", text))


def crosswalk(rows, units):
    unit_names = dict(units)
    names = collections.defaultdict(set)
    bare_counties = collections.defaultdict(set)
    for code, name in units:
        level = code.count(".") + 1
        scope = "" if level == 1 else code[:2] if level < 4 else code.rsplit(".", 1)[0]
        names[level, scope, key(name, level)].add(code)
        if level == 2:
            bare_counties[scope, key(name, level).split(":")[-1]].add(code)
    for (scope, bare), codes in bare_counties.items():
        if len(codes) == 1:
            names[2, scope, bare] = codes
    identifiers = set()
    codes = collections.defaultdict(list)
    for row in rows:
        if len(row) != 19 or row[6] != "A" or row[8] != "ID" or row[7] not in {"ADM1", "ADM2", "ADM3", "ADM4"}:
            raise ValueError("invalid Indonesian administrative row")
        if not row[0].isdigit() or row[0] in identifiers:
            raise ValueError("duplicate or invalid GeoNames identifier")
        identifiers.add(row[0])
        level = int(row[7][-1])
        if not row[10] or row[9] not in {"", "ID"} or not row[9 + level]:
            continue
        codes[level, tuple(row[10:10 + level])].append(row[0])
    matched = {}
    reasons = collections.Counter()
    for level in range(1, 5):
        for row in rows:
            if row[7] != f"ADM{level}":
                continue
            if codes[level, tuple(row[10:10 + level])] != [row[0]]:
                reasons[level, "invalid_or_duplicate_code"] += 1
                continue
            scope = ""
            if level > 1:
                parent_level = 3 if level == 4 else 1
                parent = codes[parent_level, tuple(row[10:10 + parent_level])]
                if len(parent) != 1 or parent[0] not in matched:
                    reasons[level, "unmapped_parent"] += 1
                    continue
                scope = matched[parent[0]]
            if level == 3 and re.match(LABELS[4], row[1].lower()):
                reasons[level, "wrong_level_label"] += 1
                continue
            found = set()
            for name in [row[1], row[2], *row[3].split(",")]:
                k = key(name, level)
                if k:
                    found.update(names.get((level, scope, k), ()))
            if level == 2 and ":" in (primary := key(row[1], level) or ""):
                found = {c for c in found if key(unit_names[c], 2).split(":")[0] == primary.split(":")[0]}
            if level >= 3 and row[11]:
                county = codes[2, tuple(row[10:12])]
                if len(county) != 1 or county[0] not in matched:
                    reasons[level, "unmapped_county"] += 1
                    continue
                found = {c for c in found if ".".join(c.split(".")[:2]) == matched[county[0]]}
            if len(found) != 1:
                reasons[level, "ambiguous_or_missing_name"] += 1
                continue
            matched[row[0]] = next(iter(found))
        reverse = collections.defaultdict(list)
        for row in rows:
            if row[7] == f"ADM{level}" and row[0] in matched:
                reverse[matched[row[0]]].append(row[0])
        for ids in reverse.values():
            if len(ids) > 1:
                for ident in ids:
                    del matched[ident]
                    reasons[level, "duplicate_unit"] += 1
    return matched, {f"{level}:{reason}": n for (level, reason), n in sorted(reasons.items())}


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(8 * 1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def prepare(archive, hierarchy, out, manifest):
    digest = sha256(archive)
    if digest != ARCHIVE_SHA256:
        raise ValueError("GeoNames archive SHA-256 mismatch")
    raw = hierarchy.read_bytes()
    if hashlib.sha256(raw).hexdigest() != HIERARCHY_SHA256:
        raise ValueError("Indonesian hierarchy SHA-256 mismatch")
    units = json.loads(gzip.decompress(raw))
    with zipfile.ZipFile(archive) as z, z.open("allCountries.txt") as stream:
        rows = [r for r in csv.reader(io.TextIOWrapper(stream, encoding="utf-8"), delimiter="\t")
                if len(r) == 19 and r[6] == "A" and r[8] == "ID" and r[7] in {"ADM1", "ADM2", "ADM3", "ADM4"}]
    mapped, reasons = crosswalk(rows, units)
    points = []
    unit_names = dict(units)
    for r in rows:
        if r[7] != "ADM3" or r[0] not in mapped:
            continue
        x, y = float(r[5]), float(r[4])
        if not math.isfinite(x) or not math.isfinite(y) or not (-180 <= x <= 180 and -90 <= y <= 90):
            raise ValueError("invalid GeoNames coordinate")
        evidence = next(n for n in [r[1], r[2], *r[3].split(",")] if key(n, 3) == key(unit_names[mapped[r[0]]], 3))
        points.append([int(r[0]), mapped[r[0]], x, y, r[1], evidence])
    locations = collections.Counter((p[2], p[3]) for p in points)
    unique = [p for p in points if locations[p[2], p[3]] == 1]
    reasons["3:duplicate_coordinates"] = len(points) - len(unique)
    points = sorted(unique, key=lambda r: r[1])
    output = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=0) as compressed:
        compressed.write(json.dumps(points, ensure_ascii=False, separators=(",", ":")).encode())
    payload = output.getvalue()
    out.write_bytes(payload)
    manifest.write_text(json.dumps({
        "source": "GeoNames allCountries.zip, cached 2026-10-27",
        "url": "https://download.geonames.org/export/dump/allCountries.zip",
        "archive_sha256": digest,
        "hierarchy_sha256": hashlib.sha256(raw).hexdigest(),
        "license": "CC-BY-4.0",
        "attribution": "GeoNames, https://www.geonames.org/",
        "readme": "https://download.geonames.org/export/dump/readme.txt",
        "features": dict(sorted(collections.Counter(r[7] for r in rows).items())),
        "crosswalk": dict(sorted(collections.Counter(r[7] for r in rows if r[0] in mapped).items())),
        "abstentions": reasons,
        "output_sha256": hashlib.sha256(payload).hexdigest(),
        "points": len(points),
        "limits": "Names/codes identify candidates, not surveyed seats or current boundaries. Runtime import also requires unique county containment and preserves existing points. ADM4 is audited but not imported as a populated settlement."
    }, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("archive", type=Path)
    parser.add_argument("--hierarchy", type=Path, default=Path("data/id-hierarchy.json.gz"))
    parser.add_argument("--out", type=Path, default=Path("data/id-admin-points.json.gz"))
    parser.add_argument("--manifest", type=Path, default=Path("data/id-admin-points.sources.json"))
    args = parser.parse_args()
    prepare(args.archive, args.hierarchy, args.out, args.manifest)
