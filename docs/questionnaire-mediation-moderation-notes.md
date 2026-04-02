# Questionnaire mediation & moderation notes

This note captures the minimum method guidance needed to synthesize a **project skill** for questionnaire-based mediation and moderation analysis.

It is intentionally framed as a **skill-first** capability: before we freeze a computation path into a plugin, the agent usually still needs to reason about construct definitions, scoring rules, model choice, covariates, and reporting boundaries.

---

## What this method family covers

### Mediation

Use mediation when the research question is whether an independent variable `X` relates to an outcome `Y` through an intervening mechanism `M`.

Typical questionnaire example:

- `X`: perceived stress
- `M`: coping style
- `Y`: burnout

Operationally, the agent should treat mediation as a workflow that requires:

1. clearly defined variable roles
2. defensible temporal or theoretical ordering
3. indirect-effect estimation and confidence intervals
4. explicit reporting of assumptions and limits

### Moderation

Use moderation when the research question is whether the relation between `X` and `Y` changes as a function of a moderator `W`.

Typical questionnaire example:

- `X`: perceived stress
- `W`: social support
- `Y`: wellbeing

Operationally, the workflow should require:

1. a theoretically justified moderator
2. an interaction specification such as `X × W`
3. probing and interpretation of conditional effects
4. explicit reporting of coding / centering / simple-slopes decisions

### Conditional process framing

Some projects need both, such as moderated mediation or mediated moderation. For now, this repo should treat those as **advanced extensions of the same workflow family**, but still guard them behind the same pre-analysis checks.

---

## Why this is skill-first before plugin-first

For questionnaire projects, the unstable part is often not the arithmetic. The unstable part is the **analysis contract**:

- which variables are latent vs observed
- whether scales were scored correctly
- whether reverse-keyed items were handled
- what missing-data rule was used
- whether the project wants regression-style estimation, SEM, or a GUI-backed workflow
- how the team wants to interpret borderline or inconsistent results

That makes this a strong fit for a project skill first:

- the skill captures SOP, decision rules, failure checks, and reporting expectations
- a later plugin can own stable executable computation once the contract is settled

---

## Expected inputs

A good mediation / moderation run should not start unless the project has at least:

- a scored questionnaire dataset or a scoring-ready dataset plus codebook
- clear variable-role definitions for `X`, `M`, `Y`, and optional `W`
- a stated hypothesis or analysis question
- documented covariates, if any
- a missing-data handling rule
- confirmation of item direction and reverse-key handling

Helpful but not always mandatory:

- reliability / validity pre-check results
- CFA or measurement-model evidence when latent constructs matter
- preregistration or analysis plan notes

---

## Recommended workflow

1. **Confirm scoring and coding**
   - verify item direction
   - verify reverse-key handling
   - verify scale aggregation rules
   - document missing-data handling

2. **Lock the model roles**
   - document which variable is `X`, `M`, `Y`, and `W`
   - stop if the same construct is playing multiple ambiguous roles

3. **Choose the estimation family**
   - PROCESS-style regression workflow when the project wants an observed-variable conditional process analysis
   - lavaan / SEM-style workflow when the project needs path modeling, latent-variable framing, or more explicit model syntax
   - JASP-style GUI workflow when the user needs a transparent teaching / review surface

4. **Specify the estimation details**
   - indirect-effect inference should be based on bootstrap confidence intervals when appropriate
   - moderation runs should explicitly record whether predictors were centered / standardized and how interactions were constructed
   - conditional effects should record the probing strategy (e.g. simple slopes / simple effects)

5. **Interpret conservatively**
   - report effect direction, interval estimates, and modeling choices
   - separate statistical evidence from causal claims
   - record limitations such as cross-sectional design, measurement concerns, or sample constraints

6. **Produce handoff artifacts**
   - decision log
   - model specification note
   - reporting checklist
   - plugin handoff candidate if the computation path is now stable enough to standardize

---

## Failure checks

Stop or escalate when:

- `X`, `M`, `Y`, or `W` roles are ambiguous
- scale scoring is unverified
- reverse-key handling is unknown
- the user mixes latent and observed workflows without an explicit modeling decision
- the project wants publication-grade inference but has no documented bootstrap / interaction-probing plan
- theoretical ordering is too weak to support a mediation interpretation

---

## What a future plugin would own

Only promote this workflow into a stable plugin when the team has already settled:

- canonical input schema
- canonical output schema
- exact model family / software path
- minimum diagnostics and error semantics
- repeatable reporting contract

Until then, keep it as a governed skill.

---

## Source references

Primary references used for this note:

1. PROCESS macro official site — https://processmacro.org/
2. PROCESS macro FAQ — https://processmacro.org/faq.html
3. lavaan mediation tutorial — https://lavaan.ugent.be/tutorial/mediation.html
4. lavaan estimators / bootstrap notes — https://lavaan.ugent.be/tutorial/est.html
5. JASP process analysis overview — https://jasp-stats.org/2020/11/26/performing-mediation-and-moderation-analyses-in-jasp/

These sources anchor the current project-skill rehearsal; they do **not** remove the need for project-specific statistical review.
