import argparse
import collections
import gzip
import hashlib
import io
import json
import pathlib
import re
import urllib.request

COMMIT = "c2eff0881e3c75b8822f8c84ea9d23affc11aadb"
URL = f"https://raw.githubusercontent.com/cahyadsn/wilayah/{COMMIT}/db/wilayah.sql"
SHA256 = "c4c3396d9380d4edee072af1d9dff83573b574d7cd00a6562cf82e200e954031"
COUNTS = {1: 38, 2: 514, 3: 7285, 4: 83762}


def prepare(source, output, manifest):
    if source:
        raw = source.read_bytes()
    else:
        with urllib.request.urlopen(URL, timeout=60) as response:
            raw = response.read(4 * 1024 * 1024 + 1)
    if hashlib.sha256(raw).hexdigest() != SHA256:
        raise ValueError("expected the pinned names/code SQL file")
    pattern = re.compile(r"^\('([0-9]{2}(?:\.[0-9]{2}){0,2}(?:\.[0-9]{4})?)','((?:\\.|''|[^'])*)'\)[,;]$")
    records = {}
    for line in raw.decode("utf-8").splitlines():
        if not line.startswith("('"):
            continue
        match = pattern.fullmatch(line)
        if not match:
            raise ValueError(f"invalid source record: {line!r}")
        code, name = match.groups()
        name = name.replace("\\'", "'").replace("''", "'").replace("\\\\", "\\")
        if code in records or not name.strip() or any(ord(c) < 32 for c in name):
            raise ValueError(f"invalid name or duplicate code: {code}")
        records[code] = name
    counts = collections.Counter(code.count(".") + 1 for code in records)
    if counts != COUNTS:
        raise ValueError(f"unexpected national counts: {counts}")
    if any(code.rsplit(".", 1)[0] not in records for code in records if "." in code):
        raise ValueError("missing parent code")
    encoded = json.dumps(sorted(records.items()), ensure_ascii=False, separators=(",", ":")).encode()
    buffer = io.BytesIO()
    with gzip.GzipFile(fileobj=buffer, mode="wb", mtime=0) as stream:
        stream.write(encoded)
    compressed = buffer.getvalue()
    output.write_bytes(compressed)
    manifest.write_text(json.dumps({
        "repository": "https://github.com/cahyadsn/wilayah",
        "commit": COMMIT,
        "url": URL,
        "source_sha256": SHA256,
        "gzip_sha256": hashlib.sha256(compressed).hexdigest(),
        "license": "MIT",
        "decree": "Kepmendagri No. 300.2.2-2138 Tahun 2025",
        "fields": ["administrative_code", "name"],
        "counts": dict(sorted(counts.items())),
    }, indent=2) + "\n")
    print(f"{len(records)} names/code pairs, {len(compressed)} compressed bytes; no coordinates")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=pathlib.Path)
    parser.add_argument("--out", type=pathlib.Path, default=pathlib.Path("data/id-hierarchy.json.gz"))
    parser.add_argument("--manifest", type=pathlib.Path, default=pathlib.Path("data/id-hierarchy.sources.json"))
    args = parser.parse_args()
    prepare(args.source, args.out, args.manifest)
