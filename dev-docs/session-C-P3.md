# 会话C：P3-14 + P3-15 + P3-16（小瑕疵）

**目标文件：`engine/src/cli.rs`**  
**独立任务，可与会话A/B同时运行**  
**三条都很小，合成一个commit即可**

---

## P3-14：耦合类 findings 的 `location.file_path` 是空串

**问题**：l3（耦合）类型的 finding 里 `location.file_path` 是 `""`，IDE/CI 里无法点击跳转。

**定位**：在 `cli.rs` 里搜索构建 finding 的地方，找到 l3/coupling 类型的 location 构建逻辑：
```bash
grep -n "l3\|coupling\|file_path.*empty\|location.*{" engine/src/cli.rs | head -20
```

**修复**：l3 finding 通常跨多个文件（社区/耦合），把**最重要的那个文件**（社区中节点数最多的文件，或耦合强度最高的文件）填入 `file_path`。  
若无法确定单一文件，把社区里所有文件拼成逗号分隔串，或填第一个文件。至少不能是空串。

**验收**：`audit-risk check <workspace> --json` 输出里，l3 类型 finding 的 `location.file_path` 不为空串。

---

## P3-15：多余位置参数被静默忽略

**问题**：`audit-risk check mydir extra_arg` 不报错，`extra_arg` 被无声忽略，掩盖打字错误。

**定位**：在 `cli.rs` 的命令解析处找 `check` 子命令的 positional 参数处理，确认是否有 unknown positional 检测。

**修复**：解析完所有已知参数后，若还有剩余位置参数，报用法错误：
```rust
// 在各子命令解析末尾加：
if let Some(unknown) = rest.iter().find(|arg| !arg.starts_with("--")) {
    return Err(UsageError::new(format!(
        "未知参数 `{unknown}`。用法：audit-risk check <workspace> [--json] [--fail-on <level>]"
    )));
}
```

**验收**：
```bash
audit-risk check . extra_arg   # 应报错，退出码非0
audit-risk check . --json      # 应正常运行
```

---

## P3-16：损坏的 repair plan JSON 给出误导性错误

**问题**：repair plan 文件里是坏 JSON 时，报错说"方案已过期或 plan_id 有误"，实际原因是 JSON 解析失败。

**定位**：`engine/src/cli.rs`，`load_repair_plan` 函数（在 P0-2 的代码里），找到 JSON 解析失败的错误处理：
```rust
// 目前
.map_err(|error| CliRuntimeError::environment(format!("找不到修复方案 {plan_id}。方案可能已过期（10 分钟有效）或 plan_id 有误。")))
// 应改为区分两种情况：文件不存在 vs JSON损坏
```

**修复**：区分错误类型：
```rust
let raw = fs::read_to_string(&plan_path)
    .map_err(|_| CliRuntimeError::environment(format!(
        "找不到修复方案 {plan_id}。方案可能已过期（10 分钟有效）或 plan_id 有误。"
    )))?;
let plan: RepairPlanDocument = serde_json::from_str(&raw)
    .map_err(|error| CliRuntimeError::environment(format!(
        "修复方案 {plan_id} 的文件内容损坏（无法解析 JSON）：{error}。请重新运行 `audit-risk repair plan` 生成新方案。"
    )))?;
```

**验收**：
```bash
# 写入损坏 JSON
echo '{bad json' > .hologram/repair-plans/test-plan.json
audit-risk repair apply . --plan test-plan
# 错误信息应包含"损坏"或"无法解析"，而不是"已过期或 plan_id 有误"
```

---

## 执行顺序

三条都很简单，建议顺序：P3-16 → P3-15 → P3-14（从最简单到稍复杂）。

## 验证

```bash
cargo test
cargo +1.97.0 clippy --all-targets -- -D warnings
```

## 提交信息

```
fix(cli): fix empty file_path in l3 findings, reject unknown positional args, clarify corrupt plan error
```

---

## 执行结果（2026-07-10）

- P3-14：已由 `run_full_check` 向 L3 signal owner 传入 workspace root；owner 会去掉 `Node.location` 的数值行号、转换为工作区相对路径并与 changed file 精确匹配，再写入 finding。`cli.rs::derive_findings` 保持薄映射，不增加空路径 fallback；测试覆盖最终 finding、`file:line` 和同名路径碰撞。
- P3-15：已在 `check` 命令解析边界拒绝第 2 个位置参数，并通过真实 `audit-risk` 二进制测试校验非零退出码、未知参数和用法提示。
- P3-16：已在 `load_repair_plan` 区分 JSON 语法损坏与合法 JSON 的结构不合法错误，均提示重新生成方案；集成测试校验错误不再落到过期或 plan id 误导口径。
- 验证：`cargo test` 全绿；`cargo +1.97.0 clippy --all-targets -- -D warnings` 通过。
