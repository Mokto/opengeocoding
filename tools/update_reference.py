import hashlib
import json
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CLDR_COMMIT = "91c267402229a59e3ef2774544f001bf959e8809"


def download(url):
    request = urllib.request.Request(
        url, headers={"User-Agent": "geolocation-reference-builder"}
    )
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read()
    return data


def clean(value):
    return " ".join(
        "".join(c for c in value.lower() if c.isalpha() or c in "-/ ").split()
    )


countries, sources = {}, []
url = "https://download.geonames.org/export/dump/countryInfo.txt"
data = download(url)
sources.append(
    {"url": url, "sha256": hashlib.sha256(data).hexdigest(), "license": "CC-BY-4.0"}
)
valid = set()
for line in data.decode().splitlines():
    if not line or line.startswith("#"):
        continue
    fields = line.split("\t")
    valid.add(fields[0].lower())
    for name in (fields[0], fields[1], fields[4]):
        countries[clean(name)] = fields[0].lower()
for language in (
    "en",
    "de",
    "fr",
    "es",
    "da",
    "pt",
    "it",
    "nl",
    "pl",
    "ru",
    "el",
    "tr",
    "ar",
    "zh",
    "ja",
    "ko",
):
    url = f"https://raw.githubusercontent.com/unicode-org/cldr-json/{CLDR_COMMIT}/cldr-json/cldr-localenames-full/main/{language}/territories.json"
    data = download(url)
    sources.append(
        {
            "url": url,
            "sha256": hashlib.sha256(data).hexdigest(),
            "license": "Unicode-3.0",
        }
    )
    names = json.loads(data)["main"][language]["localeDisplayNames"]["territories"]
    for code, name in names.items():
        code = code.split("-")[0]
        if (
            len(code) == 2
            and code.isascii()
            and code.isalpha()
            and code.lower() in valid
        ):
            countries[clean(name)] = code.lower()
            countries[code.lower()] = code.lower()
    print(f"Loaded public country names: {language}", flush=True)
url = "https://download.geonames.org/export/dump/admin1CodesASCII.txt"
data = download(url)
sources.append(
    {"url": url, "sha256": hashlib.sha256(data).hexdigest(), "license": "CC-BY-4.0"}
)
owners = {}
for line in data.decode().splitlines():
    code, name, ascii_name, _ = line.split("\t")
    for value in (name, ascii_name):
        owners.setdefault(value, set()).add(code.split(".")[0].lower())
regions = {name: next(iter(codes)) for name, codes in owners.items() if len(codes) == 1}
ranges, printable = [], []
for code in range(sys.maxunicode + 1):
    for predicate, target in ((str.isalpha, ranges), (str.isprintable, printable)):
        if not predicate(chr(code)):
            continue
        if target and target[-1][1] == code - 1:
            target[-1][1] = code
        else:
            target.append([code, code])
(ROOT / "data").mkdir(exist_ok=True)
(ROOT / "data/reference.json").write_text(
    json.dumps(
        {
            "countries": countries,
            "region_countries": regions,
            "overrides": {},
            "alpha_ranges": ranges,
            "printable_ranges": printable,
        },
        ensure_ascii=False,
        indent=2,
        sort_keys=True,
    )
    + "\n"
)
(ROOT / "data/sources.json").write_text(
    json.dumps(
        {
            "sources": sources,
            "unicode_version": __import__("unicodedata").unidata_version,
        },
        indent=2,
    )
    + "\n"
)
print(
    f"Wrote {len(countries)} country aliases and {len(regions)} region names",
    flush=True,
)
