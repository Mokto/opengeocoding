#!/usr/bin/env python3
"""Capture a seeded candidate pool from Geoscience Australia's G-NAF adaptation."""

import argparse
import hashlib
import json
from pathlib import Path
import urllib.parse
import urllib.request

URL = "https://services-ap1.arcgis.com/ypkPEy1AmwPKGNNv/arcgis/rest/services/national_address_points/FeatureServer/0"
ITEM = "https://www.arcgis.com/sharing/rest/content/items/41473f0c760842a38cb3bcf6a3f91935"


def encode(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def draw_ids(seed, first, last, size):
    if not 0 < size <= last - first + 1:
        raise ValueError("candidate count must fit the OBJECTID range")
    span = last - first + 1
    ceiling = (1 << 256) // span * span
    selected = set()
    counter = 0
    while len(selected) < size:
        value = int.from_bytes(hashlib.sha256(f"{seed}\0{counter}".encode()).digest(), "big")
        counter += 1
        if value < ceiling:
            selected.add(first + value % span)
    return sorted(selected)


def capture(out, seed, size):
    out.mkdir(parents=True, exist_ok=True)
    receipts = []

    def get(name, url, **params):
        form = urllib.parse.urlencode(dict(f="json", **params)).encode()
        body = form if url.endswith("/query") else None
        if body is None:
            url += "?" + form.decode()
        path = out / name
        request = encode({"url": url, "form": form.decode() if body else None})
        request_path = path.with_suffix(path.suffix + ".request.json")
        if request_path.exists() and request_path.read_bytes() != request:
            raise ValueError(f"capture parameters changed: {path}")
        request_path.write_bytes(request)
        if not path.exists():
            req = urllib.request.Request(url, data=body, headers={"User-Agent": "OpenGeocoding registry capture"})
            with urllib.request.urlopen(req, timeout=90) as response:
                data = response.read()
            value = json.loads(data)
            if "error" in value:
                raise ValueError(value["error"])
            path.write_bytes(data)
        data = path.read_bytes()
        value = json.loads(data)
        if "error" in value:
            raise ValueError(value["error"])
        receipts.append({"file": name, "url": url, "form": form.decode() if body else None, "sha256": hashlib.sha256(data).hexdigest()})
        return value

    def live_layer():
        with urllib.request.urlopen(URL + "?f=json", timeout=90) as response:
            return json.load(response)

    metadata = get("layer.json", URL)
    get("item.json", ITEM)
    if metadata["editingInfo"] != live_layer()["editingInfo"]:
        raise ValueError("layer changed since checkpoint; start a new capture directory")
    query = URL + "/query"
    total = get("count.json", query, where="1=1", returnCountOnly="true")["count"]
    bounds = []
    for order in ["ASC", "DESC"]:
        page = get(f"bound-{order}.json", query, where="1=1", outFields="OBJECTID", returnGeometry="false", orderByFields=f"OBJECTID {order}", resultRecordCount=1)
        bounds.append(page["features"][0]["attributes"]["OBJECTID"])
    selected = draw_ids(seed, *bounds, size)
    features = []
    for start in range(0, len(selected), 400):
        ids = selected[start:start + 400]
        where = "OBJECTID IN (" + ",".join(map(str, ids)) + ")"
        count = get(f"count-{start:05}.json", query, where=where, returnCountOnly="true")["count"]
        page = get(f"page-{start:05}.json", query, objectIds=",".join(map(str, ids)), outFields="*", outSR=4326, returnGeometry="true")
        rows = page["features"]
        returned = [f["attributes"]["OBJECTID"] for f in rows]
        if page.get("exceededTransferLimit") or len(rows) != count or len(set(returned)) != len(rows) or not set(returned) <= set(ids):
            raise ValueError(f"truncated, duplicate or unexpected OBJECTIDs in page {start}")
        if page["spatialReference"].get("wkid") != 4326:
            raise ValueError("expected EPSG:4326 geometry")
        features.extend(rows)
        print(f"G-NAF: {start + len(ids)}/{size} candidate IDs; {len(features)} source rows", flush=True)
    if metadata["editingInfo"] != live_layer()["editingInfo"]:
        raise ValueError("layer changed during capture; start a new directory")
    value = {
        "capture": {"seed": seed, "candidate_ids": size, "first_objectid": bounds[0], "last_objectid": bounds[1], "source_count": total, "editing_info": metadata["editingInfo"], "geometry_crs": "EPSG:4326", "selection": "uniform SHA256 rejection draw from OBJECTID range; gaps omitted"},
        "sources": receipts,
        "features": sorted(features, key=lambda f: f["attributes"]["OBJECTID"]),
    }
    path = out / "gnaf.json"
    data = encode(value)
    if path.exists() and path.read_bytes() != data:
        raise ValueError("refusing to overwrite a different final snapshot")
    path.write_bytes(data)
    print(f"{path}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--seed", required=True)
    parser.add_argument("--candidates", type=int, default=20000)
    args = parser.parse_args()
    capture(args.out, args.seed, args.candidates)
