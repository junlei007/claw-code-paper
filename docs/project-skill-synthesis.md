# Project Skill Synthesis

This document defines the **minimal executable path** for turning approved materials into a reusable project-level skill draft.

It is the first implementation step of the “controlled self-extension” direction defined in:

- [`./research-method-standards.md`](./research-method-standards.md)

---

## Goal

When a user provides papers, SOPs, codebooks, or internal notes, we should be able to quickly create a **project-local skill draft** without mutating the runtime or prematurely promoting the workflow into a plugin.

The current MVP does this with a scaffold command:

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

Generated default location:

```text
.claw/project-skills/<slug>/
```

Each generated skill draft contains:

- `SKILL.md`
- `skill.json`
- `README.md`

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
- [`../examples/external-plugins/research-regression/README.md`](../examples/external-plugins/research-regression/README.md)
