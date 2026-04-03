# Research Output Style Guide

This document defines the **default presentation standard** for analysis outputs generated in this project.

Default target:

- **SSCI / top-journal-ready working style**

That does **not** mean every output is instantly submission-ready. It means the system should default to a presentation style that is:

- clean
- restrained
- readable
- method-explicit
- easy to polish into a manuscript artifact

If the user provides a journal style, lab template, house style, or advisor preference, that **user-supplied standard overrides this default**.

---

## 1. Priority rule

Default precedence:

1. explicit user-provided style guide or target journal requirement
2. project-level method or report contract
3. this default style guide

So the agent should behave as follows:

- if the user says “按 APA/某期刊规范来”，follow that
- if the user gives an example figure/table/report, align to that
- otherwise, use this document as the default baseline

---

## 2. Core presentation principles

All research outputs should default to:

1. **Minimal ornament**
   - no decorative colors, gradients, 3D effects, or chartjunk

2. **Interpretability first**
   - axis labels, units, model labels, and notes must be explicit

3. **Publication-friendly structure**
   - titles, captions, notes, and variable names should be easy to move into a paper

4. **Method transparency**
   - important estimator / sample / model / uncertainty details must not be hidden

5. **Bilingual robustness when needed**
   - Chinese / English labels should render cleanly and remain readable

---

## 3. Default figure standard

Unless the user requests otherwise, figures should follow these defaults:

- white background
- black / grayscale first, restrained accent color second
- consistent font family and fallback handling
- no unnecessary legend if direct labels are clearer
- no dense wall-of-text inside plotting area
- concise title, informative caption, explicit note if needed

### Figure typography

- prefer clean sans-serif for figures
- keep font sizes consistent across title / axis / tick / note
- support CJK rendering correctly when Chinese appears
- avoid oversized bold styling unless used sparingly for emphasis

### Figure geometry

- use aspect ratios suitable for paper export
- avoid cramped margins and clipped labels
- rotate labels only when necessary
- prefer fewer panels with clearer logic over many tiny subplots

### Figure notes

When the figure needs methodological context, put it in a note, not by cluttering the plot body.

Typical figure-note content:

- sample size
- estimator or model family
- whether coefficients are standardized
- confidence interval type
- significance notation only if required

---

## 4. Default table standard

Tables should default to:

- manuscript-friendly structure
- stable column naming
- controlled decimal precision
- aligned statistics by meaning, not by raw dump order

Recommended defaults:

- usually 2–3 decimals unless method norms require otherwise
- coefficient tables should separate estimate / SE / CI / p-value clearly
- fit tables should group global fit metrics together
- notes should define abbreviations and estimation method

Do **not** dump raw JSON directly into user-facing tables or reports unless the user explicitly asks for machine-readable detail.

---

## 5. Default narrative-writing standard

User-facing narrative output should sound like a careful research assistant, not a raw tool log.

Default writing rules:

- lead with the analytical question and main finding
- keep numerical claims tied to a concrete result
- separate result description from interpretation
- avoid overclaiming causality
- surface caveats in normal prose, not hidden in a machine blob

Preferred report structure:

1. objective
2. data / model summary
3. key findings
4. diagnostics / assumptions / caveats
5. suggested next step

### Style constraints

- no raw JSON blocks in the main narrative by default
- no “tool returned / function called / payload” style wording in final user-facing prose
- no exaggerated certainty
- no unexplained statistical abbreviation wall

---

## 6. SEM-specific default standard

For CFA / SEM outputs, default to:

- report estimator and missing-data handling explicitly
- separate measurement results from structural results
- identify whether coefficients are standardized
- report fit indices in a compact, conventional order
- keep path diagrams clean and static-first
- prefer simple node labels over verbose sentence labels

### SEM figures

- use restrained monochrome / low-saturation styling by default
- keep latent and manifest nodes visually distinct but not flashy
- suppress nonessential residual/intercept clutter unless analytically necessary
- optimize for quick model inspection first, journal polishing second

### SEM narrative

- distinguish model fit, loading quality, and path evidence
- distinguish direct, indirect, and total effects
- explicitly mention convergence or identification warnings
- avoid claiming “the model is good” without pointing to concrete diagnostics

---

## 7. Override protocol

If the user supplies a target style, the agent should adapt:

- terminology
- decimal rules
- significance notation
- caption format
- table ordering
- figure aesthetics
- language tone

But even under a custom style, the following should remain non-negotiable:

- statistical warnings are not hidden
- labels remain readable
- methods remain traceable
- machine-oriented dumps stay out of the main presentation unless requested

---

## 8. Engineering implication

Skills, plugins, and report generators should treat this document as the default house style for:

- chart output
- table shaping
- result narration
- artifact packaging

When a method-specific contract is added later, it should either:

- inherit this default, or
- explicitly document why it deviates

