use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    let root = std::env::temp_dir().join(format!("audit-risk-repair-security-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(root.join(".hologram/repair-plans")).expect("create repair plans directory");
    let output = Command::new("git")
        .args(["init", "-q", root.to_str().expect("utf8 workspace")])
        .output()
        .expect("initialize git workspace");
    assert!(output.status.success(), "git init failed: {}", String::from_utf8_lossy(&output.stderr));
    root
}

fn write_plan(workspace: &Path, plan_id: &str, approval_state: &str, required_tests: Value, operations: Value) {
    let plan = json!({
        "plan_id": plan_id,
        "finding_id": "finding-1",
        "expires_at": "2999-01-01T00:00:00Z",
        "approval_state": approval_state,
        "required_tests": required_tests,
        "operations": operations,
    });
    fs::write(
        workspace.join(".hologram/repair-plans").join(format!("{plan_id}.json")),
        serde_json::to_vec(&plan).expect("serialize plan"),
    )
    .expect("write plan");
}

fn operation(path: &str, content: &str) -> Value {
    json!({
        "file_path": path,
        "start_line": 1,
        "end_line": 1,
        "new_content": content,
    })
}

fn run_cli(workspace: &Path, args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_audit-risk"))
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run audit-risk")
}

fn apply_args(workspace: &Path, plan_id: &str) -> Vec<String> {
    vec![
        "repair".to_string(),
        "apply".to_string(),
        workspace.display().to_string(),
        "--plan".to_string(),
        plan_id.to_string(),
    ]
}

#[test]
fn repair_apply_requires_recorded_approval_before_writing() {
    let root = workspace();
    fs::write(root.join("source.rs"), "let value = 1;\n").expect("source");
    write_plan(
        &root,
        "waiting-plan",
        "waiting_approval",
        json!(["git diff --check"]),
        json!([operation("source.rs", "let value = 2;")]),
    );

    let output = run_cli(&root, &apply_args(&root, "waiting-plan"));
    let source_before_approval = fs::read_to_string(root.join("source.rs")).expect("source after rejected apply");
    let approve_args = vec![
        "repair".to_string(),
        "approve".to_string(),
        root.display().to_string(),
        "--plan".to_string(),
        "waiting-plan".to_string(),
    ];
    let approval = run_cli(&root, &approve_args);
    assert!(
        approval.status.success(),
        "repair approve must record approval; stdout: {}; stderr: {}",
        String::from_utf8_lossy(&approval.stdout),
        String::from_utf8_lossy(&approval.stderr),
    );
    let applied = run_cli(&root, &apply_args(&root, "waiting-plan"));
    let source_after_approval = fs::read_to_string(root.join("source.rs")).expect("source after approved apply");
    let audit = fs::read_to_string(root.join(".hologram/audit.jsonl")).expect("approval audit");
    let _ = fs::remove_dir_all(&root);

    assert!(!output.status.success(), "unapproved apply must be rejected");
    assert_eq!(source_before_approval, "let value = 1;\n", "unapproved plan must not write files");
    assert!(audit.contains("\"event_type\":\"repair_approved\""), "approval must write a standalone audit event");
    assert!(audit.contains("\"approved_by\":"), "approval audit must identify the local CLI approver");
    assert!(audit.contains("\"approved_at\":"), "approval audit must record approval time");
    assert!(applied.status.success(), "approved plan must apply");
    assert_eq!(source_after_approval, "let value = 2;\n");
}

#[test]
fn repair_apply_approve_records_approval_before_writing() {
    let root = workspace();
    fs::write(root.join("source.rs"), "let value = 1;\n").expect("source");
    write_plan(
        &root,
        "single-step-plan",
        "waiting_approval",
        json!(["git diff --check"]),
        json!([operation("source.rs", "let value = 2;")]),
    );
    let mut args = apply_args(&root, "single-step-plan");
    args.push("--approve".to_string());

    let output = run_cli(&root, &args);
    let audit = fs::read_to_string(root.join(".hologram/audit.jsonl")).expect("approval audit");
    let source = fs::read_to_string(root.join("source.rs")).expect("source after apply");
    let _ = fs::remove_dir_all(&root);

    assert!(output.status.success(), "apply --approve must approve then apply");
    assert!(audit.find("repair_approved").expect("approval audit") < audit.find("repair_applied").expect("apply audit"));
    assert_eq!(source, "let value = 2;\n");
}

#[test]
fn repair_apply_rejects_untrusted_plan_inputs_without_side_effects() {
    let root = workspace();
    let outside = root.parent().expect("parent").join("outside.txt");
    let marker = root.parent().expect("parent").join("rce-proof");
    let git_config = root.join(".git/config");
    let git_config_before = fs::read(&git_config).expect("git config before apply");
    let lock_file = root.join("Cargo.lock");
    fs::write(&lock_file, b"lock content\n").expect("lock file");
    let cases = [
        ("absolute", outside.display().to_string(), json!(["git diff --check"])),
        ("traversal", "../outside.txt".to_string(), json!(["git diff --check"])),
        ("sensitive", ".env".to_string(), json!(["git diff --check"])),
        ("git-config", ".git/config".to_string(), json!(["git diff --check"])),
        ("lock-file", "Cargo.lock".to_string(), json!(["git diff --check"])),
        ("rce", "safe.rs".to_string(), json!([format!("touch {}", marker.display())])),
    ];

    let mut statuses = Vec::new();
    for (plan_id, target, required_tests) in cases {
        write_plan(
            &root,
            plan_id,
            "approved",
            required_tests,
            json!([operation(&target, "unsafe write")]),
        );
        statuses.push(run_cli(&root, &apply_args(&root, plan_id)).status.success());
    }
    let outside_content = fs::read_to_string(&outside).ok();
    let marker_exists = marker.exists();
    let env_exists = root.join(".env").exists();
    let git_config_after = fs::read(&git_config).expect("git config after apply");
    let lock_file_after = fs::read(&lock_file).expect("lock file after apply");
    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_file(&outside);
    let _ = fs::remove_file(&marker);

    assert!(statuses.iter().all(|success| !success), "all unsafe plans must be rejected");
    assert!(outside_content.is_none(), "unsafe plan must not write outside workspace");
    assert!(!marker_exists, "plan-controlled command must not execute");
    assert!(!env_exists, "sensitive workspace file must not be created");
    assert_eq!(git_config_after, git_config_before, ".git/config must remain unchanged");
    assert_eq!(lock_file_after, b"lock content\n", "lock file must remain unchanged");
}

#[test]
fn repair_apply_rejects_plan_id_path_traversal_before_loading_a_plan() {
    let root = workspace();
    fs::write(root.join("victim.rs"), "let value = 1;\n").expect("victim");
    fs::write(
        root.join(".hologram/evil.json"),
        serde_json::to_vec(&json!({
            "plan_id": "../evil",
            "finding_id": "finding-1",
            "expires_at": "2999-01-01T00:00:00Z",
            "approval_state": "approved",
            "required_tests": ["git diff --check"],
            "operations": [operation("victim.rs", "let value = 2;")],
        }))
        .expect("serialize traversal plan"),
    )
    .expect("write traversal plan");

    let output = run_cli(&root, &apply_args(&root, "../evil"));
    let victim = fs::read_to_string(root.join("victim.rs")).expect("victim after apply");
    let _ = fs::remove_dir_all(&root);

    assert!(!output.status.success(), "plan_id traversal must be rejected");
    assert_eq!(victim, "let value = 1;\n");
}

#[test]
fn repair_apply_blocks_new_risks_before_any_write() {
    let root = workspace();
    fs::write(root.join("source.rs"), "let value = 1;\n").expect("source");
    write_plan(
        &root,
        "risky-content",
        "approved",
        json!(["git diff --check"]),
        json!([operation("source.rs", "const password = \"supersecret123\";")]),
    );

    let output = run_cli(&root, &apply_args(&root, "risky-content"));
    let source = fs::read_to_string(root.join("source.rs")).expect("source after apply");
    let _ = fs::remove_dir_all(&root);

    assert!(!output.status.success(), "secondary audit must block new risk");
    assert_eq!(source, "let value = 1;\n", "blocked risk must not write files");
}

#[test]
fn repair_apply_rolls_back_new_files_and_preserves_binary_snapshots() {
    let root = workspace();
    let binary = vec![0_u8, 159, 146, 150, 255, 1];
    fs::write(root.join("binary.dat"), &binary).expect("binary fixture");
    fs::create_dir(root.join("write-fails")).expect("failing target directory");
    write_plan(
        &root,
        "rollback-plan",
        "approved",
        json!(["git diff --check"]),
        json!([
            operation("created.rs", "let created = true;"),
            operation("binary.dat", "replacement"),
            operation("write-fails", "cannot write a directory"),
        ]),
    );

    let output = run_cli(&root, &apply_args(&root, "rollback-plan"));
    let created_exists = root.join("created.rs").exists();
    let binary_after = fs::read(root.join("binary.dat")).expect("binary after apply");
    let _ = fs::remove_dir_all(&root);

    assert!(!output.status.success(), "write failure must fail apply");
    assert!(!created_exists, "rollback must delete files created by the failed apply");
    assert_eq!(binary_after, binary, "rollback must restore original binary bytes");
}
