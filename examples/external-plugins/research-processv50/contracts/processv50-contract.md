# processv50_run contract

This external-plugin prototype is intentionally **contract-first**.

It does **not** bundle Andrew F. Hayes' PROCESS code. Instead, it expects a locally available `process.R` path supplied by one of:

1. input field `processScriptPath`
2. environment variable `PROCESSV50_R_PATH`
3. a private local workflow convention outside the tracked repository

## Input expectations

Required:
- `datasetPath`
- `model`
- `y`
- `x`

Optional:
- `m`, `w`, `z`
- `covariates`
- `boot`, `conf`, `center`, `seed`
- `outputPath`
- `processScriptPath`

## Current execution model

The wrapper:
- reads a CSV-style dataset into R
- sources the local `process.R`
- calls `process(...)`
- captures console output into a text artifact
- writes a machine-readable JSON sidecar artifact next to the text report when possible
- for simple moderation runs (currently model `1` with one moderator), also derives a moderation decomposition plot and a Johnson-Neyman plot
- returns structured metadata describing the run

## Current output shape

- `status`
- `dataset`
- `analysis`
- `processScript`
- `artifacts.report`
- `artifacts.reportJson`
- `artifacts.moderationDecompositionPlot` (when plot generation succeeds)
- `artifacts.johnsonNeymanPlot` (when plot generation succeeds)
- `artifacts.plotMetadataJson` (when plot generation succeeds)
- `reportParse`
  - `reportParse.detectedSections`
  - `reportParse.effectSummary`
  - `reportParse.visualizationData`
  - `reportParse.johnsonNeyman`
- `plots.johnsonNeyman`
- `warnings`

## Limits

- current prototype focuses on observed-variable PROCESS-style workflows
- latent-variable mediation / moderation requests should be redirected to an SEM / structural-equation workflow instead of this plugin
- report parsing is intentionally shallow; it adds first-pass section extraction plus a conservative `effectSummary` layer while still preserving the native text report rather than inventing publication-grade structured coefficients
- moderation decomposition / JN plotting now prefers PROCESS-native visualization / Johnson-Neyman sections when present, then falls back to a simple model-based reconstruction when the current run is compatible with that shortcut
- stability depends on the local PROCESS release and the team's private execution environment
