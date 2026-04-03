# Questionnaire SEM rehearsal

This document records an end-to-end rehearsal of the `project-skill` governance path for **questionnaire-based structural equation modeling (SEM)**.

The goal is not to claim that the repo already ships a finished SEM calculator. The goal is to prove that the repo can:

1. absorb SEM method notes into a governed skill
2. make **R/lavaan** the default execution recommendation
3. keep the workflow skill-first until the executable contract becomes stable enough for a plugin

---

## Why this rehearsal matters

A real questionnaire-analysis user may ask for things like:

- “帮我做一个 CFA 看量表结构稳不稳”
- “帮我用 SEM 看 stress、coping 和 burnout 的路径关系”
- “帮我做多组比较，看男女样本的结构路径是否不同”

Those are real research requests, but they are not yet one frozen built-in contract in this repo.

So the right near-term move is:

1. collect source materials
2. synthesize a governed SEM skill
3. validate it
4. promote it only after held-out validation is recorded
5. keep the method skill-first while defaulting execution to **R/lavaan**

---

## Source materials used

- [`./questionnaire-sem-notes.md`](./questionnaire-sem-notes.md)
- [`./research-method-standards.md`](./research-method-standards.md)
- [`./project-skill-synthesis.md`](./project-skill-synthesis.md)

External method references folded into the local note:

- lavaan overview — <https://lavaan.ugent.be/tutorial/>
- lavaan CFA tutorial — <https://lavaan.ugent.be/tutorial/cfa.html>
- lavaan SEM tutorial — <https://lavaan.ugent.be/tutorial/sem.html>
- lavaan multiple-groups tutorial — <https://lavaan.ugent.be/tutorial/groups.html>
- lavaan package overview — <https://lavaan.ugent.be/>

---

## Why this stayed skill-first

This exercise keeps questionnaire SEM in the **project skill** lane.

Reason:

- the repo does not yet have one frozen executable contract for every CFA / SEM / invariance request
- SEM requests vary in construct definitions, model syntax, diagnostics, and reporting expectations
- the workflow still needs method judgment before a plugin boundary becomes safe

So the current output is:

- a reusable governed SEM workflow skill
- an explicit **R/lavaan-first** execution preference
- not yet a bundled SEM method
- not yet a stable external plugin contract

---

## Rehearsal commands

### 1. Initialize the skill draft

```bash
cd rust
./target/debug/claw project-skill init questionnaire-sem-sop \
  --title "Questionnaire SEM SOP" \
  --description "Governed workflow for questionnaire-based CFA and SEM with an R/lavaan-first execution path." \
  --domain survey \
  --use-when "Use when a questionnaire project needs CFA, SEM, indirect effects, or multi-group invariance planning before a stable plugin contract exists." \
  --source ../docs/questionnaire-sem-notes.md \
  --source ../docs/research-method-standards.md \
  --source ../docs/project-skill-synthesis.md \
  --input-expectation "Scored questionnaire dataset or scoring-ready dataset plus codebook" \
  --input-expectation "Explicit construct definitions and indicator-to-factor mapping" \
  --workflow-step "Confirm construct definitions, scoring direction, reverse-key handling, and missing-data policy before modeling." \
  --workflow-step "Choose observed vs latent framing explicitly, and default SEM fitting to the R/lavaan path." \
  --workflow-step "Fit CFA before structural claims when latent constructs are central, then document fit indices, loadings, and warnings." \
  --workflow-step "For multi-group requests, record an invariance plan before comparing latent means or structural paths." \
  --output "Model specification note" \
  --output "Fit summary with warnings and standardized effects" \
  --output "Interpretation checklist for CFA/SEM/invariance conclusions" \
  --limit "This skill does not replace statistical review, publication judgment, or advanced SEM diagnostics." \
  --failure-check "Stop if construct-to-item mapping is ambiguous or reverse-key handling is unverified." \
  --failure-check "Stop if the project requests group comparisons without a measurement invariance plan." \
  --evaluation-example "Primary project example: stress, coping, and burnout modeled with latent CFA + SEM paths." \
  --evaluation-example "Held-out example: compare configural, metric, and scalar invariance across gender groups before interpreting structural differences." \
  --output-root ../examples/project-skills
```

### 2. Run non-blocking diagnostics

```bash
cd rust
./target/debug/claw project-skill doctor ../examples/project-skills/questionnaire-sem-sop
```

### 3. Run validation

```bash
cd rust
./target/debug/claw --output-format json project-skill validate ../examples/project-skills/questionnaire-sem-sop
```

### 4. Promote after held-out validation is marked passed

```bash
cd rust
./target/debug/claw --output-format json project-skill promote ../examples/project-skills/questionnaire-sem-sop \
  --to project \
  --held-out-validation passed
```

### 5. Re-validate the promoted skill

```bash
cd rust
./target/debug/claw project-skill validate ../examples/project-skills/questionnaire-sem-sop
```

---

## What this rehearsal proves

This gives the repo a concrete reference for:

- how SEM guidance becomes a governed skill
- how **R/lavaan** becomes the default execution recommendation without hard-coding a plugin too early
- how the repo can stage CFA / SEM / invariance capability growth in a reviewable way

---

## Follow-on decision rule

If repeated projects converge on one stable computation contract, the next step is **not** to keep bloating the skill.

The next step is to define an external plugin contract for the stable execution layer while keeping the skill focused on:

- method choice
- precondition checks
- interpretation limits
- reporting SOP
