import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("prepare", Path(__file__).with_name("prepare-id-admin-points.py"))
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)

UNITS = [("33", "Jawa Tengah"), ("33.01", "Kabupaten Foo"), ("33.02", "Kota Foo"),
         ("33.01.01", "Seroja"), ("33.02.01", "Seroja"), ("33.01.02", "Rimba"),
         ("33.01.02.2001", "Kenanga")]


def row(ident, level, name, admin, aliases=""):
    return [str(ident), name, name, aliases, "-7.2", "110.2", "A", f"ADM{level}", "ID", "", *admin,
            "0", "", "0", "Asia/Jakarta", "2026-10-01"]


def rows():
    return [row(1, 1, "Jawa Tengah", ["GN", "", "", ""]),
            row(2, 2, "Kabupaten Foo", ["GN", "COUNTY", "", ""]),
            row(3, 2, "Kota Foo", ["GN", "CITY", "", ""])]


class Crosswalk(unittest.TestCase):
    def test_typed_counties_and_unique_province_district(self):
        data = rows() + [row(4, 3, "Kecamatan Rimba", ["GN", "", "DISTRICT", ""])]
        mapped, _ = prepare.crosswalk(data, UNITS)
        self.assertEqual(mapped, {"1": "33", "2": "33.01", "3": "33.02", "4": "33.01.02"})

    def test_district_namesakes_need_county_evidence(self):
        for county, expected in [("", None), ("COUNTY", "33.01.01"), ("CITY", "33.02.01"), ("UNKNOWN", None)]:
            with self.subTest(county=county):
                mapped, _ = prepare.crosswalk(rows() + [row(4, 3, "Seroja", ["GN", county, "X", ""])], UNITS)
                self.assertEqual(mapped.get("4"), expected)

    def test_sparse_village_parents_are_not_inferred(self):
        data = rows() + [row(4, 3, "Rimba", ["GN", "COUNTY", "X", ""]),
                         row(5, 4, "Desa Kenanga", ["GN", "COUNTY", "X", "Y"]),
                         row(6, 4, "Desa Kenanga", ["GN", "COUNTY", "", "Z"])]
        mapped, _ = prepare.crosswalk(data, UNITS)
        self.assertEqual(mapped.get("5"), "33.01.02.2001")
        self.assertNotIn("6", mapped)

    def test_ambiguity_conflicts_and_duplicates_abstain(self):
        for extra in [
            [row(4, 3, "Rimba", ["OTHER", "", "X", ""])],
            [row(4, 3, "Rimba", ["GN", "CITY", "X", ""])],
            [row(4, 3, "Rimba", ["GN", "", "X", ""], "Seroja")],
            [row(4, 3, "Desa Rimba", ["GN", "", "X", ""], "Rimba")],
            [row(4, 3, "Rimba", ["GN", "", "X", ""]), row(5, 3, "Rimba", ["GN", "", "Y", ""])],
            [row(4, 3, "Rimba", ["GN", "", "X", ""]), row(5, 3, "Other", ["GN", "", "X", ""])],
        ]:
            with self.subTest(extra=extra):
                mapped, _ = prepare.crosswalk(rows() + extra, UNITS)
                self.assertNotIn("4", mapped)

    def test_foreign_letters_are_not_silently_dropped(self):
        self.assertIsNone(prepare.key("Rimba森林", 3))
        self.assertEqual(prepare.key("Kecamatan Rímbâ", 3), "rimba")
        self.assertNotEqual(prepare.key("Kota Foo", 2), prepare.key("Kabupaten Foo", 2))
        with self.assertRaises(ValueError):
            prepare.crosswalk(rows() + [rows()[0]], UNITS)


if __name__ == "__main__":
    unittest.main()
