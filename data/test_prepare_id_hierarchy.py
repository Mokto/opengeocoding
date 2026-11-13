import pathlib
import runpy
import subprocess
import sys

if len(sys.argv) == 1:
    subprocess.run([sys.executable, __file__, "--check"], check=True, timeout=5)
else:
    assert sys.argv[1:] == ["--check"]
    pattern = runpy.run_path(str(pathlib.Path(__file__).with_name("prepare-id-hierarchy.py")))["RECORD"]
    for row in [
        "('11','Aceh');",
        "('11.01.02.2003','Kota Baru'),",
        r"('11','O\'Brien\\Town');",
        "('11','O''Brien');",
    ]:
        assert pattern.fullmatch(row), row
    assert pattern.fullmatch("('11','" + "\\&" * 2000) is None
    assert pattern.fullmatch("('111','Aceh');") is None
