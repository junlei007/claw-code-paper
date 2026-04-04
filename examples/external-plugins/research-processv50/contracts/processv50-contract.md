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
- returns structured metadata describing the run

## Current output shape

- `status`
- `dataset`
- `analysis`
- `processScript`
- `artifacts.report`
- `reportParse`
- `warnings`

## Limits

- current prototype focuses on observed-variable PROCESS-style workflows
- latent-variable mediation / moderation requests should be redirected to an SEM / structural-equation workflow instead of this plugin
- report parsing is intentionally shallow; it adds first-pass section extraction while still preserving the native text report rather than inventing publication-grade structured coefficients
- stability depends on the local PROCESS release and the team's private execution environment
