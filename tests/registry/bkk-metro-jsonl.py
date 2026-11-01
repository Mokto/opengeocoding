#!/usr/bin/env python3
import argparse
import csv
import hashlib
import io
import json
from pathlib import Path
from zipfile import ZipFile


def prepare(source, out):
    digest = hashlib.sha256(source.read_bytes()).hexdigest()
    with ZipFile(source) as archive:
        def rows(name):
            with archive.open(name) as stream:
                yield from csv.DictReader(io.TextIOWrapper(stream, encoding="utf-8-sig", newline=""))

        routes = {r["route_id"]: r["route_short_name"] for r in rows("routes.txt") if r["route_type"] == "1"}
        trips = {r["trip_id"]: routes[r["route_id"]] for r in rows("trips.txt") if r["route_id"] in routes}
        stops = {}
        for row in rows("stops.txt"):
            if not row["stop_id"] or row["stop_id"] in stops:
                raise ValueError("empty or duplicate GTFS stop_id")
            stops[row["stop_id"]] = row
        served = {}
        for row in rows("stop_times.txt"):
            line = trips.get(row["trip_id"])
            if line is None:
                continue
            stop = stops[row["stop_id"]]
            parent = stops[stop["parent_station"]]
            if stop["location_type"] not in ("", "0") or parent["location_type"] != "1" or parent["parent_station"]:
                raise ValueError("expected a metro platform with a top-level parent station")
            served.setdefault(parent["stop_id"], set()).add(line)
        versions = list(rows("feed_info.txt"))
        if len(versions) != 1:
            raise ValueError("expected one GTFS feed_info record")
        feed = versions[0]
        records = []
        for station, lines in sorted(served.items()):
            row = stops[station]
            lat, lon = float(row["stop_lat"]), float(row["stop_lon"])
            if not 45.7 < lat < 48.7 or not 16 < lon < 23:
                raise ValueError("expected Hungarian WGS84 station coordinates")
            records.append({"record": row, "metro_lines": sorted(lines), "point": [lat, lon],
                            "feed_version": feed["feed_version"], "feed_start_date": feed["feed_start_date"],
                            "feed_end_date": feed["feed_end_date"], "source_sha256": digest})
    if not records:
        raise ValueError("no metro parent stations")
    out.write_text("".join(json.dumps(r, ensure_ascii=False, sort_keys=True, allow_nan=False) + "\n" for r in records), encoding="utf-8")
    print(f"{len(records)} metro parent stations -> {out}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    prepare(args.source, args.out)
