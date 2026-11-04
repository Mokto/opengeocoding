import contextlib
import csv
import hashlib
import importlib.util
import io
import math
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
from pathlib import Path

spec = importlib.util.spec_from_file_location("localdata", Path(__file__).with_name("kr-localdata-lots.py"))
localdata = importlib.util.module_from_spec(spec)
spec.loader.exec_module(localdata)


class LocaldataLots(unittest.TestCase):
    def test_bundled_capture_matches_both_pinned_checksums(self):
        source = Path(__file__).resolve().parents[2] / "data" / "kr-localdata-lots.tsv.gz"
        metadata = localdata.json.loads(source.with_suffix("").with_suffix(".json").read_text())
        self.assertEqual(hashlib.sha256(source.read_bytes()).hexdigest(), metadata["gzip_sha256"])
        with localdata.gzip.open(source, "rb") as stream:
            self.assertEqual(hashlib.sha256(stream.read()).hexdigest(), metadata["tsv_sha256"])

    def test_only_bare_positive_native_lots_are_emitted(self):
        self.assertEqual(localdata.address("경기도 가상시 새동 산 8-2번지"), ("경기도 가상시 새동 산8-2", True))
        self.assertEqual(localdata.address("강원도 가상시 새동 9"), ("강원특별자치도 가상시 새동 9", True))
        for text in ["경기도 가상시 새동 0", "경기도 가상시 새동 01", "경기도 가상시 새동 1-0", "경기도 가상시 새동 1-2-3", "경기도 가상시 새동 산 산8", "경기도 가상시 새로 12", "경기도 가상시 새동 １２", "경기도 가상시 새동 123456", "경기도 가상시 새동 1-123456"]:
            self.assertIsNone(localdata.address(text), text)
        for text in ["경기도 가상시 새동 12 외 2필지", "경기도 가상시 새동 12, 2층", "경기도 가상시 새동 12 (빌딩)", "경기도 가상시 새동 3 층"]:
            parsed = localdata.address(text)
            self.assertIsNotNone(parsed)
            self.assertFalse(parsed[1], text)

    def test_quarantine_precedes_country_envelope_and_bare_filter(self):
        key = "경기도 가상시 새동 1"
        rows = [(key, 127, 37, True), (key, 10, 55, False)]
        self.assertEqual(localdata.screen(rows), ([], 1))
        self.assertEqual(localdata.screen([(key, 127, 37, True), (key, math.nan, math.nan, False)]), ([], 1))
        crowded = [(f"경기도 가상시 새동 {i}", 127, 37, i == 1) for i in range(1, 6)]
        self.assertEqual(localdata.screen(crowded), ([], 5))

    def test_raw_invalid_coordinates_quarantine_the_key_before_projection(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for bad_x in ["0", "nan", "inf", "bad", ""]:
                with self.subTest(raw_x=bad_x):
                    source = root / "capture.csv"
                    with source.open("w", encoding="cp949", newline="") as stream:
                        writer = csv.writer(stream)
                        writer.writerow(["영업상태코드", "지번주소", "좌표정보(X)", "좌표정보(Y)"])
                        writer.writerow(["01", "경기도 가상시 새동 1", "200000", "500000"])
                        writer.writerow(["01", "경기도 가상시 새동 1 (빌딩)", bad_x, "500000"])
                        writer.writerow(["01", "경기도 가상시 새동 2", "200001", "500001"])
                    outputs = [SimpleNamespace(stdout="127.000783539 38.002746025"), SimpleNamespace(stdout="127.0 37.0\n127.0001 37.0001\n")]
                    out = root / "prepared.tsv.gz"
                    with patch.object(localdata.subprocess, "run", side_effect=outputs), contextlib.redirect_stdout(io.StringIO()):
                        localdata.prepare(source, hashlib.sha256(source.read_bytes()).hexdigest(), out, "2026-11-04")
                    with localdata.gzip.open(out, "rt") as stream:
                        content = stream.read()
                    self.assertNotIn("새동 1\t", content)
                    self.assertIn("새동 2\t", content)

    def test_institution_category_does_not_select_rows(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "capture.csv"
            with source.open("w", encoding="cp949", newline="") as stream:
                writer = csv.writer(stream)
                writer.writerow(["영업상태코드", "지번주소", "좌표정보(X)", "좌표정보(Y)", "위생업태명"])
                for i, category in enumerate(["학교", "병원", "산업체"], 1):
                    writer.writerow(["01", f"경기도 가상시 새동 {i}", str(200000+i), "500000", category])
            outputs = [SimpleNamespace(stdout="127.000783539 38.002746025"), SimpleNamespace(stdout="127.0 37.0\n127.0001 37.0\n127.0002 37.0\n")]
            out = root / "prepared.tsv.gz"
            with patch.object(localdata.subprocess, "run", side_effect=outputs), contextlib.redirect_stdout(io.StringIO()):
                localdata.prepare(source, hashlib.sha256(source.read_bytes()).hexdigest(), out, "2026-11-04")
            with localdata.gzip.open(out, "rt") as stream:
                rows = list(csv.DictReader(stream, delimiter="\t"))
            self.assertEqual({row["lot_address"] for row in rows}, {f"경기도 가상시 새동 {i}" for i in range(1, 4)})

    def test_duplicates_and_annotations_cannot_weight_an_address(self):
        key = "경기도 가상시 새동 1"
        rows = [(key, 127, 37, True)] * 20 + [(key, 127, 37, False)]
        self.assertEqual(localdata.screen(rows), ([(key, 127, 37)], 0))
        self.assertEqual(localdata.screen([(key, 127, 37, False)]), ([], 0))


if __name__ == "__main__":
    unittest.main()
