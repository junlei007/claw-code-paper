# Research Method Registry

This document is the canonical registry for research-analysis methods in this repository.

It answers four questions:
- what methods are already integrated
- how users should run them
- what artifacts they should expect
- what to do when a needed method is not integrated yet

---

## Status vocabulary

| Status | Meaning |
| --- | --- |
| `integrated` | Shipped in the current product as a callable bundled capability |
| `external` | Recommended to live as an external plugin rather than in the bundled core |
| `manual` | Not implemented as a callable tool; currently analyst-guided / prompt-guided |
| `planned` | Intended roadmap item; not callable yet |

---

## Reference user workflow

For survey research, the current reference path is:

```text
claw init --research survey
  -> survey_metadata
  -> survey_score
  -> survey_psychometrics
  -> survey_report
```

Default local output directory:

```text
.claw/artifacts/
```

If a method is not listed as `integrated`, users should follow the extension decision path in:
- [`./research-method-standards.md`](./research-method-standards.md)

---

## Integrated methods

### 1. survey_metadata

- **Status:** `integrated`
- **Domain:** data ingestion / metadata / questionnaire structure inspection
- **Backend:** Python
- **Entry point:** `survey_metadata`
- **Source:** bundled survey plugin
- **Purpose:** inspect a questionnaire dataset and return a structured metadata summary before scoring or psychometrics
- **Typical inputs:**
  - `datasetPath`
  - optional `format`
  - optional `sheetName`
  - optional `delimiter` / `encoding` / `naValues`
  - optional `scaleDefinitions`
  - optional `reverseItems`
- **Typical outputs:**
  - dataset summary
  - schema summary
  - missingness summary
  - questionnaire coverage / warnings
  - backend guidance for downstream steps
- **Recommended artifact:** `metadata.json`
- **Limits:** does not itself compute scores or psychometric statistics

### 2. survey_score

- **Status:** `integrated`
- **Domain:** scoring / reverse coding / scale construction
- **Backend:** Python
- **Entry point:** `survey_score`
- **Source:** bundled survey plugin
- **Purpose:** compute reverse-keyed scoring plus scale-level mean/sum scores and optionally export a scored dataset
- **Typical inputs:**
  - `datasetPath`
  - `scaleDefinitions`
  - optional `reverseItems`
  - optional `responseScale`
  - optional `idColumns`
  - optional `outputPath`
- **Typical outputs:**
  - score summaries
  - scale-level scoring diagnostics
  - preview rows
  - optional scored dataset artifact
- **Recommended artifacts:**
  - `scored.csv`
  - optionally `scoring-summary.json`
- **Limits:** not a substitute for reliability / validity / CFA analysis

### 3. survey_psychometrics

- **Status:** `integrated`
- **Domain:** reliability / validity pre-checks / CFA
- **Backend:** R
- **Entry point:** `survey_psychometrics`
- **Source:** bundled survey plugin
- **Purpose:** run reliability, validity pre-checks, and optional CFA for questionnaire data
- **Typical inputs:**
  - `datasetPath`
  - optional `scaleDefinitions`
  - optional `reverseItems`
  - optional `responseScale`
  - optional `analysisId`
  - optional `cfaModel`
  - optional `estimator`
  - optional `missingHandling`
- **Typical outputs:**
  - reliability metrics
  - validity pre-checks
  - optional CFA result
  - warnings for convergence / identification / data quality issues
- **Recommended artifact:** `psychometrics.json`
- **Limits:** domain interpretation still requires researcher judgment; CFA quality depends on model quality, sample size, and data quality

### 4. survey_report

- **Status:** `integrated`
- **Domain:** reporting / synthesis
- **Backend:** Python
- **Entry point:** `survey_report`
- **Source:** bundled survey plugin
- **Purpose:** render a Markdown report draft from structured survey-analysis results
- **Typical inputs:**
  - dataset summary / metadata summary
  - scoring summary
  - psychometrics summary
  - report context values
- **Typical outputs:**
  - Markdown report draft
- **Recommended review artifact:** `report-review.json`
- **Recommended artifact:** `report.md`
- **Limits:** report output is a reviewed draft workflow target, not automatic final publication-ready formatting

---

## Methods that are not integrated yet

The following categories are expected to be needed, but are not currently exposed as bundled callable tools in this branch:

| Method area | Status | Recommended path |
| --- | --- | --- |
| EFA / factor extraction / rotation | `planned` | external plugin first, promote if reused broadly |
| questionnaire SEM / CFA / invariance | `external` / `skill-first` | start from [`./questionnaire-sem-rehearsal.md`](./questionnaire-sem-rehearsal.md), [`../examples/project-skills/questionnaire-sem-sop/`](../examples/project-skills/questionnaire-sem-sop/), and [`../examples/external-plugins/research-sem/`](../examples/external-plugins/research-sem/) |
| regression / mediation / moderation | `external` | start from [`../examples/external-plugins/research-regression/`](../examples/external-plugins/research-regression/) and expand only after the contract stabilizes |
| grouped comparisons / ANOVA variants | `planned` | external plugin first |
| publication-grade table generation | `planned` | plugin if output contract stabilizes |
| domain-specific niche methods | `manual` / `external` | do not bundle by default |

---

## What users should do when a method is missing

1. Check this registry first.
2. If the method is `integrated`, use the documented tool chain.
3. If the method is not integrated:
   - use an existing external plugin if available
   - otherwise create a project-level skill for the workflow
   - promote to an external or bundled plugin only if the method becomes stable and reusable
4. Follow the standards in:
   - [`./research-method-standards.md`](./research-method-standards.md)

---

## Notes for maintainers

This registry should stay lightweight and practical.

For each new method, record at least:
- status
- domain
- backend
- entry point
- expected inputs
- expected outputs
- artifact shape
- limits / caveats
