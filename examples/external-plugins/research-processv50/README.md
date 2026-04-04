# research-processv50 external plugin prototype

This directory is a **reference external plugin prototype** for questionnaire-focused PROCESSv50 workflows.

It exists to show the intended extension path when a team:

- already has a local PROCESSv50 installation
- wants a callable Claw tool contract
- does **not** want to commit or redistribute PROCESS source files inside this repository

See also:

- [`../../../docs/processv50-notes.md`](../../../docs/processv50-notes.md)
- [`../../../docs/research-method-registry.md`](../../../docs/research-method-registry.md)
- [`../../../docs/research-method-standards.md`](../../../docs/research-method-standards.md)
- [`../../../examples/project-skills/questionnaire-processv50-sop/`](../../../examples/project-skills/questionnaire-processv50-sop/)
- [`./contracts/processv50-contract.md`](./contracts/processv50-contract.md)

---

## Why external

PROCESSv50 is a strong example of a method family that should begin **outside** the bundled core:

- it can be highly reusable
- it depends on a local third-party statistical runtime
- packaging / distribution boundaries should stay explicit
- teams may need private environment setup before the contract is stable

So this prototype is intentionally thin: one manifest, one wrapper tool, one fixture dataset, one mock process script for validation, one contract note, one README.

---

## Included tool

| Tool | Purpose |
| --- | --- |
| `processv50_run` | Run a PROCESSv50-style model through a local `process.R` path and capture the text report as an artifact |

---

## Local example

Using a private local PROCESS install:

```bash
printf '%s' '{
  "datasetPath": "examples/external-plugins/research-processv50/fixtures/process_demo.csv",
  "model": 4,
  "y": "burnout",
  "x": "stress",
  "m": "coping",
  "boot": 5000,
  "outputPath": ".claw/artifacts/processv50-report.txt"
}' | \
env CLAW_TOOL_NAME=processv50_run \
    CLAW_PLUGIN_ID=research-processv50@example \
    CLAW_PLUGIN_ROOT=$(pwd)/examples/external-plugins/research-processv50 \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    PROCESSV50_R_PATH=/absolute/path/to/process.R \
    Rscript examples/external-plugins/research-processv50/tools/processv50_tools.R
```

Using the bundled mock fixture for contract validation only:

```bash
printf '%s' '{
  "datasetPath": "examples/external-plugins/research-processv50/fixtures/process_demo.csv",
  "model": 1,
  "y": "wellbeing",
  "x": "stress",
  "w": "support",
  "processScriptPath": "examples/external-plugins/research-processv50/fixtures/mock_process.R"
}' | \
env CLAW_TOOL_NAME=processv50_run \
    CLAW_PLUGIN_ID=research-processv50@example \
    CLAW_PLUGIN_ROOT=$(pwd)/examples/external-plugins/research-processv50 \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    Rscript examples/external-plugins/research-processv50/tools/processv50_tools.R
```

---

## Current limitations

- CSV-style datasets only in this prototype
- preserves the PROCESS text report instead of extracting a publication-grade structured summary
- depends on a locally provided `process.R` script path
- does not yet standardize model-number presets, coefficient parsing, or reporting tables
- should remain external / private until the execution and redistribution boundaries are fully settled
