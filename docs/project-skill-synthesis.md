# Project Skill Synthesis

This document defines the **minimal executable path** for turning approved materials into a reusable project-level skill draft.

It is the first implementation step of the “controlled self-extension” direction defined in:

- [`./research-method-standards.md`](./research-method-standards.md)

Recent agent-oriented wording in this document also draws on Anthropic’s engineering post, ["Writing effective tools for agents — with agents"](https://www.anthropic.com/engineering/writing-tools-for-agents), published September 11, 2025.

---

## Goal

When a user provides papers, SOPs, codebooks, or internal notes, we should be able to quickly create a **project-local skill draft** without mutating the runtime or prematurely promoting the workflow into a plugin.

The current MVP does this with either:

```bash
cd rust
./target/debug/claw project-skill init survey-cleaning-sop \
  --title "Survey Cleaning SOP" \
  --description "Draft workflow for survey data cleaning and coding checks." \
  --domain survey \
  --use-when "Use when a project needs a repeatable local workflow for questionnaire cleaning before scoring." \
  --source ../docs/research-method-standards.md \
  --source ../docs/research-method-registry.md \
  --input-expectation "Approved questionnaire codebook" \
  --failure-check "Stop if reverse-keyed items are ambiguous" \
  --evaluation-example "Held-out pilot dataset walkthrough"
```

or the lower-level scaffold script:

```bash
python3 tools/scaffold_project_skill.py \
  --slug survey-cleaning-sop \
  --title "Survey Cleaning SOP" \
  --description "Draft workflow for survey data cleaning and coding checks." \
  --domain survey \
  --use-when "Use when a project needs a repeatable local workflow for questionnaire cleaning before scoring." \
  --source docs/research-method-standards.md \
  --source docs/research-method-registry.md \
  --input-expectation "Approved questionnaire codebook" \
  --workflow-step "Review the approved materials and extract stable procedure steps." \
  --workflow-step "List required inputs, assumptions, and failure checks." \
  --output "Project-local SKILL.md draft" \
  --output "skill.json metadata" \
  --failure-check "Stop if reverse-keyed items are ambiguous" \
  --evaluation-example "Held-out pilot dataset walkthrough"
```

Generated default location depends on where you invoke the scaffold:

- `cd rust && ./target/debug/claw ...` → `rust/.claw/project-skills/<slug>/`
- `python3 tools/scaffold_project_skill.py ...` from repo root → `.claw/project-skills/<slug>/`

Each generated skill draft contains:

- `SKILL.md`
- `skill.json`
- `README.md`

Quick validation:

```bash
cd rust
./target/debug/claw project-skill validate ./.claw/project-skills/survey-cleaning-sop
./target/debug/claw project-skill doctor ./.claw/project-skills/survey-cleaning-sop
```

`validate` 现在除了 pass/fail 之外，还会给出：

- 当前 maturity / verification / held-out 状态
- warning 数量与具体 warning
- 下一步 remediation 建议

`doctor` 则更偏非阻断诊断，用来补充：

- scaffold 痕迹是否还太重
- source materials 是否有 unresolved reference
- evaluation examples 是否还停留在 placeholder
- outputs 是否还是过于泛化

如果你想让 agent 机器读取这些治理结果，可以直接用 JSON：

```bash
./target/debug/claw --output-format json project-skill validate ./.claw/project-skills/survey-cleaning-sop
./target/debug/claw --output-format json project-skill doctor ./.claw/project-skills/survey-cleaning-sop
./target/debug/claw --output-format json project-skill promote ./.claw/project-skills/survey-cleaning-sop \
  --to project \
  --held-out-validation passed
```

Promotion example:

```bash
./target/debug/claw project-skill promote ./.claw/project-skills/survey-cleaning-sop \
  --to project \
  --held-out-validation passed
```

The promote command updates metadata and re-runs the same governance gates used by `validate`.

---

## Compatibility export model

Automatic skill synthesis should **not** assume that every ecosystem uses the same file shape.

Current strategy:

1. keep one **canonical** internal draft
2. export compatibility adapters for other ecosystems

### Canonical draft

- `.claw/project-skills/<slug>/SKILL.md`
- `.claw/project-skills/<slug>/skill.json`

### OpenClaw-compatible export

OpenClaw-style workflow skills are exported as:

```text
<openclaw-root>/skills/<slug>/SKILL.md
```

### Claude-compatible exports

Claude compatibility is split by intent:

- **workflow / SOP / reusable procedure** → `.claude/commands/<slug>.md`
- **specialist / persona / method assistant** → `.claude/agents/<slug>.md`

### Example

```bash
cd rust
./target/debug/claw project-skill init survey-cleaning-sop \
  --title "Survey Cleaning SOP" \
  --description "Draft workflow for local survey cleaning." \
  --domain survey \
  --use-when "Use before scoring." \
  --source ../docs/research-method-standards.md \
  --target openclaw \
  --target claude-command \
  --target claude-agent \
  --openclaw-root .. \
  --claude-root ..
```

That produces:

```text
../skills/survey-cleaning-sop/SKILL.md
../.claude/commands/survey-cleaning-sop.md
../.claude/agents/survey-cleaning-sop.md
```

---

## Why this is a skill first

Use this path when the value is still mainly in:

- workflow knowledge
- method guidance
- reusable SOP
- interpretation scaffolding
- user-provided or retrieval-backed materials

Do **not** use this scaffold as a substitute for a stable executable method contract.

If the workflow becomes stable, computation-heavy, and reusable across projects, evaluate promotion into an external plugin.

---

## Metadata contract

The generated `skill.json` follows the governance fields from the standards doc:

- `source_materials`
- `generated_at`
- `generated_by`
- `domain`
- `use_when`
- `input_expectations`
- `workflow`
- `outputs`
- `limits`
- `failure_checks`
- `evaluation_examples`
- `verification_status`
- `held_out_validation_status`
- `maturity_level`

Compatibility exports are adapters around this canonical metadata, not replacements for it.

Current maturity options:

- `draft`
- `project`
- `published`
- `deprecated`

---

## Review rule

Generated project skills are **not auto-trusted**.

Before broader reuse:

1. review the extracted procedure
2. fill in missing project details
3. validate the workflow on a concrete example
4. only then promote from `draft` to `project`

---

## Agent-oriented quality rules

When skill drafts are synthesized from materials, the goal is not to create the largest possible registry. The goal is to create a **small number of agent-usable workflow contracts**.

### Prefer fewer, clearer skills

Do not create a new skill if the draft is only:

- a thin wrapper around one low-level step
- mostly duplicated with an existing skill
- missing a real research workflow boundary

Prefer one higher-signal skill for a complete SOP over many overlapping micro-skills.

### Name skills so another agent can pick them correctly

Use slugs and titles that expose both:

- the research domain or method family
- the concrete action or workflow

Good patterns:

- `survey-cleaning-sop`
- `regression-diagnostics-checklist`
- `interview-coding-consistency-review`

### Optimize for usable context, not maximum detail

The canonical draft can remain rich, but the top of the skill should make these items easy to recover:

- what problem this skill solves
- when to use it
- what inputs it expects
- what outputs/artifacts it should produce
- what warnings or limits apply

If a detail does not help the next agent act correctly, it should not dominate the draft.

### Validate with realistic examples

Before promoting a draft, test it on at least one realistic project example and ideally one held-out example that was not used as synthesis source material. This reduces the risk of polishing the skill around only its training materials.

### Keep failure guidance explicit

When the draft is incomplete or risky, say so directly. Prefer actionable warnings and missing-information checks over vague caveats.

Use this checklist when deciding whether a draft is ready to promote:

- [`./self-extension-evaluation-checklist.md`](./self-extension-evaluation-checklist.md)

---

## Relationship to external plugins

If the requested method is missing from the integrated registry:

1. start with a project skill if the task is mostly procedural
2. start with an external plugin if the task is mostly stable computation

See also:

- [`./research-method-registry.md`](./research-method-registry.md)
- [`./self-extension-evaluation-checklist.md`](./self-extension-evaluation-checklist.md)
- [`./research-extension-demo.md`](./research-extension-demo.md)
- [`../examples/external-plugins/research-regression/README.md`](../examples/external-plugins/research-regression/README.md)
