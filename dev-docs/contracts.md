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
- `.hologram/audit.jsonl` 的新记录必须带 `prev_hash` 与 `integrity_hash`，形成顺序 SHA-256 哈希链。
- 没有 `integrity_hash` 的历史记录属于未受保护记录。报告必须将完整性状态标为 `partial`、`verified=false` 并给出 `unprotected_count`，不得标为已验证。哈希链不具有密钥绑定，不能单独宣称为不可篡改。
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

## Delivery Report Integrity Contract

```ts
interface DeliveryAuditIntegritySummary {
  status: 'empty' | 'verified' | 'partial' | 'failed';
  verified: boolean;
  entry_count: number;
  chained_entry_count: number;
  unprotected_count: number;
  unprotected_lines: number[];
  last_hash?: string;
  issues: string[];
}

interface DeliveryReportSignature {
  algorithm: 'sha256';
  digest: string;
}
```

要求：

- `audit-risk report` 必须导出 `audit.integrity`，明确最近导出窗口内审计链是否可信。
- 发现哈希链损坏时，`audit.integrity.status` 必须为 `failed`，并给出最小可定位的 `issues`。
- `audit.integrity` 必须基于全量 JSONL 链校验，展示窗口不能截断校验链；无哈希记录必须返回 `partial` 而不是 `verified`。
- 导出 JSON 必须附带 `report_signature`，用于对整个 machine-readable report 做二次校验。
- `doctor` 与管理侧只读视图不得自定义另一套完整性口径，必须复用同一个 `audit.integrity` 结果。

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
  proposal_validation?: {
    secondary_audit: string;
    syntax_check: string;
    logic_change: string;
    blocked_message?: string;
  };
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
- repair proposal 一旦生成，必须先写入 `repair_proposal_validation` 再决定是否允许展示；UI 和只读工具不允许跳过这层 owner 结论直接显示 proposal。
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

## CLI Command Contract

```ts
type CliCommandTier = 'primary' | 'secondary';
type CliStatus = 'ok' | 'ready' | 'needs_attention' | 'error';

interface CliStructuredEnvelope {
  schema_version: 'audit-risk.cli.v1';
  command:
    | 'check'
    | 'watch'
    | 'diff'
    | 'init'
    | 'doctor'
    | 'report'
    | 'rules'
    | 'audit'
    | 'verify'
    | 'notify';
  generated_at: string;
  workspace_root?: string;
  status: CliStatus;
}
```

补充命令面：

- 人类界面命令：`audit-risk`（零参数首页）、`help`、`tour`、`check`、`diff`、`init`、`doctor`、`report`、`report --history-compare`、`notify --test`、`auth login/logout/status`、`observe`
- 结构化 JSON 键名继续稳定英文；中文化要求只作用于人类可读终端输出

命令分层：

- primary：`check`、`watch`、`diff`、`init`、`doctor`
- secondary：`report`、`rules`、`audit`、`verify`、`notify`

默认输出：

- `check` / `diff` / `init` / `doctor` / `report` / `notify --test`：默认输出中文人类页面；加 `--json` 时切回结构化 JSON
- `rules` / `audit` / `verify`：JSON 到 `stdout`
- `watch`：人类可读终端摘要；`--jsonl` 时输出结构化事件流

退出码：

- `0`：命令执行成功，结果低于当前 fail gate
- `1`：未预期内部错误
- `2`：命令执行成功，但 gate 达到失败阈值
- `3`：环境或配置错误
- `4`：CLI 用法错误

要求：

- 公共二进制名统一为 `audit-risk`；`hologram-risk-check` 只允许作为迁移期内部实现名存在，不能继续出现在正式文档和公共帮助里。
- `watch --jsonl` 至少要保留 `session_started`、`check_completed`、`finding_emitted`、`finding_suppressed`、`gate_decided`、`session_error` 六类事件。
- `watch` 默认只展示 `critical/high/medium`；`low/info` 仅在 `--verbose` 下允许进入人类可读摘要。
- `watch` 必须按 `file_path + rule_id` 做防抖；同一文件同一规则 10 分钟内只允许输出一次，重复命中应进入 `finding_suppressed` 路径而不是持续刷屏。
- `watch --observe` 必须启动本地只读观察页，并打印 `local_url`、`public_url` 与二维码图片路径；若外网/LAN 绑定受环境限制，必须显式写出 fallback note，而不是静默失败。
- 零参数 `audit-risk` 不再直接报错；无论当前目录是不是 workspace，都先进入中文新手首页，并在 workspace 内同时展示当前目录状态、上次审查结果、Core/Pro 状态和推荐下一步。
- `help`、`tour`、`auth status`、Pro gate 提示都必须用中文大白话；不能把英文冷错误直接暴露给最终用户。
- `check`、`diff`、`init`、`doctor`、`report`、`notify --test` 的默认终端输出也必须走同一套中文产品壳；脚本或 hook 若要消费机器结果，必须显式传 `--json`。
- `check` / `diff` 输出至少包含 `changed_files`、`analysis`、`review`、`audit_ref`；`check` 还必须包含 `changed_files_source: 'git_status' | 'external_nul_list'`。
- `check --files-from <path|->` 的输入固定为 UTF-8、NUL 分隔、workspace-relative 的文件路径；显式空清单表示本轮没有变更，不得回退读取 `git status`。
- GitHub Action 的 PR/push 审查必须在完整 checkout 上显式比较 base/head，以 `git diff --find-renames --name-only -z --diff-filter=ACMR` 生成清单并复用 `--files-from`；初次 push 的全零 base 使用已写入 object database 的空 tree。
- `external_nul_list` 当前是文件级增量合同：扫描 head 中 A/C/M/R 文件的完整内容。稳定 finding 指纹或行级 baseline 合同建立前，不得宣称它只阻断 diff 新增行里的 finding。
- `init` 输出必须列出 `created_files`。
- `init` 必须先完成全部目标文件的冲突与可写性预检，再进入写入阶段；预检失败不得留下部分接入文件。
- `init --force` 覆盖已有文件前必须在同目录保留 `.bak.<timestamp>-<uuid>` 备份；`init --dry-run` 只返回 `planned_files`，不得写文件或修改 `core.hooksPath`。
- `doctor` 输出必须区分 `ready`、`needs_attention`、`error`，并显式列出检查项、blockers 与 notes。
- `doctor` 的最小检查项至少包含：CLI 版本、`git`/`cargo` 依赖完整性、workspace 可读性、`.hologram` 可写性、delivery config 可解析性、review/repair rule package 可加载性（含 `package_id/version`）、provider 配置就绪状态、audit 路径可写性。
- `doctor` 是只读诊断命令：不得创建 workspace、`.hologram` 或 audit 目录，也不得修改 Git 配置；必须只读检查 `core.hooksPath` 与生成的 pre-commit hook 是否共同就位。
- 风险豁免只允许通过 `audit-risk approve [workspace] --finding <finding_id> --reason <text> --expires <ISO8601>` 记录，不支持 `audit-risk:ignore` 一类源码注释豁免。
- `approve` 只能引用当前 workspace 最近一次 `review_check` 中存在的 finding；审批记录必须通过 `append_audit_entry` 写入 SHA-256 审计链，并记录 canonical workspace root、`finding_id`、理由、审批时间、失效时间、actor 与 finding 指纹。多个 workspace 即使共用同一绝对 audit 路径，也不得交叉消费审批。
- finding 指纹必须同时绑定结构化 finding 信息和 workspace 内证据文件内容哈希，代码证据变化后旧审批不得继续生效。完整 `YYYY-MM-DD` 日期在该 UTC 日期结束后失效，完整 RFC3339 时间按其绝对时间失效。
- `check` / `report` 只应用 finding_id、指纹均匹配且尚未过期的审批；应用后将该 finding 的 `gate_effect` 降为 `allow`、重算 gate，并把 `applied_approvals` 写入当前输出和后续 `review_check` 审计记录，不得静默跳过 finding。
- `notify --test` 必须返回结构化结果，至少包含 `tested_url`、`http_status`、`ok`；未提供 webhook URL 时，允许从 `delivery.json.observe.webhook_url` 或环境变量读取。
- `report --history-compare`、`observe`、`notify`、`watch --observe` 必须在命令入口第一层做 Pro entitlement gate；gate 失败时只返回中文提示，不进入任何真实功能逻辑。
- secondary 命令允许在迁移期复用现有 phase5 壳，但公共命令面只能由 Rust CLI owner 定义，不能再由 TS 脚本私自扩字段或改退出码。

## Personal Pro Entitlement Contract

```ts
type PersonalEntitlementState =
  | 'active'
  | 'grace'
  | 'expired'
  | 'revoked'
  | 'device_mismatch'
  | 'missing'
  | 'invalid';

interface PersonalEntitlement {
  user_id: string;
  plan: 'core_free' | 'pro_personal_monthly';
  features: string[];
  issued_at: string;
  valid_until: string;
  device_id: string;
  last_refresh_time: string;
  status: 'active' | 'revoked';
  payment_pending?: boolean;
  next_billing_at?: string;
}
```

要求：

- 本地 entitlement 目录固定为 `{app_support_dir}/audit-risk/entitlement/`，最少包含 `entitlement.json`、`entitlement.sig`、`device_secret` 三个文件。
- `session.json` 为登录中会话缓存，至少包含 `session_id / status / created_at / expires_at / poll_url / exchange_url / login_url`；`expires_at` 过期后不能继续显示“登录进行中”。
- `device_secret` 为首次登录生成的本机随机值，独立文件保存；丢失或 `device_id` 不匹配后必须进入 `device_mismatch`，提示重新 `audit-risk auth login`。
- `device_id` 的 exact 输入为 UTF-8 `trim(device_secret) + "|" + os + "|" + machine_identity`，对这些字节计算 SHA-256 并输出小写十六进制；`os` 使用 Rust `std::env::consts::OS` 原值，`machine_identity` 去除来源格式的外围空白后保留原始大小写。`machine_identity` 必须来自 macOS `IOPlatformUUID`、Linux `machine-id` 或 Windows `MachineGuid`，不得使用可覆盖的主机名环境变量或 `unknown-host` 占位值。
- entitlement canonical 签名字段固定包含 `device_id / features / issued_at / last_refresh_time / plan / status / user_id / valid_until`。服务端必须按键名字典序输出 UTF-8 compact JSON（无额外空白，字符串使用标准 JSON escaping），对这些 exact bytes 生成 Ed25519 签名，再以 standard Base64 编码 64 字节签名；CLI 使用同一字节合同验签，且不得改写服务端返回的已签名 `device_id`。
- 未覆盖 `device_id` 的旧签名不做兼容，必须进入 `invalid` 并要求重新登录绑定设备。
- 服务端 entitlement 的 `status` 当前只接受 `active / revoked`；本地状态机固定为 `active / grace / expired / revoked / device_mismatch / missing / invalid`，其中 `grace/expired` 由 `valid_until` 与 72 小时宽限期派生，未知远端 status 必须进入 `invalid`，不能放行 Pro。
- `payment_pending=true` 且 `plan=core_free` 时，`auth status` 必须显示“支付确认中”，不能退回泛化未登录。
- `auth login` 的浏览器流程、邮箱/手机号验证码、微信支付/支付宝、`GET /api/auth/poll`、`POST /api/auth/exchange`、`POST /api/entitlement/refresh` 的接口合同已经冻结，但当前 Core 仓库在服务端未接入前只能实现 CLI 侧合同与状态读取，不能伪造 Pro 成功态。
- `auth status` 输出必须固定显示登录状态、当前版本、有效期/宽限期、设备绑定、开通方式或取消方式。
- `delivery.json.auth.base_url` 是 `auth login`、`auth status` stale entitlement refresh 与 `doctor` 的首选服务地址；环境变量 `AUDIT_RISK_AUTH_BASE_URL` 只作为回退。
- `doctor` 至少暴露 `auth_service` 与 `entitlement_cache`，其中服务失败要结构化到 `network_unreachable / bad_json / timeout / auth_service_error`。
- `observe`、`notify` 等 Pro 能力由授权系统控制，不依赖加壳、混淆或“防护锁”作为主方案。

## Delivery Integration Contract

```ts
interface DeliveryConfig {
  version: 'phase5.v1';
  workspace: {
    root: string;
    changed_files_source: 'git_status';
  };
  provider: {
    name: string;
    model: string;
    base_url: string;
    key_source: 'env' | 'secure_store';
    env_var?: string;
  };
  rule_packages: {
    review_paths: string[];
    repair_paths: string[];
    disabled_review_rule_ids: string[];
    disabled_repair_rule_ids: string[];
  };
  audit: {
    jsonl_path: string;
    report_output_path: string;
    recent_limit: number;
  };
  auth: {
    base_url: string;
  };
  automation: {
    verify_commands: string[];
    pre_commit_hook: string;
    ci_workflow: string;
    fail_on_decision: GateDecisionValue;
  };
}

interface DeliveryMachineReport {
  generated_at: string;
  workspace: {
    root: string;
    changed_files_source: 'git_status';
    audit_jsonl_path: string;
    report_output_path: string;
  };
  provider: {
    name: string;
    model: string;
    base_url: string;
    key_source: 'env' | 'secure_store';
    ready: boolean;
    reason: string;
    env_var?: string;
  };
  policies: {
    review: ResolvedRulePolicy;
    repair: ResolvedRulePolicy;
  };
  current_review: CurrentReviewSummaryResponse;
  audit: AuditQueryResult;
  automation: {
    verify_commands: string[];
    pre_commit_hook: string;
    ci_workflow: string;
    fail_on_decision: GateDecisionValue;
    should_fail: boolean;
  };
}
```

要求：

- `delivery.json` 是 workspace/provider/rule-package/audit 接入真源，不依赖 UI 文案或 README 推断。
- `delivery.json.auth.base_url` 是 CLI 侧 auth 服务地址的首选真源；环境变量 `AUDIT_RISK_AUTH_BASE_URL` 只作为回退，不应成为长期唯一配置入口。
- Rust CLI owner 与 `src-ui/src/risk/delivery.ts` 必须对 `delivery.json.auth.base_url` 保持同一合同，禁止一边支持 auth 配置、另一边仍生成旧模板。
- workspace rule package 文件缺失时，系统回退到默认 policy，而不是把“未自定义扩展”误判为交付失败。
- machine report 必须同时包含 current review、normalized audit、active policy 和 automation fail gate。
- pre-commit / CI 都只能消费 `audit-risk report` 或其下游 Delivery Plane，而不是各写一套私有脚本逻辑。

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
- CLI 审批必须显式执行 `audit-risk repair approve <workspace> --plan <id>`；它将 `waiting_approval` 转为 `approved`，并在 apply 前写入独立 `repair_approved` 审计记录，至少包含 plan、审批时间和本地 CLI 操作者声明。`repair apply --approve` 是自动化单步入口，但必须先写同一审批审计记录再 apply。
- CLI apply 必须清洗 `plan_id`（仅 `[A-Za-z0-9_.-]`，且拒绝 `..` 与路径分隔符），拒绝绝对路径、父目录穿越、符号链接、`.git/`、`.env*`、锁文件和密钥容器路径；规范化后的写入目标必须仍在工作区内。
- apply 在任何写入前必须对每个 `new_content` 执行二次风险扫描；命中新风险时不得写入任何文件。
- 回滚快照必须记录目标是否原本存在和原始字节；失败时删除本次新建文件、按字节恢复原文件，并且只有全部恢复成功才能记录 `repair_rolled_back`，否则必须记录回滚失败。
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
- CLI 不得执行 repair plan 提供的任意 `required_tests` 命令。当前 CLI 仅允许内置的固定 argv 预检 `git diff --check` 并记录结果；其他计划内命令必须拒绝，不能降级为 shell 或 `Command::new` 调用。
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

## RepairProposalValidationSummary Contract

```ts
interface RepairProposalValidationSummary {
  secondary_audit: {
    passed: boolean;
    summary: string;
  };
  syntax_check: {
    passed: boolean;
    summary: string;
  };
  logic_change: {
    summary: string;
  };
  blocked: boolean;
  blocked_reason?: string;
}
```

要求：

- 所有 AI 生成的 repair proposal 在展示给用户前，必须先经过二次审计和快速语法检查。
- 成功展示时，至少要有三条显式输出：`✅ 二次审计通过`、`✅ 语法检查通过`、`⚠️ 逻辑变更提示 ...`。
- 若二次审计判定 proposal 引入新风险，必须直接阻断展示，并使用固定文案 `该修复方案引入了新的风险，已被系统自动拦截`。
- 若快速语法检查失败，必须直接阻断展示，并使用固定文案 `该修复方案未通过语法检查，已被系统自动拦截`。
- `blocked` / `blocked_reason` 属于 owner 层真相，不能让 UI 或 prompt 自行推断。

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
