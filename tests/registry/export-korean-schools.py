import argparse
import hashlib
import json
import math
from pathlib import Path
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET

parser = argparse.ArgumentParser()
parser.add_argument("--out", type=Path, required=True)
args = parser.parse_args()
if args.out.exists():
    raise FileExistsError(f"Refusing to overwrite {args.out}")
args.out.parent.mkdir(parents=True, exist_ok=True)
base = "https://www.data.go.kr"
metadata_url = base + "/download/columList.json?pk=15021148&ext=CSV"


def fetch(url, suffix):
    path = args.out.with_suffix(suffix)
    if path.exists():
        raise FileExistsError(f"Refusing to overwrite {path}")
    with urllib.request.urlopen(url, timeout=90) as response:
        data = response.read()
    with path.open("xb") as output:
        output.write(data)
    print(f"Saved {path.name}: {len(data)} bytes", flush=True)
    return data, {"url": url, "file": path.name, "sha256": hashlib.sha256(data).hexdigest()}


rights_data, rights = fetch(base + "/biz/dcat/metadata/15021148.do", ".rights.xml")
claims = [node.text.strip() for node in ET.fromstring(rights_data).iter("{http://purl.org/dc/terms/}rights") if node.text]
if not claims or set(claims) != {"이용허락범위 제한 없음"}:
    raise ValueError("Reuse terms changed; review them before exporting")
header_data, metadata = fetch(metadata_url, ".columns.json")
header = json.loads(header_data)
count = header["totalCount"]
if type(count) is not int or count <= 0:
    raise ValueError("Expected a positive dataset row count")
records = []
sources = [rights, metadata]
for page in range(1, math.ceil(count / 10000) + 1):
    query = urllib.parse.urlencode({
        "publicDataPk": "15021148",
        "svcTableNm": header["tableVO"]["svcTableNm"],
        "colNmList": header["tableVO"]["colNmList"],
        "totalCount": count,
        "perPage": 10000,
        "page": page,
    }, doseq=True)
    page_data, source = fetch(base + "/download/standard.json?" + query, f".page-{page}.json")
    rows = json.loads(page_data)
    if not isinstance(rows, list) or len(rows) != min(10000, count - len(records)):
        raise ValueError(f"Unexpected row count in page {page}")
    records.extend(rows)
    sources.append(source)
if len(records) != count or len({row["SCHOOL_ID"] for row in records}) != count:
    raise ValueError("Snapshot row count changed or school IDs repeat")
records.sort(key=lambda row: row["SCHOOL_ID"])
data = (json.dumps(records, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n").encode()
manifest = json.dumps({
    "dataset": "https://www.data.go.kr/data/15021148/standard.do",
    "records": count,
    "reference_dates": sorted({row["REFERENCE_DATE"] for row in records}),
    "sha256": hashlib.sha256(data).hexdigest(),
    "sources": sources,
}, ensure_ascii=False, indent=2) + "\n"
with args.out.with_suffix(".manifest.json").open("x", encoding="utf-8") as output:
    output.write(manifest)
with args.out.open("xb") as output:
    output.write(data)
print(f"{count} records -> {args.out}", flush=True)
