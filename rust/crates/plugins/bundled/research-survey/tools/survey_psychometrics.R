#!/usr/bin/env Rscript

suppressPackageStartupMessages({
  library(jsonlite)
  library(psych)
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

emit_json <- function(x) {
  cat(toJSON(x, auto_unbox = TRUE, null = "null", digits = 8))
}

tool_error <- function(plugin_id, tool_name, code, message, details = list()) {
  emit_json(list(
    plugin = plugin_id,
    tool = tool_name,
    status = "error",
    error = list(
      code = code,
      message = message,
      details = details
    )
  ))
  quit(save = "no", status = 0)
}

plugin_id <- Sys.getenv("CLAW_PLUGIN_ID", "unknown")
tool_name <- Sys.getenv("CLAW_TOOL_NAME", "unknown")
plugin_root <- normalizePath(Sys.getenv("CLAW_PLUGIN_ROOT", "."), winslash = "/", mustWork = FALSE)
workspace_root <- normalizePath(Sys.getenv("CLAW_WORKSPACE_ROOT", plugin_root), winslash = "/", mustWork = FALSE)

if (tool_name != "survey_psychometrics") {
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
  payload <- list(value = payload)
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

normalize_scalar <- function(value, default = NULL) {
  if (is.null(value) || length(value) == 0) {
    return(default)
  }
  if (is.list(value)) {
    return(default)
  }
  value[[1]]
}

normalize_string_list <- function(value) {
  if (is.null(value) || length(value) == 0) {
    return(character())
  }
  values <- unlist(value, use.names = FALSE)
  values <- as.character(values)
  values <- trimws(values)
  values[nzchar(values)]
}

normalize_numeric_list <- function(value) {
  if (is.null(value) || length(value) == 0) {
    return(numeric())
  }
  values <- suppressWarnings(as.numeric(unlist(value, use.names = FALSE)))
  values[is.finite(values)]
}

coalesce <- function(...) {
  values <- list(...)
  for (value in values) {
    if (!is.null(value) && length(value) > 0 && !(is.character(value) && !nzchar(value[[1]]))) {
      return(value)
    }
  }
  NULL
}

clean_messages <- function(messages) {
  values <- unlist(messages, use.names = FALSE)
  values <- as.character(values)
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

infer_format <- function(dataset_path, requested_format) {
  fmt <- tolower(trimws(coalesce(requested_format, "auto")))
  if (fmt != "auto") {
    return(fmt)
  }
  ext <- tolower(tools::file_ext(dataset_path))
  if (ext == "xls") {
    return("xlsx")
  }
  ext
}

read_dataset <- function(dataset_path, fmt, payload) {
  if (fmt %in% c("csv", "tsv", "dat")) {
    delimiter <- normalize_scalar(payload$delimiter)
    if (is.null(delimiter) || !nzchar(delimiter)) {
      delimiter <- if (fmt == "tsv") "\t" else ","
    }
    encoding <- normalize_scalar(payload$encoding, "UTF-8")
    na_values <- normalize_string_list(payload$naValues)
    layout <- normalize_scalar(payload$layout)
    if (identical(fmt, "dat") && identical(layout, "fixed-width")) {
      widths <- normalize_numeric_list(coalesce(payload$widths, payload$columnWidths, payload$fwfWidths))
      if (!length(widths)) {
        tool_error(
          plugin_id,
          tool_name,
          "missing_fixed_widths",
          "widths (or columnWidths / fwfWidths) are required when reading fixed-width .dat files",
          list(datasetPath = dataset_path)
        )
      }
      column_names <- normalize_string_list(coalesce(payload$columnNames, payload$columns))
      return(utils::read.fwf(
        dataset_path,
        widths = as.integer(widths),
        na.strings = if (length(na_values)) na_values else c("", "NA"),
        fileEncoding = encoding,
        stringsAsFactors = FALSE,
        col.names = if (length(column_names)) column_names else NULL
      ))
    }
    return(utils::read.table(
      dataset_path,
      sep = delimiter,
      header = TRUE,
      na.strings = if (length(na_values)) na_values else c("", "NA"),
      fileEncoding = encoding,
      stringsAsFactors = FALSE,
      check.names = FALSE
    ))
  }
  if (fmt == "xlsx") {
    if (!requireNamespace("readxl", quietly = TRUE)) {
      tool_error(plugin_id, tool_name, "missing_dependency", "readxl is required to read .xlsx files in the R backend", list(dependency = "readxl"))
    }
    sheet_name <- payload$sheetName
    if (is.null(sheet_name) || length(sheet_name) == 0) {
      sheet_name <- 1
    }
    return(as.data.frame(readxl::read_excel(dataset_path, sheet = sheet_name)))
  }
  if (fmt == "sav") {
    if (!requireNamespace("haven", quietly = TRUE)) {
      tool_error(plugin_id, tool_name, "missing_dependency", "haven is required to read SPSS .sav files in the R backend", list(dependency = "haven"))
    }
    return(as.data.frame(haven::read_sav(dataset_path)))
  }
  tool_error(plugin_id, tool_name, "unsupported_format", sprintf("unsupported dataset format: %s", fmt), list(supportedFormats = list("csv", "tsv", "xlsx", "sav", "dat")))
}

extract_scale_definitions <- function(payload) {
  if (is.null(payload$scaleDefinitions) || length(payload$scaleDefinitions) == 0) {
    return(list())
  }
  defs <- payload$scaleDefinitions
  if (!is.list(defs)) {
    return(list())
  }
  Filter(Negate(is.null), lapply(seq_along(defs), function(i) {
    scale <- defs[[i]]
    if (!is.list(scale)) {
      return(NULL)
    }
    list(
      name = coalesce(normalize_scalar(scale$name), sprintf("scale_%s", i)),
      items = normalize_string_list(scale$items),
      reverseItems = normalize_string_list(scale$reverseItems)
    )
  }))
}

extract_model_variables <- function(model_spec) {
  if (is.null(model_spec) || !nzchar(trimws(model_spec))) {
    return(character())
  }
  parsed <- tryCatch(lavaan::lavaanify(model_spec, warn = FALSE, as.data.frame. = TRUE), error = function(err) NULL)
  if (is.null(parsed) || !nrow(parsed)) {
    return(character())
  }
  latent_factors <- unique(parsed$lhs[parsed$op == "=~"])
  vars <- unique(c(
    parsed$rhs[parsed$op == "=~"],
    parsed$rhs[parsed$op == "~"],
    parsed$lhs[parsed$op == "~"]
  ))
  vars <- vars[nzchar(vars)]
  vars[!vars %in% latent_factors]
}

evaluate_quietly <- function(expr) {
  warnings <- character()
  messages <- character()
  stdout_file <- tempfile("survey-stdout-")
  stderr_file <- tempfile("survey-stderr-")
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
    while (sink.number(type = "message") > message_depth) {
      sink(type = "message")
    }
    while (sink.number() > output_depth) {
      sink()
    }
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

  list(
    value = result,
    warnings = clean_messages(c(warnings, messages, stdout_lines, stderr_lines))
  )
}

looks_like_identifier_column <- function(name, column) {
  normalized_name <- gsub("[^a-z0-9]+", "", tolower(name))
  observed <- suppressWarnings(as.numeric(column[!is.na(column)]))
  if (!length(observed) || any(!is.finite(observed))) {
    return(FALSE)
  }
  integer_like <- all(abs(observed - round(observed)) < 1e-08)
  sequential <- length(unique(observed)) == length(observed) && length(observed) >= 3 && all(diff(sort(observed)) == 1)
  explicit_id_name <- normalized_name %in% c("id", "recordid", "caseid", "subjectid", "participantid", "respondentid", "userid")
  explicit_id_name || (grepl("id$", normalized_name) && integer_like && sequential)
}

select_default_analysis_items <- function(df) {
  numeric_candidates <- names(df)[vapply(df, is.numeric, logical(1))]
  if (!length(numeric_candidates)) {
    return(list(items = character(), warnings = character()))
  }
  identifier_like <- numeric_candidates[vapply(numeric_candidates, function(name) {
    looks_like_identifier_column(name, df[[name]])
  }, logical(1))]
  usable_items <- setdiff(numeric_candidates, identifier_like)
  warnings <- character()
  if (length(identifier_like)) {
    warnings <- c(warnings, sprintf("Excluded %s identifier-like numeric columns from fallback psychometrics item selection.", length(identifier_like)))
  }
  list(items = usable_items, warnings = warnings)
}

inspect_numeric_matrix <- function(df, correlation_threshold = 0.999999, eigen_tolerance = 1e-08) {
  usable <- stats::na.omit(df)
  warnings <- character()
  if (ncol(usable) < 2 || nrow(usable) < 3) {
    return(list(
      usable = usable,
      completeCases = nrow(usable),
      correlation = NULL,
      warnings = warnings,
      hasPerfectCorrelations = FALSE,
      isPositiveDefinite = FALSE,
      isSingular = TRUE
    ))
  }

  correlation <- tryCatch(stats::cor(usable, use = "pairwise.complete.obs"), error = function(err) err)
  if (inherits(correlation, "error")) {
    return(list(
      usable = usable,
      completeCases = nrow(usable),
      correlation = NULL,
      warnings = sprintf("Correlation matrix failed: %s", correlation$message),
      hasPerfectCorrelations = FALSE,
      isPositiveDefinite = FALSE,
      isSingular = TRUE
    ))
  }

  if (any(!is.finite(correlation))) {
    return(list(
      usable = usable,
      completeCases = nrow(usable),
      correlation = correlation,
      warnings = "The correlation matrix contained non-finite values.",
      hasPerfectCorrelations = FALSE,
      isPositiveDefinite = FALSE,
      isSingular = TRUE
    ))
  }

  perfect_pairs <- which(upper.tri(correlation) & abs(correlation) >= correlation_threshold, arr.ind = TRUE)
  has_perfect_correlations <- nrow(perfect_pairs) > 0
  if (has_perfect_correlations) {
    pair_labels <- apply(perfect_pairs, 1, function(idx) {
      sprintf("%s ~ %s", colnames(correlation)[idx[[1]]], colnames(correlation)[idx[[2]]])
    })
    warnings <- c(warnings, sprintf("Perfect correlations detected: %s.", paste(pair_labels, collapse = ", ")))
  }

  eigenvalues <- tryCatch(eigen(correlation, symmetric = TRUE, only.values = TRUE)$values, error = function(err) numeric())
  is_positive_definite <- length(eigenvalues) == ncol(correlation) && all(is.finite(eigenvalues)) && min(eigenvalues) > eigen_tolerance
  rank_value <- tryCatch(qr(correlation)$rank, error = function(err) 0L)
  is_singular <- !is_positive_definite || rank_value < ncol(correlation)
  if (is_singular && !has_perfect_correlations) {
    warnings <- c(warnings, "The item correlation matrix was singular or not positive definite.")
  }

  list(
    usable = usable,
    completeCases = nrow(usable),
    correlation = correlation,
    warnings = clean_messages(warnings),
    hasPerfectCorrelations = has_perfect_correlations,
    isPositiveDefinite = is_positive_definite,
    isSingular = is_singular
  )
}

collect_analysis_items <- function(scale_definitions, model_spec, df) {
  declared <- unique(unlist(lapply(scale_definitions, function(x) x$items), use.names = FALSE))
  declared <- c(declared, extract_model_variables(model_spec))
  declared <- unique(declared[nzchar(declared)])
  if (!length(declared)) {
    fallback <- select_default_analysis_items(df)
    return(list(
      items = fallback$items,
      warnings = clean_messages(c(
        if (length(fallback$items)) "No scaleDefinitions provided; using fallback numeric questionnaire items for psychometrics." else character(),
        fallback$warnings
      ))
    ))
  }
  list(items = declared, warnings = character())
}

reverse_score_item <- function(x, min_value = NULL, max_value = NULL) {
  observed <- x[!is.na(x)]
  if (!length(observed)) {
    return(list(values = x, warning = NULL))
  }
  low <- coalesce(min_value, suppressWarnings(min(observed)))
  high <- coalesce(max_value, suppressWarnings(max(observed)))
  list(
    values = (as.numeric(low) + as.numeric(high)) - x,
    warning = if (is.null(min_value) || is.null(max_value)) "Reverse-scoring bounds were inferred from observed item values." else NULL
  )
}

make_numeric_frame <- function(df, items, reverse_items, response_scale) {
  warnings <- character()
  selected <- df[, items, drop = FALSE]
  numeric_df <- data.frame(lapply(selected, function(column) suppressWarnings(as.numeric(column))), check.names = FALSE)
  names(numeric_df) <- items
  response_min <- if (is.list(response_scale)) normalize_scalar(response_scale$min) else NULL
  response_max <- if (is.list(response_scale)) normalize_scalar(response_scale$max) else NULL
  for (item in intersect(reverse_items, items)) {
    rescored <- reverse_score_item(numeric_df[[item]], response_min, response_max)
    numeric_df[[item]] <- rescored$values
    if (!is.null(rescored$warning)) {
      warnings <- c(warnings, sprintf("%s (%s)", rescored$warning, item))
    }
  }
  list(data = numeric_df, warnings = clean_messages(warnings))
}

build_reliability_scales <- function(scale_definitions, item_names) {
  if (length(scale_definitions)) {
    return(scale_definitions)
  }
  list(list(name = "all_items", items = item_names, reverseItems = character()))
}

summarize_reliability <- function(df, scale_definitions) {
  results <- list()
  warnings <- character()
  reliability_scales <- build_reliability_scales(scale_definitions, names(df))

  for (scale in reliability_scales) {
    scale_items <- intersect(scale$items, names(df))
    if (length(scale_items) < 2) {
      warnings <- c(warnings, sprintf("Scale %s skipped because it has fewer than 2 usable items.", scale$name))
      next
    }

    scale_df <- stats::na.omit(df[, scale_items, drop = FALSE])
    if (nrow(scale_df) < 3) {
      warnings <- c(warnings, sprintf("Scale %s skipped because too few complete cases remained after listwise deletion.", scale$name))
      next
    }

    diagnostics <- inspect_numeric_matrix(scale_df)
    scale_warnings <- prefix_messages(sprintf("Scale %s", scale$name), diagnostics$warnings)
    scale_result <- list(items = scale_items, completeCases = nrow(scale_df))

    alpha_capture <- evaluate_quietly(psych::alpha(scale_df, warnings = FALSE, check.keys = FALSE))
    alpha_result <- alpha_capture$value
    scale_warnings <- c(scale_warnings, prefix_messages(sprintf("Alpha %s", scale$name), alpha_capture$warnings))
    if (!inherits(alpha_result, "error")) {
      scale_result$alpha <- unname(alpha_result$total$raw_alpha)
    } else {
      scale_warnings <- c(scale_warnings, sprintf("Alpha failed for %s: %s", scale$name, alpha_result$message))
    }

    if (diagnostics$isSingular) {
      scale_warnings <- c(scale_warnings, sprintf("Omega skipped for %s because the item correlation matrix was singular or not positive definite.", scale$name))
    } else {
      omega_capture <- evaluate_quietly(psych::omega(scale_df, plot = FALSE, warnings = FALSE))
      omega_result <- omega_capture$value
      scale_warnings <- c(scale_warnings, prefix_messages(sprintf("Omega %s", scale$name), omega_capture$warnings))
      if (!inherits(omega_result, "error") && is.finite(omega_result$omega.tot)) {
        scale_result$omega <- unname(omega_result$omega.tot)
      } else if (inherits(omega_result, "error")) {
        scale_warnings <- c(scale_warnings, sprintf("Omega failed for %s: %s", scale$name, omega_result$message))
      } else {
        scale_warnings <- c(scale_warnings, sprintf("Omega failed for %s because a finite omega estimate was not returned.", scale$name))
      }
    }

    results[[scale$name]] <- scale_result
    warnings <- c(warnings, scale_warnings)
  }

  list(results = results, warnings = clean_messages(warnings))
}

summarize_validity <- function(df) {
  diagnostics <- inspect_numeric_matrix(df)
  warnings <- diagnostics$warnings
  usable <- diagnostics$usable

  if (ncol(usable) < 2 || nrow(usable) < 3) {
    return(list(result = NULL, warnings = c(warnings, "Validity pre-checks skipped because too few complete rows/items were available.")))
  }
  if (is.null(diagnostics$correlation) || any(!is.finite(diagnostics$correlation))) {
    return(list(result = NULL, warnings = c(warnings, "Validity pre-checks skipped because the correlation matrix was unusable.")))
  }

  result <- list(completeCases = diagnostics$completeCases)

  if (diagnostics$isSingular) {
    warnings <- c(warnings, "KMO skipped because the item correlation matrix was singular or not positive definite.")
  } else {
    kmo_capture <- evaluate_quietly(psych::KMO(diagnostics$correlation))
    warnings <- c(warnings, prefix_messages("KMO", kmo_capture$warnings))
    kmo <- kmo_capture$value
    if (!inherits(kmo, "error")) {
      result$kmo <- unname(kmo$MSA)
    } else {
      warnings <- c(warnings, sprintf("KMO failed: %s", kmo$message))
    }
  }

  bartlett_capture <- evaluate_quietly(psych::cortest.bartlett(diagnostics$correlation, n = diagnostics$completeCases))
  warnings <- c(warnings, prefix_messages("Bartlett", bartlett_capture$warnings))
  bartlett <- bartlett_capture$value

  if (!inherits(bartlett, "error")) {
    result$bartlett <- list(
      chiSquare = unname(bartlett$chisq),
      df = unname(bartlett$df),
      pValue = unname(bartlett$p.value)
    )
  } else {
    warnings <- c(warnings, sprintf("Bartlett failed: %s", bartlett$message))
  }

  list(result = result, warnings = clean_messages(warnings))
}

summarize_cfa <- function(df, payload) {
  model_spec <- normalize_scalar(payload$cfaModel)
  if (is.null(model_spec) || !nzchar(trimws(model_spec))) {
    return(list(result = NULL, warnings = character()))
  }

  model_items <- intersect(extract_model_variables(model_spec), names(df))
  cfa_df <- if (length(model_items) >= 2) df[, model_items, drop = FALSE] else df
  diagnostics <- inspect_numeric_matrix(cfa_df)
  warnings <- diagnostics$warnings

  if (ncol(cfa_df) < 2 || nrow(cfa_df) < 3) {
    return(list(result = NULL, warnings = c(warnings, "CFA skipped because too few rows/items were available for model estimation.")))
  }
  if (diagnostics$hasPerfectCorrelations || diagnostics$isSingular) {
    return(list(result = NULL, warnings = c(warnings, "CFA skipped because the observed item correlations were singular or perfectly collinear.")))
  }

  estimator <- normalize_scalar(payload$estimator, "MLR")
  missing_handling <- normalize_scalar(payload$missingHandling, "fiml")
  fit_capture <- evaluate_quietly(lavaan::cfa(model_spec, data = df, estimator = estimator, missing = missing_handling, std.lv = TRUE))
  fit <- fit_capture$value
  warnings <- c(warnings, prefix_messages("CFA", fit_capture$warnings))

  if (inherits(fit, "error")) {
    return(list(result = NULL, warnings = c(warnings, sprintf("CFA failed: %s", fit$message))))
  }

  measures_capture <- evaluate_quietly(lavaan::fitMeasures(fit, c("chisq", "df", "pvalue", "cfi", "tli", "rmsea", "srmr")))
  standardized_capture <- evaluate_quietly(lavaan::standardizedSolution(fit))
  measures <- measures_capture$value
  standardized <- standardized_capture$value
  warnings <- c(
    warnings,
    prefix_messages("CFA fitMeasures", measures_capture$warnings),
    prefix_messages("CFA standardizedSolution", standardized_capture$warnings)
  )

  result <- list(
    converged = isTRUE(lavaan::inspect(fit, "converged")),
    estimator = estimator,
    missing = missing_handling
  )
  if (!inherits(measures, "error")) {
    result$fit <- as.list(measures)
  } else {
    warnings <- c(warnings, sprintf("CFA fit measures failed: %s", measures$message))
  }

  if (!inherits(standardized, "error")) {
    loadings <- standardized[standardized$op == "=~", c("lhs", "rhs", "est.std"), drop = FALSE]
    result$standardizedLoadings <- if (nrow(loadings)) {
      unname(apply(loadings, 1, function(row) {
        list(factor = unname(row[[1]]), item = unname(row[[2]]), estimate = as.numeric(row[[3]]))
      }))
    } else {
      list()
    }
  } else {
    warnings <- c(warnings, sprintf("CFA standardized loadings failed: %s", standardized$message))
  }

  list(result = result, warnings = clean_messages(warnings))
}

analysis_id <- normalize_scalar(payload$analysisId, basename(normalize_scalar(payload$datasetPath, "analysis")))
dataset_path_input <- normalize_scalar(payload$datasetPath)
if (is.null(dataset_path_input) || !nzchar(trimws(dataset_path_input))) {
  tool_error(plugin_id, tool_name, "missing_dataset_path", "datasetPath is required for survey_psychometrics")
}

dataset_path <- resolve_existing_path(dataset_path_input)
if (!file.exists(dataset_path)) {
  tool_error(plugin_id, tool_name, "dataset_not_found", sprintf("dataset not found: %s", dataset_path), list(datasetPath = dataset_path_input, resolvedPath = dataset_path))
}

format <- infer_format(dataset_path, normalize_scalar(payload$format, "auto"))
dataset <- read_dataset(dataset_path, format, payload)
names(dataset) <- names(dataset) |> as.character()
scale_definitions <- extract_scale_definitions(payload)
top_level_reverse <- normalize_string_list(payload$reverseItems)
analysis_items_info <- collect_analysis_items(scale_definitions, normalize_scalar(payload$cfaModel), dataset)
analysis_items <- unique(analysis_items_info$items)
warnings <- clean_messages(analysis_items_info$warnings)
if (!length(analysis_items)) {
  tool_error(plugin_id, tool_name, "no_analysis_items", "No usable questionnaire items were provided for psychometric analysis")
}

missing_items <- setdiff(analysis_items, names(dataset))
usable_items <- intersect(analysis_items, names(dataset))
if (length(usable_items) < 2) {
  tool_error(plugin_id, tool_name, "insufficient_items", "Psychometric analysis requires at least two usable items", list(missingItems = missing_items, availableColumns = names(dataset)))
}
if (length(missing_items)) {
  warnings <- c(warnings, sprintf("%s declared items were not found in the dataset.", length(missing_items)))
}

reverse_items <- unique(c(top_level_reverse, unlist(lapply(scale_definitions, function(x) x$reverseItems), use.names = FALSE)))
coerced <- make_numeric_frame(dataset, usable_items, reverse_items, payload$responseScale)
warnings <- c(warnings, coerced$warnings)
numeric_df <- coerced$data

all_na_items <- names(numeric_df)[vapply(numeric_df, function(col) all(is.na(col)), logical(1))]
if (length(all_na_items)) {
  numeric_df <- numeric_df[, setdiff(names(numeric_df), all_na_items), drop = FALSE]
  warnings <- c(warnings, sprintf("Dropped %s items that could not be coerced to numeric values.", length(all_na_items)))
}

constant_items <- names(numeric_df)[vapply(numeric_df, function(col) {
  observed <- unique(col[!is.na(col)])
  length(observed) < 2
}, logical(1))]
if (length(constant_items)) {
  numeric_df <- numeric_df[, setdiff(names(numeric_df), constant_items), drop = FALSE]
  warnings <- c(warnings, sprintf("Dropped %s items with no observed variance.", length(constant_items)))
}

if (ncol(numeric_df) < 2) {
  tool_error(plugin_id, tool_name, "insufficient_numeric_items", "Fewer than two numeric items remained after coercion", list(droppedItems = unique(c(all_na_items, constant_items))))
}

reliability <- summarize_reliability(numeric_df, scale_definitions)
validity <- summarize_validity(numeric_df)
cfa <- summarize_cfa(numeric_df, payload)

result <- list(
  plugin = plugin_id,
  tool = tool_name,
  status = "ok",
  analysisId = analysis_id,
  dataset = list(
    path = dataset_path,
    workspaceRelativePath = if (startsWith(dataset_path, paste0(workspace_root, "/"))) sub(paste0("^", workspace_root, "/"), "", dataset_path) else NULL,
    format = format,
    rows = nrow(dataset),
    columns = ncol(dataset),
    analysisItems = names(numeric_df)
  ),
  reliability = reliability$results,
  validity = validity$result,
  cfa = cfa$result,
  warnings = clean_messages(c(warnings, reliability$warnings, validity$warnings, cfa$warnings))
)

emit_json(result)
