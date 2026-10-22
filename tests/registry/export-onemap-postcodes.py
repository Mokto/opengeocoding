import argparse
import hashlib
import json
import os
from pathlib import Path
import time
import urllib.parse
import urllib.request

BASE = "https://www.onemap.gov.sg/api/common/elastic/search"
# Postal sectors 01 to 82; no sector outside them is in use.
FRAME = range(10000, 830000)


def draw(seed, size):
    return sorted((f"{n:06d}" for n in FRAME), key=lambda code: hashlib.sha256(f"{seed}\0{code}".encode()).digest())[:size]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--seed", required=True)
    parser.add_argument("--probes", type=int, default=1200)
    parser.add_argument("--pause", type=float, default=0.5)
    args = parser.parse_args()
    responses = args.out / "responses"
    responses.mkdir(parents=True, exist_ok=True)
    token = os.environ.get("ONEMAP_TOKEN")

    def fetch(code, page):
        url = BASE + "?" + urllib.parse.urlencode({"searchVal": code, "returnGeom": "Y", "getAddrDetails": "Y", "pageNum": page})
        path = responses / f"{code}-{page}.json"
        if not path.exists():
            if not token:
                raise ValueError("Set ONEMAP_TOKEN to a OneMap API token; the search API requires one")
            with urllib.request.urlopen(urllib.request.Request(url, headers={"Authorization": token}), timeout=60) as response:
                data = response.read()
            if "error" in json.loads(data):
                raise ValueError(f"{code} page {page}: {json.loads(data)['error']}")
            with path.open("xb") as output:
                output.write(data)
            time.sleep(args.pause)
        data = path.read_bytes()
        parsed = json.loads(data)
        if "error" in parsed or parsed["pageNum"] != page:
            raise ValueError(f"{path} is not page {page} of a successful search")
        return parsed, {"file": f"responses/{path.name}", "url": url, "sha256": hashlib.sha256(data).hexdigest()}

    probes, sources = [], []
    for n, code in enumerate(sorted(draw(args.seed, args.probes))):
        results, page = [], 1
        while True:
            parsed, source = fetch(code, page)
            sources.append(source)
            results.extend(parsed["results"])
            if page >= parsed["totalNumPages"]:
                break
            page += 1
        if len(results) != parsed["found"]:
            raise ValueError(f"{code}: {len(results)} results for {parsed['found']} found")
        probes.append({"postal": code, "results": results})
        if (n + 1) % 100 == 0:
            print(f"{n + 1} of {args.probes} postcodes probed", flush=True)
    snapshot = {"seed": args.seed, "frame": f"{FRAME.start:06d}-{FRAME.stop - 1:06d}", "sources": sources, "probes": probes}
    data = (json.dumps(snapshot, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode()
    path = args.out / "onemap.json"
    if path.exists() and path.read_bytes() != data:
        raise FileExistsError(f"Refusing to change {path}")
    if not path.exists():
        with path.open("xb") as output:
            output.write(data)
    found = sum(1 for p in probes if any(r["POSTAL"] == p["postal"] for r in p["results"]))
    print(f"{found} of {len(probes)} drawn postcodes found; SHA256 {hashlib.sha256(data).hexdigest()}", flush=True)


if __name__ == "__main__":
    main()
