import json
import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest


WORKFLOW = Path(__file__).resolve().parents[1] / "workflows/staging-publish.yml"
LIB = "\n".join(
    line[10:] for line in WORKFLOW.read_text().split("<<'LIB'\n", 1)[1].split("\n          LIB", 1)[0].splitlines()
)
NAMES = ["build-base", "build-head", "eval-base", "eval-head", "report"]


class GoldProgressTest(unittest.TestCase):
    def render(self, run=None, phases="", command="timings"):
        with tempfile.TemporaryDirectory() as root:
            Path(root, "phases.tsv").write_text(phases)
            if run is not None:
                Path(root, "run.json").write_text(json.dumps(run))
            return subprocess.check_output(
                ["bash", "-euo", "pipefail", "-c", LIB + "\n" + command],
                cwd=root, env={**os.environ, "RUNNER_TEMP": root, "GITHUB_ENV": f"{root}/env", "RUN_ID": "gold-0123456789", "KEY": "test", "URL": "http://test.invalid"}, text=True,
            )

    def test_all_steps_are_pending_before_the_run_starts(self):
        output = self.render()
        for name in NAMES:
            self.assertIn(f"| {name} | pending |", output)

    def test_gateway_steps_show_percentage_status_and_exact_durations(self):
        run = {"progress_percent": 40, "steps": [
            {"name": NAMES[0], "status": "completed", "seconds": 891},
            {"name": NAMES[1], "status": "completed", "seconds": 1},
            {"name": NAMES[2], "status": "running", "seconds": 31},
            {"name": NAMES[3], "status": "pending"},
            {"name": NAMES[4], "status": "pending"},
        ]}
        output = self.render(run)
        self.assertIn("40% (2/5 steps completed; not a time estimate)", output)
        self.assertIn("| build-base | completed | 14m51s |", output)
        self.assertIn("| build-head | completed | 0m01s |", output)
        self.assertIn("| eval-base | running | 0m31s |", output)
        self.assertIn("| report | pending | — |", output)
        run["steps"][2].update(status="failed", seconds=61)
        self.assertIn("| eval-base | failed | 1m01s |", self.render(run))
        for step in run["steps"]:
            step.update(status="completed", seconds=0)
        run["progress_percent"] = 100
        self.assertIn("100% (5/5 steps completed", self.render(run))

    def test_older_gateway_keeps_observed_times_and_lists_future_steps(self):
        output = self.render({"status": "running", "phase": "eval-base"}, "build-base\t100\t160\n")
        self.assertIn("| build-base | 1m00s |", output)
        self.assertEqual(output.count("| build-base |"), 1)
        self.assertIn("| eval-head | pending |", output)
        self.assertNotIn("%", output)

    def test_unchanged_phase_still_refreshes_the_comment_on_each_poll(self):
        wait = textwrap.dedent(WORKFLOW.read_text().split("- name: Wait for the run\n        run: |\n", 1)[1].split("      - name: Report\n", 1)[0])
        wait = wait.replace('. "$RUNNER_TEMP/lib.sh"', "")
        output = self.render(command='''
polls=0
curl() {
  polls=$((polls + 1))
  if [ "$polls" -le 2 ]; then
    printf '%s' '{"status":"running","phase":"eval-base"}' > run.json
  else
    printf '%s' '{"status":"succeeded","phase":"done","report":"ok","exit":0}' > run.json
  fi
}
sleep() { :; }
progress() { echo "REFRESH:$1"; }
''' + wait)
        self.assertEqual(output.count("REFRESH:running"), 2)

    def test_comment_includes_the_refresh_time(self):
        output = self.render(command="""
upsert() { cat \"$2\"; }
BASE_SHA=aaaaaaaa HEAD_SHA=bbbbbbbb
progress queued 'Waiting to start.'
""")
        self.assertRegex(output, r"updated \d{2}:\d{2}:\d{2} UTC")
        self.assertIn("| report | pending |", output)


if __name__ == "__main__":
    unittest.main()
