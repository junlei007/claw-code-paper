# SEM External Plugin Contract

This directory is a **prototype external plugin**, not a bundled core capability.

Tool:

- `sem_lavaan`

## Purpose

Demonstrate the extension path for questionnaire-focused CFA / SEM workflows when the project wants a stable execution lane, but the overall method family is still too broad to bundle into the core.

The intended default execution backend is:

- **R**
- **lavaan**

## Input contract

```json
{
  "datasetPath": "examples/external-plugins/research-sem/fixtures/sem_demo.csv",
  "analysisType": "sem",
  "modelSpec": "stress =~ stress1 + stress2 + stress3\ncoping =~ coping1 + coping2 + coping3\nburnout =~ burnout1 + burnout2 + burnout3\ncoping ~ a*stress\nburnout ~ cprime*stress + b*coping\nindirect := a*b\ntotal := cprime + (a*b)",
  "estimator": "ML",
  "missingHandling": "fiml",
  "bootstrap": 200,
  "outputPath": ".claw/artifacts/sem-summary.json"
}
```

Optional fields:

- `groupColumn` for a multi-group fit
- `delimiter` / `encoding` / `naValues` for CSV parsing overrides

## Output shape

The tool returns:

- dataset metadata
- model execution settings
- fit diagnostics
- standardized loadings
- standardized structural paths
- defined parameters (e.g. indirect effects)
- warnings

If `outputPath` is provided, it also writes the same structured payload to disk.

## Interpretation boundary

This plugin computes a model summary. It does **not** make substantive research claims for the user.

Researchers still need to judge:

- construct validity
- model adequacy
- invariance appropriateness
- causal limits
- publication readiness

## Current scope boundary

This prototype is intentionally narrow:

- CSV only
- one generic `lavaan` execution surface
- CFA / SEM fit summaries
- no automatic modification-index driven model revision
- no automated measurement invariance sequence yet
- no publication-grade table formatter yet
