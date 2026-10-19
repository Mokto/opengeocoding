import argparse
import hashlib
import json
from pathlib import Path
import urllib.parse
import urllib.request

BASE = "https://serviciosgis.catastrobogota.gov.co/arcgis/rest/services/catastro/placadomiciliaria/MapServer/0"
RESIDUES = (137, 1871, 3023)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    rows = []
    sources = []
    for residue in RESIDUES:
        name = f"bogota-{residue}.json"
        path = args.out / name
        where = f"PDOTIPO=1 AND MOD(OBJECTID,5000)={residue}"
        url = BASE + "/query?" + urllib.parse.urlencode({"f": "json", "where": where, "outFields": "*", "outSR": 4326, "resultRecordCount": 1000})
        if not path.exists():
            if args.offline:
                raise FileNotFoundError(path)
            with urllib.request.urlopen(url, timeout=120) as response:
                data = response.read()
            with path.open("xb") as output:
                output.write(data)
        page = json.loads(path.read_bytes())
        if "error" in page or page.get("exceededTransferLimit") or page["spatialReference"]["wkid"] != 4326:
            raise ValueError(f"Incomplete or invalid source page {name}")
        if any(r["attributes"]["OBJECTID"] % 5000 != residue or r["attributes"]["PDOTIPO"] != 1 for r in page["features"]):
            raise ValueError(f"Source page does not match its sampling frame: {name}")
        rows.extend(page["features"])
        sources.append({"file": name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "url": url, "query": where})
        print(f"{name}: {len(page['features'])} principal plates", flush=True)
    if not rows or len({r["attributes"]["PDOCODIGO"] for r in rows}) != len(rows):
        raise ValueError("Empty or duplicate registry identifiers")
    rows.sort(key=lambda row: row["attributes"]["PDOCODIGO"])
    data = (json.dumps({"sources": sources, "features": rows}, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode()
    path = args.out / "bogota.json"
    if path.exists() and path.read_bytes() != data:
        raise FileExistsError(f"Refusing to change {path}")
    if not path.exists():
        with path.open("xb") as output:
            output.write(data)
    print(f"{len(rows)} source candidates; SHA256 {hashlib.sha256(data).hexdigest()}", flush=True)


if __name__ == "__main__":
    main()
