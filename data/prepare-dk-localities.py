import argparse
import gzip
import hashlib
import json
import pathlib
import subprocess
import tempfile
import zipfile

SOURCE_SHA256 = "a56addf371a081c75f88e567040d4935ef5f1185ddf2d3d167289d32ee0bd16c"


def prepare(source, output):
    if hashlib.sha256(source.read_bytes()).hexdigest() != SOURCE_SHA256:
        raise ValueError("expected the pinned KDS Bebyggelser_2024.zip")
    with tempfile.TemporaryDirectory() as directory:
        root = pathlib.Path(directory)
        with zipfile.ZipFile(source) as archive:
            for name in ("Byer.shp", "Byer.shx", "Byer.dbf", "Byer.prj"):
                (root / name).write_bytes(archive.read(name))
        rows = root / "rows.jsonl"
        quote = lambda p: "'" + str(p).replace("'", "''") + "'"
        subprocess.run([
            "duckdb", "-no-agent", "-c",
            "LOAD spatial; COPY (SELECT CAST(kode AS UINTEGER) code,navn name,"
            "ST_AsGeoJSON(ST_Transform(geom,'EPSG:25832','EPSG:4326',always_xy:=true)) geometry "
            f"FROM ST_Read({quote(root / 'Byer.shp')}) ORDER BY kode,lokalID,OGC_FID) "
            f"TO {quote(rows)} (FORMAT JSON);",
        ], check=True)
        records = []
        for line in rows.open():
            row = json.loads(line)
            geometry = row["geometry"]
            if geometry["type"] != "Polygon" or not row["name"] or not row["code"]:
                raise ValueError("invalid settlement polygon")
            rings = []
            for ring in geometry["coordinates"]:
                previous = (0, 0)
                deltas = []
                for lon, lat in ring:
                    if not (7.5 <= lon <= 16 and 54 <= lat <= 58.5):
                        raise ValueError("polygon outside Denmark")
                    point = (round(lon * 10_000_000), round(lat * 10_000_000))
                    deltas.append([point[0] - previous[0], point[1] - previous[1]])
                    previous = point
                rings.append(deltas)
            records.append([row["code"], row["name"], rings])
        if len(records) != 8006:
            raise ValueError("unexpected settlement count")
        encoded = json.dumps(records, ensure_ascii=False, separators=(",", ":")).encode()
        output.write_bytes(gzip.compress(encoded, mtime=0))
        print(f"{len(records)} polygons, {output.stat().st_size} bytes, SHA256 {hashlib.sha256(output.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    args = parser.parse_args()
    prepare(args.source, args.output)
