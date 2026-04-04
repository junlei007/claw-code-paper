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

## Important modeling boundary

This prototype is for **observed-variable PROCESS-style** mediation / moderation workflows.

If your project needs **latent variables** or latent mediation / moderation, prefer a **SEM / structural equation** route instead, such as the [`../research-sem/`](../research-sem/) prototype and the questionnaire SEM workflow docs.

---

## Included tool

| Tool | Purpose |
| --- | --- |
| `processv50_run` | Run a PROCESSv50-style model through a local `process.R` path and capture the text report, JSON sidecar summary, and simple-moderation plots when applicable |

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

Using the richer bundled moderation-report fixture to validate native conditional-effect / JN parsing:

```bash
printf '%s' '{
  "datasetPath": "examples/external-plugins/research-processv50/fixtures/process_demo.csv",
  "model": 1,
  "y": "burnout",
  "x": "stress",
  "w": "support",
  "processScriptPath": "examples/external-plugins/research-processv50/fixtures/mock_process_moderation_sections.R",
  "outputPath": ".claw/artifacts/processv50-native-demo.txt"
}' | \
env CLAW_TOOL_NAME=processv50_run \
    CLAW_PLUGIN_ID=research-processv50@example \
    CLAW_PLUGIN_ROOT=$(pwd)/examples/external-plugins/research-processv50 \
    CLAW_WORKSPACE_ROOT=$(pwd) \
    Rscript examples/external-plugins/research-processv50/tools/processv50_tools.R
```

For a more complex conditional-process shape with combined labels such as `support=... · climate=...`, use `fixtures/mock_process_complex_condition_sections.R`.

When `outputPath` is set to something like `.claw/artifacts/processv50-report.txt`, the wrapper now also tries to emit:

- a sibling JSON sidecar such as `.claw/artifacts/processv50-report.json`
- for simple moderation runs (currently PROCESS model `1` with one numeric moderator), a moderation decomposition figure such as `.claw/artifacts/processv50-report-moderation-decomposition.png`
- for the same simple moderation case, a Johnson-Neyman figure such as `.claw/artifacts/processv50-report-johnson-neyman.png`
- when plot generation succeeds, a plot metadata sidecar such as `.claw/artifacts/processv50-report-plots.json`

The plotting path now prefers PROCESS-native report sections when they are available:

- `Data for visualizing the conditional effect of the focal predictor:`
- `Moderator value(s) defining Johnson-Neyman significance region(s):`
- `Conditional effect of focal predictor at values of the moderator:`

If those sections are not present but the current run is compatible with a simple moderation fallback, the wrapper reconstructs the visuals from an R linear-model approximation instead.

---

## Current limitations

- CSV-style datasets only in this prototype
- preserves the PROCESS text report and only adds shallow structured section/effect extraction in the JSON sidecar / tool output
- moderation decomposition / JN plotting is strongest when PROCESS emits native visualization / JN sections; otherwise it falls back only when the run is compatible with a simple numeric moderation reconstruction
- depends on a locally provided `process.R` script path
- does not yet standardize model-number presets, coefficient parsing, or reporting tables
- should remain external / private until the execution and redistribution boundaries are fully settled
