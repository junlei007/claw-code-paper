mod init;
mod input;
mod render;

use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use api::{
    resolve_model_alias, resolve_startup_auth_source, AuthSource, ClawApiClient, ContentBlockDelta,
    InputContentBlock, InputMessage, MessageRequest, MessageResponse, OpenAiCompatConfig,
    OutputContentBlock, ProviderClient, StreamEvent as ApiStreamEvent, ToolChoice, ToolDefinition,
    ToolResultContentBlock,
};

use commands::{
    handle_agents_slash_command, handle_plugins_slash_command, handle_skills_slash_command,
    render_slash_command_help, resume_supported_slash_commands, slash_command_specs, SlashCommand,
};
use compat_harness::{extract_manifest, UpstreamPaths};
use init::{initialize_repo, InitOptions, InitResearchProfile};
use plugins::{PluginManager, PluginManagerConfig};
use render::{MarkdownStreamState, Spinner, TerminalRenderer};
use runtime::{
    clear_oauth_credentials, generate_pkce_pair, generate_state, load_system_prompt,
    parse_oauth_callback_request_target, save_oauth_credentials, ApiClient, ApiRequest,
    AssistantEvent, CompactionConfig, ConfigLoader, ConfigSource, ContentBlock,
    ConversationMessage, ConversationRuntime, MessageRole, OAuthAuthorizationRequest, OAuthConfig,
    OAuthTokenExchangeRequest, PermissionMode, PermissionPolicy, ProjectContext, RuntimeConfig,
    RuntimeError, RuntimeFeatureConfig, RuntimeProviderProfile, RuntimeProviderTransport, Session,
    TokenUsage, ToolError, ToolExecutor, UsageTracker,
};
use serde_json::json;
use tools::GlobalToolRegistry;

const DEFAULT_MODEL: &str = "claude-opus-4-6";
fn max_tokens_for_model(model: &str) -> u32 {
    if model.contains("deepseek") {
        8_192
    } else if model.contains("opus") {
        32_000
    } else {
        64_000
    }
}
const DEFAULT_DATE: &str = "2026-03-31";
const DEFAULT_OAUTH_CALLBACK_PORT: u16 = 4545;
const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_TARGET: Option<&str> = option_env!("TARGET");
const GIT_SHA: Option<&str> = option_env!("GIT_SHA");
const INTERNAL_PROGRESS_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);

type AllowedToolSet = BTreeSet<String>;

fn main() {
    if let Err(error) = run() {
        let cli_name = current_cli_name();
        eprintln!(
            "error: {error}

Run `{cli_name} --help` for usage."
        );
        std::process::exit(1);
    }
}

fn current_cli_name() -> String {
    if let Ok(value) = env::var("CLAW_DISPLAY_NAME") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    env::args()
        .next()
        .and_then(|value| {
            Path::new(&value)
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToOwned::to_owned)
        })
        .filter(|value| matches!(value.as_str(), "claw" | "paperowl"))
        .unwrap_or_else(|| "claw".to_string())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    match parse_args(&args)? {
        CliAction::DumpManifests => dump_manifests(),
        CliAction::BootstrapPlan => print_bootstrap_plan(),
        CliAction::Agents { args } => LiveCli::print_agents(args.as_deref())?,
        CliAction::Skills { args } => LiveCli::print_skills(args.as_deref())?,
        CliAction::PrintSystemPrompt { cwd, date } => print_system_prompt(cwd, date),
        CliAction::Version => print_version(),
        CliAction::ResumeSession {
            session_path,
            commands,
        } => resume_session(&session_path, &commands),
        CliAction::Prompt {
            prompt,
            model,
            provider,
            output_format,
            allowed_tools,
            permission_mode,
        } => LiveCli::new(model, provider, true, allowed_tools, permission_mode)?
            .run_turn_with_output(&prompt, output_format)?,
        CliAction::Login => run_login()?,
        CliAction::Logout => run_logout()?,
        CliAction::Init { options } => run_init(&options)?,
        CliAction::Plugins { action, target } => {
            run_plugins_command(action.as_deref(), target.as_deref())?
        }
        CliAction::ProjectSkill {
            command,
            output_format,
        } => run_project_skill_command(&command, output_format)?,
        CliAction::Repl {
            model,
            provider,
            allowed_tools,
            permission_mode,
        } => run_repl(model, provider, allowed_tools, permission_mode)?,
        CliAction::Help => print_help(),
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliAction {
    DumpManifests,
    BootstrapPlan,
    Agents {
        args: Option<String>,
    },
    Skills {
        args: Option<String>,
    },
    PrintSystemPrompt {
        cwd: PathBuf,
        date: String,
    },
    Version,
    ResumeSession {
        session_path: PathBuf,
        commands: Vec<String>,
    },
    Prompt {
        prompt: String,
        model: Option<String>,
        provider: Option<String>,
        output_format: CliOutputFormat,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
    },
    Login,
    Logout,
    Init {
        options: InitOptions,
    },
    Plugins {
        action: Option<String>,
        target: Option<String>,
    },
    ProjectSkill {
        command: ProjectSkillCommand,
        output_format: CliOutputFormat,
    },
    Repl {
        model: Option<String>,
        provider: Option<String>,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
    },
    // prompt-mode formatting is only supported for non-interactive runs
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliOutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProjectSkillCommand {
    Init {
        slug: String,
        title: String,
        description: String,
        domain: String,
        use_when: String,
        sources: Vec<String>,
        input_expectations: Vec<String>,
        workflow_steps: Vec<String>,
        outputs: Vec<String>,
        limits: Vec<String>,
        failure_checks: Vec<String>,
        evaluation_examples: Vec<String>,
        generated_by: String,
        maturity: String,
        output_root: Option<PathBuf>,
        targets: Vec<String>,
        openclaw_root: Option<PathBuf>,
        claude_root: Option<PathBuf>,
    },
    Validate {
        path: PathBuf,
    },
    Promote {
        path: PathBuf,
        to: String,
        verification_status: Option<String>,
        held_out_validation_status: Option<String>,
    },
    Doctor {
        path: PathBuf,
    },
}

impl CliOutputFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            other => Err(format!(
                "unsupported value for --output-format: {other} (expected text or json)"
            )),
        }
    }
}

#[allow(clippy::too_many_lines)]
fn parse_args(args: &[String]) -> Result<CliAction, String> {
    let mut model = None;
    let mut provider = None;
    let mut output_format = CliOutputFormat::Text;
    let mut permission_mode = default_permission_mode();
    let mut wants_version = false;
    let mut allowed_tool_values = Vec::new();
    let mut rest = Vec::new();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--version" | "-V" => {
                wants_version = true;
                index += 1;
            }
            "--model" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --model".to_string())?;
                model = Some(resolve_model_alias(value));
                index += 2;
            }
            flag if flag.starts_with("--model=") => {
                model = Some(resolve_model_alias(&flag[8..]));
                index += 1;
            }
            "--provider" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --provider".to_string())?;
                provider = Some(value.clone());
                index += 2;
            }
            flag if flag.starts_with("--provider=") => {
                provider = Some(flag[11..].to_string());
                index += 1;
            }
            "--output-format" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --output-format".to_string())?;
                output_format = CliOutputFormat::parse(value)?;
                index += 2;
            }
            "--permission-mode" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --permission-mode".to_string())?;
                permission_mode = parse_permission_mode_arg(value)?;
                index += 2;
            }
            flag if flag.starts_with("--output-format=") => {
                output_format = CliOutputFormat::parse(&flag[16..])?;
                index += 1;
            }
            flag if flag.starts_with("--permission-mode=") => {
                permission_mode = parse_permission_mode_arg(&flag[18..])?;
                index += 1;
            }
            "--dangerously-skip-permissions" => {
                permission_mode = PermissionMode::DangerFullAccess;
                index += 1;
            }
            "-p" => {
                // Claw Code compat: -p "prompt" = one-shot prompt
                let prompt = args[index + 1..].join(" ");
                if prompt.trim().is_empty() {
                    return Err("-p requires a prompt string".to_string());
                }
                return Ok(CliAction::Prompt {
                    prompt,
                    model,
                    provider,
                    output_format,
                    allowed_tools: normalize_allowed_tools(&allowed_tool_values)?,
                    permission_mode,
                });
            }
            "--print" => {
                // Claw Code compat: --print makes output non-interactive
                output_format = CliOutputFormat::Text;
                index += 1;
            }
            "--allowedTools" | "--allowed-tools" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --allowedTools".to_string())?;
                allowed_tool_values.push(value.clone());
                index += 2;
            }
            flag if flag.starts_with("--allowedTools=") => {
                allowed_tool_values.push(flag[15..].to_string());
                index += 1;
            }
            flag if flag.starts_with("--allowed-tools=") => {
                allowed_tool_values.push(flag[16..].to_string());
                index += 1;
            }
            other => {
                rest.push(other.to_string());
                index += 1;
            }
        }
    }

    if wants_version {
        return Ok(CliAction::Version);
    }

    let allowed_tools = normalize_allowed_tools(&allowed_tool_values)?;

    if rest.is_empty() {
        return Ok(CliAction::Repl {
            model,
            provider,
            allowed_tools,
            permission_mode,
        });
    }
    if matches!(rest.first().map(String::as_str), Some("--help" | "-h")) {
        return Ok(CliAction::Help);
    }
    if rest.first().map(String::as_str) == Some("--resume") {
        return parse_resume_args(&rest[1..]);
    }

    match rest[0].as_str() {
        "dump-manifests" => Ok(CliAction::DumpManifests),
        "bootstrap-plan" => Ok(CliAction::BootstrapPlan),
        "agents" => Ok(CliAction::Agents {
            args: join_optional_args(&rest[1..]),
        }),
        "skills" => Ok(CliAction::Skills {
            args: join_optional_args(&rest[1..]),
        }),
        "system-prompt" => parse_system_prompt_args(&rest[1..]),
        "login" => Ok(CliAction::Login),
        "logout" => Ok(CliAction::Logout),
        "init" => parse_init_args(&rest[1..]),
        "plugin" | "plugins" => parse_plugins_args(&rest[1..]),
        "project-skill" => match parse_project_skill_args(&rest[1..])? {
            CliAction::ProjectSkill { command, .. } => Ok(CliAction::ProjectSkill {
                command,
                output_format,
            }),
            other => Ok(other),
        },
        "prompt" => {
            let prompt = rest[1..].join(" ");
            if prompt.trim().is_empty() {
                return Err("prompt subcommand requires a prompt string".to_string());
            }
            Ok(CliAction::Prompt {
                prompt,
                model,
                provider,
                output_format,
                allowed_tools,
                permission_mode,
            })
        }
        other if other.starts_with('/') => parse_direct_slash_cli_action(&rest),
        _other => Ok(CliAction::Prompt {
            prompt: rest.join(" "),
            model,
            provider,
            output_format,
            allowed_tools,
            permission_mode,
        }),
    }
}

fn join_optional_args(args: &[String]) -> Option<String> {
    let joined = args.join(" ");
    let trimmed = joined.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn parse_init_args(args: &[String]) -> Result<CliAction, String> {
    let mut options = InitOptions::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return Ok(CliAction::Help),
            "--research" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --research".to_string())?;
                options.research_profile =
                    Some(InitResearchProfile::parse(value).ok_or_else(|| {
                        format!("unsupported research profile '{value}'. Use: survey")
                    })?);
                index += 2;
            }
            flag if flag.starts_with("--research=") => {
                let value = &flag["--research=".len()..];
                options.research_profile =
                    Some(InitResearchProfile::parse(value).ok_or_else(|| {
                        format!("unsupported research profile '{value}'. Use: survey")
                    })?);
                index += 1;
            }
            other => {
                options.research_profile =
                    Some(InitResearchProfile::parse(other).ok_or_else(|| {
                        format!(
                            "unknown init flag or research profile '{other}'. Use --research survey or `claw init survey`."
                        )
                    })?);
                index += 1;
            }
        }
    }

    Ok(CliAction::Init { options })
}

fn parse_plugins_args(args: &[String]) -> Result<CliAction, String> {
    if matches!(args.first().map(String::as_str), Some("--help" | "-h")) {
        return Ok(CliAction::Help);
    }
    let action = args.first().cloned();
    let target = args.get(1).cloned();
    if args.len() > 2 {
        return Err(
            "plugins accepts at most two positional arguments: [action] [target]".to_string(),
        );
    }
    Ok(CliAction::Plugins { action, target })
}

fn parse_project_skill_args(args: &[String]) -> Result<CliAction, String> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Err(
            "project-skill requires a subcommand: init <slug>, validate <path>, or promote <path> --to <maturity>".to_string(),
        );
    };

    match subcommand {
        "--help" | "-h" => Ok(CliAction::Help),
        "init" => parse_project_skill_init_args(&args[1..]),
        "validate" => parse_project_skill_validate_args(&args[1..]),
        "promote" => parse_project_skill_promote_args(&args[1..]),
        "doctor" => parse_project_skill_doctor_args(&args[1..]),
        other => Err(format!(
            "unknown project-skill subcommand '{other}'. Use init <slug>, validate <path>, promote <path> --to <maturity>, or doctor <path>."
        )),
    }
}

fn parse_project_skill_init_args(args: &[String]) -> Result<CliAction, String> {
    let Some(slug) = args.first().filter(|value| !value.starts_with('-')) else {
        return Err("project-skill init requires a slug".to_string());
    };

    let mut title = None;
    let mut description = None;
    let mut domain = None;
    let mut use_when = None;
    let mut sources = Vec::new();
    let mut input_expectations = Vec::new();
    let mut workflow_steps = Vec::new();
    let mut outputs = Vec::new();
    let mut limits = Vec::new();
    let mut failure_checks = Vec::new();
    let mut evaluation_examples = Vec::new();
    let mut generated_by = "claw project-skill init".to_string();
    let mut maturity = "draft".to_string();
    let mut output_root = None;
    let mut targets = Vec::new();
    let mut openclaw_root = None;
    let mut claude_root = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return Ok(CliAction::Help),
            "--title" => {
                title = Some(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --title".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--description" => {
                description = Some(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --description".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--domain" => {
                domain = Some(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --domain".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--use-when" => {
                use_when = Some(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --use-when".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--source" => {
                sources.push(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --source".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--input-expectation" => {
                input_expectations.push(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --input-expectation".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--workflow-step" => {
                workflow_steps.push(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --workflow-step".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--output" => {
                outputs.push(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --output".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--limit" => {
                limits.push(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --limit".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--failure-check" => {
                failure_checks.push(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --failure-check".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--evaluation-example" => {
                evaluation_examples.push(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --evaluation-example".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--generated-by" => {
                generated_by = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --generated-by".to_string())?
                    .clone();
                index += 2;
            }
            "--maturity" => {
                maturity = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --maturity".to_string())?
                    .clone();
                index += 2;
            }
            "--output-root" => {
                output_root =
                    Some(PathBuf::from(args.get(index + 1).ok_or_else(|| {
                        "missing value for --output-root".to_string()
                    })?));
                index += 2;
            }
            "--target" => {
                targets.push(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --target".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--openclaw-root" => {
                openclaw_root =
                    Some(PathBuf::from(args.get(index + 1).ok_or_else(|| {
                        "missing value for --openclaw-root".to_string()
                    })?));
                index += 2;
            }
            "--claude-root" => {
                claude_root =
                    Some(PathBuf::from(args.get(index + 1).ok_or_else(|| {
                        "missing value for --claude-root".to_string()
                    })?));
                index += 2;
            }
            other => {
                return Err(format!("unknown project-skill init option: {other}"));
            }
        }
    }

    if sources.is_empty() {
        return Err("project-skill init requires at least one --source".to_string());
    }

    Ok(CliAction::ProjectSkill {
        command: ProjectSkillCommand::Init {
            slug: slug.clone(),
            title: title.ok_or_else(|| "project-skill init requires --title".to_string())?,
            description: description
                .ok_or_else(|| "project-skill init requires --description".to_string())?,
            domain: domain.ok_or_else(|| "project-skill init requires --domain".to_string())?,
            use_when: use_when
                .ok_or_else(|| "project-skill init requires --use-when".to_string())?,
            sources,
            input_expectations,
            workflow_steps,
            outputs,
            limits,
            failure_checks,
            evaluation_examples,
            generated_by,
            maturity,
            output_root,
            targets,
            openclaw_root,
            claude_root,
        },
        output_format: CliOutputFormat::Text,
    })
}

fn parse_project_skill_validate_args(args: &[String]) -> Result<CliAction, String> {
    let Some(path) = args.first() else {
        return Err("project-skill validate requires a path".to_string());
    };
    if args.len() > 1 {
        return Err("project-skill validate accepts exactly one path".to_string());
    }
    Ok(CliAction::ProjectSkill {
        command: ProjectSkillCommand::Validate {
            path: PathBuf::from(path),
        },
        output_format: CliOutputFormat::Text,
    })
}

fn parse_project_skill_promote_args(args: &[String]) -> Result<CliAction, String> {
    let Some(path) = args.first().filter(|value| !value.starts_with('-')) else {
        return Err("project-skill promote requires a path".to_string());
    };

    let mut to = None;
    let mut verification_status = None;
    let mut held_out_validation_status = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return Ok(CliAction::Help),
            "--to" => {
                to = Some(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --to".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--verification-status" => {
                verification_status = Some(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --verification-status".to_string())?
                        .clone(),
                );
                index += 2;
            }
            "--held-out-validation" => {
                held_out_validation_status = Some(
                    args.get(index + 1)
                        .ok_or_else(|| "missing value for --held-out-validation".to_string())?
                        .clone(),
                );
                index += 2;
            }
            other => {
                return Err(format!("unknown project-skill promote option: {other}"));
            }
        }
    }

    Ok(CliAction::ProjectSkill {
        command: ProjectSkillCommand::Promote {
            path: PathBuf::from(path),
            to: to.ok_or_else(|| "project-skill promote requires --to".to_string())?,
            verification_status,
            held_out_validation_status,
        },
        output_format: CliOutputFormat::Text,
    })
}

fn parse_project_skill_doctor_args(args: &[String]) -> Result<CliAction, String> {
    let Some(path) = args.first() else {
        return Err("project-skill doctor requires a path".to_string());
    };
    if args.len() > 1 {
        return Err("project-skill doctor accepts exactly one path".to_string());
    }
    Ok(CliAction::ProjectSkill {
        command: ProjectSkillCommand::Doctor {
            path: PathBuf::from(path),
        },
        output_format: CliOutputFormat::Text,
    })
}

fn parse_direct_slash_cli_action(rest: &[String]) -> Result<CliAction, String> {
    let raw = rest.join(" ");
    match SlashCommand::parse(&raw) {
        Some(SlashCommand::Help) => Ok(CliAction::Help),
        Some(SlashCommand::Agents { args }) => Ok(CliAction::Agents { args }),
        Some(SlashCommand::Skills { args }) => Ok(CliAction::Skills { args }),
        Some(command) => Err(format!(
            "unsupported direct slash command outside the REPL: {command_name}",
            command_name = match command {
                SlashCommand::Unknown(name) => format!("/{name}"),
                _ => rest[0].clone(),
            }
        )),
        None => Err(format!("unknown subcommand: {}", rest[0])),
    }
}

fn normalize_allowed_tools(values: &[String]) -> Result<Option<AllowedToolSet>, String> {
    current_tool_registry()?.normalize_allowed_tools(values)
}

fn current_tool_registry() -> Result<GlobalToolRegistry, String> {
    let cwd = env::current_dir().map_err(|error| error.to_string())?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load().map_err(|error| error.to_string())?;
    let plugin_manager = build_plugin_manager(&cwd, &loader, &runtime_config);
    let plugin_tools = plugin_manager
        .aggregated_tools()
        .map_err(|error| error.to_string())?;
    GlobalToolRegistry::with_plugin_tools(plugin_tools)
}

fn parse_permission_mode_arg(value: &str) -> Result<PermissionMode, String> {
    normalize_permission_mode(value)
        .ok_or_else(|| {
            format!(
                "unsupported permission mode '{value}'. Use read-only, workspace-write, or danger-full-access."
            )
        })
        .map(permission_mode_from_label)
}

fn permission_mode_from_label(mode: &str) -> PermissionMode {
    match mode {
        "read-only" => PermissionMode::ReadOnly,
        "workspace-write" => PermissionMode::WorkspaceWrite,
        "danger-full-access" => PermissionMode::DangerFullAccess,
        other => panic!("unsupported permission mode label: {other}"),
    }
}

fn default_permission_mode() -> PermissionMode {
    env::var("CLAW_PERMISSION_MODE")
        .ok()
        .as_deref()
        .and_then(normalize_permission_mode)
        .map_or(PermissionMode::DangerFullAccess, permission_mode_from_label)
}

fn filter_tool_specs(
    tool_registry: &GlobalToolRegistry,
    allowed_tools: Option<&AllowedToolSet>,
) -> Vec<ToolDefinition> {
    tool_registry.definitions(allowed_tools)
}

fn parse_system_prompt_args(args: &[String]) -> Result<CliAction, String> {
    let mut cwd = env::current_dir().map_err(|error| error.to_string())?;
    let mut date = DEFAULT_DATE.to_string();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--cwd" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --cwd".to_string())?;
                cwd = PathBuf::from(value);
                index += 2;
            }
            "--date" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "missing value for --date".to_string())?;
                date.clone_from(value);
                index += 2;
            }
            other => return Err(format!("unknown system-prompt option: {other}")),
        }
    }

    Ok(CliAction::PrintSystemPrompt { cwd, date })
}

fn parse_resume_args(args: &[String]) -> Result<CliAction, String> {
    let session_path = args
        .first()
        .ok_or_else(|| "missing session path for --resume".to_string())
        .map(PathBuf::from)?;
    let commands = args[1..].to_vec();
    if commands
        .iter()
        .any(|command| !command.trim_start().starts_with('/'))
    {
        return Err("--resume trailing arguments must be slash commands".to_string());
    }
    Ok(CliAction::ResumeSession {
        session_path,
        commands,
    })
}

fn dump_manifests() {
    let workspace_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let paths = UpstreamPaths::from_workspace_dir(&workspace_dir);
    match extract_manifest(&paths) {
        Ok(manifest) => {
            println!("commands: {}", manifest.commands.entries().len());
            println!("tools: {}", manifest.tools.entries().len());
            println!("bootstrap phases: {}", manifest.bootstrap.phases().len());
        }
        Err(error) => {
            eprintln!("failed to extract manifests: {error}");
            std::process::exit(1);
        }
    }
}

fn print_bootstrap_plan() {
    for phase in runtime::BootstrapPlan::claw_default().phases() {
        println!("- {phase:?}");
    }
}

fn default_oauth_config() -> OAuthConfig {
    OAuthConfig {
        client_id: String::from("9d1c250a-e61b-44d9-88ed-5944d1962f5e"),
        authorize_url: String::from("https://platform.claw.dev/oauth/authorize"),
        token_url: String::from("https://platform.claw.dev/v1/oauth/token"),
        callback_port: None,
        manual_redirect_url: None,
        scopes: vec![
            String::from("user:profile"),
            String::from("user:inference"),
            String::from("user:sessions:claw_code"),
        ],
    }
}

fn run_login() -> Result<(), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let config = ConfigLoader::default_for(&cwd).load()?;
    let default_oauth = default_oauth_config();
    let oauth = config.oauth().unwrap_or(&default_oauth);
    let callback_port = oauth.callback_port.unwrap_or(DEFAULT_OAUTH_CALLBACK_PORT);
    let redirect_uri = runtime::loopback_redirect_uri(callback_port);
    let pkce = generate_pkce_pair()?;
    let state = generate_state()?;
    let authorize_url =
        OAuthAuthorizationRequest::from_config(oauth, redirect_uri.clone(), state.clone(), &pkce)
            .build_url();

    println!("Starting Claw OAuth login...");
    println!("Listening for callback on {redirect_uri}");
    if let Err(error) = open_browser(&authorize_url) {
        eprintln!("warning: failed to open browser automatically: {error}");
        println!("Open this URL manually:\n{authorize_url}");
    }

    let callback = wait_for_oauth_callback(callback_port)?;
    if let Some(error) = callback.error {
        let description = callback
            .error_description
            .unwrap_or_else(|| "authorization failed".to_string());
        return Err(io::Error::other(format!("{error}: {description}")).into());
    }
    let code = callback.code.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "callback did not include code")
    })?;
    let returned_state = callback.state.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "callback did not include state")
    })?;
    if returned_state != state {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "oauth state mismatch").into());
    }

    let client = ClawApiClient::from_auth(AuthSource::None).with_base_url(api::read_base_url());
    let exchange_request =
        OAuthTokenExchangeRequest::from_config(oauth, code, state, pkce.verifier, redirect_uri);
    let runtime = tokio::runtime::Runtime::new()?;
    let token_set = runtime.block_on(client.exchange_oauth_code(oauth, &exchange_request))?;
    save_oauth_credentials(&runtime::OAuthTokenSet {
        access_token: token_set.access_token,
        refresh_token: token_set.refresh_token,
        expires_at: token_set.expires_at,
        scopes: token_set.scopes,
    })?;
    println!("Claw OAuth login complete.");
    Ok(())
}

fn run_logout() -> Result<(), Box<dyn std::error::Error>> {
    clear_oauth_credentials()?;
    println!("Claw OAuth credentials cleared.");
    Ok(())
}

fn open_browser(url: &str) -> io::Result<()> {
    let commands = if cfg!(target_os = "macos") {
        vec![("open", vec![url])]
    } else if cfg!(target_os = "windows") {
        vec![("cmd", vec!["/C", "start", "", url])]
    } else {
        vec![("xdg-open", vec![url])]
    };
    for (program, args) in commands {
        match Command::new(program).args(args).spawn() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no supported browser opener command found",
    ))
}

fn wait_for_oauth_callback(
    port: u16,
) -> Result<runtime::OAuthCallbackParams, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let (mut stream, _) = listener.accept()?;
    let mut buffer = [0_u8; 4096];
    let bytes_read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_line = request.lines().next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing callback request line")
    })?;
    let target = request_line.split_whitespace().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing callback request target",
        )
    })?;
    let callback = parse_oauth_callback_request_target(target)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let body = if callback.error.is_some() {
        "Claw OAuth login failed. You can close this window."
    } else {
        "Claw OAuth login succeeded. You can close this window."
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    Ok(callback)
}

fn print_system_prompt(cwd: PathBuf, date: String) {
    match load_system_prompt(cwd, date, env::consts::OS, "unknown") {
        Ok(sections) => println!("{}", sections.join("\n\n")),
        Err(error) => {
            eprintln!("failed to build system prompt: {error}");
            std::process::exit(1);
        }
    }
}

fn print_version() {
    println!("{}", render_version_report());
}

fn resume_session(session_path: &Path, commands: &[String]) {
    let session = match Session::load_from_path(session_path) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("failed to restore session: {error}");
            std::process::exit(1);
        }
    };

    if commands.is_empty() {
        println!(
            "Restored session from {} ({} messages).",
            session_path.display(),
            session.messages.len()
        );
        return;
    }

    let mut session = session;
    for raw_command in commands {
        let Some(command) = SlashCommand::parse(raw_command) else {
            eprintln!("unsupported resumed command: {raw_command}");
            std::process::exit(2);
        };
        match run_resume_command(session_path, &session, &command) {
            Ok(ResumeCommandOutcome {
                session: next_session,
                message,
            }) => {
                session = next_session;
                if let Some(message) = message {
                    println!("{message}");
                }
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ResumeCommandOutcome {
    session: Session,
    message: Option<String>,
}

#[derive(Debug, Clone)]
struct StatusContext {
    cwd: PathBuf,
    session_path: Option<PathBuf>,
    loaded_config_files: usize,
    discovered_config_files: usize,
    memory_file_count: usize,
    project_root: Option<PathBuf>,
    git_branch: Option<String>,
}

#[derive(Debug, Clone, Copy)]
struct StatusUsage {
    message_count: usize,
    turns: u32,
    latest: TokenUsage,
    cumulative: TokenUsage,
    estimated_tokens: usize,
}

fn format_model_report(model: &str, message_count: usize, turns: u32) -> String {
    format!(
        "Model
  Current model    {model}
  Session messages {message_count}
  Session turns    {turns}

Usage
  Inspect current model with /model
  Switch models with /model <name>"
    )
}

fn format_model_switch_report(previous: &str, next: &str, message_count: usize) -> String {
    format!(
        "Model updated
  Previous         {previous}
  Current          {next}
  Preserved msgs   {message_count}"
    )
}

fn format_permissions_report(mode: &str) -> String {
    let modes = [
        ("read-only", "Read/search tools only", mode == "read-only"),
        (
            "workspace-write",
            "Edit files inside the workspace",
            mode == "workspace-write",
        ),
        (
            "danger-full-access",
            "Unrestricted tool access",
            mode == "danger-full-access",
        ),
    ]
    .into_iter()
    .map(|(name, description, is_current)| {
        let marker = if is_current {
            "● current"
        } else {
            "○ available"
        };
        format!("  {name:<18} {marker:<11} {description}")
    })
    .collect::<Vec<_>>()
    .join(
        "
",
    );

    format!(
        "Permissions
  Active mode      {mode}
  Mode status      live session default

Modes
{modes}

Usage
  Inspect current mode with /permissions
  Switch modes with /permissions <mode>"
    )
}

fn format_permissions_switch_report(previous: &str, next: &str) -> String {
    format!(
        "Permissions updated
  Result           mode switched
  Previous mode    {previous}
  Active mode      {next}
  Applies to       subsequent tool calls
  Usage            /permissions to inspect current mode"
    )
}

fn format_cost_report(usage: TokenUsage) -> String {
    format!(
        "Cost
  Input tokens     {}
  Output tokens    {}
  Cache create     {}
  Cache read       {}
  Total tokens     {}",
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_creation_input_tokens,
        usage.cache_read_input_tokens,
        usage.total_tokens(),
    )
}

fn format_resume_report(session_path: &str, message_count: usize, turns: u32) -> String {
    format!(
        "Session resumed
  Session file     {session_path}
  Messages         {message_count}
  Turns            {turns}"
    )
}

fn format_compact_report(removed: usize, resulting_messages: usize, skipped: bool) -> String {
    if skipped {
        format!(
            "Compact
  Result           skipped
  Reason           session below compaction threshold
  Messages kept    {resulting_messages}"
        )
    } else {
        format!(
            "Compact
  Result           compacted
  Messages removed {removed}
  Messages kept    {resulting_messages}"
        )
    }
}

fn parse_git_status_metadata(status: Option<&str>) -> (Option<PathBuf>, Option<String>) {
    let Some(status) = status else {
        return (None, None);
    };
    let branch = status.lines().next().and_then(|line| {
        line.strip_prefix("## ")
            .map(|line| {
                line.split(['.', ' '])
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
            .filter(|value| !value.is_empty())
    });
    let project_root = find_git_root().ok();
    (project_root, branch)
}

fn find_git_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        return Err("not a git repository".into());
    }
    let path = String::from_utf8(output.stdout)?.trim().to_string();
    if path.is_empty() {
        return Err("empty git root".into());
    }
    Ok(PathBuf::from(path))
}

#[allow(clippy::too_many_lines)]
fn run_resume_command(
    session_path: &Path,
    session: &Session,
    command: &SlashCommand,
) -> Result<ResumeCommandOutcome, Box<dyn std::error::Error>> {
    match command {
        SlashCommand::Help => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_repl_help()),
        }),
        SlashCommand::Compact => {
            let result = runtime::compact_session(
                session,
                CompactionConfig {
                    max_estimated_tokens: 0,
                    ..CompactionConfig::default()
                },
            );
            let removed = result.removed_message_count;
            let kept = result.compacted_session.messages.len();
            let skipped = removed == 0;
            result.compacted_session.save_to_path(session_path)?;
            Ok(ResumeCommandOutcome {
                session: result.compacted_session,
                message: Some(format_compact_report(removed, kept, skipped)),
            })
        }
        SlashCommand::Clear { confirm } => {
            if !confirm {
                return Ok(ResumeCommandOutcome {
                    session: session.clone(),
                    message: Some(
                        "clear: confirmation required; rerun with /clear --confirm".to_string(),
                    ),
                });
            }
            let cleared = Session::new();
            cleared.save_to_path(session_path)?;
            Ok(ResumeCommandOutcome {
                session: cleared,
                message: Some(format!(
                    "Cleared resumed session file {}.",
                    session_path.display()
                )),
            })
        }
        SlashCommand::Status => {
            let tracker = UsageTracker::from_session(session);
            let usage = tracker.cumulative_usage();
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format_status_report(
                    "restored-session",
                    StatusUsage {
                        message_count: session.messages.len(),
                        turns: tracker.turns(),
                        latest: tracker.current_turn_usage(),
                        cumulative: usage,
                        estimated_tokens: 0,
                    },
                    default_permission_mode().as_str(),
                    &status_context(Some(session_path))?,
                )),
            })
        }
        SlashCommand::Cost => {
            let usage = UsageTracker::from_session(session).cumulative_usage();
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format_cost_report(usage)),
            })
        }
        SlashCommand::Config { section } => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_config_report(section.as_deref())?),
        }),
        SlashCommand::Memory => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_memory_report()?),
        }),
        SlashCommand::Init => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(init_claw_md(&InitOptions::default())?),
        }),
        SlashCommand::Diff => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_diff_report()?),
        }),
        SlashCommand::Version => Ok(ResumeCommandOutcome {
            session: session.clone(),
            message: Some(render_version_report()),
        }),
        SlashCommand::Export { path } => {
            let export_path = resolve_export_path(path.as_deref(), session)?;
            fs::write(&export_path, render_export_text(session))?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(format!(
                    "Export\n  Result           wrote transcript\n  File             {}\n  Messages         {}",
                    export_path.display(),
                    session.messages.len(),
                )),
            })
        }
        SlashCommand::Agents { args } => {
            let cwd = env::current_dir()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(handle_agents_slash_command(args.as_deref(), &cwd)?),
            })
        }
        SlashCommand::Skills { args } => {
            let cwd = env::current_dir()?;
            Ok(ResumeCommandOutcome {
                session: session.clone(),
                message: Some(handle_skills_slash_command(args.as_deref(), &cwd)?),
            })
        }
        SlashCommand::Branch { .. }
        | SlashCommand::Worktree { .. }
        | SlashCommand::CommitPushPr { .. }
        | SlashCommand::Bughunter { .. }
        | SlashCommand::Commit
        | SlashCommand::Pr { .. }
        | SlashCommand::Issue { .. }
        | SlashCommand::Ultraplan { .. }
        | SlashCommand::Teleport { .. }
        | SlashCommand::DebugToolCall
        | SlashCommand::Resume { .. }
        | SlashCommand::Model { .. }
        | SlashCommand::Permissions { .. }
        | SlashCommand::Session { .. }
        | SlashCommand::Plugins { .. }
        | SlashCommand::Unknown(_) => Err("unsupported resumed slash command".into()),
    }
}

fn run_repl(
    model: Option<String>,
    provider: Option<String>,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cli = LiveCli::new(model, provider, true, allowed_tools, permission_mode)?;
    let mut editor = input::LineEditor::new("> ", slash_command_completion_candidates());
    println!("{}", cli.startup_banner());

    loop {
        match editor.read_line()? {
            input::ReadOutcome::Submit(input) => {
                let trimmed = input.trim().to_string();
                if trimmed.is_empty() {
                    continue;
                }
                if matches!(trimmed.as_str(), "/exit" | "/quit") {
                    cli.persist_session()?;
                    break;
                }
                if let Some(command) = SlashCommand::parse(&trimmed) {
                    if cli.handle_repl_command(command)? {
                        cli.persist_session()?;
                    }
                    continue;
                }
                editor.push_history(input);
                cli.run_turn(&trimmed)?;
            }
            input::ReadOutcome::Cancel => {}
            input::ReadOutcome::Exit => {
                cli.persist_session()?;
                break;
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct SessionHandle {
    id: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
struct ManagedSessionSummary {
    id: String,
    path: PathBuf,
    modified_epoch_secs: u64,
    message_count: usize,
}

struct LiveCli {
    model: String,
    provider: Option<String>,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    system_prompt: Vec<String>,
    runtime_config: RuntimeConfig,
    feature_config: RuntimeFeatureConfig,
    tool_registry: GlobalToolRegistry,
    runtime: ConversationRuntime<DefaultRuntimeClient, CliToolExecutor>,
    session: SessionHandle,
}

impl LiveCli {
    fn new(
        model: Option<String>,
        provider: Option<String>,
        enable_tools: bool,
        allowed_tools: Option<AllowedToolSet>,
        permission_mode: PermissionMode,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let system_prompt = build_system_prompt()?;
        let session = create_managed_session_handle()?;
        let (runtime_config, feature_config, tool_registry) = build_runtime_plugin_state()?;
        let selection = resolve_client_selection(&runtime_config, model, provider)?;
        let runtime = build_runtime(
            Session::new(),
            selection.clone(),
            system_prompt.clone(),
            enable_tools,
            true,
            allowed_tools.clone(),
            permission_mode,
            None,
            feature_config.clone(),
            tool_registry.clone(),
        )?;
        let cli = Self {
            model: selection.model,
            provider: selection.provider_id,
            allowed_tools,
            permission_mode,
            system_prompt,
            runtime_config,
            feature_config,
            tool_registry,
            runtime,
            session,
        };
        cli.persist_session()?;
        Ok(cli)
    }

    fn startup_banner(&self) -> String {
        let cwd = env::current_dir().map_or_else(
            |_| "<unknown>".to_string(),
            |path| path.display().to_string(),
        );
        let provider = self
            .provider
            .as_deref()
            .map_or_else(|| "auto".to_string(), ToOwned::to_owned);
        format!(
            "\x1b[38;5;173m▖▘  ▝▗\x1b[0m     \x1b[1mOwl CLI v{VERSION}\x1b[0m\n\
\x1b[38;5;173m▗▝▖▗▝▗\x1b[0m     \x1b[2m{} · {}\x1b[0m\n\
\x1b[38;5;173m▗ ▝▘ ▗\x1b[0m     \x1b[2m{}\x1b[0m\n\
\x1b[38;5;173m▝▗    ▖▘\x1b[0m   \x1b[2mPermissions\x1b[0m  {}\n\
\x1b[38;5;173m ▝▖▗▖▘\x1b[0m     \x1b[2mSession\x1b[0m      {}\n\
\x1b[38;5;173m━━▝▘▝▘━━\x1b[0m   \x1b[2mWorkflow\x1b[0m     metadata → scoring → psychometrics → report\n\n\
  Type \x1b[1m/help\x1b[0m for commands · \x1b[2mShift+Enter\x1b[0m for newline",
            self.model,
            provider,
            cwd,
            self.permission_mode.as_str(),
            self.session.id,
        )
    }

    fn build_runtime_for_session(
        &self,
        session: Session,
        model: Option<String>,
        enable_tools: bool,
        emit_output: bool,
        progress: Option<InternalPromptProgressReporter>,
    ) -> Result<
        ConversationRuntime<DefaultRuntimeClient, CliToolExecutor>,
        Box<dyn std::error::Error>,
    > {
        let selection =
            resolve_client_selection(&self.runtime_config, model, self.provider.clone())?;
        build_runtime(
            session,
            selection,
            self.system_prompt.clone(),
            enable_tools,
            emit_output,
            self.allowed_tools.clone(),
            self.permission_mode,
            progress,
            self.feature_config.clone(),
            self.tool_registry.clone(),
        )
    }

    fn reload_runtime_state(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let (runtime_config, feature_config, tool_registry) = build_runtime_plugin_state()?;
        self.runtime_config = runtime_config;
        self.feature_config = feature_config;
        self.tool_registry = tool_registry;
        Ok(())
    }

    fn run_turn(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut spinner = Spinner::new();
        let mut stdout = io::stdout();
        spinner.tick(
            "🦀 Thinking...",
            TerminalRenderer::new().color_theme(),
            &mut stdout,
        )?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let result = self.runtime.run_turn(input, Some(&mut permission_prompter));
        match result {
            Ok(_) => {
                spinner.finish(
                    "✨ Done",
                    TerminalRenderer::new().color_theme(),
                    &mut stdout,
                )?;
                println!();
                self.persist_session()?;
                Ok(())
            }
            Err(error) => {
                spinner.fail(
                    "❌ Request failed",
                    TerminalRenderer::new().color_theme(),
                    &mut stdout,
                )?;
                Err(Box::new(error))
            }
        }
    }

    fn run_turn_with_output(
        &mut self,
        input: &str,
        output_format: CliOutputFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match output_format {
            CliOutputFormat::Text => self.run_turn(input),
            CliOutputFormat::Json => self.run_prompt_json(input),
        }
    }

    fn run_prompt_json(&mut self, input: &str) -> Result<(), Box<dyn std::error::Error>> {
        let session = self.runtime.session().clone();
        let mut runtime =
            self.build_runtime_for_session(session, Some(self.model.clone()), true, false, None)?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let summary = runtime.run_turn(input, Some(&mut permission_prompter))?;
        self.runtime = runtime;
        self.persist_session()?;
        println!(
            "{}",
            json!({
                "message": final_assistant_text(&summary),
                "model": self.model,
                "iterations": summary.iterations,
                "tool_uses": collect_tool_uses(&summary),
                "tool_results": collect_tool_results(&summary),
                "usage": {
                    "input_tokens": summary.usage.input_tokens,
                    "output_tokens": summary.usage.output_tokens,
                    "cache_creation_input_tokens": summary.usage.cache_creation_input_tokens,
                    "cache_read_input_tokens": summary.usage.cache_read_input_tokens,
                }
            })
        );
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn handle_repl_command(
        &mut self,
        command: SlashCommand,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(match command {
            SlashCommand::Help => {
                println!("{}", render_repl_help());
                false
            }
            SlashCommand::Status => {
                self.print_status();
                false
            }
            SlashCommand::Bughunter { scope } => {
                self.run_bughunter(scope.as_deref())?;
                false
            }
            SlashCommand::Commit => {
                self.run_commit()?;
                true
            }
            SlashCommand::Pr { context } => {
                self.run_pr(context.as_deref())?;
                false
            }
            SlashCommand::Issue { context } => {
                self.run_issue(context.as_deref())?;
                false
            }
            SlashCommand::Ultraplan { task } => {
                self.run_ultraplan(task.as_deref())?;
                false
            }
            SlashCommand::Teleport { target } => {
                self.run_teleport(target.as_deref())?;
                false
            }
            SlashCommand::DebugToolCall => {
                self.run_debug_tool_call()?;
                false
            }
            SlashCommand::Compact => {
                self.compact()?;
                false
            }
            SlashCommand::Model { model } => self.set_model(model)?,
            SlashCommand::Permissions { mode } => self.set_permissions(mode)?,
            SlashCommand::Clear { confirm } => self.clear_session(confirm)?,
            SlashCommand::Cost => {
                self.print_cost();
                false
            }
            SlashCommand::Resume { session_path } => self.resume_session(session_path)?,
            SlashCommand::Config { section } => {
                Self::print_config(section.as_deref())?;
                false
            }
            SlashCommand::Memory => {
                Self::print_memory()?;
                false
            }
            SlashCommand::Init => {
                run_init(&InitOptions::default())?;
                false
            }
            SlashCommand::Diff => {
                Self::print_diff()?;
                false
            }
            SlashCommand::Version => {
                Self::print_version();
                false
            }
            SlashCommand::Export { path } => {
                self.export_session(path.as_deref())?;
                false
            }
            SlashCommand::Session { action, target } => {
                self.handle_session_command(action.as_deref(), target.as_deref())?
            }
            SlashCommand::Plugins { action, target } => {
                self.handle_plugins_command(action.as_deref(), target.as_deref())?
            }
            SlashCommand::Agents { args } => {
                Self::print_agents(args.as_deref())?;
                false
            }
            SlashCommand::Skills { args } => {
                Self::print_skills(args.as_deref())?;
                false
            }
            SlashCommand::Branch { .. } => {
                eprintln!("branch commands are not available in this build");
                false
            }
            SlashCommand::Worktree { .. } => {
                eprintln!("worktree commands are not available in this build");
                false
            }
            SlashCommand::CommitPushPr { .. } => {
                eprintln!("commit-push-pr automation is not available in this build");
                false
            }
            SlashCommand::Unknown(name) => {
                eprintln!("unknown slash command: /{name}");
                false
            }
        })
    }

    fn persist_session(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.runtime.session().save_to_path(&self.session.path)?;
        Ok(())
    }

    fn print_status(&self) {
        let cumulative = self.runtime.usage().cumulative_usage();
        let latest = self.runtime.usage().current_turn_usage();
        println!(
            "{}",
            format_status_report(
                &self.model,
                StatusUsage {
                    message_count: self.runtime.session().messages.len(),
                    turns: self.runtime.usage().turns(),
                    latest,
                    cumulative,
                    estimated_tokens: self.runtime.estimated_tokens(),
                },
                self.permission_mode.as_str(),
                &status_context(Some(&self.session.path)).expect("status context should load"),
            )
        );
    }

    fn set_model(&mut self, model: Option<String>) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(model) = model else {
            println!(
                "{}",
                format_model_report(
                    &self.model,
                    self.runtime.session().messages.len(),
                    self.runtime.usage().turns(),
                )
            );
            return Ok(false);
        };

        let model = resolve_model_alias(&model);

        if model == self.model {
            println!(
                "{}",
                format_model_report(
                    &self.model,
                    self.runtime.session().messages.len(),
                    self.runtime.usage().turns(),
                )
            );
            return Ok(false);
        }

        let previous = self.model.clone();
        let session = self.runtime.session().clone();
        let message_count = session.messages.len();
        self.runtime =
            self.build_runtime_for_session(session, Some(model.clone()), true, true, None)?;
        self.model.clone_from(&model);
        println!(
            "{}",
            format_model_switch_report(&previous, &model, message_count)
        );
        Ok(true)
    }

    fn set_permissions(
        &mut self,
        mode: Option<String>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(mode) = mode else {
            println!(
                "{}",
                format_permissions_report(self.permission_mode.as_str())
            );
            return Ok(false);
        };

        let normalized = normalize_permission_mode(&mode).ok_or_else(|| {
            format!(
                "unsupported permission mode '{mode}'. Use read-only, workspace-write, or danger-full-access."
            )
        })?;

        if normalized == self.permission_mode.as_str() {
            println!("{}", format_permissions_report(normalized));
            return Ok(false);
        }

        let previous = self.permission_mode.as_str().to_string();
        let session = self.runtime.session().clone();
        self.permission_mode = permission_mode_from_label(normalized);
        self.runtime =
            self.build_runtime_for_session(session, Some(self.model.clone()), true, true, None)?;
        println!(
            "{}",
            format_permissions_switch_report(&previous, normalized)
        );
        Ok(true)
    }

    fn clear_session(&mut self, confirm: bool) -> Result<bool, Box<dyn std::error::Error>> {
        if !confirm {
            println!(
                "clear: confirmation required; run /clear --confirm to start a fresh session."
            );
            return Ok(false);
        }

        self.session = create_managed_session_handle()?;
        self.runtime = self.build_runtime_for_session(
            Session::new(),
            Some(self.model.clone()),
            true,
            true,
            None,
        )?;
        println!(
            "Session cleared\n  Mode             fresh session\n  Preserved model  {}\n  Permission mode  {}\n  Session          {}",
            self.model,
            self.permission_mode.as_str(),
            self.session.id,
        );
        Ok(true)
    }

    fn print_cost(&self) {
        let cumulative = self.runtime.usage().cumulative_usage();
        println!("{}", format_cost_report(cumulative));
    }

    fn resume_session(
        &mut self,
        session_path: Option<String>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(session_ref) = session_path else {
            println!("Usage: /resume <session-path>");
            return Ok(false);
        };

        let handle = resolve_session_reference(&session_ref)?;
        let session = Session::load_from_path(&handle.path)?;
        let message_count = session.messages.len();
        self.runtime =
            self.build_runtime_for_session(session, Some(self.model.clone()), true, true, None)?;
        self.session = handle;
        println!(
            "{}",
            format_resume_report(
                &self.session.path.display().to_string(),
                message_count,
                self.runtime.usage().turns(),
            )
        );
        Ok(true)
    }

    fn print_config(section: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_config_report(section)?);
        Ok(())
    }

    fn print_memory() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_memory_report()?);
        Ok(())
    }

    fn print_agents(args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        println!("{}", handle_agents_slash_command(args, &cwd)?);
        Ok(())
    }

    fn print_skills(args: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        println!("{}", handle_skills_slash_command(args, &cwd)?);
        Ok(())
    }

    fn print_diff() -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_diff_report()?);
        Ok(())
    }

    fn print_version() {
        println!("{}", render_version_report());
    }

    fn export_session(
        &self,
        requested_path: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let export_path = resolve_export_path(requested_path, self.runtime.session())?;
        fs::write(&export_path, render_export_text(self.runtime.session()))?;
        println!(
            "Export\n  Result           wrote transcript\n  File             {}\n  Messages         {}",
            export_path.display(),
            self.runtime.session().messages.len(),
        );
        Ok(())
    }

    fn handle_session_command(
        &mut self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        match action {
            None | Some("list") => {
                println!("{}", render_session_list(&self.session.id)?);
                Ok(false)
            }
            Some("switch") => {
                let Some(target) = target else {
                    println!("Usage: /session switch <session-id>");
                    return Ok(false);
                };
                let handle = resolve_session_reference(target)?;
                let session = Session::load_from_path(&handle.path)?;
                let message_count = session.messages.len();
                self.runtime = self.build_runtime_for_session(
                    session,
                    Some(self.model.clone()),
                    true,
                    true,
                    None,
                )?;
                self.session = handle;
                println!(
                    "Session switched\n  Active session   {}\n  File             {}\n  Messages         {}",
                    self.session.id,
                    self.session.path.display(),
                    message_count,
                );
                Ok(true)
            }
            Some(other) => {
                println!("Unknown /session action '{other}'. Use /session list or /session switch <session-id>.");
                Ok(false)
            }
        }
    }

    fn handle_plugins_command(
        &mut self,
        action: Option<&str>,
        target: Option<&str>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        let loader = ConfigLoader::default_for(&cwd);
        let runtime_config = loader.load()?;
        let mut manager = build_plugin_manager(&cwd, &loader, &runtime_config);
        let result = handle_plugins_slash_command(action, target, &mut manager)?;
        println!("{}", result.message);
        if result.reload_runtime {
            self.reload_runtime_features()?;
        }
        Ok(false)
    }

    fn reload_runtime_features(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.reload_runtime_state()?;
        self.runtime = self.build_runtime_for_session(
            self.runtime.session().clone(),
            Some(self.model.clone()),
            true,
            true,
            None,
        )?;
        self.persist_session()
    }

    fn compact(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let result = self.runtime.compact(CompactionConfig::default());
        let removed = result.removed_message_count;
        let kept = result.compacted_session.messages.len();
        let skipped = removed == 0;
        self.runtime = self.build_runtime_for_session(
            result.compacted_session,
            Some(self.model.clone()),
            true,
            true,
            None,
        )?;
        self.persist_session()?;
        println!("{}", format_compact_report(removed, kept, skipped));
        Ok(())
    }

    fn run_internal_prompt_text_with_progress(
        &self,
        prompt: &str,
        enable_tools: bool,
        progress: Option<InternalPromptProgressReporter>,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let session = self.runtime.session().clone();
        let mut runtime = self.build_runtime_for_session(
            session,
            Some(self.model.clone()),
            enable_tools,
            false,
            progress,
        )?;
        let mut permission_prompter = CliPermissionPrompter::new(self.permission_mode);
        let summary = runtime.run_turn(prompt, Some(&mut permission_prompter))?;
        Ok(final_assistant_text(&summary).trim().to_string())
    }

    fn run_internal_prompt_text(
        &self,
        prompt: &str,
        enable_tools: bool,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.run_internal_prompt_text_with_progress(prompt, enable_tools, None)
    }

    fn run_bughunter(&self, scope: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let scope = scope.unwrap_or("the current repository");
        let prompt = format!(
            "You are /bughunter. Inspect {scope} and identify the most likely bugs or correctness issues. Prioritize concrete findings with file paths, severity, and suggested fixes. Use tools if needed."
        );
        println!("{}", self.run_internal_prompt_text(&prompt, true)?);
        Ok(())
    }

    fn run_ultraplan(&self, task: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let task = task.unwrap_or("the current repo work");
        let prompt = format!(
            "You are /ultraplan. Produce a deep multi-step execution plan for {task}. Include goals, risks, implementation sequence, verification steps, and rollback considerations. Use tools if needed."
        );
        let mut progress = InternalPromptProgressRun::start_ultraplan(task);
        match self.run_internal_prompt_text_with_progress(&prompt, true, Some(progress.reporter()))
        {
            Ok(plan) => {
                progress.finish_success();
                println!("{plan}");
                Ok(())
            }
            Err(error) => {
                progress.finish_failure(&error.to_string());
                Err(error)
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn run_teleport(&self, target: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let Some(target) = target.map(str::trim).filter(|value| !value.is_empty()) else {
            println!("Usage: /teleport <symbol-or-path>");
            return Ok(());
        };

        println!("{}", render_teleport_report(target)?);
        Ok(())
    }

    fn run_debug_tool_call(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("{}", render_last_tool_debug_report(self.runtime.session())?);
        Ok(())
    }

    fn run_commit(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let status = git_output(&["status", "--short"])?;
        if status.trim().is_empty() {
            println!("Commit\n  Result           skipped\n  Reason           no workspace changes");
            return Ok(());
        }

        git_status_ok(&["add", "-A"])?;
        let staged_stat = git_output(&["diff", "--cached", "--stat"])?;
        let prompt = format!(
            "Generate a git commit message in plain text Lore format only. Base it on this staged diff summary:\n\n{}\n\nRecent conversation context:\n{}",
            truncate_for_prompt(&staged_stat, 8_000),
            recent_user_context(self.runtime.session(), 6)
        );
        let message = sanitize_generated_message(&self.run_internal_prompt_text(&prompt, false)?);
        if message.trim().is_empty() {
            return Err("generated commit message was empty".into());
        }

        let path = write_temp_text_file("claw-commit-message.txt", &message)?;
        let output = Command::new("git")
            .args(["commit", "--file"])
            .arg(&path)
            .current_dir(env::current_dir()?)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!("git commit failed: {stderr}").into());
        }

        println!(
            "Commit\n  Result           created\n  Message file     {}\n\n{}",
            path.display(),
            message.trim()
        );
        Ok(())
    }

    fn run_pr(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let staged = git_output(&["diff", "--stat"])?;
        let prompt = format!(
            "Generate a pull request title and body from this conversation and diff summary. Output plain text in this format exactly:\nTITLE: <title>\nBODY:\n<body markdown>\n\nContext hint: {}\n\nDiff summary:\n{}",
            context.unwrap_or("none"),
            truncate_for_prompt(&staged, 10_000)
        );
        let draft = sanitize_generated_message(&self.run_internal_prompt_text(&prompt, false)?);
        let (title, body) = parse_titled_body(&draft)
            .ok_or_else(|| "failed to parse generated PR title/body".to_string())?;

        if command_exists("gh") {
            let body_path = write_temp_text_file("claw-pr-body.md", &body)?;
            let output = Command::new("gh")
                .args(["pr", "create", "--title", &title, "--body-file"])
                .arg(&body_path)
                .current_dir(env::current_dir()?)
                .output()?;
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!(
                    "PR\n  Result           created\n  Title            {title}\n  URL              {}",
                    if stdout.is_empty() { "<unknown>" } else { &stdout }
                );
                return Ok(());
            }
        }

        println!("PR draft\n  Title            {title}\n\n{body}");
        Ok(())
    }

    fn run_issue(&self, context: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
        let prompt = format!(
            "Generate a GitHub issue title and body from this conversation. Output plain text in this format exactly:\nTITLE: <title>\nBODY:\n<body markdown>\n\nContext hint: {}\n\nConversation context:\n{}",
            context.unwrap_or("none"),
            truncate_for_prompt(&recent_user_context(self.runtime.session(), 10), 10_000)
        );
        let draft = sanitize_generated_message(&self.run_internal_prompt_text(&prompt, false)?);
        let (title, body) = parse_titled_body(&draft)
            .ok_or_else(|| "failed to parse generated issue title/body".to_string())?;

        if command_exists("gh") {
            let body_path = write_temp_text_file("claw-issue-body.md", &body)?;
            let output = Command::new("gh")
                .args(["issue", "create", "--title", &title, "--body-file"])
                .arg(&body_path)
                .current_dir(env::current_dir()?)
                .output()?;
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!(
                    "Issue\n  Result           created\n  Title            {title}\n  URL              {}",
                    if stdout.is_empty() { "<unknown>" } else { &stdout }
                );
                return Ok(());
            }
        }

        println!("Issue draft\n  Title            {title}\n\n{body}");
        Ok(())
    }
}

fn sessions_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let path = cwd.join(".claw").join("sessions");
    fs::create_dir_all(&path)?;
    Ok(path)
}

fn create_managed_session_handle() -> Result<SessionHandle, Box<dyn std::error::Error>> {
    let id = generate_session_id();
    let path = sessions_dir()?.join(format!("{id}.json"));
    Ok(SessionHandle { id, path })
}

fn generate_session_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("session-{millis}")
}

fn resolve_session_reference(reference: &str) -> Result<SessionHandle, Box<dyn std::error::Error>> {
    let direct = PathBuf::from(reference);
    let path = if direct.exists() {
        direct
    } else {
        sessions_dir()?.join(format!("{reference}.json"))
    };
    if !path.exists() {
        return Err(format!("session not found: {reference}").into());
    }
    let id = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(reference)
        .to_string();
    Ok(SessionHandle { id, path })
}

fn list_managed_sessions() -> Result<Vec<ManagedSessionSummary>, Box<dyn std::error::Error>> {
    let mut sessions = Vec::new();
    for entry in fs::read_dir(sessions_dir()?)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let metadata = entry.metadata()?;
        let modified_epoch_secs = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs())
            .unwrap_or_default();
        let message_count = Session::load_from_path(&path)
            .map(|session| session.messages.len())
            .unwrap_or_default();
        let id = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("unknown")
            .to_string();
        sessions.push(ManagedSessionSummary {
            id,
            path,
            modified_epoch_secs,
            message_count,
        });
    }
    sessions.sort_by(|left, right| right.modified_epoch_secs.cmp(&left.modified_epoch_secs));
    Ok(sessions)
}

fn render_session_list(active_session_id: &str) -> Result<String, Box<dyn std::error::Error>> {
    let sessions = list_managed_sessions()?;
    let mut lines = vec![
        "Sessions".to_string(),
        format!("  Directory         {}", sessions_dir()?.display()),
    ];
    if sessions.is_empty() {
        lines.push("  No managed sessions saved yet.".to_string());
        return Ok(lines.join("\n"));
    }
    for session in sessions {
        let marker = if session.id == active_session_id {
            "● current"
        } else {
            "○ saved"
        };
        lines.push(format!(
            "  {id:<20} {marker:<10} msgs={msgs:<4} modified={modified} path={path}",
            id = session.id,
            msgs = session.message_count,
            modified = session.modified_epoch_secs,
            path = session.path.display(),
        ));
    }
    Ok(lines.join("\n"))
}

fn render_repl_help() -> String {
    [
        "REPL".to_string(),
        "  /exit                Quit the REPL".to_string(),
        "  /quit                Quit the REPL".to_string(),
        "  Up/Down              Navigate prompt history".to_string(),
        "  Tab                  Complete slash commands".to_string(),
        "  Ctrl-C               Clear input (or exit on empty prompt)".to_string(),
        "  Shift+Enter/Ctrl+J   Insert a newline".to_string(),
        String::new(),
        render_slash_command_help(),
    ]
    .join(
        "
",
    )
}

fn status_context(
    session_path: Option<&Path>,
) -> Result<StatusContext, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let discovered_config_files = loader.discover().len();
    let runtime_config = loader.load()?;
    let project_context = ProjectContext::discover_with_git(&cwd, DEFAULT_DATE)?;
    let (project_root, git_branch) =
        parse_git_status_metadata(project_context.git_status.as_deref());
    Ok(StatusContext {
        cwd,
        session_path: session_path.map(Path::to_path_buf),
        loaded_config_files: runtime_config.loaded_entries().len(),
        discovered_config_files,
        memory_file_count: project_context.instruction_files.len(),
        project_root,
        git_branch,
    })
}

fn format_status_report(
    model: &str,
    usage: StatusUsage,
    permission_mode: &str,
    context: &StatusContext,
) -> String {
    [
        format!(
            "Status
  Model            {model}
  Permission mode  {permission_mode}
  Messages         {}
  Turns            {}
  Estimated tokens {}",
            usage.message_count, usage.turns, usage.estimated_tokens,
        ),
        format!(
            "Usage
  Latest total     {}
  Cumulative input {}
  Cumulative output {}
  Cumulative total {}",
            usage.latest.total_tokens(),
            usage.cumulative.input_tokens,
            usage.cumulative.output_tokens,
            usage.cumulative.total_tokens(),
        ),
        format!(
            "Workspace
  Cwd              {}
  Project root     {}
  Git branch       {}
  Session          {}
  Config files     loaded {}/{}
  Memory files     {}",
            context.cwd.display(),
            context
                .project_root
                .as_ref()
                .map_or_else(|| "unknown".to_string(), |path| path.display().to_string()),
            context.git_branch.as_deref().unwrap_or("unknown"),
            context.session_path.as_ref().map_or_else(
                || "live-repl".to_string(),
                |path| path.display().to_string()
            ),
            context.loaded_config_files,
            context.discovered_config_files,
            context.memory_file_count,
        ),
    ]
    .join(
        "

",
    )
}

fn render_config_report(section: Option<&str>) -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let discovered = loader.discover();
    let runtime_config = loader.load()?;

    let mut lines = vec![
        format!(
            "Config
  Working directory {}
  Loaded files      {}
  Merged keys       {}",
            cwd.display(),
            runtime_config.loaded_entries().len(),
            runtime_config.merged().len()
        ),
        "Discovered files".to_string(),
    ];
    for entry in discovered {
        let source = match entry.source {
            ConfigSource::User => "user",
            ConfigSource::Project => "project",
            ConfigSource::Local => "local",
        };
        let status = if runtime_config
            .loaded_entries()
            .iter()
            .any(|loaded_entry| loaded_entry.path == entry.path)
        {
            "loaded"
        } else {
            "missing"
        };
        lines.push(format!(
            "  {source:<7} {status:<7} {}",
            entry.path.display()
        ));
    }

    if let Some(section) = section {
        lines.push(format!("Merged section: {section}"));
        let value = match section {
            "env" => runtime_config.get("env"),
            "hooks" => runtime_config.get("hooks"),
            "model" => runtime_config.get("model"),
            "providers" => runtime_config.get("providers"),
            "research" => runtime_config.get("research"),
            "plugins" => runtime_config
                .get("plugins")
                .or_else(|| runtime_config.get("enabledPlugins")),
            other => {
                lines.push(format!(
                    "  Unsupported config section '{other}'. Use env, hooks, model, providers, research, or plugins."
                ));
                return Ok(lines.join(
                    "
",
                ));
            }
        };
        lines.push(format!(
            "  {}",
            match value {
                Some(value) => value.render(),
                None => "<unset>".to_string(),
            }
        ));
        return Ok(lines.join(
            "
",
        ));
    }

    lines.push("Merged JSON".to_string());
    lines.push(format!("  {}", runtime_config.as_json().render()));
    Ok(lines.join(
        "
",
    ))
}

fn render_memory_report() -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let project_context = ProjectContext::discover(&cwd, DEFAULT_DATE)?;
    let mut lines = vec![format!(
        "Memory
  Working directory {}
  Instruction files {}",
        cwd.display(),
        project_context.instruction_files.len()
    )];
    if project_context.instruction_files.is_empty() {
        lines.push("Discovered files".to_string());
        lines.push(
            "  No CLAW instruction files discovered in the current directory ancestry.".to_string(),
        );
    } else {
        lines.push("Discovered files".to_string());
        for (index, file) in project_context.instruction_files.iter().enumerate() {
            let preview = file.content.lines().next().unwrap_or("").trim();
            let preview = if preview.is_empty() {
                "<empty>"
            } else {
                preview
            };
            lines.push(format!("  {}. {}", index + 1, file.path.display(),));
            lines.push(format!(
                "     lines={} preview={}",
                file.content.lines().count(),
                preview
            ));
        }
    }
    Ok(lines.join(
        "
",
    ))
}

fn init_claw_md(options: &InitOptions) -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    Ok(initialize_repo(&cwd, options)?.render())
}

fn run_init(options: &InitOptions) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", init_claw_md(options)?);
    Ok(())
}

fn run_project_skill_command(
    command: &ProjectSkillCommand,
    output_format: CliOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;

    match command {
        ProjectSkillCommand::Init {
            slug,
            title,
            description,
            domain,
            use_when,
            sources,
            input_expectations,
            workflow_steps,
            outputs,
            limits,
            failure_checks,
            evaluation_examples,
            generated_by,
            maturity,
            output_root,
            targets,
            openclaw_root,
            claude_root,
        } => {
            let script_path = find_project_skill_scaffold_script(&cwd)?;
            let mut process = Command::new("python3");
            process.arg(&script_path).current_dir(&cwd);
            process
                .arg("--slug")
                .arg(slug)
                .arg("--title")
                .arg(title)
                .arg("--description")
                .arg(description)
                .arg("--domain")
                .arg(domain)
                .arg("--use-when")
                .arg(use_when)
                .arg("--generated-by")
                .arg(generated_by)
                .arg("--maturity")
                .arg(maturity);

            for source in sources {
                process.arg("--source").arg(source);
            }
            for input in input_expectations {
                process.arg("--input-expectation").arg(input);
            }
            for step in workflow_steps {
                process.arg("--workflow-step").arg(step);
            }
            for output in outputs {
                process.arg("--output").arg(output);
            }
            for limit in limits {
                process.arg("--limit").arg(limit);
            }
            for check in failure_checks {
                process.arg("--failure-check").arg(check);
            }
            for example in evaluation_examples {
                process.arg("--evaluation-example").arg(example);
            }
            if let Some(root) = output_root {
                process.arg("--output-root").arg(root);
            }
            for target in targets {
                process.arg("--target").arg(target);
            }
            if let Some(root) = openclaw_root {
                process.arg("--openclaw-root").arg(root);
            }
            if let Some(root) = claude_root {
                process.arg("--claude-root").arg(root);
            }

            let output = process.output()?;
            if !output.stdout.is_empty() {
                match output_format {
                    CliOutputFormat::Text => {
                        print!("{}", String::from_utf8_lossy(&output.stdout));
                    }
                    CliOutputFormat::Json => {
                        let raw = String::from_utf8_lossy(&output.stdout);
                        let parsed: serde_json::Value = serde_json::from_str(&raw)?;
                        println!("{}", serde_json::to_string_pretty(&parsed)?);
                    }
                }
            }
            if !output.stderr.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&output.stderr));
            }
            if !output.status.success() {
                return Err("project-skill scaffold command failed".into());
            }
            Ok(())
        }
        ProjectSkillCommand::Validate { path } => {
            let report = validate_project_skill_report(path)?;
            match output_format {
                CliOutputFormat::Text => println!("{}", report.text),
                CliOutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report.json)?)
                }
            }
            Ok(())
        }
        ProjectSkillCommand::Promote {
            path,
            to,
            verification_status,
            held_out_validation_status,
        } => {
            let report = promote_project_skill(
                path,
                to,
                verification_status.as_deref(),
                held_out_validation_status.as_deref(),
            )?;
            match output_format {
                CliOutputFormat::Text => println!("{}", report.text),
                CliOutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report.json)?)
                }
            }
            Ok(())
        }
        ProjectSkillCommand::Doctor { path } => {
            let report = doctor_project_skill(path)?;
            match output_format {
                CliOutputFormat::Text => println!("{}", report.text),
                CliOutputFormat::Json => {
                    println!("{}", serde_json::to_string_pretty(&report.json)?)
                }
            }
            Ok(())
        }
    }
}

fn run_plugins_command(
    action: Option<&str>,
    target: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load()?;
    let mut manager = build_plugin_manager(&cwd, &loader, &runtime_config);
    let result = handle_plugins_slash_command(action, target, &mut manager)?;
    println!("{}", result.message);
    Ok(())
}

fn resolve_project_skill_root(path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()?.join(path)
    };
    Ok(if candidate.is_dir() {
        candidate
    } else {
        candidate
            .parent()
            .ok_or_else(|| "project-skill validate path has no parent".to_string())?
            .to_path_buf()
    })
}

fn ensure_project_skill_files(skill_root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let skill_md = skill_root.join("SKILL.md");
    let metadata = skill_root.join("skill.json");
    let readme = skill_root.join("README.md");

    let mut missing = Vec::new();
    for required in [&skill_md, &metadata, &readme] {
        if !required.is_file() {
            missing.push(required.display().to_string());
        }
    }
    if !missing.is_empty() {
        return Err(format!("missing project-skill files: {}", missing.join(", ")).into());
    }
    Ok(())
}

fn read_project_skill_metadata(
    skill_root: &Path,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let metadata = skill_root.join("skill.json");
    let raw = fs::read_to_string(&metadata)?;
    Ok(serde_json::from_str(&raw)?)
}

fn metadata_string<'a>(value: &'a serde_json::Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(|entry| entry.as_str())
}

fn metadata_array<'a>(
    value: &'a serde_json::Value,
    field: &str,
) -> Option<&'a Vec<serde_json::Value>> {
    value.get(field).and_then(|entry| entry.as_array())
}

fn metadata_string_items(
    value: &serde_json::Value,
    field: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let items = metadata_array(value, field)
        .ok_or_else(|| format!("project-skill {field} must be an array"))?;
    let mut rendered = Vec::with_capacity(items.len());
    for item in items {
        let text = item
            .as_str()
            .ok_or_else(|| format!("project-skill {field} entries must be strings"))?;
        rendered.push(text.to_string());
    }
    Ok(rendered)
}

fn render_project_skill_markdown(
    value: &serde_json::Value,
) -> Result<String, Box<dyn std::error::Error>> {
    let name = metadata_string(value, "name")
        .ok_or_else(|| "project-skill name must be a string".to_string())?;
    let description = metadata_string(value, "description")
        .ok_or_else(|| "project-skill description must be a string".to_string())?;
    let title = metadata_string(value, "title")
        .ok_or_else(|| "project-skill title must be a string".to_string())?;
    let domain = metadata_string(value, "domain")
        .ok_or_else(|| "project-skill domain must be a string".to_string())?;
    let use_when = metadata_string(value, "use_when")
        .ok_or_else(|| "project-skill use_when must be a string".to_string())?;
    let generated_at = metadata_string(value, "generated_at")
        .ok_or_else(|| "project-skill generated_at must be a string".to_string())?;
    let maturity = metadata_string(value, "maturity_level")
        .ok_or_else(|| "project-skill maturity_level must be a string".to_string())?;
    let verification_status = metadata_string(value, "verification_status")
        .ok_or_else(|| "project-skill verification_status must be a string".to_string())?;
    let held_out_validation_status = metadata_string(value, "held_out_validation_status")
        .ok_or_else(|| "project-skill held_out_validation_status must be a string".to_string())?;

    let render_bullets = |items: Vec<String>| -> String {
        items
            .into_iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let source_materials = metadata_array(value, "source_materials")
        .ok_or_else(|| "project-skill source_materials must be an array".to_string())?
        .iter()
        .map(|entry| {
            let path = entry
                .get("path")
                .and_then(|item| item.as_str())
                .ok_or_else(|| {
                    "project-skill source_materials.path must be a string".to_string()
                })?;
            let sha = entry.get("sha256").and_then(|item| item.as_str());
            let exists = entry
                .get("exists")
                .and_then(|item| item.as_bool())
                .unwrap_or(false);
            let suffix = match (sha, exists) {
                (Some(sha), true) => format!(" (sha256: `{sha}`)"),
                (Some(sha), false) => format!(" (missing locally, sha256: `{sha}`)"),
                (None, true) => String::new(),
                (None, false) => " (missing locally)".to_string(),
            };
            Ok(format!("- `{path}`{suffix}"))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join("\n");

    Ok(format!(
        "---\nname: {name}\ndescription: {description}\n---\n\n# {title}\n\n## Purpose\n\nThis project-level skill captures a reusable workflow in the **{domain}** domain.\n\n## Use when\n\n{use_when}\n\n## Inputs expected\n\n{inputs}\n\n## Source materials\n\n{sources}\n\n## Workflow\n\n{workflow}\n\n## Expected outputs\n\n{outputs}\n\n## Limits\n\n{limits}\n\n## Failure checks\n\n{failure_checks}\n\n## Evaluation examples\n\n{evaluation_examples}\n\n## Governance metadata\n\n- generated_at: `{generated_at}`\n- maturity: `{maturity}`\n- verification: `{verification_status}`\n- held_out_validation: `{held_out_validation_status}`\n\n## Maintainer notes\n\n- Keep interpretation guidance separate from executable tool contracts.\n- Refresh source material provenance when the workflow changes materially.\n- Promote stable computation into an external or bundled plugin only after the computation boundary stabilizes.\n"
        ,
        inputs = render_bullets(metadata_string_items(value, "input_expectations")?),
        sources = source_materials,
        workflow = render_bullets(metadata_string_items(value, "workflow")?),
        outputs = render_bullets(metadata_string_items(value, "outputs")?),
        limits = render_bullets(metadata_string_items(value, "limits")?),
        failure_checks = render_bullets(metadata_string_items(value, "failure_checks")?),
        evaluation_examples = render_bullets(metadata_string_items(value, "evaluation_examples")?),
    ))
}

fn render_project_skill_readme(
    value: &serde_json::Value,
) -> Result<String, Box<dyn std::error::Error>> {
    let title = metadata_string(value, "title")
        .ok_or_else(|| "project-skill title must be a string".to_string())?;
    let maturity = metadata_string(value, "maturity_level")
        .ok_or_else(|| "project-skill maturity_level must be a string".to_string())?;
    let verification_status = metadata_string(value, "verification_status")
        .ok_or_else(|| "project-skill verification_status must be a string".to_string())?;
    let held_out_validation_status = metadata_string(value, "held_out_validation_status")
        .ok_or_else(|| "project-skill held_out_validation_status must be a string".to_string())?;

    let next_step = match maturity {
        "draft" => format!(
            "Review the workflow, run a realistic example, and record held-out validation before promoting beyond `{maturity}`."
        ),
        "project" => "Reuse this as a governed project-level skill. If the computation path stabilizes across projects, extract the stable execution layer into an external plugin.".to_string(),
        "published" => "Treat this as a published skill contract; changes should preserve compatibility or ship with explicit migration guidance.".to_string(),
        "deprecated" => "Do not extend this skill without a replacement plan; prefer migrating users to the designated successor workflow.".to_string(),
        other => format!("Review the current maturity state (`{other}`) before broader reuse."),
    };

    Ok(format!(
        "# {title}\n\nThis directory contains the canonical governed skill artifact for this workflow.\n\n## Current status\n\n- maturity: `{maturity}`\n- verification: `{verification_status}`\n- held_out_validation: `{held_out_validation_status}`\n\n## What is here\n\n- `SKILL.md` — reusable workflow contract for agents and reviewers\n- `skill.json` — canonical governance metadata consumed by validation and promotion commands\n\n## Next step\n\n{next_step}\n"
    ))
}

fn rewrite_project_skill_docs(
    skill_root: &Path,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        skill_root.join("SKILL.md"),
        render_project_skill_markdown(value)?,
    )?;
    fs::write(
        skill_root.join("README.md"),
        render_project_skill_readme(value)?,
    )?;
    Ok(())
}

struct ProjectSkillCommandReport {
    text: String,
    json: serde_json::Value,
}

fn validate_project_skill_metadata(
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let required_fields = [
        "source_materials",
        "generated_at",
        "generated_by",
        "domain",
        "use_when",
        "input_expectations",
        "workflow",
        "outputs",
        "limits",
        "failure_checks",
        "evaluation_examples",
        "verification_status",
        "held_out_validation_status",
        "maturity_level",
    ];
    let mut missing_fields = Vec::new();
    for field in required_fields {
        if value.get(field).is_none() {
            missing_fields.push(field);
        }
    }
    if !missing_fields.is_empty() {
        return Err(format!(
            "project-skill metadata is missing required fields: {}",
            missing_fields.join(", ")
        )
        .into());
    }

    let required_non_empty_arrays = [
        "source_materials",
        "input_expectations",
        "workflow",
        "outputs",
        "limits",
        "failure_checks",
        "evaluation_examples",
    ];
    let mut empty_fields = Vec::new();
    for field in required_non_empty_arrays {
        match value.get(field).and_then(|entry| entry.as_array()) {
            Some(items) if !items.is_empty() => {}
            _ => empty_fields.push(field),
        }
    }
    if !empty_fields.is_empty() {
        return Err(format!(
            "project-skill metadata requires non-empty arrays for: {}",
            empty_fields.join(", ")
        )
        .into());
    }

    let maturity = value
        .get("maturity_level")
        .and_then(|entry| entry.as_str())
        .ok_or_else(|| "project-skill maturity_level must be a string".to_string())?;
    let allowed_maturity = ["draft", "project", "published", "deprecated"];
    if !allowed_maturity.contains(&maturity) {
        return Err(format!(
            "project-skill maturity_level must be one of: {}",
            allowed_maturity.join(", ")
        )
        .into());
    }

    let verification_status = value
        .get("verification_status")
        .and_then(|entry| entry.as_str())
        .ok_or_else(|| "project-skill verification_status must be a string".to_string())?;
    let allowed_verification_status = [
        "drafted",
        "example-validated",
        "held-out-validated",
        "published",
        "deprecated",
    ];
    if !allowed_verification_status.contains(&verification_status) {
        return Err(format!(
            "project-skill verification_status must be one of: {}",
            allowed_verification_status.join(", ")
        )
        .into());
    }

    let held_out_validation_status = value
        .get("held_out_validation_status")
        .and_then(|entry| entry.as_str())
        .ok_or_else(|| "project-skill held_out_validation_status must be a string".to_string())?;
    let allowed_held_out_status = ["pending", "passed", "failed"];
    if !allowed_held_out_status.contains(&held_out_validation_status) {
        return Err(format!(
            "project-skill held_out_validation_status must be one of: {}",
            allowed_held_out_status.join(", ")
        )
        .into());
    }

    if matches!(maturity, "project" | "published") && held_out_validation_status != "passed" {
        return Err("project/published skills require held_out_validation_status=passed".into());
    }
    if maturity == "project"
        && !matches!(
            verification_status,
            "held-out-validated" | "published" | "deprecated"
        )
    {
        return Err(
            "project skills require verification_status=held-out-validated (or stronger)".into(),
        );
    }
    if maturity == "published" && verification_status != "published" {
        return Err("published skills require verification_status=published".into());
    }

    Ok(())
}

fn project_skill_warnings_and_remediation(
    value: &serde_json::Value,
    skill_root: &Path,
) -> (Vec<String>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut remediation = Vec::new();

    let maturity = metadata_string(value, "maturity_level").unwrap_or("unknown");
    let verification_status = metadata_string(value, "verification_status").unwrap_or("unknown");
    let held_out_validation_status =
        metadata_string(value, "held_out_validation_status").unwrap_or("unknown");

    if maturity == "draft" {
        warnings.push("skill is still at draft maturity".to_string());
        remediation.push(format!(
            "promote after validation: claw project-skill promote {} --to project --held-out-validation passed",
            skill_root.display()
        ));
    }

    if verification_status == "drafted" {
        warnings.push("verification_status is still drafted".to_string());
        remediation.push(
            "run at least one realistic example, then update verification via project-skill promote"
                .to_string(),
        );
    } else if verification_status == "example-validated" && held_out_validation_status != "passed" {
        warnings.push("held-out validation has not passed yet".to_string());
        remediation.push(
            "run a held-out example and then promote with --held-out-validation passed".to_string(),
        );
    }

    if held_out_validation_status == "pending" {
        warnings.push("held_out_validation_status is pending".to_string());
        remediation.push(
            "record the outcome of a held-out evaluation before treating this as reusable beyond the source material".to_string(),
        );
    } else if held_out_validation_status == "failed" {
        warnings.push("held_out_validation_status is failed".to_string());
        remediation.push(
            "revise the workflow and re-run held-out validation before promotion".to_string(),
        );
    }

    if let Some(sources) = metadata_array(value, "source_materials") {
        let missing_sources = sources
            .iter()
            .filter(|entry| entry.get("exists").and_then(|flag| flag.as_bool()) == Some(false))
            .count();
        if missing_sources > 0 {
            warnings.push(format!(
                "{missing_sources} source material reference(s) are unresolved"
            ));
            remediation.push(
                "replace missing references with resolvable local files or add verified source provenance"
                    .to_string(),
            );
        }
    }

    if let Some(examples) = metadata_array(value, "evaluation_examples") {
        let pending_examples = examples
            .iter()
            .filter_map(|entry| entry.as_str())
            .filter(|item| item.to_ascii_lowercase().contains("pending validation"))
            .count();
        if pending_examples > 0 {
            warnings.push(format!(
                "{pending_examples} evaluation example(s) still contain pending placeholders"
            ));
            remediation.push(
                "replace placeholder evaluation examples with concrete project and held-out runs"
                    .to_string(),
            );
        }
    }

    if let Some(checks) = metadata_array(value, "failure_checks") {
        let weak_checks = checks
            .iter()
            .filter_map(|entry| entry.as_str())
            .filter(|item| item.to_ascii_lowercase().contains("unclear"))
            .count();
        if weak_checks > 0 {
            warnings.push("failure checks still contain generic placeholder language".to_string());
            remediation.push(
                "rewrite failure checks so they mention concrete stop conditions and review triggers"
                    .to_string(),
            );
        }
    }

    (warnings, remediation)
}

fn project_skill_doctor_diagnostics(value: &serde_json::Value) -> Vec<(String, String)> {
    let mut diagnostics = Vec::new();

    let maturity = metadata_string(value, "maturity_level").unwrap_or("unknown");
    let verification_status = metadata_string(value, "verification_status").unwrap_or("unknown");
    let held_out_validation_status =
        metadata_string(value, "held_out_validation_status").unwrap_or("unknown");

    if maturity == "draft" && verification_status == "drafted" {
        diagnostics.push((
            "info".to_string(),
            "draft skill has not yet recorded a realistic validation run".to_string(),
        ));
    }
    if held_out_validation_status == "pending" {
        diagnostics.push((
            "warn".to_string(),
            "held-out validation is still pending; reuse outside the source material may be risky"
                .to_string(),
        ));
    }
    if held_out_validation_status == "failed" {
        diagnostics.push((
            "warn".to_string(),
            "held-out validation failed; revise the workflow before promotion".to_string(),
        ));
    }

    if let Some(sources) = metadata_array(value, "source_materials") {
        let unresolved = sources
            .iter()
            .filter(|entry| entry.get("exists").and_then(|flag| flag.as_bool()) == Some(false))
            .count();
        if unresolved > 0 {
            diagnostics.push((
                "warn".to_string(),
                format!("{unresolved} source material reference(s) cannot be resolved locally"),
            ));
        }
    }

    if let Some(examples) = metadata_array(value, "evaluation_examples") {
        let placeholders = examples
            .iter()
            .filter_map(|entry| entry.as_str())
            .filter(|item| item.to_ascii_lowercase().contains("pending validation"))
            .count();
        if placeholders > 0 {
            diagnostics.push((
                "warn".to_string(),
                format!("{placeholders} evaluation example(s) still use placeholder text"),
            ));
        }
    }

    if let Some(outputs) = metadata_array(value, "outputs") {
        let generic_outputs = outputs
            .iter()
            .filter_map(|entry| entry.as_str())
            .filter(|item| item.eq_ignore_ascii_case("SKILL.md draft"))
            .count();
        if generic_outputs > 0 {
            diagnostics.push((
                "info".to_string(),
                "outputs still look scaffold-level; add concrete artifacts if the workflow produces them"
                    .to_string(),
            ));
        }
    }

    diagnostics
}

fn validate_project_skill_report(
    path: &Path,
) -> Result<ProjectSkillCommandReport, Box<dyn std::error::Error>> {
    let skill_root = resolve_project_skill_root(path)?;
    ensure_project_skill_files(&skill_root)?;
    let value = read_project_skill_metadata(&skill_root)?;
    validate_project_skill_metadata(&value)?;
    let (warnings, remediation) = project_skill_warnings_and_remediation(&value, &skill_root);

    let maturity = metadata_string(&value, "maturity_level").unwrap_or("unknown");
    let verification_status = metadata_string(&value, "verification_status").unwrap_or("unknown");
    let held_out_validation_status =
        metadata_string(&value, "held_out_validation_status").unwrap_or("unknown");

    let mut lines = vec![
        "Project skill validation".to_string(),
        "  Result           valid".to_string(),
        format!("  Skill root       {}", skill_root.display()),
        format!("  Maturity         {maturity}"),
        format!("  Verification     {verification_status}"),
        format!("  Held-out         {held_out_validation_status}"),
    ];

    if warnings.is_empty() {
        lines.push("  Warnings         none".to_string());
    } else {
        lines.push(format!("  Warnings         {}", warnings.len()));
        lines.push("Warnings".to_string());
        for warning in &warnings {
            lines.push(format!("  - {warning}"));
        }
    }

    if remediation.is_empty() {
        lines.push("Remediation".to_string());
        lines.push("  - no action required".to_string());
    } else {
        lines.push("Remediation".to_string());
        for item in &remediation {
            lines.push(format!("  - {item}"));
        }
    }

    Ok(ProjectSkillCommandReport {
        text: lines.join("\n"),
        json: json!({
            "command": "project-skill.validate",
            "result": "valid",
            "skill_root": skill_root.display().to_string(),
            "maturity": maturity,
            "verification_status": verification_status,
            "held_out_validation_status": held_out_validation_status,
            "warnings": warnings,
            "remediation": remediation,
        }),
    })
}

fn doctor_project_skill(
    path: &Path,
) -> Result<ProjectSkillCommandReport, Box<dyn std::error::Error>> {
    let skill_root = resolve_project_skill_root(path)?;
    ensure_project_skill_files(&skill_root)?;
    let value = read_project_skill_metadata(&skill_root)?;
    validate_project_skill_metadata(&value)?;

    let (warnings, remediation) = project_skill_warnings_and_remediation(&value, &skill_root);
    let diagnostics = project_skill_doctor_diagnostics(&value);

    let mut lines = vec![
        "Project skill doctor".to_string(),
        format!("  Skill root       {}", skill_root.display()),
        format!(
            "  Maturity         {}",
            metadata_string(&value, "maturity_level").unwrap_or("unknown")
        ),
        format!(
            "  Verification     {}",
            metadata_string(&value, "verification_status").unwrap_or("unknown")
        ),
        format!(
            "  Held-out         {}",
            metadata_string(&value, "held_out_validation_status").unwrap_or("unknown")
        ),
        format!("  Diagnostics      {}", diagnostics.len()),
    ];

    lines.push("Findings".to_string());
    if diagnostics.is_empty() {
        lines.push("  - no additional non-blocking issues detected".to_string());
    } else {
        for (severity, message) in &diagnostics {
            lines.push(format!("  - [{severity}] {message}"));
        }
    }

    lines.push("Warnings".to_string());
    if warnings.is_empty() {
        lines.push("  - none".to_string());
    } else {
        for warning in &warnings {
            lines.push(format!("  - {warning}"));
        }
    }

    lines.push("Remediation".to_string());
    if remediation.is_empty() {
        lines.push("  - no action required".to_string());
    } else {
        for item in &remediation {
            lines.push(format!("  - {item}"));
        }
    }

    Ok(ProjectSkillCommandReport {
        text: lines.join("\n"),
        json: json!({
            "command": "project-skill.doctor",
            "skill_root": skill_root.display().to_string(),
            "maturity": metadata_string(&value, "maturity_level").unwrap_or("unknown"),
            "verification_status": metadata_string(&value, "verification_status").unwrap_or("unknown"),
            "held_out_validation_status": metadata_string(&value, "held_out_validation_status").unwrap_or("unknown"),
            "diagnostics": diagnostics.iter().map(|(severity, message)| {
                json!({"severity": severity, "message": message})
            }).collect::<Vec<_>>(),
            "warnings": warnings,
            "remediation": remediation,
        }),
    })
}

fn promote_project_skill(
    path: &Path,
    to: &str,
    verification_status: Option<&str>,
    held_out_validation_status: Option<&str>,
) -> Result<ProjectSkillCommandReport, Box<dyn std::error::Error>> {
    let skill_root = resolve_project_skill_root(path)?;
    ensure_project_skill_files(&skill_root)?;
    let mut value = read_project_skill_metadata(&skill_root)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "project-skill metadata must be a JSON object".to_string())?;

    object.insert(
        "maturity_level".to_string(),
        serde_json::Value::String(to.to_string()),
    );

    let effective_verification_status =
        verification_status
            .map(str::to_string)
            .unwrap_or_else(|| match to {
                "project" => "held-out-validated".to_string(),
                "published" => "published".to_string(),
                "deprecated" => "deprecated".to_string(),
                _ => object
                    .get("verification_status")
                    .and_then(|entry| entry.as_str())
                    .unwrap_or("drafted")
                    .to_string(),
            });
    object.insert(
        "verification_status".to_string(),
        serde_json::Value::String(effective_verification_status.clone()),
    );

    if let Some(status) = held_out_validation_status {
        object.insert(
            "held_out_validation_status".to_string(),
            serde_json::Value::String(status.to_string()),
        );
    }

    validate_project_skill_metadata(&value)?;
    let metadata_path = skill_root.join("skill.json");
    fs::write(&metadata_path, serde_json::to_string_pretty(&value)? + "\n")?;
    rewrite_project_skill_docs(&skill_root, &value)?;

    let held_out = value
        .get("held_out_validation_status")
        .and_then(|entry| entry.as_str())
        .unwrap_or("pending");
    Ok(ProjectSkillCommandReport {
        text: format!(
            "Project skill promotion\n  Result           promoted\n  Skill root       {}\n  Maturity         {}\n  Verification     {}\n  Held-out         {}",
            skill_root.display(),
            to,
            effective_verification_status,
            held_out
        ),
        json: json!({
            "command": "project-skill.promote",
            "result": "promoted",
            "skill_root": skill_root.display().to_string(),
            "maturity": to,
            "verification_status": effective_verification_status,
            "held_out_validation_status": held_out,
        }),
    })
}

fn find_project_skill_scaffold_script(cwd: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    for ancestor in cwd.ancestors() {
        let candidate = ancestor.join("tools/scaffold_project_skill.py");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let fallback =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tools/scaffold_project_skill.py");
    if fallback.is_file() {
        return Ok(fallback);
    }

    Err("could not locate tools/scaffold_project_skill.py".into())
}

fn normalize_permission_mode(mode: &str) -> Option<&'static str> {
    match mode.trim() {
        "read-only" => Some("read-only"),
        "workspace-write" => Some("workspace-write"),
        "danger-full-access" => Some("danger-full-access"),
        _ => None,
    }
}

fn render_diff_report() -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--", ":(exclude).omx"])
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git diff failed: {stderr}").into());
    }
    let diff = String::from_utf8(output.stdout)?;
    if diff.trim().is_empty() {
        return Ok(
            "Diff\n  Result           clean working tree\n  Detail           no current changes"
                .to_string(),
        );
    }
    Ok(format!("Diff\n\n{}", diff.trim_end()))
}

fn render_teleport_report(target: &str) -> Result<String, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;

    let file_list = Command::new("rg")
        .args(["--files"])
        .current_dir(&cwd)
        .output()?;
    let file_matches = if file_list.status.success() {
        String::from_utf8(file_list.stdout)?
            .lines()
            .filter(|line| line.contains(target))
            .take(10)
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let content_output = Command::new("rg")
        .args(["-n", "-S", "--color", "never", target, "."])
        .current_dir(&cwd)
        .output()?;

    let mut lines = vec![format!("Teleport\n  Target           {target}")];
    if !file_matches.is_empty() {
        lines.push(String::new());
        lines.push("File matches".to_string());
        lines.extend(file_matches.into_iter().map(|path| format!("  {path}")));
    }

    if content_output.status.success() {
        let matches = String::from_utf8(content_output.stdout)?;
        if !matches.trim().is_empty() {
            lines.push(String::new());
            lines.push("Content matches".to_string());
            lines.push(truncate_for_prompt(&matches, 4_000));
        }
    }

    if lines.len() == 1 {
        lines.push("  Result           no matches found".to_string());
    }

    Ok(lines.join("\n"))
}

fn render_last_tool_debug_report(session: &Session) -> Result<String, Box<dyn std::error::Error>> {
    let last_tool_use = session
        .messages
        .iter()
        .rev()
        .find_map(|message| {
            message.blocks.iter().rev().find_map(|block| match block {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
        })
        .ok_or_else(|| "no prior tool call found in session".to_string())?;

    let tool_result = session.messages.iter().rev().find_map(|message| {
        message.blocks.iter().rev().find_map(|block| match block {
            ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } if tool_use_id == &last_tool_use.0 => {
                Some((tool_name.clone(), output.clone(), *is_error))
            }
            _ => None,
        })
    });

    let mut lines = vec![
        "Debug tool call".to_string(),
        format!("  Tool id          {}", last_tool_use.0),
        format!("  Tool name        {}", last_tool_use.1),
        "  Input".to_string(),
        indent_block(&last_tool_use.2, 4),
    ];

    match tool_result {
        Some((tool_name, output, is_error)) => {
            lines.push("  Result".to_string());
            lines.push(format!("    name           {tool_name}"));
            lines.push(format!(
                "    status         {}",
                if is_error { "error" } else { "ok" }
            ));
            lines.push(indent_block(&output, 4));
        }
        None => lines.push("  Result           missing tool result".to_string()),
    }

    Ok(lines.join("\n"))
}

fn indent_block(value: &str, spaces: usize) -> String {
    let indent = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn git_output(args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn git_status_ok(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(env::current_dir()?)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} failed: {stderr}", args.join(" ")).into());
    }
    Ok(())
}

fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn write_temp_text_file(
    filename: &str,
    contents: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = env::temp_dir().join(filename);
    fs::write(&path, contents)?;
    Ok(path)
}

fn recent_user_context(session: &Session, limit: usize) -> String {
    let requests = session
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .filter_map(|message| {
            message.blocks.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.trim().to_string()),
                _ => None,
            })
        })
        .rev()
        .take(limit)
        .collect::<Vec<_>>();

    if requests.is_empty() {
        "<no prior user messages>".to_string()
    } else {
        requests
            .into_iter()
            .rev()
            .enumerate()
            .map(|(index, text)| format!("{}. {}", index + 1, text))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn truncate_for_prompt(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.trim().to_string()
    } else {
        let truncated = value.chars().take(limit).collect::<String>();
        format!("{}\n…[truncated]", truncated.trim_end())
    }
}

fn sanitize_generated_message(value: &str) -> String {
    value.trim().trim_matches('`').trim().replace("\r\n", "\n")
}

fn parse_titled_body(value: &str) -> Option<(String, String)> {
    let normalized = sanitize_generated_message(value);
    let title = normalized
        .lines()
        .find_map(|line| line.strip_prefix("TITLE:").map(str::trim))?;
    let body_start = normalized.find("BODY:")?;
    let body = normalized[body_start + "BODY:".len()..].trim();
    Some((title.to_string(), body.to_string()))
}

fn render_version_report() -> String {
    let git_sha = GIT_SHA.unwrap_or("unknown");
    let target = BUILD_TARGET.unwrap_or("unknown");
    let cli_name = current_cli_name();
    format!(
        "{cli_name}\n  Version          {VERSION}\n  Git SHA          {git_sha}\n  Target           {target}\n  Build date       {DEFAULT_DATE}"
    )
}

fn render_export_text(session: &Session) -> String {
    let mut lines = vec!["# Conversation Export".to_string(), String::new()];
    for (index, message) in session.messages.iter().enumerate() {
        let role = match message.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        };
        lines.push(format!("## {}. {role}", index + 1));
        for block in &message.blocks {
            match block {
                ContentBlock::Text { text } => lines.push(text.clone()),
                ContentBlock::ToolUse { id, name, input } => {
                    lines.push(format!("[tool_use id={id} name={name}] {input}"));
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    tool_name,
                    output,
                    is_error,
                } => {
                    lines.push(format!(
                        "[tool_result id={tool_use_id} name={tool_name} error={is_error}] {output}"
                    ));
                }
            }
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

fn default_export_filename(session: &Session) -> String {
    let stem = session
        .messages
        .iter()
        .find_map(|message| match message.role {
            MessageRole::User => message.blocks.iter().find_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            }),
            _ => None,
        })
        .map_or("conversation", |text| {
            text.lines().next().unwrap_or("conversation")
        })
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(8)
        .collect::<Vec<_>>()
        .join("-");
    let fallback = if stem.is_empty() {
        "conversation"
    } else {
        &stem
    };
    format!("{fallback}.txt")
}

fn resolve_export_path(
    requested_path: Option<&str>,
    session: &Session,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let file_name =
        requested_path.map_or_else(|| default_export_filename(session), ToOwned::to_owned);
    let final_name = if Path::new(&file_name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("txt"))
    {
        file_name
    } else {
        format!("{file_name}.txt")
    };
    Ok(cwd.join(final_name))
}

fn build_system_prompt() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(load_system_prompt(
        env::current_dir()?,
        DEFAULT_DATE,
        env::consts::OS,
        "unknown",
    )?)
}

fn build_runtime_plugin_state(
) -> Result<(RuntimeConfig, RuntimeFeatureConfig, GlobalToolRegistry), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let loader = ConfigLoader::default_for(&cwd);
    let runtime_config = loader.load()?;
    let plugin_manager = build_plugin_manager(&cwd, &loader, &runtime_config);
    let tool_registry = GlobalToolRegistry::with_plugin_tools(plugin_manager.aggregated_tools()?)?;
    Ok((
        runtime_config.clone(),
        runtime_config.feature_config().clone(),
        tool_registry,
    ))
}

fn build_plugin_manager(
    cwd: &Path,
    loader: &ConfigLoader,
    runtime_config: &runtime::RuntimeConfig,
) -> PluginManager {
    let plugin_settings = runtime_config.plugins();
    let mut plugin_config = PluginManagerConfig::new(loader.config_home().to_path_buf());
    plugin_config.enabled_plugins = plugin_settings.enabled_plugins().clone();
    plugin_config.external_dirs = plugin_settings
        .external_directories()
        .iter()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path))
        .collect();
    plugin_config.install_root = plugin_settings
        .install_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.registry_path = plugin_settings
        .registry_path()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    plugin_config.bundled_root = plugin_settings
        .bundled_root()
        .map(|path| resolve_plugin_path(cwd, loader.config_home(), path));
    PluginManager::new(plugin_config)
}

fn resolve_plugin_path(cwd: &Path, config_home: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else if value.starts_with('.') {
        cwd.join(path)
    } else {
        config_home.join(path)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedClientSelection {
    model: String,
    provider_id: Option<String>,
    provider_profile: Option<RuntimeProviderProfile>,
}

fn resolve_client_selection(
    runtime_config: &RuntimeConfig,
    model: Option<String>,
    provider: Option<String>,
) -> Result<ResolvedClientSelection, Box<dyn std::error::Error>> {
    let provider_id = provider.or_else(|| {
        runtime_config
            .providers()
            .default_profile()
            .map(ToOwned::to_owned)
    });
    let provider_profile = match provider_id.as_deref() {
        Some(provider_id) => Some(
            runtime_config
                .providers()
                .profile(provider_id)
                .ok_or_else(|| format!("unknown provider profile: {provider_id}"))?
                .clone(),
        ),
        None => None,
    };

    let model = model
        .or_else(|| {
            provider_profile
                .as_ref()
                .and_then(|profile| profile.default_model().map(ToOwned::to_owned))
        })
        .or_else(|| runtime_config.model().map(ToOwned::to_owned))
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    Ok(ResolvedClientSelection {
        model: resolve_model_alias(&model),
        provider_id,
        provider_profile,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternalPromptProgressState {
    command_label: &'static str,
    task_label: String,
    step: usize,
    phase: String,
    detail: Option<String>,
    saw_final_text: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InternalPromptProgressEvent {
    Started,
    Update,
    Heartbeat,
    Complete,
    Failed,
}

#[derive(Debug)]
struct InternalPromptProgressShared {
    state: Mutex<InternalPromptProgressState>,
    output_lock: Mutex<()>,
    started_at: Instant,
}

#[derive(Debug, Clone)]
struct InternalPromptProgressReporter {
    shared: Arc<InternalPromptProgressShared>,
}

#[derive(Debug)]
struct InternalPromptProgressRun {
    reporter: InternalPromptProgressReporter,
    heartbeat_stop: Option<mpsc::Sender<()>>,
    heartbeat_handle: Option<thread::JoinHandle<()>>,
}

impl InternalPromptProgressReporter {
    fn ultraplan(task: &str) -> Self {
        Self {
            shared: Arc::new(InternalPromptProgressShared {
                state: Mutex::new(InternalPromptProgressState {
                    command_label: "Ultraplan",
                    task_label: task.to_string(),
                    step: 0,
                    phase: "planning started".to_string(),
                    detail: Some(format!("task: {task}")),
                    saw_final_text: false,
                }),
                output_lock: Mutex::new(()),
                started_at: Instant::now(),
            }),
        }
    }

    fn emit(&self, event: InternalPromptProgressEvent, error: Option<&str>) {
        let snapshot = self.snapshot();
        let line = format_internal_prompt_progress_line(event, &snapshot, self.elapsed(), error);
        self.write_line(&line);
    }

    fn mark_model_phase(&self) {
        let snapshot = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("internal prompt progress state poisoned");
            state.step += 1;
            state.phase = if state.step == 1 {
                "analyzing request".to_string()
            } else {
                "reviewing findings".to_string()
            };
            state.detail = Some(format!("task: {}", state.task_label));
            state.clone()
        };
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Update,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    fn mark_tool_phase(&self, name: &str, input: &str) {
        let detail = describe_tool_progress(name, input);
        let snapshot = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("internal prompt progress state poisoned");
            state.step += 1;
            state.phase = format!("running {name}");
            state.detail = Some(detail);
            state.clone()
        };
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Update,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    fn mark_text_phase(&self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        let detail = truncate_for_summary(first_visible_line(trimmed), 120);
        let snapshot = {
            let mut state = self
                .shared
                .state
                .lock()
                .expect("internal prompt progress state poisoned");
            if state.saw_final_text {
                return;
            }
            state.saw_final_text = true;
            state.step += 1;
            state.phase = "drafting final plan".to_string();
            state.detail = (!detail.is_empty()).then_some(detail);
            state.clone()
        };
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Update,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    fn emit_heartbeat(&self) {
        let snapshot = self.snapshot();
        self.write_line(&format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Heartbeat,
            &snapshot,
            self.elapsed(),
            None,
        ));
    }

    fn snapshot(&self) -> InternalPromptProgressState {
        self.shared
            .state
            .lock()
            .expect("internal prompt progress state poisoned")
            .clone()
    }

    fn elapsed(&self) -> Duration {
        self.shared.started_at.elapsed()
    }

    fn write_line(&self, line: &str) {
        let _guard = self
            .shared
            .output_lock
            .lock()
            .expect("internal prompt progress output lock poisoned");
        let mut stdout = io::stdout();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }
}

impl InternalPromptProgressRun {
    fn start_ultraplan(task: &str) -> Self {
        let reporter = InternalPromptProgressReporter::ultraplan(task);
        reporter.emit(InternalPromptProgressEvent::Started, None);

        let (heartbeat_stop, heartbeat_rx) = mpsc::channel();
        let heartbeat_reporter = reporter.clone();
        let heartbeat_handle = thread::spawn(move || loop {
            match heartbeat_rx.recv_timeout(INTERNAL_PROGRESS_HEARTBEAT_INTERVAL) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => heartbeat_reporter.emit_heartbeat(),
            }
        });

        Self {
            reporter,
            heartbeat_stop: Some(heartbeat_stop),
            heartbeat_handle: Some(heartbeat_handle),
        }
    }

    fn reporter(&self) -> InternalPromptProgressReporter {
        self.reporter.clone()
    }

    fn finish_success(&mut self) {
        self.stop_heartbeat();
        self.reporter
            .emit(InternalPromptProgressEvent::Complete, None);
    }

    fn finish_failure(&mut self, error: &str) {
        self.stop_heartbeat();
        self.reporter
            .emit(InternalPromptProgressEvent::Failed, Some(error));
    }

    fn stop_heartbeat(&mut self) {
        if let Some(sender) = self.heartbeat_stop.take() {
            let _ = sender.send(());
        }
        if let Some(handle) = self.heartbeat_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for InternalPromptProgressRun {
    fn drop(&mut self) {
        self.stop_heartbeat();
    }
}

fn format_internal_prompt_progress_line(
    event: InternalPromptProgressEvent,
    snapshot: &InternalPromptProgressState,
    elapsed: Duration,
    error: Option<&str>,
) -> String {
    let elapsed_seconds = elapsed.as_secs();
    let step_label = if snapshot.step == 0 {
        "current step pending".to_string()
    } else {
        format!("current step {}", snapshot.step)
    };
    let mut status_bits = vec![step_label, format!("phase {}", snapshot.phase)];
    if let Some(detail) = snapshot
        .detail
        .as_deref()
        .filter(|detail| !detail.is_empty())
    {
        status_bits.push(detail.to_string());
    }
    let status = status_bits.join(" · ");
    match event {
        InternalPromptProgressEvent::Started => {
            format!(
                "🧭 {} status · planning started · {status}",
                snapshot.command_label
            )
        }
        InternalPromptProgressEvent::Update => {
            format!("… {} status · {status}", snapshot.command_label)
        }
        InternalPromptProgressEvent::Heartbeat => format!(
            "… {} heartbeat · {elapsed_seconds}s elapsed · {status}",
            snapshot.command_label
        ),
        InternalPromptProgressEvent::Complete => format!(
            "✔ {} status · completed · {elapsed_seconds}s elapsed · {} steps total",
            snapshot.command_label, snapshot.step
        ),
        InternalPromptProgressEvent::Failed => format!(
            "✘ {} status · failed · {elapsed_seconds}s elapsed · {}",
            snapshot.command_label,
            error.unwrap_or("unknown error")
        ),
    }
}

fn describe_tool_progress(name: &str, input: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(input).unwrap_or(serde_json::Value::String(input.to_string()));
    match name {
        "bash" | "Bash" => {
            let command = parsed
                .get("command")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if command.is_empty() {
                "running shell command".to_string()
            } else {
                format!("command {}", truncate_for_summary(command.trim(), 100))
            }
        }
        "read_file" | "Read" => format!("reading {}", extract_tool_path(&parsed)),
        "write_file" | "Write" => format!("writing {}", extract_tool_path(&parsed)),
        "edit_file" | "Edit" => format!("editing {}", extract_tool_path(&parsed)),
        "glob_search" | "Glob" => {
            let pattern = parsed
                .get("pattern")
                .and_then(|value| value.as_str())
                .unwrap_or("?");
            let scope = parsed
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            format!("glob `{pattern}` in {scope}")
        }
        "grep_search" | "Grep" => {
            let pattern = parsed
                .get("pattern")
                .and_then(|value| value.as_str())
                .unwrap_or("?");
            let scope = parsed
                .get("path")
                .and_then(|value| value.as_str())
                .unwrap_or(".");
            format!("grep `{pattern}` in {scope}")
        }
        "web_search" | "WebSearch" => parsed
            .get("query")
            .and_then(|value| value.as_str())
            .map_or_else(
                || "running web search".to_string(),
                |query| format!("query {}", truncate_for_summary(query, 100)),
            ),
        _ => {
            let summary = summarize_tool_payload(input);
            if summary.is_empty() {
                format!("running {name}")
            } else {
                format!("{name}: {summary}")
            }
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_arguments)]
fn build_runtime(
    session: Session,
    selection: ResolvedClientSelection,
    system_prompt: Vec<String>,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    permission_mode: PermissionMode,
    progress_reporter: Option<InternalPromptProgressReporter>,
    feature_config: RuntimeFeatureConfig,
    tool_registry: GlobalToolRegistry,
) -> Result<ConversationRuntime<DefaultRuntimeClient, CliToolExecutor>, Box<dyn std::error::Error>>
{
    Ok(ConversationRuntime::new_with_features(
        session,
        DefaultRuntimeClient::new(
            selection,
            enable_tools,
            emit_output,
            allowed_tools.clone(),
            tool_registry.clone(),
            progress_reporter,
        )?,
        CliToolExecutor::new(allowed_tools.clone(), emit_output, tool_registry.clone()),
        permission_policy(permission_mode, &tool_registry),
        system_prompt,
        feature_config,
    ))
}

struct CliPermissionPrompter {
    current_mode: PermissionMode,
}

impl CliPermissionPrompter {
    fn new(current_mode: PermissionMode) -> Self {
        Self { current_mode }
    }
}

impl runtime::PermissionPrompter for CliPermissionPrompter {
    fn decide(
        &mut self,
        request: &runtime::PermissionRequest,
    ) -> runtime::PermissionPromptDecision {
        println!();
        println!("Permission approval required");
        println!("  Tool             {}", request.tool_name);
        println!("  Current mode     {}", self.current_mode.as_str());
        println!("  Required mode    {}", request.required_mode.as_str());
        println!("  Input            {}", request.input);
        print!("Approve this tool call? [y/N]: ");
        let _ = io::stdout().flush();

        let mut response = String::new();
        match io::stdin().read_line(&mut response) {
            Ok(_) => {
                let normalized = response.trim().to_ascii_lowercase();
                if matches!(normalized.as_str(), "y" | "yes") {
                    runtime::PermissionPromptDecision::Allow
                } else {
                    runtime::PermissionPromptDecision::Deny {
                        reason: format!(
                            "tool '{}' denied by user approval prompt",
                            request.tool_name
                        ),
                    }
                }
            }
            Err(error) => runtime::PermissionPromptDecision::Deny {
                reason: format!("permission approval failed: {error}"),
            },
        }
    }
}

struct DefaultRuntimeClient {
    runtime: tokio::runtime::Runtime,
    client: ProviderClient,
    model: String,
    enable_tools: bool,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    tool_registry: GlobalToolRegistry,
    progress_reporter: Option<InternalPromptProgressReporter>,
}

impl DefaultRuntimeClient {
    fn new(
        selection: ResolvedClientSelection,
        enable_tools: bool,
        emit_output: bool,
        allowed_tools: Option<AllowedToolSet>,
        tool_registry: GlobalToolRegistry,
        progress_reporter: Option<InternalPromptProgressReporter>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let model = selection.model.clone();
        let client = match selection.provider_profile {
            Some(profile) => {
                build_profile_provider_client(selection.provider_id.as_deref(), &profile)?
            }
            None => ProviderClient::from_model_with_default_auth(
                &model,
                Some(resolve_cli_auth_source()?),
            )?,
        };
        Ok(Self {
            runtime: tokio::runtime::Runtime::new()?,
            client,
            model,
            enable_tools,
            emit_output,
            allowed_tools,
            tool_registry,
            progress_reporter,
        })
    }
}

fn build_profile_provider_client(
    provider_id: Option<&str>,
    profile: &RuntimeProviderProfile,
) -> Result<ProviderClient, Box<dyn std::error::Error>> {
    match profile.transport() {
        RuntimeProviderTransport::OpenAiCompat => {
            let config = OpenAiCompatConfig::custom(
                profile.provider_name(),
                profile.api_key_env(),
                profile.base_url_env().map_or_else(
                    || {
                        provider_id.map_or_else(
                            || "OPENAI_COMPAT_BASE_URL".to_string(),
                            |provider_id| {
                                format!(
                                    "{}_BASE_URL",
                                    provider_id.to_ascii_uppercase().replace('-', "_")
                                )
                            },
                        )
                    },
                    ToOwned::to_owned,
                ),
                profile.base_url(),
            );
            Ok(ProviderClient::from_openai_compat_config(config)?)
        }
    }
}

fn resolve_cli_auth_source() -> Result<AuthSource, Box<dyn std::error::Error>> {
    Ok(resolve_startup_auth_source(|| {
        let cwd = env::current_dir().map_err(api::ApiError::from)?;
        let config = ConfigLoader::default_for(&cwd).load().map_err(|error| {
            api::ApiError::Auth(format!("failed to load runtime OAuth config: {error}"))
        })?;
        Ok(config.oauth().cloned())
    })?)
}

impl ApiClient for DefaultRuntimeClient {
    #[allow(clippy::too_many_lines)]
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        if let Some(progress_reporter) = &self.progress_reporter {
            progress_reporter.mark_model_phase();
        }
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages: convert_messages(&request.messages),
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: self
                .enable_tools
                .then(|| filter_tool_specs(&self.tool_registry, self.allowed_tools.as_ref())),
            tool_choice: self.enable_tools.then_some(ToolChoice::Auto),
            stream: true,
        };

        self.runtime.block_on(async {
            let mut stream = self
                .client
                .stream_message(&message_request)
                .await
                .map_err(|error| RuntimeError::new(error.to_string()))?;
            let mut stdout = io::stdout();
            let mut sink = io::sink();
            let out: &mut dyn Write = if self.emit_output {
                &mut stdout
            } else {
                &mut sink
            };
            let renderer = TerminalRenderer::new();
            let mut markdown_stream = MarkdownStreamState::default();
            let mut events = Vec::new();
            let mut pending_tool: Option<(String, String, String)> = None;
            let mut saw_stop = false;

            while let Some(event) = stream
                .next_event()
                .await
                .map_err(|error| RuntimeError::new(error.to_string()))?
            {
                match event {
                    ApiStreamEvent::MessageStart(start) => {
                        for block in start.message.content {
                            push_output_block(block, out, &mut events, &mut pending_tool, true)?;
                        }
                    }
                    ApiStreamEvent::ContentBlockStart(start) => {
                        push_output_block(
                            start.content_block,
                            out,
                            &mut events,
                            &mut pending_tool,
                            true,
                        )?;
                    }
                    ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                        ContentBlockDelta::TextDelta { text } => {
                            if !text.is_empty() {
                                if let Some(progress_reporter) = &self.progress_reporter {
                                    progress_reporter.mark_text_phase(&text);
                                }
                                if let Some(rendered) = markdown_stream.push(&renderer, &text) {
                                    write!(out, "{rendered}")
                                        .and_then(|()| out.flush())
                                        .map_err(|error| RuntimeError::new(error.to_string()))?;
                                }
                                events.push(AssistantEvent::TextDelta(text));
                            }
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            if let Some((_, _, input)) = &mut pending_tool {
                                input.push_str(&partial_json);
                            }
                        }
                        ContentBlockDelta::ThinkingDelta { .. }
                        | ContentBlockDelta::SignatureDelta { .. } => {}
                    },
                    ApiStreamEvent::ContentBlockStop(_) => {
                        if let Some(rendered) = markdown_stream.flush(&renderer) {
                            write!(out, "{rendered}")
                                .and_then(|()| out.flush())
                                .map_err(|error| RuntimeError::new(error.to_string()))?;
                        }
                        if let Some((id, name, input)) = pending_tool.take() {
                            if let Some(progress_reporter) = &self.progress_reporter {
                                progress_reporter.mark_tool_phase(&name, &input);
                            }
                            // Display tool call now that input is fully accumulated
                            writeln!(out, "\n{}", format_tool_call_start(&name, &input))
                                .and_then(|()| out.flush())
                                .map_err(|error| RuntimeError::new(error.to_string()))?;
                            events.push(AssistantEvent::ToolUse { id, name, input });
                        }
                    }
                    ApiStreamEvent::MessageDelta(delta) => {
                        events.push(AssistantEvent::Usage(TokenUsage {
                            input_tokens: delta.usage.input_tokens,
                            output_tokens: delta.usage.output_tokens,
                            cache_creation_input_tokens: 0,
                            cache_read_input_tokens: 0,
                        }));
                    }
                    ApiStreamEvent::MessageStop(_) => {
                        saw_stop = true;
                        if let Some(rendered) = markdown_stream.flush(&renderer) {
                            write!(out, "{rendered}")
                                .and_then(|()| out.flush())
                                .map_err(|error| RuntimeError::new(error.to_string()))?;
                        }
                        events.push(AssistantEvent::MessageStop);
                    }
                }
            }

            if !saw_stop
                && events.iter().any(|event| {
                    matches!(event, AssistantEvent::TextDelta(text) if !text.is_empty())
                        || matches!(event, AssistantEvent::ToolUse { .. })
                })
            {
                events.push(AssistantEvent::MessageStop);
            }

            if events
                .iter()
                .any(|event| matches!(event, AssistantEvent::MessageStop))
            {
                return Ok(events);
            }

            let response = self
                .client
                .send_message(&MessageRequest {
                    stream: false,
                    ..message_request.clone()
                })
                .await
                .map_err(|error| RuntimeError::new(error.to_string()))?;
            response_to_events(response, out)
        })
    }
}

fn final_assistant_text(summary: &runtime::TurnSummary) -> String {
    summary
        .assistant_messages
        .last()
        .map(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn collect_tool_uses(summary: &runtime::TurnSummary) -> Vec<serde_json::Value> {
    summary
        .assistant_messages
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, name, input } => Some(json!({
                "id": id,
                "name": name,
                "input": input,
            })),
            _ => None,
        })
        .collect()
}

fn collect_tool_results(summary: &runtime::TurnSummary) -> Vec<serde_json::Value> {
    summary
        .tool_results
        .iter()
        .flat_map(|message| message.blocks.iter())
        .filter_map(|block| match block {
            ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } => Some(json!({
                "tool_use_id": tool_use_id,
                "tool_name": tool_name,
                "output": output,
                "is_error": is_error,
            })),
            _ => None,
        })
        .collect()
}

fn slash_command_completion_candidates() -> Vec<String> {
    slash_command_specs()
        .iter()
        .flat_map(|spec| {
            std::iter::once(spec.name)
                .chain(spec.aliases.iter().copied())
                .map(|name| format!("/{name}"))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn format_tool_call_start(name: &str, input: &str) -> String {
    let parsed: serde_json::Value =
        serde_json::from_str(input).unwrap_or(serde_json::Value::String(input.to_string()));

    let detail = match name {
        "bash" | "Bash" => format_bash_call(&parsed),
        "read_file" | "Read" => {
            let path = extract_tool_path(&parsed);
            format!("\x1b[2m📄 Reading {path}…\x1b[0m")
        }
        "write_file" | "Write" => {
            let path = extract_tool_path(&parsed);
            let lines = parsed
                .get("content")
                .and_then(|value| value.as_str())
                .map_or(0, |content| content.lines().count());
            format!("\x1b[1;32m✏️ Writing {path}\x1b[0m \x1b[2m({lines} lines)\x1b[0m")
        }
        "edit_file" | "Edit" => {
            let path = extract_tool_path(&parsed);
            let old_value = parsed
                .get("old_string")
                .or_else(|| parsed.get("oldString"))
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let new_value = parsed
                .get("new_string")
                .or_else(|| parsed.get("newString"))
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            format!(
                "\x1b[1;33m📝 Editing {path}\x1b[0m{}",
                format_patch_preview(old_value, new_value)
                    .map(|preview| format!("\n{preview}"))
                    .unwrap_or_default()
            )
        }
        "glob_search" | "Glob" => format_search_start("🔎 Glob", &parsed),
        "grep_search" | "Grep" => format_search_start("🔎 Grep", &parsed),
        "web_search" | "WebSearch" => parsed
            .get("query")
            .and_then(|value| value.as_str())
            .unwrap_or("?")
            .to_string(),
        _ => summarize_tool_payload(input),
    };

    let border = "─".repeat(name.len() + 8);
    format!(
        "\x1b[38;5;245m╭─ \x1b[1;36m{name}\x1b[0;38;5;245m ─╮\x1b[0m\n\x1b[38;5;245m│\x1b[0m {detail}\n\x1b[38;5;245m╰{border}╯\x1b[0m"
    )
}

fn format_tool_result(name: &str, output: &str, is_error: bool) -> String {
    let icon = if is_error {
        "\x1b[1;31m✗\x1b[0m"
    } else {
        "\x1b[1;32m✓\x1b[0m"
    };
    if is_error {
        let summary = truncate_for_summary(output.trim(), 160);
        return if summary.is_empty() {
            format!("{icon} \x1b[38;5;245m{name}\x1b[0m")
        } else {
            format!("{icon} \x1b[38;5;245m{name}\x1b[0m\n\x1b[38;5;203m{summary}\x1b[0m")
        };
    }

    let parsed: serde_json::Value =
        serde_json::from_str(output).unwrap_or(serde_json::Value::String(output.to_string()));
    match name {
        "bash" | "Bash" => format_bash_result(icon, &parsed),
        "read_file" | "Read" => format_read_result(icon, &parsed),
        "write_file" | "Write" => format_write_result(icon, &parsed),
        "edit_file" | "Edit" => format_edit_result(icon, &parsed),
        "glob_search" | "Glob" => format_glob_result(icon, &parsed),
        "grep_search" | "Grep" => format_grep_result(icon, &parsed),
        _ => format_generic_tool_result(icon, name, &parsed),
    }
}

const DISPLAY_TRUNCATION_NOTICE: &str =
    "\x1b[2m… output truncated for display; full result preserved in session.\x1b[0m";
const READ_DISPLAY_MAX_LINES: usize = 80;
const READ_DISPLAY_MAX_CHARS: usize = 6_000;
const TOOL_OUTPUT_DISPLAY_MAX_LINES: usize = 60;
const TOOL_OUTPUT_DISPLAY_MAX_CHARS: usize = 4_000;

fn extract_tool_path(parsed: &serde_json::Value) -> String {
    parsed
        .get("file_path")
        .or_else(|| parsed.get("filePath"))
        .or_else(|| parsed.get("path"))
        .and_then(|value| value.as_str())
        .unwrap_or("?")
        .to_string()
}

fn format_search_start(label: &str, parsed: &serde_json::Value) -> String {
    let pattern = parsed
        .get("pattern")
        .and_then(|value| value.as_str())
        .unwrap_or("?");
    let scope = parsed
        .get("path")
        .and_then(|value| value.as_str())
        .unwrap_or(".");
    format!("{label} {pattern}\n\x1b[2min {scope}\x1b[0m")
}

fn format_patch_preview(old_value: &str, new_value: &str) -> Option<String> {
    if old_value.is_empty() && new_value.is_empty() {
        return None;
    }
    Some(format!(
        "\x1b[38;5;203m- {}\x1b[0m\n\x1b[38;5;70m+ {}\x1b[0m",
        truncate_for_summary(first_visible_line(old_value), 72),
        truncate_for_summary(first_visible_line(new_value), 72)
    ))
}

fn format_bash_call(parsed: &serde_json::Value) -> String {
    let command = parsed
        .get("command")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    if command.is_empty() {
        String::new()
    } else {
        format!(
            "\x1b[48;5;236;38;5;255m $ {} \x1b[0m",
            truncate_for_summary(command, 160)
        )
    }
}

fn first_visible_line(text: &str) -> &str {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(text)
}

fn format_bash_result(icon: &str, parsed: &serde_json::Value) -> String {
    let mut lines = vec![format!("{icon} \x1b[38;5;245mbash\x1b[0m")];
    if let Some(task_id) = parsed
        .get("backgroundTaskId")
        .and_then(|value| value.as_str())
    {
        write!(&mut lines[0], " backgrounded ({task_id})").expect("write to string");
    } else if let Some(status) = parsed
        .get("returnCodeInterpretation")
        .and_then(|value| value.as_str())
        .filter(|status| !status.is_empty())
    {
        write!(&mut lines[0], " {status}").expect("write to string");
    }

    if let Some(stdout) = parsed.get("stdout").and_then(|value| value.as_str()) {
        if !stdout.trim().is_empty() {
            lines.push(truncate_output_for_display(
                stdout,
                TOOL_OUTPUT_DISPLAY_MAX_LINES,
                TOOL_OUTPUT_DISPLAY_MAX_CHARS,
            ));
        }
    }
    if let Some(stderr) = parsed.get("stderr").and_then(|value| value.as_str()) {
        if !stderr.trim().is_empty() {
            lines.push(format!(
                "\x1b[38;5;203m{}\x1b[0m",
                truncate_output_for_display(
                    stderr,
                    TOOL_OUTPUT_DISPLAY_MAX_LINES,
                    TOOL_OUTPUT_DISPLAY_MAX_CHARS,
                )
            ));
        }
    }

    lines.join("\n\n")
}

fn format_read_result(icon: &str, parsed: &serde_json::Value) -> String {
    let file = parsed.get("file").unwrap_or(parsed);
    let path = extract_tool_path(file);
    let start_line = file
        .get("startLine")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1);
    let num_lines = file
        .get("numLines")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let total_lines = file
        .get("totalLines")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(num_lines);
    let content = file
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let end_line = start_line.saturating_add(num_lines.saturating_sub(1));

    format!(
        "{icon} \x1b[2m📄 Read {path} (lines {}-{} of {})\x1b[0m\n{}",
        start_line,
        end_line.max(start_line),
        total_lines,
        truncate_output_for_display(content, READ_DISPLAY_MAX_LINES, READ_DISPLAY_MAX_CHARS)
    )
}

fn format_write_result(icon: &str, parsed: &serde_json::Value) -> String {
    let path = extract_tool_path(parsed);
    let kind = parsed
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or("write");
    let line_count = parsed
        .get("content")
        .and_then(|value| value.as_str())
        .map_or(0, |content| content.lines().count());
    format!(
        "{icon} \x1b[1;32m✏️ {} {path}\x1b[0m \x1b[2m({line_count} lines)\x1b[0m",
        if kind == "create" { "Wrote" } else { "Updated" },
    )
}

fn format_structured_patch_preview(parsed: &serde_json::Value) -> Option<String> {
    let hunks = parsed.get("structuredPatch")?.as_array()?;
    let mut preview = Vec::new();
    for hunk in hunks.iter().take(2) {
        let lines = hunk.get("lines")?.as_array()?;
        for line in lines.iter().filter_map(|value| value.as_str()).take(6) {
            match line.chars().next() {
                Some('+') => preview.push(format!("\x1b[38;5;70m{line}\x1b[0m")),
                Some('-') => preview.push(format!("\x1b[38;5;203m{line}\x1b[0m")),
                _ => preview.push(line.to_string()),
            }
        }
    }
    if preview.is_empty() {
        None
    } else {
        Some(preview.join("\n"))
    }
}

fn format_edit_result(icon: &str, parsed: &serde_json::Value) -> String {
    let path = extract_tool_path(parsed);
    let suffix = if parsed
        .get("replaceAll")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        " (replace all)"
    } else {
        ""
    };
    let preview = format_structured_patch_preview(parsed).or_else(|| {
        let old_value = parsed
            .get("oldString")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let new_value = parsed
            .get("newString")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        format_patch_preview(old_value, new_value)
    });

    match preview {
        Some(preview) => format!("{icon} \x1b[1;33m📝 Edited {path}{suffix}\x1b[0m\n{preview}"),
        None => format!("{icon} \x1b[1;33m📝 Edited {path}{suffix}\x1b[0m"),
    }
}

fn format_glob_result(icon: &str, parsed: &serde_json::Value) -> String {
    let num_files = parsed
        .get("numFiles")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let filenames = parsed
        .get("filenames")
        .and_then(|value| value.as_array())
        .map(|files| {
            files
                .iter()
                .filter_map(|value| value.as_str())
                .take(8)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    if filenames.is_empty() {
        format!("{icon} \x1b[38;5;245mglob_search\x1b[0m matched {num_files} files")
    } else {
        format!("{icon} \x1b[38;5;245mglob_search\x1b[0m matched {num_files} files\n{filenames}")
    }
}

fn format_grep_result(icon: &str, parsed: &serde_json::Value) -> String {
    let num_matches = parsed
        .get("numMatches")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let num_files = parsed
        .get("numFiles")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let content = parsed
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let filenames = parsed
        .get("filenames")
        .and_then(|value| value.as_array())
        .map(|files| {
            files
                .iter()
                .filter_map(|value| value.as_str())
                .take(8)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let summary = format!(
        "{icon} \x1b[38;5;245mgrep_search\x1b[0m {num_matches} matches across {num_files} files"
    );
    if !content.trim().is_empty() {
        format!(
            "{summary}\n{}",
            truncate_output_for_display(
                content,
                TOOL_OUTPUT_DISPLAY_MAX_LINES,
                TOOL_OUTPUT_DISPLAY_MAX_CHARS,
            )
        )
    } else if !filenames.is_empty() {
        format!("{summary}\n{filenames}")
    } else {
        summary
    }
}

fn format_generic_tool_result(icon: &str, name: &str, parsed: &serde_json::Value) -> String {
    let rendered_output = match parsed {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Null => String::new(),
        serde_json::Value::Object(_) | serde_json::Value::Array(_) => {
            serde_json::to_string_pretty(parsed).unwrap_or_else(|_| parsed.to_string())
        }
        _ => parsed.to_string(),
    };
    let preview = truncate_output_for_display(
        &rendered_output,
        TOOL_OUTPUT_DISPLAY_MAX_LINES,
        TOOL_OUTPUT_DISPLAY_MAX_CHARS,
    );

    if preview.is_empty() {
        format!("{icon} \x1b[38;5;245m{name}\x1b[0m")
    } else if preview.contains('\n') {
        format!("{icon} \x1b[38;5;245m{name}\x1b[0m\n{preview}")
    } else {
        format!("{icon} \x1b[38;5;245m{name}:\x1b[0m {preview}")
    }
}

fn summarize_tool_payload(payload: &str) -> String {
    let compact = match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) => value.to_string(),
        Err(_) => payload.trim().to_string(),
    };
    truncate_for_summary(&compact, 96)
}

fn truncate_for_summary(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

fn truncate_output_for_display(content: &str, max_lines: usize, max_chars: usize) -> String {
    let original = content.trim_end_matches('\n');
    if original.is_empty() {
        return String::new();
    }

    let mut preview_lines = Vec::new();
    let mut used_chars = 0usize;
    let mut truncated = false;

    for (index, line) in original.lines().enumerate() {
        if index >= max_lines {
            truncated = true;
            break;
        }

        let newline_cost = usize::from(!preview_lines.is_empty());
        let available = max_chars.saturating_sub(used_chars + newline_cost);
        if available == 0 {
            truncated = true;
            break;
        }

        let line_chars = line.chars().count();
        if line_chars > available {
            preview_lines.push(line.chars().take(available).collect::<String>());
            truncated = true;
            break;
        }

        preview_lines.push(line.to_string());
        used_chars += newline_cost + line_chars;
    }

    let mut preview = preview_lines.join("\n");
    if truncated {
        if !preview.is_empty() {
            preview.push('\n');
        }
        preview.push_str(DISPLAY_TRUNCATION_NOTICE);
    }
    preview
}

fn push_output_block(
    block: OutputContentBlock,
    out: &mut (impl Write + ?Sized),
    events: &mut Vec<AssistantEvent>,
    pending_tool: &mut Option<(String, String, String)>,
    streaming_tool_input: bool,
) -> Result<(), RuntimeError> {
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                let rendered = TerminalRenderer::new().markdown_to_ansi(&text);
                write!(out, "{rendered}")
                    .and_then(|()| out.flush())
                    .map_err(|error| RuntimeError::new(error.to_string()))?;
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            // During streaming, the initial content_block_start has an empty input ({}).
            // The real input arrives via input_json_delta events. In
            // non-streaming responses, preserve a legitimate empty object.
            let initial_input = if streaming_tool_input
                && input.is_object()
                && input.as_object().is_some_and(serde_json::Map::is_empty)
            {
                String::new()
            } else {
                input.to_string()
            };
            *pending_tool = Some((id, name, initial_input));
        }
        OutputContentBlock::Thinking { .. } | OutputContentBlock::RedactedThinking { .. } => {}
    }
    Ok(())
}

fn response_to_events(
    response: MessageResponse,
    out: &mut (impl Write + ?Sized),
) -> Result<Vec<AssistantEvent>, RuntimeError> {
    let mut events = Vec::new();
    let mut pending_tool = None;

    for block in response.content {
        push_output_block(block, out, &mut events, &mut pending_tool, false)?;
        if let Some((id, name, input)) = pending_tool.take() {
            events.push(AssistantEvent::ToolUse { id, name, input });
        }
    }

    events.push(AssistantEvent::Usage(TokenUsage {
        input_tokens: response.usage.input_tokens,
        output_tokens: response.usage.output_tokens,
        cache_creation_input_tokens: response.usage.cache_creation_input_tokens,
        cache_read_input_tokens: response.usage.cache_read_input_tokens,
    }));
    events.push(AssistantEvent::MessageStop);
    Ok(events)
}

struct CliToolExecutor {
    renderer: TerminalRenderer,
    emit_output: bool,
    allowed_tools: Option<AllowedToolSet>,
    tool_registry: GlobalToolRegistry,
}

impl CliToolExecutor {
    fn new(
        allowed_tools: Option<AllowedToolSet>,
        emit_output: bool,
        tool_registry: GlobalToolRegistry,
    ) -> Self {
        Self {
            renderer: TerminalRenderer::new(),
            emit_output,
            allowed_tools,
            tool_registry,
        }
    }
}

impl ToolExecutor for CliToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if self
            .allowed_tools
            .as_ref()
            .is_some_and(|allowed| !allowed.contains(tool_name))
        {
            return Err(ToolError::new(format!(
                "tool `{tool_name}` is not enabled by the current --allowedTools setting"
            )));
        }
        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        match self.tool_registry.execute(tool_name, &value) {
            Ok(output) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &output, false);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|error| ToolError::new(error.to_string()))?;
                }
                Ok(output)
            }
            Err(error) => {
                if self.emit_output {
                    let markdown = format_tool_result(tool_name, &error, true);
                    self.renderer
                        .stream_markdown(&markdown, &mut io::stdout())
                        .map_err(|stream_error| ToolError::new(stream_error.to_string()))?;
                }
                Err(ToolError::new(error))
            }
        }
    }
}

fn permission_policy(mode: PermissionMode, tool_registry: &GlobalToolRegistry) -> PermissionPolicy {
    tool_registry.permission_specs(None).into_iter().fold(
        PermissionPolicy::new(mode),
        |policy, (name, required_permission)| {
            policy.with_tool_requirement(name, required_permission)
        },
    )
}

fn convert_messages(messages: &[ConversationMessage]) -> Vec<InputMessage> {
    messages
        .iter()
        .filter_map(|message| {
            let role = match message.role {
                MessageRole::System | MessageRole::User | MessageRole::Tool => "user",
                MessageRole::Assistant => "assistant",
            };
            let content = message
                .blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => InputContentBlock::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => InputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::from_str(input)
                            .unwrap_or_else(|_| serde_json::json!({ "raw": input })),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        output,
                        is_error,
                        ..
                    } => InputContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: vec![ToolResultContentBlock::Text {
                            text: output.clone(),
                        }],
                        is_error: *is_error,
                    },
                })
                .collect::<Vec<_>>();
            (!content.is_empty()).then(|| InputMessage {
                role: role.to_string(),
                content,
            })
        })
        .collect()
}

fn print_help_to(out: &mut impl Write) -> io::Result<()> {
    let cli_name = current_cli_name();
    writeln!(out, "{cli_name} v{VERSION}")?;
    writeln!(out)?;
    writeln!(out, "Usage:")?;
    writeln!(
        out,
        "  {cli_name} [--model MODEL] [--provider PROFILE] [--allowedTools TOOL[,TOOL...]]"
    )?;
    writeln!(out, "      Start the interactive REPL")?;
    writeln!(
        out,
        "  {cli_name} [--model MODEL] [--provider PROFILE] [--output-format text|json] prompt TEXT"
    )?;
    writeln!(out, "      Send one prompt and exit")?;
    writeln!(
        out,
        "  {cli_name} [--model MODEL] [--provider PROFILE] [--output-format text|json] TEXT"
    )?;
    writeln!(out, "      Shorthand non-interactive prompt mode")?;
    writeln!(
        out,
        "  {cli_name} --resume SESSION.json [/status] [/compact] [...]"
    )?;
    writeln!(
        out,
        "      Inspect or maintain a saved session without entering the REPL"
    )?;
    writeln!(out, "  {cli_name} dump-manifests")?;
    writeln!(out, "  {cli_name} bootstrap-plan")?;
    writeln!(out, "  {cli_name} agents")?;
    writeln!(out, "  {cli_name} skills")?;
    writeln!(
        out,
        "  {cli_name} system-prompt [--cwd PATH] [--date YYYY-MM-DD]"
    )?;
    writeln!(out, "  {cli_name} login")?;
    writeln!(out, "  {cli_name} logout")?;
    writeln!(out, "  {cli_name} init [--research survey]")?;
    writeln!(
        out,
        "  {cli_name} plugins [list|install <path>|enable <name>|disable <name>|uninstall <id>|update <id>]"
    )?;
    writeln!(
        out,
        "  {cli_name} project-skill <init|validate|promote|doctor> [...]"
    )?;
    writeln!(out)?;
    writeln!(out, "Flags:")?;
    writeln!(
        out,
        "  --model MODEL              Override the active model"
    )?;
    writeln!(
        out,
        "  --provider PROFILE         Select a configured provider profile"
    )?;
    writeln!(
        out,
        "  --output-format FORMAT     Non-interactive output format: text or json"
    )?;
    writeln!(
        out,
        "  --permission-mode MODE     Set read-only, workspace-write, or danger-full-access"
    )?;
    writeln!(
        out,
        "  --dangerously-skip-permissions  Skip all permission checks"
    )?;
    writeln!(out, "  --allowedTools TOOLS       Restrict enabled tools (repeatable; comma-separated aliases supported)")?;
    writeln!(
        out,
        "  --version, -V              Print version and build information locally"
    )?;
    writeln!(out)?;
    writeln!(out, "Interactive slash commands:")?;
    writeln!(out, "{}", render_slash_command_help())?;
    writeln!(out)?;
    let resume_commands = resume_supported_slash_commands()
        .into_iter()
        .map(|spec| match spec.argument_hint {
            Some(argument_hint) => format!("/{} {}", spec.name, argument_hint),
            None => format!("/{}", spec.name),
        })
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(out, "Resume-safe commands: {resume_commands}")?;
    writeln!(out, "Examples:")?;
    writeln!(out, "  {cli_name} --model opus \"summarize this repo\"")?;
    writeln!(
        out,
        "  {cli_name} --output-format json prompt \"explain src/main.rs\""
    )?;
    writeln!(
        out,
        "  {cli_name} --allowedTools read,glob \"summarize Cargo.toml\""
    )?;
    writeln!(
        out,
        "  {cli_name} --resume session.json /status /diff /export notes.txt"
    )?;
    writeln!(out, "  {cli_name} agents")?;
    writeln!(out, "  {cli_name} /skills")?;
    writeln!(out, "  {cli_name} login")?;
    writeln!(out, "  {cli_name} init --research survey")?;
    writeln!(
        out,
        "  {cli_name} plugins install ../examples/external-plugins/research-regression"
    )?;
    writeln!(
        out,
        "  {cli_name} project-skill init survey-cleaning-sop --title \"Survey Cleaning SOP\" --description \"Draft workflow for local survey cleaning.\" --domain survey --use-when \"Use before scoring\" --source docs/research-method-standards.md"
    )?;
    writeln!(
        out,
        "  {cli_name} project-skill promote .claw/project-skills/survey-cleaning-sop --to project --held-out-validation passed"
    )?;
    writeln!(
        out,
        "  {cli_name} project-skill doctor .claw/project-skills/survey-cleaning-sop"
    )?;
    Ok(())
}

fn print_help() {
    let _ = print_help_to(&mut io::stdout());
}

#[cfg(test)]
mod tests {
    use super::{
        describe_tool_progress, doctor_project_skill, filter_tool_specs, format_compact_report,
        format_cost_report, format_internal_prompt_progress_line, format_model_report,
        format_model_switch_report, format_permissions_report, format_permissions_switch_report,
        format_resume_report, format_status_report, format_tool_call_start, format_tool_result,
        normalize_permission_mode, parse_args, parse_git_status_metadata, permission_policy,
        print_help_to, promote_project_skill, push_output_block, render_config_report,
        render_memory_report, render_repl_help, resolve_client_selection, resolve_model_alias,
        response_to_events, resume_supported_slash_commands, status_context,
        validate_project_skill_report, CliAction, CliOutputFormat, InitOptions,
        InitResearchProfile, InternalPromptProgressEvent, InternalPromptProgressState,
        ProjectSkillCommand, SlashCommand, StatusUsage,
    };
    use api::{MessageResponse, OutputContentBlock, Usage};
    use plugins::{PluginTool, PluginToolDefinition, PluginToolPermission};
    use runtime::{
        AssistantEvent, ConfigLoader, ContentBlock, ConversationMessage, MessageRole,
        PermissionMode,
    };
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tools::GlobalToolRegistry;

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        let seq = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "claw-cli-tests-{}-{nanos}-{seq}",
            std::process::id()
        ))
    }

    fn registry_with_plugin_tool() -> GlobalToolRegistry {
        GlobalToolRegistry::with_plugin_tools(vec![PluginTool::new(
            "plugin-demo@external",
            "plugin-demo",
            PluginToolDefinition {
                name: "plugin_echo".to_string(),
                description: Some("Echo plugin payload".to_string()),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "message": { "type": "string" }
                    },
                    "required": ["message"],
                    "additionalProperties": false
                }),
            },
            "echo".to_string(),
            Vec::new(),
            PluginToolPermission::WorkspaceWrite,
            None,
        )])
        .expect("plugin tool registry should build")
    }

    #[test]
    fn defaults_to_repl_when_no_args() {
        assert_eq!(
            parse_args(&[]).expect("args should parse"),
            CliAction::Repl {
                model: None,
                provider: None,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
            }
        );
    }

    #[test]
    fn parses_prompt_subcommand() {
        let args = vec![
            "prompt".to_string(),
            "hello".to_string(),
            "world".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "hello world".to_string(),
                model: None,
                provider: None,
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
            }
        );
    }

    #[test]
    fn parses_bare_prompt_and_json_output_flag() {
        let args = vec![
            "--output-format=json".to_string(),
            "--model".to_string(),
            "custom-opus".to_string(),
            "explain".to_string(),
            "this".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "explain this".to_string(),
                model: Some("custom-opus".to_string()),
                provider: None,
                output_format: CliOutputFormat::Json,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
            }
        );
    }

    #[test]
    fn resolves_model_aliases_in_args() {
        let args = vec![
            "--model".to_string(),
            "opus".to_string(),
            "explain".to_string(),
            "this".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "explain this".to_string(),
                model: Some("claude-opus-4-6".to_string()),
                provider: None,
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
            }
        );
    }

    #[test]
    fn parses_provider_flag_in_args() {
        let args = vec![
            "--provider".to_string(),
            "deepseek".to_string(),
            "prompt".to_string(),
            "hello".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Prompt {
                prompt: "hello".to_string(),
                model: None,
                provider: Some("deepseek".to_string()),
                output_format: CliOutputFormat::Text,
                allowed_tools: None,
                permission_mode: PermissionMode::DangerFullAccess,
            }
        );
    }

    #[test]
    fn resolve_client_selection_prefers_default_provider_profile_model() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".claw");
        fs::create_dir_all(cwd.join(".claw")).expect("project config dir");
        fs::create_dir_all(&home).expect("home config dir");
        fs::write(
            home.join("settings.json"),
            r#"{
              "model": "opus",
              "providers": {
                "default": "deepseek",
                "profiles": {
                  "deepseek": {
                    "type": "openai-compat",
                    "providerName": "DeepSeek",
                    "apiKeyEnv": "DEEPSEEK_API_KEY",
                    "baseUrl": "https://api.deepseek.com/v1",
                    "defaultModel": "deepseek-chat"
                  }
                }
              }
            }"#,
        )
        .expect("write settings");

        let config = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");
        let selection =
            resolve_client_selection(&config, None, None).expect("selection should succeed");
        assert_eq!(selection.provider_id.as_deref(), Some("deepseek"));
        assert_eq!(selection.model, "deepseek-chat");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn resolve_client_selection_prefers_explicit_model_over_provider_default() {
        let root = temp_dir();
        let cwd = root.join("project");
        let home = root.join("home").join(".claw");
        fs::create_dir_all(cwd.join(".claw")).expect("project config dir");
        fs::create_dir_all(&home).expect("home config dir");
        fs::write(
            home.join("settings.json"),
            r#"{
              "providers": {
                "default": "deepseek",
                "profiles": {
                  "deepseek": {
                    "type": "openai-compat",
                    "providerName": "DeepSeek",
                    "apiKeyEnv": "DEEPSEEK_API_KEY",
                    "baseUrl": "https://api.deepseek.com/v1",
                    "defaultModel": "deepseek-chat"
                  }
                }
              }
            }"#,
        )
        .expect("write settings");

        let config = ConfigLoader::new(&cwd, &home)
            .load()
            .expect("config should load");
        let selection = resolve_client_selection(
            &config,
            Some("deepseek-reasoner".to_string()),
            Some("deepseek".to_string()),
        )
        .expect("selection should succeed");
        assert_eq!(selection.provider_id.as_deref(), Some("deepseek"));
        assert_eq!(selection.model, "deepseek-reasoner");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn resolve_client_selection_rejects_unknown_provider_profile() {
        let config = runtime::RuntimeConfig::empty();
        let error = resolve_client_selection(&config, None, Some("missing".to_string()))
            .expect_err("selection should fail");
        assert!(error
            .to_string()
            .contains("unknown provider profile: missing"));
    }

    #[test]
    fn resolves_known_model_aliases() {
        assert_eq!(resolve_model_alias("opus"), "claude-opus-4-6");
        assert_eq!(resolve_model_alias("sonnet"), "claude-sonnet-4-6");
        assert_eq!(resolve_model_alias("haiku"), "claude-haiku-4-5-20251213");
        assert_eq!(resolve_model_alias("custom-opus"), "custom-opus");
    }

    #[test]
    fn parses_version_flags_without_initializing_prompt_mode() {
        assert_eq!(
            parse_args(&["--version".to_string()]).expect("args should parse"),
            CliAction::Version
        );
        assert_eq!(
            parse_args(&["-V".to_string()]).expect("args should parse"),
            CliAction::Version
        );
    }

    #[test]
    fn parses_permission_mode_flag() {
        let args = vec!["--permission-mode=read-only".to_string()];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Repl {
                model: None,
                provider: None,
                allowed_tools: None,
                permission_mode: PermissionMode::ReadOnly,
            }
        );
    }

    #[test]
    fn parses_allowed_tools_flags_with_aliases_and_lists() {
        let args = vec![
            "--allowedTools".to_string(),
            "read,glob".to_string(),
            "--allowed-tools=write_file".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::Repl {
                model: None,
                provider: None,
                allowed_tools: Some(
                    ["glob_search", "read_file", "write_file"]
                        .into_iter()
                        .map(str::to_string)
                        .collect()
                ),
                permission_mode: PermissionMode::DangerFullAccess,
            }
        );
    }

    #[test]
    fn rejects_unknown_allowed_tools() {
        let error = parse_args(&["--allowedTools".to_string(), "teleport".to_string()])
            .expect_err("tool should be rejected");
        assert!(error.contains("unsupported tool in --allowedTools: teleport"));
    }

    #[test]
    fn parses_system_prompt_options() {
        let args = vec![
            "system-prompt".to_string(),
            "--cwd".to_string(),
            "/tmp/project".to_string(),
            "--date".to_string(),
            "2026-04-01".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::PrintSystemPrompt {
                cwd: PathBuf::from("/tmp/project"),
                date: "2026-04-01".to_string(),
            }
        );
    }

    #[test]
    fn parses_login_and_logout_subcommands() {
        assert_eq!(
            parse_args(&["login".to_string()]).expect("login should parse"),
            CliAction::Login
        );
        assert_eq!(
            parse_args(&["logout".to_string()]).expect("logout should parse"),
            CliAction::Logout
        );
        assert_eq!(
            parse_args(&["init".to_string()]).expect("init should parse"),
            CliAction::Init {
                options: InitOptions::default()
            }
        );
        assert_eq!(
            parse_args(&[
                "init".to_string(),
                "--research".to_string(),
                "survey".to_string(),
            ])
            .expect("survey init should parse"),
            CliAction::Init {
                options: InitOptions {
                    research_profile: Some(InitResearchProfile::Survey),
                }
            }
        );
        assert_eq!(
            parse_args(&["init".to_string(), "survey".to_string()])
                .expect("survey shorthand init should parse"),
            CliAction::Init {
                options: InitOptions {
                    research_profile: Some(InitResearchProfile::Survey),
                }
            }
        );
        assert_eq!(
            parse_args(&["agents".to_string()]).expect("agents should parse"),
            CliAction::Agents { args: None }
        );
        assert_eq!(
            parse_args(&["skills".to_string()]).expect("skills should parse"),
            CliAction::Skills { args: None }
        );
        assert_eq!(
            parse_args(&["agents".to_string(), "--help".to_string()])
                .expect("agents help should parse"),
            CliAction::Agents {
                args: Some("--help".to_string())
            }
        );
    }

    #[test]
    fn parses_direct_agents_and_skills_slash_commands() {
        assert_eq!(
            parse_args(&["/agents".to_string()]).expect("/agents should parse"),
            CliAction::Agents { args: None }
        );
        assert_eq!(
            parse_args(&["/skills".to_string()]).expect("/skills should parse"),
            CliAction::Skills { args: None }
        );
        assert_eq!(
            parse_args(&["/skills".to_string(), "help".to_string()])
                .expect("/skills help should parse"),
            CliAction::Skills {
                args: Some("help".to_string())
            }
        );
        let error = parse_args(&["/status".to_string()])
            .expect_err("/status should remain REPL-only when invoked directly");
        assert!(error.contains("unsupported direct slash command"));
    }

    #[test]
    fn parses_project_skill_init_subcommand() {
        let args = vec![
            "project-skill".to_string(),
            "init".to_string(),
            "survey-cleaning-sop".to_string(),
            "--title".to_string(),
            "Survey Cleaning SOP".to_string(),
            "--description".to_string(),
            "Draft workflow".to_string(),
            "--domain".to_string(),
            "survey".to_string(),
            "--use-when".to_string(),
            "Use before scoring".to_string(),
            "--source".to_string(),
            "docs/research-method-standards.md".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("project-skill init should parse"),
            CliAction::ProjectSkill {
                command: ProjectSkillCommand::Init {
                    slug: "survey-cleaning-sop".to_string(),
                    title: "Survey Cleaning SOP".to_string(),
                    description: "Draft workflow".to_string(),
                    domain: "survey".to_string(),
                    use_when: "Use before scoring".to_string(),
                    sources: vec!["docs/research-method-standards.md".to_string()],
                    input_expectations: Vec::new(),
                    workflow_steps: Vec::new(),
                    outputs: Vec::new(),
                    limits: Vec::new(),
                    failure_checks: Vec::new(),
                    evaluation_examples: Vec::new(),
                    generated_by: "claw project-skill init".to_string(),
                    maturity: "draft".to_string(),
                    output_root: None,
                    targets: Vec::new(),
                    openclaw_root: None,
                    claude_root: None,
                },
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_project_skill_validate_subcommand() {
        let args = vec![
            "project-skill".to_string(),
            "validate".to_string(),
            ".claw/project-skills/survey-cleaning-sop".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("project-skill validate should parse"),
            CliAction::ProjectSkill {
                command: ProjectSkillCommand::Validate {
                    path: PathBuf::from(".claw/project-skills/survey-cleaning-sop"),
                },
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_project_skill_promote_subcommand() {
        let args = vec![
            "project-skill".to_string(),
            "promote".to_string(),
            ".claw/project-skills/survey-cleaning-sop".to_string(),
            "--to".to_string(),
            "project".to_string(),
            "--held-out-validation".to_string(),
            "passed".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("project-skill promote should parse"),
            CliAction::ProjectSkill {
                command: ProjectSkillCommand::Promote {
                    path: PathBuf::from(".claw/project-skills/survey-cleaning-sop"),
                    to: "project".to_string(),
                    verification_status: None,
                    held_out_validation_status: Some("passed".to_string()),
                },
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_project_skill_doctor_subcommand() {
        let args = vec![
            "project-skill".to_string(),
            "doctor".to_string(),
            ".claw/project-skills/survey-cleaning-sop".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("project-skill doctor should parse"),
            CliAction::ProjectSkill {
                command: ProjectSkillCommand::Doctor {
                    path: PathBuf::from(".claw/project-skills/survey-cleaning-sop"),
                },
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_project_skill_json_output_format() {
        let args = vec![
            "--output-format".to_string(),
            "json".to_string(),
            "project-skill".to_string(),
            "validate".to_string(),
            ".claw/project-skills/survey-cleaning-sop".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("project-skill json output should parse"),
            CliAction::ProjectSkill {
                command: ProjectSkillCommand::Validate {
                    path: PathBuf::from(".claw/project-skills/survey-cleaning-sop"),
                },
                output_format: CliOutputFormat::Json,
            }
        );
    }

    #[test]
    fn parses_project_skill_compatibility_targets() {
        let args = vec![
            "project-skill".to_string(),
            "init".to_string(),
            "survey-cleaning-sop".to_string(),
            "--title".to_string(),
            "Survey Cleaning SOP".to_string(),
            "--description".to_string(),
            "Draft workflow".to_string(),
            "--domain".to_string(),
            "survey".to_string(),
            "--use-when".to_string(),
            "Use before scoring".to_string(),
            "--source".to_string(),
            "docs/research-method-standards.md".to_string(),
            "--target".to_string(),
            "openclaw".to_string(),
            "--target".to_string(),
            "claude-command".to_string(),
            "--openclaw-root".to_string(),
            ".compat/openclaw".to_string(),
            "--claude-root".to_string(),
            ".compat/claude".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("compat targets should parse"),
            CliAction::ProjectSkill {
                command: ProjectSkillCommand::Init {
                    slug: "survey-cleaning-sop".to_string(),
                    title: "Survey Cleaning SOP".to_string(),
                    description: "Draft workflow".to_string(),
                    domain: "survey".to_string(),
                    use_when: "Use before scoring".to_string(),
                    sources: vec!["docs/research-method-standards.md".to_string()],
                    input_expectations: Vec::new(),
                    workflow_steps: Vec::new(),
                    outputs: Vec::new(),
                    limits: Vec::new(),
                    failure_checks: Vec::new(),
                    evaluation_examples: Vec::new(),
                    generated_by: "claw project-skill init".to_string(),
                    maturity: "draft".to_string(),
                    output_root: None,
                    targets: vec!["openclaw".to_string(), "claude-command".to_string()],
                    openclaw_root: Some(PathBuf::from(".compat/openclaw")),
                    claude_root: Some(PathBuf::from(".compat/claude")),
                },
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn parses_project_skill_governance_flags() {
        let args = vec![
            "project-skill".to_string(),
            "init".to_string(),
            "survey-cleaning-sop".to_string(),
            "--title".to_string(),
            "Survey Cleaning SOP".to_string(),
            "--description".to_string(),
            "Draft workflow".to_string(),
            "--domain".to_string(),
            "survey".to_string(),
            "--use-when".to_string(),
            "Use before scoring".to_string(),
            "--source".to_string(),
            "docs/research-method-standards.md".to_string(),
            "--input-expectation".to_string(),
            "Approved questionnaire codebook".to_string(),
            "--failure-check".to_string(),
            "Stop if reverse-keyed items are ambiguous".to_string(),
            "--evaluation-example".to_string(),
            "Held-out pilot dataset walkthrough".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("governance flags should parse"),
            CliAction::ProjectSkill {
                command: ProjectSkillCommand::Init {
                    slug: "survey-cleaning-sop".to_string(),
                    title: "Survey Cleaning SOP".to_string(),
                    description: "Draft workflow".to_string(),
                    domain: "survey".to_string(),
                    use_when: "Use before scoring".to_string(),
                    sources: vec!["docs/research-method-standards.md".to_string()],
                    input_expectations: vec!["Approved questionnaire codebook".to_string()],
                    workflow_steps: Vec::new(),
                    outputs: Vec::new(),
                    limits: Vec::new(),
                    failure_checks: vec!["Stop if reverse-keyed items are ambiguous".to_string()],
                    evaluation_examples: vec!["Held-out pilot dataset walkthrough".to_string()],
                    generated_by: "claw project-skill init".to_string(),
                    maturity: "draft".to_string(),
                    output_root: None,
                    targets: Vec::new(),
                    openclaw_root: None,
                    claude_root: None,
                },
                output_format: CliOutputFormat::Text,
            }
        );
    }

    #[test]
    fn validate_project_skill_reports_warnings_and_remediation() {
        let root = temp_dir().join("skill-warning-case");
        fs::create_dir_all(&root).expect("temp skill root should be creatable");
        fs::write(root.join("SKILL.md"), "# Draft Skill\n").expect("skill md should write");
        fs::write(root.join("README.md"), "# Draft Skill\n").expect("readme should write");
        fs::write(
            root.join("skill.json"),
            serde_json::to_string_pretty(&json!({
                "name": "survey-cleaning-sop",
                "title": "Survey Cleaning SOP",
                "description": "Draft workflow",
                "domain": "survey",
                "generated_at": "2026-04-02T00:00:00Z",
                "generated_by": "claw project-skill init",
                "use_when": "Use before scoring",
                "input_expectations": ["Approved questionnaire codebook"],
                "workflow": ["Review questionnaire structure"],
                "outputs": ["SKILL.md draft"],
                "limits": ["Draft only"],
                "failure_checks": ["Stop if required inputs are unclear"],
                "evaluation_examples": [
                    "Primary project example: pending validation",
                    "Held-out example: pending validation"
                ],
                "verification_status": "drafted",
                "held_out_validation_status": "pending",
                "maturity_level": "draft",
                "source_materials": [
                    {"path": "missing-source.md", "exists": false, "sha256": serde_json::Value::Null}
                ]
            }))
            .expect("json should serialize"),
        )
        .expect("skill json should write");

        let report = validate_project_skill_report(&root)
            .expect("draft skill should validate with warnings");
        assert!(
            report.text.contains("Warnings"),
            "validation report should contain warnings section: {}",
            report.text
        );
        assert!(
            report.text.contains("skill is still at draft maturity"),
            "validation report should mention draft warning: {}",
            report.text
        );
        assert!(
            report.text.contains("Remediation"),
            "validation report should contain remediation section: {}",
            report.text
        );
        assert!(
            report.text.contains("promote after validation"),
            "validation report should include remediation guidance: {}",
            report.text
        );

        fs::remove_dir_all(&root).expect("temp skill root should clean up");
    }

    #[test]
    fn doctor_project_skill_reports_non_blocking_findings() {
        let root = temp_dir().join("skill-doctor-case");
        fs::create_dir_all(&root).expect("temp skill root should be creatable");
        fs::write(root.join("SKILL.md"), "# Draft Skill\n").expect("skill md should write");
        fs::write(root.join("README.md"), "# Draft Skill\n").expect("readme should write");
        fs::write(
            root.join("skill.json"),
            serde_json::to_string_pretty(&json!({
                "name": "survey-cleaning-sop",
                "title": "Survey Cleaning SOP",
                "description": "Draft workflow",
                "domain": "survey",
                "generated_at": "2026-04-02T00:00:00Z",
                "generated_by": "claw project-skill init",
                "use_when": "Use before scoring",
                "input_expectations": ["Approved questionnaire codebook"],
                "workflow": ["Review questionnaire structure"],
                "outputs": ["SKILL.md draft"],
                "limits": ["Draft only"],
                "failure_checks": ["Stop if required inputs are unclear"],
                "evaluation_examples": [
                    "Primary project example: pending validation",
                    "Held-out example: pending validation"
                ],
                "verification_status": "drafted",
                "held_out_validation_status": "pending",
                "maturity_level": "draft",
                "source_materials": [
                    {"path": "missing-source.md", "exists": false, "sha256": serde_json::Value::Null}
                ]
            }))
            .expect("json should serialize"),
        )
        .expect("skill json should write");

        let report =
            doctor_project_skill(&root).expect("doctor should report non-blocking findings");
        assert!(
            report.text.contains("Project skill doctor"),
            "doctor report should contain header: {}",
            report.text
        );
        assert!(
            report
                .text
                .contains("[warn] held-out validation is still pending"),
            "doctor report should contain held-out warning: {}",
            report.text
        );
        assert!(
            report.text.contains("outputs still look scaffold-level"),
            "doctor report should flag generic outputs: {}",
            report.text
        );

        fs::remove_dir_all(&root).expect("temp skill root should clean up");
    }

    #[test]
    fn promote_project_skill_rewrites_skill_docs_from_metadata() {
        let root = temp_dir().join("skill-promote-doc-sync");
        fs::create_dir_all(&root).expect("temp skill root should be creatable");
        fs::write(root.join("SKILL.md"), "# stale\n").expect("skill md should write");
        fs::write(root.join("README.md"), "# stale\n").expect("readme should write");
        fs::write(
            root.join("skill.json"),
            serde_json::to_string_pretty(&json!({
                "name": "survey-cleaning-sop",
                "title": "Survey Cleaning SOP",
                "description": "Draft workflow",
                "domain": "survey",
                "generated_at": "2026-04-02T00:00:00Z",
                "generated_by": "claw project-skill init",
                "use_when": "Use before scoring",
                "input_expectations": ["Approved questionnaire codebook"],
                "workflow": ["Review questionnaire structure"],
                "outputs": ["Analysis decision log"],
                "limits": ["Draft only"],
                "failure_checks": ["Stop if required inputs are unclear"],
                "evaluation_examples": [
                    "Primary project example: validated",
                    "Held-out example: validated"
                ],
                "verification_status": "drafted",
                "held_out_validation_status": "pending",
                "maturity_level": "draft",
                "source_materials": [
                    {"path": "docs/research-method-standards.md", "exists": true, "sha256": "abc123"}
                ]
            }))
            .expect("json should serialize"),
        )
        .expect("skill json should write");

        let report = promote_project_skill(&root, "project", None, Some("passed"))
            .expect("promotion should succeed");
        assert!(
            report.text.contains("Maturity         project"),
            "promotion report should contain project maturity: {}",
            report.text
        );

        let skill_md = fs::read_to_string(root.join("SKILL.md")).expect("skill md should read");
        assert!(
            skill_md.contains("- maturity: `project`"),
            "skill markdown should reflect promoted maturity: {skill_md}"
        );
        assert!(
            skill_md.contains("- verification: `held-out-validated`"),
            "skill markdown should reflect promoted verification: {skill_md}"
        );
        assert!(
            skill_md.contains("- held_out_validation: `passed`"),
            "skill markdown should reflect passed held-out validation: {skill_md}"
        );

        let readme = fs::read_to_string(root.join("README.md")).expect("readme should read");
        assert!(
            readme.contains("- maturity: `project`"),
            "readme should reflect promoted maturity: {readme}"
        );
        assert!(
            readme.contains("external plugin"),
            "readme should describe the next step after promotion: {readme}"
        );

        fs::remove_dir_all(&root).expect("temp skill root should clean up");
    }

    #[test]
    fn parses_plugins_subcommand() {
        assert_eq!(
            parse_args(&["plugins".to_string()]).expect("plugins list should parse"),
            CliAction::Plugins {
                action: None,
                target: None,
            }
        );
        assert_eq!(
            parse_args(&[
                "plugins".to_string(),
                "install".to_string(),
                "../examples/external-plugins/research-regression".to_string(),
            ])
            .expect("plugins install should parse"),
            CliAction::Plugins {
                action: Some("install".to_string()),
                target: Some("../examples/external-plugins/research-regression".to_string()),
            }
        );
        assert_eq!(
            parse_args(&[
                "plugin".to_string(),
                "disable".to_string(),
                "research-regression".to_string(),
            ])
            .expect("plugin alias should parse"),
            CliAction::Plugins {
                action: Some("disable".to_string()),
                target: Some("research-regression".to_string()),
            }
        );
    }

    #[test]
    fn parses_resume_flag_with_slash_command() {
        let args = vec![
            "--resume".to_string(),
            "session.json".to_string(),
            "/compact".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.json"),
                commands: vec!["/compact".to_string()],
            }
        );
    }

    #[test]
    fn parses_resume_flag_with_multiple_slash_commands() {
        let args = vec![
            "--resume".to_string(),
            "session.json".to_string(),
            "/status".to_string(),
            "/compact".to_string(),
            "/cost".to_string(),
        ];
        assert_eq!(
            parse_args(&args).expect("args should parse"),
            CliAction::ResumeSession {
                session_path: PathBuf::from("session.json"),
                commands: vec![
                    "/status".to_string(),
                    "/compact".to_string(),
                    "/cost".to_string(),
                ],
            }
        );
    }

    #[test]
    fn filtered_tool_specs_respect_allowlist() {
        let allowed = ["read_file", "grep_search"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let filtered = filter_tool_specs(&GlobalToolRegistry::builtin(), Some(&allowed));
        let names = filtered
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["read_file", "grep_search"]);
    }

    #[test]
    fn filtered_tool_specs_include_plugin_tools() {
        let filtered = filter_tool_specs(&registry_with_plugin_tool(), None);
        let names = filtered
            .into_iter()
            .map(|definition| definition.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"bash".to_string()));
        assert!(names.contains(&"plugin_echo".to_string()));
    }

    #[test]
    fn permission_policy_uses_plugin_tool_permissions() {
        let policy = permission_policy(PermissionMode::ReadOnly, &registry_with_plugin_tool());
        let required = policy.required_mode_for("plugin_echo");
        assert_eq!(required, PermissionMode::WorkspaceWrite);
    }

    #[test]
    fn shared_help_uses_resume_annotation_copy() {
        let help = commands::render_slash_command_help();
        assert!(help.contains("Slash commands"));
        assert!(help.contains("[resume] = also available via claw --resume SESSION.json"));
    }

    #[test]
    fn repl_help_includes_shared_commands_and_exit() {
        let help = render_repl_help();
        assert!(help.contains("REPL"));
        assert!(help.contains("/help"));
        assert!(help.contains("/status"));
        assert!(help.contains("/model [model]"));
        assert!(help.contains("/permissions [read-only|workspace-write|danger-full-access]"));
        assert!(help.contains("/clear [--confirm]"));
        assert!(help.contains("/cost"));
        assert!(help.contains("/resume <session-path>"));
        assert!(help.contains("/config [env|hooks|model|providers|research|plugins]"));
        assert!(help.contains("/memory"));
        assert!(help.contains("/init"));
        assert!(help.contains("/diff"));
        assert!(help.contains("/version"));
        assert!(help.contains("/export [file]"));
        assert!(help.contains("/session [list|switch <session-id>]"));
        assert!(help.contains(
            "/plugin [list|install <path>|enable <name>|disable <name>|uninstall <id>|update <id>]"
        ));
        assert!(help.contains("aliases: /plugins, /marketplace"));
        assert!(help.contains("/agents"));
        assert!(help.contains("/skills"));
        assert!(help.contains("/exit"));
    }

    #[test]
    fn resume_supported_command_list_matches_expected_surface() {
        let names = resume_supported_slash_commands()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "help", "status", "compact", "clear", "cost", "config", "memory", "init", "diff",
                "version", "export", "agents", "skills",
            ]
        );
    }

    #[test]
    fn resume_report_uses_sectioned_layout() {
        let report = format_resume_report("session.json", 14, 6);
        assert!(report.contains("Session resumed"));
        assert!(report.contains("Session file     session.json"));
        assert!(report.contains("Messages         14"));
        assert!(report.contains("Turns            6"));
    }

    #[test]
    fn compact_report_uses_structured_output() {
        let compacted = format_compact_report(8, 5, false);
        assert!(compacted.contains("Compact"));
        assert!(compacted.contains("Result           compacted"));
        assert!(compacted.contains("Messages removed 8"));
        let skipped = format_compact_report(0, 3, true);
        assert!(skipped.contains("Result           skipped"));
    }

    #[test]
    fn cost_report_uses_sectioned_layout() {
        let report = format_cost_report(runtime::TokenUsage {
            input_tokens: 20,
            output_tokens: 8,
            cache_creation_input_tokens: 3,
            cache_read_input_tokens: 1,
        });
        assert!(report.contains("Cost"));
        assert!(report.contains("Input tokens     20"));
        assert!(report.contains("Output tokens    8"));
        assert!(report.contains("Cache create     3"));
        assert!(report.contains("Cache read       1"));
        assert!(report.contains("Total tokens     32"));
    }

    #[test]
    fn permissions_report_uses_sectioned_layout() {
        let report = format_permissions_report("workspace-write");
        assert!(report.contains("Permissions"));
        assert!(report.contains("Active mode      workspace-write"));
        assert!(report.contains("Modes"));
        assert!(report.contains("read-only          ○ available Read/search tools only"));
        assert!(report.contains("workspace-write    ● current   Edit files inside the workspace"));
        assert!(report.contains("danger-full-access ○ available Unrestricted tool access"));
    }

    #[test]
    fn permissions_switch_report_is_structured() {
        let report = format_permissions_switch_report("read-only", "workspace-write");
        assert!(report.contains("Permissions updated"));
        assert!(report.contains("Result           mode switched"));
        assert!(report.contains("Previous mode    read-only"));
        assert!(report.contains("Active mode      workspace-write"));
        assert!(report.contains("Applies to       subsequent tool calls"));
    }

    #[test]
    fn init_help_mentions_direct_subcommand() {
        let mut help = Vec::new();
        print_help_to(&mut help).expect("help should render");
        let help = String::from_utf8(help).expect("help should be utf8");
        assert!(help.contains("claw init [--research survey]"));
        assert!(
            help.contains(
                "claw plugins [list|install <path>|enable <name>|disable <name>|uninstall <id>|update <id>]"
            )
        );
        assert!(help.contains("claw project-skill init survey-cleaning-sop"));
        assert!(
            help.contains("claw project-skill promote .claw/project-skills/survey-cleaning-sop")
        );
        assert!(help.contains("claw project-skill doctor .claw/project-skills/survey-cleaning-sop"));
        assert!(help.contains("claw agents"));
        assert!(help.contains("claw skills"));
        assert!(help.contains("claw /skills"));
    }

    #[test]
    fn model_report_uses_sectioned_layout() {
        let report = format_model_report("sonnet", 12, 4);
        assert!(report.contains("Model"));
        assert!(report.contains("Current model    sonnet"));
        assert!(report.contains("Session messages 12"));
        assert!(report.contains("Switch models with /model <name>"));
    }

    #[test]
    fn model_switch_report_preserves_context_summary() {
        let report = format_model_switch_report("sonnet", "opus", 9);
        assert!(report.contains("Model updated"));
        assert!(report.contains("Previous         sonnet"));
        assert!(report.contains("Current          opus"));
        assert!(report.contains("Preserved msgs   9"));
    }

    #[test]
    fn status_line_reports_model_and_token_totals() {
        let status = format_status_report(
            "sonnet",
            StatusUsage {
                message_count: 7,
                turns: 3,
                latest: runtime::TokenUsage {
                    input_tokens: 5,
                    output_tokens: 4,
                    cache_creation_input_tokens: 1,
                    cache_read_input_tokens: 0,
                },
                cumulative: runtime::TokenUsage {
                    input_tokens: 20,
                    output_tokens: 8,
                    cache_creation_input_tokens: 2,
                    cache_read_input_tokens: 1,
                },
                estimated_tokens: 128,
            },
            "workspace-write",
            &super::StatusContext {
                cwd: PathBuf::from("/tmp/project"),
                session_path: Some(PathBuf::from("session.json")),
                loaded_config_files: 2,
                discovered_config_files: 3,
                memory_file_count: 4,
                project_root: Some(PathBuf::from("/tmp")),
                git_branch: Some("main".to_string()),
            },
        );
        assert!(status.contains("Status"));
        assert!(status.contains("Model            sonnet"));
        assert!(status.contains("Permission mode  workspace-write"));
        assert!(status.contains("Messages         7"));
        assert!(status.contains("Latest total     10"));
        assert!(status.contains("Cumulative total 31"));
        assert!(status.contains("Cwd              /tmp/project"));
        assert!(status.contains("Project root     /tmp"));
        assert!(status.contains("Git branch       main"));
        assert!(status.contains("Session          session.json"));
        assert!(status.contains("Config files     loaded 2/3"));
        assert!(status.contains("Memory files     4"));
    }

    #[test]
    fn config_report_supports_section_views() {
        let report = render_config_report(Some("env")).expect("config report should render");
        assert!(report.contains("Merged section: env"));
        let plugins_report =
            render_config_report(Some("plugins")).expect("plugins config report should render");
        assert!(plugins_report.contains("Merged section: plugins"));
        let providers_report =
            render_config_report(Some("providers")).expect("providers config report should render");
        assert!(providers_report.contains("Merged section: providers"));
        let research_report =
            render_config_report(Some("research")).expect("research config report should render");
        assert!(research_report.contains("Merged section: research"));
    }

    #[test]
    fn memory_report_uses_sectioned_layout() {
        let report = render_memory_report().expect("memory report should render");
        assert!(report.contains("Memory"));
        assert!(report.contains("Working directory"));
        assert!(report.contains("Instruction files"));
        assert!(report.contains("Discovered files"));
    }

    #[test]
    fn config_report_uses_sectioned_layout() {
        let report = render_config_report(None).expect("config report should render");
        assert!(report.contains("Config"));
        assert!(report.contains("Discovered files"));
        assert!(report.contains("Merged JSON"));
    }

    #[test]
    fn parses_git_status_metadata() {
        let (root, branch) = parse_git_status_metadata(Some(
            "## rcc/cli...origin/rcc/cli
 M src/main.rs",
        ));
        assert_eq!(branch.as_deref(), Some("rcc/cli"));
        let _ = root;
    }

    #[test]
    fn status_context_reads_real_workspace_metadata() {
        let context = status_context(None).expect("status context should load");
        assert!(context.cwd.is_absolute());
        assert_eq!(context.discovered_config_files, 5);
        assert!(context.loaded_config_files <= context.discovered_config_files);
    }

    #[test]
    fn normalizes_supported_permission_modes() {
        assert_eq!(normalize_permission_mode("read-only"), Some("read-only"));
        assert_eq!(
            normalize_permission_mode("workspace-write"),
            Some("workspace-write")
        );
        assert_eq!(
            normalize_permission_mode("danger-full-access"),
            Some("danger-full-access")
        );
        assert_eq!(normalize_permission_mode("unknown"), None);
    }

    #[test]
    fn clear_command_requires_explicit_confirmation_flag() {
        assert_eq!(
            SlashCommand::parse("/clear"),
            Some(SlashCommand::Clear { confirm: false })
        );
        assert_eq!(
            SlashCommand::parse("/clear --confirm"),
            Some(SlashCommand::Clear { confirm: true })
        );
    }

    #[test]
    fn parses_resume_and_config_slash_commands() {
        assert_eq!(
            SlashCommand::parse("/resume saved-session.json"),
            Some(SlashCommand::Resume {
                session_path: Some("saved-session.json".to_string())
            })
        );
        assert_eq!(
            SlashCommand::parse("/clear --confirm"),
            Some(SlashCommand::Clear { confirm: true })
        );
        assert_eq!(
            SlashCommand::parse("/config"),
            Some(SlashCommand::Config { section: None })
        );
        assert_eq!(
            SlashCommand::parse("/config env"),
            Some(SlashCommand::Config {
                section: Some("env".to_string())
            })
        );
        assert_eq!(SlashCommand::parse("/memory"), Some(SlashCommand::Memory));
        assert_eq!(SlashCommand::parse("/init"), Some(SlashCommand::Init));
    }

    #[test]
    fn init_template_mentions_detected_rust_workspace() {
        let rendered =
            crate::init::render_init_claw_md(std::path::Path::new("."), &InitOptions::default());
        assert!(rendered.contains("# CLAW.md"));
        assert!(rendered.contains("cargo clippy --workspace --all-targets -- -D warnings"));
    }

    #[test]
    fn converts_tool_roundtrip_messages() {
        let messages = vec![
            ConversationMessage::user_text("hello"),
            ConversationMessage::assistant(vec![ContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "bash".to_string(),
                input: "{\"command\":\"pwd\"}".to_string(),
            }]),
            ConversationMessage {
                role: MessageRole::Tool,
                blocks: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    tool_name: "bash".to_string(),
                    output: "ok".to_string(),
                    is_error: false,
                }],
                usage: None,
            },
        ];

        let converted = super::convert_messages(&messages);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[1].role, "assistant");
        assert_eq!(converted[2].role, "user");
    }
    #[test]
    fn repl_help_mentions_history_completion_and_multiline() {
        let help = render_repl_help();
        assert!(help.contains("Up/Down"));
        assert!(help.contains("Tab"));
        assert!(help.contains("Shift+Enter/Ctrl+J"));
    }

    #[test]
    fn tool_rendering_helpers_compact_output() {
        let start = format_tool_call_start("read_file", r#"{"path":"src/main.rs"}"#);
        assert!(start.contains("read_file"));
        assert!(start.contains("src/main.rs"));

        let done = format_tool_result(
            "read_file",
            r#"{"file":{"filePath":"src/main.rs","content":"hello","numLines":1,"startLine":1,"totalLines":1}}"#,
            false,
        );
        assert!(done.contains("📄 Read src/main.rs"));
        assert!(done.contains("hello"));
    }

    #[test]
    fn tool_rendering_truncates_large_read_output_for_display_only() {
        let content = (0..200)
            .map(|index| format!("line {index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        let output = json!({
            "file": {
                "filePath": "src/main.rs",
                "content": content,
                "numLines": 200,
                "startLine": 1,
                "totalLines": 200
            }
        })
        .to_string();

        let rendered = format_tool_result("read_file", &output, false);

        assert!(rendered.contains("line 000"));
        assert!(rendered.contains("line 079"));
        assert!(!rendered.contains("line 199"));
        assert!(rendered.contains("full result preserved in session"));
        assert!(output.contains("line 199"));
    }

    #[test]
    fn tool_rendering_truncates_large_bash_output_for_display_only() {
        let stdout = (0..120)
            .map(|index| format!("stdout {index:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        let output = json!({
            "stdout": stdout,
            "stderr": "",
            "returnCodeInterpretation": "completed successfully"
        })
        .to_string();

        let rendered = format_tool_result("bash", &output, false);

        assert!(rendered.contains("stdout 000"));
        assert!(rendered.contains("stdout 059"));
        assert!(!rendered.contains("stdout 119"));
        assert!(rendered.contains("full result preserved in session"));
        assert!(output.contains("stdout 119"));
    }

    #[test]
    fn tool_rendering_truncates_generic_long_output_for_display_only() {
        let items = (0..120)
            .map(|index| format!("payload {index:03}"))
            .collect::<Vec<_>>();
        let output = json!({
            "summary": "plugin payload",
            "items": items,
        })
        .to_string();

        let rendered = format_tool_result("plugin_echo", &output, false);

        assert!(rendered.contains("plugin_echo"));
        assert!(rendered.contains("payload 000"));
        assert!(rendered.contains("payload 040"));
        assert!(!rendered.contains("payload 080"));
        assert!(!rendered.contains("payload 119"));
        assert!(rendered.contains("full result preserved in session"));
        assert!(output.contains("payload 119"));
    }

    #[test]
    fn tool_rendering_truncates_raw_generic_output_for_display_only() {
        let output = (0..120)
            .map(|index| format!("raw {index:03}"))
            .collect::<Vec<_>>()
            .join("\n");

        let rendered = format_tool_result("plugin_echo", &output, false);

        assert!(rendered.contains("plugin_echo"));
        assert!(rendered.contains("raw 000"));
        assert!(rendered.contains("raw 059"));
        assert!(!rendered.contains("raw 119"));
        assert!(rendered.contains("full result preserved in session"));
        assert!(output.contains("raw 119"));
    }

    #[test]
    fn ultraplan_progress_lines_include_phase_step_and_elapsed_status() {
        let snapshot = InternalPromptProgressState {
            command_label: "Ultraplan",
            task_label: "ship plugin progress".to_string(),
            step: 3,
            phase: "running read_file".to_string(),
            detail: Some("reading rust/crates/claw-cli/src/main.rs".to_string()),
            saw_final_text: false,
        };

        let started = format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Started,
            &snapshot,
            Duration::from_secs(0),
            None,
        );
        let heartbeat = format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Heartbeat,
            &snapshot,
            Duration::from_secs(9),
            None,
        );
        let completed = format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Complete,
            &snapshot,
            Duration::from_secs(12),
            None,
        );
        let failed = format_internal_prompt_progress_line(
            InternalPromptProgressEvent::Failed,
            &snapshot,
            Duration::from_secs(12),
            Some("network timeout"),
        );

        assert!(started.contains("planning started"));
        assert!(started.contains("current step 3"));
        assert!(heartbeat.contains("heartbeat"));
        assert!(heartbeat.contains("9s elapsed"));
        assert!(heartbeat.contains("phase running read_file"));
        assert!(completed.contains("completed"));
        assert!(completed.contains("3 steps total"));
        assert!(failed.contains("failed"));
        assert!(failed.contains("network timeout"));
    }

    #[test]
    fn describe_tool_progress_summarizes_known_tools() {
        assert_eq!(
            describe_tool_progress("read_file", r#"{"path":"src/main.rs"}"#),
            "reading src/main.rs"
        );
        assert!(
            describe_tool_progress("bash", r#"{"command":"cargo test -p claw-cli"}"#)
                .contains("cargo test -p claw-cli")
        );
        assert_eq!(
            describe_tool_progress("grep_search", r#"{"pattern":"ultraplan","path":"rust"}"#),
            "grep `ultraplan` in rust"
        );
    }

    #[test]
    fn push_output_block_renders_markdown_text() {
        let mut out = Vec::new();
        let mut events = Vec::new();
        let mut pending_tool = None;

        push_output_block(
            OutputContentBlock::Text {
                text: "# Heading".to_string(),
            },
            &mut out,
            &mut events,
            &mut pending_tool,
            false,
        )
        .expect("text block should render");

        let rendered = String::from_utf8(out).expect("utf8");
        assert!(rendered.contains("Heading"));
        assert!(rendered.contains('\u{1b}'));
    }

    #[test]
    fn push_output_block_skips_empty_object_prefix_for_tool_streams() {
        let mut out = Vec::new();
        let mut events = Vec::new();
        let mut pending_tool = None;

        push_output_block(
            OutputContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "read_file".to_string(),
                input: json!({}),
            },
            &mut out,
            &mut events,
            &mut pending_tool,
            true,
        )
        .expect("tool block should accumulate");

        assert!(events.is_empty());
        assert_eq!(
            pending_tool,
            Some(("tool-1".to_string(), "read_file".to_string(), String::new(),))
        );
    }

    #[test]
    fn response_to_events_preserves_empty_object_json_input_outside_streaming() {
        let mut out = Vec::new();
        let events = response_to_events(
            MessageResponse {
                id: "msg-1".to_string(),
                kind: "message".to_string(),
                model: "claude-opus-4-6".to_string(),
                role: "assistant".to_string(),
                content: vec![OutputContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({}),
                }],
                stop_reason: Some("tool_use".to_string()),
                stop_sequence: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                request_id: None,
            },
            &mut out,
        )
        .expect("response conversion should succeed");

        assert!(matches!(
            &events[0],
            AssistantEvent::ToolUse { name, input, .. }
                if name == "read_file" && input == "{}"
        ));
    }

    #[test]
    fn response_to_events_preserves_non_empty_json_input_outside_streaming() {
        let mut out = Vec::new();
        let events = response_to_events(
            MessageResponse {
                id: "msg-2".to_string(),
                kind: "message".to_string(),
                model: "claude-opus-4-6".to_string(),
                role: "assistant".to_string(),
                content: vec![OutputContentBlock::ToolUse {
                    id: "tool-2".to_string(),
                    name: "read_file".to_string(),
                    input: json!({ "path": "rust/Cargo.toml" }),
                }],
                stop_reason: Some("tool_use".to_string()),
                stop_sequence: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                request_id: None,
            },
            &mut out,
        )
        .expect("response conversion should succeed");

        assert!(matches!(
            &events[0],
            AssistantEvent::ToolUse { name, input, .. }
                if name == "read_file" && input == "{\"path\":\"rust/Cargo.toml\"}"
        ));
    }

    #[test]
    fn response_to_events_ignores_thinking_blocks() {
        let mut out = Vec::new();
        let events = response_to_events(
            MessageResponse {
                id: "msg-3".to_string(),
                kind: "message".to_string(),
                model: "claude-opus-4-6".to_string(),
                role: "assistant".to_string(),
                content: vec![
                    OutputContentBlock::Thinking {
                        thinking: "step 1".to_string(),
                        signature: Some("sig_123".to_string()),
                    },
                    OutputContentBlock::Text {
                        text: "Final answer".to_string(),
                    },
                ],
                stop_reason: Some("end_turn".to_string()),
                stop_sequence: None,
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 1,
                    cache_creation_input_tokens: 0,
                    cache_read_input_tokens: 0,
                },
                request_id: None,
            },
            &mut out,
        )
        .expect("response conversion should succeed");

        assert!(matches!(
            &events[0],
            AssistantEvent::TextDelta(text) if text == "Final answer"
        ));
        assert!(!String::from_utf8(out).expect("utf8").contains("step 1"));
    }
}
