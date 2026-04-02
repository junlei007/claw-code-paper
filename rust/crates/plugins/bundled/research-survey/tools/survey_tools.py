#!/usr/bin/env python3
from __future__ import annotations

import csv
import json
import os
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import pandas as pd
    from pandas.api.types import (
        is_bool_dtype,
        is_datetime64_any_dtype,
        is_numeric_dtype,
        is_string_dtype,
    )
except Exception:  # pragma: no cover - environment-specific import failure
    pd = None
    is_bool_dtype = is_datetime64_any_dtype = is_numeric_dtype = is_string_dtype = None

try:
    import pyreadstat
except Exception:  # pragma: no cover - environment-specific import failure
    pyreadstat = None

SUPPORTED_FORMATS = {"csv", "tsv", "xlsx", "sav", "dat"}
SAMPLE_VALUES_LIMIT = 5
MISSINGNESS_LIMIT = 20


@dataclass
class ToolInvocationError(Exception):
    message: str
    code: str = "tool_error"
    details: dict[str, Any] | None = None


def load_payload() -> dict[str, Any]:
    raw = sys.stdin.read().strip() or os.environ.get("CLAW_TOOL_INPUT", "{}")
    try:
        value = json.loads(raw)
    except Exception as exc:  # pragma: no cover - malformed stdin
        raise ToolInvocationError(
            message=f"invalid JSON tool input: {exc}",
            code="invalid_input",
            details={"raw": raw[:500]},
        ) from exc
    return value if isinstance(value, dict) else {"value": value}


def emit(payload: dict[str, Any]) -> None:
    print(json.dumps(payload, ensure_ascii=False, indent=None, default=json_default))


def json_default(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    if pd is not None:
        if isinstance(value, pd.Timestamp):
            return value.isoformat()
        if pd.isna(value):
            return None
    if isinstance(value, Path):
        return str(value)
    if hasattr(value, "item"):
        try:
            return value.item()
        except Exception:  # pragma: no cover - defensive
            pass
    return str(value)


def plugin_context() -> tuple[str, str, Path, Path]:
    plugin_id = os.environ.get("CLAW_PLUGIN_ID", "unknown")
    tool_name = os.environ.get("CLAW_TOOL_NAME", "unknown")
    plugin_root = Path(os.environ.get("CLAW_PLUGIN_ROOT", ".")).resolve()
    workspace_root = Path(os.environ.get("CLAW_WORKSPACE_ROOT", plugin_root)).resolve()
    return plugin_id, tool_name, plugin_root, workspace_root


def resolve_existing_path(raw_path: str, workspace_root: Path, plugin_root: Path) -> Path:
    path = Path(raw_path).expanduser()
    if path.is_absolute():
        return path
    workspace_candidate = (workspace_root / path).resolve()
    if workspace_candidate.exists():
        return workspace_candidate
    return (plugin_root / path).resolve()


def resolve_output_path(raw_path: str, workspace_root: Path) -> Path:
    path = Path(raw_path).expanduser()
    if path.is_absolute():
        return path
    return (workspace_root / path).resolve()


def detect_format(dataset_path: Path, requested_format: Any) -> str:
    if isinstance(requested_format, str) and requested_format.strip():
        normalized = requested_format.strip().lower()
        if normalized != "auto":
            return normalized
    suffix = dataset_path.suffix.lower().lstrip(".")
    if suffix in SUPPORTED_FORMATS:
        return suffix
    if suffix == "xls":
        return "xlsx"
    raise ToolInvocationError(
        message=f"unsupported dataset format for {dataset_path.name}",
        code="unsupported_format",
        details={"supportedFormats": sorted(SUPPORTED_FORMATS)},
    )


def require_pandas() -> None:
    if pd is None:
        raise ToolInvocationError(
            message="pandas is required for survey dataset ingestion",
            code="missing_dependency",
            details={"dependency": "pandas"},
        )


def infer_delimiter(dataset_path: Path, fmt: str, payload: dict[str, Any]) -> str:
    delimiter = payload.get("delimiter")
    if isinstance(delimiter, str) and delimiter:
        return delimiter
    if fmt == "tsv":
        return "\t"
    if fmt == "csv":
        return ","
    with dataset_path.open(
        "r",
        encoding=payload.get("encoding") or "utf-8-sig",
        newline="",
    ) as handle:
        sample = handle.read(4096)
    try:
        dialect = csv.Sniffer().sniff(sample, delimiters=",\t;|")
        return dialect.delimiter
    except Exception:
        return ","


def load_dataframe(
    dataset_path: Path,
    fmt: str,
    payload: dict[str, Any],
) -> tuple[Any, dict[str, str], list[str]]:
    require_pandas()
    warnings: list[str] = []
    labels: dict[str, str] = {}
    na_values = payload.get("naValues")

    if fmt in {"csv", "tsv", "dat"}:
        if fmt == "dat" and payload.get("layout") == "fixed-width":
            df = pd.read_fwf(dataset_path, na_values=na_values)
        else:
            df = pd.read_csv(
                dataset_path,
                sep=infer_delimiter(dataset_path, fmt, payload),
                encoding=payload.get("encoding") or "utf-8-sig",
                na_values=na_values,
            )
    elif fmt == "xlsx":
        df = pd.read_excel(
            dataset_path,
            sheet_name=payload.get("sheetName", 0),
            na_values=na_values,
        )
    elif fmt == "sav":
        if pyreadstat is None:
            raise ToolInvocationError(
                message="pyreadstat is required to read SPSS .sav files",
                code="missing_dependency",
                details={"dependency": "pyreadstat"},
            )
        df, meta = pyreadstat.read_sav(dataset_path, apply_value_formats=False)
        labels = {
            str(name): label
            for name, label in getattr(meta, "column_names_to_labels", {}).items()
            if label
        }
        warnings.extend(extract_sav_warnings(meta))
    else:
        raise ToolInvocationError(
            message=f"format {fmt} is not yet supported",
            code="unsupported_format",
        )

    df.columns = [str(column) for column in df.columns]
    return df, labels, warnings


def extract_sav_warnings(meta: Any) -> list[str]:
    warnings: list[str] = []
    missing_ranges = getattr(meta, "missing_ranges", None)
    if missing_ranges:
        warnings.append(
            "SPSS user-missing ranges were detected; verify downstream missing-value handling."
        )
    variable_measure = getattr(meta, "variable_measure", None)
    if variable_measure:
        nominal_count = sum(1 for value in variable_measure.values() if value == "nominal")
        if nominal_count:
            warnings.append(f"SPSS metadata marks {nominal_count} variables as nominal.")
    return warnings


def summarize_dataframe(
    df: Any,
    dataset_path: Path,
    fmt: str,
    payload: dict[str, Any],
    labels: dict[str, str],
    warnings: list[str],
    plugin_root: Path,
) -> dict[str, Any]:
    row_count = int(len(df.index))
    column_names = [str(column) for column in df.columns]
    missing_by_column = []
    columns = []
    numeric_columns: list[str] = []
    categorical_columns: list[str] = []
    datetime_columns: list[str] = []

    for column_name in column_names:
        series = df[column_name]
        missing_count = int(series.isna().sum())
        non_missing = series.dropna()
        unique_non_missing = int(non_missing.nunique()) if len(non_missing.index) else 0
        column_role = infer_column_role(series)
        if column_role == "numeric":
            numeric_columns.append(column_name)
        elif column_role == "datetime":
            datetime_columns.append(column_name)
        else:
            categorical_columns.append(column_name)

        sample_values = [
            json_default(value)
            for value in non_missing.astype(object).head(SAMPLE_VALUES_LIMIT).tolist()
        ]
        column_summary = {
            "name": column_name,
            "label": labels.get(column_name),
            "role": column_role,
            "dtype": str(series.dtype),
            "missingCount": missing_count,
            "missingRate": round((missing_count / row_count), 4) if row_count else 0.0,
            "uniqueNonMissing": unique_non_missing,
            "sampleValues": sample_values,
        }
        if column_role == "numeric" and len(non_missing.index):
            numeric_series = pd.to_numeric(non_missing, errors="coerce").dropna()
            if len(numeric_series.index):
                column_summary["numericSummary"] = {
                    "min": json_default(numeric_series.min()),
                    "max": json_default(numeric_series.max()),
                    "mean": round(float(numeric_series.mean()), 4),
                    "std": round(float(numeric_series.std(ddof=1)), 4)
                    if len(numeric_series.index) > 1
                    else None,
                }
        missing_by_column.append(
            {
                "name": column_name,
                "missingCount": missing_count,
                "missingRate": round((missing_count / row_count), 4)
                if row_count
                else 0.0,
            }
        )
        columns.append(column_summary)

    missing_by_column.sort(key=lambda item: (-item["missingCount"], item["name"]))
    total_cells = row_count * len(column_names)
    missing_cells = int(df.isna().sum().sum())
    questionnaire = questionnaire_summary(payload, column_names)
    warnings.extend(questionnaire.pop("warnings"))

    return {
        "status": "ok",
        "dataset": {
            "path": str(dataset_path),
            "exists": dataset_path.exists(),
            "format": fmt,
            "rows": row_count,
            "columns": len(column_names),
            "workspaceRelativePath": workspace_relative_path(dataset_path),
        },
        "schema": {
            "columnNames": column_names,
            "numericColumns": numeric_columns,
            "categoricalColumns": categorical_columns,
            "datetimeColumns": datetime_columns,
        },
        "missingness": {
            "totalCells": total_cells,
            "missingCells": missing_cells,
            "missingRate": round((missing_cells / total_cells), 4) if total_cells else 0.0,
            "topColumns": missing_by_column[:MISSINGNESS_LIMIT],
        },
        "questionnaire": questionnaire,
        "columnSummaries": columns,
        "backendHints": {
            "ingest": "python",
            "psychometrics": "r",
            "reporting": "markdown/quarto",
            "recommendedNext": [
                "review coding and reverse-keyed items",
                "run reliability (alpha / omega)",
                "run validity pre-checks (KMO / Bartlett / EFA as needed)",
                "fit CFA in R/lavaan and inspect fit indices",
            ],
            "rContract": str(plugin_root / "contracts" / "r-psychometrics.md"),
        },
        "warnings": warnings,
    }


def questionnaire_summary(
    payload: dict[str, Any],
    column_names: list[str],
) -> dict[str, Any]:
    scale_definitions = (
        payload.get("scaleDefinitions")
        if isinstance(payload.get("scaleDefinitions"), list)
        else []
    )
    top_level_reverse = normalize_string_list(payload.get("reverseItems"))
    declared_items: list[str] = []
    scale_summaries = []
    reverse_items = set(top_level_reverse)
    warnings: list[str] = []

    for index, raw_scale in enumerate(scale_definitions, start=1):
        if not isinstance(raw_scale, dict):
            warnings.append(
                f"scaleDefinitions[{index - 1}] is not an object and was ignored."
            )
            continue
        items = normalize_string_list(raw_scale.get("items"))
        scale_reverse = normalize_string_list(raw_scale.get("reverseItems"))
        reverse_items.update(scale_reverse)
        declared_items.extend(items)
        scale_summaries.append(
            {
                "name": raw_scale.get("name") or f"scale_{index}",
                "itemCount": len(items),
                "items": items,
                "reverseItems": scale_reverse,
                "missingItems": [item for item in items if item not in column_names],
            }
        )

    missing_declared_items = sorted(
        {item for item in declared_items if item not in column_names}
    )
    unresolved_reverse_items = sorted(
        item for item in reverse_items if item not in column_names
    )
    return {
        "scaleCount": len(scale_summaries),
        "scaleDefinitions": scale_summaries,
        "declaredItemCount": len(set(declared_items)),
        "reverseItemCount": len(reverse_items),
        "reverseItems": sorted(reverse_items),
        "missingDeclaredItems": missing_declared_items,
        "unresolvedReverseItems": unresolved_reverse_items,
        "warnings": warnings
        + (
            [
                f"{len(missing_declared_items)} declared scale items were not found in the dataset."
            ]
            if missing_declared_items
            else []
        )
        + (
            [
                f"{len(unresolved_reverse_items)} reverse-coded items were not found in the dataset."
            ]
            if unresolved_reverse_items
            else []
        ),
    }


def normalize_string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    return [str(item) for item in value if str(item).strip()]


def normalize_scale_definitions(payload: dict[str, Any]) -> tuple[list[dict[str, Any]], list[str]]:
    raw_scales = payload.get("scaleDefinitions")
    if not isinstance(raw_scales, list):
        return [], []

    scales: list[dict[str, Any]] = []
    warnings: list[str] = []
    for index, raw_scale in enumerate(raw_scales, start=1):
        if not isinstance(raw_scale, dict):
            warnings.append(
                f"scaleDefinitions[{index - 1}] is not an object and was ignored."
            )
            continue

        name = str(raw_scale.get("name") or f"scale_{index}").strip() or f"scale_{index}"
        items = normalize_string_list(raw_scale.get("items"))
        reverse_items = normalize_string_list(raw_scale.get("reverseItems"))
        scoring = raw_scale.get("scoring") if isinstance(raw_scale.get("scoring"), dict) else {}
        method = str(scoring.get("method") or raw_scale.get("method") or "mean").strip().lower()
        if method not in {"mean", "sum"}:
            warnings.append(
                f"Scale {name} requested unsupported scoring method {method!r}; defaulted to 'mean'."
            )
            method = "mean"

        min_valid_items_raw = scoring.get("minValidItems", raw_scale.get("minValidItems"))
        min_valid_items = None
        if min_valid_items_raw is not None:
            try:
                min_valid_items = max(1, int(min_valid_items_raw))
            except Exception:
                warnings.append(
                    f"Scale {name} has invalid minValidItems={min_valid_items_raw!r}; it was ignored."
                )

        output_column = str(
            raw_scale.get("outputColumn") or scoring.get("outputColumn") or f"{name}_score"
        ).strip() or f"{name}_score"

        scales.append(
            {
                "name": name,
                "items": items,
                "reverseItems": reverse_items,
                "method": method,
                "minValidItems": min_valid_items,
                "outputColumn": output_column,
            }
        )

    return scales, warnings


def normalize_number(value: Any) -> float | None:
    if value is None or value == "":
        return None
    try:
        return float(value)
    except Exception:
        return None


def reverse_score_series(series: Any, response_min: float | None, response_max: float | None) -> tuple[Any, str | None]:
    observed = series.dropna()
    if observed.empty:
        return series, None

    low = response_min if response_min is not None else normalize_number(observed.min())
    high = response_max if response_max is not None else normalize_number(observed.max())
    if low is None or high is None:
        return series, "Reverse-scoring bounds could not be determined from observed item values."

    warning = None
    if response_min is None or response_max is None:
        warning = "Reverse-scoring bounds were inferred from observed item values."
    return (low + high) - series, warning


def build_scale_scores(
    df: Any,
    payload: dict[str, Any],
) -> tuple[Any, list[dict[str, Any]], list[str]]:
    scales, scale_warnings = normalize_scale_definitions(payload)
    if not scales:
        raise ToolInvocationError(
            message="scaleDefinitions is required for survey_score",
            code="missing_scale_definitions",
        )

    response_scale = payload.get("responseScale") if isinstance(payload.get("responseScale"), dict) else {}
    response_min = normalize_number(response_scale.get("min"))
    response_max = normalize_number(response_scale.get("max"))
    top_level_reverse = set(normalize_string_list(payload.get("reverseItems")))
    scored_df = df.copy()
    scoring_results: list[dict[str, Any]] = []
    warnings = list(scale_warnings)

    for scale in scales:
        name = scale["name"]
        items = scale["items"]
        reverse_items = sorted(top_level_reverse.union(scale["reverseItems"]))
        output_column = scale["outputColumn"]
        method = scale["method"]
        missing_items = [item for item in items if item not in scored_df.columns]
        usable_items = [item for item in items if item in scored_df.columns]

        if not usable_items:
            warnings.append(f"Scale {name} skipped because none of its declared items were found.")
            scoring_results.append(
                {
                    "name": name,
                    "outputColumn": output_column,
                    "method": method,
                    "items": items,
                    "usableItems": [],
                    "missingItems": missing_items,
                    "rowsScored": 0,
                    "warning": "No declared items were present in the dataset.",
                }
            )
            continue

        numeric_frame = scored_df[usable_items].apply(pd.to_numeric, errors="coerce")
        reverse_warnings: list[str] = []
        applied_reverse_items: list[str] = []
        for item in reverse_items:
            if item not in numeric_frame.columns:
                continue
            numeric_frame[item], reverse_warning = reverse_score_series(
                numeric_frame[item], response_min, response_max
            )
            applied_reverse_items.append(item)
            if reverse_warning:
                reverse_warnings.append(f"{reverse_warning} ({item})")

        valid_counts = numeric_frame.notna().sum(axis=1)
        min_valid_items = scale["minValidItems"] or len(usable_items)
        min_valid_items = min(min_valid_items, len(usable_items))
        if method == "sum":
            score_series = numeric_frame.sum(axis=1, min_count=1)
        else:
            score_series = numeric_frame.mean(axis=1)
        score_series = score_series.mask(valid_counts < min_valid_items)
        scored_df[output_column] = score_series

        non_missing_scores = score_series.dropna()
        scoring_results.append(
            {
                "name": name,
                "outputColumn": output_column,
                "method": method,
                "items": items,
                "usableItems": usable_items,
                "missingItems": missing_items,
                "reverseItemsApplied": applied_reverse_items,
                "minValidItems": min_valid_items,
                "rowsScored": int(non_missing_scores.shape[0]),
                "missingScoreCount": int(score_series.isna().sum()),
                "summary": {
                    "mean": round(float(non_missing_scores.mean()), 4) if not non_missing_scores.empty else None,
                    "std": round(float(non_missing_scores.std(ddof=1)), 4)
                    if non_missing_scores.shape[0] > 1
                    else None,
                    "min": json_default(non_missing_scores.min()) if not non_missing_scores.empty else None,
                    "max": json_default(non_missing_scores.max()) if not non_missing_scores.empty else None,
                },
                "warnings": reverse_warnings
                + (
                    [f"{len(missing_items)} declared items were not found."]
                    if missing_items
                    else []
                ),
            }
        )
        warnings.extend(f"Scale {name}: {warning}" for warning in reverse_warnings)
        if missing_items:
            warnings.append(f"Scale {name}: {len(missing_items)} declared items were not found.")

    return scored_df, scoring_results, warnings


def write_dataframe_artifact(df: Any, output_path: Path) -> dict[str, Any]:
    suffix = output_path.suffix.lower()
    output_path.parent.mkdir(parents=True, exist_ok=True)
    if suffix == ".xlsx":
        df.to_excel(output_path, index=False)
        kind = "xlsx"
    else:
        sep = "\t" if suffix == ".tsv" else ","
        df.to_csv(output_path, index=False, sep=sep)
        kind = "tsv" if sep == "\t" else "csv"

    return {
        "path": str(output_path),
        "workspaceRelativePath": workspace_relative_path(output_path),
        "kind": kind,
    }


def run_scoring(
    payload: dict[str, Any],
    plugin_root: Path,
    workspace_root: Path,
) -> dict[str, Any]:
    raw_dataset_path = payload.get("datasetPath")
    if not isinstance(raw_dataset_path, str) or not raw_dataset_path.strip():
        raise ToolInvocationError(
            message="datasetPath is required for survey_score",
            code="missing_dataset_path",
        )

    dataset_path = resolve_existing_path(raw_dataset_path, workspace_root, plugin_root)
    if not dataset_path.exists():
        raise ToolInvocationError(
            message=f"dataset not found: {dataset_path}",
            code="dataset_not_found",
            details={"datasetPath": raw_dataset_path, "resolvedPath": str(dataset_path)},
        )

    fmt = detect_format(dataset_path, payload.get("format"))
    dataframe, labels, warnings = load_dataframe(dataset_path, fmt, payload)
    scored_df, scoring_results, scoring_warnings = build_scale_scores(dataframe, payload)
    warnings.extend(scoring_warnings)

    id_columns = normalize_string_list(payload.get("idColumns"))
    preview_columns = [column for column in id_columns if column in scored_df.columns]
    preview_columns.extend(
        result["outputColumn"]
        for result in scoring_results
        if result["outputColumn"] in scored_df.columns
    )
    if not preview_columns:
        preview_columns = [result["outputColumn"] for result in scoring_results if result["outputColumn"] in scored_df.columns]

    preview = (
        scored_df[preview_columns].head(5).replace({pd.NA: None}).to_dict(orient="records")
        if preview_columns
        else []
    )

    artifact = None
    output_path_raw = payload.get("outputPath")
    if isinstance(output_path_raw, str) and output_path_raw.strip():
        output_path = resolve_output_path(output_path_raw, workspace_root)
        artifact = write_dataframe_artifact(scored_df, output_path)

    return {
        "status": "ok",
        "dataset": {
            "path": str(dataset_path),
            "format": fmt,
            "rows": int(len(scored_df.index)),
            "columns": int(len(scored_df.columns)),
            "workspaceRelativePath": workspace_relative_path(dataset_path),
            "labelsDetected": len(labels),
        },
        "scoring": {
            "scaleCount": len(scoring_results),
            "scales": scoring_results,
            "previewColumns": preview_columns,
            "previewRows": preview,
            "artifact": artifact,
        },
        "backendHints": {
            "primary": "python",
            "recommendedNext": [
                "inspect scored scale distributions and missingness",
                "run survey_psychometrics on scored item definitions when needed",
                "feed score summaries into survey_report or downstream models",
            ],
        },
        "warnings": warnings,
    }


def infer_column_role(series: Any) -> str:
    if is_bool_dtype is not None and is_bool_dtype(series):
        return "boolean"
    if is_datetime64_any_dtype is not None and is_datetime64_any_dtype(series):
        return "datetime"
    if is_numeric_dtype is not None and is_numeric_dtype(series):
        return "numeric"
    if is_string_dtype is not None and is_string_dtype(series):
        return "text"
    return "categorical"


def workspace_relative_path(path: Path) -> str | None:
    workspace_root_raw = os.environ.get("CLAW_WORKSPACE_ROOT")
    if not workspace_root_raw:
        return None
    workspace_root = Path(workspace_root_raw).expanduser()
    try:
        return str(path.resolve().relative_to(workspace_root.resolve()))
    except Exception:
        return None


def render_report(
    payload: dict[str, Any],
    plugin_root: Path,
    workspace_root: Path,
) -> dict[str, Any]:
    template_path = plugin_root / "templates" / "survey-report.md"
    template = template_path.read_text(encoding="utf-8")
    dataset_summary = (
        payload.get("datasetSummary")
        if isinstance(payload.get("datasetSummary"), dict)
        else {}
    )
    dataset = (
        dataset_summary.get("dataset")
        if isinstance(dataset_summary.get("dataset"), dict)
        else dataset_summary
        if isinstance(dataset_summary, dict)
        else {}
    )
    questionnaire = (
        dataset_summary.get("questionnaire")
        if isinstance(dataset_summary.get("questionnaire"), dict)
        else {}
    )
    results = payload.get("results") if isinstance(payload.get("results"), dict) else {}
    replacements = {
        "title": payload.get("title") or "Survey Analysis Report",
        "dataset_path": payload.get("datasetPath") or dataset.get("path") or "TBD",
        "format": dataset.get("format") or "unknown",
        "sample_size": dataset.get("rows") or results.get("sampleSize") or "TBD",
        "scale_count": coalesce(questionnaire.get("scaleCount"), results.get("scaleCount"), "TBD"),
        "reverse_item_count": coalesce(
            questionnaire.get("reverseItemCount"),
            results.get("reverseItemCount"),
            "TBD",
        ),
        "alpha": coalesce(results.get("alpha"), results.get("cronbachAlpha"), "not-run"),
        "omega": coalesce(results.get("omega"), results.get("mcdonaldOmega"), "not-run"),
        "kmo": coalesce(results.get("kmo"), "not-run"),
        "bartlett": coalesce(results.get("bartlett"), "not-run"),
        "model_spec": coalesce(
            results.get("modelSpec"), results.get("cfaModel"), "not-run"
        ),
        "cfi": coalesce(results.get("cfi"), "not-run"),
        "tli": coalesce(results.get("tli"), "not-run"),
        "rmsea": coalesce(results.get("rmsea"), "not-run"),
        "srmr": coalesce(results.get("srmr"), "not-run"),
    }
    rendered = template
    for key, value in replacements.items():
        rendered = rendered.replace(f"{{{{{key}}}}}", str(value))

    notes = payload.get("notes") if isinstance(payload.get("notes"), list) else []
    if notes:
        rendered += "\n" + "\n".join(f"- {note}" for note in notes)

    output_path_raw = payload.get("outputPath")
    artifact = None
    if isinstance(output_path_raw, str) and output_path_raw.strip():
        output_path = resolve_output_path(output_path_raw, workspace_root)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(rendered, encoding="utf-8")
        artifact = {
            "path": str(output_path),
            "workspaceRelativePath": workspace_relative_path(output_path),
            "kind": "markdown",
        }

    return {
        "status": "ok",
        "report": {
            "title": replacements["title"],
            "template": str(template_path),
            "artifact": artifact,
            "markdown": rendered,
        },
        "backendHints": {
            "primary": "markdown",
            "next": [
                "review narrative claims against statistical output",
                "add reliability/validity/CFA results from the R backend contract",
                "promote markdown to Quarto/PDF if needed",
            ],
        },
    }


def coalesce(*values: Any) -> Any:
    for value in values:
        if value is not None and value != "":
            return value
    return None


def run_metadata(
    payload: dict[str, Any],
    plugin_root: Path,
    workspace_root: Path,
) -> dict[str, Any]:
    raw_dataset_path = payload.get("datasetPath")
    if not isinstance(raw_dataset_path, str) or not raw_dataset_path.strip():
        raise ToolInvocationError(
            message="datasetPath is required for survey_metadata",
            code="missing_dataset_path",
        )
    dataset_path = resolve_existing_path(raw_dataset_path, workspace_root, plugin_root)
    if not dataset_path.exists():
        raise ToolInvocationError(
            message=f"dataset not found: {dataset_path}",
            code="dataset_not_found",
            details={"datasetPath": raw_dataset_path, "resolvedPath": str(dataset_path)},
        )
    fmt = detect_format(dataset_path, payload.get("format"))
    dataframe, labels, warnings = load_dataframe(dataset_path, fmt, payload)
    response = summarize_dataframe(
        dataframe,
        dataset_path,
        fmt,
        payload,
        labels,
        warnings,
        plugin_root,
    )
    response["dataset"]["sheetName"] = payload.get("sheetName") if fmt == "xlsx" else None
    response["dataset"]["labelsDetected"] = len(labels)
    response["dataset"]["columnsPreview"] = response["schema"]["columnNames"][:10]
    return response


def main() -> None:
    plugin_id, tool_name, plugin_root, workspace_root = plugin_context()
    try:
        payload = load_payload()
        if tool_name == "survey_metadata":
            result = run_metadata(payload, plugin_root, workspace_root)
        elif tool_name == "survey_score":
            result = run_scoring(payload, plugin_root, workspace_root)
        elif tool_name == "survey_report":
            result = render_report(payload, plugin_root, workspace_root)
        else:
            raise ToolInvocationError(
                message=f"unsupported tool: {tool_name}",
                code="unsupported_tool",
            )
        emit({"plugin": plugin_id, "tool": tool_name, **result})
    except ToolInvocationError as exc:
        emit(
            {
                "plugin": plugin_id,
                "tool": tool_name,
                "status": "error",
                "error": {
                    "code": exc.code,
                    "message": exc.message,
                    "details": exc.details or {},
                },
            }
        )


if __name__ == "__main__":
    main()
