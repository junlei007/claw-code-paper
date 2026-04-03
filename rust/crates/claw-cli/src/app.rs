use std::io::{self, Write};
use std::path::PathBuf;

use crate::args::{OutputFormat, PermissionMode};
use crate::input::{LineEditor, ReadOutcome};
use crate::render::{Spinner, TerminalRenderer};
use runtime::{ConversationClient, ConversationMessage, RuntimeError, StreamEvent, UsageSummary};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionConfig {
    pub model: String,
    pub permission_mode: PermissionMode,
    pub config: Option<PathBuf>,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionState {
    pub turns: usize,
    pub compacted_messages: usize,
    pub last_model: String,
    pub last_usage: UsageSummary,
}

impl SessionState {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            turns: 0,
            compacted_messages: 0,
            last_model: model.into(),
            last_usage: UsageSummary::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandResult {
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Help,
    Status,
    Compact,
    Unknown(String),
}

impl SlashCommand {
    #[must_use]
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let command = trimmed
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default();
        Some(match command {
            "help" => Self::Help,
            "status" => Self::Status,
            "compact" => Self::Compact,
            other => Self::Unknown(other.to_string()),
        })
    }
}

struct SlashCommandHandler {
    command: SlashCommand,
    summary: &'static str,
}

const SLASH_COMMAND_HANDLERS: &[SlashCommandHandler] = &[
    SlashCommandHandler {
        command: SlashCommand::Help,
        summary: "Show command help",
    },
    SlashCommandHandler {
        command: SlashCommand::Status,
        summary: "Show current session status",
    },
    SlashCommandHandler {
        command: SlashCommand::Compact,
        summary: "Compact local session history",
    },
];

pub struct CliApp {
    config: SessionConfig,
    renderer: TerminalRenderer,
    state: SessionState,
    conversation_client: ConversationClient,
    conversation_history: Vec<ConversationMessage>,
}

impl CliApp {
    pub fn new(config: SessionConfig) -> Result<Self, RuntimeError> {
        let state = SessionState::new(config.model.clone());
        let conversation_client = ConversationClient::from_env(config.model.clone())?;
        Ok(Self {
            config,
            renderer: TerminalRenderer::new(),
            state,
            conversation_client,
            conversation_history: Vec::new(),
        })
    }

    pub fn run_repl(&mut self) -> io::Result<()> {
        let mut editor = LineEditor::new("› ", Vec::new());
        println!("Claw Code interactive mode");
        println!("Type /help for commands. Shift+Enter or Ctrl+J inserts a newline.");

        loop {
            match editor.read_line()? {
                ReadOutcome::Submit(input) => {
                    if input.trim().is_empty() {
                        continue;
                    }
                    self.handle_submission(&input, &mut io::stdout())?;
                }
                ReadOutcome::Cancel => continue,
                ReadOutcome::Exit => break,
            }
        }

        Ok(())
    }

    pub fn run_prompt(&mut self, prompt: &str, out: &mut impl Write) -> io::Result<()> {
        self.render_response(prompt, out)
    }

    pub fn handle_submission(
        &mut self,
        input: &str,
        out: &mut impl Write,
    ) -> io::Result<CommandResult> {
        if let Some(command) = SlashCommand::parse(input) {
            return self.dispatch_slash_command(command, out);
        }

        self.state.turns += 1;
        self.render_response(input, out)?;
        Ok(CommandResult::Continue)
    }

    fn dispatch_slash_command(
        &mut self,
        command: SlashCommand,
        out: &mut impl Write,
    ) -> io::Result<CommandResult> {
        match command {
            SlashCommand::Help => Self::handle_help(out),
            SlashCommand::Status => self.handle_status(out),
            SlashCommand::Compact => self.handle_compact(out),
            SlashCommand::Unknown(name) => {
                writeln!(out, "Unknown slash command: /{name}")?;
                Ok(CommandResult::Continue)
            }
            _ => {
                writeln!(out, "Slash command unavailable in this mode")?;
                Ok(CommandResult::Continue)
            }
        }
    }

    fn handle_help(out: &mut impl Write) -> io::Result<CommandResult> {
        writeln!(out, "Available commands:")?;
        for handler in SLASH_COMMAND_HANDLERS {
            let name = match handler.command {
                SlashCommand::Help => "/help",
                SlashCommand::Status => "/status",
                SlashCommand::Compact => "/compact",
                _ => continue,
            };
            writeln!(out, "  {name:<9} {}", handler.summary)?;
        }
        Ok(CommandResult::Continue)
    }

    fn handle_status(&mut self, out: &mut impl Write) -> io::Result<CommandResult> {
        writeln!(
            out,
            "status: turns={} model={} permission-mode={:?} output-format={:?} last-usage={} in/{} out config={}",
            self.state.turns,
            self.state.last_model,
            self.config.permission_mode,
            self.config.output_format,
            self.state.last_usage.input_tokens,
            self.state.last_usage.output_tokens,
            self.config
                .config
                .as_ref()
                .map_or_else(|| String::from("<none>"), |path| path.display().to_string())
        )?;
        Ok(CommandResult::Continue)
    }

    fn handle_compact(&mut self, out: &mut impl Write) -> io::Result<CommandResult> {
        self.state.compacted_messages += self.state.turns;
        self.state.turns = 0;
        self.conversation_history.clear();
        writeln!(
            out,
            "Compacted session history into a local summary ({} messages total compacted).",
            self.state.compacted_messages
        )?;
        Ok(CommandResult::Continue)
    }

    fn handle_stream_event(
        renderer: &TerminalRenderer,
        event: StreamEvent,
        stream_spinner: &mut Spinner,
        tool_spinner: &mut Spinner,
        saw_text: &mut bool,
        turn_usage: &mut UsageSummary,
        out: &mut impl Write,
    ) {
        match event {
            StreamEvent::TextDelta(delta) => {
                if !*saw_text {
                    let _ =
                        stream_spinner.finish("Streaming response", renderer.color_theme(), out);
                    *saw_text = true;
                }
                let _ = write!(out, "{delta}");
                let _ = out.flush();
            }
            StreamEvent::ToolCallStart { name, input } => {
                if *saw_text {
                    let _ = writeln!(out);
                }
                let _ = tool_spinner.tick(
                    &format!("Running tool `{name}` · {}", summarize_tool_input(&name, &input)),
                    renderer.color_theme(),
                    out,
                );
            }
            StreamEvent::ToolCallResult {
                name,
                output,
                is_error,
            } => {
                let label = if is_error {
                    format!("Tool `{name}` failed")
                } else {
                    format!("Tool `{name}` completed")
                };
                let _ = tool_spinner.finish(&label, renderer.color_theme(), out);
                let rendered_output =
                    format!("### Tool `{name}`\n\n{}\n", format_tool_result_preview(&name, &output, is_error));
                let _ = renderer.stream_markdown(&rendered_output, out);
            }
            StreamEvent::Usage(usage) => {
                *turn_usage = usage;
            }
        }
    }

    fn write_turn_output(
        &self,
        summary: &runtime::TurnSummary,
        out: &mut impl Write,
    ) -> io::Result<()> {
        match self.config.output_format {
            OutputFormat::Text => {
                writeln!(
                    out,
                    "\nToken usage: {} input / {} output",
                    self.state.last_usage.input_tokens, self.state.last_usage.output_tokens
                )?;
            }
            OutputFormat::Json => {
                writeln!(
                    out,
                    "{}",
                    serde_json::json!({
                        "message": summary.assistant_text,
                        "usage": {
                            "input_tokens": self.state.last_usage.input_tokens,
                            "output_tokens": self.state.last_usage.output_tokens,
                        }
                    })
                )?;
            }
            OutputFormat::Ndjson => {
                writeln!(
                    out,
                    "{}",
                    serde_json::json!({
                        "type": "message",
                        "text": summary.assistant_text,
                        "usage": {
                            "input_tokens": self.state.last_usage.input_tokens,
                            "output_tokens": self.state.last_usage.output_tokens,
                        }
                    })
                )?;
            }
        }
        Ok(())
    }

    fn render_response(&mut self, input: &str, out: &mut impl Write) -> io::Result<()> {
        let mut stream_spinner = Spinner::new();
        stream_spinner.tick(
            "Opening conversation stream",
            self.renderer.color_theme(),
            out,
        )?;

        let mut turn_usage = UsageSummary::default();
        let mut tool_spinner = Spinner::new();
        let mut saw_text = false;
        let renderer = &self.renderer;

        let result =
            self.conversation_client
                .run_turn(&mut self.conversation_history, input, |event| {
                    Self::handle_stream_event(
                        renderer,
                        event,
                        &mut stream_spinner,
                        &mut tool_spinner,
                        &mut saw_text,
                        &mut turn_usage,
                        out,
                    );
                });

        let summary = match result {
            Ok(summary) => summary,
            Err(error) => {
                stream_spinner.fail(
                    "Streaming response failed",
                    self.renderer.color_theme(),
                    out,
                )?;
                return Err(io::Error::other(error));
            }
        };
        self.state.last_usage = summary.usage.clone();
        if saw_text {
            writeln!(out)?;
        } else {
            stream_spinner.finish("Streaming response", self.renderer.color_theme(), out)?;
        }

        self.write_turn_output(&summary, out)?;
        let _ = turn_usage;
        Ok(())
    }
}

fn summarize_tool_input(name: &str, input: &str) -> String {
    let parsed = serde_json::from_str::<Value>(input).unwrap_or(Value::String(input.to_string()));
    match name {
        "read_file" | "Read" => format!("reading {}", extract_tool_path(&parsed)),
        "write_file" | "Write" => format!("writing {}", extract_tool_path(&parsed)),
        "edit_file" | "Edit" => format!("editing {}", extract_tool_path(&parsed)),
        "glob_search" | "Glob" => format!(
            "glob `{}` in {}",
            parsed
                .get("pattern")
                .and_then(Value::as_str)
                .unwrap_or("?"),
            parsed.get("path").and_then(Value::as_str).unwrap_or(".")
        ),
        "grep_search" | "Grep" => format!(
            "grep `{}` in {}",
            parsed
                .get("pattern")
                .and_then(Value::as_str)
                .unwrap_or("?"),
            parsed.get("path").and_then(Value::as_str).unwrap_or(".")
        ),
        "bash" | "Bash" => parsed
            .get("command")
            .and_then(Value::as_str)
            .map_or_else(|| "running shell command".to_string(), truncate_for_summary),
        "TodoWrite" => parsed
            .get("todos")
            .and_then(Value::as_array)
            .map_or_else(|| "updating todos".to_string(), |todos| {
                format!("updating {} todo item(s)", todos.len())
            }),
        _ => summarize_json_value(&parsed),
    }
}

fn format_tool_result_preview(name: &str, output: &str, is_error: bool) -> String {
    if is_error {
        return format!("```text\n{}\n```", truncate_block(output, 16, 1200));
    }

    let parsed = serde_json::from_str::<Value>(output).unwrap_or(Value::String(output.to_string()));
    match name {
        "glob_search" | "Glob" => format_glob_result(&parsed),
        "read_file" | "Read" => format_read_result(&parsed),
        "write_file" | "Write" => format_write_result(&parsed),
        "edit_file" | "Edit" => format_edit_result(&parsed),
        "TodoWrite" => format_todo_write_result(&parsed),
        _ => match parsed {
            Value::Object(_) | Value::Array(_) => summarize_json_value(&parsed),
            Value::String(text) => {
                if text.contains('\n') {
                    format!("```text\n{}\n```", truncate_block(&text, 20, 1600))
                } else {
                    truncate_for_summary(&text)
                }
            }
            Value::Null => "No output.".to_string(),
            other => other.to_string(),
        },
    }
}

fn extract_tool_path(parsed: &Value) -> String {
    parsed
        .get("file_path")
        .or_else(|| parsed.get("filePath"))
        .or_else(|| parsed.get("path"))
        .and_then(Value::as_str)
        .unwrap_or("?")
        .to_string()
}

fn format_glob_result(parsed: &Value) -> String {
    let num_files = parsed.get("numFiles").and_then(Value::as_u64).unwrap_or(0);
    let mut lines = vec![format!("Matched {num_files} file(s).")];
    if let Some(files) = parsed.get("filenames").and_then(Value::as_array) {
        let preview = files
            .iter()
            .filter_map(Value::as_str)
            .take(5)
            .map(|path| format!("- `{path}`"))
            .collect::<Vec<_>>();
        if !preview.is_empty() {
            lines.push(preview.join("\n"));
        }
    }
    lines.join("\n\n")
}

fn format_read_result(parsed: &Value) -> String {
    let file = parsed.get("file").unwrap_or(parsed);
    let path = extract_tool_path(file);
    let start_line = file.get("startLine").and_then(Value::as_u64).unwrap_or(1);
    let num_lines = file.get("numLines").and_then(Value::as_u64).unwrap_or(0);
    let total_lines = file
        .get("totalLines")
        .and_then(Value::as_u64)
        .unwrap_or(num_lines);
    let end_line = start_line.saturating_add(num_lines.saturating_sub(1));
    let content = file.get("content").and_then(Value::as_str).unwrap_or_default();
    format!(
        "Read `{path}` (lines {}-{} of {}).\n\n```text\n{}\n```",
        start_line,
        end_line.max(start_line),
        total_lines,
        truncate_block(content, 32, 1800)
    )
}

fn format_write_result(parsed: &Value) -> String {
    let path = extract_tool_path(parsed);
    let kind = parsed.get("type").and_then(Value::as_str).unwrap_or("write");
    let lines = parsed
        .get("content")
        .and_then(Value::as_str)
        .map_or(0, |content| content.lines().count());
    match kind {
        "create" => format!("Created `{path}` ({lines} lines)."),
        _ => format!("Updated `{path}` ({lines} lines)."),
    }
}

fn format_edit_result(parsed: &Value) -> String {
    let path = extract_tool_path(parsed);
    let replace_all = parsed
        .get("replaceAll")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if replace_all {
        format!("Edited `{path}` (replace all).")
    } else {
        format!("Edited `{path}`.")
    }
}

fn format_todo_write_result(parsed: &Value) -> String {
    let old_len = parsed
        .get("oldTodos")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let new_len = parsed
        .get("newTodos")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let verification_nudge = parsed
        .get("verificationNudgeNeeded")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if verification_nudge {
        format!("Updated todos ({old_len} → {new_len}). Verification follow-up is recommended.")
    } else {
        format!("Updated todos ({old_len} → {new_len}).")
    }
}

fn summarize_json_value(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let preferred = [
                "type",
                "status",
                "result",
                "message",
                "filePath",
                "path",
                "numFiles",
                "numMatches",
                "durationMs",
                "truncated",
            ];
            let mut parts = preferred
                .iter()
                .filter_map(|key| {
                    map.get(*key).map(|val| match val {
                        Value::String(text) => format!("{key}={}", truncate_for_summary(text)),
                        Value::Number(num) => format!("{key}={num}"),
                        Value::Bool(flag) => format!("{key}={flag}"),
                        Value::Null => format!("{key}=null"),
                        Value::Array(items) => format!("{key}=[{} item(s)]", items.len()),
                        Value::Object(_) => format!("{key}={{…}}"),
                    })
                })
                .collect::<Vec<_>>();
            if parts.is_empty() {
                let keys = map.keys().take(6).cloned().collect::<Vec<_>>().join(", ");
                if keys.is_empty() {
                    "structured output".to_string()
                } else {
                    format!("structured output ({keys})")
                }
            } else {
                parts.truncate(6);
                parts.join(" · ")
            }
        }
        Value::Array(items) => format!("structured output [{} item(s)]", items.len()),
        Value::String(text) => truncate_for_summary(text),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn truncate_for_summary(text: &str) -> String {
    let limit = 100usize;
    let mut chars = text.chars();
    let short = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{short}…")
    } else {
        short
    }
}

fn truncate_block(text: &str, max_lines: usize, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut used_chars = 0usize;
    let mut lines = Vec::new();
    let mut truncated = false;
    for (index, line) in trimmed.lines().enumerate() {
        if index >= max_lines || used_chars >= max_chars {
            truncated = true;
            break;
        }
        let remaining = max_chars.saturating_sub(used_chars);
        let line_short = if line.chars().count() > remaining {
            truncated = true;
            line.chars().take(remaining).collect::<String>()
        } else {
            line.to_string()
        };
        used_chars += line_short.chars().count() + 1;
        lines.push(line_short);
    }
    let mut result = lines.join("\n");
    if truncated {
        result.push_str("\n… output truncated for display.");
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::args::{OutputFormat, PermissionMode};

    use super::{
        format_tool_result_preview, summarize_tool_input, CommandResult, SessionConfig,
        SlashCommand,
    };

    #[test]
    fn parses_required_slash_commands() {
        assert_eq!(SlashCommand::parse("/help"), Some(SlashCommand::Help));
        assert_eq!(SlashCommand::parse(" /status "), Some(SlashCommand::Status));
        assert_eq!(
            SlashCommand::parse("/compact now"),
            Some(SlashCommand::Compact)
        );
    }

    #[test]
    fn help_output_lists_commands() {
        let mut out = Vec::new();
        let result = super::CliApp::handle_help(&mut out).expect("help succeeds");
        assert_eq!(result, CommandResult::Continue);
        let output = String::from_utf8_lossy(&out);
        assert!(output.contains("/help"));
        assert!(output.contains("/status"));
        assert!(output.contains("/compact"));
    }

    #[test]
    fn session_state_tracks_config_values() {
        let config = SessionConfig {
            model: "sonnet".into(),
            permission_mode: PermissionMode::DangerFullAccess,
            config: Some(PathBuf::from("settings.toml")),
            output_format: OutputFormat::Text,
        };

        assert_eq!(config.model, "sonnet");
        assert_eq!(config.permission_mode, PermissionMode::DangerFullAccess);
        assert_eq!(config.config, Some(PathBuf::from("settings.toml")));
    }

    #[test]
    fn tool_start_summary_avoids_dumping_raw_json() {
        let summary = summarize_tool_input(
            "TodoWrite",
            r#"{"todos":[{"content":"a","status":"completed"},{"content":"b","status":"in_progress"}]}"#,
        );
        assert_eq!(summary, "updating 2 todo item(s)");
    }

    #[test]
    fn tool_result_preview_summarizes_todo_write_json() {
        let preview = format_tool_result_preview(
            "TodoWrite",
            r#"{"oldTodos":[{"content":"a"}],"newTodos":[{"content":"a"},{"content":"b"}],"verificationNudgeNeeded":true}"#,
            false,
        );
        assert!(preview.contains("Updated todos (1 → 2)"));
        assert!(!preview.contains("\"oldTodos\""));
    }
}
