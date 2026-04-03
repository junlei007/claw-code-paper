# Report Review Contract

This document defines the minimum review contract for research-analysis reports generated in this project.

It exists so report quality does not depend only on prompt luck or a stronger model.

---

## Purpose

Every user-facing analysis report should be treated as:

1. a **draft artifact**
2. followed by a **review artifact**
3. optionally followed by a **targeted revision pass**

The review contract is meant to improve first-pass quality for:

- figure / table accuracy
- report structure
- narrative style

---

## Required review dimensions

Every report review payload should contain these top-level dimensions:

- `figureAccuracy`
- `structureQuality`
- `narrativeQuality`

Each dimension should report:

- `score`
- `verdict`
- `issues`
- `checksRun`

Allowed verdict values:

- `pass`
- `revise`
- `fail`
- `not_applicable`

---

## Minimum review artifact shape

```json
{
  "qualityProfile": "ssci-default",
  "dimensions": {
    "figureAccuracy": {
      "score": null,
      "verdict": "not_applicable",
      "issues": [],
      "checksRun": 0
    },
    "structureQuality": {
      "score": 92,
      "verdict": "pass",
      "issues": [],
      "checksRun": 8
    },
    "narrativeQuality": {
      "score": 84,
      "verdict": "pass",
      "issues": [],
      "checksRun": 6
    }
  },
  "overallVerdict": "pass",
  "overallScore": 88,
  "requiredFixes": []
}
```

---

## Gate meanings

### 1. Figure / table accuracy

This gate asks:

- do figures and tables match the structured statistical source?
- do reported labels and statistics align with the figure/table spec?
- are referenced figure/table artifacts actually present?

This gate should become as deterministic as possible.

### 2. Structure quality

This gate asks:

- does the report satisfy the expected Results-style section structure?
- are required sections present and ordered reasonably?
- are required reporting fields surfaced when the method family demands them?

This gate should mostly be deterministic.

### 3. Narrative quality

This gate asks:

- does the prose read like a restrained academic results draft?
- does it avoid tool-log / JSON / payload wording?
- does it avoid unsupported causal or overconfident phrasing?

This gate may combine rule-based lint and model-based review, but rule-based checks should come first.

---

## House style interaction

Default precedence:

1. explicit user-supplied journal / lab / advisor style
2. method-specific report contract
3. project house style in [`./research-output-style-guide.md`](./research-output-style-guide.md)

Review should evaluate against that effective style target.

---

## Delivery rule

A report should not be treated as the final user-facing output by default unless:

- all hard review gates pass
- critical warnings are surfaced
- the review artifact is available for inspection

Until then, the report is a **draft under review**, not a final polished deliverable.
