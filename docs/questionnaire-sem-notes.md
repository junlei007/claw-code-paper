# Questionnaire SEM notes

This note captures the minimum method guidance needed to synthesize a **project skill** for questionnaire-based structural equation modeling (SEM).

It is intentionally framed as a **skill-first, R-first** capability:

- **skill-first** because the unstable part is often the modeling contract, not only the computation
- **R-first** because the default execution path for SEM, CFA, indirect effects, and measurement invariance is more mature in the `lavaan` ecosystem than in general-purpose Python workflows

Default presentation baseline for outputs produced from this method family:

- [`./research-output-style-guide.md`](./research-output-style-guide.md)

---

## What this method family covers

For this repo, the questionnaire SEM family currently includes:

1. **Confirmatory factor analysis (CFA)**
   - confirm whether questionnaire items support the proposed latent constructs

2. **Structural equation modeling (SEM)**
   - specify structural paths among latent or observed constructs

3. **Indirect / mediation effects inside SEM**
   - estimate indirect paths in a path-model or latent-variable framework

4. **Multiple-group SEM / measurement invariance**
   - compare groups only after the measurement model is defensible

For now, treat latent moderation, mixture SEM, longitudinal SEM, and multilevel SEM as **advanced extensions** that should be gated behind stronger review.

---

## Why R should be the default execution path

For this method family, the best default is usually **R + lavaan**.

Reasons:

- `lavaan` is one of the most common open-source research workflows for CFA / SEM
- the reporting language around fit indices, standardized loadings, path coefficients, and invariance testing maps naturally to the R / lavaan ecosystem
- many questionnaire-analysis examples, tutorials, and teaching materials already assume lavaan-style syntax
- related R packages such as `semTools`, `psych`, and `semPlot` fit naturally around the same workflow

That does **not** mean Python is useless.

Python is still a strong fit for:

- data ingestion and preprocessing
- scoring and recoding
- metadata extraction
- report assembly
- figure post-processing

But the default SEM fitting lane should remain **R-first** unless a project has a strong reason to standardize a different backend.

---

## Expected inputs

A good questionnaire SEM run should not start unless the project has at least:

- a scored dataset or a scoring-ready dataset plus codebook
- explicit latent construct definitions
- indicator-to-factor mapping
- documented reverse-key handling
- a missing-data handling rule
- a theory-driven structural hypothesis
- a sample size that is at least plausibly adequate for the proposed model

Helpful but not always mandatory:

- prior reliability summaries
- prior EFA or construct-development notes
- preregistration or analysis-plan notes
- a grouping variable if invariance or multi-group SEM is requested

---

## Recommended workflow

1. **Confirm the measurement story first**
   - define constructs and item membership
   - verify scoring direction and reverse-key handling
   - stop if construct definitions are still ambiguous

2. **Choose observed vs latent framing explicitly**
   - use latent-variable SEM when construct measurement quality matters
   - use observed composite/path models only when that simplification is defensible

3. **Fit CFA before structural claims when latent constructs are central**
   - inspect loading patterns
   - inspect convergence and warnings
   - inspect fit indices before moving to the structural model

4. **Fit the structural model conservatively**
   - document path directions and theory-based ordering
   - separate direct, indirect, and total effects clearly
   - use bootstrap confidence intervals for indirect effects when appropriate

5. **Handle multi-group comparisons carefully**
   - do not compare latent means or structural paths across groups before checking measurement invariance
   - record the exact invariance sequence used (for example configural -> metric -> scalar)

6. **Produce explicit handoff artifacts**
   - model specification note
   - fit summary with warnings
   - parameter summary with standardized effects
   - interpretation checklist
   - plugin handoff note if the computation contract begins to stabilize

---

## Minimum reporting expectations

A reusable SEM workflow in this repo should typically report:

- estimator and missing-data handling choice
- whether the model converged
- key fit indices such as `CFI`, `TLI`, `RMSEA`, `SRMR`, and chi-square statistics where appropriate
- standardized factor loadings for CFA
- standardized structural path coefficients for SEM
- indirect-effect estimates and confidence intervals when mediation is modeled
- any identification, convergence, Heywood-case, or modification-index warnings that materially affect interpretation

For user-facing delivery, these results should normally be shaped into:

- a compact manuscript-friendly fit summary
- a clean parameter summary table
- a restrained SEM/CFA figure suitable for paper-draft refinement
- prose that separates statistical result description from substantive interpretation

---

## Failure checks

Stop or escalate when:

- construct-to-item mapping is ambiguous
- reverse-key handling is unknown or unverified
- the sample is too small for the proposed model and no simplification decision has been made
- the user wants group comparisons without an invariance plan
- the model does not converge and no fallback strategy is documented
- fit is poor but the workflow is proceeding as if the latent model were already defensible
- the project is making strong causal language claims from cross-sectional SEM without explicit caution

---

## Why this is still skill-first before plugin-first

Even if the computation uses R/lavaan, the unstable part is often still the **analysis contract**:

- which constructs are latent vs observed
- whether CFA is required before SEM
- which fit thresholds or decision rules the team accepts
- how to handle poor fit, cross-loadings, or model revision requests
- whether the request is explanatory, confirmatory, exploratory, or publication-facing

That makes the current best output a governed skill first:

- the **skill** owns decision rules, prerequisites, interpretation limits, and reporting SOP
- a future **plugin** can own the stable executable contract after the team converges on one repeatable lane

---

## What a future plugin would own

Only promote this workflow into a stable external plugin when the team has settled:

- canonical SEM input schema
- canonical model-spec representation
- supported estimators / missing-data strategies
- standard diagnostics and warning semantics
- expected output tables / figures / artifacts

Until then, keep the executable path flexible but default it to **R/lavaan**.

---

## Source references

Primary references used for this note:

1. lavaan overview — https://lavaan.ugent.be/tutorial/
2. lavaan CFA tutorial — https://lavaan.ugent.be/tutorial/cfa.html
3. lavaan SEM tutorial — https://lavaan.ugent.be/tutorial/sem.html
4. lavaan multiple-groups / invariance tutorial — https://lavaan.ugent.be/tutorial/groups.html
5. lavaan package overview — https://lavaan.ugent.be/

These sources anchor the current project-skill guidance; they do **not** replace project-specific statistical review.
