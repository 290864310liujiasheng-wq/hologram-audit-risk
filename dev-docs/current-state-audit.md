# 当前状态审计

生成日期：2026-06-20

## Existing Truth Inventory

This section records the current truth for the adopted repo.

本仓库是半路接管项目：已有 HoloGram 代码图谱与桌面 IDE 基座，当前产品方向已切换为“AI 编码风控平台”。本阶段目标是建立内部真源和架构边界，不进行业务代码改造。

## 已确认事实

- 根目录存在 `AGENTS.md`，本次已改写为当前 agent 宪法。
- 根 `README.md` 仍是 HoloGram 外部介绍，包含 3D 星图、MCP、安装和截图叙事。
- `docs/DATA_FLOW_ARCHITECTURE.md` 仍是 HoloGram 数据流说明，可作为现有基座证据，不作为新产品边界。
- `engine/` 提供 Rust 代码图谱、静态分析、路由/耦合/数据流等候选 evidence 能力。
- `src-ui/src/provider/` 已有 Anthropic/OpenAI-compatible provider 抽象。
- `src-ui/src/agent/` 已有 Agent 循环、工具注册、权限门禁、hooks、memory、logger。
- `src-tauri/src/audit.rs` 已有 append-only JSONL 审计日志雏形。
- 当前 git 状态在本次写作前已有未跟踪 `src-ui/.npm-cache/`，与本任务无关。

## Product Boundary

- 旧公开 README 把项目讲成 HoloGram 代码星图工具；当前内部真源把代码图谱限定为风控 evidence plane。
- 旧工具描述强调 Agent 与图谱互动；当前主线要求 Agent 服务于风险审查、解释、拦截和审计。
- 旧环境示例包含平台 provider 字段；当前产品边界要求客户自带模型 API，密钥不得写入代码或文档。

## 当前采用边界

- 采用：深色 IDE 工作台、代码图谱分析、本地凭证/审计、Provider 抽象、权限门禁。
- 暂不采用：把 HoloGram 作为产品名或最终叙事、把 3D 图谱作为唯一核心卖点、把模型 API 做成平台统一供应。
- 暂不迁移：`README.md`、`docs/`、旧截图和安装说明。迁移它们属于公开产品语义变更，后续单独确认。

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

下一步安全任务：

- 下一阶段若继续前进，重点转到规则精度、live provider 降级体验和真实修复提案质量，而不是回头重做本阶段 owner/UI/E2E 主干。
