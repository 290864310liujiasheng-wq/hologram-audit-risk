use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

use unicode_width::UnicodeWidthStr;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use walkdir::WalkDir;

use crate::engine::{engine_analyze, engine_init};
use crate::routing::preflight::{load_baseline, run_full_check, save_baseline};
use crate::routing::secrets::SecretScanner;

pub const CLI_SCHEMA_VERSION: &str = "audit-risk.cli.v1";

const DEFAULT_AUDIT_JSONL_PATH: &str = ".hologram/audit.jsonl";
const DEFAULT_REPORT_OUTPUT_PATH: &str = ".hologram/latest-risk-report.json";
const DEFAULT_REVIEW_RULE_PATH: &str = ".hologram/rules/review.workspace.json";
const DEFAULT_REPAIR_RULE_PATH: &str = ".hologram/rules/repair.workspace.json";
const DEFAULT_PRE_COMMIT_PATH: &str = ".githooks/pre-commit";
const DEFAULT_CI_WORKFLOW_PATH: &str = ".github/workflows/hologram-risk.yml";
const DEFAULT_POLL_INTERVAL_MS: u64 = 1_000;
const WATCH_DEBOUNCE_WINDOW_MS: u128 = 10 * 60 * 1_000;
const DEFAULT_OBSERVE_BIND: &str = "0.0.0.0:8787";
const DEFAULT_OBSERVE_TITLE: &str = "audit-risk observe";
const PRO_PERSONAL_PLAN: &str = "pro_personal_monthly";
const PRO_PERSONAL_PRICE_LABEL: &str = "29 元/月";
const ENTITLEMENT_GRACE_HOURS: i64 = 72;
const ENTITLEMENT_REFRESH_INTERVAL_HOURS: i64 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandTier {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultOutputMode {
    Json,
    Human,
    Jsonl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FailGate {
    Off,
    Warn,
    RequireApproval,
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Home,
    Help,
    Tour,
    Version,
    Check {
        workspace: String,
        pretty: bool,
        verbose: bool,
        fail_on: FailGate,
    },
    Watch {
        workspace: String,
        verbose: bool,
        jsonl: bool,
        observe: bool,
        fail_on: FailGate,
    },
    Diff {
        before: String,
        after: String,
        pretty: bool,
        fail_on: FailGate,
    },
    Init {
        workspace: String,
        force: bool,
    },
    Doctor {
        workspace: Option<String>,
    },
    Report {
        workspace: Option<String>,
        config: Option<String>,
        output: Option<String>,
        fail_on: Option<FailGate>,
        history_compare: bool,
    },
    Rules {
        workspace: Option<String>,
        config: Option<String>,
    },
    Audit {
        workspace: Option<String>,
        config: Option<String>,
        query: Option<String>,
        limit: Option<usize>,
    },
    Verify {
        workspace: Option<String>,
    },
    Notify {
        workspace: Option<String>,
        test: bool,
        webhook_url: Option<String>,
    },
    Observe {
        workspace: Option<String>,
    },
    Auth {
        action: AuthAction,
    },
    RepairPlan {
        workspace: String,
        finding_id: String,
    },
    RepairApply {
        workspace: String,
        plan_id: String,
        approve: bool,
    },
    RepairApprove {
        workspace: String,
        plan_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthAction {
    Login,
    Logout,
    Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCliCommand {
    pub command: CliCommand,
    pub tier: CommandTier,
    pub default_output: DefaultOutputMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageError {
    message: String,
}

impl UsageError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone)]
struct CliRuntimeError {
    exit_code: i32,
    message: String,
}

impl CliRuntimeError {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            exit_code: 1,
            message: message.into(),
        }
    }

    fn environment(message: impl Into<String>) -> Self {
        Self {
            exit_code: 3,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone)]
struct CommandOutcome {
    exit_code: i32,
    stdout_json: Option<Value>,
    stdout_text: Option<String>,
    pretty_json: bool,
}

impl CommandOutcome {
    fn json(exit_code: i32, value: Value) -> Self {
        Self {
            exit_code,
            stdout_json: Some(value),
            stdout_text: None,
            pretty_json: true,
        }
    }

    fn text(exit_code: i32, value: impl Into<String>) -> Self {
        Self {
            exit_code,
            stdout_json: None,
            stdout_text: Some(value.into()),
            pretty_json: false,
        }
    }
}

fn resolve_output_mode(args: &[String], default_output: DefaultOutputMode) -> DefaultOutputMode {
    if take_flag(args, "--json") {
        DefaultOutputMode::Json
    } else {
        default_output
    }
}

pub fn parse_cli_command(args: &[String]) -> Result<ParsedCliCommand, UsageError> {
    let Some(subcommand) = args.first().map(String::as_str) else {
        return Ok(ParsedCliCommand {
            command: CliCommand::Home,
            tier: CommandTier::Primary,
            default_output: DefaultOutputMode::Human,
        });
    };

    let rest = &args[1..];
    match subcommand {
        "help" | "--help" | "-h" => {
            reject_unknown_flags(rest, &[])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Help,
                tier: CommandTier::Primary,
                default_output: DefaultOutputMode::Human,
            })
        }
        "tour" => {
            reject_unknown_flags(rest, &[])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Tour,
                tier: CommandTier::Primary,
                default_output: DefaultOutputMode::Human,
            })
        }
        "version" | "--version" | "-V" => {
            reject_unknown_flags(rest, &[])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Version,
                tier: CommandTier::Primary,
                default_output: DefaultOutputMode::Human,
            })
        }
        "check" => {
            let workspace = required_positional(subcommand, rest, 0, "<workspace>")?;
            let pretty = take_flag(rest, "--pretty");
            let verbose = take_flag(rest, "--verbose");
            let fail_on = parse_optional_fail_on(rest)?;
            let output_mode = resolve_output_mode(rest, DefaultOutputMode::Human);
            reject_unknown_flags(rest, &["--pretty", "--verbose", "--fail-on", "--json"])?;
            if let Some(unknown) = positional_arguments(rest).get(1) {
                return Err(UsageError::new(format!(
                    "未知参数 `{unknown}`。用法：audit-risk check <workspace> [--json] [--fail-on <level>]"
                )));
            }
            Ok(ParsedCliCommand {
                command: CliCommand::Check {
                    workspace,
                    pretty,
                    verbose,
                    fail_on,
                },
                tier: CommandTier::Primary,
                default_output: output_mode,
            })
        }
        "watch" => {
            let workspace = required_positional(subcommand, rest, 0, "<workspace>")?;
            let verbose = take_flag(rest, "--verbose");
            let jsonl = take_flag(rest, "--jsonl");
            let observe = take_flag(rest, "--observe");
            let fail_on = parse_optional_fail_on(rest)?;
            reject_unknown_flags(rest, &["--verbose", "--jsonl", "--observe", "--fail-on"])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Watch {
                    workspace,
                    verbose,
                    jsonl,
                    observe,
                    fail_on,
                },
                tier: CommandTier::Primary,
                default_output: DefaultOutputMode::Human,
            })
        }
        "diff" => {
            // diff compares two paths (old vs new). New users often expect it to
            // review the current git diff — steer them to `check .` instead of
            // dumping a bare "requires <after>".
            if take_flag(rest, "--help") || take_flag(rest, "-h") || positional_arguments(rest).len() < 2 {
                return Err(UsageError::new(
                    "用法：audit-risk diff <旧> <新>\n\
                     \n\
                     对比两个目录或文件的风险差异，<旧> 和 <新> 都是路径。\n\
                     例 1：audit-risk diff ./v1 ./v2\n\
                     例 2：audit-risk diff old.py new.py --json\n\
                     \n\
                     只想审查当前工作区/Git 改动？用 `audit-risk check .`。",
                ));
            }
            let before = required_positional(subcommand, rest, 0, "<before>")?;
            let after = required_positional(subcommand, rest, 1, "<after>")?;
            let pretty = take_flag(rest, "--pretty");
            let fail_on = parse_optional_fail_on(rest)?;
            let output_mode = resolve_output_mode(rest, DefaultOutputMode::Human);
            reject_unknown_flags(rest, &["--pretty", "--fail-on", "--json"])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Diff {
                    before,
                    after,
                    pretty,
                    fail_on,
                },
                tier: CommandTier::Primary,
                default_output: output_mode,
            })
        }
        "init" => {
            let workspace = required_positional(subcommand, rest, 0, "<workspace>")?;
            let force = take_flag(rest, "--force");
            let output_mode = resolve_output_mode(rest, DefaultOutputMode::Human);
            reject_unknown_flags(rest, &["--force", "--json"])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Init { workspace, force },
                tier: CommandTier::Primary,
                default_output: output_mode,
            })
        }
        "doctor" => {
            let output_mode = resolve_output_mode(rest, DefaultOutputMode::Human);
            reject_unknown_flags(rest, &["--json"])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Doctor {
                    workspace: optional_positional(rest, 0),
                },
                tier: CommandTier::Primary,
                default_output: output_mode,
            })
        }
        "report" => {
            let config = take_option(rest, "--config")?;
            let output = take_option(rest, "--output")?;
            let fail_on = take_option(rest, "--fail-on")?
                .map(|value| parse_fail_gate(&value))
                .transpose()?;
            let history_compare = take_flag(rest, "--history-compare");
            let output_mode = resolve_output_mode(rest, DefaultOutputMode::Human);
            reject_unknown_flags(rest, &["--config", "--output", "--fail-on", "--json", "--history-compare"])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Report {
                    workspace: optional_positional(rest, 0),
                    config,
                    output,
                    fail_on,
                    history_compare,
                },
                tier: CommandTier::Secondary,
                default_output: output_mode,
            })
        }
        "rules" => {
            let config = take_option(rest, "--config")?;
            reject_unknown_flags(rest, &["--config"])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Rules {
                    workspace: optional_positional(rest, 0),
                    config,
                },
                tier: CommandTier::Secondary,
                default_output: DefaultOutputMode::Json,
            })
        }
        "audit" => {
            let config = take_option(rest, "--config")?;
            let query = take_option(rest, "--query")?;
            let limit = take_option(rest, "--limit")?
                .map(|raw| {
                    raw.parse::<usize>()
                        .map_err(|_| UsageError::new("`--limit` must be a positive integer."))
                })
                .transpose()?;
            reject_unknown_flags(rest, &["--config", "--query", "--limit"])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Audit {
                    workspace: optional_positional(rest, 0),
                    config,
                    query,
                    limit,
                },
                tier: CommandTier::Secondary,
                default_output: DefaultOutputMode::Json,
            })
        }
        "verify" => {
            reject_unknown_flags(rest, &[])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Verify {
                    workspace: optional_positional(rest, 0),
                },
                tier: CommandTier::Secondary,
                default_output: DefaultOutputMode::Json,
            })
        }
        "observe" => {
            reject_unknown_flags(rest, &[])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Observe {
                    workspace: optional_positional(rest, 0),
                },
                tier: CommandTier::Secondary,
                default_output: DefaultOutputMode::Human,
            })
        }
        "notify" => {
            let webhook_url = take_option(rest, "--webhook-url")?;
            let test = take_flag(rest, "--test");
            let output_mode = resolve_output_mode(rest, DefaultOutputMode::Human);
            reject_unknown_flags(rest, &["--test", "--webhook-url", "--json"])?;
            if !test {
                return Err(UsageError::new("`notify` currently requires --test."));
            }
            Ok(ParsedCliCommand {
                command: CliCommand::Notify {
                    workspace: optional_positional(rest, 0),
                    test,
                    webhook_url,
                },
                tier: CommandTier::Secondary,
                default_output: output_mode,
            })
        }
        "auth" => {
            let action = match rest.first().map(String::as_str) {
                Some("login") => AuthAction::Login,
                Some("logout") => AuthAction::Logout,
                Some("status") => AuthAction::Status,
                Some(other) => {
                    return Err(UsageError::new(format!(
                        "`auth` 不认识 `{other}`。可用命令：audit-risk auth login / logout / status"
                    )));
                }
                None => {
                    return Err(UsageError::new(
                        "`auth` 需要一个动作：audit-risk auth login / logout / status",
                    ));
                }
            };
            reject_unknown_flags(&rest[1..], &[])?;
            Ok(ParsedCliCommand {
                command: CliCommand::Auth { action },
                tier: CommandTier::Secondary,
                default_output: DefaultOutputMode::Human,
            })
        }
        "repair" => {
            let subaction = rest.first().map(String::as_str);
            match subaction {
                Some("plan") => {
                    let rest2 = &rest[1..];
                    let workspace = required_positional("repair plan", rest2, 0, "<workspace>")?;
                    let finding_id = take_option(rest2, "--finding")?
                        .ok_or_else(|| UsageError::new("`repair plan` 需要 `--finding <finding_id>`。"))?;
                    let output_mode = resolve_output_mode(rest2, DefaultOutputMode::Json);
                    reject_unknown_flags(rest2, &["--finding", "--json"])?;
                    Ok(ParsedCliCommand {
                        command: CliCommand::RepairPlan { workspace, finding_id },
                        tier: CommandTier::Secondary,
                        default_output: output_mode,
                    })
                }
                Some("apply") => {
                    let rest2 = &rest[1..];
                    let workspace = required_positional("repair apply", rest2, 0, "<workspace>")?;
                    let plan_id = take_option(rest2, "--plan")?
                        .ok_or_else(|| UsageError::new("`repair apply` 需要 `--plan <plan_id>`。"))?;
                    let approve = rest2.iter().any(|argument| argument == "--approve");
                    let output_mode = resolve_output_mode(rest2, DefaultOutputMode::Json);
                    reject_unknown_flags(rest2, &["--plan", "--approve", "--json"])?;
                    Ok(ParsedCliCommand {
                        command: CliCommand::RepairApply { workspace, plan_id, approve },
                        tier: CommandTier::Secondary,
                        default_output: output_mode,
                    })
                }
                Some("approve") => {
                    let rest2 = &rest[1..];
                    let workspace = required_positional("repair approve", rest2, 0, "<workspace>")?;
                    let plan_id = take_option(rest2, "--plan")?
                        .ok_or_else(|| UsageError::new("`repair approve` 需要 `--plan <plan_id>`。"))?;
                    let output_mode = resolve_output_mode(rest2, DefaultOutputMode::Json);
                    reject_unknown_flags(rest2, &["--plan", "--json"])?;
                    Ok(ParsedCliCommand {
                        command: CliCommand::RepairApprove { workspace, plan_id },
                        tier: CommandTier::Secondary,
                        default_output: output_mode,
                    })
                }
                Some(other) => Err(UsageError::new(format!(
                    "`repair` 不认识 `{other}`。可用命令：audit-risk repair plan <workspace> --finding <id> / repair approve <workspace> --plan <id> / repair apply <workspace> --plan <id> [--approve]"
                ))),
                None => Err(UsageError::new(
                    "`repair` 需要一个动作：audit-risk repair plan / repair approve / repair apply",
                )),
            }
        }
        _ => Err(UsageError::new(format!(
            "不认识这个命令：`{subcommand}`。\n\n运行 `audit-risk help` 查看全部命令。"
        ))),
    }
}

pub fn build_structured_output_envelope(command: &str, status: &str, workspace_root: Option<&str>) -> Value {
    let mut object = Map::new();
    object.insert("schema_version".into(), Value::String(CLI_SCHEMA_VERSION.to_string()));
    object.insert("command".into(), Value::String(command.to_string()));
    object.insert("generated_at".into(), Value::String(now_iso()));
    object.insert("status".into(), Value::String(status.to_string()));
    if let Some(workspace) = workspace_root {
        object.insert("workspace_root".into(), Value::String(normalize_path(workspace)));
    }
    Value::Object(object)
}

pub fn usage_text() -> String {
    render_help_screen()
}

pub fn run_audit_risk_cli(args: Vec<String>) -> i32 {
    let parsed = match parse_cli_command(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{}", error.message());
            return 4;
        }
    };

    match execute_command(parsed) {
        Ok(outcome) => {
            if let Some(json) = outcome.stdout_json {
                let rendered = if outcome.pretty_json {
                    serde_json::to_string_pretty(&json)
                } else {
                    serde_json::to_string(&json)
                }
                .unwrap_or_else(|_| "{}".to_string());
                println!("{rendered}");
            }
            if let Some(text) = outcome.stdout_text {
                println!("{text}");
            }
            outcome.exit_code
        }
        Err(error) => {
            eprintln!("{}", error.message);
            error.exit_code
        }
    }
}

pub fn run_legacy_hologram_risk_check(args: Vec<String>) -> i32 {
    let mut workspace: Option<String> = None;
    let mut pretty = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--workspace" => {
                workspace = args.get(index + 1).cloned();
                index += 2;
            }
            "--pretty" => {
                pretty = true;
                index += 1;
            }
            value => {
                eprintln!("Unsupported legacy argument `{value}`.\n\nUse `audit-risk check <workspace>` instead.");
                return 4;
            }
        }
    }

    let Some(workspace) = workspace else {
        eprintln!("usage: audit-risk check <workspace> [--pretty]");
        return 4;
    };

    let mut args = vec!["check".to_string(), workspace];
    if pretty {
        args.push("--pretty".to_string());
    }
    run_audit_risk_cli(args)
}

fn execute_command(parsed: ParsedCliCommand) -> Result<CommandOutcome, CliRuntimeError> {
    let output_mode = parsed.default_output;
    match parsed.command {
        CliCommand::Home => run_home_command(),
        CliCommand::Help => Ok(CommandOutcome::text(0, usage_text())),
        CliCommand::Tour => Ok(CommandOutcome::text(0, tour_text())),
        CliCommand::Version => Ok(CommandOutcome::text(
            0,
            format!("audit-risk {}", env!("CARGO_PKG_VERSION")),
        )),
        CliCommand::Check {
            workspace,
            pretty,
            verbose,
            fail_on,
        } => run_check_command(&workspace, pretty, verbose, fail_on, output_mode),
        CliCommand::Watch {
            workspace,
            verbose,
            jsonl,
            observe,
            fail_on,
        } => run_watch_command(&workspace, verbose, jsonl, observe, fail_on),
        CliCommand::Diff {
            before,
            after,
            pretty,
            fail_on,
        } => run_diff_command(&before, &after, pretty, fail_on, output_mode),
        CliCommand::Init { workspace, force } => run_init_command(&workspace, force, output_mode),
        CliCommand::Doctor { workspace } => run_doctor_command(workspace.as_deref(), output_mode),
        CliCommand::Report {
            workspace,
            config,
            output,
            fail_on,
            history_compare,
        } => run_report_command_native(workspace, config, output, fail_on, history_compare, output_mode),
        CliCommand::Rules { workspace, config } => run_rules_command(workspace, config),
        CliCommand::Audit {
            workspace,
            config,
            query,
            limit,
        } => run_audit_command(workspace, config, query, limit),
        CliCommand::Verify { workspace: _ } => run_verify_command(),
        CliCommand::Notify {
            workspace,
            test,
            webhook_url,
        } => run_notify_command(workspace.as_deref(), test, webhook_url.as_deref(), output_mode),
        CliCommand::Observe { workspace } => run_observe_command(workspace.as_deref()),
        CliCommand::Auth { action } => run_auth_command(action),
        CliCommand::RepairPlan { workspace, finding_id } => {
            run_repair_plan_command(&workspace, &finding_id)
        }
        CliCommand::RepairApply { workspace, plan_id, approve } => {
            run_repair_apply_command(&workspace, &plan_id, approve)
        }
        CliCommand::RepairApprove { workspace, plan_id } => {
            run_repair_approve_command(&workspace, &plan_id)
        }
    }
}

fn run_home_command() -> Result<CommandOutcome, CliRuntimeError> {
    let cwd = std::env::current_dir()
        .map_err(|error| CliRuntimeError::environment(format!("无法读取当前目录：{error}")))?;
    Ok(CommandOutcome::text(0, render_home_screen(&cwd)))
}

fn run_auth_command(action: AuthAction) -> Result<CommandOutcome, CliRuntimeError> {
    match action {
        AuthAction::Status => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            Ok(CommandOutcome::text(
                0,
                render_auth_status_for_dir_with_workspace(&entitlement_dir(), &cwd),
            ))
        }
        AuthAction::Login => Ok(CommandOutcome::text(
            0,
            auth_login_text_for_dir_with_base_url(
                &entitlement_dir(),
                auth_base_url_for_workspace(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))).as_deref(),
            )?,
        )),
        AuthAction::Logout => {
            let dir = entitlement_dir();
            let mut removed = Vec::new();
            for file_name in ["entitlement.json", "entitlement.sig", "session.json"] {
                let path = dir.join(file_name);
                if path.exists() {
                    fs::remove_file(&path).map_err(|error| {
                        CliRuntimeError::environment(format!("退出登录失败，无法删除 {}：{error}", path.display()))
                    })?;
                    removed.push(file_name);
                }
            }
            let suffix = if removed.is_empty() {
                "本机没有发现已缓存的 Pro 授权。"
            } else {
                "已清除本机授权缓存，Core 免费功能仍可继续使用。"
            };
            Ok(CommandOutcome::text(
                0,
                render_product_shell(
                    &[
                        "当前视图：退出登录".to_string(),
                        "处理结果：已完成".to_string(),
                        "当前版本：Core 免费版".to_string(),
                    ],
                    &[format!("退出登录：{suffix}")],
                    &["退出登录只影响 Pro 授权缓存，不会影响你继续使用 Core 免费主路径。".to_string()],
                    &["如需重新开通，再运行 `audit-risk auth login`。".to_string()],
                    &[
                        "`audit-risk auth login`".to_string(),
                        "`audit-risk watch .`".to_string(),
                    ],
                    &[],
                ),
            ))
        }
    }
}

fn run_check_command(
    workspace: &str,
    pretty: bool,
    verbose: bool,
    fail_on: FailGate,
    output_mode: DefaultOutputMode,
) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_existing_workspace_path(workspace)?;
    let payload = build_workspace_check_payload(&workspace_path)?;
    let exit_code = gate_exit_code(payload["review"]["gate_decision"]["decision"].as_str(), fail_on);
    if output_mode == DefaultOutputMode::Json {
        let mut outcome = CommandOutcome::json(exit_code, payload);
        outcome.pretty_json = pretty;
        Ok(outcome)
    } else {
        Ok(CommandOutcome::text(exit_code, render_check_screen(&payload, verbose)?))
    }
}

fn run_watch_command(
    workspace: &str,
    verbose: bool,
    jsonl: bool,
    observe: bool,
    fail_on: FailGate,
) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_existing_workspace_path(workspace)?;
    if observe {
        ensure_pro_feature("observe", &workspace_path)?;
    }
    let mut stdout = io::stdout();
    let observe_runtime = if observe {
        Some(start_observe_runtime(&workspace_path)?)
    } else {
        None
    };
    if jsonl {
        emit_watch_event(
            &mut stdout,
            "session_started",
            Some(&workspace_path),
            json!({
                "mode": "jsonl",
                "observe": observe_runtime.as_ref().map(|runtime| runtime.observe_payload()),
            }),
        )?;
    } else {
        writeln!(
            stdout,
            "{}",
            render_watch_start_screen(&workspace_path, observe_runtime.as_ref())
        )
        .map_err(|error| CliRuntimeError::internal(format!("failed to write watch header: {error}")))?;
    }

    let mut emitted_findings: BTreeMap<String, u128> = BTreeMap::new();
    let mut previous_snapshot = workspace_snapshot(&workspace_path)?;

    loop {
        let payload = build_workspace_check_payload(&workspace_path)?;
        let now_ms = current_unix_millis();
        let findings = payload["review"]["findings"]
            .as_array()
            .ok_or_else(|| CliRuntimeError::internal("watch payload is missing review.findings"))?;
        let (visible_findings, suppressed_count) = filter_watch_findings_for_output(
            findings,
            now_ms,
            &emitted_findings,
            WATCH_DEBOUNCE_WINDOW_MS,
        );
        for finding in &visible_findings {
            emitted_findings.insert(build_watch_finding_key(finding), now_ms);
        }
        let exit_code = gate_exit_code(payload["review"]["gate_decision"]["decision"].as_str(), fail_on);
        if let Some(runtime) = observe_runtime.as_ref() {
            runtime.update(&payload);
        }

        if jsonl {
            emit_watch_event(
                &mut stdout,
                "check_completed",
                Some(&workspace_path),
                {
                    let mut clone = payload.clone();
                    if let Some(review) = clone.get_mut("review").and_then(Value::as_object_mut) {
                        review.insert("findings".into(), json!(visible_findings));
                    }
                    clone
                },
            )?;
            emit_watch_event(
                &mut stdout,
                if suppressed_count > 0 { "finding_suppressed" } else { "finding_emitted" },
                Some(&workspace_path),
                json!({
                    "finding_count": findings.len(),
                    "suppressed_count": suppressed_count,
                    "exit_code": exit_code,
                }),
            )?;
            emit_watch_event(
                &mut stdout,
                "gate_decided",
                Some(&workspace_path),
                payload["review"]["gate_decision"].clone(),
            )?;
        } else {
            let summary = render_watch_summary_human(
                &{
                    let mut clone = payload.clone();
                    if let Some(review) = clone.get_mut("review").and_then(Value::as_object_mut) {
                        review.insert("findings".into(), json!(visible_findings));
                    }
                    if suppressed_count > 0 {
                        clone.as_object_mut()
                            .expect("watch payload object")
                            .insert("suppressed_count".into(), json!(suppressed_count));
                    }
                    clone
                },
                verbose,
            )?;
            writeln!(stdout, "{summary}")
                .map_err(|error| CliRuntimeError::internal(format!("failed to write watch summary: {error}")))?;
        }
        stdout
            .flush()
            .map_err(|error| CliRuntimeError::internal(format!("failed to flush watch output: {error}")))?;

        loop {
            thread::sleep(Duration::from_millis(DEFAULT_POLL_INTERVAL_MS));
            let snapshot = workspace_snapshot(&workspace_path)?;
            if snapshot != previous_snapshot {
                previous_snapshot = snapshot;
                break;
            }
        }
    }
}

fn run_diff_command(
    before: &str,
    after: &str,
    pretty: bool,
    fail_on: FailGate,
    output_mode: DefaultOutputMode,
) -> Result<CommandOutcome, CliRuntimeError> {
    let before_path = resolve_existing_path(before)?;
    let after_path = resolve_existing_path(after)?;
    let payload = build_diff_payload(&before_path, &after_path)?;
    let exit_code = gate_exit_code(payload["review"]["gate_decision"]["decision"].as_str(), fail_on);
    if output_mode == DefaultOutputMode::Json {
        let mut outcome = CommandOutcome::json(exit_code, payload);
        outcome.pretty_json = pretty;
        Ok(outcome)
    } else {
        Ok(CommandOutcome::text(exit_code, render_diff_screen(&payload)?))
    }
}

fn run_init_command(
    workspace: &str,
    force: bool,
    output_mode: DefaultOutputMode,
) -> Result<CommandOutcome, CliRuntimeError> {
    let cwd = std::env::current_dir()
        .map_err(|error| CliRuntimeError::environment(format!("failed to determine current directory: {error}")))?;
    let workspace_path = resolve_workspace_argument(&cwd, workspace);
    if !workspace_path.exists() {
        return Err(CliRuntimeError::environment(format!(
            "workspace does not exist: {}",
            workspace_path.display()
        )));
    }
    let files = build_default_init_files(&workspace_path);
    let mut created = Vec::new();
    for (relative_path, content, executable) in files {
        let absolute_path = workspace_path.join(relative_path);
        if absolute_path.exists() && !force {
            return Err(CliRuntimeError::environment(format!(
                "refusing to overwrite existing file without --force: {}",
                absolute_path.display()
            )));
        }
        if let Some(parent) = absolute_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| CliRuntimeError::internal(format!("failed to create {}: {error}", parent.display())))?;
        }
        fs::write(&absolute_path, content)
            .map_err(|error| CliRuntimeError::internal(format!("failed to write {}: {error}", absolute_path.display())))?;
        if executable {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = fs::metadata(&absolute_path)
                    .map_err(|error| CliRuntimeError::internal(format!("failed to stat {}: {error}", absolute_path.display())))?
                    .permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&absolute_path, permissions)
                    .map_err(|error| CliRuntimeError::internal(format!("failed to chmod {}: {error}", absolute_path.display())))?;
            }
        }
        created.push(normalize_path(
            absolute_path
                .strip_prefix(&workspace_path)
                .unwrap_or(&absolute_path)
                .display()
                .to_string(),
        ));
    }
    // Activate the generated pre-commit hook. Writing `.githooks/pre-commit`
    // does nothing on its own — git ignores it until core.hooksPath points
    // there. Without this the "blocks risky commits" promise silently never
    // fires. We only set it when it's a git repo and hooksPath isn't already
    // pointing somewhere the user chose.
    let hook_activation = activate_pre_commit_hook(&workspace_path);
    let mut output = build_structured_output_envelope("init", "ok", Some(&workspace_path.display().to_string()));
    if let Some(object) = output.as_object_mut() {
        object.insert("created_files".into(), json!(created));
        object.insert("hook_activation".into(), json!(hook_activation));
    }
    if output_mode == DefaultOutputMode::Json {
        Ok(CommandOutcome::json(0, output))
    } else {
        Ok(CommandOutcome::text(0, render_init_screen(&output)?))
    }
}

/// Point git at `.githooks` so the generated pre-commit hook actually runs.
/// Returns a human-readable status for the init screen.
fn activate_pre_commit_hook(workspace_path: &Path) -> String {
    let is_git_repo = workspace_path.join(".git").exists();
    if !is_git_repo {
        return "未激活：当前目录不是 Git 仓库，`git init` 后运行 `git config core.hooksPath .githooks` 即可启用提交前拦截。".to_string();
    }
    let current = Command::new("git")
        .arg("-C")
        .arg(workspace_path)
        .args(["config", "--get", "core.hooksPath"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty());
    match current {
        Some(existing) if existing != ".githooks" => format!(
            "未改动：core.hooksPath 已指向 `{existing}`。如需启用 audit-risk 钩子，请手动合并或改为 `.githooks`。"
        ),
        _ => {
            let set = Command::new("git")
                .arg("-C")
                .arg(workspace_path)
                .args(["config", "core.hooksPath", ".githooks"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if set {
                "已激活：core.hooksPath 已设为 `.githooks`，提交前会自动运行 audit-risk check --fail-on block。".to_string()
            } else {
                "未激活：设置 core.hooksPath 失败，请手动运行 `git config core.hooksPath .githooks`。".to_string()
            }
        }
    }
}

fn run_doctor_command(
    workspace: Option<&str>,
    output_mode: DefaultOutputMode,
) -> Result<CommandOutcome, CliRuntimeError> {
    let cwd = std::env::current_dir()
        .map_err(|error| CliRuntimeError::environment(format!("failed to determine current directory: {error}")))?;
    let workspace_path = workspace
        .map(|value| resolve_workspace_argument(&cwd, value))
        .unwrap_or_else(|| default_workspace_root(&cwd));
    let normalized_workspace = normalize_path(workspace_path.display().to_string());

    let mut checks = Vec::new();
    let mut blockers = Vec::new();
    let mut notes = Vec::new();

    checks.push(json!({
        "name": "cli_version",
        "status": "ok",
        "detail": CLI_SCHEMA_VERSION,
    }));
    checks.push(json!({
        "name": "engine_version",
        "status": "ok",
        "detail": env!("CARGO_PKG_VERSION"),
    }));
    checks.push(json!({
        "name": "dependency_git",
        "status": command_exists("git"),
        "detail": "git",
    }));
    checks.push(json!({
        "name": "dependency_cargo",
        "status": command_exists("cargo"),
        "detail": "cargo",
    }));
    checks.push(json!({
        "name": "dependency_node",
        "status": command_exists("node"),
        "detail": "node",
    }));

    if workspace_path.exists() && workspace_path.is_dir() {
        checks.push(json!({
            "name": "workspace",
            "status": "ok",
            "detail": normalized_workspace,
        }));
    } else {
        blockers.push(format!("workspace is missing or not a directory: {}", workspace_path.display()));
        checks.push(json!({
            "name": "workspace",
            "status": "error",
            "detail": normalized_workspace,
        }));
    }

    let hologram_dir = workspace_path.join(".hologram");
    let hologram_dir_status = if hologram_dir.exists() || fs::create_dir_all(&hologram_dir).is_ok() {
        "ok"
    } else {
        blockers.push(format!(".hologram is missing or not writable: {}", hologram_dir.display()));
        "error"
    };
    checks.push(json!({
        "name": "hologram_dir",
        "status": hologram_dir_status,
        "detail": normalize_path(hologram_dir.display().to_string()),
    }));

    let delivery_config_path = workspace_path.join(".hologram/delivery.json");
    if delivery_config_path.exists() {
        match fs::read_to_string(&delivery_config_path) {
            Ok(raw) => match serde_json::from_str::<Value>(&raw) {
                Ok(_) => checks.push(json!({
                    "name": "delivery_config",
                    "status": "ok",
                    "detail": normalize_path(delivery_config_path.display().to_string()),
                })),
                Err(error) => {
                    blockers.push(format!("delivery config is not valid JSON: {error}"));
                    checks.push(json!({
                        "name": "delivery_config",
                        "status": "error",
                        "detail": normalize_path(delivery_config_path.display().to_string()),
                    }));
                }
            },
            Err(error) => {
                blockers.push(format!("failed to read delivery config: {error}"));
                checks.push(json!({
                    "name": "delivery_config",
                    "status": "error",
                    "detail": normalize_path(delivery_config_path.display().to_string()),
                }));
            }
        }
    } else {
        notes.push(format!("delivery config not found yet: {}", delivery_config_path.display()));
        checks.push(json!({
            "name": "delivery_config",
            "status": "needs_attention",
            "detail": normalize_path(delivery_config_path.display().to_string()),
        }));
    }

    for (name, relative_path) in [("review_rule_package", DEFAULT_REVIEW_RULE_PATH), ("repair_rule_package", DEFAULT_REPAIR_RULE_PATH)] {
        let path = workspace_path.join(relative_path);
        let mut check = json!({
            "name": name,
            "detail": normalize_path(path.display().to_string()),
        });
        match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<Value>(&raw) {
                Ok(value) => {
                    if let Some(object) = check.as_object_mut() {
                        object.insert("status".into(), json!("ok"));
                        if let Some(version) = value.get("version").and_then(Value::as_str) {
                            object.insert("version".into(), json!(version));
                        }
                        if let Some(package_id) = value.get("package_id").and_then(Value::as_str) {
                            object.insert("package_id".into(), json!(package_id));
                        }
                    }
                }
                Err(_) => {
                    blockers.push(format!("{name} is not valid JSON: {}", path.display()));
                    if let Some(object) = check.as_object_mut() {
                        object.insert("status".into(), json!("error"));
                    }
                }
            },
            Err(_) => {
                notes.push(format!("{name} not found yet: {}", path.display()));
                if let Some(object) = check.as_object_mut() {
                    object.insert("status".into(), json!("needs_attention"));
                }
            }
        }
        checks.push(check);
    }

    let provider_ready = delivery_config_path
        .exists()
        .then(|| fs::read_to_string(&delivery_config_path).ok())
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.get("provider").cloned());

    match provider_ready {
        Some(provider) => {
            let status = provider
                .get("key_source")
                .and_then(Value::as_str)
                .map(|source| match source {
                    "env" => {
                        let env_var = provider
                            .get("env_var")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        if env_var.is_empty() {
                            blockers.push("delivery.json 里 provider.env_var 为空，无法定位密钥来源。".to_string());
                            "error"
                        } else if std::env::var(env_var).ok().map(|value| !value.trim().is_empty()).unwrap_or(false) {
                            "ok"
                        } else {
                            notes.push(format!("Provider 密钥环境变量未设置：{env_var}。它用于调用模型生成白话解释；不配置时结构化审查仍可用，只是没有模型解释。设置方法：export {env_var}=<你的密钥>。"));
                            "needs_attention"
                        }
                    }
                    "secure_store" => "needs_attention",
                    _ => "error",
                })
                .unwrap_or("error");
            checks.push(json!({
                "name": "provider_config",
                "status": status,
                "detail": provider,
            }));
        }
        None => {
            notes.push("delivery.json 里没有找到 provider 配置。".to_string());
            checks.push(json!({
                "name": "provider_config",
                "status": "needs_attention",
                "detail": "missing",
            }));
        }
    }

    let audit_path = workspace_path.join(DEFAULT_AUDIT_JSONL_PATH);
    let audit_parent = audit_path.parent().map(Path::to_path_buf).unwrap_or_else(|| workspace_path.clone());
    let audit_status = if audit_parent.exists() || fs::create_dir_all(&audit_parent).is_ok() {
        "ok"
    } else {
        blockers.push(format!("audit path is not writable: {}", audit_path.display()));
        "error"
    };
    checks.push(json!({
        "name": "audit_path",
        "status": audit_status,
        "detail": normalize_path(audit_path.display().to_string()),
    }));

    let auth_service = auth_base_url_for_workspace(&workspace_path);
    if let Some(base_url) = auth_service.as_ref() {
        let auth_probe = auth_http_json("GET", &format!("{}/api/auth/poll?session_id=doctor-probe", base_url.trim_end_matches('/')), None);
        let (status, detail) = match auth_probe {
            Ok(_) => ("ok", json!(base_url)),
            Err(error) => (
                "error",
                json!({
                    "base_url": base_url,
                    "code": classify_auth_service_error(&error).code,
                    "message": error.message,
                }),
            ),
        };
        checks.push(json!({
            "name": "auth_service",
            "status": status,
            "detail": detail,
        }));
    } else {
        notes.push("未配置 auth 服务地址。仅 Pro 登录/授权需要，Core 免费功能不受影响；如需开通 Pro，在 delivery.json 的 auth.base_url 填入服务地址。".to_string());
        checks.push(json!({
            "name": "auth_service",
            "status": "needs_attention",
            "detail": "missing",
        }));
    }

    let entitlement_cache = load_entitlement_status();
    let entitlement_state = match entitlement_cache.state {
        EntitlementState::Active | EntitlementState::Grace => "ok",
        EntitlementState::Missing => "needs_attention",
        _ => "error",
    };
    checks.push(json!({
        "name": "entitlement_cache",
        "status": entitlement_state,
        "detail": {
            "path": normalize_path(entitlement_dir().display().to_string()),
            "state": format!("{:?}", entitlement_cache.state).to_lowercase(),
            "plan": entitlement_cache.plan,
            "payment_pending": entitlement_cache.payment_pending,
        }
    }));

    let status = if !blockers.is_empty() {
        "error"
    } else if notes.is_empty() {
        "ready"
    } else {
        "needs_attention"
    };
    let mut output = build_structured_output_envelope("doctor", status, Some(&workspace_path.display().to_string()));
    if let Some(object) = output.as_object_mut() {
        object.insert("checks".into(), json!(checks));
        object.insert("blockers".into(), json!(blockers));
        object.insert("notes".into(), json!(notes));
    }
    let exit_code = if status == "error" { 3 } else { 0 };
    if output_mode == DefaultOutputMode::Json {
        Ok(CommandOutcome::json(exit_code, output))
    } else {
        Ok(CommandOutcome::text(exit_code, render_doctor_screen(&output)?))
    }
}

fn run_notify_command(
    workspace: Option<&str>,
    test: bool,
    webhook_url: Option<&str>,
    output_mode: DefaultOutputMode,
) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = workspace.map(PathBuf::from).unwrap_or(std::env::current_dir().map_err(|error| {
        CliRuntimeError::environment(format!("failed to determine current directory: {error}"))
    })?);
    ensure_pro_feature("notify", &workspace_path)?;
    if !test {
        return Err(CliRuntimeError::environment("notify currently only supports --test."));
    }
    let resolved_webhook = resolve_webhook_url(&workspace_path, webhook_url)?;
    let payload = json!({
        "event": "audit-risk.notify_test",
        "generated_at": now_iso(),
        "workspace_root": normalize_path(workspace_path.display().to_string()),
    });
    let test_result = send_webhook_test(&resolved_webhook, &payload)?;
    let mut output = build_structured_output_envelope("notify", if test_result.ok { "ok" } else { "error" }, Some(&workspace_path.display().to_string()));
    if let Some(object) = output.as_object_mut() {
        object.insert("tested_url".into(), json!(resolved_webhook));
        object.insert("http_status".into(), json!(test_result.http_status));
        object.insert("ok".into(), json!(test_result.ok));
    }
    let exit_code = if test_result.ok { 0 } else { 3 };
    if output_mode == DefaultOutputMode::Json {
        Ok(CommandOutcome::json(exit_code, output))
    } else {
        Ok(CommandOutcome::text(exit_code, render_notify_screen(&output)?))
    }
}

fn run_report_command_native(
    workspace: Option<String>,
    config_path: Option<String>,
    output_path: Option<String>,
    fail_on: Option<FailGate>,
    history_compare: bool,
    output_mode: DefaultOutputMode,
) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace = resolve_workspace_or_cwd(workspace.as_deref())?;
    if history_compare {
        ensure_pro_feature("history_compare", &workspace)?;
    }
    let config = effective_delivery_config(&workspace, config_path.as_deref())?;
    let review_policy = resolve_rule_policy(&workspace, Some(&config), "review")?;
    let repair_policy = resolve_rule_policy(&workspace, Some(&config), "repair")?;
    let mut check = build_workspace_check_payload(&workspace)?;
    apply_report_review_policy(&mut check, &review_policy);
    let audit_path = delivery_path(&workspace, config.pointer("/audit/jsonl_path").and_then(Value::as_str), DEFAULT_AUDIT_JSONL_PATH);
    let recent_limit = config.pointer("/audit/recent_limit").and_then(Value::as_u64).unwrap_or(200) as usize;
    let audit_entries = read_recent_audit_jsonl(&audit_path, recent_limit);
    let policies = json!({"review": review_policy, "repair": repair_policy});
    let threshold = fail_on.unwrap_or_else(|| {
        config
            .pointer("/automation/fail_on_decision")
            .and_then(Value::as_str)
            .and_then(|value| parse_fail_gate(value).ok())
            .unwrap_or(FailGate::Block)
    });
    let report_output_path = config
        .pointer("/audit/report_output_path")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_REPORT_OUTPUT_PATH);
    let target_path = output_path
        .as_deref()
        .map(PathBuf::from)
        .map(|path| if path.is_absolute() { path } else { workspace.join(path) })
        .unwrap_or_else(|| delivery_path(&workspace, Some(report_output_path), DEFAULT_REPORT_OUTPUT_PATH));
    let mut automation = config.get("automation").cloned().unwrap_or_else(|| json!({}));
    automation["fail_on_decision"] = json!(fail_gate_to_str(threshold));
    automation["should_fail"] = json!(gate_exit_code(
        check.pointer("/review/gate_decision/decision").and_then(Value::as_str),
        threshold,
    ) == 2);
    let report_without_signature = json!({
        "generated_at": now_iso(),
        "workspace": {
            "root": normalize_path(workspace.display().to_string()),
            "changed_files_source": config.pointer("/workspace/changed_files_source").and_then(Value::as_str).unwrap_or("git_status"),
            "audit_jsonl_path": config.pointer("/audit/jsonl_path").and_then(Value::as_str).unwrap_or(DEFAULT_AUDIT_JSONL_PATH),
            "report_output_path": report_output_path,
        },
        "provider": build_delivery_provider_status(&config),
        "policies": policies,
        "current_review": build_current_review_report(&check, &audit_entries),
        "audit": build_delivery_audit_report(audit_entries),
        "automation": automation,
    });
    let mut report = report_without_signature;
    report["report_signature"] = json!({
        "algorithm": "sha256",
        "digest": sha256_hex(&serde_json::to_vec(&report).expect("report must serialize")),
    });
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CliRuntimeError::environment(format!("无法创建报告目录 {}：{error}", parent.display()))
        })?;
    }
    fs::write(&target_path, serde_json::to_vec_pretty(&report).expect("report must serialize")).map_err(|error| {
        CliRuntimeError::environment(format!("无法写入报告文件 {}：{error}", target_path.display()))
    })?;
    let exit_code = gate_exit_code(
        check.pointer("/review/gate_decision/decision").and_then(Value::as_str),
        threshold,
    );
    if output_mode == DefaultOutputMode::Json {
        Ok(CommandOutcome::json(exit_code, report))
    } else {
        let rendered = if history_compare {
            render_history_compare_screen(&report)?
        } else {
            render_report_screen(&report)?
        };
        Ok(CommandOutcome::text(exit_code, rendered))
    }
}

fn default_delivery_config(workspace: &Path) -> Value {
    json!({
        "version": "phase5.v1",
        "workspace": { "root": normalize_path(workspace.display().to_string()), "changed_files_source": "git_status" },
        "provider": {
            "name": "deepseek",
            "model": "deepseek-v4-pro",
            "base_url": "https://api.deepseek.com",
            "key_source": "env",
            "env_var": "DEEPSEEK_API_KEY",
        },
        "rule_packages": {
            "review_paths": [], "repair_paths": [],
            "disabled_review_rule_ids": [], "disabled_repair_rule_ids": [],
        },
        "audit": {
            "jsonl_path": DEFAULT_AUDIT_JSONL_PATH,
            "report_output_path": DEFAULT_REPORT_OUTPUT_PATH,
            "recent_limit": 20,
        },
        "auth": { "base_url": "" },
        "automation": {
            "verify_commands": ["audit-risk check . --fail-on block", "audit-risk doctor ."],
            "pre_commit_hook": DEFAULT_PRE_COMMIT_PATH,
            "ci_workflow": DEFAULT_CI_WORKFLOW_PATH,
            "fail_on_decision": "block",
        },
    })
}

fn delivery_path(workspace: &Path, configured_path: Option<&str>, default_path: &str) -> PathBuf {
    let path = PathBuf::from(configured_path.unwrap_or(default_path));
    if path.is_absolute() { path } else { workspace.join(path) }
}

fn read_recent_audit_jsonl(path: &Path, limit: usize) -> Vec<Value> {
    let mut entries = read_audit_jsonl(path);
    let start = entries.len().saturating_sub(limit);
    entries.drain(..start);
    entries
}

fn build_delivery_provider_status(config: &Value) -> Value {
    let provider = config.get("provider").cloned().unwrap_or_else(|| json!({}));
    let key_source = provider.get("key_source").and_then(Value::as_str).unwrap_or("env");
    let env_var = provider.get("env_var").and_then(Value::as_str).unwrap_or("DEEPSEEK_API_KEY");
    let ready = key_source != "env" || std::env::var(env_var).map(|value| !value.trim().is_empty()).unwrap_or(false);
    json!({
        "name": provider.get("name").and_then(Value::as_str).unwrap_or("deepseek"),
        "model": provider.get("model").and_then(Value::as_str).unwrap_or("deepseek-v4-pro"),
        "base_url": provider.get("base_url").and_then(Value::as_str).unwrap_or("https://api.deepseek.com"),
        "key_source": key_source,
        "ready": ready,
        "reason": if ready { "Provider configuration is ready.".to_string() } else { format!("Provider key is missing from {env_var}.") },
        "env_var": if key_source == "env" { Value::String(env_var.to_string()) } else { Value::Null },
    })
}

fn build_current_review_report(check: &Value, audit_entries: &[Value]) -> Value {
    let review = check.get("review").cloned().unwrap_or_else(|| json!({}));
    let findings = review.get("findings").and_then(Value::as_array).cloned().unwrap_or_default();
    let gate = review.get("gate_decision").cloned().unwrap_or_else(|| json!({}));
    let repair_history = normalize_delivery_audit_records(audit_entries)
        .into_iter()
        .filter(|record| record.get("plane").and_then(Value::as_str) == Some("repair"))
        .take(5)
        .map(|record| json!({
            "timestamp": record.get("timestamp").cloned().unwrap_or(Value::Null),
            "stage": record.get("stage").cloned().unwrap_or(Value::Null),
            "status": record.get("status").cloned().unwrap_or(Value::Null),
            "subject": record.get("subject").cloned().unwrap_or(Value::Null),
            "reason": record.get("reason").cloned().unwrap_or(Value::Null),
            "state_change": record.get("state_change").cloned().unwrap_or(Value::Null),
            "error": record.get("error").cloned().unwrap_or(Value::Null),
        }))
        .collect::<Vec<_>>();
    json!({
        "status": "ok",
        "review": review,
        "gate_decision": gate,
        "workbench_queue": [
            {"step_id": "review", "state": if findings.is_empty() { "clean" } else { "needs_attention" }, "summary": format!("{} 条风险待处理", findings.len())},
            {"step_id": "gate", "state": gate.get("decision").cloned().unwrap_or_else(|| json!("allow")), "summary": gate.get("reason").cloned().unwrap_or(Value::Null)},
        ],
        "repair_history": repair_history,
        "repair_workbench": {"status_state": if findings.is_empty() { "not_required" } else { "waiting_approval" }, "test_count": 0},
    })
}

fn apply_report_review_policy(check: &mut Value, policy: &Value) {
    let reason = check
        .get("one_line")
        .and_then(Value::as_str)
        .unwrap_or("No enabled rule requires intervention.")
        .to_string();
    let Some(review) = check.get_mut("review").and_then(Value::as_object_mut) else {
        return;
    };
    let findings = review.get("findings").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut effects = BTreeMap::new();
    for rule in policy.get("rules").and_then(Value::as_array).into_iter().flatten() {
        if let (Some(rule_id), Some(effect)) = (
            rule.get("rule_id").and_then(Value::as_str),
            rule.get("gate_effect").and_then(Value::as_str),
        ) {
            effects.insert(rule_id, effect);
        }
    }
    let decision = findings
        .iter()
        .filter_map(|finding| finding.get("rule_id").and_then(Value::as_str))
        .filter_map(|rule_id| effects.get(rule_id).copied())
        .max_by_key(|effect| match *effect {
            "block" => 3,
            "require_approval" => 2,
            "warn" => 1,
            _ => 0,
        })
        .unwrap_or("allow");
    let finding_ids = findings
        .iter()
        .filter_map(|finding| finding.get("finding_id").cloned())
        .collect::<Vec<_>>();
    review.insert(
        "gate_decision".to_string(),
        json!({
            "decision": decision,
            "reason": reason,
            "finding_count": finding_ids.len(),
            "finding_ids": finding_ids,
            "subject_ref": "workspace",
            "policy_snapshot_id": policy.get("policy_snapshot_id").cloned().unwrap_or(Value::Null),
        }),
    );
}

fn build_delivery_audit_report(entries: Vec<Value>) -> Value {
    let records = normalize_delivery_audit_records(&entries);
    let integrity = build_delivery_audit_integrity(&entries);
    json!({
        "entries": entries,
        "records": records,
        "integrity": integrity,
    })
}

fn run_verify_command() -> Result<CommandOutcome, CliRuntimeError> {
    Ok(CommandOutcome::text(
        1,
        "`audit-risk verify` 是开发环境 CI 工具，不作为独立发行命令。\n若要检查代码风险，请使用：audit-risk check <workspace>",
    ))
}

fn run_audit_command(
    workspace: Option<String>,
    config_path: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_workspace_or_cwd(workspace.as_deref())?;
    let config = if config_path.is_some() {
        Some(effective_delivery_config(&workspace_path, config_path.as_deref())?)
    } else {
        load_delivery_config(&workspace_path, None)?
    };
    let query = query.unwrap_or_default();
    let normalized_query = query.trim().to_lowercase();
    let audit_path = delivery_path(
        &workspace_path,
        config
            .as_ref()
            .and_then(|value| value.pointer("/audit/jsonl_path"))
            .and_then(Value::as_str),
        DEFAULT_AUDIT_JSONL_PATH,
    );
    let configured_limit = config
        .as_ref()
        .and_then(|value| value.pointer("/audit/recent_limit"))
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(20);
    let mut records = normalize_delivery_audit_records(&read_audit_jsonl(&audit_path));
    if records.iter().all(|record| record.get("timestamp").and_then(Value::as_str).unwrap_or_default().is_empty()) {
        records.reverse();
    }
    let matches_query = |record: &&Value| {
        normalized_query.is_empty() || audit_record_haystack(record).contains(&normalized_query)
    };
    let total_matches = records.iter().filter(matches_query).count();
    let records = records
        .iter()
        .filter(matches_query)
        .take(limit.unwrap_or(configured_limit))
        .cloned()
        .collect::<Vec<_>>();
    Ok(CommandOutcome::json(0, json!({
        "query": query,
        "total_matches": total_matches,
        "records": records,
    })))
}

fn resolve_workspace_or_cwd(workspace: Option<&str>) -> Result<PathBuf, CliRuntimeError> {
    match workspace {
        Some(path) => resolve_existing_workspace_path(path),
        None => {
            let cwd = std::env::current_dir()
                .map_err(|error| CliRuntimeError::environment(format!("failed to determine current directory: {error}")))?;
            let workspace = default_workspace_root(&cwd);
            if workspace.exists() {
                Ok(workspace)
            } else {
                Err(CliRuntimeError::environment(format!("workspace does not exist: {}", workspace.display())))
            }
        }
    }
}

fn read_audit_jsonl(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let raw_line = line.trim();
            let mut entry = serde_json::from_str::<Value>(raw_line).ok()?;
            if let Some(object) = entry.as_object_mut() {
                object.entry("raw_line".to_string()).or_insert_with(|| json!(raw_line));
            }
            Some(entry)
        })
        .collect()
}

fn audit_record_haystack(record: &Value) -> String {
    [
        record.get("plane").and_then(Value::as_str).unwrap_or_default(),
        record.get("stage").and_then(Value::as_str).unwrap_or_default(),
        record.get("status").and_then(Value::as_str).unwrap_or_default(),
        record.get("subject").and_then(Value::as_str).unwrap_or_default(),
        record.get("reason").and_then(Value::as_str).unwrap_or_default(),
        record.pointer("/error/code").and_then(Value::as_str).unwrap_or_default(),
    ]
    .join(" ")
    .to_lowercase()
}

fn normalize_delivery_audit_records(entries: &[Value]) -> Vec<Value> {
    let mut records = entries
        .iter()
        .filter_map(normalize_delivery_audit_record)
        .collect::<Vec<_>>();
    records.sort_by(|left, right| {
        right
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(left.get("timestamp").and_then(Value::as_str).unwrap_or_default())
    });
    records
}

fn normalize_delivery_audit_record(entry: &Value) -> Option<Value> {
    if entry.get("plane").is_some() && entry.get("tool").is_none() && entry.get("event_type").is_none() {
        return Some(entry.clone());
    }
    let tool = entry
        .get("tool")
        .or_else(|| entry.get("event_type"))
        .and_then(Value::as_str)?;
    if tool != "review_check" && !tool.starts_with("approval.") && !tool.starts_with("repair_") {
        return None;
    }
    let timestamp = entry
        .get("ts")
        .or_else(|| entry.get("timestamp"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let details = entry.get("details").and_then(Value::as_object);
    let gate_decision = details.and_then(|details| details.get("gate_decision"));
    let plane = if tool == "review_check" {
        "review"
    } else if tool.starts_with("approval.") {
        "approval"
    } else {
        "repair"
    };
    let stage = if tool == "review_check" {
        "review"
    } else if tool.starts_with("approval.") || tool == "repair_approval" || tool == "repair_approved" {
        "approval"
    } else if tool == "repair_rollback" || tool == "repair_rolled_back" {
        "rollback"
    } else if tool == "repair_plan" || tool == "repair_planned" {
        "proposal_generation"
    } else {
        "apply"
    };
    let action = entry.get("action").and_then(Value::as_str).unwrap_or("allowed");
    let status = if let Some(error) = details.and_then(|details| details.get("error_code")).and_then(Value::as_str) {
        if details.and_then(|details| details.get("error_retryable")).and_then(Value::as_bool) == Some(true) {
            "degraded".to_string()
        } else {
            let _ = error;
            "failed".to_string()
        }
    } else if tool == "review_check" {
        gate_decision
            .and_then(|gate| gate.get("decision"))
            .and_then(Value::as_str)
            .unwrap_or(if action == "allowed" { "allow" } else { "block" })
            .to_string()
    } else if tool.starts_with("approval.") || tool == "repair_approved" {
        if action == "allowed" { "approved" } else { "rejected" }.to_string()
    } else if tool == "repair_applied" {
        "applied".to_string()
    } else if tool == "repair_rolled_back" {
        "rolled_back".to_string()
    } else {
        if action == "allowed" { "allow" } else { "block" }.to_string()
    };
    let subject = details
        .and_then(|details| details.get("subject"))
        .or_else(|| details.and_then(|details| details.get("patch_proposal_id")))
        .or_else(|| entry.get("path"))
        .or_else(|| entry.get("subject_ref"))
        .and_then(Value::as_str)
        .unwrap_or("workspace");
    let finding_ids = details
        .and_then(|details| details.get("finding_ids"))
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_else(|| entry.get("finding_id").and_then(Value::as_str).map(|value| vec![value]).unwrap_or_default());
    let evidence_ids = details
        .and_then(|details| details.get("evidence_ids"))
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    let error = details.and_then(|details| details.get("error_code")).and_then(Value::as_str).map(|code| {
        json!({
            "code": code,
            "stage": details.and_then(|details| details.get("error_stage")).cloned().unwrap_or(Value::Null),
            "retryable": details.and_then(|details| details.get("error_retryable")).cloned().unwrap_or(Value::Null),
        })
    });
    Some(json!({
        "event_id": format!("{tool}:{timestamp}:{subject}"),
        "timestamp": timestamp,
        "plane": plane,
        "stage": stage,
        "status": status,
        "subject": subject,
        "reason": entry.get("reason").and_then(Value::as_str).unwrap_or_default(),
        "finding_ids": finding_ids,
        "evidence_ids": evidence_ids,
        "policy_snapshot_id": details.and_then(|details| details.get("policy_snapshot_id")).cloned().unwrap_or(Value::Null),
        "state_change": details.and_then(|details| details.get("state_change")).cloned().unwrap_or(Value::Null),
        "error": error.unwrap_or(Value::Null),
        "raw": entry,
    }))
}

fn build_delivery_audit_integrity(entries: &[Value]) -> Value {
    if entries.is_empty() {
        return json!({"status": "empty", "verified": true, "entry_count": 0, "chained_entry_count": 0, "legacy_entry_count": 0, "issues": []});
    }
    let mut issues = Vec::new();
    let mut chained_entry_count = 0usize;
    let mut legacy_entry_count = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        let Some(actual_hash) = entry.get("integrity_hash").and_then(Value::as_str) else {
            legacy_entry_count += 1;
            continue;
        };
        chained_entry_count += 1;
        let tool = entry.get("tool").and_then(Value::as_str).unwrap_or("unknown");
        let timestamp = entry.get("ts").and_then(Value::as_str).unwrap_or_default();
        let Some(payload) = canonical_audit_integrity_payload(entry) else {
            issues.push(format!("{tool}@{timestamp} cannot reconstruct integrity payload."));
            continue;
        };
        if actual_hash != sha256_hex(payload.as_bytes()) {
            issues.push(format!("{tool}@{timestamp} has mismatched integrity_hash."));
        }
        let expected_prev_hash = if index == 0 {
            None
        } else {
            entries[index - 1]
                .get("integrity_hash")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| entries[index - 1].get("raw_line").and_then(Value::as_str).map(|line| sha256_hex(line.as_bytes())))
        };
        if entry.get("prev_hash").and_then(Value::as_str) != expected_prev_hash.as_deref() {
            issues.push(format!("{tool}@{timestamp} has mismatched prev_hash."));
        }
    }
    let last_hash = entries.iter().rev().find_map(|entry| entry.get("integrity_hash")).cloned().unwrap_or(Value::Null);
    let verified = issues.is_empty();
    json!({
        "status": if !verified { "failed" } else if legacy_entry_count > 0 { "legacy_anchor" } else { "verified" },
        "verified": verified,
        "entry_count": entries.len(),
        "chained_entry_count": chained_entry_count,
        "legacy_entry_count": legacy_entry_count,
        "last_hash": last_hash,
        "issues": issues,
    })
}

fn canonical_audit_integrity_payload(entry: &Value) -> Option<String> {
    let string = |key: &str| serde_json::to_string(entry.get(key)?.as_str()?).ok();
    let ts = string("ts")?;
    let tool = string("tool")?;
    let path = string("path")?;
    let action = string("action")?;
    let reason = string("reason")?;
    let details = entry.get("details").map(serde_json::to_string).transpose().ok().flatten();
    let prev_hash = entry.get("prev_hash").cloned().unwrap_or(Value::Null);
    let mut payload = format!("{{\"ts\":{ts},\"tool\":{tool},\"path\":{path},\"action\":{action},\"reason\":{reason}");
    if let Some(details) = details {
        payload.push_str(&format!(",\"details\":{details}"));
    }
    payload.push_str(&format!(",\"prev_hash\":{prev_hash}}}"));
    Some(payload)
}

fn run_rules_command(workspace: Option<String>, config: Option<String>) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_workspace_or_cwd(workspace.as_deref())?;
    let config = if config.is_some() {
        Some(effective_delivery_config(&workspace_path, config.as_deref())?)
    } else {
        load_delivery_config(&workspace_path, None)?
    };
    let summaries = ["review", "repair"]
        .into_iter()
        .map(|plane| summarize_rule_policy(&workspace_path, config.as_ref(), plane))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CommandOutcome::json(0, Value::Array(summaries)))
}

fn load_delivery_config(workspace: &Path, config_path: Option<&str>) -> Result<Option<Value>, CliRuntimeError> {
    let path = match config_path {
        Some(path) => {
            let candidate = PathBuf::from(path);
            if candidate.is_absolute() { candidate } else { workspace.join(candidate) }
        }
        None => workspace.join(".hologram/delivery.json"),
    };
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str(&raw)
            .map(Some)
            .map_err(|error| CliRuntimeError::environment(format!("delivery config is not valid JSON {}: {error}", path.display()))),
        Err(error) if config_path.is_none() && error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CliRuntimeError::environment(format!("cannot read delivery config {}: {error}", path.display()))),
    }
}

fn effective_delivery_config(workspace: &Path, config_path: Option<&str>) -> Result<Value, CliRuntimeError> {
    let mut defaults = default_delivery_config(workspace);
    if let Some(config) = load_delivery_config(workspace, config_path)? {
        merge_json_objects(&mut defaults, config);
    }
    if config_path.is_some() {
        validate_delivery_config(&defaults)?;
    }
    Ok(defaults)
}

fn validate_delivery_config(config: &Value) -> Result<(), CliRuntimeError> {
    let require_text = |pointer: &str, name: &str| {
        config.pointer(pointer).and_then(Value::as_str).filter(|value| !value.trim().is_empty()).ok_or_else(|| {
            CliRuntimeError::environment(format!("delivery config requires {name}."))
        })
    };
    if config.pointer("/version").and_then(Value::as_str) != Some("phase5.v1") {
        return Err(CliRuntimeError::environment("unsupported delivery config version."));
    }
    require_text("/workspace/root", "a workspace root")?;
    require_text("/provider/name", "provider name")?;
    require_text("/provider/model", "provider model")?;
    require_text("/provider/base_url", "provider base URL")?;
    if config.pointer("/provider/key_source").and_then(Value::as_str) == Some("env") {
        require_text("/provider/env_var", "provider.env_var when key_source is env")?;
    }
    require_text("/audit/jsonl_path", "audit jsonl path")?;
    require_text("/audit/report_output_path", "audit report output path")?;
    if config.pointer("/audit/recent_limit").and_then(Value::as_u64).unwrap_or_default() == 0 {
        return Err(CliRuntimeError::environment("delivery config requires audit.recent_limit > 0."));
    }
    if config.pointer("/auth/base_url").and_then(Value::as_str).is_none() {
        return Err(CliRuntimeError::environment("delivery config requires auth.base_url."));
    }
    if config.pointer("/automation/verify_commands").and_then(Value::as_array).is_none_or(Vec::is_empty) {
        return Err(CliRuntimeError::environment("delivery config requires at least one verification command."));
    }
    config.pointer("/automation/fail_on_decision")
        .and_then(Value::as_str)
        .ok_or_else(|| CliRuntimeError::environment("delivery config requires automation.fail_on_decision."))
        .and_then(|value| parse_fail_gate(value).map_err(|_| CliRuntimeError::environment("delivery config has an invalid automation.fail_on_decision.")))?;
    Ok(())
}

fn merge_json_objects(target: &mut Value, source: Value) {
    match (target, source) {
        (Value::Object(target), Value::Object(source)) => {
            for (key, value) in source {
                match target.get_mut(&key) {
                    Some(existing) => merge_json_objects(existing, value),
                    None => { target.insert(key, value); }
                }
            }
        }
        (target, source) => *target = source,
    }
}

fn summarize_rule_policy(workspace: &Path, config: Option<&Value>, plane: &str) -> Result<Value, CliRuntimeError> {
    let policy = resolve_rule_policy(workspace, config, plane)?;
    let rules = policy.get("rules").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(json!({
        "plane": plane,
        "policy_snapshot_id": policy.get("policy_snapshot_id").cloned().unwrap_or(Value::Null),
        "package_ids": policy.get("packages").and_then(Value::as_array)
            .map(|packages| packages.iter().filter_map(|package| package.get("package_id").cloned()).collect::<Vec<_>>())
            .unwrap_or_default(),
        "rule_count": rules.len(),
        "top_rule_ids": rules.iter().take(5).filter_map(|rule| rule.get("rule_id").cloned()).collect::<Vec<_>>(),
    }))
}

fn resolve_rule_policy(workspace: &Path, config: Option<&Value>, plane: &str) -> Result<Value, CliRuntimeError> {
    let mut packages = vec![default_rule_package(plane)];
    let paths_key = if plane == "review" { "review_paths" } else { "repair_paths" };
    let disabled_key = if plane == "review" { "disabled_review_rule_ids" } else { "disabled_repair_rule_ids" };
    let disabled = config
        .and_then(|value| value.pointer(&format!("/rule_packages/{disabled_key}")))
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect::<BTreeSet<_>>())
        .unwrap_or_default();
    if let Some(paths) = config
        .and_then(|value| value.pointer(&format!("/rule_packages/{paths_key}")))
        .and_then(Value::as_array)
    {
        for path in paths.iter().filter_map(Value::as_str) {
            let path = PathBuf::from(path);
            let path = if path.is_absolute() { path } else { workspace.join(path) };
            let raw = fs::read_to_string(&path).map_err(|error| {
                CliRuntimeError::environment(format!("无法读取 {plane} 规则包 {}：{error}", path.display()))
            })?;
            let package: Value = serde_json::from_str(&raw).map_err(|error| {
                CliRuntimeError::environment(format!("{plane} 规则包不是有效 JSON {}：{error}", path.display()))
            })?;
            let package_entries = package.as_array().cloned().unwrap_or_else(|| vec![package]);
            for package in package_entries {
                if package.get("enabled").and_then(Value::as_bool).unwrap_or(false)
                    && package.get("plane").and_then(Value::as_str) == Some(plane)
                {
                    packages.push(package);
                }
            }
        }
    }
    let mut rules = BTreeMap::new();
    for package in &packages {
        if let Some(entries) = package.get("rules").and_then(Value::as_array) {
            for rule in entries {
                let rule_id = rule.get("rule_id").and_then(Value::as_str).unwrap_or_default();
                if !rule_id.is_empty()
                    && rule.get("enabled").and_then(Value::as_bool).unwrap_or(true)
                    && !disabled.contains(rule_id)
                {
                    rules.insert(rule_id.to_string(), rule.clone());
                }
            }
        }
    }
    let mut rules = rules.into_values().collect::<Vec<_>>();
    rules.sort_by(|left, right| {
        right.get("priority").and_then(Value::as_i64).unwrap_or_default()
            .cmp(&left.get("priority").and_then(Value::as_i64).unwrap_or_default())
            .then_with(|| left.get("rule_id").and_then(Value::as_str).cmp(&right.get("rule_id").and_then(Value::as_str)))
    });
    let snapshot = format!(
        "policy:{plane}:{}",
        packages.iter().map(|package| format!("{}@{}", package.get("package_id").and_then(Value::as_str).unwrap_or("unknown"), package.get("version").and_then(Value::as_str).unwrap_or("unknown"))).collect::<Vec<_>>().join("+")
    );
    Ok(json!({
        "plane": plane,
        "packages": packages,
        "rules": rules,
        "policy_snapshot_id": snapshot,
    }))
}

fn default_rule_package(plane: &str) -> Value {
    let rules = match plane {
        "review" => vec![
            ("check.l5", 100, "block"),
            ("check.l4", 80, "require_approval"),
            ("check.l3", 60, "warn"),
            ("check.l2", 40, "warn"),
        ],
        _ => vec![
            ("repair.test.required_command_failed", 110, "block"),
            ("repair.scope.out_of_scope_write", 100, "block"),
            ("repair.scope.absolute_path_write", 95, "block"),
            ("repair.scope.sensitive_path_write", 90, "block"),
            ("repair.scope.duplicate_file_write", 50, "warn"),
            ("repair.scope.large_patch_blast_radius", 45, "warn"),
        ],
    };
    json!({
        "package_id": format!("{plane}.default"),
        "version": "v1",
        "plane": plane,
        "enabled": true,
        "rules": rules.into_iter().map(|(rule_id, priority, gate_effect)| json!({"rule_id": rule_id, "priority": priority, "gate_effect": gate_effect, "enabled": true})).collect::<Vec<_>>(),
    })
}

fn run_repair_plan_command(workspace: &str, finding_id: &str) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_existing_workspace_path(workspace)?;

    // Load delivery.json to find provider config — user must have run `audit-risk init` first.
    let delivery_path = workspace_path.join(".hologram/delivery.json");
    let delivery: serde_json::Value = fs::read_to_string(&delivery_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .ok_or_else(|| {
            CliRuntimeError::environment(
                "找不到 .hologram/delivery.json。请先运行 `audit-risk init <workspace>` 完成初始化。".to_string(),
            )
        })?;

    let provider = delivery.get("provider").ok_or_else(|| {
        CliRuntimeError::environment("delivery.json 缺少 provider 配置，请检查 .hologram/delivery.json。".to_string())
    })?;

    // Resolve the API key from environment variable (key_source: env).
    let key_source = provider.get("key_source").and_then(|v| v.as_str()).unwrap_or("env");
    let api_key = if key_source == "env" {
        let env_var = provider.get("env_var").and_then(|v| v.as_str()).unwrap_or("");
        if env_var.is_empty() {
            return Err(CliRuntimeError::environment(
                "delivery.json provider.env_var 未配置，无法读取 API Key。".to_string(),
            ));
        }
        std::env::var(env_var).ok().filter(|v| !v.trim().is_empty()).ok_or_else(|| {
            CliRuntimeError::environment(format!(
                "环境变量 {env_var} 未设置或为空。请设置好 API Key 后重试。"
            ))
        })?
    } else {
        return Err(CliRuntimeError::environment(
            "当前只支持 key_source=env 的 provider 配置。".to_string(),
        ));
    };

    let provider_name = provider.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
    let model = provider.get("model").and_then(|v| v.as_str()).unwrap_or("unknown");
    let base_url = provider.get("base_url").and_then(|v| v.as_str()).unwrap_or("");

    // Run check to get the latest findings, then look up the requested finding_id.
    let payload = build_workspace_check_payload(&workspace_path)?;
    let findings = payload["review"]["findings"]
        .as_array()
        .ok_or_else(|| CliRuntimeError::internal("check payload missing review.findings".to_string()))?;

    let finding = findings
        .iter()
        .find(|f| f.get("finding_id").and_then(|v| v.as_str()) == Some(finding_id))
        .ok_or_else(|| {
            CliRuntimeError::environment(format!(
                "找不到 finding_id={finding_id}。请先运行 `audit-risk check <workspace> --json` 确认 finding ID。"
            ))
        })?;

    let severity = finding.get("severity").and_then(|v| v.as_str()).unwrap_or("low");
    let file_path = finding["location"].get("file_path").and_then(|v| v.as_str()).unwrap_or("unknown");
    let start_line = finding["location"].get("start_line").and_then(|v| v.as_u64()).unwrap_or(1);
    let end_line = finding["location"].get("end_line").and_then(|v| v.as_u64()).unwrap_or(start_line);
    let explanation = finding.get("plain_explanation").and_then(|v| v.as_str()).unwrap_or("");
    let rule_id = finding.get("rule_id").and_then(|v| v.as_str()).unwrap_or("");

    // Read source file content for context (capped at 200 lines to avoid huge prompts).
    let abs_file = if std::path::Path::new(file_path).is_absolute() {
        std::path::PathBuf::from(file_path)
    } else {
        workspace_path.join(file_path)
    };
    let source_lines: Vec<String> = fs::read_to_string(&abs_file)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let context_start = start_line.saturating_sub(10).max(1) as usize;
    let context_end = (end_line as usize + 10).min(source_lines.len());
    let source_context = source_lines
        .get(context_start.saturating_sub(1)..context_end)
        .unwrap_or(&[])
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{}: {}", context_start + i, line))
        .collect::<Vec<_>>()
        .join("\n");

    // Build prompt and call the model via HTTP.
    let prompt = format!(
        "你是一个代码安全修复助手。以下是一条风险 finding，请给出最小化、安全的修复方案。\n\n\
        风险说明：{explanation}\n\
        规则：{rule_id}\n\
        严重程度：{severity}\n\
        文件：{file_path}（第 {start_line}-{end_line} 行）\n\n\
        相关源码（行号: 内容）：\n{source_context}\n\n\
        请返回一个 JSON 对象，格式如下（只返回 JSON，不要其他文字）：\n\
        {{\n\
          \"summary\": \"一句话说明修复了什么\",\n\
          \"rationale\": \"为什么这样修复能消除风险\",\n\
          \"operations\": [\n\
            {{\n\
              \"file_path\": \"{file_path}\",\n\
              \"start_line\": {start_line},\n\
              \"end_line\": {end_line},\n\
              \"old_content\": \"原始代码行（完整）\",\n\
              \"new_content\": \"修复后的代码行（完整）\",\n\
              \"summary\": \"这一处改动的说明\"\n\
            }}\n\
          ]\n\
        }}"
    );

    let proposal_value = call_model_for_repair(base_url, provider_name, model, &api_key, &prompt)?;

    // Validate the proposal: summary/rationale/operation.summary must not be placeholder text.
    let summary = proposal_value.get("summary").and_then(|v| v.as_str()).unwrap_or("");
    let rationale = proposal_value.get("rationale").and_then(|v| v.as_str()).unwrap_or("");
    for (field, value) in [("summary", summary), ("rationale", rationale)] {
        let lower = value.to_lowercase();
        if lower.is_empty() || matches!(lower.trim(), "fix" | "todo" | "update" | "修复" | "待办") {
            return Err(CliRuntimeError::internal(format!(
                "模型返回的 {field} 是占位文本，无法作为有效修复方案。"
            )));
        }
    }

    // Validate operations cover the finding's file path.
    let operations = proposal_value.get("operations").and_then(|v| v.as_array()).ok_or_else(|| {
        CliRuntimeError::internal("模型返回的修复方案缺少 operations 字段。".to_string())
    })?;
    let covers_target = operations.iter().any(|op| {
        op.get("file_path").and_then(|v| v.as_str()).map(|p| normalize_path(p) == normalize_path(file_path)).unwrap_or(false)
    });
    if !covers_target {
        return Err(CliRuntimeError::internal(format!(
            "模型返回的修复方案未覆盖目标文件 {file_path}，已拒绝。"
        )));
    }

    // Generate plan_id and persist to .hologram/repair-plans/.
    let plan_id = format!("rp_{}", pseudo_id(&format!("{finding_id}{}", now_iso())));
    let expires_at = repair_plan_expiry_iso();
    let plan = json!({
        "plan_id": plan_id,
        "finding_id": finding_id,
        "file_path": file_path,
        "start_line": start_line,
        "end_line": end_line,
        "severity": severity,
        "rule_id": rule_id,
        "strategy": "语义修复",
        "risk_note": "此修复会改动源码，请在确认前人工复核业务语义。",
        "required_tests": ["git diff --check"],
        "operations": operations,
        "summary": summary,
        "rationale": rationale,
        "provider_name": provider_name,
        "model": model,
        "created_at": now_iso(),
        "expires_at": expires_at,
        "approval_state": "waiting_approval",
    });

    let plans_dir = workspace_path.join(".hologram/repair-plans");
    fs::create_dir_all(&plans_dir).map_err(|e| CliRuntimeError::internal(format!("无法创建 repair-plans 目录：{e}")))?;
    let plan_path = plans_dir.join(format!("{plan_id}.json"));
    fs::write(&plan_path, serde_json::to_string_pretty(&plan).unwrap_or_default())
        .map_err(|e| CliRuntimeError::internal(format!("无法写入修复方案文件：{e}")))?;

    // Append audit event.
    append_repair_audit_event(&workspace_path, "repair_planned", &plan_id, finding_id, "修复方案已生成，等待用户确认。");

    let mut output = build_structured_output_envelope("repair", "ok", Some(&workspace_path.display().to_string()));
    if let Some(obj) = output.as_object_mut() {
        obj.insert("repair".into(), plan);
    }
    Ok(CommandOutcome::json(0, output))
}

const REPAIR_PREFLIGHT_COMMAND: &str = "git diff --check";

#[derive(Debug, Deserialize)]
struct RepairPlanDocument {
    plan_id: String,
    finding_id: String,
    expires_at: String,
    approval_state: String,
    #[serde(default)]
    required_tests: Vec<String>,
    operations: Vec<RepairOperation>,
}

#[derive(Debug, Deserialize)]
struct RepairOperation {
    file_path: String,
    start_line: usize,
    end_line: usize,
    new_content: String,
}

struct RepairCandidate {
    relative_path: String,
    absolute_path: PathBuf,
    snapshot: RepairSnapshot,
    patch_content: String,
    replacement: String,
}

struct RepairSnapshot {
    path: PathBuf,
    existed: bool,
    bytes: Vec<u8>,
}

fn run_repair_approve_command(workspace: &str, plan_id: &str) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_existing_workspace_path(workspace)?;
    let workspace_canonical = workspace_path.canonicalize().map_err(|error| {
        CliRuntimeError::environment(format!("无法解析工作区路径 {}：{error}", workspace_path.display()))
    })?;
    let (_, plan, actor) = approve_repair_plan(&workspace_canonical, plan_id)?;
    let mut output = build_structured_output_envelope("repair", "ok", Some(&workspace_canonical.display().to_string()));
    if let Some(object) = output.as_object_mut() {
        object.insert("approval".into(), json!({
            "plan_id": plan.plan_id,
            "approved_by": actor,
            "approved_at": now_iso(),
            "state": "approved",
        }));
    }
    Ok(CommandOutcome::json(0, output))
}

fn run_repair_apply_command(workspace: &str, plan_id: &str, approve: bool) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_existing_workspace_path(workspace)?;
    let workspace_canonical = workspace_path.canonicalize().map_err(|error| {
        CliRuntimeError::environment(format!("无法解析工作区路径 {}：{error}", workspace_path.display()))
    })?;

    if approve {
        approve_repair_plan(&workspace_canonical, plan_id)?;
    }

    let (plan_path, plan) = load_repair_plan(&workspace_canonical, plan_id)?;
    if plan.approval_state != "approved" {
        return Err(CliRuntimeError::environment(format!(
            "修复方案 {plan_id} 尚未审批。请先运行 `audit-risk repair approve <workspace> --plan {plan_id}`，或在本次 apply 中显式传入 `--approve`。"
        )));
    }
    if is_repair_plan_expired(&plan.expires_at) {
        return Err(CliRuntimeError::environment(format!(
            "修复方案 {plan_id} 已过期。请重新运行 `audit-risk repair plan` 生成新方案。"
        )));
    }
    if plan.operations.is_empty() {
        return Err(CliRuntimeError::environment("修复方案不包含任何操作，已拒绝执行。".to_string()));
    }
    if plan.required_tests != [REPAIR_PREFLIGHT_COMMAND] {
        append_repair_audit_event(&workspace_canonical, "repair_preflight_rejected", plan_id, &plan.finding_id,
            "修复方案包含未受信任的预检命令，已拒绝执行。");
        return Err(CliRuntimeError::environment(
            "修复方案包含未受信任的 required_tests；CLI 只允许内置 `git diff --check`。".to_string(),
        ));
    }

    let candidates = build_repair_candidates(&workspace_canonical, &plan.operations)?;
    secondary_audit_repair_candidates(&candidates)?;
    let preflight_results = run_repair_preflight(&workspace_canonical, plan_id, &plan.finding_id)?;

    let mut applied_files = Vec::new();
    for candidate in &candidates {
        if let Err(write_error) = fs::write(&candidate.absolute_path, &candidate.replacement) {
            let rollback_failures = rollback_repair_snapshots(&candidates);
            if rollback_failures.is_empty() {
                append_repair_audit_event(&workspace_canonical, "repair_rolled_back", plan_id, &plan.finding_id,
                    &format!("写入失败，已完成字节级回滚：{write_error}"));
            } else {
                append_repair_audit_event(&workspace_canonical, "repair_rollback_failed", plan_id, &plan.finding_id,
                    &format!("写入失败，回滚不完整：{}", rollback_failures.join("；")));
            }
            return Err(CliRuntimeError::internal(format!(
                "写入 {} 失败：{write_error}。{}",
                candidate.absolute_path.display(),
                if rollback_failures.is_empty() { "已完成回滚".to_string() } else { format!("回滚失败：{}", rollback_failures.join("；")) },
            )));
        }
        applied_files.push(candidate.relative_path.clone());
    }

    fs::remove_file(&plan_path).map_err(|error| {
        CliRuntimeError::internal(format!("修复已写入但无法移除已应用方案：{error}"))
    })?;
    append_repair_audit_event(&workspace_canonical, "repair_applied", plan_id, &plan.finding_id,
        &format!("修复已成功应用，涉及文件：{}", applied_files.join(", ")));

    let mut output = build_structured_output_envelope("repair", "ok", Some(&workspace_canonical.display().to_string()));
    if let Some(object) = output.as_object_mut() {
        object.insert("apply".into(), json!({
            "plan_id": plan_id,
            "applied_files": applied_files,
            "preflight": { "passed": true, "commands_run": [REPAIR_PREFLIGHT_COMMAND], "results": preflight_results },
            "audit_ref": DEFAULT_AUDIT_JSONL_PATH,
        }));
    }
    Ok(CommandOutcome::json(0, output))
}

fn approve_repair_plan(workspace: &Path, plan_id: &str) -> Result<(PathBuf, RepairPlanDocument, String), CliRuntimeError> {
    let plan_path = repair_plan_path(workspace, plan_id)?;
    let raw = fs::read_to_string(&plan_path)
        .map_err(|error| CliRuntimeError::environment(format!("无法读取修复方案 {plan_id}：{error}")))?;
    let mut value: Value = serde_json::from_str(&raw)
        .map_err(|error| CliRuntimeError::environment(format!("修复方案 {plan_id} 不是有效 JSON：{error}")))?;
    let plan: RepairPlanDocument = serde_json::from_value(value.clone())
        .map_err(|error| CliRuntimeError::environment(format!("修复方案 {plan_id} 字段不完整：{error}")))?;
    validate_repair_plan(&plan, plan_id)?;
    if plan.approval_state != "waiting_approval" {
        return Err(CliRuntimeError::environment(format!("修复方案 {plan_id} 当前状态为 `{}`，不能再次审批。", plan.approval_state)));
    }
    let actor = local_repair_approver();
    let approved_at = now_iso();
    let object = value.as_object_mut().ok_or_else(|| CliRuntimeError::environment("修复方案必须是 JSON 对象。".to_string()))?;
    object.insert("approval_state".to_string(), Value::String("approved".to_string()));
    object.insert("approved_at".to_string(), Value::String(approved_at.clone()));
    object.insert("approved_by".to_string(), Value::String(actor.clone()));
    fs::write(&plan_path, serde_json::to_vec_pretty(&value).expect("serialize approved repair plan"))
        .map_err(|error| CliRuntimeError::environment(format!("无法写入审批后的修复方案：{error}")))?;
    append_repair_approval_event(workspace, plan_id, &plan.finding_id, &actor, &approved_at);
    let approved_plan: RepairPlanDocument = serde_json::from_value(value)
        .map_err(|error| CliRuntimeError::internal(format!("审批后的修复方案无效：{error}")))?;
    Ok((plan_path, approved_plan, actor))
}

fn load_repair_plan(workspace: &Path, plan_id: &str) -> Result<(PathBuf, RepairPlanDocument), CliRuntimeError> {
    let plan_path = repair_plan_path(workspace, plan_id)?;
    let raw = fs::read_to_string(&plan_path)
        .map_err(|error| CliRuntimeError::environment(format!("无法读取修复方案 {plan_id}：{error}")))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| CliRuntimeError::environment(format!(
            "修复方案 {plan_id} 的文件内容损坏（无法解析 JSON）：{error}。请重新运行 `audit-risk repair plan` 生成新方案。"
        )))?;
    let plan: RepairPlanDocument = serde_json::from_value(value)
        .map_err(|error| CliRuntimeError::environment(format!(
            "修复方案 {plan_id} 的文件结构不合法：{error}。请重新运行 `audit-risk repair plan` 生成新方案。"
        )))?;
    validate_repair_plan(&plan, plan_id)?;
    Ok((plan_path, plan))
}

fn validate_repair_plan(plan: &RepairPlanDocument, requested_plan_id: &str) -> Result<(), CliRuntimeError> {
    validate_repair_plan_id(requested_plan_id)?;
    if plan.plan_id != requested_plan_id {
        return Err(CliRuntimeError::environment("修复方案文件的 plan_id 与请求不一致，已拒绝执行。".to_string()));
    }
    if plan.expires_at.trim().is_empty() || is_repair_plan_expired(&plan.expires_at) {
        return Err(CliRuntimeError::environment("修复方案已过期或 expires_at 无效，已拒绝执行。".to_string()));
    }
    Ok(())
}

fn repair_plan_path(workspace: &Path, plan_id: &str) -> Result<PathBuf, CliRuntimeError> {
    validate_repair_plan_id(plan_id)?;
    let path = workspace.join(".hologram/repair-plans").join(format!("{plan_id}.json"));
    let canonical = path.canonicalize()
        .map_err(|_| CliRuntimeError::environment(format!("找不到修复方案 {plan_id}。")))?;
    if !canonical.starts_with(workspace) {
        return Err(CliRuntimeError::environment("修复方案路径越出工作区，已拒绝执行。".to_string()));
    }
    Ok(canonical)
}

fn validate_repair_plan_id(plan_id: &str) -> Result<(), CliRuntimeError> {
    let valid = !plan_id.is_empty()
        && !plan_id.contains("..")
        && plan_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(CliRuntimeError::environment("plan_id 只允许字母、数字、`_`、`-`、`.`，且不能包含 `..` 或路径分隔符。".to_string()))
    }
}

fn build_repair_candidates(workspace: &Path, operations: &[RepairOperation]) -> Result<Vec<RepairCandidate>, CliRuntimeError> {
    let mut candidates = Vec::with_capacity(operations.len());
    let mut seen_paths = BTreeSet::new();
    for operation in operations {
        if operation.start_line == 0 || operation.end_line < operation.start_line {
            return Err(CliRuntimeError::environment("修复方案包含无效的行范围，已拒绝执行。".to_string()));
        }
        let (relative_path, absolute_path) = resolve_repair_target_path(workspace, &operation.file_path)?;
        if !seen_paths.insert(relative_path.clone()) {
            return Err(CliRuntimeError::environment(format!("修复方案重复修改 `{relative_path}`，已拒绝执行。")));
        }
        let existed = absolute_path.exists();
        let bytes = if existed {
            fs::read(&absolute_path).map_err(|error| {
                CliRuntimeError::environment(format!("无法读取修复目标 {relative_path}：{error}"))
            })?
        } else {
            Vec::new()
        };
        let original_content = String::from_utf8(bytes.clone()).map_err(|_| {
            CliRuntimeError::environment(format!("修复目标 {relative_path} 不是 UTF-8 文本，已拒绝修改以保护二进制内容。"))
        })?;
        let replacement = replace_repair_lines(&original_content, operation);
        candidates.push(RepairCandidate {
            relative_path,
            absolute_path: absolute_path.clone(),
            snapshot: RepairSnapshot { path: absolute_path, existed, bytes },
            patch_content: operation.new_content.clone(),
            replacement,
        });
    }
    Ok(candidates)
}

fn resolve_repair_target_path(workspace: &Path, file_path: &str) -> Result<(String, PathBuf), CliRuntimeError> {
    let relative = Path::new(file_path);
    if relative.as_os_str().is_empty() || relative.is_absolute() || is_sensitive_repair_path(relative) {
        return Err(CliRuntimeError::environment(format!("修复目标 `{file_path}` 不允许写入，已拒绝执行。")));
    }
    let mut raw_path = workspace.to_path_buf();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => {
                raw_path.push(part);
                if fs::symlink_metadata(&raw_path).map(|meta| meta.file_type().is_symlink()).unwrap_or(false) {
                    return Err(CliRuntimeError::environment(format!("修复目标 `{file_path}` 包含符号链接，已拒绝执行。")));
                }
            }
            _ => return Err(CliRuntimeError::environment(format!("修复目标 `{file_path}` 只能是工作区内的普通相对路径。"))),
        }
    }
    if raw_path.is_dir() {
        return Err(CliRuntimeError::environment(format!("修复目标 `{file_path}` 是目录，已拒绝执行。")));
    }
    let resolved = if raw_path.exists() {
        raw_path.canonicalize()
    } else {
        let parent = raw_path.parent().ok_or_else(|| CliRuntimeError::environment("修复目标缺少父目录。".to_string()))?;
        parent.canonicalize().map(|canonical_parent| {
            canonical_parent.join(raw_path.file_name().expect("repair target has file name"))
        })
    }
    .map_err(|error| CliRuntimeError::environment(format!("无法规范化修复目标 `{file_path}`：{error}")))?;
    if !resolved.starts_with(workspace) {
        return Err(CliRuntimeError::environment(format!("修复目标 `{file_path}` 越出工作区，已拒绝执行。")));
    }
    Ok((normalize_path(file_path), resolved))
}

fn is_sensitive_repair_path(path: &Path) -> bool {
    let components: Vec<String> = path.components().filter_map(|component| match component {
        std::path::Component::Normal(value) => value.to_str().map(str::to_string),
        _ => None,
    }).collect();
    let file_name = components.last().map(String::as_str).unwrap_or_default();
    components.iter().any(|component| component == ".git")
        || file_name.starts_with(".env")
        || matches!(file_name, "Cargo.lock" | "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock")
        || matches!(Path::new(file_name).extension().and_then(|value| value.to_str()), Some("key" | "pem" | "p12" | "pfx"))
}

fn replace_repair_lines(original: &str, operation: &RepairOperation) -> String {
    let mut lines: Vec<String> = original.lines().map(str::to_string).collect();
    let replace_start = operation.start_line - 1;
    let replace_end = operation.end_line.min(lines.len());
    if replace_start >= lines.len() {
        lines.push(operation.new_content.clone());
    } else {
        lines.splice(replace_start..replace_end, std::iter::once(operation.new_content.clone()));
    }
    lines.join("\n") + if original.ends_with('\n') { "\n" } else { "" }
}

fn secondary_audit_repair_candidates(candidates: &[RepairCandidate]) -> Result<(), CliRuntimeError> {
    let scanner = SecretScanner::new();
    let mut findings = Vec::new();
    for candidate in candidates {
        for finding in scanner.scan_content(&candidate.relative_path, &candidate.patch_content) {
            findings.push(format!("{}:{}", finding.file_path, finding.line));
        }
    }
    if findings.is_empty() {
        Ok(())
    } else {
        Err(CliRuntimeError::environment(format!(
            "二次审计发现补丁引入风险（{}），已阻断 apply。",
            findings.join(", ")
        )))
    }
}

fn run_repair_preflight(workspace: &Path, plan_id: &str, finding_id: &str) -> Result<Vec<Value>, CliRuntimeError> {
    let output = Command::new("git")
        .args(["diff", "--check"])
        .current_dir(workspace)
        .output()
        .map_err(|error| CliRuntimeError::environment(format!("无法运行内置预检：{error}")))?;
    let result = json!({
        "command": REPAIR_PREFLIGHT_COMMAND,
        "passed": output.status.success(),
        "stdout": String::from_utf8_lossy(&output.stdout).trim(),
        "stderr": String::from_utf8_lossy(&output.stderr).trim(),
    });
    if output.status.success() {
        Ok(vec![result])
    } else {
        append_repair_audit_event(workspace, "repair_preflight_failed", plan_id, finding_id, "内置预检 git diff --check 失败，修复已阻断。");
        Err(CliRuntimeError::environment("内置预检 `git diff --check` 失败，修复已阻断。".to_string()))
    }
}

fn rollback_repair_snapshots(candidates: &[RepairCandidate]) -> Vec<String> {
    let mut failures = Vec::new();
    for candidate in candidates.iter().rev() {
        let result = if candidate.snapshot.existed {
            fs::write(&candidate.snapshot.path, &candidate.snapshot.bytes)
        } else if candidate.snapshot.path.exists() {
            fs::remove_file(&candidate.snapshot.path)
        } else {
            Ok(())
        };
        if let Err(error) = result {
            failures.push(format!("{}: {error}", candidate.relative_path));
        }
    }
    failures
}

fn local_repair_approver() -> String {
    match std::env::var("USER").or_else(|_| std::env::var("USERNAME")) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => "unknown-local-user".to_string(),
    }
}

/// Call the provider model with a plain HTTP POST and return the parsed proposal JSON.
fn call_model_for_repair(
    base_url: &str,
    provider_name: &str,
    model: &str,
    api_key: &str,
    prompt: &str,
) -> Result<serde_json::Value, CliRuntimeError> {
    // Build a minimal OpenAI-compatible chat request that both DeepSeek and OpenAI accept.
    // Anthropic uses a different envelope — detect by provider name.
    let is_anthropic = provider_name.to_lowercase().contains("anthropic");

    let (url, body, auth_header) = if is_anthropic {
        let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
        let body = json!({
            "model": model,
            "max_tokens": 2048,
            "messages": [{"role": "user", "content": prompt}]
        });
        (url, body, format!("x-api-key: {api_key}"))
    } else {
        let url = format!("{}/v1/chat/completions", base_url.trim_end_matches('/'));
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.2
        });
        (url, body, format!("Authorization: Bearer {api_key}"))
    };

    let body_str = serde_json::to_string(&body)
        .map_err(|e| CliRuntimeError::internal(format!("无法序列化请求体：{e}")))?;

    // Use curl as the HTTP client — avoids adding an async runtime or HTTP crate dependency.
    let auth_parts: Vec<&str> = auth_header.splitn(2, ": ").collect();
    let (header_name, header_value) = if auth_parts.len() == 2 {
        (auth_parts[0], auth_parts[1])
    } else {
        return Err(CliRuntimeError::internal("认证头格式有误。".to_string()));
    };

    let result = Command::new("curl")
        .args([
            "-s", "-X", "POST", &url,
            "-H", "Content-Type: application/json",
            "-H", &format!("{header_name}: {header_value}"),
            "-d", &body_str,
            "--max-time", "60",
        ])
        .output()
        .map_err(|e| CliRuntimeError::internal(format!("无法调用 curl：{e}。请确认系统已安装 curl。")))?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(CliRuntimeError::internal(format!("curl 请求失败：{stderr}")));
    }

    let raw = String::from_utf8_lossy(&result.stdout);
    let response: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| CliRuntimeError::internal(format!("模型响应不是有效 JSON：{e}")))?;

    // Extract text content from OpenAI-compatible or Anthropic response.
    let text = if is_anthropic {
        response["content"][0]["text"].as_str()
    } else {
        response["choices"][0]["message"]["content"].as_str()
    }
    .ok_or_else(|| {
        CliRuntimeError::internal(format!(
            "模型响应中找不到文本内容。原始响应：{}",
            raw.chars().take(300).collect::<String>()
        ))
    })?;

    // Extract JSON from the response text (model might wrap it in ```json blocks).
    let json_text = extract_json_from_text(text);
    serde_json::from_str(json_text.as_deref().unwrap_or(text))
        .map_err(|e| CliRuntimeError::internal(format!("无法解析模型返回的 JSON：{e}")))
}

/// Strip ```json ... ``` fences if the model wrapped the response.
fn extract_json_from_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if let Some(inner) = trimmed.strip_prefix("```json").and_then(|s| s.strip_suffix("```")) {
        return Some(inner.trim().to_string());
    }
    if let Some(inner) = trimmed.strip_prefix("```").and_then(|s| s.strip_suffix("```")) {
        return Some(inner.trim().to_string());
    }
    None
}

/// Append a minimal repair audit event to .hologram/audit.jsonl.
fn append_repair_audit_event(workspace: &Path, event_type: &str, plan_id: &str, finding_id: &str, reason: &str) {
    let audit_path = workspace.join(DEFAULT_AUDIT_JSONL_PATH);
    if let Some(parent) = audit_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let event = json!({
        "event_type": event_type,
        "plane": "repair",
        "subject_ref": plan_id,
        "finding_id": finding_id,
        "reason": reason,
        "timestamp": now_iso(),
    });
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&audit_path) {
        let _ = writeln!(file, "{}", serde_json::to_string(&event).unwrap_or_default());
    }
}

fn append_repair_approval_event(
    workspace: &Path,
    plan_id: &str,
    finding_id: &str,
    approved_by: &str,
    approved_at: &str,
) {
    let audit_path = workspace.join(DEFAULT_AUDIT_JSONL_PATH);
    if let Some(parent) = audit_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let event = json!({
        "event_type": "repair_approved",
        "plane": "repair",
        "subject_ref": plan_id,
        "finding_id": finding_id,
        "approved_by": approved_by,
        "approved_at": approved_at,
        "timestamp": now_iso(),
    });
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&audit_path) {
        let _ = writeln!(file, "{}", serde_json::to_string(&event).unwrap_or_default());
    }
}

/// Generate a short deterministic-ish id from a seed string (no randomness needed for plan IDs).
fn pseudo_id(seed: &str) -> String {
    // Use a simple djb2-style hash to produce a short hex string.
    let mut hash: u64 = 5381;
    for byte in seed.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    format!("{hash:016x}")
}

/// ISO timestamp 10 minutes from now for plan expiry.
fn repair_plan_expiry_iso() -> String {
    // chrono is already used elsewhere in this file (now_iso uses it).
    let expiry = chrono::Utc::now() + chrono::Duration::minutes(10);
    expiry.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Return true if the plan's expires_at timestamp is in the past.
fn is_repair_plan_expired(expires_at: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|exp| exp <= chrono::Utc::now())
        .unwrap_or(true)
}

fn run_observe_command(workspace: Option<&str>) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = workspace.map(PathBuf::from).unwrap_or(std::env::current_dir().map_err(|error| {
        CliRuntimeError::environment(format!("无法读取当前目录：{error}"))
    })?);
    let workspace_path = resolve_existing_path(&workspace_path.display().to_string())?;
    ensure_pro_feature("observe", &workspace_path)?;
    let runtime = start_observe_runtime(&workspace_path)?;
    Ok(CommandOutcome::text(
        0,
        render_product_shell(
            &[
                "当前视图：手机观察".to_string(),
                "运行状态：已开启".to_string(),
                format!("目标目录：{}", normalize_path(workspace_path.display().to_string())),
            ],
            &[format!(
                "本机地址：{}；局域网地址：{}",
                runtime.local_url, runtime.public_url
            )],
            &["手机观察不是第二套产品，而是当前终端审查结果的只读旁路视图。".to_string()],
            &[format!(
                "二维码图片：{}",
                runtime.qr_path.unwrap_or_else(|| "当前环境未生成二维码，可直接复制局域网地址。".to_string())
            )],
            &[
                "`audit-risk watch . --observe`".to_string(),
                "`audit-risk auth status`".to_string(),
            ],
            &["保持这个命令运行，手机或旁路设备就能看到最近一次审查状态。".to_string()],
        ),
    ))
}

fn command_exists(command: &str) -> &'static str {
    let passed = Command::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if passed { "ok" } else { "needs_attention" }
}

fn build_workspace_check_payload(workspace: &Path) -> Result<Value, CliRuntimeError> {
    let before = load_baseline(workspace);
    engine_init(workspace)
        .map_err(|error| CliRuntimeError::environment(format!("engine init failed: {error}")))?;
    let analysis = engine_analyze(workspace)
        .map_err(|error| CliRuntimeError::environment(format!("engine analyze failed: {error}")))?;
    let changed_files = git_changed_files(workspace);
    let check = run_full_check(
        &before,
        &analysis.graph,
        &changed_files,
        &workspace.to_string_lossy(),
    );
    save_baseline(workspace, &analysis.graph);
    Ok(build_review_payload(
        "check",
        workspace,
        changed_files,
        json!({
            "node_count": analysis.node_count,
            "edge_count": analysis.edge_count,
            "community_count": analysis.community_count,
            "elapsed_secs": analysis.elapsed_secs,
        }),
        check,
    ))
}

fn build_diff_payload(before: &Path, after: &Path) -> Result<Value, CliRuntimeError> {
    let (before_workspace, after_workspace, changed_files, cleanup_paths) =
        prepare_diff_inputs(before, after)?;

    let payload = (|| {
        engine_init(&before_workspace)
            .map_err(|error| CliRuntimeError::environment(format!("engine init failed for before diff input: {error}")))?;
        let before_analysis = engine_analyze(&before_workspace)
            .map_err(|error| CliRuntimeError::environment(format!("engine analyze failed for before diff input: {error}")))?;

        engine_init(&after_workspace)
            .map_err(|error| CliRuntimeError::environment(format!("engine init failed for after diff input: {error}")))?;
        let after_analysis = engine_analyze(&after_workspace)
            .map_err(|error| CliRuntimeError::environment(format!("engine analyze failed for after diff input: {error}")))?;

        let check = run_full_check(
            &before_analysis.graph,
            &after_analysis.graph,
            &changed_files,
            &after_workspace.to_string_lossy(),
        );

        Ok(build_review_payload(
            "diff",
            &after_workspace,
            changed_files,
            json!({
                "before_root": normalize_path(before_workspace.display().to_string()),
                "after_root": normalize_path(after_workspace.display().to_string()),
                "node_count": after_analysis.node_count,
                "edge_count": after_analysis.edge_count,
                "community_count": after_analysis.community_count,
                "elapsed_secs": after_analysis.elapsed_secs,
            }),
            check,
        ))
    })();

    for path in cleanup_paths {
        let _ = fs::remove_dir_all(path);
    }

    payload
}

fn build_review_payload(
    command: &str,
    workspace: &Path,
    changed_files: Vec<String>,
    analysis: Value,
    check: Value,
) -> Value {
    let findings = derive_findings(&check);
    let gate_decision = derive_gate_decision(&check, &findings);
    let mut output = build_structured_output_envelope(command, "ok", Some(&workspace.display().to_string()));
    if let Some(object) = output.as_object_mut() {
        object.insert("changed_files".into(), json!(changed_files));
        object.insert("analysis".into(), analysis);
        object.insert(
            "review".into(),
            json!({
                "findings": findings,
                "gate_decision": gate_decision,
                "degraded_reasons": [],
                "raw_check": check,
            }),
        );
        object.insert(
            "audit_ref".into(),
            json!({
                "jsonl_path": DEFAULT_AUDIT_JSONL_PATH,
            }),
        );
    }
    output
}

fn derive_findings(check: &Value) -> Vec<Value> {
    let mut findings = Vec::new();
    for (bucket, severity) in [
        ("l5_violations", "critical"),
        ("l4_violations", "high"),
        ("l3_violations", "medium"),
        ("l2_violations", "low"),
    ] {
        if let Some(entries) = check.get(bucket).and_then(Value::as_array) {
            for (index, entry) in entries.iter().enumerate() {
                let signal = entry.get("signal").cloned().unwrap_or_else(|| json!({}));
                let file_path = signal
                    .get("file_path")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let line = signal.get("line").and_then(Value::as_u64).unwrap_or(1);
                let description = signal
                    .get("description")
                    .and_then(Value::as_str)
                    .or_else(|| entry.get("message").and_then(Value::as_str))
                    .unwrap_or("发现一条需要关注的风险，建议查看源码确认影响范围。");
                findings.push(json!({
                    "finding_id": format!("{bucket}:{index}"),
                    "rule_id": bucket.replace("_violations", "").replace("l", "check.l"),
                    "severity": severity,
                    "plain_explanation": description,
                    "location": {
                        "file_path": file_path,
                        "start_line": line,
                        "end_line": line,
                    },
                }));
            }
        }
    }
    // Rank so the most actionable findings surface first: severity high→low,
    // then findings pinned to a concrete file:line above whole-project signals.
    // This keeps a leaked key or injection from being buried under structural
    // "shared data" notes that share the same severity.
    findings.sort_by(|a, b| {
        finding_priority(b)
            .cmp(&finding_priority(a))
            .then_with(|| finding_specificity(b).cmp(&finding_specificity(a)))
    });
    findings
}

/// Higher = more urgent. Keyed off severity plus the finding's semantic class
/// (leaked secrets / injection / dangerous execution outrank generic structure).
fn finding_priority(finding: &Value) -> i32 {
    let severity_rank = match finding.get("severity").and_then(Value::as_str) {
        Some("critical") => 400,
        Some("high") => 300,
        Some("medium") => 200,
        Some("low") => 100,
        _ => 0,
    };
    let explanation = finding
        .get("plain_explanation")
        .and_then(Value::as_str)
        .unwrap_or("");
    // Nudge the highest-consequence classes above same-severity peers.
    let class_bonus = if explanation.contains("密钥") || explanation.contains("secret") || explanation.contains("key") {
        50
    } else if explanation.contains("注入") || explanation.contains("injection") {
        40
    } else if explanation.contains("危险") || explanation.contains("执行") {
        30
    } else {
        0
    };
    severity_rank + class_bonus
}

/// 1 when the finding points at a concrete file:line, 0 for whole-project signals.
fn finding_specificity(finding: &Value) -> i32 {
    let loc = finding.get("location").and_then(Value::as_object);
    let has_file = loc
        .and_then(|l| l.get("file_path"))
        .and_then(Value::as_str)
        .map(|f| !f.is_empty() && f != "unknown")
        .unwrap_or(false);
    let has_line = loc
        .and_then(|l| l.get("start_line"))
        .and_then(Value::as_u64)
        .map(|n| n > 0)
        .unwrap_or(false);
    i32::from(has_file && has_line)
}

fn derive_gate_decision(check: &Value, findings: &[Value]) -> Value {
    let finding_count = findings.len();
    let decision = if check.get("l5_violations").and_then(Value::as_array).map(|entries| !entries.is_empty()).unwrap_or(false) {
        "block"
    } else if check.get("l4_violations").and_then(Value::as_array).map(|entries| !entries.is_empty()).unwrap_or(false) {
        "require_approval"
    } else if check.get("l3_violations").and_then(Value::as_array).map(|entries| !entries.is_empty()).unwrap_or(false) {
        "warn"
    } else {
        "allow"
    };
    let finding_ids: Vec<String> = findings
        .iter()
        .filter_map(|finding| finding.get("finding_id").and_then(Value::as_str).map(str::to_string))
        .collect();
    json!({
        "decision": decision,
        "reason": check.get("one_line").and_then(Value::as_str).unwrap_or("本次审查未返回摘要说明。"),
        "finding_count": finding_count,
        "finding_ids": finding_ids,
        "subject_ref": "workspace",
        "policy_snapshot_id": "default.v1",
    })
}

fn gate_exit_code(decision: Option<&str>, fail_on: FailGate) -> i32 {
    if fail_on == FailGate::Off {
        return 0;
    }
    let rank = match decision.unwrap_or("allow") {
        "allow" => FailGate::Off,
        "warn" => FailGate::Warn,
        "require_approval" => FailGate::RequireApproval,
        "block" => FailGate::Block,
        _ => FailGate::Off,
    };
    if rank >= fail_on {
        2
    } else {
        0
    }
}

fn render_watch_summary(payload: &Value, verbose: bool) -> Result<String, CliRuntimeError> {
    let findings = payload["review"]["findings"]
        .as_array()
        .ok_or_else(|| CliRuntimeError::internal("watch payload is missing review.findings"))?;
    let gate = payload["review"]["gate_decision"]["decision"]
        .as_str()
        .unwrap_or("allow");
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for severity in ["critical", "high", "medium", "low"] {
        counts.insert(
            severity,
            findings
                .iter()
                .filter(|entry| entry.get("severity").and_then(Value::as_str) == Some(severity))
                .count(),
        );
    }
    let mut parts = vec![format!(
        "[{}] gate={} critical={} high={} medium={}",
        payload["generated_at"].as_str().unwrap_or(""),
        gate,
        counts["critical"],
        counts["high"],
        counts["medium"],
    )];
    if verbose {
        parts.push(format!("low={}", counts["low"]));
    }
    if let Some(suppressed) = payload.get("suppressed_count").and_then(Value::as_u64) {
        if suppressed > 0 {
            parts.push(format!("suppressed={suppressed}"));
        }
    }
    let filtered: Vec<String> = findings
        .iter()
        .filter(|entry| {
            let severity = entry.get("severity").and_then(Value::as_str).unwrap_or("low");
            verbose || matches!(severity, "critical" | "high" | "medium")
        })
        .take(5)
        .map(|entry| {
            let severity = entry.get("severity").and_then(Value::as_str).unwrap_or("unknown");
            let explanation = entry
                .get("plain_explanation")
                .and_then(Value::as_str)
                .unwrap_or("detected risk");
            let location = entry
                .get("location")
                .and_then(Value::as_object)
                .map(|loc| {
                    format!(
                        "{}:{}",
                        loc.get("file_path").and_then(Value::as_str).unwrap_or("unknown"),
                        loc.get("start_line").and_then(Value::as_u64).unwrap_or(1)
                    )
                })
                .unwrap_or_else(|| "unknown".to_string());
            format!("  - [{severity}] {location} {explanation}")
        })
        .collect();
    parts.extend(filtered);
    Ok(parts.join("\n"))
}

fn render_watch_summary_human(payload: &Value, verbose: bool) -> Result<String, CliRuntimeError> {
    let summary = render_watch_summary(payload, verbose)?;
    let gate = payload["review"]["gate_decision"]["decision"]
        .as_str()
        .unwrap_or("allow");
    let critical = payload["review"]["findings"]
        .as_array()
        .map(|entries| entries.iter().filter(|entry| entry.get("severity").and_then(Value::as_str) == Some("critical")).count())
        .unwrap_or(0);
    let high = payload["review"]["findings"]
        .as_array()
        .map(|entries| entries.iter().filter(|entry| entry.get("severity").and_then(Value::as_str) == Some("high")).count())
        .unwrap_or(0);
    let medium = payload["review"]["findings"]
        .as_array()
        .map(|entries| entries.iter().filter(|entry| entry.get("severity").and_then(Value::as_str) == Some("medium")).count())
        .unwrap_or(0);
    let findings = payload["review"]["findings"]
        .as_array()
        .ok_or_else(|| CliRuntimeError::internal("watch payload is missing review.findings"))?;
    let visible = findings
        .iter()
        .filter(|entry| {
            let severity = entry.get("severity").and_then(Value::as_str).unwrap_or("low");
            verbose || matches!(severity, "critical" | "high" | "medium")
        })
        .take(5)
        .map(|entry| {
            let severity = entry.get("severity").and_then(Value::as_str).unwrap_or("unknown");
            let explanation = entry
                .get("plain_explanation")
                .and_then(Value::as_str)
                .unwrap_or("detected risk");
            let location = entry
                .get("location")
                .and_then(Value::as_object)
                .map(|loc| {
                    format!(
                        "{}:{}",
                        loc.get("file_path").and_then(Value::as_str).unwrap_or("unknown"),
                        loc.get("start_line").and_then(Value::as_u64).unwrap_or(1)
                    )
                })
                .unwrap_or_else(|| "unknown".to_string());
            let raw = format!("[{severity}] {location} {explanation}");
            if matches!(severity, "critical" | "high") {
                ansi_red(&raw)
            } else if severity == "medium" {
                ansi_yellow(&raw)
            } else if severity == "low" {
                ansi_dim(&raw)
            } else {
                raw
            }
        })
        .collect::<Vec<_>>();
    let mut note_lines = vec![summary];
    if let Some(suppressed) = payload.get("suppressed_count").and_then(Value::as_u64) {
        if suppressed > 0 {
            note_lines.push(format!("防抖提示：10 分钟内重复命中的同文件同规则已折叠 {suppressed} 次。"));
        }
    }
    Ok(render_product_shell(
        &[
            "当前视图：持续守护".to_string(),
            format!(
                "守护结论：{}",
                colorize_watch_header(gate_decision_label(gate), gate, critical + high, medium)
            ),
            format!("可见风险：{} 条", findings.len()),
        ],
        &if visible.is_empty() {
            vec!["当前没有需要展示的新风险。".to_string()]
        } else {
            visible
        },
        &[match gate {
            "allow" => "守护模式这次没有发现需要拦截的新问题。".to_string(),
            "warn" => "守护模式已经看到需要你留意的中风险信号。".to_string(),
            "require_approval" => "守护模式发现了高风险改动，最好先让人确认。".to_string(),
            "block" => "守护模式发现了阻断级问题，不应该继续放行。".to_string(),
            _ => "守护结果不完整，建议重新检查。".to_string(),
        }],
        &[
            "继续写代码时看这页就够了，不需要再开第二个终端盯日志。".to_string(),
            if verbose {
                "当前已打开详细模式，低风险项也会显示。".to_string()
            } else {
                "如需看低风险项，再加 `--verbose`。".to_string()
            },
        ],
        &[
            "`audit-risk check .`".to_string(),
            "`audit-risk report .`".to_string(),
        ],
        &note_lines,
    ))
}

fn render_watch_start_screen(workspace: &Path, observe_runtime: Option<&ObserveRuntime>) -> String {
    let mut advice = vec!["首次扫描已经开始，后续每次文件变化都会刷新这一页。".to_string()];
    let mut notes = vec!["普通用户默认只看 critical / high / medium；低风险项请用 `--verbose`。".to_string()];
    if let Some(runtime) = observe_runtime {
        advice.push(format!("本机观察地址：{}", runtime.local_url));
        advice.push(format!("局域网观察地址：{}", runtime.public_url));
        if let Some(qr_path) = runtime.qr_path.as_ref() {
            advice.push(format!("二维码图片：{qr_path}"));
        }
        if let Some(note) = runtime.note.as_ref() {
            notes.push(format!("observe 说明：{note}"));
        }
    }
    render_product_shell(
        &[
            "当前视图：持续守护".to_string(),
            format!("工作目录：{}", normalize_path(workspace.display().to_string())),
            "运行状态：已启动，正在做首次扫描".to_string(),
        ],
        &["守护模式会在你保存文件后自动重新审查，不需要手工重复敲 check。".to_string()],
        &["如果首屏没有反馈，用户很难判断命令是在工作还是已经卡住。".to_string()],
        &advice,
        &[
            "`audit-risk check .`".to_string(),
            "`audit-risk report .`".to_string(),
        ],
        &notes,
    )
}

fn filter_watch_findings_for_output(
    findings: &[Value],
    now_ms: u128,
    previous_emissions: &BTreeMap<String, u128>,
    debounce_window_ms: u128,
) -> (Vec<Value>, usize) {
    let mut visible = Vec::new();
    let mut suppressed = 0;
    for finding in findings {
        if should_emit_watch_finding(finding, now_ms, previous_emissions, debounce_window_ms) {
            visible.push(finding.clone());
        } else {
            suppressed += 1;
        }
    }
    (visible, suppressed)
}

fn build_watch_finding_key(finding: &Value) -> String {
    let rule_id = finding.get("rule_id").and_then(Value::as_str).unwrap_or("unknown");
    let location = finding.get("location").and_then(Value::as_object);
    let file_path = location
        .and_then(|loc| loc.get("file_path"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    format!("{file_path}::{rule_id}")
}

fn colorize_watch_header(header: &str, gate: &str, high_severity_count: usize, medium_count: usize) -> String {
    if high_severity_count > 0 || matches!(gate, "block" | "require_approval") {
        return ansi_red(header);
    }
    if medium_count > 0 || gate == "warn" {
        return ansi_yellow(header);
    }
    if gate == "allow" {
        return ansi_green(header);
    }
    header.to_string()
}

fn should_emit_watch_finding(
    finding: &Value,
    now_ms: u128,
    previous_emissions: &std::collections::BTreeMap<String, u128>,
    debounce_window_ms: u128,
) -> bool {
    let key = build_watch_finding_key(finding);
    match previous_emissions.get(&key) {
        Some(previous) => now_ms.saturating_sub(*previous) > debounce_window_ms,
        None => true,
    }
}

fn emit_watch_event(
    stdout: &mut io::Stdout,
    event: &str,
    workspace: Option<&Path>,
    payload: Value,
) -> Result<(), CliRuntimeError> {
    let workspace_owned = workspace.map(|path| path.to_string_lossy().to_string());
    let mut object = build_structured_output_envelope(
        "watch",
        "ok",
        workspace_owned.as_deref(),
    );
    if let Some(map) = object.as_object_mut() {
        map.insert("event".into(), Value::String(event.to_string()));
        map.insert("ts".into(), Value::String(now_iso()));
        map.insert("payload".into(), payload);
    }
    writeln!(
        stdout,
        "{}",
        serde_json::to_string(&object)
            .map_err(|error| CliRuntimeError::internal(format!("failed to encode watch event: {error}")))?
    )
    .map_err(|error| CliRuntimeError::internal(format!("failed to write watch event: {error}")))?;
    Ok(())
}

fn workspace_snapshot(workspace: &Path) -> Result<BTreeMap<String, u128>, CliRuntimeError> {
    let mut snapshot = BTreeMap::new();
    for entry in WalkDir::new(workspace)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if is_ignored_path(path) {
            continue;
        }
        let relative = normalize_path(
            path
                .strip_prefix(workspace)
                .unwrap_or(path)
                .display()
                .to_string(),
        );
        let modified = entry
            .metadata()
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        snapshot.insert(relative, modified);
    }
    Ok(snapshot)
}

fn is_ignored_path(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str().unwrap_or_default(),
            ".git" | ".hologram" | "node_modules" | "target" | "dist" | "build"
        )
    })
}

fn prepare_diff_inputs(
    before: &Path,
    after: &Path,
) -> Result<(PathBuf, PathBuf, Vec<String>, Vec<PathBuf>), CliRuntimeError> {
    if before.is_file() && after.is_file() {
        let unique = uuid::Uuid::new_v4().to_string();
        let base_dir = std::env::temp_dir().join(format!("audit-risk-diff-{unique}"));
        let before_root = base_dir.join("before");
        let after_root = base_dir.join("after");
        fs::create_dir_all(&before_root)
            .and_then(|_| fs::create_dir_all(&after_root))
            .map_err(|error| CliRuntimeError::internal(format!("failed to prepare temp diff workspace: {error}")))?;
        let relative_name = before
            .file_name()
            .or_else(|| after.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("snippet.txt")
            .to_string();
        fs::copy(before, before_root.join(&relative_name))
            .map_err(|error| CliRuntimeError::internal(format!("failed to copy before diff input: {error}")))?;
        fs::copy(after, after_root.join(&relative_name))
            .map_err(|error| CliRuntimeError::internal(format!("failed to copy after diff input: {error}")))?;
        return Ok((
            before_root,
            after_root,
            vec![relative_name],
            vec![base_dir],
        ));
    }

    if before.is_dir() && after.is_dir() {
        let changed_files = diff_changed_files(before, after)?;
        return Ok((
            before.to_path_buf(),
            after.to_path_buf(),
            changed_files,
            Vec::new(),
        ));
    }

    Err(CliRuntimeError::environment(
        "`diff` requires either two files or two directories.",
    ))
}

fn diff_changed_files(before: &Path, after: &Path) -> Result<Vec<String>, CliRuntimeError> {
    let mut paths = BTreeSet::new();
    for root in [before, after] {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            let relative = normalize_path(
                entry
                    .path()
                    .strip_prefix(root)
                    .unwrap_or(entry.path())
                    .display()
                    .to_string(),
            );
            if !is_ignored_path(entry.path()) {
                paths.insert(relative);
            }
        }
    }

    let mut changed = Vec::new();
    for relative in paths {
        let before_file = before.join(&relative);
        let after_file = after.join(&relative);
        let before_bytes = fs::read(&before_file).ok();
        let after_bytes = fs::read(&after_file).ok();
        if before_bytes != after_bytes {
            changed.push(relative);
        }
    }
    Ok(changed)
}

fn build_default_init_files(workspace_root: &Path) -> Vec<(String, String, bool)> {
    let config = json!({
        "version": "phase5.v1",
        "workspace": {
            "root": normalize_path(workspace_root.display().to_string()),
            "changed_files_source": "git_status",
        },
        "provider": {
            "name": "deepseek",
            "model": "deepseek-v4-pro",
            "base_url": "https://api.deepseek.com",
            "key_source": "env",
            "env_var": "DEEPSEEK_API_KEY",
        },
        "rule_packages": {
            "review_paths": [DEFAULT_REVIEW_RULE_PATH],
            "repair_paths": [DEFAULT_REPAIR_RULE_PATH],
            "disabled_review_rule_ids": [],
            "disabled_repair_rule_ids": [],
        },
        "audit": {
            "jsonl_path": DEFAULT_AUDIT_JSONL_PATH,
            "report_output_path": DEFAULT_REPORT_OUTPUT_PATH,
            "recent_limit": 20,
        },
        "auth": {
            "base_url": "",
        },
        "automation": {
            "verify_commands": [
                "audit-risk check . --fail-on block",
                "audit-risk doctor .",
            ],
            "pre_commit_hook": DEFAULT_PRE_COMMIT_PATH,
            "ci_workflow": DEFAULT_CI_WORKFLOW_PATH,
            "fail_on_decision": "block",
        },
        "observe": {
            "bind": DEFAULT_OBSERVE_BIND,
            "dashboard_title": DEFAULT_OBSERVE_TITLE,
            "webhook_url": "",
        }
    });
    // The generated pre-commit hook calls `audit-risk` from PATH (or an
    // explicit AUDIT_RISK_BIN). It deliberately does NOT bake in this build
    // machine's path or `cargo run` — a customer installs a binary and has
    // neither the repo nor a Rust toolchain.
    vec![
        (
            ".hologram/delivery.json".to_string(),
            format!("{}\n", serde_json::to_string_pretty(&config).unwrap_or_else(|_| "{}".to_string())),
            false,
        ),
        (
            ".hologram/.gitignore".to_string(),
            // Volatile runtime state — never commit. delivery.json / rules stay
            // tracked (they're config the team shares).
            "# audit-risk 运行时状态，不要提交\nhologram.db\nhologram.db-shm\nhologram.db-wal\nbaseline.json\nlatest-risk-report.json\n".to_string(),
            false,
        ),
        (
            DEFAULT_REVIEW_RULE_PATH.to_string(),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({
                    "package_id": "review.workspace",
                    "version": "v1",
                    "plane": "review",
                    "source": "workspace_extension",
                    "enabled": true,
                    "description": "Workspace-specific review overrides for audit-risk CLI.",
                    "rules": [],
                }))
                .unwrap_or_else(|_| "{}".to_string())
            ),
            false,
        ),
        (
            DEFAULT_REPAIR_RULE_PATH.to_string(),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&json!({
                    "package_id": "repair.workspace",
                    "version": "v1",
                    "plane": "repair",
                    "source": "workspace_extension",
                    "enabled": true,
                    "description": "Workspace-specific repair overrides for audit-risk CLI.",
                    "rules": [],
                }))
                .unwrap_or_else(|_| "{}".to_string())
            ),
            false,
        ),
        (
            DEFAULT_PRE_COMMIT_PATH.to_string(),
            "#!/bin/sh\nset -eu\n\n# audit-risk 提交前门禁：阻断级风险会拦下提交。\n# 需要 audit-risk 在 PATH 中；自定义安装路径可设置 AUDIT_RISK_BIN。\n# 临时跳过：git commit --no-verify\n#\n# 启用本钩子：git config core.hooksPath .githooks（audit-risk init 会自动尝试设置）\n\nBIN=\"${AUDIT_RISK_BIN:-audit-risk}\"\nROOT=\"${1:-$(git rev-parse --show-toplevel 2>/dev/null || echo \"$PWD\")}\"\n\nif ! command -v \"$BIN\" >/dev/null 2>&1 && [ ! -x \"$BIN\" ]; then\n  echo \"[audit-risk] 未找到 audit-risk 可执行文件；跳过检查。请把它加入 PATH 或设置 AUDIT_RISK_BIN。\" >&2\n  exit 0\nfi\n\nexec \"$BIN\" check \"$ROOT\" --fail-on block\n".to_string(),
            true,
        ),
        (
            DEFAULT_CI_WORKFLOW_PATH.to_string(),
            // A real, runnable gate — installs the published binary and blocks
            // the PR on block-level findings. Falls back to cargo install from
            // source when no release asset is available yet.
            "name: audit-risk\n\
             \n\
             on:\n\
             \x20 pull_request:\n\
             \x20 push:\n\
             \x20   branches: [main]\n\
             \n\
             jobs:\n\
             \x20 audit-risk:\n\
             \x20   runs-on: ubuntu-latest\n\
             \x20   steps:\n\
             \x20     - uses: actions/checkout@v4\n\
             \x20       with:\n\
             \x20         fetch-depth: 0\n\
             \n\
             \x20     - name: Install audit-risk\n\
             \x20       run: |\n\
             \x20         set -e\n\
             \x20         if curl -sSf https://raw.githubusercontent.com/290864310liujiasheng-wq/hologram-audit-risk/main/install.sh | sh -s -- --prefix \"$HOME/.local\"; then\n\
             \x20           echo \"$HOME/.local/bin\" >> \"$GITHUB_PATH\"\n\
             \x20         else\n\
             \x20           echo \"预编译二进制不可用，回退到源码构建\"\n\
             \x20           cargo install --git https://github.com/290864310liujiasheng-wq/hologram-audit-risk --bin audit-risk hologram-engine --root \"$HOME/.local\"\n\
             \x20           echo \"$HOME/.local/bin\" >> \"$GITHUB_PATH\"\n\
             \x20         fi\n\
             \n\
             \x20     - name: audit-risk check（block 级风险拦截合并）\n\
             \x20       run: audit-risk check . --fail-on block\n"
                .to_string(),
            false,
        ),
    ]
}

fn resolve_existing_path(path: &str) -> Result<PathBuf, CliRuntimeError> {
    let cwd = std::env::current_dir()
        .map_err(|error| CliRuntimeError::environment(format!("failed to determine current directory: {error}")))?;
    let resolved = absolutize_path(&cwd, path);
    if resolved.exists() {
        Ok(resolved)
    } else {
        Err(CliRuntimeError::environment(format!(
            "path does not exist: {}",
            resolved.display()
        )))
    }
}

fn resolve_existing_workspace_path(path: &str) -> Result<PathBuf, CliRuntimeError> {
    let cwd = std::env::current_dir()
        .map_err(|error| CliRuntimeError::environment(format!("failed to determine current directory: {error}")))?;
    let resolved = resolve_workspace_argument(&cwd, path);
    if resolved.exists() {
        Ok(resolved)
    } else {
        Err(CliRuntimeError::environment(format!(
            "path does not exist: {}",
            resolved.display()
        )))
    }
}

fn default_workspace_root(base: &Path) -> PathBuf {
    base.to_path_buf()
}

fn resolve_workspace_argument(base: &Path, path: &str) -> PathBuf {
    if path == "." {
        return default_workspace_root(base);
    }
    absolutize_path(base, path)
}

fn absolutize_path(base: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        base.join(candidate)
    }
}

fn git_changed_files(workspace: &Path) -> Vec<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .arg("status")
        .arg("--short")
        // Expand untracked directories into individual files. Without this,
        // a brand-new folder shows up as a single `?? src/` entry — the exact
        // "AI just generated a batch of new files" case — and every per-file
        // scan (secrets included) silently misses because it gets a directory.
        .arg("--untracked-files=all")
        .output();

    match output {
        Ok(value) if value.status.success() => parse_git_status_changed_files(&String::from_utf8_lossy(&value.stdout)),
        _ => Vec::new(),
    }
}

fn parse_git_status_changed_files(raw: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in raw.lines() {
        if line.len() < 4 {
            continue;
        }
        let path_part = line[3..].trim();
        if path_part.is_empty() {
            continue;
        }
        let normalized = if let Some((_, to)) = path_part.split_once("->") {
            to.trim()
        } else {
            path_part
        };
        if !normalized.is_empty() && !normalized.starts_with("../") {
            files.push(normalize_path(normalized));
        }
    }
    files
}

#[derive(Debug, Clone)]
struct ObserveConfig {
    bind: String,
    dashboard_title: String,
    webhook_url: Option<String>,
}

#[derive(Debug)]
struct ObserveRuntime {
    local_url: String,
    public_url: String,
    qr_path: Option<String>,
    note: Option<String>,
    state: Arc<Mutex<Value>>,
}

impl ObserveRuntime {
    fn update(&self, payload: &Value) {
        if let Ok(mut guard) = self.state.lock() {
            *guard = payload.clone();
        }
    }

    fn observe_payload(&self) -> Value {
        json!({
            "local_url": self.local_url,
            "public_url": self.public_url,
            "qr_path": self.qr_path,
            "note": self.note,
        })
    }
}

fn load_observe_config(workspace: &Path) -> ObserveConfig {
    let delivery_path = workspace.join(".hologram/delivery.json");
    let observe = fs::read_to_string(&delivery_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.get("observe").cloned());
    ObserveConfig {
        bind: observe
            .as_ref()
            .and_then(|value| value.get("bind"))
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_OBSERVE_BIND)
            .to_string(),
        dashboard_title: observe
            .as_ref()
            .and_then(|value| value.get("dashboard_title"))
            .and_then(Value::as_str)
            .unwrap_or(DEFAULT_OBSERVE_TITLE)
            .to_string(),
        webhook_url: observe
            .and_then(|value| value.get("webhook_url").and_then(Value::as_str).map(str::to_string))
            .filter(|value| !value.trim().is_empty()),
    }
}

fn start_observe_runtime(workspace: &Path) -> Result<ObserveRuntime, CliRuntimeError> {
    let config = load_observe_config(workspace);
    let (listener, note) = bind_observe_listener(&config.bind)?;
    listener
        .set_nonblocking(true)
        .map_err(|error| CliRuntimeError::internal(format!("failed to set observe server nonblocking: {error}")))?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| CliRuntimeError::internal(format!("failed to resolve observe server address: {error}")))?;
    let host_ip = discover_local_ip().unwrap_or_else(|| "127.0.0.1".to_string());
    let public_url = format!("http://{}:{}", host_ip, local_addr.port());
    let local_url = format!("http://127.0.0.1:{}", local_addr.port());
    let state = Arc::new(Mutex::new(json!({
        "status": "waiting_for_first_scan",
        "workspace_root": normalize_path(workspace.display().to_string()),
    })));
    let state_for_thread = Arc::clone(&state);
    let dashboard_title = config.dashboard_title.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    let request_path = read_http_path(&mut stream).unwrap_or_else(|| "/".to_string());
                    let snapshot = state_for_thread.lock().ok().map(|guard| guard.clone()).unwrap_or_else(|| json!({}));
                    let (body, content_type) = if request_path == "/status.json" {
                        (
                            serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string()),
                            "application/json; charset=utf-8",
                        )
                    } else {
                        (
                            render_observe_dashboard_html(&dashboard_title, &snapshot),
                            "text/html; charset=utf-8",
                        )
                    };
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(100));
                }
                Err(_) => break,
            }
        }
    });

    Ok(ObserveRuntime {
        local_url,
        public_url: public_url.clone(),
        qr_path: generate_observe_qr_png(&public_url),
        note,
        state,
    })
}

fn bind_observe_listener(bind: &str) -> Result<(TcpListener, Option<String>), CliRuntimeError> {
    if let Ok(listener) = TcpListener::bind(bind) {
        return Ok((listener, None));
    }

    if let Some(port) = bind.strip_prefix("0.0.0.0:") {
        let localhost_bind = format!("127.0.0.1:{port}");
        if let Ok(listener) = TcpListener::bind(&localhost_bind) {
            return Ok((
                listener,
                Some(format!("observe server fallback: {bind} unavailable, using {localhost_bind}")),
            ));
        }
    }

    let fallback = "127.0.0.1:0";
    if let Ok(listener) = TcpListener::bind(fallback) {
        return Ok((
            listener,
            Some(format!("observe server fallback: {bind} unavailable, using {fallback}")),
        ));
    }

    Err(CliRuntimeError::environment(format!(
        "failed to bind observe server {bind}"
    )))
}

fn read_http_path(stream: &mut std::net::TcpStream) -> Option<String> {
    let mut buffer = [0u8; 1024];
    let read = stream.read(&mut buffer).ok()?;
    let request = String::from_utf8_lossy(&buffer[..read]);
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    let _method = parts.next()?;
    parts.next().map(str::to_string)
}

fn render_observe_dashboard_html(title: &str, snapshot: &Value) -> String {
    let pretty = serde_json::to_string_pretty(snapshot).unwrap_or_else(|_| "{}".to_string());
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"refresh\" content=\"2\"><title>{title}</title><style>body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:#0b0d12;color:#f3f4f6;padding:24px}}pre{{white-space:pre-wrap;background:#111827;padding:16px;border-radius:12px;overflow:auto}}h1{{font-size:20px}}</style></head><body><h1>{title}</h1><p>刷新频率：2 秒</p><pre>{pretty}</pre></body></html>"
    )
}

fn discover_local_ip() -> Option<String> {
    for interface in ["en0", "en1"] {
        let output = Command::new("ipconfig")
            .args(["getifaddr", interface])
            .output()
            .ok()?;
        if output.status.success() {
            let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !ip.is_empty() {
                return Some(ip);
            }
        }
    }
    let output = Command::new("ifconfig").output().ok()?;
    if !output.status.success() {
        return None;
    }
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if line.starts_with("inet ") {
            let ip = line.split_whitespace().nth(1)?.to_string();
            if ip != "127.0.0.1" && !ip.starts_with("198.18.") {
                return Some(ip);
            }
        }
    }
    None
}

fn generate_observe_qr_png(url: &str) -> Option<String> {
    let output_path = std::env::temp_dir().join(format!("audit-risk-observe-{}.png", uuid::Uuid::new_v4()));
    let script = format!(r#"
import Foundation
import CoreImage
import ImageIO
import UniformTypeIdentifiers

let text = "{url}"
let output = URL(fileURLWithPath: "{path}")
let data = text.data(using: .utf8)!
let filter = CIFilter(name: "CIQRCodeGenerator")!
filter.setValue(data, forKey: "inputMessage")
filter.setValue("M", forKey: "inputCorrectionLevel")
guard let image = filter.outputImage?.transformed(by: CGAffineTransform(scaleX: 8, y: 8)) else {{
  exit(2)
}}
let context = CIContext()
guard let cgImage = context.createCGImage(image, from: image.extent) else {{
  exit(3)
}}
guard let destination = CGImageDestinationCreateWithURL(output as CFURL, UTType.png.identifier as CFString, 1, nil) else {{
  exit(4)
}}
CGImageDestinationAddImage(destination, cgImage, nil)
if !CGImageDestinationFinalize(destination) {{
  exit(5)
}}
"#, url = escape_swift_string(url), path = escape_swift_string(&output_path.display().to_string()));
    let result = Command::new("/usr/bin/swift")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if result.status.success() {
        Some(output_path.display().to_string())
    } else {
        None
    }
}

fn escape_swift_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn resolve_webhook_url(workspace: &Path, cli_webhook: Option<&str>) -> Result<String, CliRuntimeError> {
    if let Some(url) = cli_webhook {
        return Ok(url.to_string());
    }
    if let Ok(url) = std::env::var("AUDIT_RISK_WEBHOOK_URL") {
        if !url.trim().is_empty() {
            return Ok(url);
        }
    }
    if let Some(url) = load_observe_config(workspace).webhook_url {
        return Ok(url);
    }
    Err(CliRuntimeError::environment(
        "webhook URL is missing; pass --webhook-url or configure observe.webhook_url in delivery.json.",
    ))
}

struct WebhookTestResult {
    ok: bool,
    http_status: String,
}

fn send_webhook_test(url: &str, payload: &Value) -> Result<WebhookTestResult, CliRuntimeError> {
    let output = Command::new("/usr/bin/curl")
        .args([
            "-sS",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "-H",
            "Content-Type: application/json",
            "-X",
            "POST",
            url,
            "-d",
            &serde_json::to_string(payload).map_err(|error| CliRuntimeError::internal(format!("failed to encode webhook payload: {error}")))?,
        ])
        .output()
        .map_err(|error| CliRuntimeError::environment(format!("failed to execute curl: {error}")))?;
    let http_status = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(WebhookTestResult {
        ok: output.status.success() && http_status.starts_with('2'),
        http_status,
    })
}

fn run_process(program: &str, args: &[String], cwd: Option<&Path>) -> Result<std::process::Output, CliRuntimeError> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.output().map_err(|error| {
        CliRuntimeError::environment(format!(
            "failed to execute `{program}`: {error}"
        ))
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EntitlementState {
    Active,
    Grace,
    Expired,
    Revoked,
    DeviceMismatch,
    Missing,
    Invalid,
}

#[derive(Debug, Clone)]
struct EntitlementStatus {
    state: EntitlementState,
    plan: Option<String>,
    valid_until: Option<String>,
    next_billing_at: Option<String>,
    payment_pending: bool,
    reason: String,
}

impl EntitlementStatus {
    fn is_pro_allowed(&self) -> bool {
        matches!(self.state, EntitlementState::Active | EntitlementState::Grace)
            && self.plan.as_deref() == Some(PRO_PERSONAL_PLAN)
    }
}

#[derive(Debug, Clone)]
struct AuthSessionStatus {
    session_id: String,
    status: String,
    login_url: String,
    expires_at: Option<String>,
}

impl AuthSessionStatus {
    fn is_pending_and_live(&self) -> bool {
        if self.status != "pending" {
            return false;
        }
        self.expires_at
            .as_deref()
            .and_then(parse_rfc3339_utc)
            .is_some_and(|expires_at| chrono::Utc::now() <= expires_at)
    }
}

#[derive(Debug, Clone)]
struct AuthServiceDiagnostic {
    code: &'static str,
    message: String,
}

impl AuthServiceDiagnostic {
    fn render_message(&self) -> String {
        format!("{}: {}", self.code, self.message)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthSessionDocument {
    session_id: String,
    status: String,
    created_at: String,
    expires_at: String,
    poll_interval_seconds: u64,
    timeout_seconds: u64,
    poll_url: String,
    exchange_url: String,
    login_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersonalEntitlementDocument {
    user_id: String,
    plan: String,
    features: Vec<String>,
    issued_at: String,
    valid_until: String,
    device_id: String,
    last_refresh_time: String,
    status: String,
    #[serde(default)]
    payment_pending: bool,
    next_billing_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthPollResponse {
    status: String,
    auth_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthExchangeRequest {
    auth_token: String,
    device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntitlementRefreshRequest {
    user_id: String,
    device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthEntitlementEnvelope {
    entitlement: PersonalEntitlementDocument,
    signature: String,
}

/// Real, current signal used to personalize the Pro paywall message instead
/// of a static feature list. Computed from data the Core tier already
/// produces (the same check pipeline `check`/`watch` use, and a plain
/// line-count of the existing audit log) — never a claim about what
/// Pro-tier detection would additionally find, since Core and Pro currently
/// run the identical detection engine. The paywall is honest about what
/// changes (command availability), not about a detection-depth difference
/// that doesn't exist.
#[derive(Debug, Clone, Default)]
struct ProGateContext {
    critical_count: usize,
    high_count: usize,
    audit_record_count: Option<usize>,
}

/// Compute `ProGateContext` for `feature` against `workspace`. Best-effort:
/// any failure (unreadable workspace, no cached report, no prior audit log)
/// falls back to an empty context, which renders the pre-existing generic
/// message — personalization is an enhancement, never a hard requirement.
///
/// Deliberately reads the cached `.hologram/latest-risk-report.json` (the
/// same file `read_last_review_summary` already reads for the home screen)
/// instead of triggering a fresh `engine_analyze`. The engine keeps a single
/// process-global instance (`static ENGINE: LazyLock<RwLock<Option<Engine>>>`
/// in engine.rs) — calling into it from a gate-check path that can run
/// concurrently with other engine-touching code (tests, or in principle a
/// second command) races that shared singleton. A plain file read has no
/// such hazard and matches the existing degrade-gracefully pattern used
/// elsewhere in this file.
fn build_pro_gate_context(feature: &str, workspace: &Path) -> ProGateContext {
    if feature == "history_compare" {
        let audit_path = workspace.join(DEFAULT_AUDIT_JSONL_PATH);
        let audit_record_count = fs::read_to_string(&audit_path)
            .ok()
            .map(|raw| raw.lines().filter(|line| !line.trim().is_empty()).count());
        return ProGateContext {
            audit_record_count,
            ..Default::default()
        };
    }

    let report_path = workspace.join(DEFAULT_REPORT_OUTPUT_PATH);
    let report = match fs::read_to_string(&report_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
    {
        Some(report) => report,
        None => return ProGateContext::default(),
    };
    let findings = report
        .pointer("/current_review/findings")
        .or_else(|| report.pointer("/review/findings"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let critical_count = findings
        .iter()
        .filter(|f| f["severity"].as_str() == Some("critical"))
        .count();
    let high_count = findings
        .iter()
        .filter(|f| f["severity"].as_str() == Some("high"))
        .count();
    ProGateContext {
        critical_count,
        high_count,
        audit_record_count: None,
    }
}

fn ensure_pro_feature(feature: &str, workspace: &Path) -> Result<(), CliRuntimeError> {
    let status = load_or_refresh_entitlement_status(&entitlement_dir());
    if status.is_pro_allowed() {
        return Ok(());
    }
    let context = build_pro_gate_context(feature, workspace);
    Err(CliRuntimeError::environment(render_pro_gate_message(feature, &status, &context)))
}

fn render_pro_gate_message(feature: &str, status: &EntitlementStatus, context: &ProGateContext) -> String {
    let (name, detail) = match feature {
        "observe" => ("手机观察", "把最近一次审查结果开成只读看板，方便你用手机或旁路设备盯状态。"),
        "notify" => ("告警推送", "把高风险审查结果推到 webhook，适合提交前或守护模式提醒。"),
        "history_compare" => ("历史风险对比", "把当前结果和历史审计样本放在一起看趋势，避免只凭单次扫描做判断。"),
        _ => ("Pro 增强功能", "解锁增强能力。"),
    };

    // Ground the pitch in the user's real, current situation instead of an
    // abstract feature list — but only when we actually have real data.
    // Core and Pro run the identical detection engine today, so this must
    // never claim Pro would find MORE; it only makes the value of the
    // gated command concrete against risk the user already has.
    let mut status_lines = vec![
        format!("当前视图：{name}"),
        format!("当前版本：{}", pro_status_label(status)),
        format!("价格：Pro 个人版 {PRO_PERSONAL_PRICE_LABEL}"),
    ];

    let personalized_detail = if feature == "history_compare" {
        match context.audit_record_count {
            Some(n) if n > 0 => {
                status_lines.push(format!("当前项目：已有 {n} 条历史审计记录"));
                ansi_bold_highlight(&format!("你已经积累了 {n} 条历史审计记录 —— {detail}"))
            }
            _ => {
                status_lines.push("当前项目：暂无历史审计记录".to_string());
                format!("继续用 check/watch 积累审计记录后，{detail}")
            }
        }
    } else {
        let urgent = context.critical_count + context.high_count;
        if urgent > 0 {
            status_lines.push(format!(
                "当前项目：{} 条严重风险、{} 条高危风险待处理",
                context.critical_count, context.high_count
            ));
            ansi_bold_highlight(&format!("你的项目当前有 {urgent} 条中高危以上风险待处理 —— {detail}"))
        } else {
            detail.to_string()
        }
    };

    render_product_shell(
        &status_lines,
        &[format!("{name} 是 Pro 个人版功能。")],
        &[personalized_detail],
        &[
            "先登录，再由授权状态机决定能不能放行。".to_string(),
            "Core 免费能力不会因为这个页面被锁死。".to_string(),
        ],
        &[
            "`audit-risk auth login`".to_string(),
            "`audit-risk auth status`".to_string(),
            "`audit-risk watch .`".to_string(),
        ],
        &["Core 免费功能仍可继续使用：check / watch / diff / init / doctor / 基础修复验证。".to_string()],
    )
}

fn entitlement_dir() -> PathBuf {
    if let Ok(path) = std::env::var("AUDIT_RISK_ENTITLEMENT_DIR") {
        return PathBuf::from(path);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".audit-risk/entitlement");
    }
    std::env::temp_dir().join("audit-risk/entitlement")
}

fn auth_base_url_for_workspace(workspace: &Path) -> Option<String> {
    let delivery_path = workspace.join(".hologram/delivery.json");
    if let Ok(raw) = fs::read_to_string(&delivery_path) {
        if let Ok(value) = serde_json::from_str::<Value>(&raw) {
            if let Some(base_url) = value
                .get("auth")
                .and_then(Value::as_object)
                .and_then(|auth| auth.get("base_url"))
                .and_then(Value::as_str)
            {
                let trimmed = base_url.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    std::env::var("AUDIT_RISK_AUTH_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn load_entitlement_status() -> EntitlementStatus {
    load_entitlement_status_from_dir(&entitlement_dir())
}

fn load_or_refresh_entitlement_status(dir: &Path) -> EntitlementStatus {
    let base_url = auth_base_url_for_workspace(dir);
    load_or_refresh_entitlement_status_with_base_url(dir, base_url.as_deref())
}

fn load_or_refresh_entitlement_status_with_base_url(
    dir: &Path,
    base_url: Option<&str>,
) -> EntitlementStatus {
    let status = load_entitlement_status_from_dir(dir);
    if should_refresh_entitlement(dir, &status) {
        if let Some(base_url) = base_url {
            if let Ok(refreshed) = refresh_entitlement_for_dir(dir, Some(base_url)) {
                return refreshed;
            }
        }
    }
    status
}

fn load_auth_session_from_dir(dir: &Path) -> Option<AuthSessionStatus> {
    let session_path = dir.join("session.json");
    let raw = fs::read_to_string(session_path).ok()?;
    let document = serde_json::from_str::<AuthSessionDocument>(&raw).ok()?;
    Some(AuthSessionStatus {
        session_id: document.session_id,
        status: document.status,
        login_url: document.login_url,
        expires_at: Some(document.expires_at),
    })
}

fn load_entitlement_status_from_dir(dir: &Path) -> EntitlementStatus {
    load_entitlement_status_from_dir_with_expected_device_id(
        dir,
        derive_device_id_for_dir(dir),
    )
}

fn load_entitlement_status_from_dir_with_expected_device_id(
    dir: &Path,
    expected_device_id: Result<String, CliRuntimeError>,
) -> EntitlementStatus {
    let json_path = dir.join("entitlement.json");
    let sig_path = dir.join("entitlement.sig");
    let device_secret_path = dir.join("device_secret");

    if !json_path.exists() {
        return EntitlementStatus {
            state: EntitlementState::Missing,
            plan: None,
            valid_until: None,
            next_billing_at: None,
            payment_pending: false,
            reason: "未登录，本机还没有 Pro 授权缓存。".to_string(),
        };
    }
    if !sig_path.exists() {
        return EntitlementStatus {
            state: EntitlementState::Invalid,
            plan: None,
            valid_until: None,
            next_billing_at: None,
            payment_pending: false,
            reason: "授权文件缺少 entitlement.sig，当前不能放行 Pro 功能。".to_string(),
        };
    }
    if !device_secret_path.exists() {
        return EntitlementStatus {
            state: EntitlementState::DeviceMismatch,
            plan: None,
            valid_until: None,
            next_billing_at: None,
            payment_pending: false,
            reason: "检测到 device_secret 丢失，请重新运行 audit-risk auth login 以绑定设备。".to_string(),
        };
    }

    let raw = match fs::read_to_string(&json_path) {
        Ok(raw) => raw,
        Err(error) => {
            return EntitlementStatus {
                state: EntitlementState::Invalid,
                plan: None,
                valid_until: None,
                next_billing_at: None,
                payment_pending: false,
                reason: format!("读取授权文件失败：{error}"),
            };
        }
    };

    // Verify the server signature before trusting any field in the JSON.
    let sig_raw = match fs::read_to_string(&sig_path) {
        Ok(sig) => sig,
        Err(error) => {
            return EntitlementStatus {
                state: EntitlementState::Invalid,
                plan: None,
                valid_until: None,
                next_billing_at: None,
                payment_pending: false,
                reason: format!("读取授权签名失败：{error}"),
            };
        }
    };
    match crate::entitlement::verify_entitlement_signature(&raw, sig_raw.trim()) {
        crate::entitlement::SignatureVerifyResult::Valid => {}
        crate::entitlement::SignatureVerifyResult::Malformed => {
            return EntitlementStatus {
                state: EntitlementState::Invalid,
                plan: None,
                valid_until: None,
                next_billing_at: None,
                payment_pending: false,
                reason: "授权签名格式错误，文件可能已损坏。".to_string(),
            };
        }
        crate::entitlement::SignatureVerifyResult::Invalid => {
            return EntitlementStatus {
                state: EntitlementState::Invalid,
                plan: None,
                valid_until: None,
                next_billing_at: None,
                payment_pending: false,
                reason: "授权签名验证失败，授权文件可能已被篡改或来自未知服务器。".to_string(),
            };
        }
    }

    let document = match serde_json::from_str::<PersonalEntitlementDocument>(&raw) {
        Ok(value) => value,
        Err(error) => {
            return EntitlementStatus {
                state: EntitlementState::Invalid,
                plan: None,
                valid_until: None,
                next_billing_at: None,
                payment_pending: false,
                reason: format!("授权文件不是合法 JSON：{error}"),
            };
        }
    };

    let plan = Some(document.plan.clone());
    let valid_until = Some(document.valid_until.clone());
    let next_billing_at = document.next_billing_at.clone();
    let payment_pending = document.payment_pending;
    let remote_status = document.status.as_str();

    if remote_status == "revoked" {
        return EntitlementStatus {
            state: EntitlementState::Revoked,
            plan,
            valid_until,
            next_billing_at,
            payment_pending,
            reason: "服务端已撤销这个授权。".to_string(),
        };
    }
    if remote_status != "active" {
        return EntitlementStatus {
            state: EntitlementState::Invalid,
            plan,
            valid_until,
            next_billing_at,
            payment_pending,
            reason: format!("授权文件里的 status `{remote_status}` 不在当前 CLI 合同内。"),
        };
    }

    let expected_device_id = match expected_device_id {
        Ok(device_id) => device_id,
        Err(error) => {
            return EntitlementStatus {
                state: EntitlementState::DeviceMismatch,
                plan,
                valid_until,
                next_billing_at,
                payment_pending,
                reason: format!(
                    "无法确认当前设备身份：{}。请重新运行 audit-risk auth login。",
                    error.message
                ),
            };
        }
    };
    if expected_device_id != document.device_id {
        return EntitlementStatus {
            state: EntitlementState::DeviceMismatch,
            plan,
            valid_until,
            next_billing_at,
            payment_pending,
            reason: "授权绑定的 device_id 与当前设备不一致，请重新运行 audit-risk auth login。".to_string(),
        };
    }

    let Some(valid_until_raw) = valid_until.as_deref() else {
        return EntitlementStatus {
            state: EntitlementState::Invalid,
            plan,
            valid_until,
            next_billing_at,
            payment_pending,
            reason: "授权文件缺少 valid_until。".to_string(),
        };
    };
    let Some(valid_until_time) = parse_rfc3339_utc(valid_until_raw) else {
        return EntitlementStatus {
            state: EntitlementState::Invalid,
            plan,
            valid_until,
            next_billing_at,
            payment_pending,
            reason: "授权文件里的 valid_until 不是合法 RFC3339 时间。".to_string(),
        };
    };

    let now = chrono::Utc::now();
    let state = if now <= valid_until_time {
        EntitlementState::Active
    } else if now <= valid_until_time + chrono::Duration::hours(ENTITLEMENT_GRACE_HOURS) {
        EntitlementState::Grace
    } else {
        EntitlementState::Expired
    };
    let reason = match state {
        EntitlementState::Active => "授权有效。".to_string(),
        EntitlementState::Grace => "授权已过期，但仍在 72 小时宽限期内。".to_string(),
        EntitlementState::Expired => "授权和宽限期都已过期。".to_string(),
        _ => "授权状态不可用。".to_string(),
    };

    EntitlementStatus {
        state,
        plan,
        valid_until,
        next_billing_at,
        payment_pending,
        reason,
    }
}

fn parse_rfc3339_utc(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&chrono::Utc))
}

fn derive_device_id_for_dir(dir: &Path) -> Result<String, CliRuntimeError> {
    let secret = fs::read_to_string(dir.join("device_secret"))
        .map_err(|error| CliRuntimeError::environment(format!("无法读取 device_secret：{error}")))?;
    let machine_identity = current_machine_identity()?;
    Ok(derive_device_id(
        secret.trim(),
        std::env::consts::OS,
        &machine_identity,
    ))
}

fn derive_device_id(secret: &str, os: &str, machine_identity: &str) -> String {
    let source = format!("{secret}|{os}|{machine_identity}");
    sha256_hex(source.as_bytes())
}

#[cfg(any(target_os = "macos", test))]
fn parse_ioreg_platform_uuid(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        if key.trim().trim_matches('"') != "IOPlatformUUID" {
            return None;
        }
        let value = value.trim().trim_matches('"').trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(any(target_os = "windows", test))]
fn parse_windows_machine_guid(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        if fields.next()? != "MachineGuid" || fields.next()? != "REG_SZ" {
            return None;
        }
        let value = fields.next()?;
        (!value.is_empty()).then(|| value.to_string())
    })
}

#[cfg(target_os = "macos")]
fn current_machine_identity() -> Result<String, CliRuntimeError> {
    let output = Command::new("/usr/sbin/ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .map_err(|error| CliRuntimeError::environment(format!("无法读取 macOS 设备标识：{error}")))?;
    if !output.status.success() {
        return Err(CliRuntimeError::environment(format!(
            "读取 macOS 设备标识失败，ioreg 退出码为 {}。",
            output.status.code().unwrap_or(-1)
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_ioreg_platform_uuid(&stdout)
        .ok_or_else(|| CliRuntimeError::environment("ioreg 未返回 IOPlatformUUID，无法绑定授权设备。"))
}

#[cfg(target_os = "linux")]
fn current_machine_identity() -> Result<String, CliRuntimeError> {
    for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(value) = fs::read_to_string(path) {
            let value = value.trim();
            if !value.is_empty() {
                return Ok(value.to_string());
            }
        }
    }
    Err(CliRuntimeError::environment(
        "无法读取 Linux machine-id，不能安全绑定授权设备。",
    ))
}

#[cfg(target_os = "windows")]
fn current_machine_identity() -> Result<String, CliRuntimeError> {
    let output = Command::new(windows_system_directory()?.join("reg.exe"))
        .args([
            "query",
            r"HKLM\SOFTWARE\Microsoft\Cryptography",
            "/v",
            "MachineGuid",
        ])
        .output()
        .map_err(|error| CliRuntimeError::environment(format!("无法读取 Windows 设备标识：{error}")))?;
    if !output.status.success() {
        return Err(CliRuntimeError::environment(format!(
            "读取 Windows 设备标识失败，reg 退出码为 {}。",
            output.status.code().unwrap_or(-1)
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_windows_machine_guid(&stdout)
        .ok_or_else(|| CliRuntimeError::environment("注册表未返回 MachineGuid，无法绑定授权设备。"))
}

#[cfg(target_os = "windows")]
fn windows_system_directory() -> Result<PathBuf, CliRuntimeError> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    }

    let mut buffer = vec![0u16; 260];
    loop {
        let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
        if length == 0 {
            return Err(CliRuntimeError::environment(
                "Windows GetSystemDirectoryW 失败，无法安全读取 MachineGuid。",
            ));
        }
        if (length as usize) < buffer.len() {
            return Ok(PathBuf::from(OsString::from_wide(
                &buffer[..length as usize],
            )));
        }
        buffer.resize(length as usize + 1, 0);
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn current_machine_identity() -> Result<String, CliRuntimeError> {
    Err(CliRuntimeError::environment(format!(
        "当前操作系统 `{}` 暂不支持稳定设备绑定。",
        std::env::consts::OS
    )))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
}

/// Fixed inner content width the box UI was authored against. Every panel
/// line in the human-mode screens was written assuming this width; this is
/// NOT an arbitrary choice we can shrink to fit a narrower terminal without
/// risking already-authored lines overflowing the new, smaller inner width.
const BOX_WIDTH: usize = 92;
/// Below this many terminal columns, drawing the BOX_WIDTH box would
/// overflow the real terminal and wrap mid-line — the exact failure mode
/// that used to render as unreadable solid color bars. Below this
/// threshold we switch to Plain mode instead of trying to shrink the box,
/// because shrinking would require re-wrapping every already-authored
/// content line to a new width, which is a much larger surface to get
/// right than simply not drawing a box at all.
const MIN_TERMINAL_WIDTH_FOR_BOX: usize = BOX_WIDTH + 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderMode {
    /// Draw the bordered box UI at the fixed, content-authored width.
    Boxed,
    /// No box, no fixed-width padding math at all — headings and indented
    /// lines only, relying on the terminal's own line-wrapping. Used when
    /// the terminal is too narrow for the box. Because there is no manual
    /// width calculation in this mode, it cannot overflow regardless of
    /// terminal width, content length, or CJK/ASCII mix — the failure mode
    /// this whole rendering path is prone to is structurally impossible
    /// here rather than merely handled.
    Plain,
}

/// Query the real terminal width. Returns `None` when stdout is not a TTY
/// (piped, redirected to a file, or running under a harness with no
/// controlling terminal) — in that case we keep the existing fixed-width
/// box, matching every prior release's behavior for redirected output.
fn detect_terminal_width() -> Option<usize> {
    terminal_size::terminal_size().map(|(terminal_size::Width(w), _)| w as usize)
}

/// Pure decision function, deliberately separated from the OS query above
/// so it can be unit tested without a real terminal.
fn decide_render_mode(detected_width: Option<usize>) -> RenderMode {
    match detected_width {
        Some(width) if width < MIN_TERMINAL_WIDTH_FOR_BOX => RenderMode::Plain,
        _ => RenderMode::Boxed,
    }
}

fn render_product_shell(
    status_lines: &[String],
    problem_lines: &[String],
    why_lines: &[String],
    advice_lines: &[String],
    next_steps: &[String],
    note_lines: &[String],
) -> String {
    let mode = decide_render_mode(detect_terminal_width());
    let bg = "\u{1b}[48;5;232m";
    let panel = "\u{1b}[48;5;234m";
    let border = "\u{1b}[38;5;240m";
    let title = "\u{1b}[38;5;45m";
    let text = "\u{1b}[38;5;252m";
    let muted = "\u{1b}[38;5;245m";
    let bright = "\u{1b}[38;5;255m";
    let green = "\u{1b}[38;5;77m";
    let yellow = "\u{1b}[38;5;220m";
    let blue = "\u{1b}[38;5;39m";
    let reset = "\u{1b}[0m";

    if mode == RenderMode::Plain {
        let mut lines = Vec::new();
        lines.push(format!("{bright}audit-risk{reset}"));
        lines.push(format!("{muted}AI 编码风控平台 — 为 AI 生成的代码提供实时审查、规则拦截和不可篡改的审计证据{reset}"));
        lines.push(String::new());
        lines.push(render_panel_plain("当前概览", status_lines, title, text, muted, reset));
        lines.push(render_panel_plain(
            "问题说明",
            &compose_problem_block(problem_lines, why_lines, advice_lines),
            yellow,
            text,
            muted,
            reset,
        ));
        lines.push(render_panel_plain("下一步", next_steps, blue, text, muted, reset));
        if !note_lines.is_empty() {
            lines.push(render_panel_plain("说明", note_lines, green, text, muted, reset));
        }
        return lines.join("\n");
    }

    let mut lines = Vec::new();
    lines.push(format!("{bg}{bright}  audit-risk{reset}"));
    lines.push(String::new());
    lines.push(format!("{bg}{bright}AI 编码风控平台 · {title}audit-risk{reset}"));
    lines.push(format!("{bg}{muted}为 AI 生成的代码提供实时审查、规则拦截和不可篡改的审计证据{reset}"));
    lines.push(String::new());
    lines.push(render_panel(
        "当前概览",
        status_lines,
        panel,
        border,
        title,
        text,
        muted,
        reset,
    ));
    lines.push(String::new());
    lines.push(render_panel(
        "问题说明",
        &compose_problem_block(problem_lines, why_lines, advice_lines),
        panel,
        border,
        yellow,
        text,
        muted,
        reset,
    ));
    lines.push(String::new());
    lines.push(render_panel(
        "下一步",
        next_steps,
        panel,
        border,
        blue,
        text,
        muted,
        reset,
    ));
    if !note_lines.is_empty() {
        lines.push(String::new());
        lines.push(render_panel(
            "说明",
            note_lines,
            panel,
            border,
            green,
            text,
            muted,
            reset,
        ));
    }
    lines.push(reset.to_string());
    lines.join("\n")
}

/// Borderless counterpart to render_panel — heading + indented lines, no
/// fixed-width padding math at all. See RenderMode::Plain for why this
/// exists instead of a narrower box.
fn render_panel_plain(
    heading: &str,
    lines: &[String],
    accent: &str,
    text: &str,
    muted: &str,
    reset: &str,
) -> String {
    let mut rows = Vec::new();
    rows.push(String::new());
    rows.push(format!("{accent}── {heading} ──{reset}"));
    if lines.is_empty() {
        rows.push(format!("{muted}· 暂无{reset}"));
    } else {
        for line in lines {
            if line.is_empty() {
                rows.push(String::new());
                continue;
            }
            if line == "这是什么问题" || line == "为什么要管" || line == "建议动作" {
                rows.push(format!("{accent}{line}{reset}"));
            } else {
                rows.push(format!("{text}{}{reset}", decorate_bullet_line(line)));
            }
        }
    }
    rows.join("\n")
}



fn compose_problem_block(
    problem_lines: &[String],
    why_lines: &[String],
    advice_lines: &[String],
) -> Vec<String> {
    let mut rows = vec!["这是什么问题".to_string()];
    rows.extend(render_section_lines(problem_lines));
    rows.push(String::new());
    rows.push("为什么要管".to_string());
    rows.extend(render_section_lines(why_lines));
    rows.push(String::new());
    rows.push("建议动作".to_string());
    rows.extend(render_section_lines(advice_lines));
    rows
}

#[allow(clippy::too_many_arguments)]
fn render_panel(
    heading: &str,
    lines: &[String],
    panel: &str,
    border: &str,
    accent: &str,
    text: &str,
    muted: &str,
    reset: &str,
) -> String {
    let width = BOX_WIDTH;
    let inner = width.saturating_sub(4);
    let mut rows = Vec::new();
    rows.push(format!(
        "{panel}{border}╭{:─<1$}╮{reset}",
        "",
        width.saturating_sub(2)
    ));
    rows.push(panel_line(
        &format!("{accent}{heading}{reset}"),
        inner,
        panel,
        border,
        reset,
    ));
    rows.push(format!(
        "{panel}{border}├{:─<1$}┤{reset}",
        "",
        width.saturating_sub(2)
    ));

    if lines.is_empty() {
        rows.push(panel_line(
            &format!("{muted}· 暂无{reset}"),
            inner,
            panel,
            border,
            reset,
        ));
    } else {
        for line in lines {
            if line.is_empty() {
                rows.push(panel_line("", inner, panel, border, reset));
                continue;
            }
            let rendered = if line == "这是什么问题"
                || line == "为什么要管"
                || line == "建议动作"
            {
                format!("{accent}{line}{reset}")
            } else {
                format!("{text}{}{reset}", decorate_bullet_line(line))
            };
            rows.push(panel_line(&rendered, inner, panel, border, reset));
        }
    }

    rows.push(format!(
        "{panel}{border}╰{:─<1$}╯{reset}",
        "",
        width.saturating_sub(2)
    ));
    rows.join("\n")
}

fn decorate_bullet_line(line: &str) -> String {
    if let Some(rest) = line.strip_prefix("- ") {
        format!("• {rest}")
    } else {
        line.to_string()
    }
}

fn panel_line(content: &str, inner_width: usize, panel: &str, border: &str, reset: &str) -> String {
    let visible = strip_ansi(content);
    // Use terminal display width, not char count — CJK characters render as
    // 2 columns wide in virtually all terminals. Counting them as 1 (via
    // .chars().count()) under-measures the line, over-pads it past the
    // intended box width, and the background-color escape bleeds across
    // the rest of the row once the line wraps past the terminal's actual
    // column width.
    let pad = inner_width.saturating_sub(UnicodeWidthStr::width(visible.as_str()));
    format!("{panel}{border}│ {content}{}{border} │{reset}", " ".repeat(pad))
}

fn strip_ansi(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn render_section_lines(lines: &[String]) -> Vec<String> {
    if lines.is_empty() {
        vec!["- 暂无".to_string()]
    } else {
        lines.iter().map(|line| format!("- {line}")).collect()
    }
}

fn render_help_screen() -> String {
    render_product_shell(
        &[
            "当前视图：命令总览".to_string(),
            "适用人群：正在用 AI 写代码、但不想靠肉眼盯风险的人".to_string(),
            format!("版本分层：Core 免费版 / Pro 个人版 {PRO_PERSONAL_PRICE_LABEL}"),
        ],
        &[
            "你现在看到的是 audit-risk 的总导航，不是报错页。".to_string(),
            "每个命令都对应同一产品里的一个页面或工作步骤。".to_string(),
        ],
        &[
            "如果入口像一堆散命令，用户就需要自己拼流程，产品会退化成脚本集合。".to_string(),
            "把首页、体检、审查、报告和授权讲清楚，才能在一个终端里完整使用。".to_string(),
        ],
        &[
            "零参数先看首页，确定当前目录状态。".to_string(),
            "第一次接入先跑 `audit-risk init .`。".to_string(),
            "接入后按 `doctor -> watch -> check -> report` 走主路径。".to_string(),
            "需要机器读取时，在支持的命令后加 `--json`。".to_string(),
        ],
        &[
            "`audit-risk`".to_string(),
            "`audit-risk tour`".to_string(),
            "`audit-risk --version`".to_string(),
            "`audit-risk init <目录>`".to_string(),
            "`audit-risk doctor [目录]`".to_string(),
            "`audit-risk watch <目录>`".to_string(),
            "`audit-risk check <目录>`  # 审查当前工作区/Git 改动".to_string(),
            "`audit-risk diff <旧> <新>`  # 对比两个目录或文件".to_string(),
            "`audit-risk report [目录]`".to_string(),
            "`audit-risk report <目录> --history-compare`".to_string(),
            "`audit-risk observe [目录]`".to_string(),
            "`audit-risk notify [目录] --test`".to_string(),
            "`audit-risk auth status`".to_string(),
        ],
        &[
            "Core 免费保留：首页、help/tour、init、doctor、check、watch、diff、基础解释、基础报告。".to_string(),
            "Pro 个人版保留：高级规则包、历史风险对比、增强报告、observe、notify、个人规则自定义加载。".to_string(),
        ],
    )
}

fn gate_decision_label(decision: &str) -> &'static str {
    match decision {
        "allow" => "通过",
        "warn" => "告警",
        "require_approval" => "需要人工确认",
        "block" => "阻断",
        _ => "未知",
    }
}

fn severity_label(severity: &str) -> &'static str {
    match severity {
        "critical" => "严重",
        "high" => "高风险",
        "medium" => "中风险",
        "low" => "低风险",
        _ => "未分级",
    }
}

fn format_finding_line(entry: &Value) -> String {
    let severity_str = entry.get("severity").and_then(Value::as_str).unwrap_or("low");
    let severity = severity_label(severity_str);
    let explanation = entry
        .get("plain_explanation")
        .and_then(Value::as_str)
        .unwrap_or("发现一条需要注意的风险。");
    let location = entry
        .get("location")
        .and_then(Value::as_object)
        .map(|loc| {
            format!(
                "{}:{}",
                loc.get("file_path").and_then(Value::as_str).unwrap_or("unknown"),
                loc.get("start_line").and_then(Value::as_u64).unwrap_or(1)
            )
        })
        .unwrap_or_else(|| "unknown".to_string());
    let line = format!("{severity} · {location} · {explanation}");
    // Color by severity so the eye lands on critical/high findings first —
    // matches the coloring watch mode already applies, now shared by
    // check/report/diff's panel-based finding preview too.
    match severity_str {
        "critical" | "high" => ansi_red(&line),
        "medium" => ansi_yellow(&line),
        "low" => ansi_dim(&line),
        _ => line,
    }
}

fn render_check_screen(payload: &Value, verbose: bool) -> Result<String, CliRuntimeError> {
    let workspace = payload
        .get("workspace_root")
        .and_then(Value::as_str)
        .unwrap_or("未知目录");
    let gate = payload
        .pointer("/review/gate_decision/decision")
        .and_then(Value::as_str)
        .unwrap_or("allow");
    let reason = payload
        .pointer("/review/gate_decision/reason")
        .and_then(Value::as_str)
        .unwrap_or("这次审查没有返回结论说明。");
    let findings = payload
        .pointer("/review/findings")
        .and_then(Value::as_array)
        .ok_or_else(|| CliRuntimeError::internal("check payload is missing review.findings"))?;
    // Noise control: by default only surface high-confidence findings
    // (critical/high — secrets, injection, dangerous execution). Structural
    // coupling signals (medium/low) collapse to a one-line count; --verbose
    // or --json shows everything. False-alarm fatigue kills security tools.
    let is_high_confidence = |entry: &&Value| {
        matches!(
            entry.get("severity").and_then(Value::as_str),
            Some("critical") | Some("high")
        )
    };
    let high_count = findings.iter().filter(is_high_confidence).count();
    let low_count = findings.len() - high_count;
    let (preview_pool, hidden_note): (Vec<&Value>, Option<String>) = if verbose {
        (findings.iter().collect(), None)
    } else {
        let pool: Vec<&Value> = findings.iter().filter(is_high_confidence).collect();
        let note = (low_count > 0).then(|| {
            format!("另有 {low_count} 条结构耦合信号（中/低风险）已折叠，用 `--verbose` 查看。")
        });
        (pool, note)
    };
    let finding_preview = preview_pool
        .iter()
        .take(if verbose { 10 } else { 3 })
        .map(|entry| format_finding_line(entry))
        .collect::<Vec<_>>();
    let risk_count_line = if verbose || low_count == 0 {
        format!("风险条数：{} 条", findings.len())
    } else {
        format!("风险条数：{high_count} 条高置信度（另有 {low_count} 条结构信号折叠）")
    };
    Ok(render_product_shell(
        &[
            format!("当前视图：项目审查（{workspace}）"),
            format!("审查结论：{}", gate_decision_label(gate)),
            risk_count_line,
        ],
        &std::iter::once(reason.to_string())
            .chain(finding_preview.clone())
            .chain(hidden_note)
            .collect::<Vec<_>>(),
        &[match gate {
            "allow" => "当前没有触发需要拦截的风险，这次变更可以继续推进。".to_string(),
            "warn" => "这次变更已经出现需要你确认的风险信号，不建议直接忽略。".to_string(),
            "require_approval" => "这次变更命中了高风险规则，应该先让人看清楚再继续。".to_string(),
            "block" => "这次变更已经达到阻断阈值，继续提交会把风险带进主线。".to_string(),
            _ => "当前结果不完整，需要重新审查确认。".to_string(),
        }],
        &[
            "先处理高置信度风险（密钥/注入/危险执行），确认是不是业务必须。".to_string(),
            "如果只是测试或演练代码，明确隔离到非生产路径。".to_string(),
            "需要自动化消费时，改用 `audit-risk check <目录> --json`。".to_string(),
        ],
        &[
            "`audit-risk check . --verbose`".to_string(),
            "`audit-risk watch .`".to_string(),
            "`audit-risk report .`".to_string(),
            "`audit-risk check . --json`".to_string(),
        ],
        &[
            format!("原始 gate 值：{gate}"),
            "JSON 合同键名保持英文；这里只有人类模式文案被中文化。".to_string(),
        ],
    ))
}

fn render_diff_screen(payload: &Value) -> Result<String, CliRuntimeError> {
    let after_root = payload
        .pointer("/analysis/after_root")
        .and_then(Value::as_str)
        .or_else(|| payload.get("workspace_root").and_then(Value::as_str))
        .unwrap_or("未知目录");
    let gate = payload
        .pointer("/review/gate_decision/decision")
        .and_then(Value::as_str)
        .unwrap_or("allow");
    let findings = payload
        .pointer("/review/findings")
        .and_then(Value::as_array)
        .ok_or_else(|| CliRuntimeError::internal("diff payload is missing review.findings"))?;
    let finding_preview = findings
        .iter()
        .take(3)
        .map(format_finding_line)
        .collect::<Vec<_>>();
    Ok(render_product_shell(
        &[
            "当前视图：变更对比审查".to_string(),
            format!("对比目标：{after_root}"),
            format!("审查结论：{}", gate_decision_label(gate)),
        ],
        &std::iter::once(format!("本次对比共识别 {} 条风险线索。", findings.len()))
            .chain(finding_preview)
            .collect::<Vec<_>>(),
        &["在目录或文件对比场景里，风险往往不是出在单个文件，而是出在新旧行为差异。".to_string()],
        &[
            "先看高风险差异，再决定是否需要更细的人工复审。".to_string(),
            "需要机器消费时，改用 `audit-risk diff <旧> <新> --json`。".to_string(),
        ],
        &[
            "`audit-risk report .`".to_string(),
            "`audit-risk diff <旧> <新> --json`".to_string(),
        ],
        &[],
    ))
}

fn render_init_screen(payload: &Value) -> Result<String, CliRuntimeError> {
    let workspace = payload
        .get("workspace_root")
        .and_then(Value::as_str)
        .unwrap_or("未知目录");
    let created_files = payload
        .get("created_files")
        .and_then(Value::as_array)
        .ok_or_else(|| CliRuntimeError::internal("init payload is missing created_files"))?
        .iter()
        .filter_map(Value::as_str)
        .map(|item| format!("已生成：{item}"))
        .collect::<Vec<_>>();
    let hook_activation = payload
        .get("hook_activation")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut suggestions = created_files.clone();
    if !hook_activation.is_empty() {
        suggestions.push(format!("提交前拦截：{hook_activation}"));
    }
    Ok(render_product_shell(
        &[
            "当前视图：项目接入".to_string(),
            format!("目标目录：{workspace}"),
            format!("接入结果：已生成 {} 个文件", created_files.len()),
        ],
        &["这个目录现在已经有 audit-risk 的最小接入骨架。".to_string()],
        &[
            "没有接入文件时，后续的规则、审计路径和自动化入口就没有统一真源。".to_string(),
            "先把骨架生成出来，后面的体检、守护和报告才有地方落。".to_string(),
        ],
        &suggestions,
        &[
            "`audit-risk doctor .`".to_string(),
            "`audit-risk watch .`".to_string(),
            "`audit-risk init . --json`".to_string(),
        ],
        &["如果你在自动化里要读取创建结果，改用 `--json`。".to_string()],
    ))
}

fn render_doctor_screen(payload: &Value) -> Result<String, CliRuntimeError> {
    let workspace = payload
        .get("workspace_root")
        .and_then(Value::as_str)
        .unwrap_or("未知目录");
    let status = payload.get("status").and_then(Value::as_str).unwrap_or("needs_attention");
    let checks = payload
        .get("checks")
        .and_then(Value::as_array)
        .ok_or_else(|| CliRuntimeError::internal("doctor payload is missing checks"))?;
    let blockers = payload
        .get("blockers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let notes = payload
        .get("notes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let preview = checks
        .iter()
        .take(6)
        .map(|check| {
            let name = check.get("name").and_then(Value::as_str).unwrap_or("unknown");
            let item_status = check.get("status").and_then(Value::as_str).unwrap_or("unknown");
            format!("{name}：{item_status}")
        })
        .collect::<Vec<_>>();
    let problem_lines = if blockers.is_empty() {
        std::iter::once("当前没有阻断项。".to_string())
            .chain(preview.clone())
            .collect::<Vec<_>>()
    } else {
        blockers.clone()
    };
    Ok(render_product_shell(
        &[
            "当前视图：环境体检".to_string(),
            format!("目标目录：{workspace}"),
            format!("总体状态：{status}"),
        ],
        &problem_lines,
        &[match status {
            "ready" => "当前目录已经具备最小运行条件，可以直接进入守护和审查。".to_string(),
            "error" => "有些基础条件不满足，继续运行只会把错误推迟到更靠后的位置。".to_string(),
            _ => "现在还不是完全阻断，但已经有注意项，最好先补齐。".to_string(),
        }],
        &if notes.is_empty() {
            vec!["如果要给脚本消费，改用 `audit-risk doctor <目录> --json`。".to_string()]
        } else {
            notes.clone()
        },
        &[
            "`audit-risk watch .`".to_string(),
            "`audit-risk check .`".to_string(),
            "`audit-risk doctor . --json`".to_string(),
        ],
        &["doctor 的 JSON 键名继续稳定英文，便于脚本和 CI 读取。".to_string()],
    ))
}

fn render_report_screen(report: &Value) -> Result<String, CliRuntimeError> {
    let workspace = report
        .pointer("/workspace/root")
        .and_then(Value::as_str)
        .or_else(|| report.get("workspace_root").and_then(Value::as_str))
        .unwrap_or("未知目录");
    let gate = report
        .pointer("/current_review/gate_decision/decision")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let integrity = report
        .pointer("/audit/integrity/status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let generated_at = report.get("generated_at").and_then(Value::as_str).unwrap_or("未知时间");
    Ok(render_product_shell(
        &[
            "当前视图：审计报告".to_string(),
            format!("目标目录：{workspace}"),
            format!("最近 gate：{}", gate_decision_label(gate)),
            format!("审计完整性：{integrity}"),
        ],
        &[
            "这份报告已经按当前 workspace 收口了 review、policy、audit 和 automation 四个面。".to_string(),
            format!("生成时间：{generated_at}"),
        ],
        &[
            "报告不是给机器看的附带产物，它是把本次审查证据固定下来，方便复盘、交接和留痕。".to_string(),
            "如果只看零散命令输出，后面很难证明当时到底审到了什么。".to_string(),
        ],
        &[
            "把这份报告当成当前风险结论的对外版本。".to_string(),
            "如果你要交给脚本或 CI，下次直接改用 `audit-risk report <目录> --json`。".to_string(),
        ],
        &[
            "`audit-risk auth status`".to_string(),
            "`audit-risk report . --json`".to_string(),
            "`audit-risk watch .`".to_string(),
        ],
        &["JSON 报告文件仍会按 delivery 配置继续落盘；这里展示的是人类模式摘要。".to_string()],
    ))
}

fn render_history_compare_screen(report: &Value) -> Result<String, CliRuntimeError> {
    let workspace = report
        .pointer("/workspace/root")
        .and_then(Value::as_str)
        .or_else(|| report.get("workspace_root").and_then(Value::as_str))
        .unwrap_or("未知目录");
    let records = report
        .pointer("/audit/records")
        .and_then(Value::as_array)
        .ok_or_else(|| CliRuntimeError::internal("report payload is missing audit.records"))?;
    let history_line = if records.len() >= 2 {
        format!("最近可用审计样本：{} 条，已经具备历史对比基础。", records.len())
    } else {
        "当前可用历史样本不足 2 条，还不能给出稳定的趋势对比。".to_string()
    };
    Ok(render_product_shell(
        &[
            "当前视图：历史风险对比".to_string(),
            format!("目标目录：{workspace}"),
            format!("历史样本：{} 条", records.len()),
        ],
        &[history_line],
        &["历史风险对比是 Pro 能力，因为它依赖持续留痕后的长期趋势，而不是一次性扫描结果。".to_string()],
        &[
            "继续让 watch / check / report 留下更多审计样本。".to_string(),
            "需要自动化导出时，依旧使用 `audit-risk report <目录> --json`。".to_string(),
        ],
        &[
            "`audit-risk watch .`".to_string(),
            "`audit-risk report .`".to_string(),
            "`audit-risk report . --json`".to_string(),
        ],
        &["这次页面先交付统一终端壳和 Pro gate；更细的趋势算法仍沿用后续报告面演进。".to_string()],
    ))
}

fn render_notify_screen(payload: &Value) -> Result<String, CliRuntimeError> {
    let tested_url = payload
        .get("tested_url")
        .and_then(Value::as_str)
        .unwrap_or("未知地址");
    let http_status = payload
        .get("http_status")
        .and_then(Value::as_i64)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "未知".to_string());
    let ok = payload.get("ok").and_then(Value::as_bool).unwrap_or(false);
    Ok(render_product_shell(
        &[
            "当前视图：告警推送测试".to_string(),
            format!("测试地址：{tested_url}"),
            format!("连通结果：{}", if ok { "通过" } else { "失败" }),
        ],
        &[format!("HTTP 状态码：{http_status}")],
        &["这一步是为了确认 Pro 告警推送链路在真正出风险前就能打通。".to_string()],
        &[
            "先修地址和鉴权，再谈接入观察和告警。".to_string(),
            "如果你要让脚本读取结果，改用 `--json`。".to_string(),
        ],
        &[
            "`audit-risk auth status`".to_string(),
            "`audit-risk notify . --test --json`".to_string(),
        ],
        &[],
    ))
}

fn render_home_screen(cwd: &Path) -> String {
    let entitlement = load_entitlement_status();
    let workspace_line = if cwd.join(".hologram/delivery.json").exists() {
        "当前目录已接入 audit-risk，可以直接运行 audit-risk watch . 或 audit-risk check ."
    } else if cwd.join(".git").exists() {
        "当前目录像一个 Git 项目，但还没接入 audit-risk，建议先运行 audit-risk init ."
    } else {
        "当前目录还不像一个 workspace。没关系，先看 tour，再决定在哪个项目里接入。"
    };
    render_product_shell(
        &[
            format!("当前目录：{}", normalize_path(cwd.display().to_string())),
            format!("目录状态：{workspace_line}"),
            format!("版本状态：{}", pro_status_label(&entitlement)),
        ],
        &[read_last_review_summary(cwd)],
        &[
            "audit-risk 的首页不是摆设，它用来告诉你当前目录能不能直接开始接入、审查和留痕。".to_string(),
            "先把目录状态、最近结论和 Core/Pro 边界讲清楚，后面的命令才不会像散命令。".to_string(),
        ],
        &[
            "第一次接入就从 init 和 doctor 开始。".to_string(),
            "日常开发主要走 watch 和 check。".to_string(),
            "需要历史留痕或交付给别人看，再导出 report。".to_string(),
        ],
        &[
            "`audit-risk init .`".to_string(),
            "`audit-risk doctor .`".to_string(),
            "`audit-risk watch .`".to_string(),
            "`audit-risk check .`".to_string(),
            "`audit-risk help`".to_string(),
            "`audit-risk tour`".to_string(),
        ],
        &[
            "Core 免费保留：首页、help/tour、init、doctor、check、watch、diff、基础解释、基础报告。".to_string(),
            format!("Pro 个人版 {PRO_PERSONAL_PRICE_LABEL}：高级规则包、历史风险对比、增强报告、observe、notify、个人规则自定义加载。"),
            "开通或刷新授权：`audit-risk auth login`。".to_string(),
        ],
    )
}

fn read_last_review_summary(workspace: &Path) -> String {
    let report_path = workspace.join(DEFAULT_REPORT_OUTPUT_PATH);
    if !report_path.exists() {
        return "还没有找到这个目录的历史审查结果。".to_string();
    }
    let raw = match fs::read_to_string(&report_path) {
        Ok(raw) => raw,
        Err(_) => return "找到了历史报告，但这次读取失败。".to_string(),
    };
    let value = match serde_json::from_str::<Value>(&raw) {
        Ok(value) => value,
        Err(_) => return "找到了历史报告，但内容不是合法 JSON。".to_string(),
    };
    let generated_at = value.get("generated_at").and_then(Value::as_str).unwrap_or("未知时间");
    let decision = value
        .pointer("/current_review/gate_decision/decision")
        .or_else(|| value.pointer("/review/gate_decision/decision"))
        .and_then(Value::as_str)
        .unwrap_or("未知结果");
    format!("{generated_at}，最近结论：{decision}")
}

#[cfg(test)]
fn render_auth_status(status: &EntitlementStatus) -> String {
    render_auth_status_with_session(status, None)
}

fn render_auth_status_for_dir(dir: &Path) -> String {
    let status = load_or_refresh_entitlement_status(dir);
    let session = load_auth_session_from_dir(dir);
    render_auth_status_with_session(&status, session.as_ref())
}

fn render_auth_status_for_dir_with_workspace(dir: &Path, workspace: &Path) -> String {
    let base_url = auth_base_url_for_workspace(workspace);
    let status = load_or_refresh_entitlement_status_with_base_url(dir, base_url.as_deref());
    let session = load_auth_session_from_dir(dir);
    render_auth_status_with_session(&status, session.as_ref())
}

fn render_auth_status_with_session(
    status: &EntitlementStatus,
    session: Option<&AuthSessionStatus>,
) -> String {
    if status.is_pro_allowed() {
        render_product_shell(
            &[
                "当前视图：授权状态".to_string(),
                "登录状态：已登录".to_string(),
                "当前版本：Pro 个人版".to_string(),
            ],
            &[format!(
                "有效期至：{}；下次扣费：{}",
                status.valid_until.as_deref().unwrap_or("未知"),
                status.next_billing_at.as_deref().unwrap_or("以支付平台显示为准"),
            )],
            &["授权状态会决定 observe、notify 等 Pro 能力是否直接放行。".to_string()],
            &[
                "继续用 watch/check 走免费主路径时，不需要额外操作。".to_string(),
                "如果要停用 Pro，在支付渠道里解约后再回来刷新状态。".to_string(),
            ],
            &[
                "`audit-risk watch .`".to_string(),
                "`audit-risk auth logout`".to_string(),
            ],
            &[
                "宽限期：72 小时（授权过期后仍可使用）。".to_string(),
                format!("当前说明：{}", status.reason),
            ],
        )
    } else if matches!(status.state, EntitlementState::Missing)
        && session.is_some_and(AuthSessionStatus::is_pending_and_live)
    {
        let session = session.expect("pending session should exist");
        render_product_shell(
            &[
                "当前视图：授权状态".to_string(),
                "登录状态：登录进行中".to_string(),
                "当前版本：Core 免费版（等待浏览器完成登录）".to_string(),
            ],
            &[format!(
                "会话 ID：{}；浏览器地址：{}；会话有效期至：{}",
                session.session_id,
                session.login_url,
                session.expires_at.as_deref().unwrap_or("未知"),
            )],
            &["本机已经准备好登录会话，但还没有拿到 entitlement，所以 Pro 还不能放行。".to_string()],
            &[
                "先完成浏览器登录，再回来运行 `audit-risk auth status`。".to_string(),
                "如果暂时不走 Pro，先继续验收 Core 命令也可以。".to_string(),
            ],
            &[
                "`audit-risk auth status`".to_string(),
                "`audit-risk watch .`".to_string(),
            ],
            &["轮询说明：服务端接入后会按 2 秒一次、最长 5 分钟轮询。".to_string()],
        )
    } else if status.payment_pending {
        render_product_shell(
            &[
                "当前视图：授权状态".to_string(),
                "登录状态：支付确认中".to_string(),
                "当前版本：Core 免费版（等待支付结果同步）".to_string(),
            ],
            &["已拿到基础授权缓存，但支付结果还没有确认完成。".to_string()],
            &[format!("支付确认完成前，Pro 功能仍然不能放行；价格保持 {PRO_PERSONAL_PRICE_LABEL}。")],
            &[
                "先等支付平台回传结果，再运行 `audit-risk auth status` 刷新。".to_string(),
                "Core 主路径现在仍然可以继续用。".to_string(),
            ],
            &[
                "`audit-risk auth status`".to_string(),
                "`audit-risk watch .`".to_string(),
            ],
            &[],
        )
    } else {
        let login_status = match status.state {
            EntitlementState::Revoked => "授权已撤销",
            EntitlementState::DeviceMismatch => "设备绑定异常",
            EntitlementState::Expired => "授权已过期",
            EntitlementState::Invalid => "授权文件无效",
            _ => "未登录",
        };
        render_product_shell(
            &[
                "当前视图：授权状态".to_string(),
                format!("登录状态：{login_status}"),
                "当前版本：Core 免费版".to_string(),
            ],
            &[format!("当前原因：{}", status.reason)],
            &[format!("未进入 Pro 有效态时，observe、notify、watch --observe 都不会被放行；Pro 定价保持 {PRO_PERSONAL_PRICE_LABEL}。")],
            &[
                "先把登录、支付或设备绑定问题处理干净。".to_string(),
                "不走 Pro 时，继续使用 Core 免费主路径。".to_string(),
            ],
            &[
                "`audit-risk auth login`".to_string(),
                "`audit-risk auth logout`".to_string(),
                "`audit-risk help`".to_string(),
            ],
            &[],
        )
    }
}

fn auth_login_text_for_dir_with_base_url(
    dir: &Path,
    auth_base_url: Option<&str>,
) -> Result<String, CliRuntimeError> {
    fs::create_dir_all(dir).map_err(|error| {
        CliRuntimeError::environment(format!("无法创建授权目录 {}：{error}", dir.display()))
    })?;

    let device_secret_path = dir.join("device_secret");
    if !device_secret_path.exists() {
        let secret = uuid::Uuid::new_v4().to_string().replace('-', "");
        fs::write(&device_secret_path, format!("{secret}\n")).map_err(|error| {
            CliRuntimeError::environment(format!("无法写入 device_secret：{error}"))
        })?;
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let session_path = dir.join("session.json");
    let auth_session_urls = build_auth_session_urls(auth_base_url, &session_id);
    let session = AuthSessionDocument {
        session_id: session_id.clone(),
        status: "pending".to_string(),
        created_at: now_iso(),
        expires_at: (chrono::Utc::now() + chrono::Duration::seconds(300))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string(),
        poll_interval_seconds: 2,
        timeout_seconds: 300,
        poll_url: auth_session_urls.poll_url,
        exchange_url: auth_session_urls.exchange_url,
        login_url: auth_session_urls.login_url,
    };
    fs::write(
        &session_path,
        serde_json::to_string_pretty(&session)
            .map_err(|error| CliRuntimeError::internal(format!("无法序列化登录 session：{error}")))?,
    )
    .map_err(|error| CliRuntimeError::environment(format!("无法写入 session.json：{error}")))?;

    if let Some(base_url) = auth_base_url {
        if !base_url.trim().is_empty() {
            let exchange_result = complete_auth_exchange(dir, &session, base_url)?;
            return Ok(render_product_shell(
                &[
                    "当前视图：登录授权".to_string(),
                    "浏览器状态：服务端模式下跳过自动拉起，由外部登录页处理".to_string(),
                    "当前结果：已完成本地授权写入".to_string(),
                ],
                &[format!(
                    "session_id：{session_id}；会话文件：{}；设备标识：{}",
                    normalize_path(session_path.display().to_string()),
                    normalize_path(device_secret_path.display().to_string()),
                )],
                &["这一步只负责把 CLI 侧登录合同和本地状态机走通，不会伪造 Pro 成功态。".to_string()],
                &[
                    format!("登录地址：{}", session.login_url),
                    format!("轮询地址：{}", session.poll_url),
                    format!("entitlement：{}", normalize_path(exchange_result.entitlement_path.display().to_string())),
                    format!("signature：{}", normalize_path(exchange_result.signature_path.display().to_string())),
                ],
                &[
                    "`audit-risk auth status`".to_string(),
                    "`audit-risk watch .`".to_string(),
                ],
                &[format!("当前状态：{}", exchange_result.status_line)],
            ));
        }
    }

    let open_note = "未配置 auth 服务地址，当前不会自动打开浏览器。需要先写入 delivery.json.auth.base_url 或设置 AUDIT_RISK_AUTH_BASE_URL。".to_string();

    Ok(render_product_shell(
        &[
            "当前视图：登录授权".to_string(),
            format!("session_id：{session_id}"),
            format!("浏览器状态：{open_note}"),
        ],
        &[format!(
            "会话文件：{}；设备标识：{}",
            normalize_path(session_path.display().to_string()),
            normalize_path(device_secret_path.display().to_string()),
        )],
        &["当前仓库只实现 CLI 侧合同和本地授权状态机；服务端未接入前不会伪造 Pro 授权。".to_string()],
        &[
            format!("登录地址：{}", session.login_url),
            format!("轮询地址：{}", session.poll_url),
            "CLI 只生成 session.json / device_secret / 占位登录地址。".to_string(),
            "若要继续走真实登录链路，先配置 delivery.json.auth.base_url 或 AUDIT_RISK_AUTH_BASE_URL。".to_string(),
        ],
        &[
            "`audit-risk auth status`".to_string(),
            "`audit-risk auth login`".to_string(),
        ],
        &["你可以先验收本地 Core 命令，不需要打开占位地址。".to_string()],
    ))
}

struct AuthSessionUrls {
    poll_url: String,
    exchange_url: String,
    login_url: String,
}

fn build_auth_session_urls(auth_base_url: Option<&str>, session_id: &str) -> AuthSessionUrls {
    let base_url = auth_base_url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("https://auth.audit-risk.local")
        .trim_end_matches('/');
    AuthSessionUrls {
        poll_url: format!("{base_url}/api/auth/poll?session_id={session_id}"),
        exchange_url: format!("{base_url}/api/auth/exchange"),
        login_url: format!("{base_url}/login?session_id={session_id}"),
    }
}

struct AuthExchangeResult {
    entitlement_path: PathBuf,
    signature_path: PathBuf,
    status_line: String,
}

fn persist_entitlement_result(
    dir: &Path,
    entitlement: &Value,
    signature: &str,
) -> Result<(PathBuf, PathBuf), CliRuntimeError> {
    let entitlement_path = dir.join("entitlement.json");
    let signature_path = dir.join("entitlement.sig");
    fs::write(
        &entitlement_path,
        serde_json::to_string_pretty(entitlement)
            .map_err(|error| CliRuntimeError::internal(format!("无法序列化 entitlement：{error}")))?,
    )
    .map_err(|error| CliRuntimeError::environment(format!("无法写入 entitlement.json：{error}")))?;
    fs::write(&signature_path, signature)
        .map_err(|error| CliRuntimeError::environment(format!("无法写入 entitlement.sig：{error}")))?;
    Ok((entitlement_path, signature_path))
}

fn should_refresh_entitlement(dir: &Path, status: &EntitlementStatus) -> bool {
    if !matches!(status.state, EntitlementState::Active | EntitlementState::Grace) {
        return false;
    }
    let Some(last_refresh_raw) = extract_last_refresh_time(dir) else {
        return true;
    };
    let Some(last_refresh_time) = parse_rfc3339_utc(&last_refresh_raw) else {
        return true;
    };
    chrono::Utc::now() >= last_refresh_time + chrono::Duration::hours(ENTITLEMENT_REFRESH_INTERVAL_HOURS)
}

fn extract_last_refresh_time(dir: &Path) -> Option<String> {
    let raw = fs::read_to_string(dir.join("entitlement.json")).ok()?;
    let document = serde_json::from_str::<PersonalEntitlementDocument>(&raw).ok()?;
    Some(document.last_refresh_time)
}

fn refresh_entitlement_for_dir(
    dir: &Path,
    base_url: Option<&str>,
) -> Result<EntitlementStatus, CliRuntimeError> {
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CliRuntimeError::environment("未配置 entitlement refresh 服务地址。"))?;
    let current_raw = fs::read_to_string(dir.join("entitlement.json"))
        .map_err(|error| CliRuntimeError::environment(format!("无法读取 entitlement.json：{error}")))?;
    let current = serde_json::from_str::<PersonalEntitlementDocument>(&current_raw)
        .map_err(|error| CliRuntimeError::environment(format!("entitlement.json 不是合法 JSON：{error}")))?;
    let device_id = derive_device_id_for_dir(dir)?;
    let refresh_body = EntitlementRefreshRequest {
        user_id: current.user_id,
        device_id: device_id.clone(),
    };
    let refresh: AuthEntitlementEnvelope = auth_http_json_typed(
        "POST",
        &format!("{}/api/entitlement/refresh", base_url.trim_end_matches('/')),
        Some(&refresh_body),
    )?;
    let entitlement_value = serde_json::to_value(&refresh.entitlement)
        .map_err(|error| CliRuntimeError::internal(format!("无法编码 refreshed entitlement：{error}")))?;
    persist_entitlement_result(dir, &entitlement_value, &refresh.signature)?;
    Ok(load_entitlement_status_from_dir(dir))
}

fn complete_auth_exchange(
    dir: &Path,
    session: &AuthSessionDocument,
    base_url: &str,
) -> Result<AuthExchangeResult, CliRuntimeError> {
    let session_id = session.session_id.as_str();
    let poll_result: AuthPollResponse = auth_http_json_typed(
        "GET",
        &format!("{}/api/auth/poll?session_id={}", base_url.trim_end_matches('/'), session_id),
        None::<&serde_json::Value>,
    )?;
    let auth_token = poll_result
        .auth_token
        .as_deref()
        .ok_or_else(|| CliRuntimeError::environment("auth poll 返回里缺少 auth_token"))?;
    let device_id = derive_device_id_for_dir(dir)?;
    let exchange_body = AuthExchangeRequest {
        auth_token: auth_token.to_string(),
        device_id: device_id.clone(),
    };
    let mut exchange_result: AuthEntitlementEnvelope = auth_http_json_typed(
        "POST",
        &format!("{}/api/auth/exchange", base_url.trim_end_matches('/')),
        Some(&exchange_body),
    )?;
    // The server is the source of truth for user_id — it comes back on the
    // exchange response itself, never hardcode a placeholder here.
    let user_id = exchange_result.entitlement.user_id.clone();
    let base_entitlement = serde_json::to_value(&exchange_result.entitlement)
        .map_err(|error| CliRuntimeError::internal(format!("无法编码 auth exchange entitlement：{error}")))?;
    let base_signature = exchange_result.signature.clone();

    exchange_result = match maybe_resolve_payment_pending(base_url, exchange_result, &user_id, &device_id) {
        Ok(value) => value,
        Err(error) => {
            let mut pending_entitlement = base_entitlement.clone();
            if let Some(object) = pending_entitlement.as_object_mut() {
                object.insert("payment_pending".to_string(), Value::Bool(true));
            }
            let _ = persist_entitlement_result(dir, &pending_entitlement, &base_signature);
            return Err(error);
        }
    };
    let entitlement_value = serde_json::to_value(&exchange_result.entitlement)
        .map_err(|error| CliRuntimeError::internal(format!("无法编码最终 entitlement：{error}")))?;
    let (entitlement_path, signature_path) =
        persist_entitlement_result(dir, &entitlement_value, &exchange_result.signature)?;
    let session_path = dir.join("session.json");
    if session_path.exists() {
        let _ = fs::remove_file(session_path);
    }
    let status_line = render_auth_status_for_dir(dir)
        .lines()
        .find(|line| line.contains("登录状态"))
        .unwrap_or("登录状态：未知")
        .to_string();
    Ok(AuthExchangeResult {
        entitlement_path,
        signature_path,
        status_line,
    })
}

fn maybe_resolve_payment_pending(
    base_url: &str,
    exchange_result: AuthEntitlementEnvelope,
    user_id: &str,
    device_id: &str,
) -> Result<AuthEntitlementEnvelope, CliRuntimeError> {
    let current_plan = exchange_result.entitlement.plan.as_str();
    if current_plan == PRO_PERSONAL_PLAN {
        return Ok(exchange_result);
    }

    let base_url = base_url.trim_end_matches('/');
    let max_attempts = payment_query_attempts(base_url);
    for _attempt in 0..max_attempts {
        let query_result: AuthEntitlementEnvelope = auth_http_json_typed(
            "GET",
            &format!("{}/api/payment/query?user_id={}&device_id={}", base_url, user_id, device_id),
            None::<&serde_json::Value>,
        )?;
        let plan = query_result.entitlement.plan.as_str();
        if plan == PRO_PERSONAL_PLAN {
            return Ok(query_result);
        }
    }

    Err(CliRuntimeError::environment(
        "支付确认中，请稍后运行 audit-risk auth status 查看状态。如已扣款未到账，请联系客服。",
    ))
}

fn payment_query_attempts(_base_url: &str) -> usize {
    #[cfg(test)]
    if _base_url.starts_with("mock://payment-timeout") {
        return 1;
    }

    6
}

#[cfg(test)]
fn signed_mock_entitlement(mut entitlement: Value, device_id: &str) -> Value {
    entitlement["device_id"] = Value::String(device_id.to_string());
    let raw = serde_json::to_string(&entitlement).expect("mock entitlement must serialize");
    let signature = crate::entitlement::sign_for_test(&raw);
    json!({
        "entitlement": entitlement,
        "signature": signature,
    })
}

#[cfg(test)]
fn mock_request_device_id(body: Option<&Value>) -> Result<&str, CliRuntimeError> {
    body.and_then(|value| value.get("device_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| CliRuntimeError::internal("mock auth request missing device_id"))
}

#[cfg(test)]
fn mock_query_device_id(url: &str) -> Result<&str, CliRuntimeError> {
    url.split_once("device_id=")
        .map(|(_, value)| value.split('&').next().unwrap_or(value))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CliRuntimeError::internal("mock payment query missing device_id"))
}

fn auth_http_json(method: &str, url: &str, body: Option<&Value>) -> Result<Value, CliRuntimeError> {
    #[cfg(test)]
    {
    if url.starts_with("mock://network-unreachable/") {
        return Err(CliRuntimeError::environment(
            AuthServiceDiagnostic {
                code: "network_unreachable",
                message: "auth service is unreachable".to_string(),
            }
            .render_message(),
        ));
    }
    if url.starts_with("mock://bad-json/") {
        return Err(CliRuntimeError::environment(
            AuthServiceDiagnostic {
                code: "bad_json",
                message: "auth service returned malformed JSON".to_string(),
            }
            .render_message(),
        ));
    }
    if url.starts_with("mock://timeout/") {
        return Err(CliRuntimeError::environment(
            AuthServiceDiagnostic {
                code: "timeout",
                message: "auth service request timed out".to_string(),
            }
            .render_message(),
        ));
    }
    if url.starts_with("mock://approved/api/auth/poll") {
        return Ok(json!({
            "status": "approved",
            "auth_token": "auth-token-123",
        }));
    }
    if url.starts_with("mock://payment-pending/api/auth/poll")
        || url.starts_with("mock://payment-timeout/api/auth/poll")
    {
        return Ok(json!({
            "status": "approved",
            "auth_token": "auth-token-123",
        }));
    }
    if url == "mock://approved/api/auth/exchange" {
        return Ok(signed_mock_entitlement(
            json!({
                "user_id": "user-1",
                "plan": "pro_personal_monthly",
                "features": ["observe", "notify"],
                "issued_at": "2026-06-27T00:00:00Z",
                "valid_until": "2999-01-01T00:00:00Z",
                "last_refresh_time": "2026-06-27T00:00:00Z",
                "status": "active",
                "next_billing_at": "2999-01-31T00:00:00Z"
            }),
            mock_request_device_id(body)?,
        ));
    }
    if url == "mock://refresh-active/api/entitlement/refresh" {
        return Ok(signed_mock_entitlement(
            json!({
                "user_id": "user-1",
                "plan": "pro_personal_monthly",
                "features": ["observe", "notify"],
                "issued_at": "2026-06-27T00:00:00Z",
                "valid_until": "2999-01-01T00:00:00Z",
                "last_refresh_time": "2999-01-01T00:00:00Z",
                "status": "active",
                "next_billing_at": "2999-01-31T00:00:00Z"
            }),
            mock_request_device_id(body)?,
        ));
    }
    if url == "mock://refresh-revoked/api/entitlement/refresh" {
        return Ok(signed_mock_entitlement(
            json!({
                "user_id": "user-1",
                "plan": "pro_personal_monthly",
                "features": ["observe"],
                "issued_at": "2026-06-27T00:00:00Z",
                "valid_until": "2999-01-01T00:00:00Z",
                "last_refresh_time": "2999-01-01T00:00:00Z",
                "status": "revoked",
                "next_billing_at": "2999-01-31T00:00:00Z"
            }),
            mock_request_device_id(body)?,
        ));
    }
    if url == "mock://payment-pending/api/auth/exchange" || url == "mock://payment-timeout/api/auth/exchange" {
        return Ok(signed_mock_entitlement(
            json!({
                "user_id": "user-1",
                "plan": "core_free",
                "features": [],
                "issued_at": "2026-06-27T00:00:00Z",
                "valid_until": "2999-01-01T00:00:00Z",
                "last_refresh_time": "2026-06-27T00:00:00Z",
                "status": "active",
                "next_billing_at": "2999-01-31T00:00:00Z"
            }),
            mock_request_device_id(body)?,
        ));
    }
    if url.starts_with("mock://payment-pending/api/payment/query?") {
        return Ok(signed_mock_entitlement(
            json!({
                "user_id": "user-1",
                "plan": "pro_personal_monthly",
                "features": ["observe", "notify"],
                "issued_at": "2026-06-27T00:00:00Z",
                "valid_until": "2999-01-01T00:00:00Z",
                "last_refresh_time": "2026-06-27T00:00:00Z",
                "status": "active",
                "next_billing_at": "2999-01-31T00:00:00Z"
            }),
            mock_query_device_id(url)?,
        ));
    }
    if url.starts_with("mock://payment-timeout/api/payment/query?") {
        return Ok(signed_mock_entitlement(
            json!({
                "user_id": "user-1",
                "plan": "core_free",
                "features": [],
                "issued_at": "2026-06-27T00:00:00Z",
                "valid_until": "2999-01-01T00:00:00Z",
                "last_refresh_time": "2026-06-27T00:00:00Z",
                "status": "active",
                "next_billing_at": "2999-01-31T00:00:00Z"
            }),
            mock_query_device_id(url)?,
        ));
    }
    }

    let mut args = vec![
        "-sS".to_string(),
        "-X".to_string(),
        method.to_string(),
        "-H".to_string(),
        "Content-Type: application/json".to_string(),
        url.to_string(),
    ];
    if let Some(body) = body {
        args.push("-d".to_string());
        args.push(
            serde_json::to_string(body)
                .map_err(|error| CliRuntimeError::internal(format!("无法序列化 auth 请求：{error}")))?,
        );
    }
    let output = run_process("/usr/bin/curl", &args, None)?;
    if !output.status.success() {
        let stderr = trimmed_stderr(&output);
        return Err(CliRuntimeError::environment(
            AuthServiceDiagnostic {
                code: classify_auth_service_stderr(&stderr),
                message: stderr,
            }
            .render_message(),
        ));
    }
    serde_json::from_slice::<Value>(&output.stdout).map_err(|error| {
        CliRuntimeError::environment(
            AuthServiceDiagnostic {
                code: "bad_json",
                message: format!("auth service returned malformed JSON: {error}"),
            }
            .render_message(),
        )
    })
}

fn auth_http_json_typed<T: for<'de> Deserialize<'de>>(
    method: &str,
    url: &str,
    body: Option<&impl Serialize>,
) -> Result<T, CliRuntimeError> {
    let body_value = body
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| CliRuntimeError::internal(format!("无法序列化 auth 请求：{error}")))?;
    let value = auth_http_json(method, url, body_value.as_ref())?;
    serde_json::from_value::<T>(value)
        .map_err(|error| CliRuntimeError::environment(format!("auth 返回结构不符合合同：{error}")))
}

fn classify_auth_service_stderr(stderr: &str) -> &'static str {
    let lower = stderr.to_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("could not resolve host")
        || lower.contains("failed to connect")
        || lower.contains("connection refused")
        || lower.contains("no route to host")
    {
        "network_unreachable"
    } else {
        "auth_service_error"
    }
}

fn classify_auth_service_error(error: &CliRuntimeError) -> AuthServiceDiagnostic {
    let message = error.message.clone();
    let code = if let Some((code, _)) = message.split_once(':') {
        match code {
            "network_unreachable" => "network_unreachable",
            "timeout" => "timeout",
            "bad_json" => "bad_json",
            "auth_service_error" => "auth_service_error",
            _ => "auth_service_error",
        }
    } else {
        "auth_service_error"
    };
    AuthServiceDiagnostic { code, message }
}

fn tour_text() -> String {
    render_product_shell(
        &[
            "当前视图：1 分钟上手".to_string(),
            "建议场景：第一次把 audit-risk 接进真实项目".to_string(),
            "默认目标：一个终端里完成接入、守护、审查和报告".to_string(),
        ],
        &["你不需要先学一堆子系统，只要按主路径把接入、体检、守护和报告走通一次。".to_string()],
        &[
            "先把正确顺序讲清楚，用户才不会把 CLI 当成一堆互相不认识的散命令。".to_string(),
            "一旦顺序稳定，后面再加 Pro 能力也不会把主路径打散。".to_string(),
        ],
        &[
            "进入项目目录：`cd your-project`".to_string(),
            "初始化接入：`audit-risk init .`".to_string(),
            "检查环境：`audit-risk doctor .`".to_string(),
            "开发时守护：`audit-risk watch .`".to_string(),
            "提交前审查：`audit-risk check .`".to_string(),
            "需要报告：`audit-risk report .`".to_string(),
        ],
        &[
            "`audit-risk init .`".to_string(),
            "`audit-risk doctor .`".to_string(),
            "`audit-risk watch .`".to_string(),
            "`audit-risk check .`".to_string(),
            "`audit-risk report .`".to_string(),
        ],
        &[
            "Core 免费版负责把风险讲明白，并保留基础报告。".to_string(),
            "Pro 个人版负责手机观察、告警推送、历史风险对比、增强报告和高级规则。".to_string(),
        ],
    )
}

fn pro_status_label(status: &EntitlementStatus) -> String {
    match status.state {
        EntitlementState::Active if status.plan.as_deref() == Some(PRO_PERSONAL_PLAN) => "Pro 个人版：有效".to_string(),
        EntitlementState::Grace if status.plan.as_deref() == Some(PRO_PERSONAL_PLAN) => "Pro 个人版：72 小时宽限期".to_string(),
        EntitlementState::Expired => "Core 免费版：Pro 授权已过期".to_string(),
        EntitlementState::Revoked => "Core 免费版：Pro 授权已撤销".to_string(),
        EntitlementState::DeviceMismatch => "Core 免费版：设备绑定不匹配".to_string(),
        EntitlementState::Invalid => "Core 免费版：本地授权文件无效".to_string(),
        EntitlementState::Missing => "Core 免费版：未登录".to_string(),
        _ => "Core 免费版".to_string(),
    }
}

fn trimmed_stderr(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("command failed with exit code {:?}", output.status.code())
    } else {
        stderr
    }
}

fn parse_optional_fail_on(args: &[String]) -> Result<FailGate, UsageError> {
    take_option(args, "--fail-on")?
        .map(|value| parse_fail_gate(&value))
        .transpose()
        .map(|value| value.unwrap_or(FailGate::Block))
}

fn parse_fail_gate(raw: &str) -> Result<FailGate, UsageError> {
    match raw {
        "off" => Ok(FailGate::Off),
        "warn" => Ok(FailGate::Warn),
        "require_approval" => Ok(FailGate::RequireApproval),
        "block" => Ok(FailGate::Block),
        _ => Err(UsageError::new(
            "`--fail-on` must be one of: off, warn, require_approval, block.",
        )),
    }
}

fn fail_gate_to_str(value: FailGate) -> &'static str {
    match value {
        FailGate::Off => "off",
        FailGate::Warn => "warn",
        FailGate::RequireApproval => "require_approval",
        FailGate::Block => "block",
    }
}

fn required_positional(
    subcommand: &str,
    args: &[String],
    position: usize,
    label: &str,
) -> Result<String, UsageError> {
    positional_arguments(args)
        .get(position)
        .cloned()
        .ok_or_else(|| UsageError::new(format!("`{subcommand}` requires {label}.")))
}

fn optional_positional(args: &[String], position: usize) -> Option<String> {
    positional_arguments(args).get(position).cloned()
}

fn positional_arguments(args: &[String]) -> Vec<String> {
    let mut positional = Vec::new();
    let mut skip_next = false;
    for (index, value) in args.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if value.starts_with("--") {
            if option_requires_value(value) && args.get(index + 1).is_some_and(|next| !next.starts_with("--")) {
                skip_next = true;
            }
            continue;
        }
        positional.push(value.clone());
    }
    positional
}

fn take_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|value| value == flag)
}

fn take_option(args: &[String], option: &str) -> Result<Option<String>, UsageError> {
    let mut values = args.iter().enumerate().filter_map(|(index, value)| {
        if value == option {
            Some(index)
        } else {
            None
        }
    });
    let Some(index) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(UsageError::new(format!("`{option}` may only be provided once.")));
    }
    let Some(value) = args.get(index + 1) else {
        return Err(UsageError::new(format!("`{option}` requires a value.")));
    };
    if value.starts_with("--") {
        return Err(UsageError::new(format!("`{option}` requires a value.")));
    }
    Ok(Some(value.clone()))
}

fn reject_unknown_flags(args: &[String], allowed: &[&str]) -> Result<(), UsageError> {
    for value in args {
        if value.starts_with("--") && !allowed.iter().any(|allowed_value| allowed_value == value) {
            return Err(UsageError::new(format!("Unknown flag `{value}`.")));
        }
    }
    Ok(())
}

fn option_requires_value(option: &str) -> bool {
    matches!(
        option,
        "--fail-on" | "--config" | "--output" | "--query" | "--limit"
    )
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn current_unix_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn ansi_red(value: &str) -> String {
    format!("\u{1b}[31m{value}\u{1b}[0m")
}

fn ansi_yellow(value: &str) -> String {
    format!("\u{1b}[33m{value}\u{1b}[0m")
}

fn ansi_green(value: &str) -> String {
    format!("\u{1b}[32m{value}\u{1b}[0m")
}

fn ansi_dim(value: &str) -> String {
    format!("\u{1b}[2m{value}\u{1b}[0m")
}

/// Bold + bright yellow — used to make a single callout line stand out
/// within a panel (e.g. the personalized Pro-gate detail) without touching
/// background color, which would need explicit scope management inside
/// panel_line to avoid bleeding into the padding/border that follows.
fn ansi_bold_highlight(value: &str) -> String {
    format!("\u{1b}[1;38;5;220m{value}\u{1b}[0m")
}

fn normalize_path(path: impl AsRef<str>) -> String {
    path.as_ref().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::{
        build_default_init_files, build_structured_output_envelope, parse_cli_command, AuthAction, CliCommand,
        CommandTier, DefaultOutputMode, FailGate, CLI_SCHEMA_VERSION,
    };
    use serde_json::json;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    /// Write `json` to `dir/entitlement.json` and a real Ed25519 signature to
    /// `dir/entitlement.sig`.  All tests that need a valid entitlement on disk
    /// must go through this helper — never write a fake sig string directly.
    fn write_signed_entitlement(dir: &std::path::Path, json: &str) {
        let sig = crate::entitlement::sign_for_test(json);
        std::fs::write(dir.join("entitlement.json"), json).expect("entitlement.json");
        std::fs::write(dir.join("entitlement.sig"), sig).expect("entitlement.sig");
    }

    #[test]
    fn parses_macos_platform_uuid_from_ioreg_output() {
        let output = r#"    "IOPlatformUUID" = "A1B2C3D4-E5F6-47A8-9012-3456789ABCDE""#;
        assert_eq!(
            super::parse_ioreg_platform_uuid(output).as_deref(),
            Some("A1B2C3D4-E5F6-47A8-9012-3456789ABCDE")
        );
    }

    #[test]
    fn parses_windows_machine_guid_from_registry_output() {
        let output = "    MachineGuid    REG_SZ    a1b2c3d4-e5f6-47a8-9012-3456789abcde";
        assert_eq!(
            super::parse_windows_machine_guid(output).as_deref(),
            Some("a1b2c3d4-e5f6-47a8-9012-3456789abcde")
        );
    }

    #[test]
    fn current_platform_exposes_stable_machine_identity() {
        let identity = super::current_machine_identity().expect("stable machine identity");
        assert!(!identity.trim().is_empty());
        assert_ne!(identity, "unknown-host");
    }

    #[test]
    fn copied_entitlement_is_rejected_on_another_machine() {
        let root_path = std::env::temp_dir().join(format!(
            "audit-risk-copied-entitlement-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        std::fs::write(root_path.join("device_secret"), "copied-secret").expect("device secret");

        let source_device = super::derive_device_id("copied-secret", "macos", "source-machine");
        let target_device = super::derive_device_id("copied-secret", "macos", "target-machine");
        let source_entitlement = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-27T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"{source_device}","last_refresh_time":"2026-06-27T00:00:00Z","status":"active","next_billing_at":"2999-01-31T00:00:00Z"}}"#
        );
        write_signed_entitlement(&root_path, &source_entitlement);

        let copied = super::load_entitlement_status_from_dir_with_expected_device_id(
            &root_path,
            Ok(target_device.clone()),
        );
        assert!(matches!(copied.state, super::EntitlementState::DeviceMismatch));
        assert!(!copied.is_pro_allowed());

        let mut rebound: serde_json::Value =
            serde_json::from_str(&source_entitlement).expect("source entitlement");
        rebound["device_id"] = serde_json::Value::String(target_device);
        std::fs::write(root_path.join("entitlement.json"), rebound.to_string())
            .expect("rebound entitlement");
        let tampered = super::load_entitlement_status_from_dir_with_expected_device_id(
            &root_path,
            Ok(super::derive_device_id(
                "copied-secret",
                "macos",
                "target-machine",
            )),
        );
        assert!(matches!(tampered.state, super::EntitlementState::Invalid));
        assert!(!tampered.is_pro_allowed());

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn machine_identity_lookup_failure_never_allows_pro() {
        let root_path = std::env::temp_dir().join(format!(
            "audit-risk-device-identity-error-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        std::fs::write(root_path.join("device_secret"), "device-secret").expect("device secret");
        let device_id = super::derive_device_id("device-secret", "macos", "source-machine");
        let entitlement = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-27T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"{device_id}","last_refresh_time":"2026-06-27T00:00:00Z","status":"active","next_billing_at":"2999-01-31T00:00:00Z"}}"#
        );
        write_signed_entitlement(&root_path, &entitlement);

        let status = super::load_entitlement_status_from_dir_with_expected_device_id(
            &root_path,
            Err(super::CliRuntimeError::environment(
                "machine identity unavailable",
            )),
        );
        assert!(matches!(status.state, super::EntitlementState::DeviceMismatch));
        assert!(!status.is_pro_allowed());
        assert!(status.reason.contains("machine identity unavailable"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn parses_primary_commands_with_expected_tier_and_defaults() {
        let check = parse_cli_command(&args(&["check", "/tmp/repo"])).expect("check should parse");
        assert_eq!(check.tier, CommandTier::Primary);
        assert_eq!(check.default_output, DefaultOutputMode::Human);
        assert!(matches!(check.command, CliCommand::Check { .. }));

        let watch = parse_cli_command(&args(&["watch", "/tmp/repo"])).expect("watch should parse");
        assert_eq!(watch.tier, CommandTier::Primary);
        assert_eq!(watch.default_output, DefaultOutputMode::Human);
        assert!(matches!(watch.command, CliCommand::Watch { .. }));
        if let CliCommand::Watch { observe, .. } = watch.command {
            assert!(!observe);
        }

        let diff = parse_cli_command(&args(&["diff", "/tmp/before.ts", "/tmp/after.ts"])).expect("diff should parse");
        assert_eq!(diff.tier, CommandTier::Primary);
        assert_eq!(diff.default_output, DefaultOutputMode::Human);
        assert!(matches!(diff.command, CliCommand::Diff { .. }));

        let init = parse_cli_command(&args(&["init", "/tmp/repo"])).expect("init should parse");
        assert_eq!(init.tier, CommandTier::Primary);
        assert_eq!(init.default_output, DefaultOutputMode::Human);
        assert!(matches!(init.command, CliCommand::Init { .. }));

        let doctor = parse_cli_command(&args(&["doctor", "/tmp/repo"])).expect("doctor should parse");
        assert_eq!(doctor.tier, CommandTier::Primary);
        assert_eq!(doctor.default_output, DefaultOutputMode::Human);
        assert!(matches!(doctor.command, CliCommand::Doctor { .. }));
    }

    #[test]
    fn parses_secondary_commands_with_expected_tier_and_defaults() {
        let report = parse_cli_command(&args(&["report", "/tmp/repo"])).expect("report should parse");
        assert_eq!(report.tier, CommandTier::Secondary);
        assert_eq!(report.default_output, DefaultOutputMode::Human);
        assert!(matches!(report.command, CliCommand::Report { .. }));

        let report_compare = parse_cli_command(&args(&["report", "/tmp/repo", "--history-compare"]))
            .expect("report history compare should parse");
        assert_eq!(report_compare.default_output, DefaultOutputMode::Human);
        assert!(matches!(report_compare.command, CliCommand::Report { .. }));

        let rules = parse_cli_command(&args(&["rules", "/tmp/repo"])).expect("rules should parse");
        assert_eq!(rules.tier, CommandTier::Secondary);
        assert_eq!(rules.default_output, DefaultOutputMode::Json);
        assert!(matches!(rules.command, CliCommand::Rules { .. }));

        let audit = parse_cli_command(&args(&["audit", "/tmp/repo"])).expect("audit should parse");
        assert_eq!(audit.tier, CommandTier::Secondary);
        assert_eq!(audit.default_output, DefaultOutputMode::Json);
        assert!(matches!(audit.command, CliCommand::Audit { .. }));

        let verify = parse_cli_command(&args(&["verify", "/tmp/repo"])).expect("verify should parse");
        assert_eq!(verify.tier, CommandTier::Secondary);
        assert_eq!(verify.default_output, DefaultOutputMode::Json);
        assert!(matches!(verify.command, CliCommand::Verify { .. }));

        let notify = parse_cli_command(&args(&["notify", "/tmp/repo", "--test", "--webhook-url", "https://example.com/hook"]))
            .expect("notify should parse");
        assert_eq!(notify.tier, CommandTier::Secondary);
        assert_eq!(notify.default_output, DefaultOutputMode::Human);
        assert!(matches!(notify.command, CliCommand::Notify { .. }));
    }

    #[test]
    fn parses_fail_on_for_primary_commands() {
        let parsed = parse_cli_command(&args(&["check", "/tmp/repo", "--fail-on", "warn"]))
            .expect("check should parse with fail-on");
        match parsed.command {
            CliCommand::Check { fail_on, .. } => assert_eq!(fail_on, FailGate::Warn),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn json_flags_preserve_machine_contract_for_interactive_commands() {
        let check = parse_cli_command(&args(&["check", "/tmp/repo", "--json"]))
            .expect("check should parse with json flag");
        assert_eq!(check.default_output, DefaultOutputMode::Json);

        let init = parse_cli_command(&args(&["init", "/tmp/repo", "--json"]))
            .expect("init should parse with json flag");
        assert_eq!(init.default_output, DefaultOutputMode::Json);

        let doctor = parse_cli_command(&args(&["doctor", "/tmp/repo", "--json"]))
            .expect("doctor should parse with json flag");
        assert_eq!(doctor.default_output, DefaultOutputMode::Json);

        let report = parse_cli_command(&args(&["report", "/tmp/repo", "--json"]))
            .expect("report should parse with json flag");
        assert_eq!(report.default_output, DefaultOutputMode::Json);

        let notify = parse_cli_command(&args(&["notify", "/tmp/repo", "--test", "--json"]))
            .expect("notify should parse with json flag");
        assert_eq!(notify.default_output, DefaultOutputMode::Json);
    }

    #[test]
    fn rejects_invalid_or_missing_arguments_as_usage_errors() {
        assert!(parse_cli_command(&args(&["watch"])).is_err(), "watch without workspace should fail");
        assert!(parse_cli_command(&args(&["diff", "/tmp/before.ts"])).is_err(), "diff without after should fail");
        assert!(parse_cli_command(&args(&["doctor", "--bogus"])).is_err(), "unknown flags should fail");
        assert!(parse_cli_command(&args(&["notify", "/tmp/repo"])).is_err(), "notify without --test should fail");
        assert!(parse_cli_command(&args(&["unknown"])).is_err(), "unknown subcommand should fail");
    }

    #[test]
    fn zero_args_and_help_commands_are_chinese_human_surfaces() {
        let home = parse_cli_command(&args(&[])).expect("empty command should open home");
        assert_eq!(home.tier, CommandTier::Primary);
        assert_eq!(home.default_output, DefaultOutputMode::Human);
        assert!(matches!(home.command, CliCommand::Home));

        let help = parse_cli_command(&args(&["help"])).expect("help should parse");
        assert_eq!(help.default_output, DefaultOutputMode::Human);
        assert!(matches!(help.command, CliCommand::Help));

        let tour = parse_cli_command(&args(&["tour"])).expect("tour should parse");
        assert_eq!(tour.default_output, DefaultOutputMode::Human);
        assert!(matches!(tour.command, CliCommand::Tour));

        for form in ["version", "--version", "-V"] {
            let v = parse_cli_command(&args(&[form])).expect("version should parse");
            assert!(matches!(v.command, CliCommand::Version), "form {form} should parse to Version");
        }
        // --version prints "audit-risk <semver>" and exits 0.
        let out = super::execute_command(parse_cli_command(&args(&["--version"])).unwrap())
            .expect("version runs");
        assert_eq!(out.exit_code, 0);
        assert!(out
            .stdout_text
            .as_deref()
            .unwrap_or_default()
            .starts_with("audit-risk "));
    }

    #[test]
    fn commercial_shell_surfaces_share_unified_layout_sections() {
        let cwd = std::path::Path::new("/tmp/workspace");
        let home = super::render_home_screen(cwd);
        assert!(home.contains("当前概览"));
        assert!(home.contains("问题说明"));
        assert!(home.contains("下一步"));
        assert!(home.contains("\u{1b}[48;5;"));
        assert!(home.contains("╭"));

        let tour = super::tour_text();
        assert!(tour.contains("当前概览"));
        assert!(tour.contains("问题说明"));
        assert!(tour.contains("下一步"));
        assert!(tour.contains("\u{1b}[48;5;"));
        assert!(tour.contains("╭"));

        let root_path = std::env::temp_dir().join(format!("audit-risk-shell-layout-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("workspace root");
        let status = super::load_entitlement_status_from_dir(&root_path);
        let gate = super::render_pro_gate_message("observe", &status, &super::ProGateContext::default());
        assert!(gate.contains("当前概览"));
        assert!(gate.contains("问题说明"));
        assert!(gate.contains("下一步"));
        assert!(gate.contains("\u{1b}[48;5;"));
        assert!(gate.contains("╭"));
        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn panel_line_aligns_right_border_for_cjk_heavy_content() {
        // Regression test: panel_line used to measure line length with
        // .chars().count(), which counts each CJK character as width 1 even
        // though terminals render them as width 2. That under-measurement
        // over-padded every line containing Chinese text (which is nearly
        // all of this UI's output), pushing rows past the intended box
        // width and causing the background-color escape to bleed once the
        // line wrapped in a real terminal — visually a solid colored bar
        // instead of readable text. Confirmed with a real screenshot in a
        // real terminal before and after this fix.
        let ascii_line = super::panel_line("hello", 40, "", "", "");
        let cjk_line = super::panel_line("当前视图：项目审查（示例仓库路径）", 40, "", "", "");

        // Every rendered line must have the SAME total visible width
        // (border char + space + inner content + pad + space + border char),
        // regardless of whether the content is ASCII or CJK.
        let ascii_visible_width = unicode_width::UnicodeWidthStr::width(super::strip_ansi(&ascii_line).as_str());
        let cjk_visible_width = unicode_width::UnicodeWidthStr::width(super::strip_ansi(&cjk_line).as_str());
        assert_eq!(
            ascii_visible_width, cjk_visible_width,
            "ASCII and CJK content must produce the same total row width so the right border stays aligned"
        );
        // Total width must be inner_width (40) + 4 (│ + space + space + │)
        assert_eq!(ascii_visible_width, 44);
        assert_eq!(cjk_visible_width, 44);
    }

    #[test]
    fn decide_render_mode_uses_plain_below_box_width_threshold() {
        // A terminal narrower than the box needs must fall back to Plain —
        // this is the actual fix for "narrow terminal still overflows even
        // with correct CJK width math": we stop trying to draw a fixed
        // 92-column box in a smaller terminal at all.
        assert_eq!(super::decide_render_mode(Some(40)), super::RenderMode::Plain);
        assert_eq!(super::decide_render_mode(Some(79)), super::RenderMode::Plain);
        assert_eq!(super::decide_render_mode(Some(95)), super::RenderMode::Plain);
    }

    #[test]
    fn decide_render_mode_uses_boxed_at_or_above_threshold() {
        assert_eq!(super::decide_render_mode(Some(96)), super::RenderMode::Boxed);
        assert_eq!(super::decide_render_mode(Some(120)), super::RenderMode::Boxed);
        assert_eq!(super::decide_render_mode(Some(500)), super::RenderMode::Boxed);
    }

    #[test]
    fn decide_render_mode_defaults_to_boxed_when_not_a_tty() {
        // Piped/redirected output (terminal_size returns None) keeps the
        // pre-existing fixed-width box behavior — there's no real viewport
        // to adapt to, and this matches every prior release's behavior for
        // `audit-risk check . > report.txt` or `| less`.
        assert_eq!(super::decide_render_mode(None), super::RenderMode::Boxed);
    }

    #[test]
    fn plain_mode_never_emits_box_drawing_characters() {
        // The whole point of Plain mode is that it cannot overflow because
        // it does no fixed-width padding math at all. Confirm it contains
        // none of the box-drawing glyphs the Boxed renderer uses.
        let rendered = super::render_panel_plain(
            "问题说明",
            &["严重 · migrations/0001_init.sql:0 · Migration file changed — may irreversibly alter data schema. Requires manual review, and this sentence is deliberately much longer than any fixed box width so it would have overflowed a narrow box.".to_string()],
            "",
            "",
            "",
            "",
        );
        for glyph in ["╭", "╮", "╰", "╯", "│", "├", "┤"] {
            assert!(!rendered.contains(glyph), "plain mode must not draw box borders, found {glyph}");
        }
        assert!(rendered.contains("问题说明"));
        assert!(rendered.contains("Migration file changed"));
    }

    #[test]
    fn plain_mode_preserves_empty_state_and_bullet_formatting() {
        let empty = super::render_panel_plain("下一步", &[], "", "", "", "");
        assert!(empty.contains("暂无"));

        let with_bullets = super::render_panel_plain(
            "下一步",
            &["- `audit-risk check .`".to_string()],
            "",
            "",
            "",
            "",
        );
        assert!(with_bullets.contains("•"), "bullet dash should still be decorated in plain mode");
    }

    #[test]
    fn check_screen_uses_product_shell_and_chinese_gate_labels() {
        let payload = json!({
            "generated_at": "2026-06-27T12:00:00Z",
            "workspace_root": "/tmp/customer-repo",
            "review": {
                "gate_decision": {
                    "decision": "require_approval",
                    "reason": "发现高风险配置变更。",
                    "finding_count": 2
                },
                "findings": [
                    {
                        "severity": "high",
                        "rule_id": "check.l4",
                        "plain_explanation": "生产配置被直接改写",
                        "location": {
                            "file_path": "config/prod.yaml",
                            "start_line": 8,
                            "end_line": 8
                        }
                    }
                ]
            }
        });

        let rendered = super::render_check_screen(&payload, false).expect("check shell");
        assert!(rendered.contains("当前概览"));
        assert!(rendered.contains("问题说明"));
        assert!(rendered.contains("下一步"));
        assert!(rendered.contains("需要人工确认"));
        assert!(rendered.contains("config/prod.yaml:8"));
        assert!(rendered.contains("╭"));
        assert!(rendered.contains("\u{1b}[48;5;"));
    }

    #[test]
    fn format_finding_line_colors_by_severity() {
        let critical = json!({"severity": "critical", "plain_explanation": "x", "location": {"file_path": "a.py", "start_line": 1}});
        let high = json!({"severity": "high", "plain_explanation": "x", "location": {"file_path": "a.py", "start_line": 1}});
        let medium = json!({"severity": "medium", "plain_explanation": "x", "location": {"file_path": "a.py", "start_line": 1}});
        let low = json!({"severity": "low", "plain_explanation": "x", "location": {"file_path": "a.py", "start_line": 1}});

        assert!(super::format_finding_line(&critical).contains("\u{1b}[31m"), "critical must be red");
        assert!(super::format_finding_line(&high).contains("\u{1b}[31m"), "high must be red");
        assert!(super::format_finding_line(&medium).contains("\u{1b}[33m"), "medium must be yellow");
        assert!(super::format_finding_line(&low).contains("\u{1b}[2m"), "low must be dim");
    }

    #[test]
    fn check_screen_finding_preview_carries_severity_color_through_the_panel() {
        // Regression guard: render_panel wraps every content line uniformly
        // in the panel's default text color. Confirm an embedded severity
        // color on a finding line survives that wrapping instead of being
        // silently overwritten, and that the CJK-width padding math (which
        // strips ANSI before measuring) still lines up the right border.
        let payload = json!({
            "workspace_root": "/tmp/customer-repo",
            "review": {
                "gate_decision": {"decision": "block", "reason": "阻断示例", "finding_count": 1},
                "findings": [
                    {"severity": "critical", "plain_explanation": "严重问题示例", "location": {"file_path": "a.py", "start_line": 1}}
                ]
            }
        });
        let rendered = super::render_check_screen(&payload, false).expect("check shell");
        assert!(rendered.contains("\u{1b}[31m"), "embedded red must survive panel wrapping");
        assert!(rendered.contains("严重问题示例"));
    }

    #[test]
    fn check_screen_covers_warn_and_block_gate_labels() {
        for (decision, expected) in [("warn", "告警"), ("block", "阻断")] {
            let payload = json!({
                "workspace_root": "/tmp/customer-repo",
                "review": {
                    "gate_decision": {
                        "decision": decision,
                        "reason": "测试 gate 标签。",
                        "finding_count": 1
                    },
                    "findings": [
                        {
                            "severity": "medium",
                            "plain_explanation": "测试风险",
                            "location": {
                                "file_path": "src/a.ts",
                                "start_line": 3,
                                "end_line": 3
                            }
                        }
                    ]
                }
            });
            let rendered = super::render_check_screen(&payload, false).expect("check shell");
            assert!(rendered.contains(expected), "expected gate label {expected} for {decision}");
        }
    }

    #[test]
    fn parses_auth_and_observe_commands() {
        let status = parse_cli_command(&args(&["auth", "status"])).expect("auth status should parse");
        assert!(matches!(status.command, CliCommand::Auth { action: AuthAction::Status }));

        let login = parse_cli_command(&args(&["auth", "login"])).expect("auth login should parse");
        assert!(matches!(login.command, CliCommand::Auth { action: AuthAction::Login }));

        let logout = parse_cli_command(&args(&["auth", "logout"])).expect("auth logout should parse");
        assert!(matches!(logout.command, CliCommand::Auth { action: AuthAction::Logout }));

        let observe = parse_cli_command(&args(&["observe", "/tmp/repo"])).expect("observe should parse");
        assert!(matches!(observe.command, CliCommand::Observe { .. }));
    }

    #[test]
    fn pro_only_entrypoints_have_chinese_gate_messages_when_core_user_calls_them() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-pro-gate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("workspace root");

        let observe = super::run_observe_command(Some(root_path.to_str().expect("utf8")));
        assert!(observe.is_err(), "observe should be gated for Core users");
        let observe_error = observe.expect_err("observe error");
        assert_eq!(observe_error.exit_code, 3);
        assert!(observe_error.message.contains("Pro 个人版"));
        assert!(observe_error.message.contains("audit-risk auth login"));
        assert!(observe_error.message.contains("手机观察"));

        let notify = super::run_notify_command(
            Some(root_path.to_str().expect("utf8")),
            true,
            Some("https://example.com/hook"),
            DefaultOutputMode::Human,
        );
        assert!(notify.is_err(), "notify should be gated for Core users");
        let notify_error = notify.expect_err("notify error");
        assert_eq!(notify_error.exit_code, 3);
        assert!(notify_error.message.contains("Pro 个人版"));
        assert!(notify_error.message.contains("告警推送"));

        let gate = super::render_pro_gate_message(
            "history_compare",
            &super::load_entitlement_status_from_dir(&root_path),
            &super::ProGateContext::default(),
        );
        assert!(gate.contains("历史风险对比"));
        assert!(gate.contains("audit-risk auth login"));

        let watch = super::run_watch_command(
            root_path.to_str().expect("utf8"),
            false,
            false,
            true,
            FailGate::Block,
        );
        assert!(watch.is_err(), "watch --observe should be gated for Core users");
        let watch_error = watch.expect_err("watch observe error");
        assert_eq!(watch_error.exit_code, 3);
        assert!(watch_error.message.contains("Pro 个人版"));
        assert!(watch_error.message.contains("手机观察"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn pro_gate_context_defaults_when_no_cached_report_exists() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-gate-context-empty-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("workspace root");

        let context = super::build_pro_gate_context("observe", &root_path);
        assert_eq!(context.critical_count, 0);
        assert_eq!(context.high_count, 0);
        assert_eq!(context.audit_record_count, None);

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn pro_gate_context_reads_findings_from_cached_report_without_touching_the_engine() {
        // Regression test: build_pro_gate_context used to call
        // build_workspace_check_payload(), which calls engine_init/
        // engine_analyze against the single process-global `static ENGINE`
        // in engine.rs. That raced any other test running concurrently on
        // the same global singleton — confirmed by 10 unrelated mcp::tests
        // failures appearing only under `cargo test` (parallel), never
        // under `cargo test <name>` (isolated). Fixed by reading the cached
        // report file instead, which is pure I/O with no shared state.
        let root_path = std::env::temp_dir().join(format!("audit-risk-gate-context-report-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root_path.join(".hologram")).expect("hologram dir");
        let report = serde_json::json!({
            "review": {
                "findings": [
                    {"severity": "critical"},
                    {"severity": "critical"},
                    {"severity": "high"},
                    {"severity": "low"},
                ]
            }
        });
        std::fs::write(
            root_path.join(".hologram/latest-risk-report.json"),
            serde_json::to_string(&report).unwrap(),
        )
        .expect("write cached report");

        let context = super::build_pro_gate_context("observe", &root_path);
        assert_eq!(context.critical_count, 2);
        assert_eq!(context.high_count, 1);

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn pro_gate_context_reads_current_review_shaped_report_too() {
        // The TS-generated `audit-risk report` output nests findings under
        // `current_review` rather than `review` — both shapes must work.
        let root_path = std::env::temp_dir().join(format!("audit-risk-gate-context-current-review-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root_path.join(".hologram")).expect("hologram dir");
        let report = serde_json::json!({
            "current_review": {
                "findings": [{"severity": "critical"}]
            }
        });
        std::fs::write(
            root_path.join(".hologram/latest-risk-report.json"),
            serde_json::to_string(&report).unwrap(),
        )
        .expect("write cached report");

        let context = super::build_pro_gate_context("notify", &root_path);
        assert_eq!(context.critical_count, 1);

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn pro_gate_context_counts_audit_log_lines_for_history_compare() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-gate-context-audit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root_path.join(".hologram")).expect("hologram dir");
        std::fs::write(
            root_path.join(".hologram/audit.jsonl"),
            "{\"a\":1}\n{\"a\":2}\n\n{\"a\":3}\n",
        )
        .expect("write audit log");

        let context = super::build_pro_gate_context("history_compare", &root_path);
        assert_eq!(context.audit_record_count, Some(3), "blank lines must not be counted");

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn pro_gate_message_personalizes_status_line_with_real_finding_counts() {
        let status = super::load_entitlement_status_from_dir(&std::env::temp_dir());
        let context = super::ProGateContext {
            critical_count: 2,
            high_count: 1,
            audit_record_count: None,
        };
        let message = super::render_pro_gate_message("observe", &status, &context);
        assert!(message.contains("2 条严重风险"));
        assert!(message.contains("1 条高危风险"));
        assert!(message.contains("3 条中高危以上风险"));
    }

    #[test]
    fn pro_gate_message_personalizes_history_compare_with_audit_record_count() {
        let status = super::load_entitlement_status_from_dir(&std::env::temp_dir());
        let context = super::ProGateContext {
            audit_record_count: Some(12),
            ..Default::default()
        };
        let message = super::render_pro_gate_message("history_compare", &status, &context);
        assert!(message.contains("已有 12 条历史审计记录"));
    }

    #[test]
    fn pro_gate_message_falls_back_to_generic_text_when_context_is_empty() {
        let status = super::load_entitlement_status_from_dir(&std::env::temp_dir());
        let message = super::render_pro_gate_message("observe", &status, &super::ProGateContext::default());
        // Must not fabricate risk numbers that don't exist.
        assert!(!message.contains("条严重风险"));
        assert!(message.contains("把最近一次审查结果开成只读看板"));
    }

    #[test]
    fn pro_gate_message_highlights_personalized_detail_but_not_generic_fallback() {
        let status = super::load_entitlement_status_from_dir(&std::env::temp_dir());

        let personalized = super::render_pro_gate_message(
            "observe",
            &status,
            &super::ProGateContext { critical_count: 1, high_count: 0, audit_record_count: None },
        );
        assert!(
            personalized.contains("\u{1b}[1;38;5;220m"),
            "a real, data-backed callout should be visually highlighted"
        );

        let generic = super::render_pro_gate_message("observe", &status, &super::ProGateContext::default());
        assert!(
            !generic.contains("\u{1b}[1;38;5;220m"),
            "the plain fallback text is not a callout and must not be highlighted"
        );
    }

    #[test]
    fn auth_login_creates_device_secret_and_session_contract_without_faking_pro() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-auth-login-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        let text = super::auth_login_text_for_dir_with_base_url(&root_path, None)
            .expect("auth login should return local contract");
        assert!(text.contains("session_id"));
        assert!(text.contains("session.json"));
        assert!(text.contains("poll_url") || text.contains("轮询地址"));
        assert!(text.contains("不会伪造 Pro 授权"));

        let device_secret = std::fs::read_to_string(root_path.join("device_secret")).expect("device_secret");
        assert!(!device_secret.trim().is_empty(), "device_secret should not be empty");
        let session_raw = std::fs::read_to_string(root_path.join("session.json")).expect("session file");
        let session: serde_json::Value = serde_json::from_str(&session_raw).expect("session json");
        assert_eq!(session["status"], "pending");
        assert!(session["session_id"].as_str().is_some());
        assert!(session["poll_url"].as_str().is_some());

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_login_without_auth_service_does_not_attempt_to_open_placeholder_browser_page() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-auth-login-no-browser-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        let text = super::auth_login_text_for_dir_with_base_url(&root_path, None)
            .expect("auth login should return local contract");

        assert!(text.contains("当前不会自动打开浏览器"));
        assert!(!text.contains("CLI 生成 session_id，并打开浏览器登录页"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_login_session_urls_follow_configured_auth_base_url() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-auth-login-configured-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");

        let error = super::auth_login_text_for_dir_with_base_url(&root_path, Some("mock://network-unreachable"))
            .expect_err("configured auth service should fail on mock network error");
        assert!(error.message.contains("network_unreachable"));

        let session_raw = std::fs::read_to_string(root_path.join("session.json")).expect("session file");
        let session: serde_json::Value = serde_json::from_str(&session_raw).expect("session json");
        let session_id = session["session_id"].as_str().expect("session_id");
        assert_eq!(
            session["poll_url"].as_str().expect("poll_url"),
            format!("mock://network-unreachable/api/auth/poll?session_id={session_id}"),
        );
        assert_eq!(
            session["exchange_url"].as_str(),
            Some("mock://network-unreachable/api/auth/exchange"),
        );
        assert_eq!(
            session["login_url"].as_str().expect("login_url"),
            format!("mock://network-unreachable/login?session_id={session_id}"),
        );

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_status_surfaces_grace_expired_revoked_and_device_mismatch() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-auth-status-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        std::fs::write(root_path.join("device_secret"), "device-secret").expect("device secret");
        let device_id = super::derive_device_id_for_dir(&root_path).expect("device id");

        let json = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-27T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"{}","last_refresh_time":"2026-06-27T00:00:00Z","status":"active","next_billing_at":"2999-01-31T00:00:00Z"}}"#,
            device_id
        );
        write_signed_entitlement(&root_path, &json);
        let active = super::render_auth_status(&super::load_entitlement_status_from_dir(&root_path));
        assert!(active.contains("已登录"));

        // 过期 24 小时：稳定处于 72 小时宽限期内（相对当前时间，避免硬编码日期过期导致测试失效）
        let grace_valid_until =
            (chrono::Utc::now() - chrono::Duration::hours(24)).format("%Y-%m-%dT%H:%M:%SZ");
        let json = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-27T00:00:00Z","valid_until":"{}","device_id":"{}","last_refresh_time":"2026-06-27T00:00:00Z","status":"active","next_billing_at":"2999-01-31T00:00:00Z"}}"#,
            grace_valid_until, device_id
        );
        write_signed_entitlement(&root_path, &json);
        let grace = super::render_auth_status(&super::load_entitlement_status_from_dir(&root_path));
        assert!(grace.contains("已登录"));
        assert!(grace.contains("72 小时"));

        let json = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-27T00:00:00Z","valid_until":"2026-06-01T00:00:00Z","device_id":"{}","last_refresh_time":"2026-06-27T00:00:00Z","status":"active","next_billing_at":"2999-01-31T00:00:00Z"}}"#,
            device_id
        );
        write_signed_entitlement(&root_path, &json);
        let expired = super::render_auth_status(&super::load_entitlement_status_from_dir(&root_path));
        assert!(expired.contains("Core 免费版"));
        assert!(expired.contains("已过期"));

        let json = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-27T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"{}","last_refresh_time":"2026-06-27T00:00:00Z","status":"revoked","next_billing_at":"2999-01-31T00:00:00Z"}}"#,
            device_id
        );
        write_signed_entitlement(&root_path, &json);
        let revoked = super::render_auth_status(&super::load_entitlement_status_from_dir(&root_path));
        assert!(revoked.contains("已撤销"));

        std::fs::remove_file(root_path.join("device_secret")).expect("remove device secret");
        let mismatch = super::render_auth_status(&super::load_entitlement_status_from_dir(&root_path));
        assert!(mismatch.contains("device_secret"));
        assert!(mismatch.contains("auth login"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_status_shows_pending_login_when_session_exists_but_entitlement_is_missing() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-auth-pending-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        let _ = super::auth_login_text_for_dir_with_base_url(&root_path, None).expect("login contract");

        let status = super::render_auth_status_for_dir(&root_path);
        assert!(status.contains("登录进行中"));
        assert!(status.contains("session"));
        assert!(status.contains("5 分钟"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_status_shows_payment_pending_when_cached_entitlement_is_waiting_for_payment_confirmation() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-payment-pending-status-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        std::fs::write(root_path.join("device_secret"), "device-secret").expect("device secret");
        let device_id = super::derive_device_id_for_dir(&root_path).expect("device id");
        let json = format!(
            r#"{{"user_id":"user-1","plan":"core_free","features":[],"issued_at":"2026-06-27T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"{}","last_refresh_time":"2026-06-27T00:00:00Z","status":"active","payment_pending":true,"next_billing_at":"2999-01-31T00:00:00Z"}}"#,
            device_id
        );
        write_signed_entitlement(&root_path, &json);

        let rendered = super::render_auth_status_for_dir(&root_path);
        assert!(rendered.contains("支付确认中"));
        assert!(rendered.contains("Core 免费版"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn entitlement_status_rejects_unknown_remote_status_even_when_valid_until_is_future() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-unknown-entitlement-status-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        std::fs::write(root_path.join("device_secret"), "device-secret").expect("device secret");
        let device_id = super::derive_device_id_for_dir(&root_path).expect("device id");
        let json = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-27T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"{}","last_refresh_time":"2026-06-27T00:00:00Z","status":"suspended","next_billing_at":"2999-01-31T00:00:00Z"}}"#,
            device_id
        );
        write_signed_entitlement(&root_path, &json);

        let status = super::load_entitlement_status_from_dir(&root_path);
        assert!(matches!(status.state, super::EntitlementState::Invalid));
        assert!(!status.is_pro_allowed());
        let rendered = super::render_auth_status_for_dir(&root_path);
        assert!(rendered.contains("授权文件无效"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_status_does_not_show_pending_login_after_session_expires() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-expired-auth-session-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        std::fs::write(
            root_path.join("session.json"),
            r#"{"session_id":"session-1","status":"pending","created_at":"2026-06-27T00:00:00Z","expires_at":"2026-06-27T00:00:01Z","poll_interval_seconds":2,"timeout_seconds":300,"poll_url":"https://auth.audit-risk.local/api/auth/poll?session_id=session-1","exchange_url":"https://auth.audit-risk.local/api/auth/exchange","login_url":"https://auth.audit-risk.local/login?session_id=session-1"}"#,
        )
        .expect("session");

        let rendered = super::render_auth_status_for_dir(&root_path);
        assert!(!rendered.contains("登录进行中"));
        assert!(rendered.contains("未登录"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_login_exchanges_entitlement_when_auth_server_is_configured() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-auth-success-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        let text = super::auth_login_text_for_dir_with_base_url(&root_path, Some("mock://approved"))
            .expect("auth login");
        assert!(text.contains("已完成本地授权写入"));

        let entitlement_raw =
            std::fs::read_to_string(root_path.join("entitlement.json")).expect("entitlement json");
        let entitlement: serde_json::Value = serde_json::from_str(&entitlement_raw).expect("entitlement value");
        assert_eq!(entitlement["plan"], "pro_personal_monthly");
        assert_eq!(entitlement["status"], "active");
        // Verify the sig on disk is a real Ed25519 signature that passes verification.
        let sig_on_disk = std::fs::read_to_string(root_path.join("entitlement.sig")).expect("sig");
        let json_on_disk = std::fs::read_to_string(root_path.join("entitlement.json")).expect("json");
        assert_eq!(
            crate::entitlement::verify_entitlement_signature(&json_on_disk, sig_on_disk.trim()),
            crate::entitlement::SignatureVerifyResult::Valid,
            "entitlement.sig on disk must be a valid Ed25519 signature"
        );
        assert!(!root_path.join("session.json").exists(), "session should be cleared after exchange");

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn entitlement_status_detects_device_mismatch_when_device_id_does_not_match() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-device-mismatch-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        std::fs::write(root_path.join("device_secret"), "device-secret").expect("device secret");
        write_signed_entitlement(&root_path, r#"{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-27T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"mismatched-device-id","last_refresh_time":"2026-06-27T00:00:00Z","status":"active","next_billing_at":"2999-01-31T00:00:00Z"}"#);

        let status = super::load_entitlement_status_from_dir(&root_path);
        assert!(matches!(status.state, super::EntitlementState::DeviceMismatch));
        assert!(status.reason.contains("当前设备"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn refresh_entitlement_updates_stale_active_entitlement() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-refresh-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        std::fs::write(root_path.join("device_secret"), "device-secret").expect("device secret");
        let device_id = super::derive_device_id_for_dir(&root_path).expect("device id");
        let json = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-20T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"{}","last_refresh_time":"2026-06-20T00:00:00Z","status":"active","next_billing_at":"2999-01-31T00:00:00Z"}}"#,
            device_id
        );
        write_signed_entitlement(&root_path, &json);

        let status = super::refresh_entitlement_for_dir(&root_path, Some("mock://refresh-active"))
            .expect("refresh should succeed");
        assert!(matches!(status.state, super::EntitlementState::Active));
        let raw = std::fs::read_to_string(root_path.join("entitlement.json")).expect("entitlement raw");
        assert!(raw.contains("\"notify\""));
        let sig_on_disk = std::fs::read_to_string(root_path.join("entitlement.sig")).expect("sig");
        assert_eq!(
            crate::entitlement::verify_entitlement_signature(&raw, sig_on_disk.trim()),
            crate::entitlement::SignatureVerifyResult::Valid,
            "refreshed entitlement.sig must be a valid Ed25519 signature"
        );

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn refresh_entitlement_surfaces_revoked_state() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-refresh-revoked-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");
        std::fs::write(root_path.join("device_secret"), "device-secret").expect("device secret");
        let device_id = super::derive_device_id_for_dir(&root_path).expect("device id");
        let json = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-20T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"{}","last_refresh_time":"2026-06-20T00:00:00Z","status":"active","next_billing_at":"2999-01-31T00:00:00Z"}}"#,
            device_id
        );
        write_signed_entitlement(&root_path, &json);

        let status = super::refresh_entitlement_for_dir(&root_path, Some("mock://refresh-revoked"))
            .expect("refresh should succeed");
        assert!(matches!(status.state, super::EntitlementState::Revoked));
        let rendered = super::render_auth_status_for_dir(&root_path);
        assert!(rendered.contains("已撤销"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_login_queries_payment_until_plan_becomes_pro() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-payment-query-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");

        let text = super::auth_login_text_for_dir_with_base_url(&root_path, Some("mock://payment-pending"))
            .expect("auth login");
        assert!(text.contains("已完成本地授权写入"));

        let entitlement_raw =
            std::fs::read_to_string(root_path.join("entitlement.json")).expect("entitlement json");
        let entitlement: serde_json::Value = serde_json::from_str(&entitlement_raw).expect("entitlement value");
        assert_eq!(entitlement["plan"], "pro_personal_monthly");
        assert_eq!(entitlement["status"], "active");

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_login_returns_waiting_message_when_payment_query_does_not_confirm_in_time() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-payment-timeout-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("entitlement root");

        let error = super::auth_login_text_for_dir_with_base_url(&root_path, Some("mock://payment-timeout"))
            .expect_err("auth login should return waiting error");
        assert!(error.message.contains("支付确认中"));
        assert!(error.message.contains("auth status"));
        assert!(root_path.join("entitlement.json").exists(), "entitlement cache should still exist");

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_http_json_classifies_network_unreachable() {
        let error = super::auth_http_json("GET", "mock://network-unreachable/api/auth/poll", None)
            .expect_err("network error expected");
        assert!(error.message.contains("network_unreachable"));
    }

    #[test]
    fn auth_http_json_classifies_bad_json() {
        let error = super::auth_http_json("GET", "mock://bad-json/api/auth/poll", None)
            .expect_err("bad json expected");
        assert!(error.message.contains("bad_json"));
    }

    #[test]
    fn doctor_surfaces_auth_service_error_code_when_configured_service_is_unreachable() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-doctor-auth-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root_path.join(".hologram")).expect("workspace");
        std::fs::write(
            root_path.join(".hologram/delivery.json"),
            r#"{"auth":{"base_url":"mock://network-unreachable"},"observe":{"bind":"0.0.0.0:8787","dashboard_title":"audit-risk observe","webhook_url":""}}"#,
        )
        .expect("delivery config");

        let outcome = super::run_doctor_command(
            Some(root_path.to_str().expect("utf8")),
            DefaultOutputMode::Json,
        )
        .expect("doctor outcome");
        let output = outcome.stdout_json.expect("doctor json");
        let checks = output["checks"].as_array().expect("checks");
        let auth = checks.iter().find(|item| item["name"] == "auth_service").expect("auth service check");
        assert_eq!(auth["status"], "error");
        assert_eq!(auth["detail"]["code"], "network_unreachable");

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn parse_git_status_changed_files_skips_parent_repo_paths_when_workspace_is_subdir() {
        let files = super::parse_git_status_changed_files(
            " M src/lib.rs\n M ../README.md\n?? ../dev-docs/acceptance.md\n",
        );
        assert_eq!(files, vec!["src/lib.rs".to_string()]);
    }

    #[test]
    fn builds_structured_output_envelope_with_required_fields() {
        let output = build_structured_output_envelope("check", "ok", Some("/tmp/repo"));
        assert_eq!(output["schema_version"], CLI_SCHEMA_VERSION);
        assert_eq!(output["command"], "check");
        assert_eq!(output["status"], "ok");
        assert_eq!(output["workspace_root"], "/tmp/repo");
        assert!(output.get("generated_at").is_some(), "generated_at should exist");
    }

    #[test]
    fn init_files_render_portable_pre_commit_hook() {
        let files = build_default_init_files(std::path::Path::new("/tmp/customer-repo"));
        let hook = files
            .iter()
            .find(|(path, _, _)| path == ".githooks/pre-commit")
            .map(|(_, content, _)| content.clone())
            .expect("expected pre-commit hook");

        // The hook must be portable: call `audit-risk` from PATH, never bake in
        // this build machine's path or require a Rust toolchain / `cargo run`.
        assert!(hook.contains("BIN=\"${AUDIT_RISK_BIN:-audit-risk}\""));
        assert!(hook.contains("check \"$ROOT\" --fail-on block"));
        assert!(!hook.contains("cargo run"));
        assert!(!hook.contains("/opt/audit-risk-platform"));
        assert!(!hook.contains("PLATFORM_ROOT"));
    }

    #[test]
    fn init_files_include_gitignore_for_volatile_state() {
        let files = build_default_init_files(std::path::Path::new("/tmp/customer-repo"));
        let gitignore = files
            .iter()
            .find(|(path, _, _)| path == ".hologram/.gitignore")
            .map(|(_, content, _)| content.clone())
            .expect("expected .hologram/.gitignore");
        assert!(gitignore.contains("hologram.db"));
        assert!(gitignore.contains("baseline.json"));
        assert!(gitignore.contains("latest-risk-report.json"));
    }

    #[test]
    fn init_files_include_observe_defaults_in_delivery_config() {
        let files = build_default_init_files(std::path::Path::new("/tmp/customer-repo"));
        let config = files
            .iter()
            .find(|(path, _, _)| path == ".hologram/delivery.json")
            .map(|(_, content, _)| content.clone())
            .expect("delivery config");
        let value: serde_json::Value = serde_json::from_str(&config).expect("json");
        assert_eq!(value["observe"]["bind"], "0.0.0.0:8787");
        assert_eq!(value["observe"]["dashboard_title"], "audit-risk observe");
        assert_eq!(value["auth"]["base_url"], "");
    }

    #[test]
    fn doctor_command_surfaces_rule_package_versions_and_dependency_checks() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-doctor-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root_path).expect("workspace root");
        let root = root_path.as_path();
        std::fs::create_dir_all(root.join(".hologram/rules")).expect("rules dir");
        std::fs::write(
            root.join(".hologram/delivery.json"),
            r#"{"version":"phase5.v1","workspace":{"root":"/tmp/workspace","changed_files_source":"git_status"},"provider":{"name":"deepseek","model":"deepseek-v4-pro","base_url":"https://api.deepseek.com","key_source":"env","env_var":"DEEPSEEK_API_KEY"},"rule_packages":{"review_paths":[".hologram/rules/review.workspace.json"],"repair_paths":[".hologram/rules/repair.workspace.json"],"disabled_review_rule_ids":[],"disabled_repair_rule_ids":[]},"audit":{"jsonl_path":".hologram/audit.jsonl","report_output_path":".hologram/latest-risk-report.json","recent_limit":20},"automation":{"verify_commands":["audit-risk check . --fail-on block"],"pre_commit_hook":".githooks/pre-commit","ci_workflow":".github/workflows/hologram-risk.yml","fail_on_decision":"block"}}"#,
        ).expect("delivery config");
        std::fs::write(
            root.join(".hologram/rules/review.workspace.json"),
            r#"{"package_id":"review.workspace","version":"v9","plane":"review","source":"workspace_extension","enabled":true,"description":"review override","rules":[]}"#,
        ).expect("review package");
        std::fs::write(
            root.join(".hologram/rules/repair.workspace.json"),
            r#"{"package_id":"repair.workspace","version":"v4","plane":"repair","source":"workspace_extension","enabled":true,"description":"repair override","rules":[]}"#,
        ).expect("repair package");

        let outcome = super::run_doctor_command(
            Some(root.to_str().expect("utf8")),
            DefaultOutputMode::Json,
        )
        .expect("doctor outcome");
        let output = outcome.stdout_json.expect("doctor json");
        let checks = output["checks"].as_array().expect("checks");

        let review = checks.iter().find(|item| item["name"] == "review_rule_package").expect("review check");
        assert_eq!(review["status"], "ok");
        assert_eq!(review["version"], "v9");

        let repair = checks.iter().find(|item| item["name"] == "repair_rule_package").expect("repair check");
        assert_eq!(repair["status"], "ok");
        assert_eq!(repair["version"], "v4");

        let engine_version = checks.iter().find(|item| item["name"] == "engine_version").expect("engine version");
        assert_eq!(engine_version["status"], "ok");
        assert_eq!(engine_version["detail"], env!("CARGO_PKG_VERSION"));

        let git_check = checks.iter().find(|item| item["name"] == "dependency_git").expect("git dependency");
        assert_eq!(git_check["status"], "ok");

        let cargo_check = checks.iter().find(|item| item["name"] == "dependency_cargo").expect("cargo dependency");
        assert_eq!(cargo_check["status"], "ok");

        let node_check = checks.iter().find(|item| item["name"] == "dependency_node").expect("node dependency");
        assert_eq!(node_check["status"], "ok");

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn auth_base_url_prefers_delivery_config_when_env_is_missing() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-auth-config-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root_path.join(".hologram")).expect("hologram dir");
        std::fs::write(
            root_path.join(".hologram/delivery.json"),
            r#"{"auth":{"base_url":"mock://approved"},"observe":{"bind":"0.0.0.0:8787","dashboard_title":"audit-risk observe","webhook_url":""}}"#,
        )
        .expect("delivery config");

        std::env::remove_var("AUDIT_RISK_AUTH_BASE_URL");
        let value = super::auth_base_url_for_workspace(&root_path);
        assert_eq!(value.as_deref(), Some("mock://approved"));

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn doctor_prefers_delivery_config_auth_service_base_url() {
        let root_path = std::env::temp_dir().join(format!("audit-risk-doctor-auth-config-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root_path.join(".hologram")).expect("hologram dir");
        std::fs::write(
            root_path.join(".hologram/delivery.json"),
            r#"{"auth":{"base_url":"mock://network-unreachable"},"observe":{"bind":"0.0.0.0:8787","dashboard_title":"audit-risk observe","webhook_url":""}}"#,
        )
        .expect("delivery config");

        std::env::remove_var("AUDIT_RISK_AUTH_BASE_URL");
        let outcome = super::run_doctor_command(
            Some(root_path.to_str().expect("utf8")),
            DefaultOutputMode::Json,
        )
        .expect("doctor outcome");
        let output = outcome.stdout_json.expect("doctor json");
        let checks = output["checks"].as_array().expect("checks");
        let auth = checks.iter().find(|item| item["name"] == "auth_service").expect("auth service check");
        assert_eq!(auth["detail"]["base_url"], "mock://network-unreachable");

        let _ = std::fs::remove_dir_all(&root_path);
    }

    #[test]
    fn default_workspace_root_resolves_dot_to_current_base_without_redirect() {
        let outer_root = std::env::temp_dir().join(format!("audit-risk-workspace-root-{}", uuid::Uuid::new_v4()));
        let repo_root = outer_root.join("repo");
        std::fs::create_dir_all(repo_root.join("engine")).expect("engine dir");
        std::fs::write(
            repo_root.join("engine/Cargo.toml"),
            "[package]\nname = \"placeholder\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        )
        .expect("cargo manifest");

        // `.` must resolve to the given base, not auto-redirect into a `repo/` child.
        let resolved = super::resolve_workspace_argument(&outer_root, ".");
        assert_eq!(
            std::fs::canonicalize(&resolved).expect("canonical resolved"),
            std::fs::canonicalize(&outer_root).expect("canonical expected"),
            "resolve_workspace_argument('.', base) must return base, not base/repo"
        );

        let _ = std::fs::remove_dir_all(&outer_root);
    }

    #[test]
    fn auth_status_refresh_prefers_workspace_delivery_config_base_url() {
        let entitlement_path = std::env::temp_dir().join(format!("audit-risk-auth-status-refresh-{}", uuid::Uuid::new_v4()));
        let workspace_path = std::env::temp_dir().join(format!("audit-risk-auth-status-workspace-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&entitlement_path).expect("entitlement root");
        std::fs::create_dir_all(workspace_path.join(".hologram")).expect("workspace hologram");
        std::fs::write(
            workspace_path.join(".hologram/delivery.json"),
            r#"{"auth":{"base_url":"mock://refresh-active"},"observe":{"bind":"0.0.0.0:8787","dashboard_title":"audit-risk observe","webhook_url":""}}"#,
        )
        .expect("delivery config");
        std::env::remove_var("AUDIT_RISK_AUTH_BASE_URL");

        std::fs::write(entitlement_path.join("device_secret"), "device-secret").expect("device secret");
        let device_id = super::derive_device_id_for_dir(&entitlement_path).expect("device id");
        let json = format!(
            r#"{{"user_id":"user-1","plan":"pro_personal_monthly","features":["observe"],"issued_at":"2026-06-20T00:00:00Z","valid_until":"2999-01-01T00:00:00Z","device_id":"{}","last_refresh_time":"2026-06-20T00:00:00Z","status":"active","next_billing_at":"2999-01-31T00:00:00Z"}}"#,
            device_id
        );
        write_signed_entitlement(&entitlement_path, &json);

        let rendered = super::render_auth_status_for_dir_with_workspace(&entitlement_path, &workspace_path);
        assert!(rendered.contains("已登录"));
        let refreshed_raw = std::fs::read_to_string(entitlement_path.join("entitlement.json")).expect("refreshed entitlement");
        assert!(refreshed_raw.contains("\"notify\""));
        let refreshed_sig = std::fs::read_to_string(entitlement_path.join("entitlement.sig")).expect("sig");
        assert_eq!(
            crate::entitlement::verify_entitlement_signature(&refreshed_raw, refreshed_sig.trim()),
            crate::entitlement::SignatureVerifyResult::Valid,
            "workspace refresh must write a valid Ed25519 signature"
        );

        let _ = std::fs::remove_dir_all(&entitlement_path);
        let _ = std::fs::remove_dir_all(&workspace_path);
    }

    #[test]
    fn watch_summary_hides_low_severity_by_default_and_shows_it_in_verbose_mode() {
        let payload = json!({
            "generated_at": "2026-06-26T00:00:00Z",
            "review": {
                "gate_decision": {
                    "decision": "warn",
                },
                "findings": [
                    {
                        "severity": "critical",
                        "plain_explanation": "critical finding",
                        "location": { "file_path": "src/a.ts", "start_line": 3 }
                    },
                    {
                        "severity": "low",
                        "plain_explanation": "low finding",
                        "location": { "file_path": "src/b.ts", "start_line": 9 }
                    }
                ]
            }
        });

        let normal = super::render_watch_summary(&payload, false).expect("normal summary");
        assert!(normal.contains("critical=1"));
        assert!(!normal.contains("low=1"));
        assert!(!normal.contains("[low]"));

        let verbose = super::render_watch_summary(&payload, true).expect("verbose summary");
        assert!(verbose.contains("low=1"));
        assert!(verbose.contains("[low] src/b.ts:9 low finding"));
    }

    #[test]
    fn watch_debounce_suppresses_same_file_and_rule_within_ten_minutes() {
        let finding = json!({
            "rule_id": "check.l3",
            "location": {
                "file_path": "src/a.ts",
                "start_line": 3
            }
        });
        let mut previous = std::collections::BTreeMap::new();
        previous.insert("src/a.ts::check.l3".to_string(), 1_000);

        assert!(!super::should_emit_watch_finding(&finding, 1_000 + 60_000, &previous, 600_000));
        assert!(super::should_emit_watch_finding(&finding, 1_000 + 601_000, &previous, 600_000));
    }

    #[test]
    fn l3_signal_file_path_reaches_finding_location() {
        let findings = super::derive_findings(&json!({
            "l3_violations": [{
                "signal": {
                    "description": "shared data coupling",
                    "file_path": "src/handler.rs",
                    "line": 0,
                    "level": 3,
                    "affected_nodes": ["source", "target"]
                },
                "level": 3
            }]
        }));

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["location"]["file_path"], "src/handler.rs");
    }

    #[test]
    fn watch_summary_surfaces_suppressed_count_when_repeats_are_debounced() {
        let payload = json!({
            "generated_at": "2026-06-26T00:00:00Z",
            "suppressed_count": 2,
            "review": {
                "gate_decision": {
                    "decision": "warn",
                },
                "findings": []
            }
        });

        let summary = super::render_watch_summary(&payload, false).expect("summary");
        assert!(summary.contains("suppressed=2"));
    }

    #[test]
    fn watch_human_summary_colors_warning_headers_and_medium_findings() {
        let payload = json!({
            "generated_at": "2026-06-26T00:00:00Z",
            "review": {
                "gate_decision": {
                    "decision": "warn",
                },
                "findings": [
                    {
                        "severity": "medium",
                        "plain_explanation": "medium finding",
                        "location": { "file_path": "src/a.ts", "start_line": 3 }
                    }
                ]
            }
        });

        let summary = super::render_watch_summary_human(&payload, false).expect("human summary");
        assert!(summary.contains("\u{1b}[33m"));
        assert!(summary.contains("[medium] src/a.ts:3 medium finding"));
    }

    #[test]
    fn parses_repair_plan_command() {
        let parsed = parse_cli_command(&args(&[
            "repair", "plan", "/tmp/repo", "--finding", "l5_violations:0", "--json",
        ]))
        .expect("repair plan should parse");
        assert_eq!(parsed.tier, CommandTier::Secondary);
        assert_eq!(parsed.default_output, DefaultOutputMode::Json);
        assert!(
            matches!(
                &parsed.command,
                CliCommand::RepairPlan { workspace, finding_id }
                    if workspace == "/tmp/repo" && finding_id == "l5_violations:0"
            ),
            "unexpected command: {:?}",
            parsed.command
        );
    }

    #[test]
    fn parses_repair_apply_command() {
        let parsed = parse_cli_command(&args(&[
            "repair", "apply", "/tmp/repo", "--plan", "rp_abc123def456", "--json",
        ]))
        .expect("repair apply should parse");
        assert_eq!(parsed.tier, CommandTier::Secondary);
        assert_eq!(parsed.default_output, DefaultOutputMode::Json);
        assert!(
            matches!(
                &parsed.command,
                CliCommand::RepairApply { workspace, plan_id, approve }
                    if workspace == "/tmp/repo" && plan_id == "rp_abc123def456" && !approve
            ),
            "unexpected command: {:?}",
            parsed.command
        );

        let approved = parse_cli_command(&args(&[
            "repair", "apply", "/tmp/repo", "--plan", "rp_abc123def456", "--approve",
        ]))
        .expect("repair apply --approve should parse");
        assert!(matches!(approved.command, CliCommand::RepairApply { approve: true, .. }));

        let approve = parse_cli_command(&args(&[
            "repair", "approve", "/tmp/repo", "--plan", "rp_abc123def456",
        ]))
        .expect("repair approve should parse");
        assert!(matches!(approve.command, CliCommand::RepairApprove { .. }));
    }

    #[test]
    fn rollback_repair_snapshots_restores_binary_bytes_and_removes_created_file() {
        let root = std::env::temp_dir().join(format!("audit-risk-repair-rollback-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("rollback root");
        let binary_path = root.join("bytes.bin");
        let created_path = root.join("created.rs");
        let original_bytes = vec![0_u8, 255, 10, 128, 65];
        std::fs::write(&binary_path, &original_bytes).expect("binary fixture");
        std::fs::write(&binary_path, b"changed bytes").expect("mutate binary");
        std::fs::write(&created_path, b"created").expect("created fixture");

        let candidates = vec![
            super::RepairCandidate {
                relative_path: "bytes.bin".to_string(),
                absolute_path: binary_path.clone(),
                snapshot: super::RepairSnapshot {
                    path: binary_path.clone(),
                    existed: true,
                    bytes: original_bytes.clone(),
                },
                patch_content: "changed bytes".to_string(),
                replacement: "changed bytes".to_string(),
            },
            super::RepairCandidate {
                relative_path: "created.rs".to_string(),
                absolute_path: created_path.clone(),
                snapshot: super::RepairSnapshot {
                    path: created_path.clone(),
                    existed: false,
                    bytes: Vec::new(),
                },
                patch_content: "created".to_string(),
                replacement: "created".to_string(),
            },
        ];

        let failures = super::rollback_repair_snapshots(&candidates);
        let binary_after = std::fs::read(&binary_path).expect("binary after rollback");
        let created_exists = created_path.exists();
        let _ = std::fs::remove_dir_all(&root);

        assert!(failures.is_empty(), "rollback failures: {failures:?}");
        assert_eq!(binary_after, original_bytes);
        assert!(!created_exists, "rollback must delete files that did not exist before apply");
    }

    #[test]
    fn repair_plan_rejects_missing_finding_flag() {
        let err = parse_cli_command(&args(&["repair", "plan", "/tmp/repo"]))
            .expect_err("missing --finding should be a usage error");
        assert!(
            err.message().contains("--finding"),
            "error must mention --finding, got: {}",
            err.message()
        );
    }

    #[test]
    fn repair_apply_rejects_missing_plan_flag() {
        let err = parse_cli_command(&args(&["repair", "apply", "/tmp/repo"]))
            .expect_err("missing --plan should be a usage error");
        assert!(
            err.message().contains("--plan"),
            "error must mention --plan, got: {}",
            err.message()
        );
    }

    #[test]
    fn repair_unknown_subaction_returns_usage_error() {
        let err = parse_cli_command(&args(&["repair", "execute", "/tmp/repo"]))
            .expect_err("unknown repair subaction should be a usage error");
        assert!(
            err.message().contains("execute") || err.message().contains("repair"),
            "error must mention the bad subaction, got: {}",
            err.message()
        );
    }

    #[test]
    fn repair_with_no_subaction_returns_usage_error() {
        let err = parse_cli_command(&args(&["repair"]))
            .expect_err("bare repair with no subaction should be a usage error");
        assert!(
            err.message().contains("repair"),
            "error must mention repair, got: {}",
            err.message()
        );
    }
}
