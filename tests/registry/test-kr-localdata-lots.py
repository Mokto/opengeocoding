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
                        writer.writerow(["영업상태코드", "지번주소", "좌표정보(X)", "좌표정보(Y)", "도로명주소"])
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
                writer.writerow(["영업상태코드", "지번주소", "좌표정보(X)", "좌표정보(Y)", "위생업태명", "도로명주소"])
                for i, category in enumerate(["학교", "병원", "산업체"], 1):
                    writer.writerow(["01", f"경기도 가상시 새동 {i}", str(200000+i), "500000", category])
            outputs = [SimpleNamespace(stdout="127.000783539 38.002746025"), SimpleNamespace(stdout="127.0 37.0\n127.0001 37.0\n127.0002 37.0\n")]
            out = root / "prepared.tsv.gz"
            with patch.object(localdata.subprocess, "run", side_effect=outputs), contextlib.redirect_stdout(io.StringIO()):
                localdata.prepare(source, hashlib.sha256(source.read_bytes()).hexdigest(), out, "2026-11-04")
            with localdata.gzip.open(out, "rt") as stream:
                rows = list(csv.DictReader(stream, delimiter="\t"))
            self.assertEqual({row["address"] for row in rows}, {f"경기도 가상시 새동 {i}" for i in range(1, 4)})

    def test_additional_registers_never_replace_meal_keys_and_share_quarantine(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            meals = root / "meals.csv"
            with meals.open("w", encoding="cp949", newline="") as stream:
                writer = csv.writer(stream)
                writer.writerow(["영업상태코드", "지번주소", "좌표정보(X)", "좌표정보(Y)", "도로명주소"])
                writer.writerow(["01", "경기도 가상시 새동 1", "200000", "500000"])
                writer.writerow(["01", "경기도 가상시 새동 2 (빌딩)", "200001", "500000"])
            shelters = root / "shelters.csv"
            with shelters.open("w", encoding="cp949", newline="") as stream:
                writer = csv.writer(stream)
                writer.writerow(["운영상태", "소재지전체주소", "경도(EPSG4326)", "위도(EPSG4326)", "도로명전체주소"])
                writer.writerow(["사용중", "경기도 가상시 새동 1", "127.0001", "37.0"])
                writer.writerow(["사용중", "경기도 다른시 가상구 새동 1", "127.0002", "37.0"])
                writer.writerow(["사용중", "경기도 가상시 새동 2", "127.0001", "37.0"])
                writer.writerow(["사용중지", "경기도 가상시 새동 3", "127.0002", "37.0"])
                writer.writerow(["사용중", "경기도 가상시 새동 4", "127.0003", "37.0"])
            zones = root / "zones.json"
            zones.write_text(localdata.json.dumps({"records": [
                {"LNMADR": "경기도 가상시 새동 4", "LONGITUDE": "127.01", "LATITUDE": "37.0", "RDNMADR": None},
                {"LNMADR": "경기도 가상시 새동 5", "LONGITUDE": "127.0004", "LATITUDE": "37.0", "RDNMADR": None},
                {"LNMADR": "경기도 가상시 새동 5", "LONGITUDE": "127.0004000000001", "LATITUDE": "37.0", "RDNMADR": None},
                {"LNMADR": None, "LONGITUDE": "127.0005", "LATITUDE": "37.0", "RDNMADR": None},
            ]}))
            sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
            outputs = [SimpleNamespace(stdout="127.000783539 38.002746025"), SimpleNamespace(stdout="127.0 37.0\n127.00001 37.0\n")]
            out = root / "prepared.tsv.gz"
            extra = [("shelters", shelters, sha(shelters)), ("protection_zones", zones, sha(zones))]
            with patch.object(localdata.subprocess, "run", side_effect=outputs), contextlib.redirect_stdout(io.StringIO()):
                localdata.prepare(meals, sha(meals), out, "2026-11-04", extra, "2026-11-05")
            with localdata.gzip.open(out, "rt") as stream:
                rows = [(r["address"].split()[-1], r["lon"]) for r in csv.DictReader(stream, delimiter="\t")]
            # 1 keeps the meal point; 2 is new; 3 is not in use; 4's two registers disagree by 870 m.
            self.assertEqual(rows, [("1", "127.000000000"), ("2", "127.000100000"), ("5", "127.000400000")])
            with self.assertRaises(ValueError):
                localdata.prepare(meals, sha(meals), out, "2026-11-04", [("shelters", shelters, "0" * 64)], "2026-11-05")

    def test_road_addresses_keep_the_building_and_meal_precedence(self):
        self.assertEqual(localdata.road("대구광역시 동구 메디밸리로8길 11, 102호 (사복동)"), "대구광역시 동구 메디밸리로8길 11")
        self.assertEqual(localdata.road("광주광역시 북구 서하로194번길 6(오치동)"), "광주광역시 북구 서하로194번길 6")
        for text in ["서울특별시 강남구 테헤란로 지하 152", "서울특별시 강남구 테헤란로 152 외", "서울특별시 강남구 테헤란로 0", "테헤란로 152", "경기도 가상시 새동 12", ""]:
            self.assertIsNone(localdata.road(text), text)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            meals = root / "meals.csv"
            with meals.open("w", encoding="cp949", newline="") as stream:
                writer = csv.writer(stream)
                writer.writerow(["영업상태코드", "지번주소", "좌표정보(X)", "좌표정보(Y)", "도로명주소"])
                writer.writerow(["01", "경기도 가상시 새동 1", "200000", "500000", "경기도 가상시 새로 1 (새동)"])
                writer.writerow(["01", "", "200001", "500000", "경기도 가상시 새로 2, 3층"])
            zones = root / "zones.json"
            zones.write_text(localdata.json.dumps({"records": [
                {"LNMADR": None, "RDNMADR": "경기도 다른시 새로 1", "LONGITUDE": "127.3", "LATITUDE": "37.0"},
                {"LNMADR": None, "RDNMADR": "경기도 가상시 새로 3(새동)", "LONGITUDE": "127.0003", "LATITUDE": "37.0"},
            ]}))
            sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
            outputs = [SimpleNamespace(stdout="127.000783539 38.002746025"), SimpleNamespace(stdout="127.0 37.0\n127.0 37.0\n127.0001 37.0\n")]
            out = root / "prepared.tsv.gz"
            with patch.object(localdata.subprocess, "run", side_effect=outputs), contextlib.redirect_stdout(io.StringIO()):
                localdata.prepare(meals, sha(meals), out, "2026-11-04", [("protection_zones", zones, sha(zones))], "2026-11-05")
            with localdata.gzip.open(out, "rt") as stream:
                rows = [(r["address"], r["lon"]) for r in csv.DictReader(stream, delimiter="\t")]
            # The lot and road of one facility share its point; another city's 새로 1 gives way to the meal register's.
            self.assertEqual(rows, [("경기도 가상시 새동 1", "127.000000000"), ("경기도 가상시 새로 1", "127.000000000"), ("경기도 가상시 새로 2", "127.000100000"), ("경기도 가상시 새로 3", "127.000300000")])

    def test_duplicates_and_annotations_cannot_weight_an_address(self):
        key = "경기도 가상시 새동 1"
        rows = [(key, 127, 37, True)] * 20 + [(key, 127, 37, False)]
        self.assertEqual(localdata.screen(rows), ([(key, 127, 37)], 0))
        self.assertEqual(localdata.screen([(key, 127, 37, False)]), ([], 0))


if __name__ == "__main__":
    unittest.main()
