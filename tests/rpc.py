import ast
import concurrent.futures
import http.server
import json
import os
import shutil
import signal
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CH = os.getenv("CLICKHOUSE_URL", "http://127.0.0.1:8125")
DATABASE = f"geolocation_rpc_test_{os.getpid()}"
state = {"failures": 0, "requests": [], "mode": "normal"}
image = os.getenv("SERVICE_IMAGE")
process = None


def clickhouse(query):
    import base64

    request = urllib.request.Request(
        CH + "?" + urllib.parse.urlencode({"query": query}), data=b""
    )
    user = os.getenv("CLICKHOUSE_USER", "default")
    password = os.getenv("CLICKHOUSE_PASSWORD", "")
    request.add_header(
        "Authorization",
        "Basic " + base64.b64encode(f"{user}:{password}".encode()).decode(),
    )
    return urllib.request.urlopen(request, timeout=30).read()


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        query = urllib.parse.parse_qs(urllib.parse.urlsplit(self.path).query)
        language = self.headers.get("Accept-Language", "")
        state["requests"].append((query, language))
        if state["failures"]:
            state["failures"] -= 1
            self.send_error(503)
            return
        if state["mode"] == "wrong_postcode":
            address = {
                "country": "United States",
                "country_code": "us",
                "city": "Paris",
                "postcode": "75460",
            }
        elif state["mode"] == "local":
            address = {
                "country": "Germany",
                "country_code": "de",
                "city": "Munich" if language else "München",
            }
        else:
            address = {
                "country": "Denmark",
                "country_code": "dk",
                "city": "Copenhagen",
                "ISO3166-2-lvl4": "DK-84",
            }
        value = {
            "display_name": address["city"],
            "address": address,
            "lat": "55.676098",
            "lon": "12.568337",
            "osm_id": 1,
            "osm_type": "node",
        }
        result = value if self.path.startswith("/reverse") else [value]
        data = json.dumps(result).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def port():
    import socket

    with socket.socket() as socket:
        socket.bind(("127.0.0.1", 0))
        return socket.getsockname()[1]


http = http.server.ThreadingHTTPServer(
    ("0.0.0.0" if image else "127.0.0.1", 0), Handler
)
threading.Thread(target=http.serve_forever, daemon=True).start()
rpc_port, metrics_port = port(), port()


def rpc(method, payload, proto=False):
    if proto:
        data = payload
    else:
        data = json.dumps(payload).encode()
    request = urllib.request.Request(
        f"http://127.0.0.1:{rpc_port}/nominatim_geolocation.NominatimGeolocation/{method}",
        data=data,
        headers={
            "Content-Type": "application/proto" if proto else "application/json",
            "Connect-Protocol-Version": "1",
            "x-organization-id": "rpc-test",
        },
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read() if proto else json.load(response)


def search(value):
    return rpc("SearchAddress", {"addressString": value})


try:
    clickhouse(f"CREATE DATABASE {DATABASE}")
    clickhouse(f"""CREATE TABLE {DATABASE}.administrative_levels (
        country_code String, country_name String, level UInt8, local_name String,
        en_name String, admin_code String, updated_at Nullable(DateTime('UTC')), parent_admin_code String
    ) ENGINE = ReplacingMergeTree ORDER BY (country_code, admin_code)""")
    clickhouse(
        f"INSERT INTO {DATABASE}.administrative_levels VALUES ('dk','Denmark',4,'Hovedstaden','Capital Region','84',NULL,'')"
    )
    with tempfile.TemporaryDirectory() as temp:
        map_path = Path(temp) / "regions.json"
        map_path.write_text("{}")
        env = {
            **os.environ,
            "NOMINATIM_HOST": f"http://127.0.0.1:{http.server_port}",
            "CLICKHOUSE_URL": CH,
            "CLICKHOUSE_USER": os.getenv("CLICKHOUSE_USER", "default"),
            "CLICKHOUSE_PASSWORD": os.getenv("CLICKHOUSE_PASSWORD", ""),
            "CLICKHOUSE_DATABASE": DATABASE,
            "CLICKHOUSE_PREFIX": "",
            "CLICKHOUSE_SINGLE_NODE": "1",
            "REGION_MAP_PATH": str(map_path),
            "RPC_PORT": str(rpc_port),
            "METRICS_PORT": str(metrics_port),
            "SENTRY_DSN": "",
        }
        if image:
            os.chmod(temp, 0o777)
            os.chmod(map_path, 0o666)
            env["NOMINATIM_HOST"] = env["NOMINATIM_HOST"].replace(
                "127.0.0.1", "host.docker.internal"
            )
            env["CLICKHOUSE_URL"] = CH.replace("127.0.0.1", "host.docker.internal")
            env["REGION_MAP_PATH"] = "/region-map/regions.json"
            command = [
                "docker",
                "run",
                "--rm",
                "--add-host=host.docker.internal:host-gateway",
                "-p",
                f"127.0.0.1:{rpc_port}:{rpc_port}",
                "-p",
                f"127.0.0.1:{metrics_port}:{metrics_port}",
                "-v",
                f"{temp}:/region-map",
            ]
            if path := os.getenv("LIBPOSTAL_MODELS"):
                command.extend(["-v", f"{Path(path).resolve()}:/data/libpostal:ro"])
            for key, value in env.items():
                if key.startswith(("CLICKHOUSE_", "NOMINATIM_")) or key in (
                    "RPC_PORT",
                    "METRICS_PORT",
                    "REGION_MAP_PATH",
                    "SENTRY_DSN",
                ):
                    command.extend(["-e", f"{key}={value}"])
            command.append(image)
        else:
            command = [
                os.getenv("SERVICE_BINARY", str(ROOT / "target/debug/geolocation"))
            ]
        process = subprocess.Popen(command, env=env)
        for _ in range(100):
            if process.poll() is not None:
                raise AssertionError(f"Service exited: {process.returncode}")
            try:
                assert (
                    urllib.request.urlopen(
                        f"http://127.0.0.1:{rpc_port}/healthz", timeout=1
                    ).read()
                    == b"OK"
                )
                break
            except (OSError, urllib.error.URLError):
                time.sleep(0.2)
        else:
            raise AssertionError("Service did not become ready")

        found = search("Copenhagen, Denmark")
        assert found["addressDetails"]["countryCode"] == "dk", found
        assert found["addressDetails"]["regions"][0]["adminCode"] == "84", found
        query = b"Copenhagen, Denmark"
        binary = rpc("SearchAddress", b"\x0a" + bytes([len(query)]) + query, proto=True)
        assert b"Copenhagen" in binary and b"Hovedstaden" in binary, binary
        print(
            "PASS: Connect JSON and protobuf search with region enrichment", flush=True
        )

        state["mode"] = "local"
        state["requests"] = []
        found = search("München, Deutschland")
        assert found["addressDetails"]["city"] == "Munich", found
        assert [language for _, language in state["requests"]] == ["en", ""], state[
            "requests"
        ]
        state["mode"] = "wrong_postcode"
        assert search("Paris, France, 75001") == {}
        state["mode"] = "normal"
        print(
            "PASS: local-language corroboration and postcode/country rejection",
            flush=True,
        )

        regions = rpc("ResolveRegion", {"lat": 55.676098, "lon": 12.568337})
        assert regions["regions"][0]["adminCode"] == "84", regions
        assert rpc("RefreshRegionMap", {}) == {}
        assert "Hovedstaden" in rpc("GetRegionMap", {})["regionMap"]
        assert json.loads(map_path.read_text()) == ast.literal_eval(
            rpc("GetRegionMap", {})["regionMap"]
        )
        print("PASS: reverse lookup and atomic region-map refresh", flush=True)

        state["failures"] = 1
        assert search("Copenhagen, Denmark")["addressDetails"]["countryCode"] == "dk"
        assert state["failures"] == 0
        for method, data in [
            ("SearchAddress", {"addressString": "bad\u0000address"}),
            ("ResolveRegion", {"lat": 100, "lon": 0}),
        ]:
            try:
                rpc(method, data)
                raise AssertionError("Invalid input accepted")
            except urllib.error.HTTPError as error:
                assert error.code == 400
                assert json.load(error)["code"] == "invalid_argument"
        with concurrent.futures.ThreadPoolExecutor(max_workers=16) as executor:
            for result in executor.map(search, ["Copenhagen, Denmark"] * 64):
                assert result["addressDetails"]["countryCode"] == "dk"
        print(
            "PASS: retry, invalid-input errors and 64 concurrent requests", flush=True
        )

        if shutil.which("grpcurl"):
            address = f"127.0.0.1:{rpc_port}"
            services = subprocess.check_output(
                ["grpcurl", "-plaintext", "-max-time", "5", address, "list"],
                text=True,
                timeout=10,
            )
            assert "nominatim_geolocation.NominatimGeolocation" in services
            found = json.loads(
                subprocess.check_output(
                    [
                        "grpcurl",
                        "-plaintext",
                        "-max-time",
                        "5",
                        "-d",
                        '{"addressString":"Copenhagen, Denmark"}',
                        address,
                        "nominatim_geolocation.NominatimGeolocation/SearchAddress",
                    ],
                    text=True,
                    timeout=10,
                )
            )
            assert found["addressDetails"]["countryCode"] == "dk"
            print("PASS: legacy gRPC and reflection", flush=True)
        else:
            print(
                "NOT RUN: legacy gRPC/reflection; install grpcurl to exercise these checks",
                flush=True,
            )

        metrics = (
            urllib.request.urlopen(f"http://127.0.0.1:{metrics_port}/metrics")
            .read()
            .decode()
        )
        assert "nominatim_geolocation_query_tier_total" in metrics
        assert (
            "grpc_requests_total" in metrics and 'organization_id="rpc-test"' in metrics
        )
        process.send_signal(signal.SIGTERM)
        assert process.wait(timeout=15) == 0
        print("PASS: metrics and SIGTERM shutdown", flush=True)
finally:
    if process is not None and process.poll() is None:
        process.terminate()
        process.wait(timeout=15)
    http.shutdown()
    clickhouse(f"DROP DATABASE IF EXISTS {DATABASE}")
