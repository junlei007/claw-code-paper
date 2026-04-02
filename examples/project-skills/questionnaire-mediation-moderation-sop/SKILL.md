---
name: questionnaire-mediation-moderation-sop
description: Governed workflow for questionnaire-based mediation and moderation analysis.
---

# Questionnaire Mediation & Moderation SOP

## Purpose

This project-level skill captures a reusable workflow in the **survey** domain.

## Use when

Use when a questionnaire project needs a mediation or moderation analysis plan before stable tooling is integrated.

## Inputs expected

- Scored questionnaire dataset with clear X/M/Y/W variable definitions
- A preregistered or explicitly stated mediation/moderation hypothesis

## Source materials

- `../docs/questionnaire-mediation-moderation-notes.md` (sha256: `157545aa19854a0a039a96a6568a8cbd8db697f6ee7d1d15babaa26e600a2e74`)
- `../docs/research-method-standards.md` (sha256: `8223885b1d76ee9c65da2fa3f3df0637957aece16f8fe723be303844a480d812`)
- `../docs/project-skill-synthesis.md` (sha256: `80be4a9cbcec69f2cf59753fee263b59eb138a85ccb9d4253cfb091377a05100`)

## Workflow

- Confirm scale scoring, coding direction, and missing-data handling before modeling.
- Choose mediation, moderation, or conditional process framing and document the model variables.
- Select an execution path such as PROCESS-style regression workflow or SEM/lavaan-style path modeling.
- Record bootstrap, interaction-probing, and reporting decisions explicitly.

## Expected outputs

- Analysis decision log
- Model specification note
- Interpretation checklist for indirect or interaction effects

## Limits

- This skill does not replace statistical review or publication judgment.

## Failure checks

- Stop if X, M, Y, or moderator roles are ambiguous.
- Stop if scale scoring or reverse-key handling is not verified.

## Evaluation examples

- Primary project example: questionnaire stress -> burnout via coping mediation.
- Held-out example: social support moderates stress -> wellbeing.

## Governance metadata

- generated_at: `2026-04-02T13:54:16Z`
- maturity: `project`
- verification: `held-out-validated`
- held_out_validation: `passed`

## Maintainer notes

- Keep interpretation guidance separate from executable tool contracts.
- Refresh source material provenance when the workflow changes materially.
- Promote stable computation into an external or bundled plugin only after the computation boundary stabilizes.
