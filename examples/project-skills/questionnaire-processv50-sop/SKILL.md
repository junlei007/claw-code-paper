---
name: questionnaire-processv50-sop
description: Governed workflow for questionnaire-based PROCESSv50 conditional process analysis.
---

# Questionnaire PROCESSv50 SOP

## Purpose

This project-level skill captures a reusable workflow in the **survey** domain.

## Use when

Use when a questionnaire project needs a PROCESSv50-style mediation, moderation, or conditional process workflow before a stable plugin contract exists.

## Inputs expected

- Scored questionnaire dataset with explicit X/M/Y/W variable definitions
- A stated mediation, moderation, or conditional process hypothesis plus any covariates

## Source materials

- `../../../docs/processv50-notes.md` (reference only)
- `../../../docs/research-method-standards.md` (reference only)
- `../../../docs/project-skill-synthesis.md` (reference only)

## Workflow

- Confirm scoring readiness, reverse-key handling, and missing-data policy before modeling.
- Lock X/M/Y/W roles and document the intended PROCESSv50 model family.
- Record bootstrap, interval, centering, and interaction-construction decisions explicitly.
- Summarize findings conservatively and keep statistical output separate from causal claims.

## Expected outputs

- Analysis decision log
- PROCESSv50 model specification note
- Interpretation checklist for indirect, interaction, or conditional effects

## Limits

- This skill does not replace statistical review, publication judgment, or a stable executable plugin contract.
- Local PROCESSv50 materials should be treated as project-local references until redistribution / packaging boundaries are reviewed.

## Failure checks

- Stop if X/M/Y/W roles are ambiguous or reverse-key handling is unverified.
- Stop if bootstrap or interaction-probing decisions are missing for the requested inference.

## Evaluation examples

- Primary project example: perceived stress affects burnout via coping in a PROCESSv50 mediation workflow.
- Held-out example: social support moderates the stress -> wellbeing relation in a PROCESSv50-style moderation workflow.

## Governance metadata

- generated_at: `2026-04-04T03:28:32Z`
- maturity: `draft`
- verification: `drafted`
- held_out_validation: `pending`

## Maintainer notes

- Replace placeholders with stable project-specific instructions before broad reuse.
- Keep interpretation guidance separate from executable tool contracts.
- Promote to an external or bundled plugin only after the computation boundary stabilizes.
