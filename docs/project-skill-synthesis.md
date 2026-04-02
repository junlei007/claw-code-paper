# Project Skill Synthesis

This document defines the **minimal executable path** for turning approved materials into a reusable project-level skill draft.

It is the first implementation step of the “controlled self-extension” direction defined in:

- [`./research-method-standards.md`](./research-method-standards.md)

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
  --source ../docs/research-method-registry.md
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
  --workflow-step "Review the approved materials and extract stable procedure steps." \
  --workflow-step "List required inputs, assumptions, and failure checks." \
  --output "Project-local SKILL.md draft" \
  --output "skill.json metadata"
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
```

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
- `verification_status`
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

## Relationship to external plugins

If the requested method is missing from the integrated registry:

1. start with a project skill if the task is mostly procedural
2. start with an external plugin if the task is mostly stable computation

See also:

- [`./research-method-registry.md`](./research-method-registry.md)
- [`./research-extension-demo.md`](./research-extension-demo.md)
- [`../examples/external-plugins/research-regression/README.md`](../examples/external-plugins/research-regression/README.md)
