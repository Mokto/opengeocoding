import argparse
import copy
import hashlib
import json
from pathlib import Path

parser = argparse.ArgumentParser(description="Training-only Korean input-form diagnostics; not a new gold corpus")
parser.add_argument("--out", type=Path, required=True)
args = parser.parse_args()
source = Path(__file__).with_name("kr.jsonl")
if args.out.resolve() == source.resolve():
    parser.error("--out must not replace the source corpus")

short = {
    "서울특별시": "서울", "부산광역시": "부산", "대구광역시": "대구",
    "인천광역시": "인천", "광주광역시": "광주", "대전광역시": "대전",
    "울산광역시": "울산", "세종특별자치시": "세종", "경기도": "경기",
    "강원특별자치도": "강원", "충청북도": "충북", "충청남도": "충남",
    "전북특별자치도": "전북", "전라북도": "전북", "전라남도": "전남",
    "경상북도": "경북", "경상남도": "경남", "제주특별자치도": "제주",
}
rows = []
locations = set()
missing = {}
for line in source.read_text().splitlines():
    case = json.loads(line)
    if case["holdout"] or case["cohort"] not in {"kr_road", "kr_lot"}:
        continue
    locations.add(case["location"])
    native, country = case["query"].rsplit(", ", 1)
    assert country == "South Korea"
    province, rest = native.split(" ", 1)
    forms = {
        "control": case["query"],
        "domestic": native,
        "country_first": f"{country}, {native}",
        "without_province": f"{rest}, {country}",
    }
    if province in short:
        abbreviated = f"{short[province]} {rest}"
        forms.update(short_province=f"{abbreviated}, {country}", short_domestic=abbreviated)
    else:
        missing[province] = missing.get(province, 0) + 1
    for form, query in forms.items():
        row = copy.deepcopy(case)
        row.update(id=f"kr_format_{form}-{case['id']}", cohort=f"kr_format_{form}", query=query)
        row["review"]["note"] = "Diagnostic input-form transformation, not a source-published query; original geography retained."
        if form == "without_province":
            row.pop("expect")
        else:
            assert row["expect"] == case["expect"]
        rows.append(row)

data = "".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in rows).encode()
args.out.write_bytes(data)
manifest = {
    "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    "sha256": hashlib.sha256(data).hexdigest(),
    "queries": len(rows),
    "training_locations": len(locations),
    "short_form_not_generated": missing,
    "without_province": "unscored: removed context can introduce namesake ambiguity",
    "unmeasured": ["source postcodes unavailable", "published romanized road strings unavailable"],
}
args.out.with_suffix(".manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
print(json.dumps(manifest, ensure_ascii=False))
