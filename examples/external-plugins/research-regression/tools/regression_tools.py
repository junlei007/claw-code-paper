#!/usr/bin/env python3
from __future__ import annotations

import csv
import json
import math
import os
import sys
from pathlib import Path
from typing import Any


class ToolInvocationError(Exception):
    def __init__(self, message: str, code: str = "tool_error", details: dict[str, Any] | None = None):
        super().__init__(message)
        self.message = message
        self.code = code
        self.details = details or {}


def load_payload() -> dict[str, Any]:
    raw = sys.stdin.read().strip() or os.environ.get("CLAW_TOOL_INPUT", "{}")
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ToolInvocationError(
            f"invalid JSON tool input: {exc}",
            code="invalid_input",
            details={"raw": raw[:500]},
        ) from exc
    if not isinstance(value, dict):
        raise ToolInvocationError("tool input must be a JSON object", code="invalid_input")
    return value


def emit(payload: dict[str, Any]) -> None:
    print(json.dumps(payload, ensure_ascii=False))


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


def load_csv_rows(dataset_path: Path, delimiter: str, encoding: str) -> list[dict[str, str]]:
    with dataset_path.open("r", encoding=encoding, newline="") as handle:
        reader = csv.DictReader(handle, delimiter=delimiter)
        if reader.fieldnames is None:
            raise ToolInvocationError("dataset has no header row", code="invalid_dataset")
        return [dict(row) for row in reader]


def parse_numeric(value: str, column: str, row_index: int) -> float:
    if value is None:
        raise ToolInvocationError(
            f"missing value in column {column}",
            code="missing_value",
            details={"column": column, "row": row_index},
        )
    text = value.strip()
    if not text:
        raise ToolInvocationError(
            f"blank value in column {column}",
            code="missing_value",
            details={"column": column, "row": row_index},
        )
    try:
        return float(text)
    except ValueError as exc:
        raise ToolInvocationError(
            f"non-numeric value in column {column}",
            code="invalid_dataset",
            details={"column": column, "row": row_index, "value": value},
        ) from exc


def transpose(matrix: list[list[float]]) -> list[list[float]]:
    return [list(row) for row in zip(*matrix)]


def matmul(a: list[list[float]], b: list[list[float]]) -> list[list[float]]:
    result = [[0.0 for _ in range(len(b[0]))] for _ in range(len(a))]
    for i in range(len(a)):
        for k in range(len(b)):
            aik = a[i][k]
            for j in range(len(b[0])):
                result[i][j] += aik * b[k][j]
    return result


def invert_matrix(matrix: list[list[float]]) -> list[list[float]]:
    size = len(matrix)
    augmented = [
        row[:] + [1.0 if i == j else 0.0 for j in range(size)]
        for i, row in enumerate(matrix)
    ]

    for col in range(size):
        pivot_row = max(range(col, size), key=lambda idx: abs(augmented[idx][col]))
        pivot = augmented[pivot_row][col]
        if abs(pivot) < 1e-12:
            raise ToolInvocationError(
                "design matrix is singular; predictors are linearly dependent",
                code="singular_matrix",
            )
        if pivot_row != col:
            augmented[col], augmented[pivot_row] = augmented[pivot_row], augmented[col]

        pivot = augmented[col][col]
        augmented[col] = [value / pivot for value in augmented[col]]

        for row in range(size):
            if row == col:
                continue
            factor = augmented[row][col]
            augmented[row] = [
                current - factor * pivot_value
                for current, pivot_value in zip(augmented[row], augmented[col])
            ]

    return [row[size:] for row in augmented]


def vector_from_column(values: list[float]) -> list[list[float]]:
    return [[value] for value in values]


def flatten_column(values: list[list[float]]) -> list[float]:
    return [row[0] for row in values]


def fit_ols(payload: dict[str, Any], plugin_root: Path, workspace_root: Path) -> dict[str, Any]:
    dataset_path = resolve_existing_path(
        str(payload.get("datasetPath", "")),
        workspace_root,
        plugin_root,
    )
    if not dataset_path.exists():
        raise ToolInvocationError(
            f"dataset not found: {dataset_path}",
            code="not_found",
        )

    outcome = payload.get("outcome")
    predictors = payload.get("predictors")
    if not isinstance(outcome, str) or not outcome.strip():
        raise ToolInvocationError("outcome must be a non-empty string", code="invalid_input")
    if not isinstance(predictors, list) or not predictors or not all(
        isinstance(item, str) and item.strip() for item in predictors
    ):
        raise ToolInvocationError(
            "predictors must be a non-empty string array",
            code="invalid_input",
        )

    delimiter = payload.get("delimiter") or ","
    encoding = payload.get("encoding") or "utf-8-sig"
    include_intercept = payload.get("includeIntercept", True)
    rows = load_csv_rows(dataset_path, delimiter, encoding)
    if not rows:
        raise ToolInvocationError("dataset contains no data rows", code="invalid_dataset")

    columns = list(rows[0].keys())
    missing_columns = [column for column in [outcome, *predictors] if column not in columns]
    if missing_columns:
        raise ToolInvocationError(
            "dataset is missing required columns",
            code="invalid_dataset",
            details={"missingColumns": missing_columns},
        )

    design_matrix: list[list[float]] = []
    y_values: list[float] = []
    dropped_rows = 0
    warnings: list[str] = []
    for row_index, row in enumerate(rows, start=1):
        try:
            y_value = parse_numeric(row[outcome], outcome, row_index)
            predictor_values = [parse_numeric(row[column], column, row_index) for column in predictors]
        except ToolInvocationError:
            dropped_rows += 1
            continue

        design_row = predictor_values[:]
        if include_intercept:
            design_row = [1.0, *design_row]
        design_matrix.append(design_row)
        y_values.append(y_value)

    if not design_matrix:
        raise ToolInvocationError(
            "no usable rows remain after numeric filtering",
            code="invalid_dataset",
        )

    n_obs = len(design_matrix)
    n_params = len(design_matrix[0])
    if n_obs <= n_params:
        warnings.append(
            "sample size is only marginally larger than the parameter count; coefficient stability may be weak."
        )

    xt = transpose(design_matrix)
    xtx = matmul(xt, design_matrix)
    xty = matmul(xt, vector_from_column(y_values))
    beta = flatten_column(matmul(invert_matrix(xtx), xty))

    fitted = [sum(weight * value for weight, value in zip(beta, row)) for row in design_matrix]
    residuals = [actual - estimate for actual, estimate in zip(y_values, fitted)]
    mean_y = sum(y_values) / len(y_values)
    sse = sum(residual * residual for residual in residuals)
    sst = sum((value - mean_y) ** 2 for value in y_values)
    r_squared = 1.0 - (sse / sst) if sst > 0 else 1.0
    adjusted_r_squared = None
    if n_obs > n_params and sst > 0:
        adjusted_r_squared = 1.0 - ((1.0 - r_squared) * (n_obs - 1) / (n_obs - n_params))
    residual_standard_error = None
    if n_obs > n_params:
        residual_standard_error = math.sqrt(sse / (n_obs - n_params))

    coefficient_names = predictors[:]
    if include_intercept:
        coefficient_names = ["intercept", *coefficient_names]

    result = {
        "status": "ok",
        "dataset": {
            "path": str(dataset_path.relative_to(workspace_root))
            if dataset_path.is_relative_to(workspace_root)
            else str(dataset_path),
            "rowsRead": len(rows),
            "rowsUsed": n_obs,
            "rowsDropped": dropped_rows,
            "columns": columns,
        },
        "model": {
            "type": "ols",
            "outcome": outcome,
            "predictors": predictors,
            "includeIntercept": include_intercept,
        },
        "coefficients": [
            {"term": name, "estimate": round(estimate, 6)}
            for name, estimate in zip(coefficient_names, beta)
        ],
        "fit": {
            "observations": n_obs,
            "parameters": n_params,
            "rSquared": round(r_squared, 6),
            "adjustedRSquared": None if adjusted_r_squared is None else round(adjusted_r_squared, 6),
            "residualStandardError": None
            if residual_standard_error is None
            else round(residual_standard_error, 6),
        },
        "warnings": warnings,
    }

    output_path_raw = payload.get("outputPath")
    if isinstance(output_path_raw, str) and output_path_raw.strip():
        output_path = resolve_output_path(output_path_raw, workspace_root)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        output_path.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
        result["artifact"] = {
            "path": str(output_path.relative_to(workspace_root))
            if output_path.is_relative_to(workspace_root)
            else str(output_path)
        }

    return result


def main() -> None:
    plugin_id, tool_name, plugin_root, workspace_root = plugin_context()
    try:
        payload = load_payload()
        if tool_name != "regression_ols":
            raise ToolInvocationError(
                f"unsupported tool: {tool_name}",
                code="unsupported_tool",
            )
        result = fit_ols(payload, plugin_root, workspace_root)
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
                    "details": exc.details,
                },
            }
        )


if __name__ == "__main__":
    main()
