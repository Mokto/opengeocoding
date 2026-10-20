import importlib.util
import json
from pathlib import Path
import random
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("addresses", Path(__file__).with_name("build-addresses.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
spec = importlib.util.spec_from_file_location("adjudicate", Path(__file__).with_name("adjudicate-gnaf.py"))
adjudicate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adjudicate)


def node(node_id=1, number="12A", lat="28.6", lon="77.2"):
    return module.record({"id": str(node_id), "version": "2", "timestamp": "2026-10-11T00:00:00Z", "lat": lat, "lon": lon, "user": "must-not-be-retained"},
                         {"addr:housenumber": number, "addr:street": "संसद मार्ग", "addr:city": "नई दिल्ली", "name": "must-not-be-retained", "phone": "must-not-be-retained", "addr:full": "must-not-be-retained"}, "IN", "India", "IND")


class Addresses(unittest.TestCase):
    def test_source_components_are_not_reinterpreted(self):
        for number in ["12A", "20-24", "2/19", "１０", "100-18107"]:
            r = node(number=number)
            self.assertEqual(r["tags"]["addr:housenumber"], number)
            rows = module.cases([r], {"source_sha256": "a" * 64})
            self.assertEqual(len(rows), 2)
            self.assertTrue(all(c["expect"]["components"]["house_number"] == number for c in rows))
            self.assertEqual(rows[0]["location"], rows[1]["location"])
            self.assertEqual(rows[0]["holdout"], rows[1]["holdout"])
            self.assertEqual(rows[0]["expect"]["point"], [28.6, 77.2])

    def test_no_personal_or_business_metadata_is_copied(self):
        text = module.encoded(node()).decode()
        self.assertNotIn("must-not-be-retained", text)
        self.assertNotIn('"user"', text)
        self.assertNotIn('"phone"', text)

    def test_invalid_coordinates_and_versions_are_rejected(self):
        for lat, lon in [("NaN", "2"), ("91", "2"), ("2", "181"), ("0", "0"), ("broken", "2")]:
            self.assertIsNone(node(lat=lat, lon=lon))
        self.assertIsNone(node(node_id=-1))

    def test_underspecified_and_contradictory_addresses_are_rejected(self):
        attrs = {"id": "1", "version": "1", "lat": "28.6", "lon": "77.2"}
        base = {"addr:housenumber": "12", "addr:street": "Main Road", "addr:city": "Delhi"}
        for drop in ["addr:housenumber", "addr:street", "addr:city"]:
            tags = base.copy()
            del tags[drop]
            self.assertIsNone(module.record(attrs, tags, "IN", "India", "IND"))
        for number in ["", "s/n", "x" * 30]:
            self.assertIsNone(module.record(attrs, {**base, "addr:housenumber": number}, "IN", "India", "IND"))
        self.assertIsNone(module.record(attrs, {**base, "addr:country": "US"}, "IN", "India", "IND"))
        self.assertIsNone(module.record(attrs, {**base, "addr:street": "Main\nRoad"}, "IN", "India", "IND"))

    def test_contact_details_plus_codes_and_placeholders_are_not_civic_addresses(self):
        for number in ["1234567890", "27Q8+343", "CV87+6J", "0905.555.010", "12;14"]:
            self.assertIsNone(node(number=number))
        attrs = {"id": "1", "version": "1", "lat": "28.6", "lon": "77.2"}
        for street in ["-", "unknown", "Main Road Phone: 123.555.010", "https://example.com", "name@example.com"]:
            tags = {"addr:housenumber": "12", "addr:street": street, "addr:city": "Delhi"}
            self.assertIsNone(module.record(attrs, tags, "IN", "India", "IND"))
        self.assertIsNotNone(node(number="12345-12349"))

    def test_place_number_and_postcode_are_preserved(self):
        attrs = {"id": "1", "version": "1", "lat": "28.6", "lon": "77.2"}
        tags = {"addr:housenumber": "2-8", "addr:place": "四丁目", "addr:postcode": "100-0001"}
        r = module.record(attrs, tags, "JP", "Japan", "JPN")
        self.assertEqual(module.cases([r], {"source_sha256": "b" * 64})[0]["expect"]["components"]["postcode"], "100-0001")

    def test_sampling_is_order_independent_and_geographically_grouped(self):
        records = [node(i + 1, str(i + 1), str(20 + (i % 150) / 4), str(70 + (i % 7))) for i in range(500)]
        a, b = module.Draw("fixed-seed", 50), module.Draw("fixed-seed", 50)
        for r in records:
            a.add(r)
        random.Random(42).shuffle(records)
        for r in records:
            b.add(r)
        self.assertEqual(a.selected(), b.selected())
        self.assertEqual(len(a.selected()), 50)
        self.assertEqual(a.eligible, 500)
        self.assertLessEqual(len(a.cells), 100)

    def test_duplicate_address_nodes_are_not_independent_locations(self):
        a = module.Draw("seed", 50)
        for i in range(10):
            a.add(node(i + 1))
        self.assertEqual(len(a.selected()), 1)

    def test_country_masks_combine_all_parts_instead_of_overwriting_mainland(self):
        mainland = [[[0, 0], [1, 0], [1, 1], [0, 0]]]
        island = [[[10, 10], [11, 10], [11, 11], [10, 10]]]
        data = {"features": [{"properties": {"ISO_A2_EH": "FR"}, "geometry": {"type": "MultiPolygon", "coordinates": [mainland]}}, {"properties": {"ISO_A2_EH": "FR"}, "geometry": {"type": "Polygon", "coordinates": island}}]}
        self.assertEqual(module.country_features(data)["FR"]["geometry"]["coordinates"], [mainland, island])

    def test_checked_in_cases_replay_from_source_records(self):
        root = Path(__file__).parent
        manifest = json.loads((root / "manifest.json").read_bytes())
        self.assertEqual(set(manifest["coverage"]), set(module.COUNTRIES))
        rows = []
        for snapshot in manifest["snapshots"]:
            self.assertEqual(snapshot["proof"]["script_sha256"], module.file_sha(Path(module.__file__)))
            rows.extend(module.cases(snapshot["records"], snapshot["proof"]))
        replayed = [json.dumps(r, ensure_ascii=False) + "\n" for r in rows]
        adjudications = json.loads((root / "adjudications.json").read_bytes())
        expected = adjudicate.apply(replayed, adjudications).encode()
        self.assertEqual((root / "global-addresses.jsonl").read_bytes(), expected)

    def test_an_adjudication_adds_the_registry_point_and_keeps_the_source_one(self):
        attrs = {"id": "7", "version": "3", "lat": "-30.0", "lon": "150.0"}
        r = module.record(attrs, {"addr:housenumber": "12", "addr:street": "Long Road", "addr:postcode": "2000"}, "AU", "Australia", "AUS")
        lines = [json.dumps(c, ensure_ascii=False) for c in module.cases([r], {"source_sha256": "c" * 64})]
        adjudications = {"role": "authoritative-alternative-point", "source": {"name": "G-NAF"}, "au": [{"node": 7, "version": 3, "pid": "GA1", "geocode_type": "PC", "point": [-30.01, 150.0], "distance_km": 1.112}]}
        for case in map(json.loads, adjudicate.apply(lines, adjudications).splitlines()):
            osm, gnaf = case["expect"]["any_of"]
            self.assertEqual(osm["point"], [-30.0, 150.0])
            self.assertEqual(gnaf, {**osm, "point": [-30.01, 150.0]})
            self.assertEqual(case["adjudication"]["ref"], "GA1")
        with self.assertRaises(ValueError):
            adjudicate.apply(lines, {**adjudications, "au": [{**adjudications["au"][0], "version": 2}]})

    def test_completed_outputs_are_immutable(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory) / "out.json"
            module.immutable(p, b"same")
            before = p.stat().st_mtime_ns
            module.immutable(p, b"same")
            self.assertEqual(p.stat().st_mtime_ns, before)
            with self.assertRaises(ValueError):
                module.immutable(p, b"different")


if __name__ == "__main__":
    unittest.main()
