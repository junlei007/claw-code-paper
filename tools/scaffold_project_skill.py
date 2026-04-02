#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from string import Template


DEFAULT_OUTPUT_ROOT = Path(".claw/project-skills")
REPO_ROOT = Path(__file__).resolve().parent.parent
TEMPLATE_PATH = REPO_ROOT / "templates/project-skill/SKILL.md.tmpl"
MATURITY_LEVELS = ("draft", "project", "published", "deprecated")
TARGETS = ("canonical", "openclaw", "claude-command", "claude-agent", "all")


@dataclass
class SourceRecord:
    path: str
    exists: bool
    sha256: str | None


@dataclass
class ExportRecord:
    target: str
    path: str


def utc_now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def slugify(raw: str) -> str:
    value = raw.strip().lower()
    out: list[str] = []
    prev_dash = False
    for ch in value:
        if ch.isalnum():
            out.append(ch)
            prev_dash = False
        elif ch in {"-", "_", " ", "/"}:
            if not prev_dash and out:
                out.append("-")
                prev_dash = True
    slug = "".join(out).strip("-")
    if not slug:
        raise SystemExit("skill slug cannot be empty")
    return slug


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Scaffold a project-level research skill draft from approved source materials."
    )
    parser.add_argument("--slug", required=True, help="Stable skill slug, e.g. survey-coding-sop")
    parser.add_argument("--title", required=True, help="Human-readable skill title")
    parser.add_argument("--description", required=True, help="Short description for the skill front matter")
    parser.add_argument("--domain", required=True, help="Domain label, e.g. survey, regression, psychometrics")
    parser.add_argument(
        "--use-when",
        required=True,
        help="Single sentence describing when the skill should be used",
    )
    parser.add_argument(
        "--source",
        action="append",
        required=True,
        help="Approved source material path or reference. Repeat for multiple sources.",
    )
    parser.add_argument(
        "--input-expectation",
        action="append",
        default=[],
        help="Expected input, prerequisite, or dependency for using this skill. Repeatable.",
    )
    parser.add_argument(
        "--workflow-step",
        action="append",
        default=[],
        help="Workflow step to seed into the generated draft. Repeatable.",
    )
    parser.add_argument(
        "--output",
        action="append",
        default=[],
        help="Expected output or artifact. Repeatable.",
    )
    parser.add_argument(
        "--limit",
        action="append",
        default=[],
        help="Known limit or caveat. Repeatable.",
    )
    parser.add_argument(
        "--failure-check",
        action="append",
        default=[],
        help="Failure check, warning gate, or validation condition. Repeatable.",
    )
    parser.add_argument(
        "--evaluation-example",
        action="append",
        default=[],
        help="Evaluation example or test case used to validate the skill. Repeatable.",
    )
    parser.add_argument(
        "--generated-by",
        default="tools/scaffold_project_skill.py",
        help="Generator identity recorded in metadata",
    )
    parser.add_argument(
        "--maturity",
        default="draft",
        choices=MATURITY_LEVELS,
        help="Initial maturity level",
    )
    parser.add_argument(
        "--output-root",
        default=str(DEFAULT_OUTPUT_ROOT),
        help="Directory where the skill draft should be generated",
    )
    parser.add_argument(
        "--target",
        action="append",
        default=[],
        choices=TARGETS,
        help="Compatibility target to generate. Repeatable. Defaults to canonical only.",
    )
    parser.add_argument(
        "--openclaw-root",
        default=".claw/compat/openclaw",
        help="Base directory for OpenClaw-compatible exports",
    )
    parser.add_argument(
        "--claude-root",
        default=".claw/compat/claude",
        help="Base directory for Claude-compatible exports",
    )
    return parser.parse_args()


def hash_source(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def collect_sources(raw_sources: list[str], workspace_root: Path) -> list[SourceRecord]:
    results: list[SourceRecord] = []
    for item in raw_sources:
        candidate = Path(item).expanduser()
        if not candidate.is_absolute():
            candidate = (workspace_root / candidate).resolve()
        if candidate.exists() and candidate.is_file():
            results.append(
                SourceRecord(
                    path=os.path.relpath(candidate, workspace_root),
                    exists=True,
                    sha256=hash_source(candidate),
                )
            )
        else:
            results.append(SourceRecord(path=item, exists=False, sha256=None))
    return results


def load_template() -> Template:
    return Template(TEMPLATE_PATH.read_text())


def bullet_block(values: list[str], fallback: str) -> str:
    if not values:
        return f"- {fallback}"
    return "\n".join(f"- {value}" for value in values)


def render_skill_markdown(args: argparse.Namespace, sources: list[SourceRecord], workspace_root: Path) -> str:
    template = load_template()
    source_lines = "\n".join(
        f"- `{source.path}`" + (f" (sha256: `{source.sha256}`)" if source.sha256 else " (reference only)")
        for source in sources
    )
    evaluation_examples = (
        args.evaluation_example
        or [
            "Primary project example: pending validation.",
            "Held-out example: pending validation.",
        ]
    )
    return template.substitute(
        skill_name=args.slug,
        skill_description=args.description,
        title=args.title,
        domain=args.domain,
        use_when=args.use_when,
        sources=source_lines,
        input_expectations=bullet_block(
            args.input_expectation,
            "Approved source materials plus a concrete research objective for this workflow.",
        ),
        workflow_steps=bullet_block(
            args.workflow_step,
            "Summarize the approved materials into a clear SOP before changing the workflow.",
        ),
        outputs=bullet_block(
            args.output,
            "A reusable project-level draft skill plus any structured artifacts the workflow requires.",
        ),
        limits=bullet_block(
            args.limit,
            "This draft is not auto-trusted; a human should review and validate it before publishing.",
        ),
        failure_checks=bullet_block(
            args.failure_check,
            "Stop and revise the draft if required inputs, assumptions, or method boundaries are unclear.",
        ),
        evaluation_examples=bullet_block(evaluation_examples, "Validation examples pending."),
        generated_at=utc_now(),
        maturity=args.maturity,
    )


def write_metadata(
    skill_root: Path,
    args: argparse.Namespace,
    sources: list[SourceRecord],
) -> None:
    metadata = {
        "name": args.slug,
        "title": args.title,
        "description": args.description,
        "domain": args.domain,
        "generated_at": utc_now(),
        "generated_by": args.generated_by,
        "use_when": args.use_when,
        "input_expectations": args.input_expectation
        or ["Approved source materials", "A concrete analysis or workflow objective"],
        "workflow": args.workflow_step
        or ["Review sources", "Extract stable procedure", "Validate draft", "Adapt locally"],
        "outputs": args.output or ["SKILL.md draft", "skill.json metadata"],
        "limits": args.limit
        or ["Draft only", "Not validated for publication", "Requires human review"],
        "failure_checks": args.failure_check
        or [
            "Required inputs and assumptions are explicit",
            "Warnings and method limits are stated before reuse",
        ],
        "evaluation_examples": args.evaluation_example
        or [
            "Primary project example: pending validation",
            "Held-out example: pending validation",
        ],
        "verification_status": "drafted",
        "held_out_validation_status": "pending",
        "maturity_level": args.maturity,
        "source_materials": [
            {
                "path": source.path,
                "exists": source.exists,
                "sha256": source.sha256,
            }
            for source in sources
        ],
    }
    (skill_root / "skill.json").write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + "\n")


def write_readme(skill_root: Path, args: argparse.Namespace) -> None:
    content = f"""# {args.title}

This directory was generated by `tools/scaffold_project_skill.py`.

## What is here

- `SKILL.md` — reusable draft workflow skill
- `skill.json` — structured metadata for controlled self-extension governance

## Next step

Review both files, replace placeholders with project-specific procedure details,
add validation evidence and failure checks, then promote the maturity level
from `{args.maturity}` when a held-out example has been reviewed.
"""
    (skill_root / "README.md").write_text(content)


def normalize_targets(raw_targets: list[str]) -> list[str]:
    if not raw_targets:
        return ["canonical"]
    normalized: list[str] = []
    for target in raw_targets:
        expanded = (
            ["canonical", "openclaw", "claude-command", "claude-agent"]
            if target == "all"
            else [target]
        )
        for item in expanded:
            if item not in normalized:
                normalized.append(item)
    return normalized


def write_compatibility_exports(
    *,
    args: argparse.Namespace,
    workspace_root: Path,
    canonical_skill_root: Path,
    rendered_skill_markdown: str,
) -> list[ExportRecord]:
    exports: list[ExportRecord] = []
    title = args.title
    description = args.description

    targets = normalize_targets(args.target)
    if "canonical" in targets:
        exports.extend(
            [
                ExportRecord(
                    target="canonical",
                    path=os.path.relpath(canonical_skill_root / "SKILL.md", workspace_root),
                ),
                ExportRecord(
                    target="canonical-metadata",
                    path=os.path.relpath(canonical_skill_root / "skill.json", workspace_root),
                ),
            ]
        )

    if "openclaw" in targets:
        openclaw_root = Path(args.openclaw_root).expanduser()
        if not openclaw_root.is_absolute():
            openclaw_root = (workspace_root / openclaw_root).resolve()
        target_root = openclaw_root / "skills" / args.slug
        target_root.mkdir(parents=True, exist_ok=True)
        (target_root / "SKILL.md").write_text(rendered_skill_markdown)
        exports.append(
            ExportRecord(
                target="openclaw",
                path=os.path.relpath(target_root / "SKILL.md", workspace_root),
            )
        )

    if "claude-command" in targets:
        claude_root = Path(args.claude_root).expanduser()
        if not claude_root.is_absolute():
            claude_root = (workspace_root / claude_root).resolve()
        command_path = claude_root / ".claude" / "commands" / f"{args.slug}.md"
        command_path.parent.mkdir(parents=True, exist_ok=True)
        command_path.write_text(
            f"""---
description: {description}
---

# {title}

Use this command when:

- {args.use_when}

## Source skill

- canonical skill: `{os.path.relpath(canonical_skill_root / "SKILL.md", workspace_root)}`

## Workflow

Follow this governed workflow draft:

{rendered_skill_markdown}
"""
        )
        exports.append(
            ExportRecord(
                target="claude-command",
                path=os.path.relpath(command_path, workspace_root),
            )
        )

    if "claude-agent" in targets:
        claude_root = Path(args.claude_root).expanduser()
        if not claude_root.is_absolute():
            claude_root = (workspace_root / claude_root).resolve()
        agent_path = claude_root / ".claude" / "agents" / f"{args.slug}.md"
        agent_path.parent.mkdir(parents=True, exist_ok=True)
        agent_path.write_text(
            f"""---
name: {args.slug}
description: {description}
---

# {title}

You are a specialized research workflow assistant for the **{args.domain}** domain.

## Use when

- {args.use_when}

## Operating rule

Start from the governed canonical skill and preserve its limits, warnings, and source attribution.

## Canonical source

- `{os.path.relpath(canonical_skill_root / "SKILL.md", workspace_root)}`
"""
        )
        exports.append(
            ExportRecord(
                target="claude-agent",
                path=os.path.relpath(agent_path, workspace_root),
            )
        )

    return exports


def main() -> None:
    args = parse_args()
    workspace_root = Path.cwd().resolve()
    slug = slugify(args.slug)
    output_root = Path(args.output_root).expanduser()
    if not output_root.is_absolute():
        output_root = (workspace_root / output_root).resolve()
    skill_root = output_root / slug
    skill_root.mkdir(parents=True, exist_ok=True)

    sources = collect_sources(args.source, workspace_root)
    rendered_skill_markdown = render_skill_markdown(args, sources, workspace_root)
    (skill_root / "SKILL.md").write_text(rendered_skill_markdown)
    write_metadata(skill_root, args, sources)
    write_readme(skill_root, args)
    exports = write_compatibility_exports(
        args=args,
        workspace_root=workspace_root,
        canonical_skill_root=skill_root,
        rendered_skill_markdown=rendered_skill_markdown,
    )

    print(
        json.dumps(
            {
                "status": "ok",
                "skillRoot": os.path.relpath(skill_root, workspace_root),
                "generatedFiles": [
                    os.path.relpath(skill_root / "SKILL.md", workspace_root),
                    os.path.relpath(skill_root / "skill.json", workspace_root),
                    os.path.relpath(skill_root / "README.md", workspace_root),
                ],
                "maturity": args.maturity,
                "exports": [
                    {"target": export.target, "path": export.path} for export in exports
                ],
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    main()
