#!/usr/bin/env python3
import csv
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile


def module(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name))
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


nar = module("export-nar-addresses.py")
prg = module("export-prg-addresses.py")


def guid(n):
    return f"{n:08x}-0000-4000-8000-000000000000"


def csv_data(fields, rows):
    stream = io.StringIO(newline="")
    writer = csv.DictWriter(stream, fields)
    writer.writeheader()
    writer.writerows(rows)
    return stream.getvalue()


def nar_zip(path, reverse=False):
    with zipfile.ZipFile(path, "w") as archive:
        for n, (code, abbr) in enumerate(sorted(nar.PROVINCES.items()), 1):
            location = dict(zip(nar.LOCATION_FIELDS, [guid(n), code + "00001", "45.42", "-75.7"]))
            address = dict.fromkeys(nar.FIELDS, "")
            address.update(LOC_GUID=guid(n), ADDR_GUID=guid(100 + n), CIVIC_NO="12", OFFICIAL_STREET_NAME="Example", OFFICIAL_STREET_TYPE="RD", PROV_CODE=code, CSD_ENG_NAME="Exampleville", CSD_FRE_NAME="Exempleville", MAIL_STREET_NAME="EXAMPLE", MAIL_STREET_TYPE="RD", MAIL_MUN_NAME="EXAMPLEVILLE", MAIL_PROV_ABVN=abbr, MAIL_POSTAL_CODE="K1A0B1")
            addresses = [address]
            if code == "35":
                addresses.append(dict(address, ADDR_GUID=guid(200), APT_NO_LABEL="7"))
            if reverse:
                addresses.reverse()
            archive.writestr(f"Locations/Location_{code}.csv", csv_data(nar.LOCATION_FIELDS, [location]))
            archive.writestr(f"Addresses/Address_{code}.csv", csv_data(nar.FIELDS, addresses))


def identity(n):
    return f"<p:idIIP><p:AD_IdentyfikatorIIP><p:lokalnyId>{guid(n)}</p:lokalnyId><p:przestrzenNazw>PL.PZGIK.200</p:przestrzenNazw><p:wersjaId>2020-01-01T00:00:00</p:wersjaId></p:AD_IdentyfikatorIIP></p:idIIP><p:poczatekWersjiObiektu>2020-01-01T00:00:00</p:poczatekWersjiObiektu>"


def prg_xml(reverse=False):
    city = f'<p:AD_Miejscowosc g:id="city">{identity(1)}<p:nazwa>Wieś Próbna</p:nazwa><p:rodzaj>01</p:rodzaj><p:identyfikatorSIMC>0000001</p:identyfikatorSIMC><p:TERYTGminy>2600001</p:TERYTGminy></p:AD_Miejscowosc>'
    street = f'<p:AD_UlicaPlac g:id="street">{identity(2)}<p:nazwaPelna>Próbna</p:nazwaPelna><p:rodzaj>1</p:rodzaj><p:TERYTNazwa1>Próbna</p:TERYTNazwa1><p:miejsce x:href="#city"/></p:AD_UlicaPlac>'
    point = f'<p:AD_PunktAdresowy g:id="point">{identity(3)}<p:numerPorzadkowy>12A/14</p:numerPorzadkowy><p:georeferencja><g:Point srsName="urn:ogc:def:crs:EPSG::2180" srsDimension="2"><g:pos>350000 625000</g:pos></g:Point></p:georeferencja><p:kodPocztowy>26-000</p:kodPocztowy><p:dataNadania>2020-01-01</p:dataNadania><p:miejscowosc x:href="#city"/><p:ulica2 x:href="#street"/></p:AD_PunktAdresowy>'
    features = [point, city, street]
    if reverse:
        features.reverse()
    return ('<g:FeatureCollection xmlns:p="https://geoportal.gov.pl/schemas/prgad/1.0" xmlns:g="http://www.opengis.net/gml/3.2" xmlns:x="http://www.w3.org/1999/xlink">' + ''.join(f'<g:featureMember>{f}</g:featureMember>' for f in features) + '</g:FeatureCollection>').encode()


class Exports(unittest.TestCase):
    def test_nar_building_join_prefers_published_unit_and_is_order_independent(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "source.zip"
            nar_zip(path)
            with zipfile.ZipFile(path) as archive:
                first = nar.prepare(archive, "test", 1)
            self.assertEqual(len(first["addresses"]), 13)
            chosen = next(row for row in first["addresses"] if row["address"]["PROV_CODE"] == "35")
            self.assertEqual(chosen["address"]["APT_NO_LABEL"], "7")
            self.assertEqual(chosen["location"]["BG_LATITUDE"], "45.42")
            nar_zip(path, reverse=True)
            with zipfile.ZipFile(path) as archive:
                self.assertEqual(nar.prepare(archive, "test", 1), first)

    def test_nar_heap_selects_lowest_hash_independent_of_stream_order(self):
        locations = [("35", dict(zip(nar.LOCATION_FIELDS, [guid(n), "3500001", "45.42", "-75.7"]))) for n in range(1, 8)]
        addresses = []
        for n in range(1, 8):
            row = dict.fromkeys(nar.FIELDS, "")
            row.update(LOC_GUID=guid(n), ADDR_GUID=guid(100 + n), PROV_CODE="35", MAIL_PROV_ABVN="ON", CIVIC_NO="12", MAIL_STREET_NAME="EXAMPLE", MAIL_MUN_NAME="EXAMPLEVILLE", MAIL_POSTAL_CODE="K1A0B1")
            addresses.append(("35", row))
        def rows(archive, kind, fields, province):
            return iter(locations if kind == "Location" else addresses)
        with patch.object(nar, "rows", side_effect=rows):
            first = nar.prepare(None, "test", 2)
            locations.reverse()
            addresses.reverse()
            self.assertEqual(first, nar.prepare(None, "test", 2))
        expected = sorted((guid(n) for n in range(1, 8)), key=lambda value: hashlib.sha256(f"test\0{value}".encode()).digest())[:2]
        self.assertEqual(sorted(expected), [row["location"]["LOC_GUID"] for row in first["addresses"]])

    def test_nar_rejects_unterminated_quotes_even_in_unused_columns(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "source.zip"
            nar_zip(path)
            with zipfile.ZipFile(path) as archive:
                contents = {name: archive.read(name) for name in archive.namelist()}
            header, row, *_ = contents["Addresses/Address_35.csv"].decode().splitlines()
            contents["Addresses/Address_35.csv"] = (header + ',ignored\n' + row + ',"unfinished\n').encode()
            with zipfile.ZipFile(path, "w") as archive:
                for name, body in contents.items():
                    archive.writestr(name, body)
            with zipfile.ZipFile(path) as archive, self.assertRaises(csv.Error):
                nar.prepare(archive, "test", 1)

    def test_nar_rejects_missing_bg_points_instead_of_using_blockfaces(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "source.zip"
            with zipfile.ZipFile(path, "w") as archive:
                for n, code in enumerate(nar.PROVINCES, 1):
                    fields = nar.LOCATION_FIELDS + ["BF_REPPOINT_LATITUDE", "BF_REPPOINT_LONGITUDE"]
                    row = dict(zip(fields, [guid(n), code + "00001", "", "", "45.42", "-75.7"]))
                    archive.writestr(f"Locations/Location_{code}.csv", csv_data(fields, [row]))
                    archive.writestr(f"Addresses/Address_{code}.csv", csv_data(nar.FIELDS, []))
            with zipfile.ZipFile(path) as archive, self.assertRaisesRegex(ValueError, "no eligible"):
                nar.prepare(archive, "test", 1)

    def test_nar_receipts_replay_final_immutability_and_metadata_minimization(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.zip"
            out = Path(directory) / "nar.json"
            nar_zip(source)
            receipt = {"url": nar.URL, "bytes": source.stat().st_size, "sha256": hashlib.sha256(source.read_bytes()).hexdigest(), "last_modified": None, "authentication": "fixture-only"}
            source.with_suffix(".zip.receipt.json").write_text(json.dumps(receipt))
            nar.capture(source, out, "test", 1)
            first = out.read_bytes()
            modified = out.stat().st_mtime_ns
            nar.capture(source, out, "test", 1)
            self.assertEqual(out.stat().st_mtime_ns, modified)
            self.assertEqual(out.read_bytes(), first)
            self.assertNotIn(b"authentication", first)
            out.write_bytes(b"immutable")
            with self.assertRaisesRegex(ValueError, "overwrite"):
                nar.capture(source, out, "test", 1)
            receipt["sha256"] = "0" * 64
            source.with_suffix(".zip.receipt.json").write_text(json.dumps(receipt))
            with self.assertRaisesRegex(ValueError, "receipt mismatch"):
                nar.capture(source, out, "test", 1)

    def test_prg_references_axis_order_and_feature_order(self):
        first = prg.prepare_member(io.BytesIO(prg_xml()), "26", "2026-10-19", "test", 5)
        second = prg.prepare_member(io.BytesIO(prg_xml(True)), "26", "2026-10-19", "test", 5)
        self.assertEqual(first, second)
        point = first["addresses"][0]
        self.assertEqual((point["east"], point["north"]), (625000, 350000))
        self.assertEqual(point["number"], "12A/14")
        self.assertEqual(point["street"]["name"], "Próbna")
        self.assertEqual(point["locality"]["municipality"], "2600001")
        for old, new in [(b'urn:ogc:def:crs:EPSG::2180', b'EPSG:4326'), (b'#city', b'https://example.invalid/city')]:
            with self.assertRaises(ValueError):
                prg.prepare_member(io.BytesIO(prg_xml().replace(old, new)), "26", "2026-10-19", "test", 5)

    def test_prg_closed_future_and_unresolved_records_are_not_backfilled(self):
        for old, new in [(b'<p:numerPorzadkowy>', b'<p:koniecWersjiObiektu>2021-01-01T00:00:00</p:koniecWersjiObiektu><p:numerPorzadkowy>'), (b'<p:dataNadania>2020-01-01', b'<p:dataNadania>2027-01-01'), (b'<p:miejscowosc x:href="#city"', b'<p:miejscowosc x:href="#missing"'), ('<p:nazwa>Wieś Próbna</p:nazwa>'.encode(), b'<p:nazwa/>') ]:
            value = prg.prepare_member(io.BytesIO(prg_xml().replace(old, new)), "26", "2026-10-19", "test", 5)
            self.assertEqual(value["addresses"], [])

    def test_prg_projection_uses_east_north_without_extension_downloads(self):
        with patch.object(prg.subprocess, "run", return_value=SimpleNamespace(stdout="id,lat,lon\nexample,51,20.7\n")) as run:
            points = prg.project([{"id": "example", "east": 625000, "north": 350000}])
        self.assertEqual(points, {"example": [51, 20.7]})
        sql = run.call_args.args[0][-1]
        self.assertIn("ST_Point(east, north)", sql)
        self.assertIn("always_xy := true", sql)
        self.assertIn("autoinstall_known_extensions=false", sql)
        self.assertIn("autoload_known_extensions=false", sql)

    def test_prg_checkpoint_integrity_and_final_replay(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.zip"
            out = Path(directory) / "prg.json"
            with zipfile.ZipFile(source, "w") as archive:
                for code in sorted(prg.PROVINCES):
                    archive.writestr(f"{code}_example_19.10.2026_12.00.00.gml", "fixture")
            source.with_suffix(".zip.receipt.json").write_text(json.dumps({"url": prg.URL, "bytes": source.stat().st_size, "sha256": hashlib.sha256(source.read_bytes()).hexdigest()}))
            def prepare(stream, province, snapshot, seed, size):
                return {"counts": {"candidate_points": 1}, "addresses": [{"id": guid(int(province)), "east": 625000, "north": 350000}]}
            def project(rows):
                return {row["id"]: [51, 20.7] for row in rows}
            with patch.object(prg, "prepare_member", side_effect=prepare), patch.object(prg, "project", side_effect=project), patch.object(prg.subprocess, "run", return_value=SimpleNamespace(stdout="fixture-version")):
                prg.capture(source, out, "test", 1)
                first = out.read_bytes()
                prg.capture(source, out, "test", 1)
                self.assertEqual(first, out.read_bytes())
                checkpoint = out.with_suffix(".work") / "02.json"
                value = json.loads(checkpoint.read_bytes())
                value["data"]["addresses"][0]["east"] = 0
                checkpoint.write_bytes(prg.encode(value))
                with self.assertRaisesRegex(ValueError, "checkpoint checksum"):
                    prg.capture(source, out, "test", 1)


if __name__ == "__main__":
    unittest.main()
