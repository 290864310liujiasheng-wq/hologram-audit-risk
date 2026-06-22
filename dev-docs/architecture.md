# 架构边界

## Decision

The single recommended architecture is：本地 IDE 工作台 + 代码证据引擎 + 风控审查核心 + 客户自带模型 provider + 权限/审计平面 + 可选多代理编排。

```text
Workspace / Git Diff / AI Tool Event
  -> Evidence Collector
  -> Rule + Static Signal Engine
  -> Model Review Provider
  -> Risk Aggregator
  -> Gate Decision
  -> Audit Trail
  -> UI Workbench / CLI / Future API
```

这条主线匹配当前仓库事实：已有 Rust 代码图谱引擎、Tauri 本地壳、前端 Agent 循环、Provider 抽象、权限门禁和审计日志。后续应把这些能力收束到“编码风控”产品语义，而不是继续扩散成通用图谱展示。

## Owner Layers

| 层 | 职责 | 当前候选位置 |
| --- | --- | --- |
| Evidence Plane | 代码图谱、diff、文件片段、依赖影响、静态信号 | `engine/`、`src-tauri/src/workspace.rs` |
| Review Core | 审查任务、finding、规则命中、风险聚合、拦截决策 | `src-ui/src/risk/` 起步，合同见 `dev-docs/contracts.md` |
| Provider Plane | 客户模型 API、base URL、模型名、流式输出、用量 | `src-ui/src/provider/`，后续可下沉到共享合同 |
| Agent Plane | 工具调用、多轮审查、多代理并行、主代理汇总 | `src-ui/src/agent/` 起步，核心语义不得只留在 prompt |
| Policy Plane | 规则包、严重级别、审批、禁止/询问/允许策略 | `src-ui/src/agent/permission.ts` 起步，需产品化合同 |
| Audit Plane | append-only 审计事件、证据引用、决策记录 | `src-tauri/src/audit.rs` 起步 |
| UI Plane | 深色 IDE 工作台，展示风险、解释、证据、审批 | `src-ui/src/ui/` |

## Forbidden Paths

- Do not 让 UI 文案拥有风险判定语义。
- Do not 让 prompt 拥有唯一规则定义。
- Do not 让 provider 适配器决定业务拦截。
- Do not 让 Tauri 命令私自定义 finding、severity、gate decision。
- Do not 让临时脚本成为审计或规则真源。

## 核心流程

### 1. 开发中审查

```text
文件变化 / AI 工具输出 / Git diff
  -> 生成 ReviewJob
  -> 收集 CodeEvidence
  -> 执行 RuleSet 与静态信号
  -> 调用客户模型生成 ReviewFinding 解释
  -> RiskAggregator 汇总去重
  -> GateDecision 放行、提醒、要求审批或阻断
  -> AuditTrail 记录
```

### 2. 多代理并行审计

```text
ReviewJob
  -> StaticAgent
  -> SecurityAgent
  -> ArchitectureAgent
  -> TestRegressionAgent
  -> DependencyAgent
  -> LeadReviewer 汇总
  -> GateDecision
```

并行代理只能产出候选 finding 和证据引用；主智能体负责去重、冲突裁决、严重级别校准和最终建议。

### 3. 受控自修复

```text
Finding
  -> RepairPlan
  -> PatchProposal
  -> User/Policy Approval
  -> Apply-time Rule Re-check
  -> Apply-time Test Gate
  -> Apply Patch
  -> AuditTrail
```

自动修复默认不静默落盘。没有修复计划、diff、审批记录，以及 apply-time 的规则复检 / 测试结果时，不允许进入 apply。

## 不中断客户使用

- 审查任务应可取消、可超时、可降级。
- 模型失败不应导致 IDE 工作台不可用。
- 高风险动作才阻断；普通风险默认给解释和建议。
- 审计写入失败不能假装成功，必须产生可见 degraded 状态。
- 后续若接入 CI 或 Git hooks，应提供旁路模式和明确超时策略。

## 与旧 HoloGram 语义的关系

- “代码星图”是 Evidence Plane 的一种展示和分析输入，不是产品主叙事。
- “Agent 工具”是审查执行和解释通道，不是泛聊天卖点。
- “约束门禁”应升级为规则/策略/gate decision 合同。
- `README.md` 和 `docs/DATA_FLOW_ARCHITECTURE.md` 暂不迁移，作为现有基座说明和历史外部文档。

## 停止条件

架构边界被接受的条件：

- 新功能能明确归属到一个 owner 层。
- 核心风险语义能在 `contracts.md` 找到对应合同。
- UI、prompt、provider、Tauri adapter 不再私自定义业务规则。
- 任一审查结论都能追溯到 evidence、rule/model、decision、audit event。

## Implementation Notes

- 2026-06-20：第一段 Review Core 纯 TypeScript owner 已落在 `src-ui/src/risk/`。
- 当前已实现：`ReviewJobRequest`、`ReviewJobResult`、`ReviewFinding`、`Rule`、`GateDecision`、`AuditEvent`、`ContractError` 等合同类型；`validateReviewJobRequest`；`validateRule`；`validateReviewFinding`；`deriveGateDecision`；`finalizeReviewJobResult`；`createAuditEvent`。
- 当前已实现桥接层：`src-ui/src/risk/check-adapter.ts`，用于把现有 HoloGram `CheckResult`/`Violation` 映射成 `ReviewFinding`。
- 当前已实现 UI 读接线：`src-ui/src/ui/check.ts` 会消费 `check-adapter` 的摘要模型，在简报面板内渲染 `风控摘要` 区块。
- 当前已实现审计落盘接线：`src-ui/src/risk/audit-bridge.ts` 生成 review audit payload，`Workspace.runCheck()` 会调用 Tauri `audit_append_review` 追加到 `.hologram/audit.jsonl`。
- 当前已实现审批写路径：`Workspace` 的 approver 回调会为 `approval_requested / approval_resolved` 写审计并写入时间轴。
- 当前已实现 Agent 审计消费：新增 `audit_recent_reviews`、`current_review_summary` 与 `active_provider_readiness` 只读工具。
- 当前已实现强运行态信号：窗口标题、状态栏、`CheckPanel` 均会暴露风险与审计计数。
- 当前已实现多代理 owner：`src-ui/src/risk/multi-agent.ts` 负责 specialist agent runs、去重、冲突和 degraded reason 聚合。
- 当前已实现自修复 owner：`src-ui/src/risk/self-heal.ts` 负责 repair plan、patch proposal、审批状态、apply/rollback 纯逻辑。
- 当前已实现 repair preflight owner：`src-ui/src/risk/rule-package.ts` 与 `self-heal.ts` 会在 apply 前校验 patch scope、敏感路径、重复写、波及面，并强制执行 `required_tests`。
- 当前已实现 repair degrade owner：`self-heal.ts` 会把 live provider / timeout / source-context 缺失归一成 `RepairIssue`；`current-review.ts` 负责把 issue 收口进工作台读模型；`Workspace` 只负责 audit 和状态适配。
- 当前已实现 current review owner：`src-ui/src/risk/current-review.ts` 负责把 check 结果收口为 summary / multi-agent / repair plan 的单一读模型。
- 当前已实现 UI/controller 接线：`Workspace` 拥有 current review state 与 repair state；`CheckPanel` 只展示并发出 repair 事件，不拥有风控/修复语义。
- 当前仍待后续补强：live provider 生成 patch proposal 的真实运行态证据、真实模型输出上的 proposal 业务语义证明，以及 provider 代理重写/证书链路径的更多真实报错样本。
