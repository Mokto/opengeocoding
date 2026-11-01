import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch
import urllib.parse
from zipfile import ZipFile


def module(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def linz_response(request, **kwargs):
    url = request.full_url if hasattr(request, "full_url") else request
    params = urllib.parse.parse_qs(request.data.decode() if hasattr(request, "data") and request.data else urllib.parse.urlsplit(url).query)
    if not url.split("?")[0].endswith("/query"):
        result = {"editingInfo": {"lastEditDate": 1}}
    elif "returnCountOnly" in params:
        result = {"count": 2}
    elif "orderByFields" in params:
        result = {"features": [{"attributes": {"OBJECTID": 500001}}]}
    elif "returnIdsOnly" in params:
        result = {"objectIds": [500001 if "OBJECTID > 500000" in params["where"][0] else 1]}
    else:
        result = {"spatialReference": {"wkid": 4326}, "features": [{"attributes": {"OBJECTID": i, "address_id": i}} for i in map(int, params["objectIds"][0].split(","))]}
    return io.BytesIO(json.dumps(result).encode())


def run_main(exporter, arguments, fetch=None):
    with patch.object(sys, "argv", ["export", *arguments]), contextlib.redirect_stdout(io.StringIO()):
        if fetch:
            with patch.object(exporter.urllib.request, "urlopen", fetch):
                exporter.main()
        else:
            exporter.main()


def expect_error(action):
    try:
        action()
    except (ValueError, FileNotFoundError, FileExistsError):
        return
    raise AssertionError("Expected unsafe/incomplete export to fail")


def check_linz():
    exporter = module("export-linz-addresses")
    with tempfile.TemporaryDirectory() as directory:
        args = ["--out", directory, "--seed", "test"]
        run_main(exporter, args, linz_response)
        path = Path(directory) / "linz.json"
        first = path.read_bytes()
        assert json.loads(first)["population"] == 2
        assert {r["attributes"]["address_id"] for r in json.loads(first)["features"]} == {1, 500001}
        run_main(exporter, args, linz_response)
        assert path.read_bytes() == first
        def changed(request, **kwargs):
            response = json.load(linz_response(request))
            if "editingInfo" in response:
                response["editingInfo"]["lastEditDate"] = 2
            return io.BytesIO(json.dumps(response).encode())
        expect_error(lambda: run_main(exporter, args, changed))
        assert path.read_bytes() == first
    with tempfile.TemporaryDirectory() as directory:
        def truncated(request, **kwargs):
            response = json.load(linz_response(request))
            if "objectIds" in response:
                response["exceededTransferLimit"] = True
            return io.BytesIO(json.dumps(response).encode())
        expect_error(lambda: run_main(exporter, ["--out", directory, "--seed", "test"], truncated))
        assert not (Path(directory) / "linz.json").exists()


def check_bogota():
    exporter = module("export-bogota-plates")
    with tempfile.TemporaryDirectory() as directory:
        for residue in exporter.RESIDUES:
            page = {"spatialReference": {"wkid": 4326}, "features": [{"attributes": {"OBJECTID": residue, "PDOTIPO": 1, "PDOCODIGO": str(residue)}}]}
            (Path(directory) / f"bogota-{residue}.json").write_text(json.dumps(page))
        args = ["--out", directory, "--offline"]
        run_main(exporter, args)
        path = Path(directory) / "bogota.json"
        first = path.read_bytes()
        run_main(exporter, args)
        assert path.read_bytes() == first
        page["exceededTransferLimit"] = True
        (Path(directory) / f"bogota-{residue}.json").write_text(json.dumps(page))
        expect_error(lambda: run_main(exporter, args))
        assert path.read_bytes() == first


def check_onemap():
    exporter = module("export-onemap-postcodes")
    assert exporter.draw("test", 3) == exporter.draw("test", 5)[:3] != exporter.draw("other", 3)
    assert all("010000" <= code <= "829999" for code in exporter.draw("test", 50))

    def response(request, **kwargs):
        assert request.headers["Authorization"] == "token"
        params = urllib.parse.parse_qs(urllib.parse.urlsplit(request.full_url).query)
        page = int(params["pageNum"][0])
        hit = {"POSTAL": params["searchVal"][0], "BLK_NO": str(page)}
        return io.BytesIO(json.dumps({"found": 2, "totalNumPages": 2, "pageNum": page, "results": [hit]}).encode())

    with tempfile.TemporaryDirectory() as directory:
        args = ["--out", directory, "--seed", "test", "--probes", "2", "--pause", "0"]
        expect_error(lambda: run_main(exporter, args, response))
        with patch.dict(exporter.os.environ, {"ONEMAP_TOKEN": "token"}):
            run_main(exporter, args, response)
        path = Path(directory) / "onemap.json"
        first = path.read_bytes()
        probes = json.loads(first)["probes"]
        assert [p["postal"] for p in probes] == sorted(exporter.draw("test", 2))
        assert [r["BLK_NO"] for r in probes[0]["results"]] == ["1", "2"]
        assert b"token" not in first
        run_main(exporter, args)
        assert path.read_bytes() == first
        (Path(directory) / "responses" / f"{probes[0]['postal']}-2.json").write_text('{"found":2,"totalNumPages":2,"pageNum":2,"results":[]}')
        expect_error(lambda: run_main(exporter, args))
        assert path.read_bytes() == first
    with tempfile.TemporaryDirectory() as directory:
        def unauthorised(request, **kwargs):
            return io.BytesIO(b'{"error":"Authentication token missing.","found":0,"totalNumPages":0,"pageNum":1,"results":[]}')
        with patch.dict(exporter.os.environ, {"ONEMAP_TOKEN": "token"}):
            expect_error(lambda: run_main(exporter, ["--out", directory, "--seed", "test", "--probes", "1"], unauthorised))
        assert not list(Path(directory).rglob("*.json"))


def check_bkk():
    exporter = module("bkk-metro-jsonl")
    with tempfile.TemporaryDirectory() as directory:
        source, out = Path(directory) / "source.zip", Path(directory) / "out.jsonl"
        files = {
            "routes.txt": "route_id,route_short_name,route_type\nm,M1,1\nb,Bus,3\n",
            "trips.txt": "trip_id,route_id\nmt,m\nbt,b\n",
            "stops.txt": "stop_id,stop_name,stop_lat,stop_lon,location_type,parent_station\nS,Állomás,47.5,19.06,1,\nP,Platform,47.5,19.06,0,S\nB,Bus station,47.5,19.06,1,\nBP,Bus platform,47.5,19.06,0,B\n",
            "stop_times.txt": "trip_id,stop_id\nmt,P\nbt,BP\n",
            "feed_info.txt": "feed_version,feed_start_date,feed_end_date\ntest,20261031,20261201\n",
        }
        def archive():
            with ZipFile(source, "w") as z:
                for name, data in files.items():
                    z.writestr(name, data)
        archive()
        exporter.prepare(source, out)
        first = out.read_bytes()
        records = [json.loads(line) for line in first.splitlines()]
        assert len(records) == 1 and records[0]["record"]["stop_id"] == "S"
        assert records[0]["metro_lines"] == ["M1"] and records[0]["point"] == [47.5, 19.06]
        exporter.prepare(source, out)
        assert first == out.read_bytes()
        original = files["stops.txt"]
        files["stops.txt"] += "S,Duplicate,47.5,19.06,1,\n"
        archive()
        expect_error(lambda: exporter.prepare(source, out))
        assert first == out.read_bytes()
        files["stops.txt"] = original.replace("47.5,19.06", "19.06,47.5")
        archive()
        expect_error(lambda: exporter.prepare(source, out))
        assert first == out.read_bytes()


if __name__ == "__main__":
    check_bkk()
    check_linz()
    check_bogota()
    check_onemap()
    print("Registry exporter checks passed")
