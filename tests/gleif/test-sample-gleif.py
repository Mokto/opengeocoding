import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("gleif", Path(__file__).with_name("sample-gleif.py"))
gleif = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gleif)

A = gleif.A


def row(first, *more, city="Valletta", number="", postcode=""):
    lines = dict(zip(["AdditionalAddressLine.1", "AdditionalAddressLine.2", "AdditionalAddressLine.3"], more))
    return {A + "FirstAddressLine": first, A + "AddressNumber": number, A + "AddressNumberWithinBuilding": "",
            **{A + k: lines.get(k, "") for k in ("AdditionalAddressLine.1", "AdditionalAddressLine.2", "AdditionalAddressLine.3")},
            A + "City": city, A + "PostalCode": postcode}


def record(lei, city, *lines, postcode="", entities=3):
    return {"lei": lei, "lines": list(lines), "city": city, "postcode": postcode, "entities": entities}


class Gleif(unittest.TestCase):
    def test_care_of_names_are_dropped_and_separate_numbers_are_not_placed(self):
        self.assertEqual(gleif.address(row("C/O Agent Ltd", "Walker House", "87 Mary Street", postcode=" KY1-9005 ")),
                         (["Walker House", "87 Mary Street"], "Valletta", "KY1-9005"))
        self.assertEqual(gleif.address(row("c/o Agent Ltd, 9 Bd Albert 1er", "9 BD ALBERT 1ER"))[0], ["9 Bd Albert 1er"])
        self.assertIsNone(gleif.address(row("Main Street", number="12")))
        self.assertIsNone(gleif.address(row("C/O Agent Ltd")))
        self.assertIsNone(gleif.address(row("Main Street", city="")))

    def test_names_fold_case_accents_and_punctuation(self):
        self.assertEqual(gleif.fold("ST. JULIAN'S"), gleif.fold("St Julians"))
        self.assertEqual(gleif.fold("Ħamrun"), "hamrun")
        def name(value):
            return {"value": value, "xml:lang": "en"}
        names = gleif.settlements([{"item": {"value": "x/Q1"}, "name": name("Sliema"), "pop": {"value": "20000"}},
                                   {"item": {"value": "x/Q1"}, "name": name("Tas-Sliema")},
                                   {"item": {"value": "x/Q1"}, "name": name("SLIEMA")}])
        self.assertEqual(names, ({"sliema": {"Q1": 20000.0}, "tas sliema": {"Q1": None}},
                                 {"sliema": {("Sliema", "en"), ("SLIEMA", "en")}, "tas sliema": {("Tas-Sliema", "en")}}))

    def test_a_namesake_far_from_the_matched_settlement_makes_the_name_ambiguous(self):
        def item(qid, name, lat, lon):
            return {"item": {"value": f"x/{qid}"}, "name": {"value": name}, "point": {"value": f"Point({lon} {lat})"}}
        rows = [item("Q1", "București", 47.53, 25.98), item("Q2", "București", 44.43, 26.10),
                item("Q3", "Sliema", 35.91, 14.50), item("Q4", "Sliema", 35.912, 14.502)]
        self.assertEqual(gleif.far_names(rows, {"bucuresti": "Q1", "sliema": "Q3"}), {"bucuresti": {"Q2"}})
        self.assertEqual(gleif.literal('Ta\' "X"', "mt"), '"Ta\' \\"X\\""@mt')

    def test_shared_addresses_in_one_named_town_spread_over_towns(self):
        names = {"valletta": {"Q1": 6000.0}, "sliema": {"Q2": 2e6}, "twin": {"Q3": 1e4, "Q4": 1e4}, "hamlet": {"Q5": None}}
        records = {str(i): record(f"L{i}", "Valletta", f"{i} Republic Street", postcode="VLT 1117") for i in range(5)}
        records |= {"s": record("LS", "Sliema", "PO Box 9", "Tower Road", postcode="SLM 1"),
                    "t": record("LT", "Twin", "1 Road"), "x": record("LX", "Hamlet", "2 Road"),
                    "h": record("LH", "Sliema", "Flat 2", entities=2)}
        unmatched, seeds = gleif.select(records, names, "Malta", 4, 3, "s", "mt")
        self.assertEqual(unmatched, 2)
        self.assertEqual(len(seeds), 4)
        self.assertEqual(sorted(s["qid"] for s in seeds), ["Q1", "Q1", "Q1", "Q2"])
        sliema = next(s for s in seeds if s["qid"] == "Q2")
        self.assertEqual(sliema["queries"], ["PO Box 9, Tower Road, Sliema SLM 1, Malta"])
        self.assertEqual((sliema["radius_km"], sliema["tags"], "postcode" in sliema), (20, ["gleif", "mt", "po_box"], False))
        valletta = next(s for s in seeds if s["qid"] == "Q1")
        self.assertEqual((valletta["postcode"], valletta["good_match"], valletta["level"]), ("VLT 1117", None, "locality"))
        other = {"b": record("LB", "Sliema", "55 Main Street", "Valletta", postcode="SLM 2")}
        self.assertEqual(gleif.select(other, names, "Malta", 4, 3, "s", "mt"), (1, []))
        _, again = gleif.select(dict(reversed(list(records.items()))), names, "Malta", 4, 3, "s", "mt")
        self.assertEqual(seeds, again)

    def test_territories_accept_their_sovereign_and_postcodes_lose_a_prefix_their_country_does_not_write(self):
        names = {"road town": {"Q1": 9000.0}}
        _, [seed] = gleif.select({"a": record("L", "Road Town", "1 Main Street")}, names, "BVI", 5, 5, "s", "vg")
        self.assertEqual(seed["country"], ["gb", "vg"])
        _, [seed] = gleif.select({"a": record("L", "Kausala", "Haltonintie 1", postcode="FI-47400")}, {"kausala": {"Q1": 5000.0}}, "Finland", 5, 5, "s", "fi")
        self.assertEqual((seed["postcode"], seed["queries"]), ("47400", ["Haltonintie 1, Kausala FI-47400, Finland"]))
        _, [seed] = gleif.select({"a": record("L", "Saint Helier", "Castle Street", postcode="JE2 3BY")}, {"saint helier": {"Q1": 3e4}}, "Jersey", 5, 5, "s", "je")
        self.assertEqual(seed["postcode"], "JE2 3BY")
        _, [seed] = gleif.select({"a": record("L", "Riga", "Elizabetes iela 10", postcode="LV-1010")}, {"riga": {"Q1": 6e5}}, "Latvia", 5, 5, "s", "lv")
        self.assertEqual(seed["postcode"], "LV-1010")


if __name__ == "__main__":
    unittest.main()
