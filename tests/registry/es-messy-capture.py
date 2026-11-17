#!/usr/bin/env python3
"""Re-fetch the CartoCiudad features `es-messy.seeds.jsonl` uses, instead of the 5-hour province capture.

  python3 tests/registry/es-messy-capture.py tests/registry/es.jsonl tests/registry/es-messy.seeds.jsonl OUT/cartociudad.json

`es.jsonl` gives each location's point; a bounding box of about 30 m around it returns the
candidate features, and the one whose SHA-256 is the seed's `ref` is kept, with its neighbours,
so `build-gold` still sees every row of the location's group. A location whose feature changed
upstream since `es.jsonl` (renamed street, moved number) is listed in `OUT.missing` and left out of
the seeds (`es-messy.py` takes that file). The snapshot repeats the capture
metadata of `es.jsonl` and says so in `capture.retrieval`.
"""
import hashlib
import json
import sys
import time
import urllib.parse
import urllib.request

URL = "https://api-features.idee.es/collections/address/items"
FIELDS = open(__file__.replace("es-messy-capture.py", "export-cartociudad-addresses.py")).read().split('FIELDS = "')[1].split('"')[0]


def location(f):
    p = {k: (v or "").strip() for k, v in f["properties"].items()}
    number = p["locator_designator_addressNumber"] + p["locator_designator_addressNumberExtension"]
    key = "\0".join([p["component_AdminUnitName_3"], p["component_AdminUnitName_4"], p["component_AddressAreaName"], p["component_ThoroughfareName"], number])
    return hashlib.sha256(key.encode()).hexdigest()


def fetch(lon, lat):
    box = 0.0003
    query = urllib.parse.urlencode(dict(f="json", properties=FIELDS, limit=100, bbox=f"{lon - box},{lat - box},{lon + box},{lat + box}"))
    for attempt in range(4):
        try:
            req = urllib.request.Request(f"{URL}?{query}", headers={"User-Agent": "OpenGeocoding registry capture"})
            return json.load(urllib.request.urlopen(req, timeout=90))["features"]
        except Exception:
            time.sleep(2 ** attempt)
    raise SystemExit(f"no answer for {lon},{lat}")


def main():
    registry, seeds, out = sys.argv[1:4]
    points, capture = {}, None
    for line in map(json.loads, open(registry)):
        points[line["location"].split(":", 1)[1]] = line["expect"]["point"]
        capture = capture or line["source"]["capture"]
    refs = sorted({json.loads(l)["ref"] for l in open(seeds)})
    features, missing = {}, []
    for n, ref in enumerate(refs):
        lat, lon = points[ref]
        rows = fetch(lon, lat)
        if not any(location(f) == ref for f in rows):
            missing.append(ref)
        for f in rows:
            features[f["id"]] = f
        if n % 50 == 0:
            print(f"{n}/{len(refs)} locations, {len(features)} features", flush=True)
    open(out + ".missing", "w").write("".join(f"{m}\n" for m in missing))
    print(f"{len(missing)} locations changed upstream since es.jsonl; listed in {out}.missing")
    capture = dict(capture, retrieval=f"targeted re-fetch around {len(refs)} points of es.jsonl, {time.strftime('%Y-%m-%d')}")
    value = {"type": "FeatureCollection", "capture": capture, "sources": [{"url": URL}], "features": sorted(features.values(), key=lambda f: f["id"])}
    open(out, "wb").write((json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":")) + "\n").encode())
    print(f"{out}: {len(features)} features")


main()
