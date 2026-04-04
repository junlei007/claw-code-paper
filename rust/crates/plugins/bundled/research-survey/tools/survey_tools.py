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


def normalize_number(value: Any, digits: int = 3) -> str:
    if value is None or value == "":
        return "not reported"
    try:
        numeric = float(value)
    except Exception:
        return str(value)
    return f"{numeric:.{digits}f}"


def first_nonempty(*values: Any) -> Any:
    for value in values:
        if value is not None and value != "":
            return value
    return None


def resolve_quality_profile(payload: dict[str, Any]) -> str:
    profile = payload.get("qualityProfile")
    if isinstance(profile, str) and profile.strip():
        return profile.strip()
    return "ssci-default"


def normalize_string_list(value: Any) -> list[str]:
    if not isinstance(value, list):
        return []
    result: list[str] = []
    for item in value:
        if isinstance(item, str) and item.strip():
            result.append(item.strip())
    return result


def normalize_visual_artifacts(
    items: Any,
    workspace_root: Path,
    kind: str,
) -> list[dict[str, Any]]:
    if not isinstance(items, list):
        return []
    normalized: list[dict[str, Any]] = []
    for index, item in enumerate(items, start=1):
        if not isinstance(item, dict):
            continue
        raw_path = first_nonempty(item.get("artifactPath"), item.get("path"))
        resolved_path = None
        relative_path = None
        if isinstance(raw_path, str) and raw_path.strip():
            resolved_path = resolve_output_path(raw_path, workspace_root)
            relative_path = workspace_relative_path(resolved_path)
        normalized.append(
            {
                "kind": kind,
                "index": index,
                "title": first_nonempty(item.get("title"), item.get("label"), f"{kind}-{index}"),
                "artifactPath": str(resolved_path) if resolved_path else None,
                "workspaceRelativePath": relative_path,
                "caption": item.get("caption") if isinstance(item.get("caption"), str) else None,
                "sourceMetrics": normalize_string_list(item.get("sourceMetrics")),
                "sourceColumns": normalize_string_list(item.get("sourceColumns")),
                "expectedValues": item.get("expectedValues")
                if isinstance(item.get("expectedValues"), dict)
                else {},
            }
        )
    return normalized


def build_report_input(
    payload: dict[str, Any],
    dataset: dict[str, Any],
    questionnaire: dict[str, Any],
    results: dict[str, Any],
    notes: list[str],
    quality_profile: str,
    workspace_root: Path,
) -> dict[str, Any]:
    return {
        "title": payload.get("title") or "Survey Analysis Report",
        "qualityProfile": quality_profile,
        "dataset": {
            "path": payload.get("datasetPath") or dataset.get("path"),
            "format": dataset.get("format"),
            "sampleSize": first_nonempty(dataset.get("rows"), results.get("sampleSize")),
        },
        "instrument": {
            "scaleCount": first_nonempty(questionnaire.get("scaleCount"), results.get("scaleCount")),
            "reverseItemCount": first_nonempty(
                questionnaire.get("reverseItemCount"), results.get("reverseItemCount")
            ),
        },
        "results": {
            "alpha": first_nonempty(results.get("alpha"), results.get("cronbachAlpha")),
            "omega": first_nonempty(results.get("omega"), results.get("mcdonaldOmega")),
            "kmo": results.get("kmo"),
            "bartlett": results.get("bartlett"),
            "modelSpec": first_nonempty(results.get("modelSpec"), results.get("cfaModel")),
            "estimator": results.get("estimator"),
            "missingHandling": results.get("missingHandling"),
            "cfi": results.get("cfi"),
            "tli": results.get("tli"),
            "rmsea": results.get("rmsea"),
            "srmr": results.get("srmr"),
            "warnings": results.get("warnings") if isinstance(results.get("warnings"), list) else [],
        },
        "notes": notes,
        "figures": normalize_visual_artifacts(payload.get("figures"), workspace_root, "figure"),
        "tables": normalize_visual_artifacts(payload.get("tables"), workspace_root, "table"),
    }


def build_report_sections(report_input: dict[str, Any]) -> dict[str, str]:
    dataset = report_input.get("dataset", {})
    instrument = report_input.get("instrument", {})
    results = report_input.get("results", {})
    notes = report_input.get("notes", [])

    objective = (
        "This report summarizes the questionnaire analysis workflow, with emphasis on measurement quality "
        "and the primary statistical results needed for a draft Results section."
    )
    sample_summary = (
        f"The analyzed dataset came from `{dataset.get('path') or 'the provided dataset'}` "
        f"({dataset.get('format') or 'unknown format'}), with an effective sample size of "
        f"{dataset.get('sampleSize') or 'not reported'}. "
        f"The instrument description currently records {instrument.get('scaleCount') or 'an unknown number of'} scale(s) "
        f"and {instrument.get('reverseItemCount') or 'an unknown number of'} reverse-coded item(s)."
    )

    measurement = (
        "Measurement quality indicators were summarized before substantive interpretation. "
        f"Cronbach's alpha was {normalize_number(results.get('alpha'))}, "
        f"McDonald's omega was {normalize_number(results.get('omega'))}, "
        f"KMO was {normalize_number(results.get('kmo'))}, and Bartlett's test was reported as {results.get('bartlett') or 'not reported'}."
    )

    model_sentence = (
        f"The primary model specification was `{results.get('modelSpec') or 'not reported'}`. "
        f"Estimator: {results.get('estimator') or 'not reported'}. "
        f"Missing-data handling: {results.get('missingHandling') or 'not reported'}."
    )
    fit_sentence = (
        f"Key fit indices were CFI = {normalize_number(results.get('cfi'))}, "
        f"TLI = {normalize_number(results.get('tli'))}, "
        f"RMSEA = {normalize_number(results.get('rmsea'))}, and "
        f"SRMR = {normalize_number(results.get('srmr'))}."
    )
    primary_results = (
        "The primary statistical results are reported below in a compact manuscript-oriented form. "
        + model_sentence
        + " "
        + fit_sentence
    )

    warnings = results.get("warnings") if isinstance(results.get("warnings"), list) else []
    diagnostics_bits = []
    if warnings:
        diagnostics_bits.append(
            "Warnings that may affect interpretation were retained explicitly: "
            + "; ".join(str(item) for item in warnings[:4])
        )
    if notes:
        diagnostics_bits.append("Analyst notes: " + "; ".join(str(item) for item in notes[:4]))
    if not diagnostics_bits:
        diagnostics_bits.append(
            "No additional diagnostics or analyst notes were supplied; publication-facing interpretation should still verify assumptions manually."
        )
    diagnostics = " ".join(diagnostics_bits)

    takeaway = (
        "Overall, this draft should be treated as a structured Results-section starting point rather than a final manuscript product. "
        "All substantive claims should remain tied to the reported statistics and any unresolved warnings above."
    )

    return {
        "objective": objective,
        "sample_summary": sample_summary,
        "measurement_results": measurement,
        "primary_results": primary_results,
        "diagnostics": diagnostics,
        "takeaway": takeaway,
    }


def revise_report_sections(
    report_input: dict[str, Any],
    sections: dict[str, str],
    required_fixes: list[str],
) -> dict[str, str]:
    revised = dict(sections)
    results = report_input.get("results", {})
    dataset = report_input.get("dataset", {})
    notes = report_input.get("notes", [])

    for fix in required_fixes:
        lowered = str(fix).lower()
        if "not reported" in lowered or "scaffold-level" in lowered:
            revised["measurement_results"] = (
                "Measurement quality indicators were reviewed before interpretation. "
                f"Sample size was {dataset.get('sampleSize') or 'available in the dataset summary'}, "
                f"but some reliability or validity statistics were not supplied in the structured inputs. "
                f"Available values include Cronbach's alpha = {normalize_number(results.get('alpha'))}, "
                f"McDonald's omega = {normalize_number(results.get('omega'))}, "
                f"KMO = {normalize_number(results.get('kmo'))}, and Bartlett's test = {results.get('bartlett') or 'not available in this run'}. "
                "Unavailable statistics should be treated as pending outputs rather than silently interpreted."
            )
            if results.get("modelSpec"):
                revised["primary_results"] = (
                    "The primary model results are reported in compact form. "
                    f"Model specification: `{results.get('modelSpec')}`. "
                    f"Estimator: {results.get('estimator') or 'not supplied'}. "
                    f"Missing-data handling: {results.get('missingHandling') or 'not supplied'}. "
                    f"Reported fit indices were CFI = {normalize_number(results.get('cfi'))}, "
                    f"TLI = {normalize_number(results.get('tli'))}, "
                    f"RMSEA = {normalize_number(results.get('rmsea'))}, and "
                    f"SRMR = {normalize_number(results.get('srmr'))}. "
                    "Any omitted fit statistics should be filled from the upstream psychometrics or SEM contract before manuscript submission."
                )
            else:
                revised["primary_results"] = (
                    "No fitted CFA or SEM result package was supplied in the current structured inputs, "
                    "so the primary model-results paragraph remains intentionally limited to reporting that absence. "
                    "A manuscript-facing Results section should not claim model fit until the upstream model output is attached."
                )
            revised["diagnostics"] = (
                "Diagnostics and reporting limits were retained explicitly. "
                + (
                    "Warnings: " + "; ".join(str(item) for item in results.get("warnings", [])[:4]) + ". "
                    if isinstance(results.get("warnings"), list) and results.get("warnings")
                    else ""
                )
                + (
                    "Analyst notes: " + "; ".join(str(item) for item in notes[:4]) + ". "
                    if notes
                    else ""
                )
                + "Any still-missing statistics should be interpreted as missing upstream evidence, not as negative findings."
            )
            revised["takeaway"] = (
                "This reviewed draft now distinguishes clearly between available evidence and still-missing evidence. "
                "It is suitable as a stronger Results-section draft, but unresolved missing outputs should be completed before final delivery."
            )
        if "tool-log wording" in lowered or "payload wording" in lowered or "json-oriented" in lowered:
            for key, value in revised.items():
                revised[key] = (
                    value.replace("payload", "structured input")
                    .replace("JSON", "structured result")
                    .replace("json", "structured result")
                )
    return revised


def render_report_markdown(template: str, replacements: dict[str, Any]) -> str:
    rendered = template
    for key, value in replacements.items():
        rendered = rendered.replace(f"{{{{{key}}}}}", str(value))
    return rendered


def structure_review(report_input: dict[str, Any], markdown: str) -> dict[str, Any]:
    required_headings = [
        "## Objective",
        "## Sample and instrument overview",
        "## Measurement quality results",
        "## Primary statistical results",
        "## Diagnostics and reporting limits",
        "## Results takeaway",
    ]
    issues: list[str] = []
    passed_checks = 0

    for heading in required_headings:
        if heading in markdown:
            passed_checks += 1
        else:
            issues.append(f"Missing required section heading: {heading}")

    results = report_input.get("results", {})
    if results.get("modelSpec") and "CFI =" not in markdown:
        issues.append("Model results were provided but compact fit reporting is missing.")
    else:
        passed_checks += 1
    if results.get("warnings") and "Warnings" not in markdown and "warning" not in markdown.lower():
        issues.append("Warnings are present in structured input but not surfaced in diagnostics.")
    else:
        passed_checks += 1

    total_checks = len(required_headings) + 2
    score = round((passed_checks / total_checks) * 100)
    verdict = "pass" if score >= 85 and not issues else "revise"
    return {
        "score": score,
        "verdict": verdict,
        "issues": issues,
        "checksRun": total_checks,
    }


def narrative_review(markdown: str) -> dict[str, Any]:
    issues: list[str] = []
    discouraged_patterns = {
        "tool returned": "Tool-log wording should not appear in the final narrative.",
        "payload": "Machine-oriented payload wording should not appear in the final narrative.",
        "json": "Raw JSON-oriented wording should stay out of user-facing report prose.",
        "obviously": "Overconfident language should be avoided in SSCI-style results writing.",
    }
    for pattern, issue in discouraged_patterns.items():
        if pattern in markdown.lower():
            issues.append(issue)

    if markdown.count("not reported") >= 5:
        issues.append("Too many 'not reported' placeholders remain; the draft still feels scaffold-level.")
    if "## Notes" in markdown:
        issues.append("A generic Notes section suggests scaffold-level reporting rather than Results-grade structure.")

    score = max(0, 100 - len(issues) * 18)
    verdict = "pass" if score >= 82 and not issues else "revise"
    return {
        "score": score,
        "verdict": verdict,
        "issues": issues,
        "checksRun": len(discouraged_patterns) + 2,
    }


def figure_accuracy_review(report_input: dict[str, Any]) -> dict[str, Any]:
    figures = report_input.get("figures", [])
    tables = report_input.get("tables", [])
    if not figures and not tables:
        return {
            "score": None,
            "verdict": "not_applicable",
            "issues": [],
            "checksRun": 0,
        }
    issues: list[str] = []
    passed_checks = 0
    total_checks = 0
    result_values = report_input.get("results", {})

    for artifact in [*figures, *tables]:
        title = artifact.get("title") or f"{artifact.get('kind', 'artifact')}-{artifact.get('index', '?')}"
        artifact_path = artifact.get("artifactPath")
        total_checks += 1
        if artifact_path:
            if Path(str(artifact_path)).exists():
                passed_checks += 1
            else:
                issues.append(f"{title}: referenced artifact file is missing ({artifact_path}).")
        else:
            issues.append(f"{title}: artifactPath/path is missing.")

        caption = artifact.get("caption") if isinstance(artifact.get("caption"), str) else None
        total_checks += 1
        if caption and caption.strip():
            passed_checks += 1
        else:
            issues.append(f"{title}: caption should be declared for provenance and manuscript reuse.")

        source_metrics = artifact.get("sourceMetrics") if isinstance(artifact.get("sourceMetrics"), list) else []
        total_checks += 1
        if source_metrics:
            missing_metrics = [
                metric
                for metric in source_metrics
                if result_values.get(metric) in (None, "")
            ]
            if missing_metrics:
                issues.append(f"{title}: sourceMetrics missing from structured results: {', '.join(missing_metrics)}.")
            else:
                passed_checks += 1
        else:
            issues.append(f"{title}: sourceMetrics should be declared for deterministic consistency review.")

        source_columns = artifact.get("sourceColumns") if isinstance(artifact.get("sourceColumns"), list) else []
        total_checks += 1
        if source_columns:
            passed_checks += 1
        else:
            issues.append(f"{title}: sourceColumns should be declared to trace the visual back to analysis inputs.")

        expected_values = artifact.get("expectedValues") if isinstance(artifact.get("expectedValues"), dict) else {}
        if expected_values:
            total_checks += len(expected_values)
            for metric, expected in expected_values.items():
                actual = result_values.get(metric)
                if values_match_with_tolerance(actual, expected):
                    passed_checks += 1
                else:
                    issues.append(
                        f"{title}: expectedValues mismatch for `{metric}` (expected {expected}, got {actual})."
                    )

    score = round((passed_checks / total_checks) * 100) if total_checks else None
    verdict = "pass" if total_checks and not issues else "revise"
    return {
        "score": score,
        "verdict": verdict,
        "issues": issues,
        "checksRun": total_checks,
    }


def values_match_with_tolerance(actual: Any, expected: Any, tolerance: float = 0.005) -> bool:
    if actual is None and expected is None:
        return True
    try:
        return abs(float(actual) - float(expected)) <= tolerance
    except Exception:
        return str(actual).strip() == str(expected).strip()


MAX_REPORT_REVISION_ITERATIONS = 2
MIN_REPORT_DELIVERY_SCORE = 85


def overall_review_verdict(
    figure_review: dict[str, Any],
    structure: dict[str, Any],
    narrative: dict[str, Any],
) -> str:
    hard_gate_verdicts = [
        figure_review.get("verdict"),
        structure.get("verdict"),
        narrative.get("verdict"),
    ]
    if any(verdict in {"revise", "fail"} for verdict in hard_gate_verdicts):
        return "revise"
    return "pass"


def revision_fix_candidates(review: dict[str, Any]) -> list[str]:
    dimensions = review.get("dimensions") if isinstance(review.get("dimensions"), dict) else {}
    fixes: list[str] = []
    for key in ("structureQuality", "narrativeQuality"):
        dimension = dimensions.get(key)
        if isinstance(dimension, dict) and dimension.get("verdict") in {"revise", "fail"}:
            issues = dimension.get("issues")
            if isinstance(issues, list):
                fixes.extend(str(item) for item in issues if str(item).strip())
    return fixes


def review_failure_summary(
    figure_review: dict[str, Any],
    structure: dict[str, Any],
    narrative: dict[str, Any],
) -> tuple[list[dict[str, Any]], list[str]]:
    failures: list[dict[str, Any]] = []
    unresolved: list[str] = []
    for name, dimension in (
        ("figureAccuracy", figure_review),
        ("structureQuality", structure),
        ("narrativeQuality", narrative),
    ):
        verdict = dimension.get("verdict")
        issues = dimension.get("issues") if isinstance(dimension.get("issues"), list) else []
        cleaned_issues = [str(item) for item in issues if str(item).strip()]
        if verdict in {"revise", "fail"}:
            failures.append(
                {
                    "dimension": name,
                    "verdict": verdict,
                    "issueCount": len(cleaned_issues),
                }
            )
            unresolved.extend(cleaned_issues)
    return failures, unresolved


def build_delivery_decision(review: dict[str, Any]) -> dict[str, Any]:
    hard_gate_failures = (
        review.get("hardGateFailures") if isinstance(review.get("hardGateFailures"), list) else []
    )
    unresolved_issues = (
        review.get("unresolvedIssues") if isinstance(review.get("unresolvedIssues"), list) else []
    )
    overall_verdict = review.get("overallVerdict")
    overall_score = review.get("overallScore")

    blocking_reasons: list[str] = []
    if overall_verdict != "pass":
        blocking_reasons.append("overall review verdict is not pass")
    if hard_gate_failures:
        blocking_reasons.append("one or more hard quality gates are still failing")
    if unresolved_issues:
        blocking_reasons.append("unresolved review issues remain after bounded revision")
    if isinstance(overall_score, int) and overall_score < MIN_REPORT_DELIVERY_SCORE:
        blocking_reasons.append(
            f"overall review score {overall_score} is below delivery threshold {MIN_REPORT_DELIVERY_SCORE}"
        )

    ready = not blocking_reasons
    status = "ready" if ready else "draft_under_review"
    summary = (
        "report passed all quality gates and is ready for user-facing delivery"
        if ready
        else "; ".join(blocking_reasons)
    )
    return {
        "status": status,
        "ready": ready,
        "minimumScoreThreshold": MIN_REPORT_DELIVERY_SCORE,
        "overallScore": overall_score,
        "blockingReasons": blocking_reasons,
        "summary": summary,
    }


def build_review_result(report_input: dict[str, Any], markdown: str) -> dict[str, Any]:
    figure_review = figure_accuracy_review(report_input)
    structure = structure_review(report_input, markdown)
    narrative = narrative_review(markdown)

    required_fixes = [*structure["issues"], *narrative["issues"], *figure_review["issues"]]
    overall_verdict = overall_review_verdict(figure_review, structure, narrative)
    hard_gate_failures, unresolved_issues = review_failure_summary(
        figure_review, structure, narrative
    )
    scored = [
        item
        for item in (figure_review["score"], structure["score"], narrative["score"])
        if isinstance(item, int)
    ]
    overall_score = round(sum(scored) / len(scored)) if scored else None

    return {
        "qualityProfile": report_input.get("qualityProfile") or "ssci-default",
        "dimensions": {
            "figureAccuracy": figure_review,
            "structureQuality": structure,
            "narrativeQuality": narrative,
        },
        "overallVerdict": overall_verdict,
        "overallScore": overall_score,
        "requiredFixes": required_fixes,
        "hardGateFailures": hard_gate_failures,
        "unresolvedIssues": unresolved_issues,
        "revision": {
            "attempted": False,
            "appliedFixes": [],
            "iterations": 0,
        },
    }


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
    notes = payload.get("notes") if isinstance(payload.get("notes"), list) else []
    quality_profile = resolve_quality_profile(payload)
    report_input = build_report_input(
        payload=payload,
        dataset=dataset,
        questionnaire=questionnaire,
        results=results,
        notes=notes,
        quality_profile=quality_profile,
        workspace_root=workspace_root,
    )
    sections = build_report_sections(report_input)
    replacements = {
        "title": report_input["title"],
        "quality_profile": quality_profile,
        "objective": sections["objective"],
        "sample_summary": sections["sample_summary"],
        "measurement_results": sections["measurement_results"],
        "primary_results": sections["primary_results"],
        "diagnostics": sections["diagnostics"],
        "takeaway": sections["takeaway"],
    }
    rendered = render_report_markdown(template, replacements)
    review = build_review_result(report_input, rendered)
    revision_attempted = False
    revision_iterations = 0
    applied_fixes: list[str] = []
    current_sections = sections

    while revision_iterations < MAX_REPORT_REVISION_ITERATIONS:
        fix_candidates = revision_fix_candidates(review)
        if not fix_candidates:
            break

        revision_attempted = True
        revision_iterations += 1
        applied_fixes.extend(fix for fix in fix_candidates if fix not in applied_fixes)
        revised_sections = revise_report_sections(
            report_input=report_input,
            sections=current_sections,
            required_fixes=fix_candidates,
        )
        revised_replacements = {
            "title": report_input["title"],
            "quality_profile": quality_profile,
            "objective": revised_sections["objective"],
            "sample_summary": revised_sections["sample_summary"],
            "measurement_results": revised_sections["measurement_results"],
            "primary_results": revised_sections["primary_results"],
            "diagnostics": revised_sections["diagnostics"],
            "takeaway": revised_sections["takeaway"],
        }
        revised_markdown = render_report_markdown(template, revised_replacements)
        if revised_markdown == rendered:
            break

        current_sections = revised_sections
        rendered = revised_markdown
        review = build_review_result(report_input, rendered)

        if review["overallVerdict"] == "pass":
            break

    review["revision"] = {
        "attempted": revision_attempted,
        "appliedFixes": applied_fixes,
        "iterations": revision_iterations,
    }

    output_path_raw = payload.get("outputPath")
    artifact = None
    review_artifact = None
    input_artifact = None
    figures_artifact = None
    tables_artifact = None
    if isinstance(output_path_raw, str) and output_path_raw.strip():
        output_path = resolve_output_path(output_path_raw, workspace_root)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(rendered, encoding="utf-8")
        artifact = {
            "path": str(output_path),
            "workspaceRelativePath": workspace_relative_path(output_path),
            "kind": "markdown",
        }
        review_output = output_path.with_suffix(".review.json")
        review_output.write_text(
            json.dumps(review, ensure_ascii=False, indent=2, default=json_default),
            encoding="utf-8",
        )
        review_artifact = {
            "path": str(review_output),
            "workspaceRelativePath": workspace_relative_path(review_output),
            "kind": "json",
        }
        input_output = output_path.with_suffix(".input.json")
        input_output.write_text(
            json.dumps(report_input, ensure_ascii=False, indent=2, default=json_default),
            encoding="utf-8",
        )
        input_artifact = {
            "path": str(input_output),
            "workspaceRelativePath": workspace_relative_path(input_output),
            "kind": "json",
        }
        figures = report_input.get("figures") if isinstance(report_input.get("figures"), list) else []
        if figures:
            figures_output = output_path.with_suffix(".figures.json")
            figures_output.write_text(
                json.dumps(figures, ensure_ascii=False, indent=2, default=json_default),
                encoding="utf-8",
            )
            figures_artifact = {
                "path": str(figures_output),
                "workspaceRelativePath": workspace_relative_path(figures_output),
                "kind": "json",
                "count": len(figures),
            }
        tables = report_input.get("tables") if isinstance(report_input.get("tables"), list) else []
        if tables:
            tables_output = output_path.with_suffix(".tables.json")
            tables_output.write_text(
                json.dumps(tables, ensure_ascii=False, indent=2, default=json_default),
                encoding="utf-8",
            )
            tables_artifact = {
                "path": str(tables_output),
                "workspaceRelativePath": workspace_relative_path(tables_output),
                "kind": "json",
                "count": len(tables),
            }

    return {
        "status": "ok",
        "delivery": build_delivery_decision(review),
        "report": {
            "title": report_input["title"],
            "template": str(template_path),
            "artifact": artifact,
            "markdown": rendered,
        },
        "reportInput": {
            "qualityProfile": quality_profile,
            "artifact": input_artifact,
            "data": report_input,
        },
        "visualArtifacts": {
            "figures": {
                "artifact": figures_artifact,
                "items": report_input.get("figures", []),
            },
            "tables": {
                "artifact": tables_artifact,
                "items": report_input.get("tables", []),
            },
        },
        "review": {
            **review,
            "artifact": review_artifact,
        },
        "backendHints": {
            "primary": "markdown",
            "next": [
                "treat markdown as a reviewed draft, not a final manuscript artifact",
                "fix all requiredFixes in the review payload before delivery",
                "promote the reviewed markdown to Quarto/PDF only after quality gates pass",
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
