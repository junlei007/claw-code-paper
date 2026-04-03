# research-sem external plugin prototype

This directory is a **reference external plugin prototype** for questionnaire-focused CFA / SEM analysis.

It exists to show the intended extension path when a method family is:

- reusable enough to deserve a tool contract
- better served by an R/lavaan execution lane
- not yet stable enough to bundle into the core runtime

See also:

- [`../../../docs/questionnaire-sem-notes.md`](../../../docs/questionnaire-sem-notes.md)
- [`../../../docs/questionnaire-sem-rehearsal.md`](../../../docs/questionnaire-sem-rehearsal.md)
- [`../../../docs/research-method-registry.md`](../../../docs/research-method-registry.md)
- [`../../../docs/research-method-standards.md`](../../../docs/research-method-standards.md)
- [`./contracts/sem-contract.md`](./contracts/sem-contract.md)

---

## Why external

Questionnaire SEM is a good example of a method family that should begin **outside** the bundled core:

- it is high-value and broadly reusable
- the execution backend can already be made fairly stable with R/lavaan
- but the full method boundary still expands quickly (CFA, SEM, invariance, latent interactions, longitudinal SEM, etc.)

So this prototype is intentionally thin: one manifest, one tool, one fixture dataset, one contract note, one README.

---

## Included tool

| Tool | Purpose |
| --- | --- |
| `sem_lavaan` | Run a lightweight CFA/SEM model on a CSV dataset and return structured fit diagnostics plus standardized estimates |

---

## Local example

```bash
printf '%s' '{
  "datasetPath": "examples/external-plugins/research-sem/fixtures/sem_demo.csv",
  "analysisType": "sem",
  "modelSpec": "stress =~ stress1 + stress2 + stress3\ncoping =~ coping1 + coping2 + coping3\nburnout =~ burnout1 + burnout2 + burnout3\ncoping ~ a*stress\nburnout ~ cprime*stress + b*coping\nindirect := a*b\ntotal := cprime + (a*b)",
  "estimator": "ML",
  "missingHandling": "fiml",
  "bootstrap": 200
}' | \
env CLAW_TOOL_NAME=sem_lavaan \
    CLAW_PLUGIN_ID=research-sem@example \
    CLAW_PLUGIN_ROOT=$(pwd)/examples/external-plugins/research-sem \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    Rscript examples/external-plugins/research-sem/tools/sem_tools.R
```

Minimal invariance example:

```bash
printf '%s' '{
  "datasetPath": "examples/external-plugins/research-sem/fixtures/sem_demo.csv",
  "analysisType": "cfa",
  "groupColumn": "group",
  "measurementInvariance": true,
  "modelSpec": "stress =~ stress1 + stress2 + stress3\ncoping =~ coping1 + coping2 + coping3\nburnout =~ burnout1 + burnout2 + burnout3"
}' | \
env CLAW_TOOL_NAME=sem_lavaan \
    CLAW_PLUGIN_ID=research-sem@example \
    CLAW_PLUGIN_ROOT=$(pwd)/examples/external-plugins/research-sem \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    Rscript examples/external-plugins/research-sem/tools/sem_tools.R
```

Optional artifact write:

```json
{
  "outputPath": ".claw/artifacts/sem-summary.json"
}
```

---

## Current limitations

- CSV only
- measurement invariance currently only covers a simple configural / metric / scalar CFA sequence
- no modification-index recommendation layer yet
- no latent interaction / mixture / multilevel SEM support yet
- no publication-grade table / figure formatter yet

That is intentional: it is a prototype governance example, not a full SEM platform.
