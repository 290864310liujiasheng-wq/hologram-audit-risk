# P1-4：将 report / audit / rules / verify 移植为纯 Rust（消除 Node.js 运行时依赖）

**优先级：P1（客户首次使用体验阻断）**  
**目标文件：`engine/src/cli.rs`**  
**不能修改：`src-ui/`（TypeScript 源码保留，仅停止从 CLI 调用它）**

---

## 背景

`report`、`audit`、`rules`、`verify` 这 4 个命令目前全部走 `run_phase5_secondary_command()`，
它在运行时调用 `node --import tsx src-ui/scripts/phase5-delivery.ts`。

客户用 `install.sh` 只安装了 Rust 二进制，机器上没有 Node.js，
所以这 4 个命令一执行就报错，即使安装完全成功也无法使用。

**修复目标：这 4 个命令只依赖 Rust 二进制本身，不需要 node / npm / tsx。**

---

## 改动范围

### 1. `audit` ── 读 JSONL + 过滤（最简单，先做）

**现在的调用路径（要删除）：**
```
CliCommand::Audit → run_phase5_secondary_command("audit", ...) → node → searchDeliveryAuditRecords()
```

**修成：**
```
CliCommand::Audit → run_audit_command(workspace, config, query, limit)
```

**新函数逻辑（对照 `src-ui/src/risk/delivery.ts` 第 321-358 行）：**

```rust
fn run_audit_command(
    workspace: Option<String>,
    config: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_workspace_or_cwd(workspace.as_deref())?;
    let jsonl_path = workspace_path.join(".hologram/audit.jsonl");
    let limit = limit.unwrap_or(20);
    let query_str = query.as_deref().unwrap_or("").trim().to_lowercase();

    // 读取最后 N 条 JSONL 记录
    let all_records: Vec<Value> = read_audit_jsonl(&jsonl_path);

    // 过滤（与 TypeScript searchDeliveryAuditRecords 逻辑一致）
    let filtered: Vec<&Value> = if query_str.is_empty() {
        all_records.iter().take(limit).collect()
    } else {
        all_records.iter().filter(|record| {
            let haystack = [
                record["plane"].as_str().unwrap_or(""),
                record["stage"].as_str().unwrap_or(""),
                record["status"].as_str().unwrap_or(""),
                record["subject"].as_str().unwrap_or(""),
                record["reason"].as_str().unwrap_or(""),
                record.get("error").and_then(|e| e["code"].as_str()).unwrap_or(""),
            ]
            .join(" ")
            .to_lowercase();
            haystack.contains(&query_str)
        }).take(limit).collect()
    };

    let total_matches = if query_str.is_empty() {
        all_records.len()
    } else {
        all_records.iter().filter(/* 同上 */).count()
    };

    let output = json!({
        "query": query.as_deref().unwrap_or(""),
        "total_matches": total_matches,
        "records": filtered,
    });
    Ok(CommandOutcome::json(0, output))
}

fn read_audit_jsonl(path: &Path) -> Vec<Value> {
    // 读取文件 → 按行拆分 → 解析 JSON → 跳过解析失败的行 → 返回 Vec<Value>
    // 取最后 config.audit.recent_limit 条（默认 200，可固定写死）
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .collect()
}
```

**输出 JSON 结构与原来保持一致：**
```json
{
  "query": "review",
  "total_matches": 42,
  "records": [ { "plane": "review", "stage": "...", ... } ]
}
```

---

### 2. `rules` ── 复用 check 结果，提取策略摘要（次简单）

**现在的调用路径（要删除）：**
```
CliCommand::Rules → run_phase5_secondary_command("rules", ...) → node → buildDeliveryRuleSummaries()
```

**修成：**
```
CliCommand::Rules → run_rules_command(workspace, config)
```

**新函数逻辑（对照 `src-ui/src/risk/delivery.ts` 第 306-320 行）：**

```rust
fn run_rules_command(
    workspace: Option<String>,
    config: Option<String>,
) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_workspace_or_cwd(workspace.as_deref())?;
    let check = build_workspace_check_payload(&workspace_path)?;

    // policies 字段在 check 输出的 review.policies 或顶层 policies 里
    // 提取 review 和 repair 两个 plane 的摘要
    let policies = check.get("review")
        .and_then(|r| r.get("policies"))
        .or_else(|| check.get("policies"))
        .cloned()
        .unwrap_or(json!({}));

    let summaries: Vec<Value> = ["review", "repair"].iter().map(|plane| {
        let policy = policies.get(plane).cloned().unwrap_or(json!({}));
        let rules = policy.get("rules").and_then(Value::as_array).map(|r| r.as_slice()).unwrap_or(&[]);
        json!({
            "plane": plane,
            "policy_snapshot_id": policy.get("policy_snapshot_id").cloned().unwrap_or(json!(null)),
            "package_ids": policy.get("packages").and_then(Value::as_array)
                .map(|pkgs| pkgs.iter().filter_map(|p| p.get("package_id").cloned()).collect::<Vec<_>>())
                .unwrap_or_default(),
            "rule_count": rules.len(),
            "top_rule_ids": rules.iter().take(5)
                .filter_map(|r| r.get("rule_id").cloned())
                .collect::<Vec<_>>(),
        })
    }).collect();

    Ok(CommandOutcome::json(0, json!(summaries)))
}
```

---

### 3. `report` ── check + JSONL → 机器报告 JSON（最复杂）

**现在的调用路径（要删除）：**
```
CliCommand::Report → run_report_command() → run_phase5_secondary_command("report", ...) → node → buildDeliveryMachineReport()
```

**修成：**
```
CliCommand::Report → run_report_command_native(workspace, config, output, fail_on, history_compare)
```

**新函数逻辑（对照 `src-ui/src/risk/delivery.ts` 第 255-304 行）：**

```rust
fn run_report_command_native(
    workspace: Option<String>,
    config: Option<String>,
    output_path_arg: Option<String>,
    fail_on: Option<FailGate>,
    history_compare: bool,
    output_mode: DefaultOutputMode,
) -> Result<CommandOutcome, CliRuntimeError> {
    let workspace_path = resolve_workspace_or_cwd(workspace.as_deref())?;

    // 1. 运行 check（已有 Rust 实现）
    let check = build_workspace_check_payload(&workspace_path)?;

    // 2. 读取审计日志（最近 200 条）
    let default_jsonl = ".hologram/audit.jsonl";
    let jsonl_path = workspace_path.join(default_jsonl);
    let audit_records = read_audit_jsonl(&jsonl_path);

    // 3. 读取 delivery.json 配置（若有）
    let delivery_config = load_delivery_config_optional(&workspace_path, config.as_deref());

    // 4. 组装机器报告（对照 buildDeliveryMachineReport 的输出结构）
    let report_output_path_str = delivery_config.as_ref()
        .and_then(|c| c.get("audit").and_then(|a| a.get("report_output_path")).and_then(Value::as_str))
        .unwrap_or(".hologram/latest-risk-report.json");
    let output_path = output_path_arg
        .as_deref()
        .map(Path::new)
        .map(|p| if p.is_absolute() { p.to_path_buf() } else { workspace_path.join(p) })
        .unwrap_or_else(|| workspace_path.join(report_output_path_str));

    let generated_at = now_iso();
    let report = json!({
        "generated_at": generated_at,
        "workspace": {
            "root": normalize_path(workspace_path.display().to_string()),
            "audit_jsonl_path": default_jsonl,
            "report_output_path": report_output_path_str,
        },
        "current_review": check.get("review").cloned().unwrap_or(json!({})),
        "audit": {
            "records": &audit_records,
            "total": audit_records.len(),
        },
        // automation.should_fail 由 fail_on gate 决定
        "automation": build_automation_block(&check, fail_on),
    });

    // 5. 写入报告文件
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(&output_path, serde_json::to_vec_pretty(&report).expect("serialize report"))
        .map_err(|e| CliRuntimeError::environment(format!("无法写入报告文件 {}：{e}", output_path.display())))?;

    // 6. 决定是否以 exit_code=2 触发 fail gate
    let should_fail = check_fail_gate(&check, fail_on);
    let exit_code = if should_fail { 2 } else { 0 };

    if output_mode == DefaultOutputMode::Json {
        Ok(CommandOutcome::json(exit_code, report))
    } else {
        Ok(CommandOutcome::text(exit_code, render_report_screen(&report)?))
    }
}
```

**辅助函数（小而局部）：**

```rust
fn load_delivery_config_optional(workspace: &Path, config_path: Option<&str>) -> Option<Value> {
    let path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join(".hologram/delivery.json"));
    fs::read_to_string(&path).ok().and_then(|raw| serde_json::from_str(&raw).ok())
}

fn build_automation_block(check: &Value, fail_on: Option<FailGate>) -> Value {
    let decision = check
        .get("review").and_then(|r| r.get("gate_decision")).and_then(|g| g.get("decision"))
        .and_then(Value::as_str)
        .unwrap_or("pass");
    let threshold = fail_on.map(fail_gate_to_str).unwrap_or("error");
    let should_fail = matches!(
        (decision, threshold),
        ("block", _) | ("require_approval", "warn" | "require_approval" | "block") | ("warn", "warn")
    );
    json!({ "fail_on_decision": threshold, "should_fail": should_fail })
}

fn check_fail_gate(check: &Value, fail_on: Option<FailGate>) -> bool {
    // 同 build_automation_block 里的 should_fail 逻辑
    build_automation_block(check, fail_on)["should_fail"].as_bool().unwrap_or(false)
}
```

---

### 4. `verify` ── 废弃为客户命令（最简单）

`verify` 是开发者 CI 工具（跑 `npm build`、`cargo test`、冒烟测试），不是客户命令。

**修成：直接输出说明并以退出码 1 返回，不再调用 Node：**

```rust
CliCommand::Verify { .. } => {
    Ok(CommandOutcome::text(1,
        "`audit-risk verify` 是开发环境 CI 工具，不作为独立发行命令。\n\
         若要检查代码风险，请使用：audit-risk check <workspace>"
    ))
}
```

---

## 清理（4 个命令移植完成后执行）

1. **删除** `fn run_phase5_secondary_command()`（约第 2124-2207 行）
2. **删除** `fn resolve_phase5_script_path()`（约第 3068-3079 行）
3. **删除** `fn resolve_platform_root()`（如果只有这两个函数使用它）
4. 检查 `struct SecondaryArgs` 是否还有其他使用者——若无，也删除

---

## 验收测试（新增在 `engine/tests/` 下）

### `tests/native_commands.rs`

```rust
// audit：无 query 时返回最近记录
#[test]
fn audit_command_returns_jsonl_records_without_query() { ... }

// audit：有 query 时过滤
#[test]
fn audit_command_filters_by_query_string() { ... }

// rules：返回 review 和 repair 两个 plane 的摘要
#[test]
fn rules_command_returns_two_plane_summaries() { ... }

// report：写入 JSON 文件且文件可解析
#[test]
fn report_command_writes_parseable_json_file() { ... }

// report：fail_on=error 且 decision=block 时 exit_code=2
#[test]
fn report_command_exits_2_when_fail_gate_triggered() { ... }

// verify：不调用 node，退出码为 1，输出含 "audit-risk check"
#[test]
fn verify_command_returns_deprecation_message_without_node() { ... }
```

---

## 完成标准

```sh
# 这两条在没有 node / npm / tsx 的环境下必须正常返回 JSON：
audit-risk audit   <workspace> --query review --limit 5
audit-risk rules   <workspace>

# 这条必须写出文件并返回 JSON：
audit-risk report  <workspace> --output /tmp/report.json

# 这条必须以退出码 1 结束，不报 "command not found: node"：
audit-risk verify  <workspace>; echo "exit: $?"

# 全部测试通过：
cargo test
cargo +1.97.0 clippy --all-targets -- -D warnings
```
