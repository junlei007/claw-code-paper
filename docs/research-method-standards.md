# Research Method Standards

This document defines the operating norms for adding, exposing, and using research-analysis methods in this project.

Its purpose is to keep the system:
- understandable for users
- extensible for future method work
- safe enough for real research workflows
- stable without overloading the Rust core

---

## 1. Core architecture rule

Use the existing boundary model:

- **Rust core** = orchestration, config, runtime, tool routing
- **Plugin/tool layer** = stable executable analysis capabilities
- **Skill layer** = reusable workflow knowledge, SOPs, interpretation scaffolds, reporting structures
- **Python / R / external runtimes** = domain computation substrates

### Interpretation

Do **not** rush to add every new research method into the Rust core.

Prefer:
1. skill first for workflow knowledge
2. external plugin for reusable but specialized methods
3. bundled plugin only for core, high-reuse research capabilities

---

## 2. Skill vs plugin decision rule

### Make it a skill when it is mostly:
- workflow knowledge
- method guidance
- analysis SOP
- reporting structure
- retrieval-backed reusable instructions
- user-teachable domain practice

### Make it a plugin/tool when it is mostly:
- stable executable computation
- file IO
- structured artifact generation
- repeatable numeric/statistical procedure
- Python/R/Rust/external runtime logic with a durable interface

### Promotion rule

If a skill becomes:
- high-frequency
- stable in inputs and outputs
- clearly reusable across projects
- dependent on repeated structured computation

then it should be evaluated for promotion into a plugin/tool.

---

## 3. Method lifecycle classes

| Class | Meaning | Where it should live |
| --- | --- | --- |
| `integrated` | core supported capability | bundled plugin |
| `external` | reusable but not core | external plugin |
| `manual` | analyst-guided workflow only | skill / docs / prompt guidance |
| `planned` | roadmap only | registry + roadmap docs |

### Bundled plugin admission rule

A method should become a **bundled plugin** only if it is:
- central to the product direction
- broadly reusable across survey-research users
- stable enough to maintain a durable contract
- worth long-term support burden

### External plugin admission rule

A method should become an **external plugin** when it is:
- useful and reusable
- but too niche, unstable, or discipline-specific for the bundled core

### Manual-only rule

A method should remain **manual / skill-guided** when:
- demand is uncertain
- the workflow is still changing fast
- the interpretation burden is too high for a stable tool contract
- the actual value is primarily in process guidance, not computation

---

## 4. Artifact convention

Default artifact root:

```text
.claw/artifacts/
```

### Baseline recommended names for the survey chain

| Step | Recommended artifact |
| --- | --- |
| metadata | `metadata.json` |
| scoring | `scored.csv` |
| psychometrics | `psychometrics.json` |
| reporting | `report.md` |
| overall run summary | `analysis-manifest.json` |

### Naming rule for future methods

For method-specific outputs, prefer:
- `method-<name>.json`
- `method-<name>.csv`
- `method-<name>.md`

### Artifact quality rule

When a method succeeds, it should write or return enough structure so that downstream steps can consume it without guessing.

At minimum, a structured method output should define:
- what was run
- on which input dataset
- with what key parameters
- what outputs were produced
- what warnings or caveats apply

---

## 5. Research execution protocol

Every integrated research method should follow these norms:

1. **Prefer local data processing**
   - do not send raw datasets to remote models when local processing is available

2. **Separate computation from interpretation**
   - structured statistical output and narrative interpretation must stay distinct

3. **Emit warnings explicitly**
   - convergence issues
   - singularity
   - missing-data caveats
   - unsupported assumptions
   - invalid coding / item mismatch / schema mismatch

4. **Be reproducible**
   - inputs, key parameters, and outputs must be inspectable

5. **Write structured artifacts**
   - avoid purely ephemeral analysis when a stable artifact is possible

---

## 6. Interpretation boundary rule

The system may:
- compute
- summarize
- structure outputs
- draft reports

The system must not:
- blur statistical output with final domain conclusions
- imply publication-level certainty from exploratory or weakly supported results
- hide warnings or failed assumptions

Researcher judgment remains final.

---

## 7. Skill synthesis / controlled self-extension

This project should support **controlled self-extension**, not unconstrained self-mutation.

### Proposed maturity levels for generated skills

- `draft`
- `project`
- `published`
- `deprecated`

### Required metadata for generated skills

- source materials
- generated_at
- generated_by
- domain
- use_when
- input expectations
- workflow
- outputs
- limits
- verification status
- maturity level

### Promotion pipeline

1. user provides materials or approved references
2. system synthesizes a skill draft
3. validate structure and example usage
4. confirm local usefulness
5. save as project skill
6. optionally publish to a registry later

### Current scaffold command

The current MVP scaffold command is:

```bash
python3 tools/scaffold_project_skill.py ...
```

It generates a governed project-local draft under:

```text
.claw/project-skills/<slug>/
```

### Safety rule

Do not treat generated skills as trustworthy by default.

They must at least declare:
- where they came from
- what they are for
- what they do not cover
- whether they were actually validated

---

## 8. New method checklist

Before adding any new method, answer these questions:

1. Is this workflow knowledge or stable computation?
2. Should it be a skill, external plugin, or bundled plugin?
3. What are the required inputs?
4. What is the output schema?
5. What artifacts should be produced?
6. What backend is appropriate?
7. What warnings must be surfaced?
8. How will we verify it?
9. What are the limits of interpretation?

If these cannot be answered clearly, the method is not ready to become an integrated bundled capability.

---

## 9. Documentation rule

When a new integrated method is added, update all of the following:
- `docs/research-method-registry.md`
- the relevant plugin README
- onboarding README surfaces if the user workflow changes materially

Keep the docs light, but keep the contract explicit.
