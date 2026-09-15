"""Regression coverage for failure propagation through the CI gate."""

import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import ci_validation


class CiValidationTests(unittest.TestCase):
    def test_failure_survives_a_later_success_and_all_groups_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp)
            with patch("ci_validation.subprocess.run", side_effect=[
                subprocess.CompletedProcess(["first"], 7),
                subprocess.CompletedProcess(["second"], 0),
            ]) as run:
                status = ci_validation.run_commands([("first", ["first"]), ("second", ["second"])], output, {})
            self.assertEqual(status, 1)
            self.assertEqual(run.call_count, 2)
            self.assertEqual([row["status"] for row in json.loads((output / "results.json").read_text())], ["FAIL", "PASS"])

    def test_coverage_requires_references_for_each_codec_mode_and_gop(self):
        commands = list(ci_validation.validation_commands(Path("/ff"), Path("/results")))
        self.assertEqual(len(commands), 2 * 2 * 3 * len(ci_validation.SETS))
        for _, command in commands:
            self.assertEqual(command[command.index("--reference-mode") + 1], "required")
            self.assertNotIn("--limit", command)
            self.assertNotIn("--frames", command)

    def test_empty_campaign_is_not_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(ci_validation.run_commands([], Path(tmp), {}), 1)


if __name__ == "__main__":
    unittest.main()
