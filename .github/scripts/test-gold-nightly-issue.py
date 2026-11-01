import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("issue", Path(__file__).with_name("gold-nightly-issue.py"))
issue = importlib.util.module_from_spec(spec)
spec.loader.exec_module(issue)

REPORT = """# Baseline vs candidate (10 shared cases)

| RPC | Checked | Pass base | Pass head | Change | Results changed |
|---|---:|---:|---:|---:|---:|
| SearchAddress | 330 | 305 | 305 | 0 | 0 |
| ReverseGeocode | 200 | 199 | 199 | 0 | 0 |

<details>
<summary><b>Runs</b></summary>
</details>

<!-- by-evidence -->
## Pass rate by country and evidence

<details>
<summary><b>Independent sources (3 countries)</b></summary>

| Country | Evidence | SearchAddress | SuggestAddress | ReverseGeocode | ResolveRegion |
|---|---|---:|---:|---:|---:|
| **All** | | 195→200/230 · 84.8%→87.0% | 10/20 · 50.0% | - | - |
| American Samoa, United States | towns | 40/50 · 80.0% | - | - | - |
| Denmark | places, registry | 150→155/170 · 88.2%→91.2% | 10/20 · 50.0% | - | - |
| Kenya | places | 5/10 · 50.0% | - | - | - |
</details>

<details>
<summary><b>Agreement with OpenStreetMap (2 countries)</b></summary>

| Country | SearchAddress | SuggestAddress | ReverseGeocode | ResolveRegion |
|---|---:|---:|---:|---:|
| **All** | 75/100 · 75.0% | - | - | - |
| Kenya | 75/100 · 75.0% | - | - | - |
| Qatar | 90/99 · 90.9% | - | - | - |
</details>
<!-- by-evidence end -->
"""


def build(previous="", head="b" * 40, date="2026-10-22 01:30 UTC"):
    return issue.body(REPORT, previous, ["American Samoa", "Denmark", "Kenya", "Qatar", "United States", "Vatican City"],
                      head, "https://run", "https://github.com/x/y", date, 100)


class Issue(unittest.TestCase):
    def test_each_table_keeps_its_sum_and_the_countries_with_enough_cases_of_its_kind(self):
        text = build()
        self.assertIn("Registers can also supply index data", text)
        self.assertNotIn("they measure accuracy", text)
        independent = text.split("## Independent sources: countries with at least 100 cases")[1].split("##")[0]
        self.assertIn("| **All** |  | 195→200/230 · 84.8%→87.0% |", independent)
        self.assertIn("| Denmark | places, registry |", independent)
        self.assertNotIn("Kenya", independent)
        osm = text.split("## Agreement with OpenStreetMap: every country of the address panel")[1].split("##")[0]
        self.assertIn("| Kenya | 75/100 · 75.0% |", osm)
        self.assertIn("| Qatar | 90/99 · 90.9% |", osm)

    def test_countries_in_neither_table_count_independent_and_shared_territory_cases(self):
        short = build().split("## Countries in neither table")[1]
        self.assertIn("American Samoa (50) · United States (50) · Vatican City (0)", short)
        self.assertNotIn("Kenya", short)
        self.assertNotIn("Qatar", short)
        self.assertNotIn("Denmark", short)

    def test_history_appends_the_head_once_newest_first_and_keeps_fourteen(self):
        first = build()
        self.assertIn("| 2026-10-22 | [`bbbbbbb`](https://github.com/x/y/commit/" + "b" * 40 + ") | 87.0% (230) | 50.0% (20) | 75.0% (100) | - |", first)
        again = build(first)
        self.assertEqual(again.count("| 2026-10-22 |"), 1)
        text = first
        for n in range(20):
            text = build(text, head=f"{n:040d}", date=f"2026-11-{n + 1:02d} 01:30 UTC")
        history = json.loads(text.split("<!-- history: ")[1].split(" -->")[0])
        self.assertEqual(len(history), 14)
        self.assertEqual(history[-1]["sha"], f"{19:040d}")
        self.assertLess(text.index("| 2026-11-20 |"), text.index("| 2026-11-19 |"))

    def test_the_rpc_table_shows_checked_and_passing_and_an_old_report_fails(self):
        text = build()
        self.assertIn("| RPC | Checked | Passing |\n|---|---:|---:|\n| SearchAddress | 330 | 305 (92.4%) |\n| ReverseGeocode | 200 | 199 (99.5%) |\n", text)
        self.assertNotIn("# Baseline vs candidate", text)
        with self.assertRaises(SystemExit):
            issue.section("no section here")


if __name__ == "__main__":
    unittest.main()
