#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from string import Template


DEFAULT_OUTPUT_ROOT = Path(".claw/project-skills")
TEMPLATE_PATH = Path("templates/project-skill/SKILL.md.tmpl")
MATURITY_LEVELS = ("draft", "project", "published", "deprecated")


@dataclass
class SourceRecord:
    path: str
    exists: bool
    sha256: str | None


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
                    path=str(candidate.relative_to(workspace_root)),
                    exists=True,
                    sha256=hash_source(candidate),
                )
            )
        else:
            results.append(SourceRecord(path=item, exists=False, sha256=None))
    return results


def load_template(workspace_root: Path) -> Template:
    template_path = workspace_root / TEMPLATE_PATH
    return Template(template_path.read_text())


def bullet_block(values: list[str], fallback: str) -> str:
    if not values:
        return f"- {fallback}"
    return "\n".join(f"- {value}" for value in values)


def render_skill_markdown(args: argparse.Namespace, sources: list[SourceRecord], workspace_root: Path) -> str:
    template = load_template(workspace_root)
    source_lines = "\n".join(
        f"- `{source.path}`" + (f" (sha256: `{source.sha256}`)" if source.sha256 else " (reference only)")
        for source in sources
    )
    return template.substitute(
        skill_name=args.slug,
        skill_description=args.description,
        title=args.title,
        domain=args.domain,
        use_when=args.use_when,
        sources=source_lines,
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
        "input_expectations": [
            "Approved source materials",
            "A concrete analysis or workflow objective",
        ],
        "workflow": args.workflow_step
        or ["Review sources", "Extract stable procedure", "Validate draft", "Adapt locally"],
        "outputs": args.output or ["SKILL.md draft", "skill.json metadata"],
        "limits": args.limit
        or ["Draft only", "Not validated for publication", "Requires human review"],
        "verification_status": "drafted",
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
then promote the maturity level from `{args.maturity}` when validated.
"""
    (skill_root / "README.md").write_text(content)


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
    (skill_root / "SKILL.md").write_text(render_skill_markdown(args, sources, workspace_root))
    write_metadata(skill_root, args, sources)
    write_readme(skill_root, args)

    print(
        json.dumps(
            {
                "status": "ok",
                "skillRoot": str(skill_root.relative_to(workspace_root)),
                "generatedFiles": [
                    str((skill_root / "SKILL.md").relative_to(workspace_root)),
                    str((skill_root / "skill.json").relative_to(workspace_root)),
                    str((skill_root / "README.md").relative_to(workspace_root)),
                ],
                "maturity": args.maturity,
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    main()
