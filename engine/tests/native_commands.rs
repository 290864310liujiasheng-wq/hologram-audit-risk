use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "audit-risk-native-commands-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(root.join(".hologram")).expect("create workspace");
    root
}

#[test]
fn check_rejects_extra_positional_argument() {
    let root = workspace();
    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["check", root.to_str().expect("utf8 workspace"), "extra_arg"])
        .output()
        .expect("run audit-risk check");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = fs::remove_dir_all(&root);

    assert!(
        !output.status.success(),
        "extra positional argument must fail"
    );
    assert!(
        stderr.contains("未知参数 `extra_arg`"),
        "error must name the extra argument: {stderr}"
    );
    assert!(
        stderr.contains("用法：audit-risk check <workspace> [--json] [--fail-on <level>]"),
        "error must show check usage: {stderr}"
    );
}

#[test]
fn check_warns_when_existing_baseline_is_corrupted() {
    let root = workspace();
    fs::write(root.join(".hologram/baseline.json"), "{not valid json")
        .expect("write corrupt baseline");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["check", root.to_str().expect("utf8 workspace"), "--json"])
        .output()
        .expect("run audit-risk check");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "check should continue without a usable baseline: {stderr}"
    );
    assert!(
        stderr.contains("baseline.json") && stderr.contains("损坏"),
        "corrupt baseline warning must be visible to the caller: {stderr}"
    );
}

#[test]
fn check_keeps_hologram_artifacts_out_of_git_status_and_changed_files() {
    let root = workspace();
    fs::write(root.join("main.rs"), "fn main() {}\n").expect("write source file");
    Command::new("git")
        .args(["init", root.to_str().expect("utf8 workspace")])
        .output()
        .expect("initialize git workspace");
    Command::new("git")
        .args([
            "-C",
            root.to_str().expect("utf8 workspace"),
            "add",
            "main.rs",
        ])
        .output()
        .expect("stage source file");
    let commit = Command::new("git")
        .args([
            "-C",
            root.to_str().expect("utf8 workspace"),
            "-c",
            "user.name=audit-risk-test",
            "-c",
            "user.email=audit-risk-test@example.com",
            "commit",
            "-m",
            "initial workspace",
        ])
        .output()
        .expect("commit source file");
    assert!(
        commit.status.success(),
        "git commit must succeed: {}",
        String::from_utf8_lossy(&commit.stderr)
    );

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["check", root.to_str().expect("utf8 workspace"), "--json"])
        .output()
        .expect("run audit-risk check");
    let response: Value = serde_json::from_slice(&output.stdout).expect("check JSON response");
    let status = Command::new("git")
        .args([
            "-C",
            root.to_str().expect("utf8 workspace"),
            "status",
            "--short",
        ])
        .output()
        .expect("read git status");
    let status_stdout = String::from_utf8_lossy(&status.stdout);
    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "check must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(status.status.success(), "git status must succeed");
    assert!(
        status_stdout.trim().is_empty(),
        "check artifacts must not dirty the workspace: {status_stdout}"
    );
    assert!(
        response["changed_files"]
            .as_array()
            .expect("check response changed_files")
            .iter()
            .all(|path| !path.as_str().unwrap_or_default().starts_with(".hologram/")),
        "check must not treat its own artifacts as user changes: {}",
        response["changed_files"]
    );
}

#[test]
fn concurrent_checks_do_not_fail_on_sqlite_locks() {
    let root = workspace();
    let mut checks = Vec::new();
    for _ in 0..3 {
        let child = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
            .args(["check", root.to_str().expect("utf8 workspace"), "--json"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start concurrent audit-risk check");
        checks.push(child);
    }

    let outputs = checks
        .into_iter()
        .map(|child| {
            child
                .wait_with_output()
                .expect("wait for concurrent audit-risk check")
        })
        .collect::<Vec<_>>();
    for output in &outputs {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "concurrent check must not fail: {stderr}"
        );
        assert!(
            !stderr.contains("database is locked"),
            "concurrent check must not report a SQLite lock: {stderr}"
        );
    }

    let report_output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["report", root.to_str().expect("utf8 workspace"), "--json"])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run report after concurrent checks");
    let report: Value =
        serde_json::from_slice(&report_output.stdout).expect("report JSON response");
    let _ = fs::remove_dir_all(&root);

    assert!(
        report_output.status.success(),
        "report must read the concurrent audit chain: {}",
        String::from_utf8_lossy(&report_output.stderr)
    );
    assert_eq!(report["audit"]["integrity"]["status"], "verified");
    assert_eq!(report["audit"]["integrity"]["entry_count"], 4);
}

#[test]
fn check_writes_a_queryable_chained_review_audit_entry() {
    let root = workspace();
    let check_output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["check", root.to_str().expect("utf8 workspace"), "--json"])
        .output()
        .expect("run audit-risk check");
    let audit_output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args([
            "audit",
            root.to_str().expect("utf8 workspace"),
            "--query",
            "review",
        ])
        .output()
        .expect("query review audit records");
    let response: Value =
        serde_json::from_slice(&audit_output.stdout).expect("audit JSON response");
    let audit_entry: Value = serde_json::from_str(
        fs::read_to_string(root.join(".hologram/audit.jsonl"))
            .expect("audit log")
            .trim(),
    )
    .expect("audit entry JSON");
    let _ = fs::remove_dir_all(&root);

    assert!(
        check_output.status.success(),
        "check must succeed: {}",
        String::from_utf8_lossy(&check_output.stderr)
    );
    assert!(audit_output.status.success());
    assert_eq!(response["total_matches"], 1);
    assert_eq!(response["records"][0]["plane"], "review");
    assert_eq!(audit_entry["tool"], "review_check");
    assert!(audit_entry["details"]["finding_count"].is_number());
    assert!(audit_entry["integrity_hash"].as_str().is_some());
}

#[test]
fn check_refuses_to_extend_a_malformed_audit_chain() {
    let root = workspace();
    let audit_path = root.join(".hologram/audit.jsonl");
    fs::write(&audit_path, "{not json\n").expect("write malformed audit log");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["check", root.to_str().expect("utf8 workspace"), "--json"])
        .output()
        .expect("run audit-risk check");
    let audit_contents = fs::read_to_string(&audit_path).expect("audit log remains readable");
    let _ = fs::remove_dir_all(&root);

    assert!(
        !output.status.success(),
        "malformed chains must fail closed"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("无效 JSON"));
    assert_eq!(audit_contents, "{not json\n");
}

#[test]
fn custom_audit_path_is_excluded_from_subsequent_check_changes() {
    let root = workspace();
    fs::write(root.join("main.rs"), "fn main() {}\n").expect("write source");
    fs::write(
        root.join(".hologram/delivery.json"),
        r#"{"audit":{"jsonl_path":"./logs/../logs/audit.jsonl"}}"#,
    )
    .expect("write delivery config");
    Command::new("git")
        .args(["init", root.to_str().expect("utf8 workspace")])
        .output()
        .expect("initialize git workspace");

    let first = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["check", root.to_str().expect("utf8 workspace"), "--json"])
        .output()
        .expect("run first check");
    let second = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["check", root.to_str().expect("utf8 workspace"), "--json"])
        .output()
        .expect("run second check");
    let response: Value = serde_json::from_slice(&second.stdout).expect("second check JSON");
    let _ = fs::remove_dir_all(&root);

    assert!(first.status.success());
    assert!(second.status.success());
    assert!(response["changed_files"]
        .as_array()
        .expect("changed files")
        .iter()
        .all(|path| path.as_str() != Some("logs/audit.jsonl")));
}

#[test]
fn audit_command_returns_filtered_jsonl_records_without_node() {
    let root = workspace();
    let entries = [
        json!({"plane":"review","stage":"completed","status":"block","subject":"src/auth.rs","reason":"review risk"}),
        json!({"plane":"repair","stage":"apply","status":"ok","subject":"src/lib.rs","reason":"repair completed"}),
    ];
    let jsonl = entries
        .iter()
        .map(|entry| serde_json::to_string(entry).expect("serialize audit entry"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        root.join(".hologram/audit.jsonl"),
        format!("{jsonl}\n{{bad json\n"),
    )
    .expect("write audit jsonl");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args([
            "audit",
            root.to_str().expect("utf8 workspace"),
            "--query",
            "review",
            "--limit",
            "5",
        ])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk audit");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: Value = serde_json::from_str(&stdout).expect("audit JSON response");
    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "audit must not require node: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(response["query"], "review");
    assert_eq!(response["total_matches"], 1);
    assert_eq!(response["records"].as_array().map(Vec::len), Some(1));
    assert_eq!(response["records"][0]["plane"], "review");
}

#[test]
fn audit_command_returns_most_recent_records_without_query() {
    let root = workspace();
    let entries = [
        json!({"plane":"review","stage":"first","status":"allow","subject":"old","reason":"old record"}),
        json!({"plane":"review","stage":"second","status":"allow","subject":"middle","reason":"middle record"}),
        json!({"plane":"repair","stage":"third","status":"allow","subject":"new","reason":"new record"}),
    ];
    let jsonl = entries
        .iter()
        .map(|entry| serde_json::to_string(entry).expect("serialize audit entry"))
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(root.join(".hologram/audit.jsonl"), format!("{jsonl}\n")).expect("write audit jsonl");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args([
            "audit",
            root.to_str().expect("utf8 workspace"),
            "--limit",
            "2",
        ])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk audit");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: Value = serde_json::from_str(&stdout).expect("audit JSON response");
    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "audit must not require node: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(response["total_matches"], 3);
    assert_eq!(response["records"].as_array().map(Vec::len), Some(2));
    assert_eq!(response["records"][0]["subject"], "new");
    assert_eq!(response["records"][1]["subject"], "middle");
}

#[test]
fn rules_command_returns_two_plane_summaries_without_node() {
    let root = workspace();
    fs::create_dir_all(root.join(".hologram/rules")).expect("rules directory");
    fs::write(
        root.join(".hologram/delivery.json"),
        serde_json::to_vec(&json!({
            "rule_packages": {
                "review_paths": [".hologram/rules/review.workspace.json"],
                "repair_paths": [".hologram/rules/repair.workspace.json"],
            },
        }))
        .expect("delivery config"),
    )
    .expect("write delivery config");
    fs::write(
        root.join(".hologram/rules/review.workspace.json"),
        serde_json::to_vec(&json!({
            "package_id": "review.workspace",
            "version": "v1",
            "plane": "review",
            "enabled": true,
            "rules": [{"rule_id": "workspace.review", "priority": 10, "enabled": true}],
        }))
        .expect("review package"),
    )
    .expect("write review package");
    fs::write(
        root.join(".hologram/rules/repair.workspace.json"),
        serde_json::to_vec(&json!({
            "package_id": "repair.workspace",
            "version": "v1",
            "plane": "repair",
            "enabled": true,
            "rules": [{"rule_id": "workspace.repair", "priority": 10, "enabled": true}],
        }))
        .expect("repair package"),
    )
    .expect("write repair package");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["rules", root.to_str().expect("utf8 workspace")])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk rules");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: Value = serde_json::from_str(&stdout).expect("rules JSON response");
    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "rules must not require node: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summaries = response.as_array().expect("rule summaries array");
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0]["plane"], "review");
    assert_eq!(summaries[1]["plane"], "repair");
    assert!(summaries[0]["package_ids"]
        .to_string()
        .contains("review.workspace"));
    assert!(summaries[1]["package_ids"]
        .to_string()
        .contains("repair.workspace"));
    assert!(summaries[0]["rule_count"].as_u64().unwrap_or_default() >= 1);
    assert!(summaries[1]["rule_count"].as_u64().unwrap_or_default() >= 1);
}

#[test]
fn report_command_writes_parseable_json_file_without_node() {
    let root = workspace();
    let output_path = root.join("report.json");
    fs::write(
        root.join(".hologram/audit.jsonl"),
        "{\"plane\":\"review\",\"stage\":\"check\",\"status\":\"allow\",\"subject\":\"workspace\",\"reason\":\"clean\"}\n",
    )
    .expect("write audit log");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args([
            "report",
            root.to_str().expect("utf8 workspace"),
            "--output",
            output_path.to_str().expect("utf8 output path"),
            "--json",
        ])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk report");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: Value = serde_json::from_str(&stdout).expect("report JSON response");
    let report: Value =
        serde_json::from_slice(&fs::read(&output_path).expect("report output file"))
            .expect("parse generated report");
    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "report must not require node: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        response["workspace"]["root"],
        root.to_str().expect("utf8 workspace")
    );
    assert_eq!(report["report_signature"]["algorithm"], "sha256");
    assert!(report["current_review"].is_object());
}

#[test]
fn report_appends_its_current_review_before_building_the_audit_section() {
    let root = workspace();
    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["report", root.to_str().expect("utf8 workspace"), "--json"])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk report");
    let response: Value = serde_json::from_slice(&output.stdout).expect("report JSON response");
    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "report must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(response["audit"]["integrity"]["status"], "verified");
    assert_eq!(response["audit"]["integrity"]["entry_count"], 1);
    assert_eq!(response["audit"]["entries"][0]["tool"], "review_check");
    assert_eq!(
        response["audit"]["entries"][0]["details"]["source_command"],
        "report"
    );
}

#[test]
fn report_command_exits_2_when_block_gate_triggers_without_node() {
    let root = workspace();
    fs::write(
        root.join("danger.py"),
        "api_key = \"sk-abcdefghijklmnopqrstuvwxyz123456\"\n",
    )
    .expect("write block-level risk fixture");
    Command::new("git")
        .args(["init", root.to_str().expect("utf8 workspace")])
        .output()
        .expect("initialize git workspace");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args([
            "report",
            root.to_str().expect("utf8 workspace"),
            "--fail-on",
            "block",
            "--json",
        ])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk report");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: Value = serde_json::from_str(&stdout).expect("report JSON response");
    let _ = fs::remove_dir_all(&root);

    assert_eq!(
        output.status.code(),
        Some(2),
        "report must preserve the fail gate: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(response["automation"]["should_fail"], true);
    assert_eq!(
        response["current_review"]["review"]["gate_decision"]["decision"],
        "block"
    );
}

#[test]
fn verify_command_returns_deprecation_message_without_node() {
    let root = workspace();
    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["verify", root.to_str().expect("utf8 workspace")])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk verify");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let _ = fs::remove_dir_all(&root);

    assert_eq!(
        output.status.code(),
        Some(1),
        "verify must exit with customer guidance: {stderr}"
    );
    assert!(
        stdout.contains("audit-risk check"),
        "verify must direct customers to check: {stdout}"
    );
    assert!(
        !stdout.contains("node"),
        "verify must not surface a Node runtime failure: {stdout}"
    );
}

#[test]
fn report_rejects_an_explicitly_invalid_delivery_config() {
    let root = workspace();
    let config_path = root.join("invalid-delivery.json");
    fs::write(&config_path, "{not json").expect("write invalid config");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args([
            "report",
            root.to_str().expect("utf8 workspace"),
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--json",
        ])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk report");
    let _ = fs::remove_dir_all(&root);

    assert!(
        !output.status.success(),
        "an explicit invalid config must not silently use defaults"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("delivery config"));
}

#[test]
fn report_rejects_a_semantically_invalid_delivery_config() {
    let root = workspace();
    let config_path = root.join("invalid-delivery.json");
    fs::write(&config_path, "{\"audit\":{\"recent_limit\":0}}").expect("write invalid config");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args([
            "report",
            root.to_str().expect("utf8 workspace"),
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--json",
        ])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk report");
    let _ = fs::remove_dir_all(&root);

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("recent_limit"));
}

#[test]
fn report_applies_disabled_review_rules_to_the_gate() {
    let root = workspace();
    fs::write(
        root.join("danger.py"),
        "api_key = \"sk-abcdefghijklmnopqrstuvwxyz123456\"\n",
    )
    .expect("write block-level risk fixture");
    Command::new("git")
        .args(["init", root.to_str().expect("utf8 workspace")])
        .output()
        .expect("initialize git workspace");
    let config_path = root.join("delivery.json");
    fs::write(
        &config_path,
        "{\"rule_packages\":{\"disabled_review_rule_ids\":[\"check.l5\"]}}",
    )
    .expect("write policy override");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args([
            "report",
            root.to_str().expect("utf8 workspace"),
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--fail-on",
            "block",
            "--json",
        ])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk report");
    let response: Value = serde_json::from_slice(&output.stdout).expect("report JSON response");
    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "disabled L5 rule must remove the block gate"
    );
    assert_eq!(
        response["current_review"]["gate_decision"]["decision"],
        "allow"
    );
    assert_eq!(response["automation"]["should_fail"], false);
}

#[test]
fn report_marks_a_tampered_audit_integrity_hash_as_failed() {
    let root = workspace();
    fs::write(
        root.join(".hologram/audit.jsonl"),
        "{\"ts\":\"2026-01-01T00:00:00Z\",\"tool\":\"review_check\",\"path\":\".\",\"action\":\"allowed\",\"reason\":\"clean\",\"details\":{},\"prev_hash\":null,\"integrity_hash\":\"tampered\"}\n",
    )
    .expect("write tampered audit log");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["report", root.to_str().expect("utf8 workspace"), "--json"])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk report");
    let response: Value = serde_json::from_slice(&output.stdout).expect("report JSON response");
    let _ = fs::remove_dir_all(&root);

    assert!(
        output.status.success(),
        "report still writes a failed integrity result"
    );
    assert_eq!(response["audit"]["integrity"]["status"], "failed");
    assert_eq!(response["audit"]["integrity"]["verified"], false);
    assert!(!response["audit"]["integrity"]["issues"]
        .as_array()
        .expect("issues")
        .is_empty());
}

#[test]
fn report_verifies_an_untampered_delivery_audit_hash() {
    let root = workspace();
    let payload = "{\"ts\":\"2026-01-01T00:00:00Z\",\"tool\":\"review_check\",\"path\":\".\",\"action\":\"allowed\",\"reason\":\"clean\",\"details\":{},\"prev_hash\":null}";
    let hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
    let audit_line = format!(
        "{{\"reason\":\"clean\",\"action\":\"allowed\",\"tool\":\"review_check\",\"path\":\".\",\"details\":{{}},\"ts\":\"2026-01-01T00:00:00Z\",\"prev_hash\":null,\"integrity_hash\":\"{hash}\"}}"
    );
    fs::write(
        root.join(".hologram/audit.jsonl"),
        format!("{audit_line}\n"),
    )
    .expect("write valid audit log");

    let output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["report", root.to_str().expect("utf8 workspace"), "--json"])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run audit-risk report");
    let response: Value = serde_json::from_slice(&output.stdout).expect("report JSON response");
    let _ = fs::remove_dir_all(&root);

    assert!(output.status.success());
    assert_eq!(response["audit"]["integrity"]["status"], "verified");
    assert_eq!(response["audit"]["integrity"]["verified"], true);
}

#[test]
fn report_normalizes_delivery_audit_entries_and_renders_the_gate() {
    let root = workspace();
    fs::write(
        root.join(".hologram/audit.jsonl"),
        "{\"ts\":\"2026-01-01T00:00:00Z\",\"tool\":\"review_check\",\"path\":\".\",\"action\":\"allowed\",\"reason\":\"clean\",\"details\":{\"gate_decision\":{\"decision\":\"allow\",\"finding_ids\":[]}}}\n",
    )
    .expect("write delivery audit log");

    let json_output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["report", root.to_str().expect("utf8 workspace"), "--json"])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run JSON report");
    let response: Value =
        serde_json::from_slice(&json_output.stdout).expect("report JSON response");
    let human_output = Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(["report", root.to_str().expect("utf8 workspace")])
        .env("PATH", "/usr/bin:/bin")
        .output()
        .expect("run human report");
    let _ = fs::remove_dir_all(&root);

    assert!(json_output.status.success());
    assert_eq!(response["audit"]["integrity"]["status"], "partial");
    assert_eq!(response["audit"]["integrity"]["verified"], false);
    assert_eq!(response["audit"]["integrity"]["unprotected_count"], 1);
    let records = response["audit"]["records"]
        .as_array()
        .expect("audit records");
    let legacy_record = records
        .iter()
        .find(|record| record["event_id"] == "review_check:2026-01-01T00:00:00Z:.")
        .expect("normalized legacy record");
    assert_eq!(legacy_record["plane"], "review");
    assert!(records
        .iter()
        .any(|record| record["raw"]["details"]["source_command"] == "report"));
    assert!(response["current_review"]["gate_decision"].is_object());
    assert!(human_output.status.success());
    assert!(String::from_utf8_lossy(&human_output.stdout).contains("最近 gate："));
    assert!(!String::from_utf8_lossy(&human_output.stdout).contains("最近 gate：未知"));
}
