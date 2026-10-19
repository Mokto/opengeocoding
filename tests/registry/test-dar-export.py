#!/usr/bin/env python3
"""Offline DAR join/projection regression check; requires DuckDB CLI + spatial."""

import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location("dar_export", Path(__file__).with_name("export-dar-addresses.py"))
dar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dar)


def row(entity, identity, **values):
    value = {key: None for key in dar.COMMON + dar.COLUMNS[entity]}
    value.update(id_lokalId=identity, status="8" if entity == "Adressepunkt" else "3", registreringFra="2020-01-01T00:00:00Z", virkningFra="2020-01-01T00:00:00Z")
    value.update(values)
    return value


def source(directory, reverse=False):
    house = dict(adgangsadressebetegnelse="Eksempelvej 12A, Lilleby, 1000 Testby", husnummertekst="12A", adgangspunkt="point-1", navngivenVej="road-1", postnummer="postal-1", supplerendeBynavn="place-1", adgangTilBygning="building-1", geoDanmarkBygning="geo-1")
    point = dict(position="POINT (720000 6175000)", oprindelse_tekniskStandard="TD", oprindelse_nøjagtighedsklasse="A", oprindelse_kilde="Test fixture")
    unit = {"adressebetegnelse": "Eksempelvej 12A, 2. tv., Lilleby, 1000 Testby", "etagebetegnelse": "2", "dørbetegnelse": "tv", "husnummer": "house-a"}
    data = {
        "Husnummer": [row("Husnummer", "house-a", **house), row("Husnummer", "house-b", **dict(house, husnummertekst="14", adgangsadressebetegnelse="Eksempelvej 14, Lilleby, 1000 Testby")), row("Husnummer", "retired-house", **dict(house, status="4")), row("Husnummer", "wrong-point-status", **dict(house, adgangTilBygning=None, geoDanmarkBygning=None, adgangspunkt="point-wrong")), row("Husnummer", "not-building", **dict(house, adgangTilBygning=None, geoDanmarkBygning=None, adgangspunkt="point-facility")), row("Husnummer", "missing-postcode", **dict(house, postnummer="missing"))],
        "Adressepunkt": [row("Adressepunkt", "point-1", **point), row("Adressepunkt", "point-wrong", **dict(point, status="3")), row("Adressepunkt", "point-facility", **dict(point, oprindelse_tekniskStandard="TA"))],
        "Adresse": [row("Adresse", "unit-1", **unit), row("Adresse", "unit-2", **dict(unit, adressebetegnelse="Eksempelvej 12A, 3. tv., Lilleby, 1000 Testby", etagebetegnelse="3")), row("Adresse", "unit-0-retired", **dict(unit, status="4")), row("Adresse", "unit-0-future", **dict(unit, virkningFra="2027-01-01T00:00:00Z")), row("Adresse", "unit-0-registration-future", **dict(unit, registreringFra="2027-01-01T00:00:00Z")), row("Adresse", "unit-0-registration-ended", **dict(unit, registreringTil="2026-01-01T00:00:00Z")), row("Adresse", "unit-0-effect-ended", **dict(unit, virkningTil="2026-01-01T00:00:00Z"))],
        "NavngivenVej": [row("NavngivenVej", "road-1", vejnavn="Eksempelvej", vejadresseringsnavn="Eksempelvej", administreresAfKommune="0101")],
        "NavngivenVejKommunedel": [row("NavngivenVejKommunedel", "road-area-1", navngivenVej="road-1", kommune="0101", vejkode="0001")],
        "Postnummer": [row("Postnummer", "postal-1", navn="Testby", postnr="1000")],
        "SupplerendeBynavn": [row("SupplerendeBynavn", "place-1", navn="Lilleby")],
    }
    manifest = dict(register="DAR", version="4", type="Current", format="JSON", generation=7, point_in_time="2026-01-01T00:00:00Z", files=[])
    directory.mkdir()
    for entity, rows in data.items():
        name = f"DAR_V4_{entity}_TotalDownload_json_Current_7.zip"
        with zipfile.ZipFile(directory / name, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr(name.removesuffix(".zip") + ".json", json.dumps(rows[::-1] if reverse else rows, ensure_ascii=False))
        raw = (directory / name).read_bytes()
        manifest["files"].append(dict(entity=entity, filename=name, bytes=len(raw), sha256=hashlib.sha256(raw).hexdigest(), request={"apikey": "never-copy-authentication"}))
    (directory / "MANIFEST.json").write_text(json.dumps(manifest))


class Export(unittest.TestCase):
    def test_current_statuses_foreign_keys_axis_order_and_building_holdout(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source(root / "source")
            first = dar.capture(root / "source", root / "out", "test", 10)
            addresses = first["addresses"]
            self.assertEqual(len(addresses), 1)
            a = addresses[0]
            self.assertEqual(a["id"], "house-a")
            self.assertEqual(a["location"], "bbr:building-1")
            self.assertEqual((a["unit_id"], a["floor"], a["door"]), ("unit-1", "2", "tv"))
            self.assertEqual(a["point_status"], "8")
            self.assertEqual(a["point_id"], "point-1")
            self.assertEqual(a["supplementary"], "Lilleby")
            self.assertTrue(54 < a["lat"] < 58.5 and 7.5 < a["lon"] < 16)
            self.assertEqual(first["capture"]["selection_counts"]["eligible_entrances"], 2)
            self.assertEqual(first["capture"]["entity_counts"]["Adresse"]["current_rows"], 2)
            self.assertNotIn("never-copy-authentication", json.dumps(first))
            self.assertEqual(first, dar.capture(root / "source", root / "out", "test", 10))
            source(root / "reordered", reverse=True)
            second = dar.capture(root / "reordered", root / "other-out", "test", 10)
            self.assertEqual(first["addresses"], second["addresses"])
            final = root / "out" / "dar.json"
            final.write_bytes(b"different pinned capture")
            with self.assertRaisesRegex(ValueError, "overwrite"):
                dar.capture(root / "source", root / "out", "test", 10)
            manifest_path = root / "source" / "MANIFEST.json"
            original = manifest_path.read_bytes()
            manifest = json.loads(original)
            manifest["files"][0].update(entity="../unrecognized", filename="DAR_V4_../unrecognized_TotalDownload_json_Current_7.zip")
            manifest_path.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError, "snapshot filename"):
                dar.capture(root / "source", root / "third-out", "test", 10)
            manifest_path.write_bytes(original)
            archive = next((root / "source").glob("*.zip"))
            archive.write_bytes(b"invalid source")
            with self.assertRaisesRegex(ValueError, "checksum"):
                dar.capture(root / "source", root / "third-out", "test", 10)


if __name__ == "__main__":
    unittest.main()
