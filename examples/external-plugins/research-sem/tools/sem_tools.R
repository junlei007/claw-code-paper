#!/usr/bin/env Rscript

suppressPackageStartupMessages({
  library(jsonlite)
  library(lavaan)
})

read_stdin <- function() {
  lines <- readLines(file("stdin"), warn = FALSE)
  raw <- paste(lines, collapse = "\n")
  if (!nzchar(trimws(raw))) {
    raw <- Sys.getenv("CLAW_TOOL_INPUT", "{}")
  }
  raw
}

json_text <- function(x) {
  toJSON(x, auto_unbox = TRUE, null = "null", digits = 8, pretty = FALSE)
}

emit_json <- function(x) {
  cat(json_text(x))
}

tool_error <- function(plugin_id, tool_name, code, message, details = list()) {
  emit_json(list(
    plugin = plugin_id,
    tool = tool_name,
    status = "error",
    error = list(code = code, message = message, details = details)
  ))
  quit(save = "no", status = 0)
}

plugin_id <- Sys.getenv("CLAW_PLUGIN_ID", "unknown")
tool_name <- Sys.getenv("CLAW_TOOL_NAME", "unknown")
plugin_root <- normalizePath(Sys.getenv("CLAW_PLUGIN_ROOT", "."), winslash = "/", mustWork = FALSE)
workspace_root <- normalizePath(Sys.getenv("CLAW_WORKSPACE_ROOT", plugin_root), winslash = "/", mustWork = FALSE)

if (tool_name != "sem_lavaan") {
  tool_error(plugin_id, tool_name, "unsupported_tool", sprintf("unsupported tool: %s", tool_name))
}

raw_payload <- read_stdin()
payload <- tryCatch(
  fromJSON(raw_payload, simplifyVector = FALSE),
  error = function(err) {
    tool_error(plugin_id, tool_name, "invalid_input", sprintf("invalid JSON tool input: %s", err$message))
  }
)
if (!is.list(payload)) {
  tool_error(plugin_id, tool_name, "invalid_input", "tool input must be a JSON object")
}

normalize_scalar <- function(value, default = NULL) {
  if (is.null(value) || length(value) == 0 || is.list(value)) {
    return(default)
  }
  value[[1]]
}

normalize_string_list <- function(value) {
  if (is.null(value) || length(value) == 0) {
    return(character())
  }
  values <- as.character(unlist(value, use.names = FALSE))
  values <- trimws(values)
  values[nzchar(values)]
}

clean_messages <- function(messages) {
  values <- as.character(unlist(messages, use.names = FALSE))
  values <- gsub("[[:space:]]+", " ", trimws(values))
  unique(values[nzchar(values)])
}

prefix_messages <- function(prefix, messages) {
  msgs <- clean_messages(messages)
  if (!length(msgs)) {
    return(character())
  }
  sprintf("%s: %s", prefix, msgs)
}

resolve_existing_path <- function(raw_path) {
  candidate <- path.expand(raw_path)
  if (grepl("^/", candidate)) {
    return(normalizePath(candidate, winslash = "/", mustWork = FALSE))
  }
  workspace_candidate <- normalizePath(file.path(workspace_root, candidate), winslash = "/", mustWork = FALSE)
  if (file.exists(workspace_candidate)) {
    return(workspace_candidate)
  }
  normalizePath(file.path(plugin_root, candidate), winslash = "/", mustWork = FALSE)
}

resolve_output_path <- function(raw_path) {
  candidate <- path.expand(raw_path)
  if (grepl("^/", candidate)) {
    return(normalizePath(candidate, winslash = "/", mustWork = FALSE))
  }
  normalizePath(file.path(workspace_root, candidate), winslash = "/", mustWork = FALSE)
}

read_dataset <- function(dataset_path, delimiter, encoding, na_values) {
  utils::read.table(
    dataset_path,
    sep = delimiter,
    header = TRUE,
    na.strings = if (length(na_values)) na_values else c("", "NA"),
    fileEncoding = encoding,
    stringsAsFactors = FALSE,
    check.names = FALSE
  )
}

extract_model_variables <- function(model_spec) {
  parsed <- tryCatch(lavaan::lavaanify(model_spec, warn = FALSE, as.data.frame. = TRUE), error = function(err) NULL)
  if (is.null(parsed) || !nrow(parsed)) {
    return(character())
  }
  latent_factors <- unique(parsed$lhs[parsed$op == "=~"])
  vars <- unique(c(
    parsed$rhs[parsed$op %in% c("=~", "~", "~~", "|")],
    parsed$lhs[parsed$op %in% c("~", "~~", "|")]
  ))
  vars <- vars[nzchar(vars)]
  vars[!vars %in% latent_factors & vars != "1"]
}

evaluate_quietly <- function(expr) {
  warnings <- character()
  messages <- character()
  stdout_file <- tempfile("sem-stdout-")
  stderr_file <- tempfile("sem-stderr-")
  stdout_con <- file(stdout_file, open = "wt")
  stderr_con <- file(stderr_file, open = "wt")
  output_depth <- sink.number()
  message_depth <- sink.number(type = "message")

  safe_close <- function(con) {
    if (inherits(con, "connection")) {
      try(close(con), silent = TRUE)
    }
  }

  on.exit({
    while (sink.number(type = "message") > message_depth) sink(type = "message")
    while (sink.number() > output_depth) sink()
    safe_close(stdout_con)
    safe_close(stderr_con)
    unlink(c(stdout_file, stderr_file), force = TRUE)
  }, add = TRUE)

  sink(stdout_con)
  sink(stderr_con, type = "message")
  result <- withCallingHandlers(
    tryCatch(force(expr), error = function(err) err),
    warning = function(wrn) {
      warnings <<- c(warnings, conditionMessage(wrn))
      invokeRestart("muffleWarning")
    },
    message = function(msg) {
      messages <<- c(messages, conditionMessage(msg))
      invokeRestart("muffleMessage")
    }
  )
  sink(type = "message")
  sink()
  safe_close(stdout_con)
  safe_close(stderr_con)

  stdout_lines <- if (file.exists(stdout_file)) readLines(stdout_file, warn = FALSE) else character()
  stderr_lines <- if (file.exists(stderr_file)) readLines(stderr_file, warn = FALSE) else character()

  list(value = result, warnings = clean_messages(c(warnings, messages, stdout_lines, stderr_lines)))
}

serialize_rows <- function(df, mapper) {
  if (is.null(df) || !nrow(df)) {
    return(list())
  }
  rows <- apply(df, 1, mapper)
  unname(rows)
}

select_existing_columns <- function(df, columns) {
  df[, intersect(columns, names(df)), drop = FALSE]
}

count_complete_cases <- function(df, columns) {
  if (!length(columns)) {
    return(0)
  }
  sum(stats::complete.cases(df[, columns, drop = FALSE]))
}

dataset_path_input <- normalize_scalar(payload$datasetPath)
if (is.null(dataset_path_input) || !nzchar(trimws(dataset_path_input))) {
  tool_error(plugin_id, tool_name, "missing_dataset_path", "datasetPath is required")
}

dataset_path <- resolve_existing_path(dataset_path_input)
if (!file.exists(dataset_path)) {
  tool_error(plugin_id, tool_name, "dataset_not_found", sprintf("dataset not found: %s", dataset_path), list(datasetPath = dataset_path_input, resolvedPath = dataset_path))
}

model_spec <- normalize_scalar(payload$modelSpec)
if (is.null(model_spec) || !nzchar(trimws(model_spec))) {
  tool_error(plugin_id, tool_name, "missing_model_spec", "modelSpec is required")
}

analysis_type <- tolower(normalize_scalar(payload$analysisType, "sem"))
if (!analysis_type %in% c("cfa", "sem")) {
  tool_error(plugin_id, tool_name, "invalid_input", "analysisType must be 'cfa' or 'sem'")
}

group_column <- normalize_scalar(payload$groupColumn)
delimiter <- normalize_scalar(payload$delimiter, ",")
encoding <- normalize_scalar(payload$encoding, "UTF-8")
na_values <- normalize_string_list(payload$naValues)
bootstrap <- suppressWarnings(as.integer(normalize_scalar(payload$bootstrap, 0)))
if (is.na(bootstrap) || bootstrap < 0) {
  tool_error(plugin_id, tool_name, "invalid_input", "bootstrap must be a non-negative integer")
}

warnings <- character()
estimator <- normalize_scalar(payload$estimator)
if (is.null(estimator) || !nzchar(trimws(estimator))) {
  estimator <- if (bootstrap > 0) "ML" else "MLR"
}
missing_handling <- normalize_scalar(payload$missingHandling, "fiml")
if (bootstrap > 0 && toupper(estimator) != "ML") {
  warnings <- c(warnings, sprintf("Bootstrap inference is usually paired with ML; continuing with estimator=%s as requested.", estimator))
}
if (!is.null(group_column) && nzchar(group_column)) {
  warnings <- c(warnings, "groupColumn was provided; this prototype fits a multi-group model but does not run an automatic measurement invariance sequence.")
}

dataset <- read_dataset(dataset_path, delimiter, encoding, na_values)
names(dataset) <- as.character(names(dataset))

required_columns <- extract_model_variables(model_spec)
if (!is.null(group_column) && nzchar(group_column)) {
  required_columns <- unique(c(required_columns, group_column))
}
if (!length(required_columns)) {
  tool_error(plugin_id, tool_name, "invalid_model_spec", "modelSpec did not yield any observable variables via lavaanify; check the syntax")
}

missing_columns <- setdiff(required_columns, names(dataset))
if (length(missing_columns)) {
  tool_error(plugin_id, tool_name, "invalid_dataset", "dataset is missing required columns", list(missingColumns = missing_columns, availableColumns = names(dataset)))
}

fit_df <- dataset[, required_columns, drop = FALSE]
model_columns <- setdiff(required_columns, if (!is.null(group_column) && nzchar(group_column)) group_column else character())
for (column in model_columns) {
  fit_df[[column]] <- suppressWarnings(as.numeric(fit_df[[column]]))
}
if (!is.null(group_column) && nzchar(group_column)) {
  fit_df[[group_column]] <- as.factor(fit_df[[group_column]])
}

all_na_columns <- model_columns[vapply(model_columns, function(column) all(is.na(fit_df[[column]])), logical(1))]
if (length(all_na_columns)) {
  tool_error(plugin_id, tool_name, "invalid_dataset", "one or more model columns could not be coerced to numeric values", list(columns = all_na_columns))
}

fit_args <- list(
  model = model_spec,
  data = fit_df,
  estimator = estimator,
  missing = missing_handling,
  std.lv = TRUE
)
if (!is.null(group_column) && nzchar(group_column)) {
  fit_args$group <- group_column
}
if (bootstrap > 0) {
  fit_args$se <- "bootstrap"
  fit_args$bootstrap <- bootstrap
}

fit_capture <- evaluate_quietly(do.call(if (analysis_type == "cfa") lavaan::cfa else lavaan::sem, fit_args))
fit <- fit_capture$value
warnings <- c(warnings, prefix_messages("lavaan", fit_capture$warnings))
if (inherits(fit, "error")) {
  tool_error(plugin_id, tool_name, "fit_failed", sprintf("%s fit failed: %s", toupper(analysis_type), fit$message), list(analysisType = analysis_type))
}

fit_measures_capture <- evaluate_quietly(lavaan::fitMeasures(fit, c("chisq", "df", "pvalue", "cfi", "tli", "rmsea", "srmr", "aic", "bic")))
param_capture <- evaluate_quietly(lavaan::parameterEstimates(fit, standardized = TRUE, ci = TRUE))
inspect_capture <- evaluate_quietly(lavaan::inspect(fit, "converged"))
warnings <- c(
  warnings,
  prefix_messages("fitMeasures", fit_measures_capture$warnings),
  prefix_messages("parameterEstimates", param_capture$warnings),
  prefix_messages("inspect", inspect_capture$warnings)
)

fit_measures <- fit_measures_capture$value
parameters <- param_capture$value
converged <- inspect_capture$value

if (inherits(fit_measures, "error")) {
  warnings <- c(warnings, sprintf("fit measures could not be extracted: %s", fit_measures$message))
  fit_measures <- NULL
}
if (inherits(parameters, "error")) {
  tool_error(plugin_id, tool_name, "parameter_extraction_failed", sprintf("parameter extraction failed: %s", parameters$message))
}

loading_rows <- select_existing_columns(
  parameters[parameters$op == "=~", , drop = FALSE],
  c("lhs", "rhs", "est", "std.all", "se", "ci.lower", "ci.upper", "group")
)
path_rows <- select_existing_columns(
  parameters[parameters$op == "~", , drop = FALSE],
  c("lhs", "rhs", "label", "est", "std.all", "se", "pvalue", "ci.lower", "ci.upper", "group")
)
defined_rows <- select_existing_columns(
  parameters[parameters$op == ":=", , drop = FALSE],
  c("lhs", "label", "est", "se", "pvalue", "ci.lower", "ci.upper", "group")
)

group_summary <- NULL
if (!is.null(group_column) && nzchar(group_column)) {
  counts <- table(fit_df[[group_column]], useNA = "ifany")
  group_summary <- unname(lapply(seq_along(counts), function(i) {
    list(group = names(counts)[[i]], rows = unname(as.integer(counts[[i]])))
  }))
}

result <- list(
  plugin = plugin_id,
  tool = tool_name,
  status = "ok",
  dataset = list(
    path = dataset_path,
    workspaceRelativePath = if (startsWith(dataset_path, paste0(workspace_root, "/"))) sub(paste0("^", workspace_root, "/"), "", dataset_path) else NULL,
    rows = nrow(dataset),
    columns = ncol(dataset),
    modelColumns = model_columns,
    completeCases = count_complete_cases(fit_df, model_columns)
  ),
  model = list(
    analysisType = analysis_type,
    estimator = estimator,
    missingHandling = missing_handling,
    bootstrap = bootstrap,
    groupColumn = if (!is.null(group_column) && nzchar(group_column)) group_column else NULL,
    converged = isTRUE(converged),
    fitMeasures = if (!is.null(fit_measures)) as.list(fit_measures) else NULL,
    groups = group_summary
  ),
  standardizedLoadings = serialize_rows(loading_rows, function(row) list(
    factor = unname(row[["lhs"]]),
    item = unname(row[["rhs"]]),
    estimate = as.numeric(row[["est"]]),
    standardized = as.numeric(row[["std.all"]]),
    se = as.numeric(row[["se"]]),
    ciLower = as.numeric(row[["ci.lower"]]),
    ciUpper = as.numeric(row[["ci.upper"]]),
    group = if ("group" %in% names(row)) as.integer(row[["group"]]) else NULL
  )),
  structuralPaths = serialize_rows(path_rows, function(row) list(
    outcome = unname(row[["lhs"]]),
    predictor = unname(row[["rhs"]]),
    label = if (nzchar(unname(row[["label"]]))) unname(row[["label"]]) else NULL,
    estimate = as.numeric(row[["est"]]),
    standardized = as.numeric(row[["std.all"]]),
    se = as.numeric(row[["se"]]),
    pValue = as.numeric(row[["pvalue"]]),
    ciLower = as.numeric(row[["ci.lower"]]),
    ciUpper = as.numeric(row[["ci.upper"]]),
    group = if ("group" %in% names(row)) as.integer(row[["group"]]) else NULL
  )),
  definedParameters = serialize_rows(defined_rows, function(row) list(
    name = unname(row[["lhs"]]),
    label = if (nzchar(unname(row[["label"]]))) unname(row[["label"]]) else NULL,
    estimate = as.numeric(row[["est"]]),
    se = as.numeric(row[["se"]]),
    pValue = as.numeric(row[["pvalue"]]),
    ciLower = as.numeric(row[["ci.lower"]]),
    ciUpper = as.numeric(row[["ci.upper"]]),
    group = if ("group" %in% names(row)) as.integer(row[["group"]]) else NULL
  )),
  warnings = clean_messages(warnings)
)

output_path_input <- normalize_scalar(payload$outputPath)
if (!is.null(output_path_input) && nzchar(trimws(output_path_input))) {
  output_path <- resolve_output_path(output_path_input)
  dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
  writeLines(json_text(result), output_path, useBytes = TRUE)
  result$artifact <- list(path = output_path)
}

emit_json(result)
