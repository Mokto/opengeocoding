import argparse
import hashlib
import json
from pathlib import Path
import urllib.parse
import urllib.request

BASE = "https://services.arcgis.com/xdsHIIxuCWByZiCB/arcgis/rest/services/LINZ_NZ_Addresses/FeatureServer/0"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--seed", required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)

    def fetch(name, url, verify=False):
        path = args.out / name
        if verify or not path.exists():
            request = url
            if len(url) > 2000:
                endpoint, params = url.split("?", 1)
                request = urllib.request.Request(endpoint, data=params.encode(), headers={"Content-Type": "application/x-www-form-urlencoded"})
            with urllib.request.urlopen(request, timeout=180) as response:
                data = response.read()
            parsed = json.loads(data)
            if "error" in parsed:
                raise ValueError(parsed["error"])
            if path.exists():
                cached = json.loads(path.read_bytes())
                if not parsed.get("editingInfo") or cached.get("editingInfo") != parsed["editingInfo"]:
                    raise ValueError("Service changed since checkpoint; start a new dated directory")
            else:
                with path.open("xb") as output:
                    output.write(data)
        data = path.read_bytes()
        print(f"{name}: {len(data)} bytes", flush=True)
        return json.loads(data), {"file": name, "url": url, "sha256": hashlib.sha256(data).hexdigest()}

    def query(name, **params):
        return fetch(name, BASE + "/query?" + urllib.parse.urlencode({"f": "json", **params}))

    before, source = fetch("layer.json", BASE + "?f=json", verify=True)
    if not before.get("editingInfo"):
        raise ValueError("Source does not expose an edit timestamp")
    sources = [source]
    count, source = query("count.json", where="address_lifecycle = 'Current'", returnCountOnly="true")
    sources.append(source)
    maximum, source = query("maximum.json", where="1=1", outFields="OBJECTID", orderByFields="OBJECTID DESC", resultRecordCount=1, returnGeometry="false")
    sources.append(source)
    maximum = maximum["features"][0]["attributes"]["OBJECTID"]
    population = []
    for start in range(0, maximum, 500000):
        ids, source = query(f"ids-{start}.json", where=f"address_lifecycle = 'Current' AND OBJECTID > {start} AND OBJECTID <= {start + 500000}", returnIdsOnly="true")
        if ids.get("exceededTransferLimit"):
            raise ValueError("Truncated ID frame")
        population.extend(ids["objectIds"])
        sources.append(source)
    if len(population) != count["count"] or len(set(population)) != len(population):
        raise ValueError("Incomplete or duplicate source OBJECTIDs")
    selected = sorted(population, key=lambda id: hashlib.sha256(f"{args.seed}\0{id}".encode()).digest())[:10000]
    rows = []
    for page, start in enumerate(range(0, len(selected), 1000)):
        batch = selected[start:start + 1000]
        data, source = query(f"page-{page}.json", objectIds=",".join(map(str, batch)), outFields="*", outSR=4326, returnGeometry="true")
        if data.get("exceededTransferLimit") or {r["attributes"]["OBJECTID"] for r in data["features"]} != set(batch):
            raise ValueError("Incomplete candidate page; retain it for diagnosis")
        if data["spatialReference"]["wkid"] != 4326:
            raise ValueError("Expected WGS84 output")
        rows.extend(data["features"])
        sources.append(source)
    after, source = fetch("layer-after.json", BASE + "?f=json", verify=True)
    sources.append(source)
    if before.get("editingInfo") != after.get("editingInfo"):
        raise ValueError("Service changed during export; start a new dated directory")
    rows.sort(key=lambda row: row["attributes"]["address_id"])
    snapshot = {"seed": args.seed, "population": len(population), "candidate_count": len(rows), "sources": sources, "features": rows}
    data = (json.dumps(snapshot, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode()
    path = args.out / "linz.json"
    if path.exists() and path.read_bytes() != data:
        raise FileExistsError(f"Refusing to change {path}")
    if not path.exists():
        with path.open("xb") as output:
            output.write(data)
    print(f"{len(rows)} candidates from {len(population)} current records; SHA256 {hashlib.sha256(data).hexdigest()}", flush=True)


if __name__ == "__main__":
    main()
