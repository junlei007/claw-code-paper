# R psychometrics / CFA bridge contract

This document defines the current contract for the bundled R backend lane.

## Purpose

Use R for psychometrics and latent-variable modeling where the ecosystem is stronger than Python for the target workflows:
- reliability (`psych`)
- KMO / Bartlett pre-checks (`psych`)
- CFA / SEM (`lavaan`)

## Input

```json
{
  "datasetPath": "data/survey.sav",
  "analysisId": "study-001-wave1",
  "scaleDefinitions": [
    {
      "name": "engagement",
      "items": ["q1", "q2", "q3", "q4"],
      "reverseItems": ["q3"]
    }
  ],
  "reverseItems": ["q3"],
  "responseScale": {"min": 1, "max": 5},
  "missingHandling": "fiml",
  "estimator": "MLR",
  "cfaModel": "engagement =~ q1 + q2 + q3 + q4"
}
```

Format-specific notes:
- `.csv` / `.tsv` / delimited `.dat`: support `delimiter`, `encoding`, and `naValues`
- fixed-width `.dat`: require `layout = "fixed-width"` plus `widths` or `columnWidths`
- `.xlsx`: requires `readxl`
- `.sav`: requires `haven`

## Output

```json
{
  "analysisId": "study-001-wave1",
  "status": "ok",
  "reliability": {
    "engagement": {
      "alpha": 0.88,
      "omega": 0.90
    }
  },
  "validity": {
    "completeCases": 412,
    "kmo": 0.84,
    "bartlett": {
      "chiSquare": 412.7,
      "df": 45,
      "pValue": 0.000001
    }
  },
  "cfa": {
    "converged": true,
    "fit": {
      "cfi": 0.96,
      "tli": 0.95,
      "rmsea": 0.05,
      "srmr": 0.04
    },
    "standardizedLoadings": []
  },
  "warnings": []
}
```

## Behavior rules

- Input should usually come from `survey_metadata` or a compatible preprocessing step.
- The backend emits JSON only; warnings and captured console chatter are normalized into `warnings`.
- `reliability` falls back to an `all_items` aggregate when no `scaleDefinitions` are supplied.
- `kmo` is skipped when the correlation matrix is singular or not positive definite.
- `cfa` is `null` when not requested or when pre-checks show singular/perfectly collinear item correlations.
- Any convergence, missing-data, identification, or singular-matrix issues must be surfaced explicitly in `warnings`.
- Keep raw statistical output separate from interpretation.
