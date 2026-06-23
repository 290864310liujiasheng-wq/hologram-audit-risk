# 合同草案

本文件定义当前产品主线的设计合同。它不是已实现 API 清单；实现前必须先把这些合同落到对应 owner 层并补测试。

## Implementation Status

- 2026-06-20：`src-ui/src/risk/review-core.ts` 已实现第一段纯合同核心。
- 已覆盖：`ReviewJobRequest` 基本校验；`Rule` 基本校验；finding 缺 evidence 校验；confidence 范围校验；source location 非空与正序行号校验；`plain_explanation` 的最小白话质量校验；enabled rules 聚合 `GateDecision`；`ReviewJobResult` 的 `completed/degraded/blocked` 基线收口；`AuditEvent` 的基础构造。
- 已覆盖：`ReviewJobRequest` 基本校验；`Rule` 基本校验；finding 缺 evidence 校验；confidence 范围校验；source location 非空与正序行号校验；`plain_explanation`、`impact`、`recommendation` 的最小白话质量校验；`GateDecision` 的 subject/policy/reason/finding 基本校验；enabled rules 聚合 `GateDecision`；`ReviewJobResult` 的 `completed/degraded/blocked` 基线收口；`AuditEvent` 的基础构造。
- 已覆盖桥接：`src-ui/src/risk/check-adapter.ts` 可把现有 `CheckResult` 违规分桶映射成 `ReviewFinding`，并给出 severity 统计。
- 已覆盖 UI 读模型：`buildCheckRiskSummary` 可生成 `CheckPanel` 使用的风险摘要视图模型。
- 已覆盖审计桥接：`src-ui/src/risk/audit-bridge.ts` 可把 `CheckResult + ReviewFinding[]` 生成 review audit payload；Tauri `audit_append_review` 可接收并落盘。
- 已覆盖审批桥接：approval allow/deny 可转成结构化 audit payload，并写入 timeline + audit jsonl。
- 已覆盖 Agent 读取：Agent 可通过 `audit_recent_reviews` 与 `current_review_summary` 读取新对象；`current_review_summary` 现返回完整 current review object，而不是旧的摘要口径。
- 已覆盖强运行态信号：标题、状态栏、简报面板均可显示风险/审计状态。
- 已覆盖多代理合同：`AgentRun`、`ReviewAggregation`、`AggregationConflict` 已进入 `review-core.ts`；`multi-agent.ts` 已实现 specialist fan-out、去重、冲突记录、degraded reason。
- 已覆盖自修复合同：`RepairPlan`、`PatchProposal`、`PatchOperation`、`RepairRollbackSnapshot` 已进入 `review-core.ts`；`self-heal.ts` 已实现 plan/proposal/apply/rollback 状态流转。
- 已覆盖第四阶段规则系统首段：`Rule` 已补 `package_id` 与 `priority`；`RulePackage` 已进入 `review-core.ts`；`rule-package.ts` 已实现默认 review / repair package、扩展包合并、禁用 rule 与 `policy_snapshot_id` 生成；`current-review.ts` 与 `self-heal.ts` 已改为消费统一 active policy。
- 已覆盖第四阶段审计系统首段：`audit-bridge.ts` 已补 `AuditQueryResult`、`AuditRecord`、统一 stage/status/error/state-change 归一；`workspace.ts` 与 `CheckPanel` 已改为消费同一个 normalized audit query truth。
- 已覆盖 repair preflight 合同：`ValidationCommandResult`、`RepairPreflightReport` 已进入 `review-core.ts`；`rule-package.ts` 已实现默认 repair 规则包；`self-heal.ts` 已在 apply 前执行 rule re-check 与 `required_tests` gate。
- 已覆盖 repair preflight 合同：`ValidationCommandResult`、`RepairPreflightReport` 已进入 `review-core.ts`；`rule-package.ts` 已实现默认 repair 规则包；`self-heal.ts` 已在 apply 前执行 rule re-check 与 `required_tests` gate，并在 preflight 阻断时抛出结构化 `RepairApplyError` 供 UI/audit 消费。
- 已覆盖 review gate 合同：`rule-package.ts` 已实现默认 review 规则包；`current-review.ts` 会基于 `check.l5/l4/l3/l2` 派生结构化 `GateDecision`，不再只返回 findings 摘要。
- 已覆盖 repair degrade 合同：`RepairIssue`、`RepairExecutionStage` 已进入 `review-core.ts`；provider 层已引入结构化 `ProviderRequestError` / `classifyProviderFailure`；`self-heal.ts` 在其上收口成 repair issue，并区分 407 代理拒绝、证书吊销、`ECONNRESET`/socket hang up 等连接中断；`current-review.ts` 已把 issue 暴露给工作台状态。
- 已覆盖 repair generation metadata：`RepairGenerationMetadata` 已进入修复链，记录 provider/model/file_count/high-severity focus，用于 live proposal 的运行态证据留口。
- 已覆盖 preflight failure contract：`RepairApplyError` 与 `buildRepairIssueFromPreflight` 已把 preflight 阻断变成结构化 issue；`buildRepairPreflightSummary` 已把失败命令与阻断 rule 摘要收口到可复用读模型。
- 已覆盖 apply execution failure contract：`RepairApplyExecutionError` 已把部分写入失败与自动回滚快照结构化；当前 review state 与 repair audit 都可读取 rollback 证据。
- 已覆盖 repair audit contract：`buildRepairAuditPayload` 已统一 proposal/approval/apply/preflight-failed/rollback 的审计详情形状，避免 `workspace.ts` 手拼散落字段；degraded issue 的 `error_stage` 与 `error_retryable` 也会进入 audit 详情。
- 已覆盖 provider secret restore contract：`src-tauri/src/credential.rs` 现已补 macOS Keychain 路径，不再只依赖 Windows DPAPI；provider key 的持久化/恢复不再在 macOS 上直接退化成 `unsupported platform`。
- 已覆盖 provider readiness contract：前后端现可通过 `credential_has` + `active_provider_readiness` 只读链路返回当前 active provider 的就绪状态、来源（inline/secure_store/missing）和缺失原因，避免 live repair proposal 只能盲试。
- 已覆盖 non-code repair validation contract：`deriveRequiredTests` 现会为 config / migration / serialization 等非代码高风险 repair 自动补 `git diff --check`，避免 `required_tests` 为空时 critical repair 永远无法形成 allow/apply 样本。
- 已覆盖 patch proposal path guard：`generatePatchProposalFromModel` 会拒绝写出 `RepairGenerationInput.files` 之外的操作路径，避免 live model 越权扩张 patch 范围。
- 已覆盖 patch proposal content guard：generation-time 校验会拒绝 no-op 文件重写，要求提案覆盖全部高风险 finding 文件，并验证实际改动区间以非空白、非纯注释语义变更的方式触达高风险 finding 行范围；同时允许直接删除高风险行作为合法修复，避免模型用局部、格式化或注释伪修复绕开核心风险。
- 已覆盖 patch proposal narrative guard：`parsePatchProposal` 会拒绝占位级 `summary` / `rationale` / `operation.summary`，要求提案给出最小可读的修复解释，而不是 `fix` / `todo` / `update` 这类伪语义文本。
- 已覆盖 current review 收口：`current-review.ts` 可把 `CheckResult` 派生为 findings、gate decision、multi-agent review 与 repair plan。
- 已取得 fresh 运行态证据：`dev-docs/evidence/phase3-runtime-samples.json` 已记录 `deepseek / deepseek-v4-pro` 的真实 proposal generation 成功样本，以及 config / migration 两组 critical repair 的 apply 后 re-check 归零证明。
- 当前非阻断缺口：`anthropic` 真实 live proposal 成功样本与更多真实上游故障 trace 仍待外部条件；阶段验收已由 `deepseek` 真样本和稳定复现 provider failure 矩阵覆盖。

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
- `impact` 与 `recommendation` 也必须是人能直接理解的描述，不能退化成短 token、规则编号或占位词。
- `locations` 必须尽量精确到行；跨文件风险可包含多个位置。
- `locations` 的 `start_line/end_line` 必须是正整数且 `end_line >= start_line`。
- `evidence_ids` 不能为空。
- `confidence` 范围为 `0..1`。

## Rule Contract

```ts
type GateEffect = 'observe' | 'warn' | 'require_approval' | 'block';

interface Rule {
  rule_id: string;
  package_id: string;
  name: string;
  category: string;
  severity: Severity;
  priority: number;
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

type RulePlane = 'review' | 'repair';
type RulePackageSource = 'system_default' | 'workspace_extension';

interface RulePackage {
  package_id: string;
  version: string;
  plane: RulePlane;
  source: RulePackageSource;
  enabled: boolean;
  description: string;
  rules: Rule[];
}
```

要求：

- 规则定义是风控真源之一，不能只写在 prompt 或 UI 文案里。
- `package_id` 与 `version` 必须能解释规则来源，避免 review / repair 混成一组匿名默认常量。
- `priority` 必须显式声明，并在相同 `gate_effect` 冲突时决定最终裁决原因。
- 默认包与扩展包必须通过同一 registry 解析出 active policy；禁止 current review、gate decision、repair preflight 各自维护一套默认规则数组。
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
- `subject_ref` 与 `policy_snapshot_id` 不能为空。
- `block` / `require_approval` 不能缺少 `reason` 或 `finding_ids`。
- review audit 的 `allowed/denied` 应与结构化 `gate_decision` 一致，不能只按 `passed` 布尔值粗暴映射。

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

## Audit Query Contract

```ts
interface AuditStateChange {
  from_state?: string;
  to_state?: string;
}

interface AuditRecord {
  event_id: string;
  timestamp: string;
  plane: 'review' | 'approval' | 'repair';
  stage: string;
  status: string;
  subject: string;
  reason: string;
  finding_ids: string[];
  evidence_ids: string[];
  policy_snapshot_id?: string;
  state_change?: AuditStateChange;
  error?: {
    code?: string;
    stage?: string;
    retryable?: boolean;
  };
}

interface AuditQueryResult {
  entries: RecentAuditEntry[];
  records: AuditRecord[];
}
```

要求：

- append-only `entries` 是原始落盘证据，`records` 是统一查询口径；UI 和只读工具必须消费同一套 `records`，不能各自解释 `details`。
- `records` 必须统一暴露 review / approval / repair 的 `stage`、`status`、`error`、`state_change`、`finding_ids`、`evidence_ids`。
- repair proposal / approval / apply / rollback 的审计详情必须尽量带上 state change 与 evidence 引用，避免复盘时只能看到一句 reason。

## Workbench Summary Contract

```ts
interface WorkbenchQueueItem {
  step_id: 'review' | 'gate' | 'evidence' | 'approval' | 'repair';
  title: string;
  state: string;
  summary: string;
  detail?: string;
}

interface RepairWorkbenchSnapshot {
  status_state: string;
  status_label: string;
  test_count: number;
  strategy: string;
  risk_note?: string;
  required_tests: string[];
  generation_input?: {
    finding_count: number;
    file_count: number;
    eligible: boolean;
    reason?: string;
  };
  provider?: { summary: string; reason?: string };
  live_repair_reason?: string;
  generation_meta?: string;
  proposal?: string;
  issue_badge?: string;
  issue_stage?: string;
  issue_summary?: string;
  issue_note?: string;
  preflight?: {
    summary: string;
    failed_commands: string[];
    blocking_rule_ids: string[];
  };
  rollback?: string;
  evidence_trace: {
    finding_count: number;
    evidence_count: number;
    repair_history_count: number;
  };
  repair_history: RepairHistoryItem[];
}

type CurrentReviewSummaryResponse =
  | { status: 'empty'; message: string }
  | {
      status: 'ok';
      review: CurrentReviewState;
      workbench_queue: WorkbenchQueueItem[];
      repair_history: RepairHistoryItem[];
      repair_workbench: RepairWorkbenchSnapshot;
    };
```

要求：

- `current_review_summary` 不得只返回原始 review state；至少要把 `workbench_queue` 与 `repair_history` 一起返回，保证只读工具和工作台消费同一条主路径真源。
- `自修复闭环` 里使用的 provider/evidence/preflight/history 状态不应继续由 UI 组件自己判断；需要由 owner 层提供统一 `RepairWorkbenchSnapshot`。
- `current_review_summary` 若支持 `limit` 之类折叠参数，tool schema、workspace executor 与文档口径必须一起更新，不能出现执行器支持但工具合同未声明的漂移。
- clean review 的后续步骤必须显式收口为 `not_required`；retryable repair issue 必须显式收口为 `degraded`。

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
- 若当前 findings 只落在 config / migration / serialization 等非代码高风险文件上，repair plan 仍必须派生至少一条最小验证命令；当前默认使用 `git diff --check` 作为 apply 前基础 gate，而语义风险是否真正消除仍由 rule re-check 证明。

## RepairPreflight Contract

```ts
interface ValidationCommandResult {
  command: string;
  passed: boolean;
  stdout: string;
  stderr: string;
}

interface RepairPreflightReport {
  repair_plan_id: string;
  findings: ReviewFinding[];
  gate_decision: GateDecision;
  test_results: ValidationCommandResult[];
}
```

要求：

- apply-time preflight 是 owner 层语义，不能散落在 UI 按钮文案或 provider prompt 中。
- patch proposal 至少要经过 repair rule package 的静态 scope 校验：命中文件范围、绝对路径、敏感路径、重复写、波及面。
- `required_tests` 必须逐条执行并记录结果；任一失败都必须转成结构化 finding 并阻断 apply。
- 当 repair plan 覆盖的是 config / migration / serialization 等非代码高风险文件时，preflight 仍必须具备最小测试 gate；当前默认命令为 `git diff --check`。
- 预检报告必须可写入 audit，供后续追溯 apply 当时的 gate 决策与验证输出。
- preflight 失败必须保留原 proposal 上下文，并把 `gate_reason`、`blocking_rule_ids`、`failed_commands` 暴露给当前 review 读模型。

## RepairGenerationMetadata Contract

```ts
interface RepairGenerationMetadata {
  repair_plan_id: string;
  provider_name: string;
  model: string;
  file_count: number;
  focus_file_paths: string[];
  high_severity_finding_ids: string[];
  generated_at: string;
}
```

要求：

- live proposal 生成成功或失败时，都应尽量记录 provider/model/file_count/high-severity focus，作为运行态证据留口。
- 这类元数据属于 owner 层读模型，不应由 UI 文案或 prompt 临时拼接。

## RepairIssue Contract

```ts
type RepairExecutionStage = 'proposal_generation' | 'preflight' | 'apply' | 'rollback';

interface RepairIssue {
  issue_id: string;
  repair_plan_id: string;
  stage: RepairExecutionStage;
  summary: string;
  error: ContractError;
  created_at: string;
}
```

要求：

- provider 失败、超时、缺少源码上下文等 repair 运行态异常，不能只落一条瞬时 status 文本，必须收口成 `RepairIssue`。
- `RepairIssue.error.retryable` 必须显式区分“可重试”与“先修配置/输入”两类失败。
- `RepairIssue.stage` 与 `error.retryable` 必须写入 repair audit，避免运行态回放时只能看到失败码、看不到所在阶段与是否建议重试。
- 当前最小 provider 错误口径至少区分：`provider_auth_invalid`、`rate_limited`、`timeout`、`provider_upstream_failed`、`network_unreachable`、`tls_handshake_failed`、`tls_cert_revoked`、`proxy_rejected`、`connection_interrupted`、`provider_unavailable`。
- `proposal_generation` 阶段的 issue 必须在 repair 面板和审计里可见，避免用户误以为系统只是没有响应。
- proposal generation 阶段返回的 `PatchProposal.operations[].file_path` 必须属于本轮提供给 model 的文件集合；否则直接视为无效提案。
- proposal generation 阶段返回的 `operations[].new_content` 必须真的改变对应文件内容；no-op rewrite 直接视为无效提案。
- proposal 的 `summary`、`rationale`、`operations[].summary` 必须是最小可读的修复解释，不能退化成 `fix`、`todo`、`update` 一类占位词。
- 当本轮 findings 中包含 `high/critical` 风险文件时，proposal 必须覆盖全部相关文件；不能只挑其中一个或改低风险文件来规避核心问题。
- 当本轮 findings 中包含 `high/critical` 行范围时，proposal 的实际改动区间必须与这些行范围重叠；只改同文件中的其他位置也视为无效提案。
- 对 `high/critical` 行范围的触达必须是非空白语义变更；只改缩进、空格、换行或纯格式化不算真正修复。
- 对 `high/critical` 行范围只改注释文本、保留代码本体不变，也不算真正修复；但直接删除高风险行本身算合法修复。

## 错误模型

```ts
interface ContractError {
  code:
    | 'invalid_request'
    | 'missing_evidence'
    | 'provider_auth_invalid'
    | 'provider_upstream_failed'
    | 'provider_unavailable'
    | 'network_unreachable'
    | 'tls_handshake_failed'
    | 'tls_cert_revoked'
    | 'proxy_rejected'
    | 'connection_interrupted'
    | 'rate_limited'
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
