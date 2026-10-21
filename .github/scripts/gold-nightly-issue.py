#!/usr/bin/env python3
"""The nightly gold issue's body, from the gold report and the issue's previous body.

The report's by-evidence section has two tables: independent sources, kept to the countries with
at least --min SearchAddress cases of that kind, and agreement with OpenStreetMap, whose countries
all filled the panel's quota. The countries in neither are listed by name. The previous body carries the history as a JSON comment."""
import argparse
import json
import re

HISTORY = 14


def section(report):
    m = re.search(r"<!-- by-evidence -->\n(.*?)<!-- by-evidence end -->", report, re.S)
    if not m:
        raise SystemExit("the report has no by-evidence section: its evaluator predates it")
    tables, title, rows = {}, None, None
    for line in m.group(1).splitlines():
        t = re.match(r"<summary><b>(.*) \(\d+ countr(?:y|ies)\)</b></summary>", line)
        if t:
            title, rows = t.group(1), []
            tables[title] = rows
        elif rows is not None and line.startswith("|"):
            rows.append([c.strip() for c in line.strip("|").split("|")])
    return tables


def checked(cell):
    """(head passing, checked) of `18/20 · 90.0%` or `17→18/20 · …`; (0, 0) for `-`."""
    m = re.match(r"(?:\d+→)?(\d+)/(\d+)", cell)
    return (int(m.group(1)), int(m.group(2))) if m else (0, 0)


def filtered(rows, minimum):
    """Header, separator, the sum and the countries with at least `minimum` SearchAddress cases."""
    search = rows[0].index("SearchAddress")
    keep = rows[:3] + [r for r in rows[3:] if checked(r[search])[1] >= minimum]
    return "\n".join("| " + " | ".join(r) + " |" for r in keep), {r[0]: checked(r[search])[1] for r in rows[3:]}


def rate(cell):
    passing, total = checked(cell)
    return f"{100 * passing / total:.1f}% ({total})" if total else "-"


def rpc_table(report):
    """The report's RPC table as checked and passing: the nightly diffs a run with itself, so its
    base and head columns are equal."""
    out = ["| RPC | Checked | Passing |", "|---|---:|---:|"]
    for line in report.split("| RPC |", 1)[1].splitlines()[2:]:
        if not line.startswith("|"):
            break
        rpc, total, _, passing = [c.strip() for c in line.strip("|").split("|")][:4]
        out.append(f"| {rpc} | {total} | {passing} ({100 * int(passing) / int(total):.1f}%) |")
    return "\n".join(out)


def body(report, previous, names, head, run_url, server, date, minimum):
    tables = section(report)
    kinds = [("Independent sources", "independent"), ("Agreement with OpenStreetMap", "osm")]
    m = re.search(r"^<!-- history: (.*) -->$", previous, re.M)
    history = json.loads(m.group(1)) if m else []
    entry = {"date": date, "sha": head}
    for title, key in kinds:
        header, _, total = tables[title][:3]
        entry.update({f"{key}_{rpc}": dict(zip(header, total)).get(rpc, "-") for rpc in ["SearchAddress", "SuggestAddress"]})
    history = ([h for h in history if h.get("sha") != head] + [entry])[-HISTORY:]
    out = [
        "<!-- gold-nightly -->",
        f"<!-- head: {head} -->",
        f"<!-- history: {json.dumps(history, ensure_ascii=False)} -->",
        f"Updated {date} by the [nightly run]({run_url}) on `main` at [`{head[:7]}`]({server}/commit/{head}).",
        "",
        rpc_table(report),
        "",
        "Independent sources are registers, curated Wikidata places and Wikidata towns: they measure accuracy. OpenStreetMap cases check agreement with mapped addresses the index itself reads in many countries: consistency, not accuracy. Cells are passing checks / checked · rate.",
        "",
        "## History",
        "",
        "SearchAddress and SuggestAddress pass rates over all countries, with the checks counted; a jump in checks is new cases, not a change in quality.",
        "",
        "| Date | Commit | Independent search | Independent suggest | OSM search | OSM suggest |",
        "|---|---|---:|---:|---:|---:|",
    ]
    for h in reversed(history):
        rates = [rate(h.get(f"{key}_{rpc}", "-")) for _, key in kinds for rpc in ["SearchAddress", "SuggestAddress"]]
        out.append(f"| {h['date'][:10]} | [`{h['sha'][:7]}`]({server}/commit/{h['sha']}) | " + " | ".join(rates) + " |")
    table, sizes = filtered(tables["Independent sources"], minimum)
    out += ["", f"## Independent sources: countries with at least {minimum} cases", "", table]
    table, panel = filtered(tables["Agreement with OpenStreetMap"], 0)
    out += ["", "## Agreement with OpenStreetMap: every country of the address panel", "", table]
    independent, mapped = {}, {name for label in panel for name in label.split(", ")}
    for label, n in sizes.items():
        for name in label.split(", "):
            independent[name] = independent.get(name, 0) + n
    short = sorted(f"{n} ({independent.get(n, 0)})" for n in names if independent.get(n, 0) < minimum and n not in mapped)
    out += ["", "## Countries in neither table", "",
            f"Fewer than {minimum} independent SearchAddress cases, in brackets (counting those that expect a territory and its country together), and no address panel.", "",
            " · ".join(short)]
    return "\n".join(out) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", required=True)
    parser.add_argument("--previous", help="the issue's current body, if it exists")
    parser.add_argument("--reference", required=True, help="data/reference.json")
    parser.add_argument("--head", required=True)
    parser.add_argument("--run-url", required=True)
    parser.add_argument("--server", required=True)
    parser.add_argument("--date", required=True)
    parser.add_argument("--min", type=int, default=100)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    previous = open(args.previous).read() if args.previous else ""
    names = json.load(open(args.reference))["country_names"].values()
    text = body(open(args.report).read(), previous, names, args.head, args.run_url, args.server, args.date, args.min)
    open(args.out, "w").write(text)


if __name__ == "__main__":
    main()
