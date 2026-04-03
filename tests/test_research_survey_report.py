from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
PLUGIN_ROOT = REPO_ROOT / "rust" / "crates" / "plugins" / "bundled" / "research-survey"
TOOL_SCRIPT = PLUGIN_ROOT / "tools" / "survey_tools.py"


class ResearchSurveyReportTests(unittest.TestCase):
    def test_survey_report_emits_review_and_input_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_root = Path(tmpdir)
            table_path = workspace_root / "fit-table.md"
            table_path.write_text("| index | value |\n| --- | --- |\n| CFI | 0.973 |\n", encoding="utf-8")

            payload = {
                "title": "Mini Survey Results",
                "datasetPath": "fixtures/mini_survey.csv",
                "datasetSummary": {
                    "dataset": {
                        "path": "fixtures/mini_survey.csv",
                        "format": "csv",
                        "rows": 120,
                    },
                    "questionnaire": {
                        "scaleCount": 2,
                        "reverseItemCount": 1,
                    },
                },
                "results": {
                    "alpha": 0.91,
                    "omega": 0.89,
                    "kmo": 0.88,
                    "bartlett": "χ²(15) = 132.4, p < .001",
                    "modelSpec": "Trust =~ T1 + T2 + T3",
                    "estimator": "MLR",
                    "missingHandling": "FIML",
                    "cfi": 0.973,
                    "tli": 0.961,
                    "rmsea": 0.041,
                    "srmr": 0.036,
                    "warnings": ["No critical convergence issues detected."],
                },
                "notes": ["Use manuscript-style wording."],
                "tables": [
                    {
                        "title": "CFA fit summary",
                        "artifactPath": str(table_path),
                        "sourceMetrics": ["cfi", "tli"],
                        "expectedValues": {
                            "cfi": 0.973,
                            "tli": 0.961,
                        },
                    }
                ],
                "outputPath": "artifacts/report.md",
            }
            env = {
                **os.environ,
                "CLAW_PLUGIN_ID": "research-survey@bundled",
                "CLAW_TOOL_NAME": "survey_report",
                "CLAW_PLUGIN_ROOT": str(PLUGIN_ROOT),
                "CLAW_WORKSPACE_ROOT": str(workspace_root),
            }

            result = subprocess.run(
                [sys.executable, str(TOOL_SCRIPT)],
                input=json.dumps(payload),
                check=True,
                capture_output=True,
                text=True,
                env=env,
            )

            response = json.loads(result.stdout)
            self.assertEqual(response["status"], "ok")
            self.assertEqual(response["review"]["overallVerdict"], "pass")
            self.assertEqual(response["review"]["dimensions"]["figureAccuracy"]["verdict"], "pass")

            report_path = workspace_root / "artifacts" / "report.md"
            review_path = report_path.with_suffix(".review.json")
            input_path = report_path.with_suffix(".input.json")

            self.assertTrue(report_path.exists())
            self.assertTrue(review_path.exists())
            self.assertTrue(input_path.exists())
            self.assertEqual(Path(response["report"]["artifact"]["path"]).resolve(), report_path.resolve())
            self.assertEqual(Path(response["review"]["artifact"]["path"]).resolve(), review_path.resolve())
            self.assertEqual(Path(response["reportInput"]["artifact"]["path"]).resolve(), input_path.resolve())

            review_payload = json.loads(review_path.read_text(encoding="utf-8"))
            self.assertIn("structureQuality", review_payload["dimensions"])
            self.assertIn("narrativeQuality", review_payload["dimensions"])
            self.assertIn("figureAccuracy", review_payload["dimensions"])


if __name__ == "__main__":
    unittest.main()
