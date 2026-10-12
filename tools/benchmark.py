import argparse
import concurrent.futures
import http.client
import http.server
import json
import math
import queue
import socket
import statistics
import threading
import time
import urllib.parse
from collections import Counter
from contextlib import ExitStack

SCENARIOS = [("raw", 1), ("raw", 10), ("raw", 100), ("localized", 1), ("fallback", 1)]
QUERIES = {
    "raw": "Copenhagen, Denmark",
    "localized": "München, Deutschland",
    "fallback": "Paris, France, 75001",
}
COUNTRIES = {"raw": "dk", "localized": "de", "fallback": "fr"}


def hits(scenario, count, local=False, postcode="75001"):
    country = COUNTRIES[scenario]
    city = {
        "raw": "Copenhagen",
        "localized": "München" if local else "Munich",
        "fallback": "Paris",
    }[scenario]
    address = {
        "country_code": country,
        "country": {"dk": "Denmark", "de": "Germany", "fr": "France"}[country],
        "city": city,
        "ISO3166-2-lvl4": {"dk": "DK-84", "de": "DE-BY", "fr": "FR-IDF"}[country],
    }
    if scenario == "fallback":
        address["postcode"] = postcode
    point = {
        "dk": (55.6761, 12.5683),
        "de": (48.1372, 11.5756),
        "fr": (48.8566, 2.3522),
    }[country]
    return json.dumps(
        [
            {
                "display_name": city,
                "address": address,
                "lat": str(point[0]),
                "lon": str(point[1]),
                "osm_id": i + 1,
                "osm_type": "node",
            }
            for i in range(count)
        ],
        ensure_ascii=False,
    ).encode()


class Mock(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def setup(self):
        super().setup()
        # Split headers/body otherwise introduce delayed-ACK latency into a loopback benchmark.
        self.connection.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)

    def log_message(self, *_):
        pass

    def respond(self, data, status=200):
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        if self.path != "/__control":
            self.respond(b"{}", 404)
            return
        try:
            config = json.loads(
                self.rfile.read(int(self.headers.get("Content-Length", "0")))
            )
            scenario, count, delay = (
                config["scenario"],
                int(config["candidates"]),
                float(config["delay_ms"]),
            )
            if (
                scenario not in QUERIES
                or not 1 <= count <= 1000
                or not 0 <= delay <= 1000
            ):
                raise ValueError("invalid fixture settings")
            payloads = {
                "english": hits(scenario, count),
                "local": hits(scenario, count, local=True),
                "wrong": hits(scenario, count, postcode="75004"),
            }
        except (KeyError, ValueError, TypeError):
            self.respond(b'{"error":"invalid fixture settings"}', 400)
            return
        with self.server.lock:
            self.server.config = scenario, delay, payloads
            self.server.counts.clear()
        self.respond(b"{}")

    def do_GET(self):
        path = urllib.parse.urlsplit(self.path)
        if path.path == "/__stats":
            with self.server.lock:
                data = json.dumps(dict(self.server.counts)).encode()
            self.respond(data)
            return
        if path.path.rstrip("/").split("/")[-1] != "search":
            self.respond(b"{}", 404)
            return
        query = urllib.parse.parse_qs(path.query).get("q", [""])[0]
        with self.server.lock:
            scenario, delay, payloads = self.server.config
            local = self.headers.get("Accept-Language", "") != "en"
            key = "local" if local else "english"
            if scenario == "fallback" and not query.casefold().startswith("75001"):
                key = "wrong"
            self.server.counts[key] += 1
            data = payloads[key]
        if delay:
            time.sleep(delay / 1000)
        self.respond(data)


def connection(url):
    parsed = urllib.parse.urlsplit(url)
    if (
        parsed.scheme != "http"
        or not parsed.hostname
        or parsed.username
        or parsed.query
        or parsed.fragment
    ):
        raise ValueError(
            "benchmark endpoints must be plain HTTP URLs without credentials, queries or fragments"
        )
    return http.client.HTTPConnection(
        parsed.hostname, parsed.port, timeout=30
    ), parsed.path.rstrip("/")


def request(conn, method, path, body=None):
    conn.request(
        method,
        path,
        body=json.dumps(body).encode() if body is not None else None,
        headers={"Content-Type": "application/json", "Connect-Protocol-Version": "1"},
    )
    response = conn.getresponse()
    data = response.read()
    if response.status != 200:
        raise RuntimeError(f"HTTP {response.status}: {data[:200]!r}")
    return json.loads(data)


def percentile(values, q):
    return sorted(values)[max(0, math.ceil(len(values) * q) - 1)]


def validate(result, scenario):
    details = result.get("addressDetails", {})
    assert details.get("countryCode") == COUNTRIES[scenario], result
    assert details.get("regions"), result
    assert bool(result.get("goodMatch")) == (scenario != "fallback"), result
    if scenario == "fallback":
        assert details.get("postcode") == "75001", result
    else:
        assert (
            details.get("city")
            == {"raw": "Copenhagen", "localized": "Munich"}[scenario]
        ), result


def main():
    parser = argparse.ArgumentParser(
        description="Mock Nominatim and compare warmed local Connect endpoints"
    )
    parser.add_argument("--serve", type=int, metavar="PORT")
    parser.add_argument("--service", action="append", default=[], metavar="NAME=URL")
    parser.add_argument("--mock-url", default="http://127.0.0.1:18080")
    parser.add_argument("--iterations", type=int, default=100)
    parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--concurrency", default="1,8")
    parser.add_argument("--delays", default="0,10")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        assert percentile([1, 2, 3, 4], 0.5) == 2
        assert len(json.loads(hits("raw", 100))) == 100
        assert (
            json.loads(hits("localized", 1, local=True))[0]["address"]["city"]
            == "München"
        )
        assert (
            json.loads(hits("fallback", 1, postcode="75004"))[0]["address"]["postcode"]
            == "75004"
        )
        validate(
            {
                "goodMatch": True,
                "addressDetails": {
                    "countryCode": "dk",
                    "city": "Copenhagen",
                    "regions": [{}],
                },
            },
            "raw",
        )
        try:
            validate({}, "raw")
        except AssertionError:
            pass
        else:
            raise AssertionError("empty geocoding result passed validation")
        print("Mock payloads, result validation and percentile checks passed")
        return
    if args.serve is not None:
        with http.server.ThreadingHTTPServer(("127.0.0.1", args.serve), Mock) as server:
            server.daemon_threads = True
            server.lock = threading.Lock()
            server.counts = Counter()
            server.config = (
                "raw",
                0,
                {
                    "english": hits("raw", 1),
                    "local": hits("raw", 1),
                    "wrong": hits("raw", 1),
                },
            )
            print(f"Mock Nominatim listening on {server.server_address}", flush=True)
            server.serve_forever()
        return
    if not args.service or args.iterations < 1 or args.rounds < 1:
        parser.error("provide services and positive iterations/rounds")
    services = [value.split("=", 1) for value in args.service]
    if any(len(value) != 2 or not value[0] for value in services) or len(
        {value[0] for value in services}
    ) != len(services):
        parser.error("services must have distinct names and NAME=URL syntax")
    concurrency = [int(value) for value in args.concurrency.split(",")]
    delays = [float(value) for value in args.delays.split(",")]
    if any(value < 1 for value in concurrency) or any(
        not 0 <= value <= 1000 for value in delays
    ):
        parser.error("invalid concurrency or mock delay")
    control, prefix = connection(args.mock_url)
    try:
        for delay in delays:
            for scenario, candidates in SCENARIOS:
                request(
                    control,
                    "POST",
                    prefix + "/__control",
                    {"scenario": scenario, "candidates": candidates, "delay_ms": delay},
                )
                for workers in concurrency:
                    with ExitStack() as stack:
                        clients = {}
                        for name, url in services:
                            pool = queue.SimpleQueue()
                            for _ in range(workers):
                                conn, base = connection(url)
                                stack.callback(conn.close)
                                pool.put((conn, base))
                            clients[name] = pool
                        executor = stack.enter_context(
                            concurrent.futures.ThreadPoolExecutor(max_workers=workers)
                        )

                        def call(name):
                            pool = clients[name]
                            conn, base = pool.get()
                            try:
                                start = time.perf_counter()
                                result = request(
                                    conn,
                                    "POST",
                                    base
                                    + "/nominatim_geolocation.NominatimGeolocation/SearchAddress",
                                    {"addressString": QUERIES[scenario]},
                                )
                                elapsed = (time.perf_counter() - start) * 1000
                                validate(result, scenario)
                                return elapsed, result
                            finally:
                                pool.put((conn, base))

                        expected = None
                        for round_number in range(args.rounds):
                            offset = round_number % len(services)
                            for name, _ in services[offset:] + services[:offset]:
                                # Alternating services can exceed an ASGI idle timeout; reconnect and warm outside timing.
                                idle = [clients[name].get() for _ in range(workers)]
                                for conn, base in idle:
                                    conn.close()
                                    clients[name].put((conn, base))
                                for _, result in executor.map(
                                    call, [name] * max(10, workers)
                                ):
                                    if expected is None:
                                        expected = result
                                    assert result == expected, (name, result, expected)
                                before = request(control, "GET", prefix + "/__stats")
                                start = time.perf_counter()
                                values = []
                                for elapsed, result in executor.map(
                                    call, [name] * args.iterations
                                ):
                                    assert result == expected, (name, result, expected)
                                    values.append(elapsed)
                                seconds = time.perf_counter() - start
                                after = request(control, "GET", prefix + "/__stats")
                                counts = {
                                    key: after.get(key, 0) - before.get(key, 0)
                                    for key in after.keys() | before.keys()
                                }
                                expected_counts = {
                                    "raw": 1,
                                    "localized": 2,
                                    "fallback": 3,
                                }[scenario] * args.iterations
                                assert sum(counts.values()) == expected_counts, (
                                    scenario,
                                    counts,
                                    expected_counts,
                                )
                                print(
                                    json.dumps(
                                        {
                                            "service": name,
                                            "scenario": scenario,
                                            "candidates": candidates,
                                            "delay_ms": delay,
                                            "concurrency": workers,
                                            "round": round_number,
                                            "calls": len(values),
                                            "p50_ms": percentile(values, 0.5),
                                            "p95_ms": percentile(values, 0.95),
                                            "mean_ms": statistics.mean(values),
                                            "rps": len(values) / seconds,
                                            "mock_calls": counts,
                                        }
                                    ),
                                    flush=True,
                                )
    finally:
        control.close()


if __name__ == "__main__":
    main()
