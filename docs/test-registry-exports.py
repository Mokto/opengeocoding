import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch
import urllib.parse


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


if __name__ == "__main__":
    check_linz()
    check_bogota()
    print("Registry exporter checks passed")
