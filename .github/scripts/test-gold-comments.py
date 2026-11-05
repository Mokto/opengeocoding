import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest


WORKFLOW = Path(__file__).resolve().parents[1] / "workflows/staging-publish.yml"
LIB = textwrap.dedent(WORKFLOW.read_text().split("<<'LIB'\n", 1)[1].split("          LIB\n", 1)[0])
KEEP = textwrap.dedent(WORKFLOW.read_text().split("      - name: Keep the report\n", 1)[1]
                       .split("      - uses:", 1)[0].split("        run: |\n", 1)[1])


class Comments(unittest.TestCase):
    def test_publication_limits_only_the_comment_and_preserves_its_marker(self):
        marker = "<!-- gold-full -->\n"
        for existing in ("", "17"):
            for text in (marker + "A short report.\n", marker + "é" * (55000 - len(marker)),
                         marker + "東京 🐎\n" * 20000 + "FINAL SENTINEL\n"):
                with self.subTest(existing=existing, length=len(text)), tempfile.TemporaryDirectory() as directory:
                    root = Path(directory)
                    source = root / "report.md"
                    source.write_text(text)
                    (root / "lib.sh").write_text(LIB)
                    gh = root / "gh"
                    gh.write_text('''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv
if "--paginate" in args:
    print(os.environ["EXISTING_COMMENT"])
else:
    payload = json.loads(Path(args[args.index("--input") + 1]).read_text())
    Path(os.environ["PUBLISHED"]).write_text(json.dumps({"args": args, "body": payload["body"]}))
''')
                    gh.chmod(0o755)
                    result = subprocess.run(["bash", "-c", 'set -euo pipefail; . "$RUNNER_TEMP/lib.sh"; upsert gold-full "$RUNNER_TEMP/report.md"'],
                                            capture_output=True, text=True, timeout=10, env={
                                                **os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"],
                                                "RUNNER_TEMP": str(root), "PR": "1", "REPO": "Mokto/opengeocoding",
                                                "GITHUB_SERVER_URL": "https://github.com", "GITHUB_RUN_ID": "42",
                                                "EXISTING_COMMENT": existing, "PUBLISHED": str(root / "published.json"),
                                            })
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertTrue((root / "published.json").exists(), result.stdout + result.stderr)
                    published = json.loads((root / "published.json").read_text())
                    body = published["body"]
                    self.assertEqual(source.read_text(), text)
                    (root / "run.json").write_text(json.dumps({"id": "test", "report": text}))
                    kept = subprocess.run(["bash", "-euo", "pipefail", "-c", KEEP], cwd=root,
                                          capture_output=True, text=True, timeout=10)
                    self.assertEqual(kept.returncode, 0, kept.stderr)
                    self.assertEqual((root / "gold-run/report.md").read_bytes(), source.read_bytes())
                    self.assertTrue(body.startswith(marker))
                    self.assertLess(len(body), 65536)
                    self.assertIn("PATCH" if existing else "POST", published["args"])
                    if len(text) <= 55000:
                        self.assertEqual(body, text)
                    else:
                        self.assertIn("Comment truncated.", body.splitlines()[2])
                        self.assertIn("https://github.com/Mokto/opengeocoding/actions/runs/42#artifacts", body)
                        self.assertNotIn("FINAL SENTINEL", body)
                        self.assertNotIn("�", body)


if __name__ == "__main__":
    unittest.main()
