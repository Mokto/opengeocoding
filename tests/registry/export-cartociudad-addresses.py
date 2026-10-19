#!/usr/bin/env python3
"""Capture province-balanced official CartoCiudad features, not geocoder answers."""

import argparse
import hashlib
import json
from pathlib import Path
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET

URL = "https://api-features.idee.es/collections/address"
WFS = "https://www.cartociudad.es/wfs-inspire/direcciones"
FIELDS = "component_ThoroughfareName,locator_designator_addressNumber,locator_designator_addressNumberExtension,component_AdminUnitName_4,component_AdminUnitName_3,component_AdminUnitName_2,component_AddressAreaName,beginLifespanVersion,dataProvider,inspireId_namespace"
NS = {"ad": "http://inspire.ec.europa.eu/schemas/ad/4.0", "gn": "http://inspire.ec.europa.eu/schemas/gn/4.0"}
FILTER = '<fes:Filter xmlns:fes="http://www.opengis.net/fes/2.0" xmlns:ad="http://inspire.ec.europa.eu/schemas/ad/4.0" xmlns:xlink="http://www.w3.org/1999/xlink"><fes:PropertyIsEqualTo><fes:ValueReference>ad:level/@xlink:href</fes:ValueReference><fes:Literal>https://inspire.ec.europa.eu/codelist/AdministrativeHierarchyLevel/3rdOrder</fes:Literal></fes:PropertyIsEqualTo></fes:Filter>'


def encode(value):
    return (json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")) + "\n").encode()


def window_offsets(seed, province, count, width):
    if count <= 0 or width <= 0:
        raise ValueError("positive population and window width required")
    width = min(width, count)
    span = count - width + 1
    return sorted({int.from_bytes(hashlib.sha256(f"{seed}\0{province}\0{i}".encode()).digest(), "big") % span for i in range(3)})


def validate_page(page, province, count, size):
    features = page["features"]
    ids = [f["id"] for f in features]
    if page["numberMatched"] != count or page["numberReturned"] != size or len(features) != size or len(set(ids)) != size:
        raise ValueError("changed population, truncated page or duplicate feature IDs")
    for feature in features:
        props = feature["properties"]
        if props["component_AdminUnitName_3"] != province or "component_PostalDescriptor" in props:
            raise ValueError("province filter or postal-code exclusion not honored")
        if feature["geometry"]["type"] != "Point":
            raise ValueError("non-point address geometry")
    return features


def capture(out, seed, width):
    if width <= 0:
        raise ValueError("positive window width required")
    out.mkdir(parents=True, exist_ok=True)
    receipts = []

    def get(name, url, cached=True):
        path = out / name
        request_path = path.with_suffix(path.suffix + ".request.json")
        request = encode({"url": url})
        if request_path.exists() and request_path.read_bytes() != request:
            raise ValueError(f"capture parameters changed: {path}")
        request_path.write_bytes(request)
        if not cached or not path.exists():
            req = urllib.request.Request(url, headers={"User-Agent": "OpenGeocoding registry capture"})
            with urllib.request.urlopen(req, timeout=90) as response:
                data = response.read()
            if cached:
                path.write_bytes(data)
        else:
            data = path.read_bytes()
        if cached:
            receipts.append({"file": name, "url": url, "sha256": hashlib.sha256(data).hexdigest()})
        return data

    get("collection.json", URL + "?f=json")
    get("queryables.json", URL + "/queryables?f=json")
    get("license.pdf", "https://www.ign.es/resources/licencia/Condiciones_licenciaUso_IGN.pdf")
    province_url = WFS + "?" + urllib.parse.urlencode(dict(service="WFS", version="2.0.0", request="GetFeature", typeNames="ad:AdminUnitName", filter=FILTER, count=100))
    root = ET.fromstring(get("provinces.gml", province_url))
    units = root.findall(".//ad:AdminUnitName", NS)
    if int(root.attrib["numberReturned"]) != int(root.attrib["numberMatched"]) or len(units) != int(root.attrib["numberReturned"]):
        raise ValueError("incomplete official province frame")
    provinces = sorted({unit.find(".//gn:text", NS).text for unit in units})
    if len(provinces) != len(units):
        raise ValueError("duplicate province names")
    features = {}
    populations = {}
    windows = []
    for index, province in enumerate(provinces):
        params = dict(f="json", properties=FIELDS, component_AdminUnitName_3=province, sortby="inspireId_localId")
        count_url = URL + "/items?" + urllib.parse.urlencode(dict(params, limit=1))
        count = json.loads(get(f"population-{index:02}.json", count_url))["numberMatched"]
        populations[province] = count
        if count == 0:
            print(f"CartoCiudad {province}: no address records", flush=True)
            continue
        size = min(width, count)
        for offset in window_offsets(seed, province, count, width):
            url = URL + "/items?" + urllib.parse.urlencode(dict(params, offset=offset, limit=size))
            name = f"page-{index:02}-{offset}.json"
            page = json.loads(get(name, url))
            rows = validate_page(page, province, count, size)
            windows.append((name, url, province, count, size, rows))
            for feature in rows:
                old = features.setdefault(feature["id"], feature)
                if old != feature:
                    raise ValueError("conflicting duplicate address")
        print(f"CartoCiudad {index + 1}/{len(provinces)} {province}: population {count}; {len(features)} candidates", flush=True)
    for index, (name, url, province, count, size, rows) in enumerate(windows):
        current = json.loads(get(name, url, cached=False))
        if validate_page(current, province, count, size) != rows:
            raise ValueError("selected window changed since capture; start a new directory")
        if index % 10 == 0:
            print(f"CartoCiudad: revalidated {index + 1}/{len(windows)} windows", flush=True)
    value = {
        "type": "FeatureCollection",
        "capture": {"seed": seed, "window_width": width, "windows_per_province": 3, "populations": populations, "geometry_crs": "OGC:CRS84", "selection": "three seeded contiguous ID-ordered windows per populated official province; not a uniform address sample", "postal_codes": "excluded"},
        "sources": receipts,
        "features": sorted(features.values(), key=lambda f: f["id"]),
    }
    path = out / "cartociudad.json"
    data = encode(value)
    if path.exists() and path.read_bytes() != data:
        raise ValueError("refusing to overwrite a different final snapshot")
    path.write_bytes(data)
    print(f"{path}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--seed", required=True)
    parser.add_argument("--window", type=int, default=128)
    args = parser.parse_args()
    capture(args.out, args.seed, args.window)
