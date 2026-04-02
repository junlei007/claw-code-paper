# Regression External Plugin Contract

This directory is a **prototype external plugin**, not a bundled core capability.

Tool:

- `regression_ols`

## Purpose

Demonstrate the governance path for methods that are:

- stable enough to deserve a tool contract
- useful for research analysis
- not yet central enough for the bundled core

## Input contract

```json
{
  "datasetPath": "examples/external-plugins/research-regression/fixtures/regression_demo.csv",
  "outcome": "performance",
  "predictors": ["study_hours", "sleep_quality"],
  "includeIntercept": true,
  "outputPath": ".claw/artifacts/regression-summary.json"
}
```

## Output shape

The tool returns:

- dataset metadata
- coefficient estimates
- fit diagnostics
- row filtering summary
- warnings

If `outputPath` is provided, it also writes the same structured payload to disk.

## Interpretation boundary

This plugin computes a model summary. It does **not** make substantive research claims for the user.

Researchers still need to judge:

- model appropriateness
- omitted-variable risk
- causal limits
- publication readiness
