import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("towns", Path(__file__).with_name("sample-towns.py"))
towns = importlib.util.module_from_spec(spec)
spec.loader.exec_module(towns)


def row(qid, label, pop=None):
    r = {"item": {"value": f"http://www.wikidata.org/entity/{qid}"}, "label": {"value": label}}
    if pop is not None:
        r["pop"] = {"value": str(pop)}
    return r


class Towns(unittest.TestCase):
    def test_population_is_the_largest_of_repeated_rows(self):
        found = towns.settlements([row("Q1", "Big", 10), row("Q1", "Big", 30), row("Q1", "Big"), row("Q2", "Small")])
        self.assertEqual(found, {"Q1": ("Big", 30.0), "Q2": ("Small", None)})

    def test_ambiguous_taken_and_country_names_are_not_eligible(self):
        found = {"Q1": ("Twin", 5), "Q2": ("twin", 9), "Q3": ("Kept", 1), "Q4": ("Taken", 2), "Q5": ("Honduras", 3), "Q6": ("A, B", 4), "Q7": ("Typed", 1)}
        eligible, seeds = towns.select(found, {"Q4"}, {"typed, ruritania"}, "Ruritania", 10, "s", {"honduras"})
        self.assertEqual([e[0] for e in eligible], ["Q3"])
        self.assertEqual(seeds[0]["queries"], ["Kept, Ruritania"])

    def test_half_by_population_then_seeded_order(self):
        found = {f"Q{i}": (f"Town {i}", i * 1000 if i % 2 else None) for i in range(1, 21)}
        _, seeds = towns.select(found, set(), set(), "R", 6, "s")
        self.assertEqual([s["qid"] for s in seeds[:3]], ["Q19", "Q17", "Q15"])
        self.assertEqual(len({s["qid"] for s in seeds}), 6)
        _, again = towns.select(dict(reversed(list(found.items()))), set(), set(), "R", 6, "s")
        self.assertEqual(seeds, again)
        big = {"Q1": ("Big", 60000), "Q2": ("Small", 100)}
        self.assertEqual([s["radius_km"] for s in towns.select(big, set(), set(), "R", 2, "s")[1]], [10, 5])

    def test_a_mixed_territory_accepts_its_sovereign_for_every_town(self):
        _, seeds = towns.select({"Q1": ("Philipsburg", 1)}, set(), set(), "Sint Maarten", 5, "s", (), "sx")
        self.assertEqual(seeds[0]["country"], ["nl", "sx"])
        self.assertNotIn("country", towns.select({"Q1": ("Bujumbura", 1)}, set(), set(), "Burundi", 5, "s", (), "bi")[1][0])

    def test_a_town_filed_under_a_unit_abroad_or_another_country_strays(self):
        def stray(item, point, parent, unit=None, country="Q142"):
            r = {"area": {"value": "Q17070"}, "item": {"value": item}, "point": {"value": point},
                 "parent": {"value": parent}, "country": {"value": country}}
            if unit:
                r["unit"] = {"value": unit}
            return r
        own = {"area": {"value": "Q17070"}, "own": {"value": "Q142"}}
        rows = [own,
                # in France, under Réunion's arrondissement of Saint-Denis
                stray("Q34791557", "Point(-0.4 46.48)", "Q653514", "Point(55.45 -20.88)"),
                stray("Q1", "Point(55.5 -21.0)", "Q653514", "Point(55.45 -20.88)"),
                # the area's own point says nothing of where its towns are
                stray("Q2", "Point(-0.4 46.48)", "Q17070", "Point(55.45 -20.88)"),
                stray("Q3", "Point(55.5 -21.0)", "Q653514", country="Q766")]
        self.assertEqual(towns.strays(rows), {"Q34791557", "Q3"})
        self.assertEqual(towns.strays(rows, "aq"), {"Q34791557"})


if __name__ == "__main__":
    unittest.main()
