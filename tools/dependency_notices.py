import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"], cwd=ROOT
    )
)
sections = []
missing = []
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if package["name"] == "geolocation":
        continue
    directory = Path(package["manifest_path"]).parent
    files = sorted(
        path
        for path in directory.iterdir()
        if path.is_file()
        and path.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE"))
    )
    header = f"{package['name']} {package['version']}\nLicense: {package['license']}\nRepository: {package['repository']}\n"
    sections.append(
        header + "\n".join(path.read_text(errors="replace") for path in files)
    )
    if not files:
        missing.append(package["name"])
text = ("\n\n" + "=" * 72 + "\n\n").join(sections)
(ROOT / "THIRD_PARTY_NOTICES.txt").write_text(
    "\n".join(line.rstrip() for line in text.splitlines()).strip() + "\n"
)
print(f"Wrote notices for {len(sections)} Cargo packages")
print("Packages without top-level notice files:", ", ".join(missing) or "none")
print(
    "License expressions:",
    sorted({p["license"] or "UNSPECIFIED" for p in metadata["packages"]}),
)
