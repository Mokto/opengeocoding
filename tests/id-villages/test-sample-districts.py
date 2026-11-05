import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("districts", Path(__file__).with_name("sample-districts.py"))
districts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(districts)

V = "http://www.wikidata.org/entity/"


def row(qid, code, label, kab="Kota Surabaya", prov="Jawa Timur", point="Point(112.75 -7.27)"):
    return {"item": {"value": V + qid}, "code": {"value": code}, "label": {"value": label}, "point": {"value": point},
            "kabLabel": {"value": kab}, "provLabel": {"value": prov}}


class Districts(unittest.TestCase):
    def test_one_chain_and_the_label_word_dropped(self):
        found = districts.read_districts({
            "Q1": [row("Q1", "3578090", "Kecamatan Gubeng")],
            "Q2": [row("Q2", "3578100", "Two"), row("Q2", "3578100", "Two", kab="Kabupaten Sidoarjo")],
        })
        self.assertEqual([(d["qid"], d["name"], d["kab"]) for d in found], [("Q1", "Gubeng", "Kota Surabaya")])

    def test_vetted_against_bps(self):
        d = lambda qid, code, name, lat=-7.0: {"qid": qid, "code": code, "name": name, "kab": "B", "lat": lat, "lon": 110.0}
        bps = {"ID3315030": ("Pepe", -7.0, 110.0, 100.0), "ID3315040": ("Other", -7.0, 110.0, 100.0),
               "ID3315050": ("Far", -7.0, 110.0, 1.0), "ID3315060": ("Twin", -7.0, 110.0, 1.0),
               "ID3315070": ("Twin", -7.0, 110.0, 1.0)}
        eligible, rejected = districts.vet([
            d("Q1", "3315030", "Pepe"), d("Q2", "3315040", "Renamed"), d("Q3", "3315050", "Far", lat=-7.5),
            d("Q4", "3315060", "Twin"), d("Q5", "3315070", "Twin"), d("Q6", "3315080", "Gone"),
        ], bps)
        self.assertEqual([(e["qid"], e["radius_km"]) for e in eligible], [("Q1", 11.3)])
        self.assertEqual(rejected, {"no_bps_district": 1, "bps_name_differs": 1, "far_from_bps": 1, "ambiguous": 2, "odd_label": 0})

    def test_written_as_addresses_write_them(self):
        d = {"name": "Gubeng", "kab": "Kota Surabaya", "prov": "Jawa Timur", "code": "3578090"}
        self.assertEqual(districts.queries(d), ["Gubeng, Kota Surabaya, Jawa Timur, Indonesia",
                                                "Kec. Gubeng, Kota Surabaya, Jawa Timur, Indonesia"])
        d = {"name": "Abenaho", "kab": "Kabupaten Yalimo", "prov": "Papua Pegunungan", "code": "9506010"}
        self.assertEqual(districts.queries(d)[1], "Distrik Abenaho, Kab. Yalimo, Papua Pegunungan, Indonesia")


if __name__ == "__main__":
    unittest.main()
