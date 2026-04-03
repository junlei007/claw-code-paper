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

normalize_bool <- function(value, default = FALSE) {
  if (is.null(value) || length(value) == 0 || is.list(value)) {
    return(default)
  }
  if (is.logical(value)) {
    return(isTRUE(value[[1]]))
  }
  lowered <- tolower(trimws(as.character(value[[1]])))
  if (!nzchar(lowered)) {
    return(default)
  }
  lowered %in% c("true", "1", "yes", "y", "on")
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

infer_extension <- function(raw_path) {
  ext <- tools::file_ext(raw_path)
  if (!nzchar(ext)) {
    return(NULL)
  }
  tolower(ext)
}

build_diagram_request <- function(path_input, format_input) {
  if (is.null(path_input) || !nzchar(trimws(path_input))) {
    return(NULL)
  }

  requested_format <- normalize_scalar(format_input)
  if (!is.null(requested_format) && nzchar(trimws(requested_format))) {
    requested_format <- tolower(trimws(requested_format))
  } else {
    requested_format <- NULL
  }

  path_extension <- infer_extension(path_input)
  if (is.null(requested_format) && is.null(path_extension)) {
    return(list(error = "diagramPath must include a .png or .pdf extension, or diagramFormat must be provided"))
  }
  if (!is.null(requested_format) && !requested_format %in% c("png", "pdf")) {
    return(list(error = "diagramFormat must be png or pdf"))
  }
  if (!is.null(path_extension) && !path_extension %in% c("png", "pdf")) {
    return(list(error = "diagramPath must end in .png or .pdf"))
  }
  if (!is.null(requested_format) && !is.null(path_extension) && requested_format != path_extension) {
    return(list(error = "diagramPath extension must match diagramFormat when both are provided"))
  }

  resolved_format <- if (!is.null(requested_format)) requested_format else path_extension
  raw_output_path <- if (!is.null(path_extension)) path_input else sprintf("%s.%s", path_input, resolved_format)
  output_path <- resolve_output_path(raw_output_path)

  list(
    format = resolved_format,
    path = output_path
  )
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

fit_measure_names <- c("chisq", "df", "pvalue", "cfi", "tli", "rmsea", "srmr", "aic", "bic")

extract_fit_measures <- function(fit) {
  capture <- evaluate_quietly(lavaan::fitMeasures(fit, fit_measure_names))
  value <- capture$value
  warnings <- prefix_messages("fitMeasures", capture$warnings)
  if (inherits(value, "error")) {
    return(list(result = NULL, warnings = c(warnings, sprintf("fit measures could not be extracted: %s", value$message))))
  }
  list(result = as.list(value), warnings = warnings)
}

run_invariance_sequence <- function(model_spec, fit_df, estimator, missing_handling, group_column, levels) {
  level_map <- list(
    configural = character(),
    metric = c("loadings"),
    scalar = c("loadings", "intercepts")
  )

  warnings <- character()
  fits <- list()
  previous_measures <- NULL

  for (level_name in levels) {
    fit_args <- list(
      model = model_spec,
      data = fit_df,
      estimator = estimator,
      missing = missing_handling,
      std.lv = TRUE,
      group = group_column
    )
    constraints <- level_map[[level_name]]
    if (length(constraints)) {
      fit_args$group.equal <- constraints
    }

    fit_capture <- evaluate_quietly(do.call(lavaan::cfa, fit_args))
    fit <- fit_capture$value
    warnings <- c(warnings, prefix_messages(sprintf("invariance %s", level_name), fit_capture$warnings))
    if (inherits(fit, "error")) {
      fits[[length(fits) + 1]] <- list(
        level = level_name,
        converged = FALSE,
        constraints = if (length(constraints)) constraints else NULL,
        fitMeasures = NULL,
        deltasFromPrevious = NULL,
        error = fit$message
      )
      break
    }

    converged_capture <- evaluate_quietly(lavaan::inspect(fit, "converged"))
    warnings <- c(warnings, prefix_messages(sprintf("inspect %s", level_name), converged_capture$warnings))
    converged <- converged_capture$value
    if (inherits(converged, "error")) {
      converged <- FALSE
      warnings <- c(warnings, sprintf("inspect %s failed: %s", level_name, converged$message))
    }

    measures_info <- extract_fit_measures(fit)
    warnings <- c(warnings, prefix_messages(sprintf("invariance %s", level_name), measures_info$warnings))
    current_measures <- measures_info$result
    deltas <- NULL
    if (!is.null(previous_measures) && !is.null(current_measures)) {
      deltas <- list(
        cfi = unname(current_measures$cfi - previous_measures$cfi),
        tli = unname(current_measures$tli - previous_measures$tli),
        rmsea = unname(current_measures$rmsea - previous_measures$rmsea),
        srmr = unname(current_measures$srmr - previous_measures$srmr),
        chisq = unname(current_measures$chisq - previous_measures$chisq),
        df = unname(current_measures$df - previous_measures$df)
      )
    }

    fits[[length(fits) + 1]] <- list(
      level = level_name,
      converged = isTRUE(converged),
      constraints = if (length(constraints)) constraints else NULL,
      fitMeasures = current_measures,
      deltasFromPrevious = deltas,
      error = NULL
    )
    previous_measures <- current_measures
  }

  list(levels = fits, warnings = clean_messages(warnings))
}

render_path_diagram <- function(fit, output_path, format, workspace_root) {
  warnings <- character()
  messages <- character()

  if (!requireNamespace("semPlot", quietly = TRUE)) {
    return(list(
      result = list(
        path = output_path,
        workspaceRelativePath = if (startsWith(output_path, paste0(workspace_root, "/"))) sub(paste0("^", workspace_root, "/"), "", output_path) else NULL,
        format = format,
        generated = FALSE,
        error = "Package 'semPlot' is required for diagram export"
      ),
      warnings = warnings
    ))
  }

  dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
  if (file.exists(output_path)) {
    unlink(output_path, force = TRUE)
  }

  device_open <- FALSE
  draw_result <- withCallingHandlers(
    tryCatch({
      if (format == "png") {
        grDevices::png(filename = output_path, width = 1800, height = 1400, res = 180)
      } else {
        grDevices::pdf(file = output_path, width = 12, height = 9)
      }
      device_open <- TRUE

      semPlot::semPaths(
        object = fit,
        what = "std",
        whatLabels = "std",
        style = "ram",
        layout = "tree",
        intercepts = FALSE,
        residuals = FALSE,
        thresholds = FALSE,
        nCharNodes = 0,
        edge.label.cex = 0.7,
        sizeMan = 7,
        sizeLat = 9,
        mar = c(6, 6, 6, 6),
        title = FALSE
      )

      TRUE
    }, error = function(err) err),
    warning = function(wrn) {
      warnings <<- c(warnings, conditionMessage(wrn))
      invokeRestart("muffleWarning")
    },
    message = function(msg) {
      messages <<- c(messages, conditionMessage(msg))
      invokeRestart("muffleMessage")
    }
  )

  if (device_open) {
    try(grDevices::dev.off(), silent = TRUE)
  }

  warnings <- c(warnings, messages)
  warnings <- prefix_messages("semPlot", warnings)
  if (inherits(draw_result, "error")) {
    return(list(
      result = list(
        path = output_path,
        workspaceRelativePath = if (startsWith(output_path, paste0(workspace_root, "/"))) sub(paste0("^", workspace_root, "/"), "", output_path) else NULL,
        format = format,
        generated = FALSE,
        error = draw_result$message
      ),
      warnings = warnings
    ))
  }

  if (!file.exists(output_path)) {
    warnings <- c(warnings, sprintf("semPlot completed without creating the requested diagram file: %s", output_path))
    return(list(
      result = list(
        path = output_path,
        workspaceRelativePath = if (startsWith(output_path, paste0(workspace_root, "/"))) sub(paste0("^", workspace_root, "/"), "", output_path) else NULL,
        format = format,
        generated = FALSE,
        error = "diagram file was not created"
      ),
      warnings = warnings
    ))
  }

  list(
    result = list(
      path = output_path,
      workspaceRelativePath = if (startsWith(output_path, paste0(workspace_root, "/"))) sub(paste0("^", workspace_root, "/"), "", output_path) else NULL,
      format = format,
      generated = TRUE,
      error = NULL
    ),
    warnings = clean_messages(warnings)
  )
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
measurement_invariance <- normalize_bool(payload$measurementInvariance, FALSE)
invariance_levels <- tolower(normalize_string_list(payload$invarianceLevels))
if (!length(invariance_levels)) {
  invariance_levels <- c("configural", "metric", "scalar")
}
invalid_invariance_levels <- setdiff(invariance_levels, c("configural", "metric", "scalar"))
if (length(invalid_invariance_levels)) {
  tool_error(
    plugin_id,
    tool_name,
    "invalid_input",
    "invarianceLevels may only contain configural, metric, or scalar",
    list(invalidLevels = invalid_invariance_levels)
  )
}
diagram_request <- build_diagram_request(
  path_input = normalize_scalar(payload$diagramPath),
  format_input = payload$diagramFormat
)
if (!is.null(diagram_request$error)) {
  tool_error(plugin_id, tool_name, "invalid_input", diagram_request$error)
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
if (!is.null(group_column) && nzchar(group_column) && !measurement_invariance) {
  warnings <- c(warnings, "groupColumn was provided; no measurement invariance sequence was requested, so the plugin only fit the requested grouped model.")
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

fit_measures_capture <- evaluate_quietly(lavaan::fitMeasures(fit, fit_measure_names))
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

measurement_invariance_result <- NULL
if (measurement_invariance) {
  if (is.null(group_column) || !nzchar(group_column)) {
    warnings <- c(warnings, "measurementInvariance was requested but groupColumn is missing; invariance sequence was skipped.")
  } else if (analysis_type != "cfa") {
    warnings <- c(warnings, "measurementInvariance was requested for analysisType=sem; this prototype only runs invariance for CFA models, so the sequence was skipped.")
  } else if (length(unique(stats::na.omit(fit_df[[group_column]]))) < 2) {
    warnings <- c(warnings, "measurementInvariance was requested but fewer than two non-missing groups were available; invariance sequence was skipped.")
  } else {
    invariance_info <- run_invariance_sequence(
      model_spec = model_spec,
      fit_df = fit_df,
      estimator = estimator,
      missing_handling = missing_handling,
      group_column = group_column,
      levels = invariance_levels
    )
    measurement_invariance_result <- list(
      requestedLevels = invariance_levels,
      sequence = invariance_info$levels
    )
    warnings <- c(warnings, invariance_info$warnings)
  }
}

diagram_result <- NULL
if (!is.null(diagram_request)) {
  diagram_info <- render_path_diagram(
    fit = fit,
    output_path = diagram_request$path,
    format = diagram_request$format,
    workspace_root = workspace_root
  )
  diagram_result <- diagram_info$result
  warnings <- c(warnings, diagram_info$warnings)
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
    measurementInvariance = if (measurement_invariance) measurement_invariance_result else NULL,
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
  diagram = diagram_result,
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
