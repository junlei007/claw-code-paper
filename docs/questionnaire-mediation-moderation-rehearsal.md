# Questionnaire mediation & moderation rehearsal

This document records a **real end-to-end rehearsal** of the new `project-skill` governance path using a common questionnaire-analysis request:

- mediation effect
- moderation effect
- conditional process framing when needed

The goal is not to claim that the repo already ships a finished mediation/moderation calculator. The goal is to prove that the agent can take source materials, synthesize a governed project skill, run the governance loop, and decide whether the workflow is still **skill-first** or ready for a plugin.

---

## Why this rehearsal matters

A questionnaire user will often ask for things like:

- “帮我看 stress 是否通过 coping 影响 burnout”
- “social support 会不会调节 stress 对 wellbeing 的影响”

Those are real research asks, but they are not yet a single stable built-in method in this repo.

So the right rehearsal is:

1. collect source materials
2. synthesize a project skill draft
3. run `doctor`
4. run `validate`
5. promote only after the held-out example requirement is satisfied
6. keep the method as a skill until the executable contract stabilizes

---

## Source materials used

- [`./questionnaire-mediation-moderation-notes.md`](./questionnaire-mediation-moderation-notes.md)
- [`./research-method-standards.md`](./research-method-standards.md)
- [`./project-skill-synthesis.md`](./project-skill-synthesis.md)

External method references folded into the local note:

- PROCESS macro — <https://processmacro.org/>
- PROCESS FAQ — <https://processmacro.org/faq.html>
- lavaan mediation tutorial — <https://lavaan.ugent.be/tutorial/mediation.html>
- lavaan estimators / bootstrap — <https://lavaan.ugent.be/tutorial/est.html>
- JASP process analysis overview — <https://jasp-stats.org/2020/11/26/performing-mediation-and-moderation-analyses-in-jasp/>

---

## Why this stayed skill-first

This exercise deliberately keeps mediation/moderation in the **project skill** lane.

Reason:

- the repo does not yet have one frozen executable contract for all mediation / moderation requests
- questionnaire projects vary in scoring, model framing, missing-data policy, and reporting expectations
- the workflow still needs a lot of SOP-level judgment before a plugin boundary becomes safe

So the current output is:

- a reusable governed workflow skill
- not yet a bundled method
- not yet a stable external plugin contract

---

## Rehearsal commands

### 1. Initialize the skill draft

```bash
cd rust
./target/debug/claw project-skill init questionnaire-mediation-moderation-sop \
  --title "Questionnaire Mediation & Moderation SOP" \
  --description "Governed workflow for questionnaire-based mediation and moderation analysis." \
  --domain survey \
  --use-when "Use when a questionnaire project needs a mediation or moderation analysis plan before stable tooling is integrated." \
  --source ../docs/questionnaire-mediation-moderation-notes.md \
  --source ../docs/research-method-standards.md \
  --source ../docs/project-skill-synthesis.md \
  --input-expectation "Scored questionnaire dataset with clear X/M/Y/W variable definitions" \
  --input-expectation "A preregistered or explicitly stated mediation/moderation hypothesis" \
  --workflow-step "Confirm scale scoring, coding direction, and missing-data handling before modeling." \
  --workflow-step "Choose mediation, moderation, or conditional process framing and document the model variables." \
  --workflow-step "Select an execution path such as PROCESS-style regression workflow or SEM/lavaan-style path modeling." \
  --workflow-step "Record bootstrap, interaction-probing, and reporting decisions explicitly." \
  --output "Analysis decision log" \
  --output "Model specification note" \
  --output "Interpretation checklist for indirect or interaction effects" \
  --limit "This skill does not replace statistical review or publication judgment." \
  --failure-check "Stop if X, M, Y, or moderator roles are ambiguous." \
  --failure-check "Stop if scale scoring or reverse-key handling is not verified." \
  --evaluation-example "Primary project example: questionnaire stress -> burnout via coping mediation." \
  --evaluation-example "Held-out example: social support moderates stress -> wellbeing." \
  --output-root ../examples/project-skills
```

### 2. Run non-blocking diagnostics

```bash
cd rust
./target/debug/claw project-skill doctor ../examples/project-skills/questionnaire-mediation-moderation-sop
```

### 3. Run machine-readable validation

```bash
cd rust
./target/debug/claw --output-format json project-skill validate ../examples/project-skills/questionnaire-mediation-moderation-sop
```

### 4. Promote after held-out validation is marked passed

```bash
cd rust
./target/debug/claw --output-format json project-skill promote ../examples/project-skills/questionnaire-mediation-moderation-sop \
  --to project \
  --held-out-validation passed
```

### 5. Re-validate the promoted skill

```bash
cd rust
./target/debug/claw project-skill validate ../examples/project-skills/questionnaire-mediation-moderation-sop
```

---

## What changed during governance

Observed governance behavior in this rehearsal:

- `doctor` reported 2 findings: the draft had not yet recorded a realistic validation run, and held-out validation was still pending
- `validate --output-format json` returned `result=valid`, but also surfaced 3 warnings: draft maturity, drafted verification, and pending held-out validation
- `promote --to project --held-out-validation passed` upgraded the skill to `maturity=project`, `verification_status=held-out-validated`, and `held_out_validation_status=passed`
- re-validation after promotion returned `Warnings none` and `Remediation - no action required`

That is the important product behavior for self-extension:

- the agent can grow capabilities
- but the growth is explicit, reviewable, and maturity-gated

---

## Output artifact produced by this rehearsal

Canonical example skill location:

- [`../examples/project-skills/questionnaire-mediation-moderation-sop/`](../examples/project-skills/questionnaire-mediation-moderation-sop/)

This example gives the repo a concrete reference point for future work:

- how questionnaire method notes become a governed skill
- how mediation/moderation stays skill-first before a plugin contract exists
- how JSON governance reports can be consumed by a future self-extension agent

---

## Follow-on decision rule

If repeated projects converge on the same computation path, the next step is **not** to bloat the skill.

The next step is to define an external plugin contract for the stable computation layer, while keeping the skill focused on:

- method choice
- precondition checks
- interpretation boundaries
- reporting SOP
