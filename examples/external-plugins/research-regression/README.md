# research-regression external plugin prototype

This directory is a **reference external plugin prototype** for regression analysis.

It exists to show the intended extension path when a research method is:

- useful and reusable
- computation-heavy enough for a tool contract
- not yet part of the bundled survey core

See also:

- [`../../../docs/research-method-registry.md`](../../../docs/research-method-registry.md)
- [`../../../docs/research-method-standards.md`](../../../docs/research-method-standards.md)
- [`./contracts/regression-contract.md`](./contracts/regression-contract.md)

---

## Why external

Regression is a good example of a method family that should usually start **outside** the bundled core:

- it is broadly useful
- but the exact modeling surface can expand quickly
- and the output contract should stabilize before promotion

So this prototype is intentionally thin: one OLS tool, one manifest, one fixture dataset, one README, one contract note.

---

## Included tool

| Tool | Purpose |
| --- | --- |
| `regression_ols` | Run a lightweight OLS regression on a CSV dataset and return structured coefficients plus fit diagnostics |

---

## Local example

```bash
printf '%s' '{
  "datasetPath": "examples/external-plugins/research-regression/fixtures/regression_demo.csv",
  "outcome": "performance",
  "predictors": ["study_hours", "sleep_quality"],
  "includeIntercept": true
}' | \
env CLAW_TOOL_NAME=regression_ols \
    CLAW_PLUGIN_ID=research-regression@example \
    CLAW_PLUGIN_ROOT=$(pwd)/examples/external-plugins/research-regression \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    python3 examples/external-plugins/research-regression/tools/regression_tools.py
```

Optional artifact write:

```json
{
  "outputPath": ".claw/artifacts/regression-summary.json"
}
```

---

## Current limitations

- CSV only
- no categorical encoding
- no robust standard errors
- no inference tables or publication formatting
- no mediation/moderation surface yet

That is intentional: it is a prototype governance example, not a full statistics package.
