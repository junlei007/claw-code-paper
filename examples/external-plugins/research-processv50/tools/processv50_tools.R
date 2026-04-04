#!/usr/bin/env Rscript

suppressPackageStartupMessages({
  library(jsonlite)
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

if (tool_name != "processv50_run") {
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

field <- function(name) {
  payload[[name, exact = TRUE]]
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
  if (is.character(value) && length(value) == 1) {
    pieces <- trimws(unlist(strsplit(value, ",", fixed = TRUE), use.names = FALSE))
    return(pieces[nzchar(pieces)])
  }
  values <- as.character(unlist(value, use.names = FALSE))
  values <- trimws(values)
  values[nzchar(values)]
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

workspace_relative_path <- function(path) {
  normalized <- normalizePath(path, winslash = "/", mustWork = FALSE)
  prefix <- paste0(workspace_root, "/")
  if (startsWith(normalized, prefix)) {
    sub(paste0("^", workspace_root, "/"), "", normalized)
  } else {
    NULL
  }
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

default_process_script_path <- function() {
  candidate <- normalizePath(file.path(workspace_root, "processv50", "PROCESS_R_v5", "process.R"), winslash = "/", mustWork = FALSE)
  if (file.exists(candidate)) candidate else NULL
}

capture_report <- function(expr) {
  output <- capture.output(result <- eval.parent(substitute(expr)), type = "output")
  list(result = result, output = output)
}

parse_scalar_token <- function(value) {
  text <- trimws(as.character(value))
  if (!nzchar(text) || text %in% c("xxxxx", "XXXXX")) {
    return(NULL)
  }
  if (grepl("^-?[0-9]+$", text)) {
    return(as.integer(text))
  }
  if (grepl("^-?([0-9]+\\.[0-9]*|\\.[0-9]+)$", text)) {
    return(as.numeric(text))
  }
  text
}

parse_key_value_fields <- function(lines) {
  fields <- list()
  pattern <- "([A-Za-z][A-Za-z0-9]*)=([^[:space:]]+)"
  for (line in lines) {
    tokens <- regmatches(line, gregexpr(pattern, line, perl = TRUE))[[1]]
    if (!length(tokens)) {
      next
    }
    for (token in tokens) {
      parts <- strsplit(token, "=", fixed = TRUE)[[1]]
      if (length(parts) != 2) {
        next
      }
      parsed <- parse_scalar_token(parts[[2]])
      if (!is.null(parsed)) {
        fields[[parts[[1]]]] <- parsed
      }
    }
  }
  fields
}

extract_outcomes <- function(lines) {
  outcomes <- character()
  for (index in seq_along(lines)) {
    if (trimws(lines[[index]]) != "Outcome Variable:") {
      next
    }
    if (index >= length(lines)) {
      next
    }
    trailing <- lines[(index + 1):length(lines)]
    trailing <- trailing[nzchar(trimws(trailing))]
    if (length(trailing)) {
      outcomes <- c(outcomes, trimws(trailing[[1]]))
    }
  }
  unname(unique(outcomes))
}

split_table_columns <- function(line) {
  trimmed <- trimws(line)
  if (!nzchar(trimmed)) {
    return(character())
  }
  unlist(strsplit(trimmed, "\\s{2,}", perl = TRUE), use.names = FALSE)
}

parse_table_block <- function(lines) {
  lines <- lines[nzchar(trimws(lines))]
  if (!length(lines)) {
    return(NULL)
  }
  columns <- split_table_columns(lines[[1]])
  if (!length(columns)) {
    return(list(rawLines = unname(lines)))
  }
  if (length(lines) == 1) {
    return(list(rawLines = unname(lines)))
  }

  row_tokens <- lapply(lines[-1], split_table_columns)
  row_tokens <- row_tokens[vapply(row_tokens, length, integer(1)) > 0]
  if (!length(row_tokens)) {
    return(list(columns = unname(columns), rows = list()))
  }

  build_row <- function(tokens, with_row_label) {
    values <- if (with_row_label) tokens[-1] else tokens
    row <- lapply(values, parse_scalar_token)
    names(row) <- columns
    if (with_row_label) {
      c(list(.row = trimws(tokens[[1]])), row)
    } else {
      row
    }
  }

  token_lengths <- vapply(row_tokens, length, integer(1))
  if (all(token_lengths == length(columns))) {
    return(list(
      columns = unname(columns),
      rows = unname(lapply(row_tokens, build_row, with_row_label = FALSE))
    ))
  }
  if (all(token_lengths == (length(columns) + 1))) {
    return(list(
      columns = c(".row", unname(columns)),
      rows = unname(lapply(row_tokens, build_row, with_row_label = TRUE))
    ))
  }

  list(rawLines = unname(lines))
}

normalize_column_key <- function(name) {
  tolower(gsub("[^A-Za-z0-9]+", "", as.character(name)))
}

first_present_value <- function(row, keys) {
  if (!is.list(row) || !length(row)) {
    return(NULL)
  }
  normalized <- vapply(names(row), normalize_column_key, character(1))
  for (key in keys) {
    match_index <- match(normalize_column_key(key), normalized)
    if (!is.na(match_index)) {
      value <- row[[match_index]]
      if (!is.null(value)) {
        return(value)
      }
    }
  }
  NULL
}

normalize_effect_rows <- function(section) {
  if (!is.list(section) || is.null(section$rows) || !length(section$rows)) {
    return(list())
  }
  unname(lapply(section$rows, function(row) {
    normalized <- list(
      label = first_present_value(row, c(".row")),
      effect = first_present_value(row, c("Effect", "Index", "Contrast")),
      standardError = first_present_value(row, c("se", "SE", "BootSE")),
      statistic = first_present_value(row, c("t", "Z", "Chi-sq")),
      pValue = first_present_value(row, c("p")),
      lowerCI = first_present_value(row, c("LLCI", "BootLLCI", "Lower")),
      upperCI = first_present_value(row, c("ULCI", "BootULCI", "Upper"))
    )
    Filter(Negate(is.null), normalized)
  }))
}

build_effect_summary <- function(sections) {
  summary <- list()
  mappings <- list(
    direct = "directEffect",
    conditionalDirect = "conditionalDirectEffects",
    indirect = "indirectEffects",
    conditionalIndirect = "conditionalIndirectEffects",
    conditionalAndUnconditionalIndirect = "conditionalAndUnconditionalIndirectEffects",
    moderatedMediationIndex = "moderatedMediationIndex",
    partialModeratedMediationIndices = "partialModeratedMediationIndices",
    moderatedModeratedMediationIndices = "moderatedModeratedMediationIndices",
    conditionalModeratedMediationIndices = "conditionalModeratedMediationIndices"
  )

  for (name in names(mappings)) {
    section <- sections[[mappings[[name]]]]
    if (is.null(section)) {
      next
    }
    if (is.list(section) && !is.null(section$rows)) {
      normalized <- normalize_effect_rows(section)
      if (length(normalized)) {
        summary[[name]] <- normalized
      }
      next
    }
    if (is.list(section) && length(section) && is.null(names(section[[1]]))) {
      normalized_blocks <- unname(lapply(section, normalize_effect_rows))
      normalized_blocks <- Filter(length, normalized_blocks)
      if (length(normalized_blocks)) {
        summary[[name]] <- normalized_blocks
      }
    }
  }

  summary
}

extract_section_block <- function(lines, start_index, headings) {
  captured <- character()
  if (start_index >= length(lines)) {
    return(captured)
  }
  for (index in (start_index + 1):length(lines)) {
    trimmed <- trimws(lines[[index]])
    if (!nzchar(trimmed)) {
      if (length(captured)) {
        break
      }
      next
    }
    if (trimmed %in% headings || trimmed == "Outcome Variable:" || grepl("^[-*]{3,}$", trimmed)) {
      break
    }
    captured <- c(captured, lines[[index]])
  }
  captured
}

parse_report_text <- function(report_text) {
  lines <- unlist(strsplit(report_text, "\n", fixed = TRUE), use.names = FALSE)
  section_titles <- c(
    modelSummary = "Model Summary:",
    directEffect = "Direct effect of X on Y:",
    conditionalDirectEffects = "Conditional direct effect(s) of X on Y:",
    indirectEffects = "Indirect effect(s) of X on Y:",
    conditionalIndirectEffects = "Conditional indirect effects of X on Y:",
    conditionalAndUnconditionalIndirectEffects = "Conditional and unconditional indirect effects of X on Y:",
    moderatedMediationIndex = "Index of moderated mediation:",
    partialModeratedMediationIndices = "Indices of partial moderated mediation:",
    moderatedModeratedMediationIndices = "Indices of moderated moderated mediation:",
    conditionalModeratedMediationIndices = "Indices of conditional moderated mediation by W:"
  )

  sections <- list()
  for (name in names(section_titles)) {
    matches <- which(trimws(lines) == section_titles[[name]])
    if (!length(matches)) {
      next
    }
    parsed_blocks <- lapply(matches, function(index) {
      parse_table_block(extract_section_block(lines, index, unname(section_titles)))
    })
    parsed_blocks <- Filter(Negate(is.null), parsed_blocks)
    if (!length(parsed_blocks)) {
      next
    }
    sections[[name]] <- if (length(parsed_blocks) == 1) parsed_blocks[[1]] else unname(parsed_blocks)
  }

  list(
    parserVersion = "0.1.0",
    rawLineCount = length(lines),
    outcomes = extract_outcomes(lines),
    keyValueFields = parse_key_value_fields(lines),
    detectedSections = unname(names(sections)),
    sections = sections,
    effectSummary = build_effect_summary(sections)
  )
}

dataset_path_input <- normalize_scalar(field("datasetPath"))
if (is.null(dataset_path_input) || !nzchar(trimws(dataset_path_input))) {
  tool_error(plugin_id, tool_name, "missing_dataset_path", "datasetPath is required for processv50_run")
}

dataset_path <- resolve_existing_path(dataset_path_input)
if (!file.exists(dataset_path)) {
  tool_error(plugin_id, tool_name, "dataset_not_found", sprintf("dataset not found: %s", dataset_path), list(datasetPath = dataset_path_input, resolvedPath = dataset_path))
}

model <- suppressWarnings(as.integer(normalize_scalar(field("model"))))
if (is.na(model)) {
  tool_error(plugin_id, tool_name, "missing_model", "model must be an integer for processv50_run")
}

y_value <- normalize_scalar(field("y"))
x_value <- normalize_scalar(field("x"))
if (is.null(y_value) || !nzchar(trimws(y_value)) || is.null(x_value) || !nzchar(trimws(x_value))) {
  tool_error(plugin_id, tool_name, "missing_roles", "y and x are required for processv50_run")
}

process_script_raw <- normalize_scalar(field("processScriptPath"), Sys.getenv("PROCESSV50_R_PATH", ""))
if (is.null(process_script_raw) || !nzchar(trimws(process_script_raw))) {
  process_script_raw <- default_process_script_path()
}
if (is.null(process_script_raw) || !nzchar(trimws(process_script_raw))) {
  tool_error(plugin_id, tool_name, "missing_process_script", "Provide processScriptPath or PROCESSV50_R_PATH to a local process.R", list(envVar = "PROCESSV50_R_PATH"))
}

process_script_path <- resolve_existing_path(process_script_raw)
if (!file.exists(process_script_path)) {
  tool_error(plugin_id, tool_name, "process_script_not_found", sprintf("process script not found: %s", process_script_path), list(processScriptPath = process_script_raw, resolvedPath = process_script_path))
}

source(process_script_path, local = .GlobalEnv)
if (!exists("process", mode = "function")) {
  tool_error(plugin_id, tool_name, "invalid_process_script", "the sourced script did not define a callable process() function", list(processScriptPath = process_script_path))
}

delimiter <- normalize_scalar(field("delimiter"), ",")
encoding <- normalize_scalar(field("encoding"), "UTF-8")
na_values <- normalize_string_list(field("naValues"))
dataset <- read_dataset(dataset_path, delimiter, encoding, na_values)

m_value <- normalize_string_list(field("m"))
w_value <- normalize_string_list(field("w"))
z_value <- normalize_string_list(field("z"))
cov_value <- normalize_string_list(field("covariates"))
boot_value <- suppressWarnings(as.integer(normalize_scalar(field("boot"), 5000)))
conf_value <- suppressWarnings(as.numeric(normalize_scalar(field("conf"), 95)))
center_value <- suppressWarnings(as.integer(normalize_scalar(field("center"), 0)))
seed_value <- suppressWarnings(as.integer(normalize_scalar(field("seed"), -999)))

call_capture <- tryCatch(
  capture_report(
    process(
      data = dataset,
      y = y_value,
      x = x_value,
      m = if (length(m_value)) m_value else "xxxxx",
      w = if (length(w_value)) w_value else "xxxxx",
      z = if (length(z_value)) z_value else "xxxxx",
      cov = if (length(cov_value)) cov_value else "xxxxx",
      model = model,
      boot = if (is.na(boot_value)) 5000 else boot_value,
      conf = if (is.na(conf_value)) 95 else conf_value,
      center = if (is.na(center_value)) 0 else center_value,
      seed = if (is.na(seed_value)) -999 else seed_value,
      outscreen = 1,
      progress = 0,
      save = 0
    )
  ),
  error = function(err) {
    tool_error(plugin_id, tool_name, "process_execution_failed", err$message, list(processScriptPath = process_script_path))
  }
)

report_text <- paste(call_capture$output, collapse = "\n")
if (!nzchar(trimws(report_text))) {
  report_text <- "PROCESSv50 run completed without captured console output."
}
report_parse <- parse_report_text(report_text)

output_path_raw <- normalize_scalar(field("outputPath"))
if (is.null(output_path_raw) || !nzchar(trimws(output_path_raw))) {
  output_path_raw <- file.path(".claw", "artifacts", sprintf("method-processv50-model-%s.txt", model))
}
output_path <- resolve_output_path(output_path_raw)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
writeLines(report_text, output_path, useBytes = TRUE)

warnings <- character()
if (is.null(normalize_scalar(field("processScriptPath"))) && !nzchar(Sys.getenv("PROCESSV50_R_PATH", ""))) {
  warnings <- c(warnings, "processv50_run fell back to the workspace-local default processv50/PROCESS_R_v5/process.R path.")
}
if (!length(m_value) && model %in% c(4, 6, 80, 81, 82)) {
  warnings <- c(warnings, "No mediator variables were provided even though the selected model is often used for mediation-style workflows.")
}
if (!length(w_value) && model %in% c(1, 7, 8, 14, 58, 59)) {
  warnings <- c(warnings, "No moderator variable was provided even though the selected model is often used for moderation-style workflows.")
}

emit_json(list(
  plugin = plugin_id,
  tool = tool_name,
  status = "ok",
  dataset = list(
    path = dataset_path,
    workspaceRelativePath = workspace_relative_path(dataset_path),
    rows = nrow(dataset),
    columns = ncol(dataset)
  ),
  analysis = list(
    model = model,
    y = y_value,
    x = x_value,
    m = unname(m_value),
    w = unname(w_value),
    z = unname(z_value),
    covariates = unname(cov_value),
    boot = if (is.na(boot_value)) NULL else boot_value,
    conf = if (is.na(conf_value)) NULL else conf_value,
    center = if (is.na(center_value)) NULL else center_value,
    seed = if (is.na(seed_value)) NULL else seed_value
  ),
  processScript = list(
    path = process_script_path,
    workspaceRelativePath = workspace_relative_path(process_script_path)
  ),
  artifacts = list(
    report = list(
      path = output_path,
      workspaceRelativePath = workspace_relative_path(output_path),
      kind = "text"
    )
  ),
  reportParse = report_parse,
  warnings = unname(warnings)
))
