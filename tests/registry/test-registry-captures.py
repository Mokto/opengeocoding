#!/usr/bin/env python3
import importlib.util
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.parse


def module(filename):
    spec = importlib.util.spec_from_file_location(filename, Path(__file__).with_name(filename))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


gnaf = module("export-gnaf-addresses.py")
cartociudad = module("export-cartociudad-addresses.py")


class Captures(unittest.TestCase):
    def test_gnaf_hash_draw_bounds_uniqueness_and_seed(self):
        first = gnaf.draw_ids("test", 10, 20, 7)
        self.assertEqual(first, gnaf.draw_ids("test", 10, 20, 7))
        self.assertEqual(len(set(first)), 7)
        self.assertTrue(all(10 <= n <= 20 for n in first))
        self.assertNotEqual(first, gnaf.draw_ids("other", 10, 20, 7))
        self.assertEqual(gnaf.draw_ids("test", 10, 12, 3), [10, 11, 12])
        for size in [0, -1, 12]:
            with self.assertRaises(ValueError):
                gnaf.draw_ids("test", 10, 20, size)

    def test_gnaf_capture_accepts_id_gaps_but_not_truncation_or_overwrites(self):
        def response(request, **kwargs):
            url = request if isinstance(request, str) else request.full_url
            if "/query" not in url:
                return io.BytesIO(b'{"editingInfo":{"lastEditDate":1}}')
            params = urllib.parse.parse_qs(request.data.decode())
            if "orderByFields" in params:
                value = {"features": [{"attributes": {"OBJECTID": 1 if params["orderByFields"] == ["OBJECTID ASC"] else 3}}]}
            elif "returnCountOnly" in params:
                value = {"count": 2}
            else:
                value = {"spatialReference": {"wkid": 4326}, "features": [{"attributes": {"OBJECTID": i}} for i in [1, 3]]}
            return io.BytesIO(json.dumps(value).encode())

        with tempfile.TemporaryDirectory() as directory, patch("urllib.request.urlopen", side_effect=response):
            out = Path(directory)
            gnaf.capture(out, "test", 3)
            snapshot = out / "gnaf.json"
            data = snapshot.read_bytes()
            self.assertEqual([f["attributes"]["OBJECTID"] for f in json.loads(data)["features"]], [1, 3])
            gnaf.capture(out, "test", 3)
            self.assertEqual(snapshot.read_bytes(), data)
            snapshot.write_bytes(b"different pinned capture")
            with self.assertRaisesRegex(ValueError, "overwrite"):
                gnaf.capture(out, "test", 3)
            self.assertEqual(snapshot.read_bytes(), b"different pinned capture")
            page = out / "page-00000.json"
            value = json.loads(page.read_bytes())
            value["features"].pop()
            page.write_bytes(json.dumps(value).encode())
            with self.assertRaisesRegex(ValueError, "truncated"):
                gnaf.capture(out, "test", 3)

    def test_span_limited_province_windows(self):
        self.assertEqual(cartociudad.window_offsets("s", "p", 5, 128), [0])
        offsets = cartociudad.window_offsets("s", "p", 1000, 128)
        self.assertEqual(offsets, cartociudad.window_offsets("s", "p", 1000, 128))
        self.assertEqual(len(set(offsets)), 3)
        self.assertTrue(all(0 <= n <= 872 for n in offsets))
        with self.assertRaises(ValueError):
            cartociudad.window_offsets("s", "p", 0, 128)

    def test_province_pages_fail_closed(self):
        row = {"id": "1", "properties": {"component_AdminUnitName_3": "A Coruña"}, "geometry": {"type": "Point"}}
        page = {"numberMatched": 10, "numberReturned": 1, "features": [row]}
        self.assertEqual(cartociudad.validate_page(page, "A Coruña", 10, 1), [row])
        for field, value in [("numberMatched", 11), ("numberReturned", 0), ("features", [])]:
            with self.assertRaises(ValueError):
                cartociudad.validate_page(dict(page, **{field: value}), "A Coruña", 10, 1)
        with self.assertRaises(ValueError):
            cartociudad.validate_page(page, "Madrid", 10, 1)
        row["properties"]["component_PostalDescriptor"] = "15001"
        with self.assertRaises(ValueError):
            cartociudad.validate_page(page, "A Coruña", 10, 1)
        del row["properties"]["component_PostalDescriptor"]
        row["geometry"]["type"] = "LineString"
        with self.assertRaises(ValueError):
            cartociudad.validate_page(page, "A Coruña", 10, 1)
        row["geometry"]["type"] = "Point"
        page.update(numberReturned=2, features=[row, row])
        with self.assertRaises(ValueError):
            cartociudad.validate_page(page, "A Coruña", 10, 2)


if __name__ == "__main__":
    unittest.main()
