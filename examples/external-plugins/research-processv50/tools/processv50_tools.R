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

derive_json_sidecar_path <- function(path) {
  replaced <- sub("(\\.[^./]+)$", ".json", path, perl = TRUE)
  if (!identical(replaced, path)) {
    return(replaced)
  }
  paste0(path, ".json")
}

derive_suffixed_artifact_path <- function(path, suffix, extension) {
  base <- sub("(\\.[^./]+)$", "", path, perl = TRUE)
  paste0(base, suffix, extension)
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

format_label_scalar <- function(value) {
  if (is.null(value)) {
    return(NULL)
  }
  if (is.numeric(value)) {
    return(format(round(value, 3), nsmall = 3, trim = TRUE))
  }
  trimws(as.character(value))
}

extract_effect_row_components <- function(row) {
  if (!is.list(row) || !length(row)) {
    return(list())
  }

  explicit <- first_present_value(row, c(".row"))
  if (!is.null(explicit)) {
    return(list(.row = format_label_scalar(explicit)))
  }

  normalized_names <- vapply(names(row), normalize_column_key, character(1))
  ignore_keys <- c(
    "effect", "index", "contrast", "se", "bootse", "t", "z", "chisq", "p",
    "llci", "bootllci", "lower", "ulci", "bootulci", "upper"
  )
  candidate_indices <- which(!normalized_names %in% ignore_keys)
  if (!length(candidate_indices)) {
    return(list())
  }

  components <- list()
  for (index in candidate_indices) {
    candidate_name <- names(row)[[index]]
    candidate_value <- row[[index]]
    if (is.null(candidate_value)) {
      next
    }
    components[[candidate_name]] <- format_label_scalar(candidate_value)
  }
  components
}

extract_effect_row_label <- function(row) {
  components <- extract_effect_row_components(row)
  if (!length(components)) {
    return(NULL)
  }
  if (!is.null(components$.row)) {
    return(components$.row)
  }
  paste(
    vapply(names(components), function(name) sprintf("%s=%s", name, components[[name]]), character(1)),
    collapse = " · "
  )
}

normalize_effect_rows <- function(section) {
  if (!is.list(section) || is.null(section$rows) || !length(section$rows)) {
    return(list())
  }
  unname(lapply(section$rows, function(row) {
    normalized <- list(
      label = extract_effect_row_label(row),
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

extract_regex_section_block <- function(lines, start_index, exact_headings, regex_headings) {
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
    if (
      trimmed %in% exact_headings ||
      any(vapply(regex_headings, function(pattern) grepl(pattern, trimmed, perl = TRUE), logical(1))) ||
      trimmed == "Outcome Variable:" ||
      grepl("^[-*]{3,}$", trimmed)
    ) {
      break
    }
    captured <- c(captured, lines[[index]])
  }
  captured
}

find_regex_heading_matches <- function(lines, pattern) {
  which(vapply(trimws(lines), function(line) grepl(pattern, line, perl = TRUE), logical(1)))
}

parse_space_separated_row <- function(line, expected_columns = NULL) {
  tokens <- strsplit(trimws(line), "\\s+", perl = TRUE)[[1]]
  tokens <- tokens[nzchar(tokens)]
  if (!length(tokens)) {
    return(NULL)
  }
  if (!is.null(expected_columns) && length(tokens) != expected_columns) {
    return(NULL)
  }
  unname(lapply(tokens, parse_scalar_token))
}

build_visualization_blocks <- function(lines) {
  heading <- "Data for visualizing the conditional effect of the focal predictor:"
  matches <- which(trimws(lines) == heading)
  if (!length(matches)) {
    return(list())
  }

  blocks <- list()
  for (index in matches) {
    begin_candidates <- which(trimws(lines) == "BEGIN DATA." & seq_along(lines) > index)
    if (!length(begin_candidates)) {
      next
    }
    begin_index <- begin_candidates[[1]]
    if (begin_index <= (index + 1)) {
      next
    }

    header_index <- begin_index - 1
    while (header_index > index && !nzchar(trimws(lines[[header_index]]))) {
      header_index <- header_index - 1
    }
    if (header_index <= index) {
      next
    }

    columns <- strsplit(trimws(lines[[header_index]]), "\\s+", perl = TRUE)[[1]]
    columns <- columns[nzchar(columns) & columns != "."]
    if (!length(columns)) {
      next
    }

    end_candidates <- which(trimws(lines) == "END DATA." & seq_along(lines) > begin_index)
    if (!length(end_candidates)) {
      next
    }
    end_index <- end_candidates[[1]]
    data_lines <- lines[(begin_index + 1):(end_index - 1)]
    data_lines <- data_lines[nzchar(trimws(data_lines))]
    if (!length(data_lines)) {
      next
    }

    rows <- lapply(data_lines, parse_space_separated_row, expected_columns = length(columns))
    rows <- Filter(Negate(is.null), rows)
    if (!length(rows)) {
      next
    }
    rows <- lapply(rows, function(row) {
      names(row) <- columns
      row
    })

    blocks[[length(blocks) + 1]] <- list(
      columns = unname(columns),
      rows = unname(rows)
    )
  }

  unname(blocks)
}

normalize_region_table_rows <- function(section) {
  if (!is.list(section) || is.null(section$rows) || !length(section$rows)) {
    return(list())
  }
  unname(lapply(section$rows, function(row) {
    normalized <- list(
      value = first_present_value(row, c("Value")),
      percentBelow = first_present_value(row, c("% below", "Percent below")),
      percentAbove = first_present_value(row, c("% above", "Percent above"))
    )
    Filter(Negate(is.null), normalized)
  }))
}

normalize_conditional_effect_table_rows <- function(section) {
  if (!is.list(section) || is.null(section$rows) || !length(section$rows)) {
    return(list())
  }
  unname(lapply(section$rows, function(row) {
    label_components <- extract_effect_row_components(row)
    normalized <- list(
      label = extract_effect_row_label(row),
      moderatorValue = first_present_value(row, c(".row", "Moderator", "W", "Z")),
      effect = first_present_value(row, c("Effect")),
      standardError = first_present_value(row, c("se", "SE")),
      statistic = first_present_value(row, c("t", "Z")),
      pValue = first_present_value(row, c("p")),
      lowerCI = first_present_value(row, c("LLCI", "Lower")),
      upperCI = first_present_value(row, c("ULCI", "Upper"))
    )
    if (is.null(normalized$moderatorValue)) {
      label <- normalized$label
      if (!is.null(label) && grepl("=", label, fixed = TRUE)) {
        normalized$moderatorValue <- sub("^.*?=", "", label)
      }
    }
    if (length(label_components) && is.null(label_components$.row)) {
      normalized$moderators <- label_components
    }
    Filter(Negate(is.null), normalized)
  }))
}

build_johnson_neyman_summary <- function(sections) {
  summary <- list()
  if (!is.null(sections$johnsonNeymanRegions)) {
    regions <- normalize_region_table_rows(sections$johnsonNeymanRegions)
    if (length(regions)) {
      summary$regions <- regions
      summary$bounds <- unname(Filter(Negate(is.null), lapply(regions, `[[`, "value")))
    }
  }
  if (!is.null(sections$johnsonNeymanConditionalEffects)) {
    conditional_effects <- normalize_conditional_effect_table_rows(sections$johnsonNeymanConditionalEffects)
    if (length(conditional_effects)) {
      summary$conditionalEffects <- conditional_effects
    }
  }
  if (!length(summary)) {
    return(list())
  }
  summary
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
  regex_section_titles <- c(
    johnsonNeymanRegions = "^Moderator value\\(s\\) defining Johnson-Neyman significance region\\(s\\):$",
    johnsonNeymanConditionalEffects = "^Conditional effect(?:s)? of focal predictor at values of the moderator(?:\\(s\\))?:$"
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

  for (name in names(regex_section_titles)) {
    matches <- find_regex_heading_matches(lines, regex_section_titles[[name]])
    if (!length(matches)) {
      next
    }
    parsed_blocks <- lapply(matches, function(index) {
      parse_table_block(extract_regex_section_block(
        lines,
        index,
        unname(section_titles),
        unname(regex_section_titles)
      ))
    })
    parsed_blocks <- Filter(Negate(is.null), parsed_blocks)
    if (!length(parsed_blocks)) {
      next
    }
    sections[[name]] <- if (length(parsed_blocks) == 1) parsed_blocks[[1]] else unname(parsed_blocks)
  }

  visualization_data <- build_visualization_blocks(lines)
  johnson_neyman <- build_johnson_neyman_summary(sections)

  list(
    parserVersion = "0.2.0",
    rawLineCount = length(lines),
    outcomes = extract_outcomes(lines),
    keyValueFields = parse_key_value_fields(lines),
    detectedSections = unname(names(sections)),
    sections = sections,
    effectSummary = build_effect_summary(sections),
    visualizationData = visualization_data,
    johnsonNeyman = johnson_neyman
  )
}

artifact_entry <- function(path, kind) {
  list(
    path = path,
    workspaceRelativePath = workspace_relative_path(path),
    kind = kind
  )
}

find_matching_column <- function(columns, target) {
  normalized_columns <- vapply(columns, normalize_column_key, character(1))
  match_index <- match(normalize_column_key(target), normalized_columns)
  if (is.na(match_index)) {
    return(NULL)
  }
  columns[[match_index]]
}

visualization_block_to_dataframe <- function(block) {
  if (!is.list(block) || is.null(block$rows) || !length(block$rows)) {
    return(NULL)
  }
  columns <- block$columns
  if (is.null(columns) || !length(columns)) {
    columns <- unique(unlist(lapply(block$rows, names), use.names = FALSE))
  }
  matrix_rows <- do.call(rbind, lapply(block$rows, function(row) {
    values <- lapply(columns, function(column) row[[column]])
    values[is.na(match(columns, names(row)))] <- list(NULL)
    unlist(lapply(values, function(value) if (is.null(value)) NA else value), use.names = FALSE)
  }))
  data <- as.data.frame(matrix_rows, stringsAsFactors = FALSE, check.names = FALSE)
  names(data) <- columns
  data
}

extract_visualization_plot_data <- function(report_parse, x_name, y_name, w_name) {
  blocks <- report_parse$visualizationData
  if (is.null(blocks) || !length(blocks)) {
    return(NULL)
  }

  for (block in blocks) {
    if (is.null(block$columns) || !length(block$columns)) {
      next
    }
    x_column <- find_matching_column(block$columns, x_name)
    y_column <- find_matching_column(block$columns, y_name)
    w_column <- find_matching_column(block$columns, w_name)
    if (is.null(x_column) || is.null(y_column) || is.null(w_column)) {
      next
    }
    data <- visualization_block_to_dataframe(block)
    if (!nrow(data)) {
      next
    }
    selected <- data[, c(x_column, y_column, w_column), drop = FALSE]
    names(selected) <- c("x", "y", "moderator")
    extra_columns <- setdiff(names(data), c(x_column, y_column, w_column))
    selected$moderatorLabel <- vapply(seq_len(nrow(data)), function(index) {
      parts <- c(sprintf("%s=%s", w_name, format_label_scalar(data[[w_column]][[index]])))
      if (length(extra_columns)) {
        parts <- c(parts, vapply(extra_columns, function(column) {
          sprintf("%s=%s", column, format_label_scalar(data[[column]][[index]]))
        }, character(1)))
      }
      paste(parts, collapse = " · ")
    }, character(1))
    suppressWarnings({
      selected$x <- as.numeric(selected$x)
      selected$y <- as.numeric(selected$y)
      selected$moderator <- as.numeric(selected$moderator)
    })
    selected <- selected[stats::complete.cases(selected), , drop = FALSE]
    if (nrow(selected)) {
      return(selected)
    }
  }

  NULL
}

extract_jn_plot_data <- function(report_parse, w_name) {
  johnson_neyman <- report_parse$johnsonNeyman
  if (is.null(johnson_neyman) || is.null(johnson_neyman$conditionalEffects) || !length(johnson_neyman$conditionalEffects)) {
    return(NULL)
  }

  conditional_effects <- do.call(rbind, lapply(johnson_neyman$conditionalEffects, function(row) {
    columns <- c("label", "moderators", "moderatorValue", "effect", "standardError", "statistic", "pValue", "lowerCI", "upperCI")
    values <- unlist(lapply(columns, function(column) {
      value <- row[[column]]
      if (identical(column, "moderators")) {
        return(list(value))
      }
      if (is.null(value)) NA else value
    }), recursive = FALSE, use.names = FALSE)
    stats::setNames(values, columns)
  }))
  conditional_effects <- as.data.frame(conditional_effects, stringsAsFactors = FALSE, check.names = FALSE)
  if (!nrow(conditional_effects)) {
    return(NULL)
  }
  suppressWarnings({
    conditional_effects$moderatorValue <- as.numeric(conditional_effects$moderatorValue)
    conditional_effects$effect <- as.numeric(conditional_effects$effect)
    conditional_effects$lowerCI <- as.numeric(conditional_effects$lowerCI)
    conditional_effects$upperCI <- as.numeric(conditional_effects$upperCI)
  })
  if ("moderators" %in% names(conditional_effects)) {
    conditional_effects$moderatorValue <- ifelse(
      is.na(conditional_effects$moderatorValue),
      vapply(conditional_effects$moderators, function(value) {
        if (!is.list(value) || is.null(value[[w_name]])) {
          return(NA_real_)
        }
        suppressWarnings(as.numeric(value[[w_name]]))
      }, numeric(1)),
      conditional_effects$moderatorValue
    )
  }
  conditional_effects <- conditional_effects[
    stats::complete.cases(conditional_effects[, c("moderatorValue", "effect"), drop = FALSE]),
    ,
    drop = FALSE
  ]
  if (!nrow(conditional_effects)) {
    return(NULL)
  }

  bounds <- johnson_neyman$bounds
  if (is.null(bounds)) {
    bounds <- numeric()
  }
  suppressWarnings(bounds <- as.numeric(unlist(bounds, use.names = FALSE)))
  bounds <- bounds[is.finite(bounds)]

  significance_pattern <- if (!("lowerCI" %in% names(conditional_effects)) || !("upperCI" %in% names(conditional_effects))) {
    if (length(bounds)) "crosses-johnson-neyman-threshold" else "not-reported"
  } else if (all(conditional_effects$lowerCI > 0 | conditional_effects$upperCI < 0, na.rm = TRUE)) {
    "always-significant"
  } else if (!any(conditional_effects$lowerCI > 0 | conditional_effects$upperCI < 0, na.rm = TRUE)) {
    "never-significant"
  } else {
    "crosses-johnson-neyman-threshold"
  }

  list(
    grid = conditional_effects,
    bounds = unname(bounds),
    moderator = w_name,
    significancePattern = significance_pattern
  )
}

write_json_artifact <- function(path, value) {
  writeLines(json_text(value), path, useBytes = TRUE)
}

build_plot_metadata_payload <- function(plots) {
  if (is.null(plots) || !length(plots)) {
    return(NULL)
  }
  list(
    version = "0.1.0",
    plots = plots
  )
}

normalize_formula_name <- function(name) {
  paste0("`", gsub("`", "\\\\`", as.character(name), fixed = TRUE), "`")
}

build_moderation_formula <- function(y_name, x_name, w_name, covariates) {
  x_term <- normalize_formula_name(x_name)
  w_term <- normalize_formula_name(w_name)
  terms <- c(x_term, w_term, sprintf("%s:%s", x_term, w_term))
  if (length(covariates)) {
    terms <- c(terms, vapply(covariates, normalize_formula_name, character(1)))
  }
  stats::as.formula(sprintf("%s ~ %s", normalize_formula_name(y_name), paste(terms, collapse = " + ")))
}

match_main_effect_index <- function(coefficient_names, variable_name) {
  normalized_target <- normalize_column_key(variable_name)
  normalized_names <- vapply(coefficient_names, normalize_column_key, character(1))
  match(normalized_target, normalized_names)
}

match_interaction_effect_index <- function(coefficient_names, x_name, w_name) {
  target <- unname(sort(c(normalize_column_key(x_name), normalize_column_key(w_name))))
  for (index in seq_along(coefficient_names)) {
    pieces <- strsplit(gsub("`", "", coefficient_names[[index]], fixed = TRUE), ":", fixed = TRUE)[[1]]
    if (length(pieces) != 2) {
      next
    }
    if (identical(unname(sort(vapply(pieces, normalize_column_key, character(1)))), target)) {
      return(index)
    }
  }
  NA_integer_
}

compute_jn_bounds <- function(b_x, b_int, var_x, var_int, cov_x_int, t_critical) {
  a <- (b_int ^ 2) - ((t_critical ^ 2) * var_int)
  b <- (2 * b_x * b_int) - ((2 * (t_critical ^ 2)) * cov_x_int)
  c_term <- (b_x ^ 2) - ((t_critical ^ 2) * var_x)

  if (abs(a) < .Machine$double.eps) {
    if (abs(b) < .Machine$double.eps) {
      return(numeric())
    }
    return(-c_term / b)
  }

  discriminant <- (b ^ 2) - (4 * a * c_term)
  if (!is.finite(discriminant) || discriminant < 0) {
    return(numeric())
  }

  roots <- c(
    (-b - sqrt(discriminant)) / (2 * a),
    (-b + sqrt(discriminant)) / (2 * a)
  )
  sort(unique(roots[is.finite(roots)]))
}

generate_moderation_plot_bundle <- function(dataset, report_parse, model, x_name, y_name, w_name, covariates, conf_level, output_path) {
  conf_level <- if (is.finite(conf_level)) conf_level else 95
  visualization_data <- extract_visualization_plot_data(report_parse, x_name, y_name, w_name)
  jn_data <- extract_jn_plot_data(report_parse, w_name)
  decomposition_source <- if (!is.null(visualization_data)) "process-report" else "lm-fallback"
  jn_source <- if (!is.null(jn_data)) "process-report" else "lm-fallback"

  required_columns <- unique(c(x_name, y_name, w_name, covariates))
  missing_columns <- required_columns[!required_columns %in% names(dataset)]
  analysis_data <- NULL
  fallback_available <- FALSE
  fallback_warning <- NULL

  if (!length(missing_columns)) {
    non_numeric <- required_columns[!vapply(required_columns, function(name) is.numeric(dataset[[name]]), logical(1))]
    if (!length(non_numeric)) {
      analysis_data <- dataset[, required_columns, drop = FALSE]
      analysis_data <- analysis_data[stats::complete.cases(analysis_data), , drop = FALSE]
      if (nrow(analysis_data) >= 5) {
        moderator_sd <- stats::sd(analysis_data[[w_name]], na.rm = TRUE)
        if (is.finite(moderator_sd) && moderator_sd > 0) {
          fallback_available <- TRUE
        } else {
          fallback_warning <- sprintf("Skipped model-based moderation/JN fallback because moderator `%s` had zero or undefined variance.", w_name)
        }
      } else {
        fallback_warning <- "Skipped model-based moderation/JN fallback because fewer than 5 complete cases were available for the moderation model."
      }
    } else {
      fallback_warning <- sprintf(
        "Skipped model-based moderation/JN fallback because the current prototype requires numeric columns: %s",
        paste(non_numeric, collapse = ", ")
      )
    }
  } else {
    fallback_warning <- sprintf(
      "Skipped model-based moderation/JN fallback because required columns were missing: %s",
      paste(missing_columns, collapse = ", ")
    )
  }

  if (is.null(visualization_data) && is.null(jn_data) && !fallback_available) {
    warning_message <- if (!is.null(fallback_warning)) {
      fallback_warning
    } else {
      sprintf(
        "Skipped moderation/JN plots because PROCESS-native plot data were unavailable and no supported fallback could be constructed for model %s.",
        model
      )
    }
    return(list(
      artifacts = list(),
      plots = NULL,
      warnings = warning_message
    ))
  }

  if ((is.null(visualization_data) || is.null(jn_data)) && fallback_available) {
    fit <- tryCatch(
      stats::lm(build_moderation_formula(y_name, x_name, w_name, covariates), data = analysis_data),
      error = function(err) err
    )
    if (inherits(fit, "error")) {
      return(list(
        artifacts = list(),
        plots = NULL,
        warnings = sprintf("Skipped moderation/JN plots because the moderation model fallback failed to fit: %s", fit$message)
      ))
    }

    coefficient_names <- names(stats::coef(fit))
    x_index <- match_main_effect_index(coefficient_names, x_name)
    interaction_index <- match_interaction_effect_index(coefficient_names, x_name, w_name)
    if (is.na(x_index) || is.na(interaction_index)) {
      return(list(
        artifacts = list(),
        plots = NULL,
        warnings = "Skipped moderation/JN plots because the moderation model fallback did not expose the expected x and x:w coefficients."
      ))
    }

    residual_df <- stats::df.residual(fit)
    if (!is.finite(residual_df) || residual_df <= 0) {
      return(list(
        artifacts = list(),
        plots = NULL,
        warnings = "Skipped moderation/JN plots because the moderation model fallback had no residual degrees of freedom."
      ))
    }

    vcov_matrix <- stats::vcov(fit)
    b_x <- stats::coef(fit)[[x_index]]
    b_int <- stats::coef(fit)[[interaction_index]]
    var_x <- vcov_matrix[x_index, x_index]
    var_int <- vcov_matrix[interaction_index, interaction_index]
    cov_x_int <- vcov_matrix[x_index, interaction_index]
    if (!all(is.finite(c(b_x, b_int, var_x, var_int, cov_x_int)))) {
      return(list(
        artifacts = list(),
        plots = NULL,
        warnings = "Skipped moderation/JN plots because the moderation model fallback produced non-finite coefficient statistics."
      ))
    }

    if (is.null(visualization_data)) {
      moderator_sd <- stats::sd(analysis_data[[w_name]], na.rm = TRUE)
      moderator_mean <- mean(analysis_data[[w_name]], na.rm = TRUE)
      moderator_levels <- data.frame(
        level = c("Low (-1 SD)", "Mean", "High (+1 SD)"),
        moderatorValue = c(moderator_mean - moderator_sd, moderator_mean, moderator_mean + moderator_sd),
        stringsAsFactors = FALSE
      )
      x_grid <- seq(min(analysis_data[[x_name]], na.rm = TRUE), max(analysis_data[[x_name]], na.rm = TRUE), length.out = 100)
      visualization_data <- do.call(rbind, lapply(seq_len(nrow(moderator_levels)), function(index) {
        row <- moderator_levels[index, , drop = FALSE]
        block <- data.frame(
          x = x_grid,
          moderator = rep(row$moderatorValue, length(x_grid)),
          moderatorLabel = rep(row$level, length(x_grid)),
          stringsAsFactors = FALSE,
          check.names = FALSE
        )
        newdata <- data.frame(
          x_grid = x_grid,
          moderator_grid = rep(row$moderatorValue, length(x_grid)),
          stringsAsFactors = FALSE,
          check.names = FALSE
        )
        names(newdata)[names(newdata) == "x_grid"] <- x_name
        names(newdata)[names(newdata) == "moderator_grid"] <- w_name
        for (covariate in covariates) {
          newdata[[covariate]] <- rep(mean(analysis_data[[covariate]], na.rm = TRUE), length(x_grid))
        }
        block$y <- as.numeric(stats::predict(fit, newdata = newdata))
        block
      }))
      decomposition_source <- "lm-fallback"
    }

    if (is.null(jn_data)) {
      alpha <- (100 - conf_level) / 100
      t_critical <- stats::qt(1 - (alpha / 2), residual_df)
      jn_grid <- data.frame(moderatorValue = seq(
        min(analysis_data[[w_name]], na.rm = TRUE),
        max(analysis_data[[w_name]], na.rm = TRUE),
        length.out = 200
      ))
      jn_grid$effect <- b_x + (b_int * jn_grid$moderatorValue)
      jn_grid$standardError <- sqrt(
        pmax(
          0,
          var_x + ((jn_grid$moderatorValue ^ 2) * var_int) + (2 * jn_grid$moderatorValue * cov_x_int)
        )
      )
      jn_grid$lowerCI <- jn_grid$effect - (t_critical * jn_grid$standardError)
      jn_grid$upperCI <- jn_grid$effect + (t_critical * jn_grid$standardError)
      raw_bounds <- compute_jn_bounds(b_x, b_int, var_x, var_int, cov_x_int, t_critical)
      moderator_range <- range(jn_grid$moderatorValue, finite = TRUE)
      jn_bounds <- raw_bounds[raw_bounds >= moderator_range[[1]] & raw_bounds <= moderator_range[[2]]]
      significance_pattern <- if (all(jn_grid$lowerCI > 0 | jn_grid$upperCI < 0, na.rm = TRUE)) {
        "always-significant"
      } else if (!any(jn_grid$lowerCI > 0 | jn_grid$upperCI < 0, na.rm = TRUE)) {
        "never-significant"
      } else {
        "crosses-johnson-neyman-threshold"
      }
      jn_data <- list(
        grid = jn_grid,
        bounds = unname(jn_bounds),
        moderator = w_name,
        significancePattern = significance_pattern
      )
      jn_source <- "lm-fallback"
    }
  }

  if (is.null(visualization_data) || !nrow(visualization_data)) {
    return(list(
      artifacts = list(),
      plots = NULL,
      warnings = sprintf(
        "Skipped moderation decomposition/JN plots because PROCESS-native plot data were incomplete and the fallback path could not fully reconstruct them for model %s.",
        model
      )
    ))
  }

  if (is.null(visualization_data$moderatorLabel)) {
    unique_levels <- sort(unique(visualization_data$moderator))
    if (length(unique_levels) <= 3) {
      labels <- setNames(
        c("Low", "Mean", "High")[seq_along(unique_levels)],
        as.character(unique_levels)
      )
      visualization_data$moderatorLabel <- unname(labels[as.character(visualization_data$moderator)])
    } else {
      visualization_data$moderatorLabel <- sprintf("%s=%.3f", w_name, visualization_data$moderator)
    }
  }

  visualization_data <- visualization_data[order(visualization_data$moderatorLabel, visualization_data$x), , drop = FALSE]

  decomposition_path <- derive_suffixed_artifact_path(output_path, "-moderation-decomposition", ".png")
  jn_path <- derive_suffixed_artifact_path(output_path, "-johnson-neyman", ".png")
  plot_metadata_path <- derive_suffixed_artifact_path(output_path, "-plots", ".json")
  palette <- c("#3B82F6", "#8B5CF6", "#F59E0B")

  grDevices::png(decomposition_path, width = 1400, height = 1000, res = 160)
  graphics::par(mar = c(5, 5, 4, 2) + 0.1)
  plot(
    range(visualization_data$x, finite = TRUE),
    range(visualization_data$y, finite = TRUE),
    type = "n",
    xlab = x_name,
    ylab = sprintf("Predicted %s", y_name),
    main = sprintf("Moderation decomposition: %s × %s", x_name, w_name)
  )
  moderator_levels <- unique(visualization_data[, c("moderator", "moderatorLabel"), drop = FALSE])
  for (index in seq_len(nrow(moderator_levels))) {
    block <- visualization_data[visualization_data$moderatorLabel == moderator_levels$moderatorLabel[[index]], , drop = FALSE]
    graphics::lines(block$x, block$y, col = palette[[(index - 1) %% length(palette) + 1]], lwd = 3)
  }
  graphics::legend(
    "topleft",
    legend = sprintf("%s = %s", moderator_levels$moderatorLabel, format(round(moderator_levels$moderator, 3), nsmall = 3)),
    col = vapply(seq_len(nrow(moderator_levels)), function(index) palette[[(index - 1) %% length(palette) + 1]], character(1)),
    lwd = 3,
    bty = "n",
    cex = 0.9
  )
  grDevices::dev.off()

  plot_entries <- list(
    moderationDecomposition = list(
      status = "created",
      source = decomposition_source,
      method = if (identical(decomposition_source, "process-report")) "process-visualization-data" else "lm-simple-slopes",
      x = x_name,
      y = y_name,
      moderator = w_name,
      moderatorLevels = unname(lapply(seq_len(nrow(moderator_levels)), function(index) {
        list(
          label = moderator_levels$moderatorLabel[[index]],
          value = moderator_levels$moderator[[index]]
        )
      })),
      artifact = artifact_entry(decomposition_path, "image/png")
    )
  )

  warnings <- character()
  artifacts <- list(
    moderationDecompositionPlot = artifact_entry(decomposition_path, "image/png")
  )

  if (!is.null(jn_data) && !is.null(jn_data$grid) && nrow(jn_data$grid)) {
    jn_grid <- jn_data$grid
    jn_bounds <- jn_data$bounds
    significance_pattern <- jn_data$significancePattern
    grDevices::png(jn_path, width = 1400, height = 1000, res = 160)
    graphics::par(mar = c(5, 5, 4, 2) + 0.1)
    plot(
      jn_grid$moderatorValue,
      jn_grid$effect,
      type = "n",
      ylim = range(c(jn_grid$lowerCI, jn_grid$upperCI, 0), finite = TRUE),
      xlab = w_name,
      ylab = sprintf("Conditional effect of %s on %s", x_name, y_name),
      main = sprintf("Johnson-Neyman plot: %s moderated by %s", x_name, w_name)
    )
    if ("lowerCI" %in% names(jn_grid) && "upperCI" %in% names(jn_grid) && all(is.finite(jn_grid$lowerCI)) && all(is.finite(jn_grid$upperCI))) {
      graphics::polygon(
        c(jn_grid$moderatorValue, rev(jn_grid$moderatorValue)),
        c(jn_grid$lowerCI, rev(jn_grid$upperCI)),
        border = NA,
        col = grDevices::adjustcolor("#3B82F6", alpha.f = 0.2)
      )
    }
    graphics::lines(jn_grid$moderatorValue, jn_grid$effect, col = "#1D4ED8", lwd = 3)
    graphics::abline(h = 0, col = "#6B7280", lty = 2)
    if (length(jn_bounds)) {
      for (bound in jn_bounds) {
        graphics::abline(v = bound, col = "#DC2626", lty = 3, lwd = 2)
      }
    }
    grDevices::dev.off()
    artifacts$johnsonNeymanPlot <- artifact_entry(jn_path, "image/png")
    plot_entries$johnsonNeyman <- list(
      status = "created",
      source = jn_source,
      method = if (identical(jn_source, "process-report")) "process-johnson-neyman" else "lm-conditional-slope",
      x = x_name,
      y = y_name,
      moderator = w_name,
      confidence = conf_level,
      significancePattern = significance_pattern,
      bounds = unname(as.list(jn_bounds)),
      artifact = artifact_entry(jn_path, "image/png")
    )
  } else {
    warnings <- c(
      warnings,
      sprintf("Skipped Johnson-Neyman plot for model %s because neither PROCESS-native nor fallback JN data were available.", model)
    )
  }

  plot_metadata <- build_plot_metadata_payload(plot_entries)
  write_json_artifact(plot_metadata_path, plot_metadata)
  artifacts$plotMetadataJson <- artifact_entry(plot_metadata_path, "json")

  list(
    artifacts = artifacts,
    plots = plot_metadata$plots,
    warnings = warnings
  )
}

build_response_payload <- function(
  plugin_id,
  tool_name,
  dataset_path,
  dataset,
  model,
  y_value,
  x_value,
  m_value,
  w_value,
  z_value,
  cov_value,
  boot_value,
  conf_value,
  center_value,
  seed_value,
  process_script_path,
  output_path,
  report_json_path,
  report_parse,
  plot_artifacts,
  plot_summaries,
  warnings
) {
  artifacts <- c(list(
    report = artifact_entry(output_path, "text")
  ), plot_artifacts)
  if (!is.null(report_json_path)) {
    artifacts$reportJson <- artifact_entry(report_json_path, "json")
  }

  payload <- list(
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
    artifacts = artifacts,
    reportParse = report_parse,
    warnings = unname(warnings)
  )

  if (!is.null(plot_summaries) && length(plot_summaries)) {
    payload$plots <- plot_summaries
  }

  payload
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
report_json_path <- derive_json_sidecar_path(output_path)

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

plot_artifacts <- list()
plot_summaries <- NULL
if (length(w_value) == 1) {
  plot_bundle <- tryCatch(
    generate_moderation_plot_bundle(
    dataset = dataset,
    report_parse = report_parse,
    model = model,
    x_name = x_value,
    y_name = y_value,
    w_name = w_value[[1]],
    covariates = cov_value,
    conf_level = conf_value,
    output_path = output_path
  ),
    error = function(err) {
      list(
        artifacts = list(),
        plots = NULL,
        warnings = sprintf("Skipped moderation/JN plots because plot generation failed: %s", err$message)
      )
    }
  )
  plot_artifacts <- plot_bundle$artifacts
  plot_summaries <- plot_bundle$plots
  warnings <- c(warnings, plot_bundle$warnings)
}

response_payload <- build_response_payload(
  plugin_id = plugin_id,
  tool_name = tool_name,
  dataset_path = dataset_path,
  dataset = dataset,
  model = model,
  y_value = y_value,
  x_value = x_value,
  m_value = m_value,
  w_value = w_value,
  z_value = z_value,
  cov_value = cov_value,
  boot_value = boot_value,
  conf_value = conf_value,
  center_value = center_value,
  seed_value = seed_value,
  process_script_path = process_script_path,
  output_path = output_path,
  report_json_path = report_json_path,
  report_parse = report_parse,
  plot_artifacts = plot_artifacts,
  plot_summaries = plot_summaries,
  warnings = warnings
)

json_sidecar_error <- tryCatch(
  {
    writeLines(json_text(response_payload), report_json_path, useBytes = TRUE)
    NULL
  },
  error = function(err) err$message
)

if (!is.null(json_sidecar_error)) {
  warnings <- c(
    warnings,
    sprintf("Failed to write PROCESS JSON sidecar artifact: %s", json_sidecar_error)
  )
  response_payload <- build_response_payload(
    plugin_id = plugin_id,
    tool_name = tool_name,
    dataset_path = dataset_path,
    dataset = dataset,
    model = model,
    y_value = y_value,
    x_value = x_value,
    m_value = m_value,
    w_value = w_value,
    z_value = z_value,
    cov_value = cov_value,
    boot_value = boot_value,
    conf_value = conf_value,
    center_value = center_value,
    seed_value = seed_value,
    process_script_path = process_script_path,
    output_path = output_path,
    report_json_path = NULL,
    report_parse = report_parse,
    plot_artifacts = plot_artifacts,
    plot_summaries = plot_summaries,
    warnings = warnings
  )
}

emit_json(response_payload)
