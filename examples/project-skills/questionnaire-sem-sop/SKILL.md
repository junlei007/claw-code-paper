---
name: questionnaire-sem-sop
description: Governed workflow for questionnaire-based CFA and SEM with an R/lavaan-first execution path.
---

# Questionnaire SEM SOP

## Purpose

This project-level skill captures a reusable workflow in the **survey** domain.

## Use when

Use when a questionnaire project needs CFA, SEM, indirect effects, or multi-group invariance planning before a stable plugin contract exists.

## Inputs expected

- Scored questionnaire dataset or scoring-ready dataset plus codebook
- Explicit construct definitions and indicator-to-factor mapping

## Source materials

- `../docs/questionnaire-sem-notes.md` (sha256: `4e9b920bdf4d9b20fb0ff4e1598d5cbdcd8104d5354fed688305d078691e79e4`)
- `../docs/research-method-standards.md` (sha256: `8223885b1d76ee9c65da2fa3f3df0637957aece16f8fe723be303844a480d812`)
- `../docs/project-skill-synthesis.md` (sha256: `6625f3ee755659b62c4c341d5c060c241af25a14649432c89ac37e26b1098a3f`)

## Workflow

- Confirm construct definitions, scoring direction, reverse-key handling, and missing-data policy before modeling.
- Choose observed vs latent framing explicitly, and default SEM fitting to the R/lavaan path.
- Fit CFA before structural claims when latent constructs are central, then document fit indices, loadings, and warnings.
- For multi-group requests, record an invariance plan before comparing latent means or structural paths.

## Expected outputs

- Model specification note
- Fit summary with warnings and standardized effects
- Interpretation checklist for CFA/SEM/invariance conclusions

## Limits

- This skill does not replace statistical review, publication judgment, or advanced SEM diagnostics.

## Failure checks

- Stop if construct-to-item mapping is ambiguous or reverse-key handling is unverified.
- Stop if the project requests group comparisons without a measurement invariance plan.

## Evaluation examples

- Primary project example: stress, coping, and burnout modeled with latent CFA + SEM paths.
- Held-out example: compare configural, metric, and scalar invariance across gender groups before interpreting structural differences.

## Governance metadata

- generated_at: `2026-04-03T02:48:14Z`
- maturity: `project`
- verification: `held-out-validated`
- held_out_validation: `passed`

## Maintainer notes

- Keep interpretation guidance separate from executable tool contracts.
- Refresh source material provenance when the workflow changes materially.
- Promote stable computation into an external or bundled plugin only after the computation boundary stabilizes.
