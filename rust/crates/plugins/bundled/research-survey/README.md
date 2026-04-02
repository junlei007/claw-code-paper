# research-survey bundled plugin

Rust-first survey research plugin scaffold for questionnaire data analysis.

## What it does now

Bundled tools:
- `survey_metadata` — Python ingest + schema summary for CSV / XLSX / SPSS `.sav` / delimited `.dat`
- `survey_psychometrics` — R psychometrics lane for reliability, validity pre-checks, and optional CFA
- `survey_report` — markdown report rendering from structured analysis results

Bundled assets:
- `templates/survey-report.md` — report template
- `contracts/r-psychometrics.md` — machine-readable contract and output conventions
- `fixtures/mini_survey.csv` — tiny local verification fixture

## Why this shape

This keeps the Rust core lightly coupled to upstream runtime changes while concentrating product differentiation in:
- provider profiles
- research-mode prompting
- bundled plugins / skills
- Python and R analysis bridges

## Expected workflow

1. Use a provider profile such as DeepSeek or another OpenAI-compatible backend.
2. Enable `research.profile = "survey"`.
3. Enable the bundled plugin.
4. Run `survey_metadata` first to inspect coding, missingness, scale coverage, and reverse-keyed items.
5. Run `survey_psychometrics` with scale definitions, reverse-coded items, and an optional CFA model.
6. Use `survey_report` to draft a markdown report artifact.

## Example settings

```json
{
  "model": "deepseek-chat",
  "providers": {
    "default": "deepseek",
    "profiles": {
      "deepseek": {
        "type": "openai-compat",
        "providerName": "DeepSeek",
        "apiKeyEnv": "DEEPSEEK_API_KEY",
        "baseUrl": "https://api.deepseek.com/v1",
        "defaultModel": "deepseek-chat"
      }
    }
  },
  "research": {
    "enabled": true,
    "profile": "survey",
    "artifactDir": ".claw/artifacts"
  },
  "plugins": {
    "enabled": {
      "research-survey@bundled": true
    }
  }
}
```

## Tool notes

### `survey_metadata`

Input highlights:
- `datasetPath` required
- `format` optional (`csv`, `xlsx`, `sav`, `dat`, or `auto`)
- `sheetName` for Excel
- `delimiter` / `encoding` / `naValues` for text-like files
- `scaleDefinitions` and `reverseItems` to validate questionnaire structure

Output highlights:
- dataset dimensions and detected format
- per-column type + missingness summary
- scale coverage and unresolved reverse-coded items
- backend hints for Python ingest and R psychometrics

Relative dataset paths are resolved against `CLAW_WORKSPACE_ROOT` when available, then the plugin root as fallback.

### `survey_psychometrics`

Input highlights:
- `datasetPath` required
- `scaleDefinitions` strongly recommended for meaningful scale-level reliability
- `reverseItems` and `responseScale` support reverse scoring
- `cfaModel` is optional; when omitted the tool only returns reliability + validity pre-checks
- fixed-width `.dat` input requires `layout = "fixed-width"` plus `widths` or `columnWidths`

Output highlights:
- `reliability` per declared scale, or a fallback `all_items` result when scale definitions are omitted
- `validity` with Bartlett plus KMO when the correlation matrix is stable enough
- `cfa` is `null` when CFA is skipped or not requested
- a normalized `warnings` array for convergence issues, singular matrices, missing items, and skipped analyses

The R backend is designed to emit JSON only; noisy R console output is captured and surfaced through `warnings` instead of leaking to stdout/stderr.

### `survey_report`

Renders the bundled markdown template and optionally writes an artifact via `outputPath`.

## Dependency notes

Python lane:
- `pandas` required
- `openpyxl` required for `.xlsx`
- `pyreadstat` required for `.sav`

R lane:
- `jsonlite`, `psych`, and `lavaan` required
- `readxl` required for `.xlsx`
- `haven` required for `.sav`

If optional file-format dependencies are missing, the tool returns a structured JSON error describing the missing package.

## Example psychometrics invocation

```bash
printf '%s' '{
  "datasetPath": "rust/crates/plugins/bundled/research-survey/fixtures/mini_survey.csv",
  "analysisId": "mini",
  "scaleDefinitions": [{"name": "engagement", "items": ["q1", "q2", "q3", "q4"], "reverseItems": ["q3"]}],
  "reverseItems": ["q3"],
  "responseScale": {"min": 1, "max": 5},
  "cfaModel": "engagement =~ q1 + q2 + q3 + q4"
}' | \
  env CLAW_TOOL_NAME=survey_psychometrics \
      CLAW_PLUGIN_ID=research-survey@bundled \
      CLAW_PLUGIN_ROOT=$(pwd)/rust/crates/plugins/bundled/research-survey \
      CLAW_WORKSPACE_ROOT=$(pwd) \
      Rscript rust/crates/plugins/bundled/research-survey/tools/survey_psychometrics.R
```

On the bundled fixture, reliability returns clean JSON and records the singular-matrix problem in `warnings`, which causes omega and CFA to be skipped instead of printing raw R errors.
