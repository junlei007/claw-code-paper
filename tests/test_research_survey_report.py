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
    def test_survey_report_emits_review_input_and_visual_metadata_artifacts(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_root = Path(tmpdir)
            figure_path = workspace_root / "path-diagram.png"
            figure_path.write_bytes(b"fake-png")
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
                "figures": [
                    {
                        "title": "Trust path diagram",
                        "artifactPath": str(figure_path),
                        "caption": "Figure 1. Standardized trust factor loadings.",
                        "sourceMetrics": ["cfi", "rmsea"],
                        "sourceColumns": ["T1", "T2", "T3"],
                        "expectedValues": {
                            "cfi": 0.973,
                            "rmsea": 0.041,
                        },
                    }
                ],
                "tables": [
                    {
                        "title": "CFA fit summary",
                        "artifactPath": str(table_path),
                        "caption": "Table 1. CFA fit summary.",
                        "sourceMetrics": ["cfi", "tli"],
                        "sourceColumns": ["model", "cfi", "tli"],
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
            self.assertEqual(response["review"]["hardGateFailures"], [])
            self.assertEqual(response["review"]["unresolvedIssues"], [])
            self.assertEqual(response["delivery"]["status"], "ready")
            self.assertTrue(response["delivery"]["ready"])
            self.assertEqual(response["delivery"]["blockingReasons"], [])

            report_path = workspace_root / "artifacts" / "report.md"
            review_path = report_path.with_suffix(".review.json")
            input_path = report_path.with_suffix(".input.json")
            figures_path = report_path.with_suffix(".figures.json")
            tables_path = report_path.with_suffix(".tables.json")

            self.assertTrue(report_path.exists())
            self.assertTrue(review_path.exists())
            self.assertTrue(input_path.exists())
            self.assertTrue(figures_path.exists())
            self.assertTrue(tables_path.exists())
            self.assertEqual(Path(response["report"]["artifact"]["path"]).resolve(), report_path.resolve())
            self.assertEqual(Path(response["review"]["artifact"]["path"]).resolve(), review_path.resolve())
            self.assertEqual(Path(response["reportInput"]["artifact"]["path"]).resolve(), input_path.resolve())
            self.assertEqual(
                Path(response["visualArtifacts"]["figures"]["artifact"]["path"]).resolve(),
                figures_path.resolve(),
            )
            self.assertEqual(
                Path(response["visualArtifacts"]["tables"]["artifact"]["path"]).resolve(),
                tables_path.resolve(),
            )

            review_payload = json.loads(review_path.read_text(encoding="utf-8"))
            self.assertIn("structureQuality", review_payload["dimensions"])
            self.assertIn("narrativeQuality", review_payload["dimensions"])
            self.assertIn("figureAccuracy", review_payload["dimensions"])

            figures_payload = json.loads(figures_path.read_text(encoding="utf-8"))
            tables_payload = json.loads(tables_path.read_text(encoding="utf-8"))
            self.assertEqual(figures_payload[0]["title"], "Trust path diagram")
            self.assertEqual(tables_payload[0]["title"], "CFA fit summary")
            self.assertEqual(response["visualArtifacts"]["figures"]["artifact"]["count"], 1)
            self.assertEqual(response["visualArtifacts"]["tables"]["artifact"]["count"], 1)

    def test_survey_report_uses_bounded_revision_loop_for_textual_fixes(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_root = Path(tmpdir)
            payload = {
                "title": "Revision Needed",
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
                    "warnings": ["Check coding before publication."],
                },
                "notes": ["Keep the report manuscript-friendly."],
                "outputPath": "artifacts/revision-report.md",
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
            self.assertTrue(response["review"]["revision"]["attempted"])
            self.assertGreaterEqual(response["review"]["revision"]["iterations"], 1)
            self.assertLessEqual(response["review"]["revision"]["iterations"], 2)
            self.assertEqual(response["review"]["hardGateFailures"], [])
            self.assertEqual(response["review"]["unresolvedIssues"], [])
            self.assertEqual(response["delivery"]["status"], "ready")

    def test_figure_accuracy_failure_blocks_overall_verdict(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_root = Path(tmpdir)
            table_path = workspace_root / "fit-table.md"
            table_path.write_text("| index | value |\n| --- | --- |\n| CFI | 0.973 |\n", encoding="utf-8")

            payload = {
                "title": "Figure Gate Failure",
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
                        "caption": "Table 1. CFA fit summary.",
                        "sourceMetrics": ["cfi", "tli"],
                        "sourceColumns": ["model", "cfi", "tli"],
                        "expectedValues": {
                            "cfi": 0.95,
                            "tli": 0.961,
                        },
                    }
                ],
                "outputPath": "artifacts/figure-gate-report.md",
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
            self.assertEqual(
                response["review"]["dimensions"]["figureAccuracy"]["verdict"], "revise"
            )
            self.assertEqual(response["review"]["overallVerdict"], "revise")
            self.assertFalse(response["review"]["revision"]["attempted"])
            self.assertEqual(
                response["review"]["hardGateFailures"][0]["dimension"], "figureAccuracy"
            )
            self.assertTrue(
                any("expectedValues mismatch" in issue for issue in response["review"]["unresolvedIssues"])
            )
            self.assertEqual(response["delivery"]["status"], "draft_under_review")
            self.assertFalse(response["delivery"]["ready"])
            self.assertTrue(response["delivery"]["blockingReasons"])

    def test_missing_visual_provenance_fields_fail_figure_accuracy_gate(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_root = Path(tmpdir)
            figure_path = workspace_root / "path-diagram.png"
            figure_path.write_bytes(b"fake-png")

            payload = {
                "title": "Missing Provenance",
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
                    "cfi": 0.973,
                    "rmsea": 0.041,
                },
                "figures": [
                    {
                        "title": "Trust path diagram",
                        "artifactPath": str(figure_path),
                        "sourceMetrics": ["cfi", "rmsea"],
                    }
                ],
                "outputPath": "artifacts/provenance-report.md",
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
            figure_gate = response["review"]["dimensions"]["figureAccuracy"]
            self.assertEqual(figure_gate["verdict"], "revise")
            self.assertTrue(
                any("caption should be declared" in issue for issue in figure_gate["issues"])
            )
            self.assertTrue(
                any("sourceColumns should be declared" in issue for issue in figure_gate["issues"])
            )
            self.assertEqual(
                response["review"]["hardGateFailures"][0]["dimension"], "figureAccuracy"
            )
            self.assertGreaterEqual(len(response["review"]["unresolvedIssues"]), 2)
            self.assertEqual(response["delivery"]["status"], "draft_under_review")


if __name__ == "__main__":
    unittest.main()
