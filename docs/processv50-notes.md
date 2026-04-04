# Questionnaire PROCESSv50 notes

This note captures the minimum method guidance needed to synthesize a **project skill** for questionnaire-based PROCESSv50 analysis.

It assumes the team has located a usable local `processv50` tool or a PROCESS-style execution surface, but the repo still does **not** treat that computation path as a stable integrated plugin contract.

Accordingly, this note keeps the capability **skill-first**:

- the skill should govern model selection, variable-role checks, scoring prerequisites, bootstrap decisions, and reporting boundaries
- a later external plugin can own stable execution only after the input/output contract is settled

---

## Local tool governance note

If the team is working from a local PROCESSv50 copy, treat that material as subject to the tool author's copyright / redistribution terms and verify those boundaries before packaging or sharing it more broadly.

Operationally, that means:

- use the local copy as a **project execution reference**, not as a public redistribution target
- do not build a bundled plugin around copied internals without checking license / redistribution boundaries
- prefer a skill-first or private external-plugin path until packaging and distribution rules are explicit

---

## When to use this workflow

Use a PROCESSv50-style workflow when the project needs an **observed-variable conditional process analysis** such as:

- simple mediation
- simple moderation
- conditional process / moderated mediation variants

This workflow is most appropriate when:

- constructs have already been scored into observed composite variables
- the team wants a regression-oriented path rather than a latent SEM-first path
- the model roles for `X`, `M`, `Y`, and optional `W` are explicit

---

## Preconditions

A good PROCESSv50 run should not start unless the project has at least:

- a scored questionnaire dataset, or a scoring-ready dataset plus confirmed scale definitions
- explicit variable-role definitions for `X`, `M`, `Y`, and optional `W`
- a stated hypothesis and covariate plan
- documented missing-data handling
- verified reverse-key handling and coding direction

Helpful but not always mandatory:

- reliability / validity evidence for scored variables
- centered / standardized predictor policy
- bootstrap plan and confidence-interval expectations

---

## Recommended workflow

1. **Confirm scoring readiness**
   - verify that the analysis is using final scored variables or that scoring rules are frozen
   - stop if reverse-key handling or composite construction is still uncertain

2. **Lock the model roles**
   - record which variable is `X`, `M`, `Y`, and optional `W`
   - record any covariates
   - stop if one construct is playing multiple ambiguous roles

3. **Choose the PROCESSv50 framing**
   - decide whether the request is mediation, moderation, or a broader conditional process variant
   - document the exact model family the analyst intends to run in the local tool

4. **Record estimation decisions**
   - bootstrap settings
   - interval type / confidence level
   - centering / interaction construction rules when moderation is involved

5. **Interpret conservatively**
   - separate statistical evidence from causal language
   - surface limitations from cross-sectional design, measurement quality, or coding uncertainty

6. **Produce handoff artifacts**
   - analysis decision log
   - model specification note
   - reporting checklist
   - candidate artifact names such as `method-processv50.json` and `method-processv50.md`

---

## Failure checks

Stop or escalate when:

- `X`, `M`, `Y`, or `W` roles are ambiguous
- scale scoring is unverified
- reverse-key handling is unknown
- bootstrap / interval decisions are missing for an indirect-effect claim
- moderation is requested but centering / interaction construction rules are unspecified
- the project needs latent-variable modeling rather than an observed-variable PROCESS-style path

---

## What a future plugin would own

Only promote this into a stable external plugin when the team has already settled:

- canonical PROCESSv50 input schema
- canonical structured output schema
- artifact naming and downstream report contract
- error semantics for unsupported model families or bad variable-role assignments

Until then, keep it as a governed project skill.
