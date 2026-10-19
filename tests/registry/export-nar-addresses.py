#!/usr/bin/env python3
"""Prepare a deterministic Canadian NAR candidate pool from an offline official ZIP."""

import argparse
from collections import Counter, defaultdict
import csv
import hashlib
import heapq
import io
import json
import math
from pathlib import Path
import re
import zipfile

URL = "https://www150.statcan.gc.ca/n1/pub/46-26-0002/2022001/202606.zip"
PROVINCES = dict(zip("10 11 12 13 24 35 46 47 48 59 60 61 62".split(), "NL PE NS NB QC ON MB SK AB BC YT NT NU".split()))
GUID = re.compile(r"[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}")
FIELDS = "LOC_GUID ADDR_GUID APT_NO_LABEL CIVIC_NO CIVIC_NO_SUFFIX OFFICIAL_STREET_NAME OFFICIAL_STREET_TYPE OFFICIAL_STREET_DIR PROV_CODE CSD_ENG_NAME CSD_FRE_NAME MAIL_STREET_NAME MAIL_STREET_TYPE MAIL_STREET_DIR MAIL_MUN_NAME MAIL_PROV_ABVN MAIL_POSTAL_CODE".split()
LOCATION_FIELDS = "LOC_GUID CSD_CODE BG_LATITUDE BG_LONGITUDE".split()


def encode(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def rows(archive, kind, required, province_only=None):
    names = sorted(archive.namelist())
    if len(names) != len(set(names)):
        raise ValueError("duplicate ZIP member")
    folder = "Locations" if kind == "Location" else "Addresses"
    pattern = re.compile(rf"{folder}/{kind}_(\d{{2}})(?:_part_\d+)?\.csv")
    found = []
    for name in names:
        match = pattern.fullmatch(name)
        if not match:
            if name.startswith(folder + "/") and not name.endswith("/"):
                raise ValueError(f"unexpected {kind} member: {name}")
            continue
        province = match[1]
        if province not in PROVINCES:
            raise ValueError(f"unknown province in {name}")
        found.append(province)
        if province_only is not None and province != province_only:
            continue
        with archive.open(name) as stream:
            reader = csv.DictReader(io.TextIOWrapper(stream, encoding="utf-8-sig", newline=""), strict=True)
            if not reader.fieldnames or not set(required) <= set(reader.fieldnames) or len(reader.fieldnames) != len(set(reader.fieldnames)):
                raise ValueError(f"missing or duplicate {kind} columns: {name}")
            for row in reader:
                if None in row or any(row[field] is None for field in required):
                    raise ValueError(f"invalid CSV row: {name}:{reader.line_num}")
                yield province, {field: row[field].strip() for field in required}
        print(f"NAR: read {name}", flush=True)
    if set(found) != set(PROVINCES):
        raise ValueError(f"incomplete province frame for {kind}: {sorted(set(found))}")


def number(row):
    return bool(re.fullmatch(r"[1-9][0-9]*", row["CIVIC_NO"]) and re.fullmatch(r"(?:[A-Za-z]|[0-9]+/[0-9]+)?", row["CIVIC_NO_SUFFIX"]))


def eligible_address(row, province):
    return (
        row["PROV_CODE"] == province
        and row["MAIL_PROV_ABVN"] == PROVINCES[province]
        and number(row)
        and bool(row["MAIL_STREET_NAME"] and row["MAIL_MUN_NAME"])
        and bool(re.fullmatch(r"[ABCEGHJ-NPRSTVXY][0-9][ABCEGHJ-NPRSTVWXYZ] ?[0-9][ABCEGHJ-NPRSTVWXYZ][0-9]", row["MAIL_POSTAL_CODE"]))
        and bool(re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9 /-]*|", row["APT_NO_LABEL"]))
        and not any(any(ord(c) < 32 or ord(c) == 127 for c in value) for value in row.values())
    )


def prepare(archive, seed, size, province_only=None):
    if size <= 0:
        raise ValueError("candidates per province must be positive")
    counts = Counter()
    frame = Counter()
    pools = defaultdict(list)
    seen = set()
    for province, row in rows(archive, "Location", LOCATION_FIELDS, province_only):
        counts["source_locations"] += 1
        guid = row["LOC_GUID"]
        if not GUID.fullmatch(guid) or guid in seen:
            raise ValueError(f"duplicate or invalid location UUID: {guid}")
        seen.add(guid)
        if not re.fullmatch(province + r"[0-9]{5}", row["CSD_CODE"]):
            counts["locations_without_matching_csd"] += 1
            continue
        try:
            lat, lon = float(row["BG_LATITUDE"]), float(row["BG_LONGITUDE"])
        except ValueError:
            counts["locations_without_bg_point"] += 1
            continue
        if not (math.isfinite(lat) and math.isfinite(lon) and 41 <= lat <= 84 and -142 <= lon <= -50):
            counts["locations_without_bg_point"] += 1
            continue
        frame[province] += 1
        rank = int.from_bytes(hashlib.sha256(f"{seed}\0{guid}".encode()).digest(), "big")
        pool = pools[province]
        item = (-rank, guid, row)
        if len(pool) < size:
            heapq.heappush(pool, item)
        elif item[:2] > pool[0][:2]:
            heapq.heapreplace(pool, item)
    del seen
    candidates = {guid: row for pool in pools.values() for _, guid, row in pool}
    chosen = {}
    seen_addresses = set()
    for province, row in rows(archive, "Address", FIELDS, province_only):
        counts["source_addresses"] += 1
        guid = row["LOC_GUID"]
        if guid not in candidates:
            continue
        address_id = row["ADDR_GUID"]
        if not GUID.fullmatch(address_id) or address_id in seen_addresses:
            raise ValueError(f"duplicate or invalid selected address UUID: {address_id}")
        seen_addresses.add(address_id)
        if candidates[guid]["CSD_CODE"][:2] != province or not eligible_address(row, province):
            counts["ineligible_candidate_address_rows"] += 1
            continue
        counts["eligible_candidate_address_rows"] += 1
        rank = (not bool(row["APT_NO_LABEL"]), address_id)
        if guid not in chosen or rank < chosen[guid][0]:
            chosen[guid] = (rank, row)
    counts["sampled_locations"] = len(candidates)
    counts["sampled_locations_without_eligible_address"] = len(candidates) - len(chosen)
    counts["candidate_locations"] = len(chosen)
    if not chosen and province_only is None:
        raise ValueError("no eligible NAR candidates")
    return {
        "capture": {
            "snapshot": "2026-06",
            "geometry_crs": "EPSG:4326",
            "seed": seed,
            "candidates_per_province": size,
            "selection": "lowest SHA256(seed NUL LOC_GUID) among BG-georeferenced locations with matching CSD in each province; no backfill after address eligibility; prefer a published unit, then lowest ADDR_GUID",
            "eligible_location_frame_by_province": dict(sorted(frame.items())),
            "counts": dict(sorted(counts.items())),
        },
        "addresses": [{"location": candidates[guid], "address": chosen[guid][1]} for guid in sorted(chosen)],
    }


def capture(source, out, seed, size):
    receipt = json.loads(source.with_suffix(".zip.receipt.json").read_text())
    digest = hashlib.sha256()
    with source.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    if receipt["url"] != URL or source.stat().st_size != receipt["bytes"] or digest.hexdigest() != receipt["sha256"]:
        raise ValueError("NAR archive receipt mismatch")
    signature = {"archive": digest.hexdigest(), "script": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "seed": seed, "size": size}
    work = out.with_suffix(".work")
    work.mkdir(parents=True, exist_ok=True)
    addresses = []
    counts = Counter()
    frame = {}
    with zipfile.ZipFile(source) as archive:
        for province in sorted(PROVINCES):
            checkpoint = work / f"{province}.json"
            binding = dict(signature, province=province)
            if checkpoint.exists():
                saved = json.loads(checkpoint.read_bytes())
                if saved["input"] != binding:
                    raise ValueError(f"checkpoint changed; use a fresh output directory: {checkpoint}")
                result = saved["data"]
                if saved["sha256"] != hashlib.sha256(encode(result)).hexdigest():
                    raise ValueError(f"checkpoint checksum mismatch: {checkpoint}")
            else:
                result = prepare(archive, seed, size, province)
                temporary = checkpoint.with_suffix(".tmp")
                temporary.write_bytes(encode({"input": binding, "data": result, "sha256": hashlib.sha256(encode(result)).hexdigest()}))
                temporary.replace(checkpoint)
            addresses.extend(result["addresses"])
            counts.update(result["capture"]["counts"])
            frame[province] = result["capture"]["eligible_location_frame_by_province"].get(province, 0)
            print(f"NAR {province}: {result['capture']['counts']}", flush=True)
    if not addresses or len({row["location"]["LOC_GUID"] for row in addresses}) != len(addresses):
        raise ValueError("empty or duplicate NAR candidate locations")
    value = {"capture": dict(result["capture"], counts=dict(sorted(counts.items())), eligible_location_frame_by_province=frame), "addresses": sorted(addresses, key=lambda row: row["location"]["LOC_GUID"])}
    value["capture"]["source"] = {key: receipt[key] for key in ["url", "bytes", "sha256", "last_modified"]}
    value["capture"]["preparer_sha256"] = signature["script"]
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
