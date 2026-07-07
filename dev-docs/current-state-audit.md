# 当前状态审计

生成日期：2026-06-22

## Existing Truth Inventory

This section records the current truth for the adopted repo.

本仓库是半路接管项目：已有 HoloGram 代码图谱与桌面 IDE 基座，当前产品方向已切换为“AI 编码风控平台”。当前主线已从第四阶段产品化继续推进到第五阶段交付化，重点是补外部接入、headless report、CI/hook 与管理员导出，而不是回头重做前三四阶段 owner / contract / runtime 主干。

## 已确认事实

- 根目录存在 `AGENTS.md`，本次已改写为当前 agent 宪法。
- 根 `README.md` 已切到 AI 编码风控平台对外口径，主产品叙事为 `audit-risk` CLI，旧 HoloGram 语义只保留为历史基座说明。
- `docs/DATA_FLOW_ARCHITECTURE.md` 仍是 HoloGram 数据流说明，可作为现有基座证据，不作为新产品边界。
- `engine/` 提供 Rust 代码图谱、静态分析、路由/耦合/数据流等候选 evidence 能力。
- `src-ui/src/provider/` 已有 Anthropic/OpenAI-compatible provider 抽象。
- `src-ui/src/agent/` 已有 Agent 循环、工具注册、权限门禁、hooks、memory、logger。
- `src-tauri/src/audit.rs` 已有 append-only JSONL 审计日志雏形。
- `src-ui/src/risk/rule-package.ts` 现已具备默认 review / repair rule package、扩展包合并、禁用 rule 与 policy snapshot 生成的统一 registry。
- `src-ui/src/risk/audit-bridge.ts` 现已具备统一 `AuditQueryResult` / `AuditRecord` 读模型，`workspace.ts` 与 `CheckPanel` 都通过同一 normalized audit truth 读取最近审计。
- `src-ui/src/risk/current-review.ts` 现已具备 `buildWorkbenchQueue`，把看风险 / 看 gate / 看证据 / 审批阻断 / repair rollback 主路径收口成纯读模型，而不是散落在 UI 条件里。
- `engine/src/bin/audit-risk.rs` 现已提供公共 CLI 主入口；`engine/src/bin/hologram-risk-check.rs` 仅保留迁移期兼容壳。
- `src-ui/src/risk/delivery.ts` 现已提供 delivery manifest、workspace rule package 装载、machine report、hook/CI 模板真源。
- `src-ui/scripts/phase5-delivery.ts` 现已退化为交付化兼容/验证壳；公共命令面已收口到 Rust `audit-risk` CLI。
- `current_review_summary` 工具现已直接返回 `review + workbench_queue + repair_history`，只读工具不再只能拿到底层 review object 再自行拼装工作台主路径。
- `src-ui/src/risk/current-review.ts` 现已具备 `buildRepairWorkbenchSnapshot`；`CheckPanel` 的 provider / generation / preflight / evidence trace / repair history 状态已改为直接消费 owner snapshot，不再在 UI 层各自推断。
- repair proposal 在展示给用户前，现已新增 owner 层 `repair_proposal_validation`：会先做二次审计、快速语法检查，并给出固定的逻辑变更提示；若引入新风险或语法非法，会在 proposal_generation 阶段直接阻断展示。
- `audit-risk doctor` 现已具备 CLI 版本、engine 版本、`git/cargo/node` 依赖完整性、rule package `package_id/version`、provider 与 audit 路径状态的一键体检输出。
- `audit-risk watch` 在人类模式下现已补“watching / initial scan running / 首轮摘要”三段首屏反馈，避免启动时看起来无响应。
- `audit-risk check / diff / init / doctor / report / notify --test` 当前默认已切到统一中文产品壳；机器消费若要保持稳定 JSON，必须显式加 `--json`。
- 这轮 task4 审计已确认并修复：对子目录 workspace 运行 `check/watch` 时不会再把父仓库 `../` 变更误带进结果；生成的 `.githooks/pre-commit` 也不会再写出坏掉的平台根路径或错误的 binary 路径。
- `current_review_summary` 工具已显式支持 `limit` 参数，用于控制折叠进 `workbench_queue / repair_history / repair_workbench` 的最近审计记录条数。
- Chrome 实机页在本轮已能刷新到 `🔮 风控4 审计3 — 全息观测站` 标题，说明最新 bundle 已被 localhost 页面加载；页面级 `Review Queue`、`门禁决策`、`多代理审计`、`自修复闭环`、`看证据 · 已就绪` 与 repair/apply 历史文案都已在同一 localhost 页面内出现。
- `phase4:verify` 的 preview smoke 已尝试在脚本内起本地 preview 并抓取 `127.0.0.1:4174`，但当前环境对子进程绑定端口返回 `EPERM`；该失败已作为环境限制写入 evidence，而不是被当成代码失败吞掉。
- 2026-06-23：通过本机 Chrome 实机查看 `http://127.0.0.1:4173/`，已确认 `Review Queue`、`门禁决策`、`多代理审计`、`自修复闭环` 在真实 localhost 页面内可见；其中 `Repair 历史` 与 `Evidence trace` 已通过同一页面滚动视图出现，但尚未额外落独立截图文件。
- 当前 git 状态在本次写作前已有未跟踪 `src-ui/.npm-cache/`，与本任务无关。

## Product Boundary

- 公开 README 已切到 `audit-risk` CLI 叙事；代码图谱被限定为风控 evidence plane，而不是产品主卖点。
- 旧工具描述强调 Agent 与图谱互动；当前主线要求 Agent 服务于风险审查、解释、拦截和审计。
- 旧环境示例包含平台 provider 字段；当前产品边界要求客户自带模型 API，密钥不得写入代码或文档。

## 当前采用边界

- 采用：深色 IDE 工作台、代码图谱分析、本地凭证/审计、Provider 抽象、权限门禁。
- 暂不采用：把 HoloGram 作为产品名或最终叙事、把 3D 图谱作为唯一核心卖点、把模型 API 做成平台统一供应。
- 暂不迁移：旧截图、安装说明和历史 HoloGram 基座文档的全部公开资产。它们仍属于公开产品语义变更，后续单独确认。

## Current Call Chains

- 代码 evidence 候选链：`engine/` 分析源码、依赖和结构信号，后续供 Review Core 消费。
- 模型 provider 候选链：`src-ui/src/provider/` 统一 Anthropic/OpenAI-compatible 请求与流式响应。
- Agent 候选链：`src-ui/src/agent/agent.ts` 驱动模型、工具调用、事件流和权限检查。
- 权限候选链：`src-ui/src/agent/permission.ts` 对工具调用做 allow/ask/deny。
- 审计候选链：`src-tauri/src/audit.rs` 写入 `.hologram/audit.jsonl`。

## Dangerous Adoption Actions

以下动作需要单独确认，不在本次文档任务内执行：

- 迁移或重写公开 `README.md`。
- 删除旧 HoloGram 文档、截图、安装说明或 API。
- 把 `dev-docs/` 声明为唯一外部文档来源。
- 改产品名、公开营销叙事或 UI 文案。
- 删除旧路由、字段、配置或兼容入口。

## 风险

- 若后续 agent 只读 `README.md`，可能继续按 HoloGram 图谱产品推进。
- 若先写 UI，风险语义可能散落在组件、prompt 或 provider 适配器里。
- 若先做自修复，可能绕过规则、审批、测试和审计合同。
- 若保留“兼容旧产品叙事”，会污染客户承诺和架构 owner。

## Handoff State

当前交接状态：

- 已建立根 `AGENTS.md` 和 `dev-docs/` 内部真源。
- 已新增第一段 Review Core 纯 TypeScript 合同核心：`src-ui/src/risk/review-core.ts`。
- 已新增 Review Core 测试：`src-ui/src/risk/review-core.test.ts`。
- 已新增 `src-ui/src/risk/check-adapter.ts` 与对应测试，用于承接现有 HoloGram check 输出。
- 已新增 `CheckPanel -> check-adapter -> ReviewFinding` 的 UI 读路径。
- 已新增 `src-ui/src/risk/audit-bridge.ts`、Tauri `audit_append_review` 命令，以及 `Workspace.runCheck()` 的审计落盘接线。
- 已新增审批写路径：允许/拒绝会进入 timeline 与 audit jsonl。
- 已新增 Agent 读取路径：可读当前 review summary 与最近审计。
- 已取得强运行态证据：Chrome 真实窗口标题显示 `风控4 审计2`，状态栏显示 `风控4 · 审计2`，简报面板可见 `最近审计` 与 `风控摘要`。
- 已新增多代理 owner：`src-ui/src/risk/multi-agent.ts` 与对应测试，支持 specialist runs、去重、冲突记录和 degraded reasons。
- 已新增自修复 owner：`src-ui/src/risk/self-heal.ts` 与对应测试，支持 repair plan、patch proposal 解析/生成、审批状态流转、apply 与 rollback。
- 已新增 repair preflight owner：`src-ui/src/risk/rule-package.ts` 与 `self-heal.ts` 的 apply-time gate，支持默认 repair 规则包、patch scope 复检与 `required_tests` 强制执行。
- 已新增 current review gate owner：`src-ui/src/risk/rule-package.ts` 已补默认 review 规则包，`current-review.ts` 会把 `check.l5/l4/l3/l2` 收口成结构化 `gate_decision`，`CheckPanel` 已展示门禁决策区块。
- 已新增 repair degrade owner：provider 层已补 `ProviderRequestError` / `classifyProviderFailure`；`self-heal.ts` 会在此基础上把 live provider 失败归一成 `RepairIssue`，当前至少区分 key/auth/rate-limit/timeout/upstream-5xx/network/TLS/proxy/source-context，并进一步区分 407 代理拒绝、证书吊销与 `ECONNRESET`/socket hang up 等连接中断；`current-review.ts` 与 `CheckPanel` 会暴露可见降级状态，`Workspace` 会把失败写入 repair audit。
- 已新增 repair generation evidence owner：`self-heal.ts` 会产出 `RepairGenerationMetadata`；`current-review.ts`、`CheckPanel` 与 repair audit 都可读取 provider/model/file_count/high-severity focus；proposal generation degrade 的 audit 还会额外保留 `error_stage` 与 `error_retryable`。
- 已新增 preflight failure summary owner：`buildRepairPreflightSummary` 会收口 `gate_reason`、失败命令、阻断 rule；`Workspace` 在 preflight 阻断时会把这些证据写进 repair audit。
- 已新增 preflight failure owner：`self-heal.ts` 会在 apply 前阻断时抛出结构化 `RepairApplyError`，`current-review.ts` 会把 preflight issue 收口到当前 review state，`Workspace` 会把 `gate_reason / validation_results / preflight_findings` 写入 repair audit。
- 已新增 apply execution failure owner：`self-heal.ts` 会在部分写入失败时自动回滚并抛出 `RepairApplyExecutionError`；`current-review.ts` 与 `CheckPanel` 会保留 rollback evidence，`Workspace` 会把执行失败写入 repair audit。
- 已新增 macOS provider secret owner：`src-tauri/src/credential.rs` 现已支持通过 macOS Keychain 存取 provider key，并用 manifest 记录已持久化 provider，避免桌面壳在 macOS 上永远无法恢复 live provider 凭证。
- 已新增 provider readiness owner：前端可通过 `active_provider_readiness` 查询当前 active provider 是否能跑 live repair planner；后端以 `credential_has` 提供只读存在性检查，不再把“没有 key”与“恢复链断裂”混在一起；`current-review.ts` 与 `CheckPanel` 也会直接暴露 readiness/source/reason。
- 已新增 live repair readiness owner：系统会显式区分“provider key 已就绪”与“当前会话仍处于 browser mock / mock workspace，因此不具备真实 repair 证据资格”；避免把聊天可用误判成 phase3 live repair 已就绪。
- 已新增 repair generation readiness owner：`runCheck` 后就会计算当前 review 是否具备自动修复输入；当 finding 数为 0 或没有可读源码文件时，`CheckPanel` 会显示空状态/阻断原因，并直接禁用 proposal 按钮，不再把这类场景误报成 provider 生成失败。
- 已新增 non-code repair test gate owner：`self-heal.ts` 现会为 config / migration / serialization 等非代码高风险 repair 自动补 `git diff --check`，避免 critical repair 因 `required_tests` 为空而永远无法形成通过样本。
- 已新增 repair file fallback owner：当结构性 finding 本身没有稳定 file path 时，repair planner 会回退到当前 `changed_files` 作为候选文件输入，尽量把可修复风险收口到真实源码文件，而不是直接退化成 `0 files`。
- 已新增 check changed-files fallback owner：`hologram_run_check` 在 watcher 没有提供 `LAST_CHANGED_FILES` 时，会回退读取当前 git 工作区的变更文件，避免工作台在真实 repo 上把“明明有未提交修改”误判成“无新变更”。
- 已新增 patch proposal path guard：`self-heal.ts` 会校验 live model 返回的文件路径必须属于本轮提供的 repair files，防止提案越权扩张到未提供文件。
- 已新增 patch proposal content guard：`self-heal.ts` 会拒绝 no-op rewrite，并要求提案覆盖全部高风险 finding 文件、实际触达其行范围，且高风险行上的改动不能只是空白/格式变化或纯注释改写；直接删除高风险行则被视为合法修复，减少“看起来生成了 patch，实际上只修了边角料”的情况。
- 已新增 patch proposal narrative guard：`self-heal.ts` 会拒绝 `summary` / `rationale` / `operation.summary` 退化成 `fix` / `todo` / `update` 这类占位文本，减少“表面生成了提案，但没有解释自己到底修了什么”的伪语义 proposal。
- 已新增 finding 合同精度校验：`review-core.ts` 现会拒绝非法行号范围，以及把 `plain_explanation` 退化成规则编号/标识符的伪解释。
- 已新增 finding 合同精度校验：`review-core.ts` 现会拒绝非法行号范围，以及把 `plain_explanation`、`impact`、`recommendation` 退化成规则编号/短 token 的伪解释。
- 已新增 gate decision 合同精度校验：`review-core.ts` 现会拒绝缺少 `subject_ref`、`policy_snapshot_id`、`reason` 或 `finding_ids` 的阻断/审批决策。
- 已新增 current review owner：`src-ui/src/risk/current-review.ts`，把 `CheckResult` 收口成 findings、multi-agent review、repair plan 的统一派生对象。
- 已新增产品化 UI 接线：`Workspace` 会维护当前 review state，`CheckPanel` 会渲染 `多代理审计` 与 `自修复闭环` 区块，并通过事件总线触发生成提案 / 审批 / apply / rollback 路径。
- 已新增 repair audit 可见性：`summarizeRecentAuditEntries` 现会展示 `repair_*` 事件，mock 工作台最近审计里可见 repair trail。
- 已确认桌面壳可启动：`cargo run` 已成功拉起 Tauri debug 窗口，不只是 `cargo check` 通过。
- 已生成真实 repo graph/cache：repo 根目录已落 `hologram_graph.json`（`5017` 节点 / `9525` 边）与 `.last_project`。
- 已取得真实 repo workspace 审计证据：`repo/.hologram/audit.jsonl` 已新增 `review_check` 记录，内容为 `Review check passed without blocking findings.`，路径指向当前 repo。
- 已取得真实桌面态 UI 证据：前台 `全息观测站` 窗口已显示当前 repo 的绝对路径节点标签，底部状态显示 `风控0 · 审计1` 与 `简报已更新 · 风险0 · 审计已记录`。
- 已补齐 `rule-taxonomy.md`、`ui-truth.md`、`multi-agent-orchestration.md`、`self-healing-policy.md`。
- 未迁移公开 README/docs。
- `src-ui/.npm-cache/` 已清理；`src-ui/node_modules` 和 `src-ui/dist` 为依赖/构建产物，未纳入 git 变更。
- 已取得第三阶段 fresh 证据文件：`dev-docs/evidence/phase3-runtime-samples.json` 已落盘，覆盖 live provider 成功样本、2 组 critical semantic repair 样本、1 组 preflight 阻断样本与 provider 边界矩阵。
- 已取得真实 live provider 成功样本：当前机器 shell 环境变量仍无 `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` / `DEEPSEEK_API_KEY`，但 macOS Keychain 下存在可恢复的 `deepseek` 凭证；本轮已通过 `deepseek / deepseek-v4-pro` 真实生成 `config.yaml` repair proposal，并让 proposal 进入 `current review`、`CheckPanel` 与 repair audit。
- 已取得 proposal 业务语义修复证明：以 `engine.run_full_check` 真实产出的 `config.yaml` 与 `migrations/0001_init.sql` 两组 L5 finding 为输入，repair apply 后对应 git diff 归零，re-check 从 `1 finding` 下降到 `0`，`one_line` 返回 `无新变更`。
- 已取得 preflight allow/block 闭环样本：allow 样本中 `git diff --check` 通过、`repair_apply` gate 为 `allow` 且 patch 成功落盘；block 样本中当前 review / CheckPanel / repair audit 同步暴露 `repair.test.required_command_failed`、失败命令 `git diff --check` 和 gate reason `修复前验证命令必须全部通过`。
- 已补 provider 边界矩阵：`auth_invalid`、`rate_limit`、`timeout`、`upstream_5xx`、`proxy_407`、`tls_handshake_failed`、`tls_cert_revoked`、`connection_reset`、`socket_hang_up` 均已有稳定复现样本，并在 `current_review` / `CheckPanel` / repair audit 三处维持同码值口径。
- 当前仍缺但不再阻断第三阶段收口：`anthropic` 真实 live proposal 成功样本与更多真实上游故障 trace；当前已由 `deepseek` 真样本和稳定复现样本覆盖阶段验收面。
- 2026-07-07：已新增 `vscode-extension` 的 audit-risk findings 侧边栏面板 owner：`vscode-extension/src/findingsTreeProvider.ts` 把 review findings 按严重程度分组渲染，展示 gate 决策，并支持点击 finding 跳转到 `file:line`（默认展开）；`extension.ts` 注册 `auditRisk.findings` 树视图与 `auditRisk.openFinding` 命令，`package.json` 注册 explorer 视图；已补 6 个集成测试（`vscode-extension/src/test/suite/extension.test.ts`）全部通过，`tsc -p ./` 编译通过。此前用于截图验证的临时 `AUDIT_RISK_AUTO_CHECK_ON_ACTIVATE` 代码已在提交前删除。改动已落 commit `91b6e0b`。
- 2026-07-07：`.mcp.json` 含本机绝对路径，已从 git 移除跟踪并加入 `.gitignore`（保留本地文件），改为提供 `.mcp.json.example` 占位模板，避免商业化交付时泄露本机路径或对客户失效。
- 2026-07-07：vscode-extension P0/P1/P2/P3 全面收口。P0：`auditRisk.clear` 补入 `package.json contributes.commands`（此前只注册了代码命令但未声明，命令面板看不到）。P3：`auditRisk.openFinding` 同步声明并通过 `commandPalette when:false` 隐藏（树点击内部命令）；`auditRisk.repair` 声明并隐藏。P1：README "当前能力" 去掉"第一阶段"标签，补充 inline hover 和侧边栏能力描述，后续方向去重已完成项。P2：新增 `audit-risk repair plan / repair apply` 两个 CLI 命令（Rust 侧 `engine/src/cli.rs`），从 `delivery.json` 读 provider 配置，通过 curl 调用用户自带模型 API 生成最小化 patch proposal，保存到 `.hologram/repair-plans/`（10 分钟过期），apply 时做 preflight gate（`required_tests`）、逐行写文件、失败自动回滚、全程写 repair audit 事件；VS Code 扩展新增 `RepairCodeActionProvider`（`vscode-extension/src/repairCodeActionProvider.ts`），给 critical/high/medium 级别的 finding 挂灯泡 QuickFix，点击 → 生成方案 → 弹窗确认 → 预览 diff → 调 repair apply → 刷新诊断；已补 7 个 Rust 解析测试和 2 个扩展集成测试，全部通过；`tsc` 编译零错误，`cargo test` 385 个测试全部通过。

下一步安全任务：

- 第四阶段四个面已形成当前主线闭环；后续若继续推进，应优先做增量 polish、真实 provider 扩样本、以及非阻断的验证自动化增强，而不是回头重做第三阶段 owner / contract / runtime 主干。
