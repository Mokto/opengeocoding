#!/usr/bin/env uv run python
"""Cuts OpenStreetMap extracts and OpenAddresses layers down to the boxes of boxes.txt.
usage: slice_sources.py BOXES OSM_DIR OA_DIR OUT_DIR   (writes OUT_DIR/osm and OUT_DIR/openaddresses)
Needs osmium-tool and shapely. OSM keeps the complete ways touching a box; OpenAddresses keeps
the rows whose first coordinate lies in one, and the source definition of every layer kept (layers without one are dropped, as the builder skips them)."""
import gzip, json, os, re, shutil, subprocess, sys, tempfile
from concurrent.futures import ProcessPoolExecutor
from shapely.geometry import box, mapping
from shapely.ops import unary_union

boxes_file, osm_dir, oa_dir, out = sys.argv[1:5]
boxes = [tuple(map(float, l.split()[1:])) for l in open(boxes_file) if l.strip()]
union = unary_union([box(*b) for b in boxes])
grid = {}  # 0.1 degree cell -> boxes touching it
for b in boxes:
    for cx in range(int(b[0] * 10) - 1, int(b[2] * 10) + 1):
        for cy in range(int(b[1] * 10) - 1, int(b[3] * 10) + 1):
            grid.setdefault((cx, cy), []).append(b)


def osm(name):
    src = os.path.join(osm_dir, name)
    info = subprocess.run(["osmium", "fileinfo", "-g", "header.boxes", src], capture_output=True, text=True, check=True).stdout.strip()
    if not info:
        return name, None
    x1, y1, x2, y2 = map(float, info.strip("()[]\" \n").replace(" ", "").split(",")[:4])
    part = union.intersection(box(x1, y1, x2, y2))
    if part.is_empty:
        return name, 0
    dst = os.path.join(out, "osm", name)
    with tempfile.NamedTemporaryFile("w", suffix=".geojson") as f:
        json.dump({"type": "Feature", "properties": {}, "geometry": mapping(part)}, f)
        f.flush()
        subprocess.run(["osmium", "extract", "-p", f.name, "--overwrite", "-o", dst, src], check=True)
    return name, os.path.getsize(dst)


COORD = re.compile(rb'"coordinates": *\[ *(-?[\d.]+), *(-?[\d.]+)')


def oa(name):
    src = name.removesuffix(".geojson.gz") + ".source.json"
    if not os.path.exists(os.path.join(oa_dir, src)):
        return name, 0, 0  # the builder skips a layer with no definition
    dst = os.path.join(out, "openaddresses", name)
    kept = total = 0
    with gzip.open(os.path.join(oa_dir, name), "rb") as f, gzip.open(dst + ".tmp", "wb", 6) as o:
        for line in f:
            total += 1
            m = COORD.search(line)
            if not m:
                continue
            x, y = float(m[1]), float(m[2])
            if any(b[0] <= x <= b[2] and b[1] <= y <= b[3] for b in grid.get((int(x * 10 // 1), int(y * 10 // 1)), ())):
                o.write(line)
                kept += 1
    if kept:
        os.replace(dst + ".tmp", dst)
        shutil.copy(os.path.join(oa_dir, src), os.path.join(out, "openaddresses", src))
    else:
        os.remove(dst + ".tmp")
    return name, kept, total


if __name__ == "__main__":
    for d in ("osm", "openaddresses"):
        shutil.rmtree(os.path.join(out, d), ignore_errors=True)
        os.makedirs(os.path.join(out, d))
    with ProcessPoolExecutor(os.cpu_count() - 2) as pool:
        for name, size in pool.map(osm, sorted(f for f in os.listdir(osm_dir) if f.endswith(".osm.pbf"))):
            print("osm", name, size, flush=True)
        for name, kept, total in pool.map(oa, sorted(f for f in os.listdir(oa_dir) if f.endswith(".geojson.gz")), chunksize=8):
            print("oa", name, kept, total, flush=True)
