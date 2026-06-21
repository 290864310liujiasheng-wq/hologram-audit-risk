# 合同草案

本文件定义当前产品主线的设计合同。它不是已实现 API 清单；实现前必须先把这些合同落到对应 owner 层并补测试。

## Implementation Status

- 2026-06-20：`src-ui/src/risk/review-core.ts` 已实现第一段纯合同核心。
- 已覆盖：`ReviewJobRequest` 基本校验；`Rule` 基本校验；finding 缺 evidence 校验；confidence 范围校验；source location 非空校验；enabled rules 聚合 `GateDecision`；`ReviewJobResult` 的 `completed/degraded/blocked` 基线收口；`AuditEvent` 的基础构造。
- 已覆盖桥接：`src-ui/src/risk/check-adapter.ts` 可把现有 `CheckResult` 违规分桶映射成 `ReviewFinding`，并给出 severity 统计。
- 已覆盖 UI 读模型：`buildCheckRiskSummary` 可生成 `CheckPanel` 使用的风险摘要视图模型。
- 已覆盖审计桥接：`src-ui/src/risk/audit-bridge.ts` 可把 `CheckResult + ReviewFinding[]` 生成 review audit payload；Tauri `audit_append_review` 可接收并落盘。
- 已覆盖审批桥接：approval allow/deny 可转成结构化 audit payload，并写入 timeline + audit jsonl。
- 已覆盖 Agent 读取：Agent 可通过 `audit_recent_reviews` 与 `current_review_summary` 读取新对象。
- 已覆盖强运行态信号：标题、状态栏、简报面板均可显示风险/审计状态。
- 已覆盖多代理合同：`AgentRun`、`ReviewAggregation`、`AggregationConflict` 已进入 `review-core.ts`；`multi-agent.ts` 已实现 specialist fan-out、去重、冲突记录、degraded reason。
- 已覆盖自修复合同：`RepairPlan`、`PatchProposal`、`PatchOperation`、`RepairRollbackSnapshot` 已进入 `review-core.ts`；`self-heal.ts` 已实现 plan/proposal/apply/rollback 状态流转。
- 已覆盖 current review 收口：`current-review.ts` 可把 `CheckResult` 派生为 findings、multi-agent review 与 repair plan。
- 部分未覆盖：真实 live provider 生成 patch proposal 的运行态证据、apply 前的真实 rule re-check / test gate 编排、Provider 失败降级 UI 呈现、finding 行号合法性与白话解释质量校验、gate decision 细粒度合法性校验。

## ReviewJob Contract

```ts
type ReviewMode = 'live' | 'pre_commit' | 'manual' | 'ci' | 'repair_validation';
type ReviewStatus = 'queued' | 'running' | 'degraded' | 'completed' | 'blocked' | 'cancelled' | 'failed';

interface ReviewJobRequest {
  workspace_id: string;
  change_id: string;
  mode: ReviewMode;
  requested_agents?: string[];
  policy_profile_id: string;
  provider_profile_id: string;
}

interface ReviewJobResult {
  job_id: string;
  status: ReviewStatus;
  findings: ReviewFinding[];
  gate_decision?: GateDecision;
  audit_event_ids: string[];
  degraded_reasons?: string[];
}
```

## Finding Contract

```ts
type Severity = 'info' | 'low' | 'medium' | 'high' | 'critical';
type FindingStatus = 'open' | 'accepted' | 'dismissed' | 'fixed' | 'suppressed';

interface ReviewFinding {
  finding_id: string;
  job_id: string;
  rule_id: string;
  severity: Severity;
  category: string;
  locations: SourceLocation[];
  plain_explanation: string;
  impact: string;
  recommendation: string;
  evidence_ids: string[];
  model_trace_id?: string;
  confidence: number;
  status: FindingStatus;
}

interface SourceLocation {
  file_path: string;
  start_line: number;
  end_line: number;
  symbol?: string;
}
```

要求：

- `plain_explanation` 必须是白话解释，不能只输出规则编号。
- `locations` 必须尽量精确到行；跨文件风险可包含多个位置。
- `evidence_ids` 不能为空。
- `confidence` 范围为 `0..1`。

## Rule Contract

```ts
type GateEffect = 'observe' | 'warn' | 'require_approval' | 'block';

interface Rule {
  rule_id: string;
  name: string;
  category: string;
  severity: Severity;
  scope: string[];
  trigger: RuleTrigger;
  gate_effect: GateEffect;
  explanation_template?: string;
  enabled: boolean;
}

interface RuleTrigger {
  kind: 'static_signal' | 'diff_pattern' | 'dependency_impact' | 'permission' | 'model_review';
  config: Record<string, unknown>;
}
```

要求：

- 规则定义是风控真源之一，不能只写在 prompt 或 UI 文案里。
- `gate_effect=block` 的规则必须能解释原因并产出审计事件。
- 后续应扩展 `rule-taxonomy.md`，固定分类和严重级别口径。

## GateDecision Contract

```ts
type GateSubjectType = 'tool_call' | 'file_write' | 'git_commit' | 'repair_apply' | 'release';
type GateDecisionValue = 'allow' | 'warn' | 'require_approval' | 'block';

interface GateDecision {
  decision_id: string;
  job_id: string;
  subject_type: GateSubjectType;
  subject_ref: string;
  decision: GateDecisionValue;
  reason: string;
  finding_ids: string[];
  policy_snapshot_id: string;
  decided_at: string;
}
```

要求：

- `block` 和 `require_approval` 必须进入审计。
- `allow` 不代表无风险；可能代表风险低于当前策略门槛。
- UI 只能展示和请求审批，不能绕过 decision。

## AuditEvent Contract

```ts
interface AuditEvent {
  event_id: string;
  workspace_id: string;
  actor: string;
  event_type:
    | 'review_started'
    | 'finding_created'
    | 'gate_decided'
    | 'approval_requested'
    | 'approval_resolved'
    | 'repair_planned'
    | 'repair_applied'
    | 'repair_rolled_back';
  subject_ref: string;
  decision_id?: string;
  evidence_ids: string[];
  timestamp: string;
  integrity_hash?: string;
}
```

要求：

- 审计是 append-only。
- 不记录密钥、完整敏感源码或无关个人信息。
- 如果只记录摘要，必须保留可追溯的 evidence 引用。

## ProviderProfile Contract

```ts
type ProviderKind = 'anthropic' | 'openai_compatible' | 'local_gateway';

interface ProviderProfile {
  provider_profile_id: string;
  kind: ProviderKind;
  display_name: string;
  base_url: string;
  model: string;
  secret_ref: string;
  timeout_ms: number;
  rate_limit?: {
    requests_per_minute?: number;
    tokens_per_minute?: number;
  };
}
```

要求：

- 客户自带模型 API，平台不硬编码统一 key。
- `secret_ref` 指向系统密钥存储、环境变量或客户网关，不包含明文 key。
- Provider 失败只能让审查 degraded 或 failed，不能让工作台整体不可用。

## Multi-Agent Contract

```ts
interface AgentRun {
  agent_run_id: string;
  job_id: string;
  agent_type: string;
  status: ReviewStatus;
  input_evidence_ids: string[];
  finding_ids: string[];
  started_at: string;
  completed_at?: string;
  error?: string;
}

interface ReviewAggregation {
  job_id: string;
  lead_agent_run_id: string;
  merged_finding_ids: string[];
  dropped_duplicates: string[];
  conflicts: AggregationConflict[];
}

interface AggregationConflict {
  finding_ids: string[];
  reason: string;
  resolution: string;
}
```

要求：

- 子代理只产出候选 finding、证据和建议。
- 主智能体负责汇总、去重、冲突裁决和最终风险口径。
- 子代理超时应产生 degraded reason，不应无限阻塞客户操作。

## RepairPlan Contract

```ts
interface RepairPlan {
  repair_plan_id: string;
  finding_ids: string[];
  strategy: string;
  patch_proposal_ref: string;
  required_tests: string[];
  risk_note: string;
  approval_state: 'draft' | 'waiting_approval' | 'approved' | 'rejected' | 'applied' | 'rolled_back';
}
```

要求：

- 没有 `approved` 不得 apply。
- apply 前必须重跑相关规则和测试。
- apply 后必须写审计事件；失败时必须保留失败原因和回滚建议。

## 错误模型

```ts
interface ContractError {
  code:
    | 'invalid_request'
    | 'missing_evidence'
    | 'provider_unavailable'
    | 'policy_blocked'
    | 'approval_required'
    | 'audit_write_failed'
    | 'timeout'
    | 'internal_error';
  message: string;
  retryable: boolean;
  evidence_ids?: string[];
}
```

要求：

- 缺 evidence 不能伪造成成功 finding。
- 审计失败不能静默吞掉；至少返回 degraded 或 failed。
- 权限阻断应返回结构化原因，供 UI 白话展示。
