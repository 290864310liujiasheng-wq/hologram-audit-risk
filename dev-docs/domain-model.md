# 领域模型

## 核心实体

### Workspace

客户正在开发和被审查的代码工作区。

- `workspace_id`
- `root_path`
- `vcs_provider`
- `active_branch`
- `policy_profile_id`
- `provider_profile_id`

### CodeChange

一次可审查的代码变化，可以来自 AI 工具输出、文件保存、Git diff、patch proposal 或提交前检查。

- `change_id`
- `workspace_id`
- `source`: `ai_output | file_save | git_diff | repair_patch | manual_scan`
- `base_ref`
- `head_ref`
- `files`
- `diff`
- `created_at`

### CodeEvidence

审查依据，不等同于 finding。

- `evidence_id`
- `change_id`
- `kind`: `source_span | diff_hunk | dependency_path | graph_signal | test_result | tool_log`
- `location`
- `content_ref`
- `summary`
- `confidence`

### ReviewJob

一次审查任务。

- `job_id`
- `workspace_id`
- `change_id`
- `mode`: `live | pre_commit | manual | ci | repair_validation`
- `status`: `queued | running | degraded | completed | blocked | cancelled | failed`
- `requested_agents`
- `created_at`
- `completed_at`

### ReviewFinding

一个风险结论。

- `finding_id`
- `job_id`
- `rule_id`
- `severity`: `info | low | medium | high | critical`
- `category`
- `locations`
- `plain_explanation`
- `impact`
- `recommendation`
- `evidence_ids`
- `model_trace_id`
- `confidence`
- `status`: `open | accepted | dismissed | fixed | suppressed`

### Rule

可执行或可解释的风控规则。

- `rule_id`
- `name`
- `category`
- `severity`
- `scope`
- `trigger`
- `gate_effect`: `observe | warn | require_approval | block`
- `explanation_template`
- `enabled`

### GateDecision

对某次变更或工具动作的放行/拦截结果。

- `decision_id`
- `job_id`
- `subject_type`: `tool_call | file_write | git_commit | repair_apply | release`
- `subject_ref`
- `decision`: `allow | warn | require_approval | block`
- `reason`
- `finding_ids`
- `policy_snapshot_id`
- `decided_at`

### AuditEvent

append-only 审计记录。

- `event_id`
- `workspace_id`
- `actor`
- `event_type`
- `subject_ref`
- `decision_id`
- `evidence_ids`
- `timestamp`
- `integrity_hash`

### ProviderProfile

客户自带模型 API 配置描述。密钥不进入文档、日志或普通配置导出。

- `provider_profile_id`
- `kind`: `anthropic | openai_compatible | local_gateway`
- `display_name`
- `base_url`
- `model`
- `secret_ref`
- `timeout_ms`
- `rate_limit`

### AgentRun

单个审查代理的一次执行。

- `agent_run_id`
- `job_id`
- `agent_type`
- `status`
- `input_evidence_ids`
- `finding_ids`
- `started_at`
- `completed_at`
- `error`

### RepairPlan

受控自修复计划。

- `repair_plan_id`
- `finding_ids`
- `strategy`
- `patch_proposal_ref`
- `required_tests`
- `risk_note`
- `approval_state`: `draft | waiting_approval | approved | rejected | applied | rolled_back`

## 状态流转

### ReviewJob

```text
queued -> running -> completed
queued -> running -> degraded -> completed
queued -> running -> blocked
queued -> running -> failed
queued -> cancelled
running -> cancelled
```

### Finding

```text
open -> accepted
open -> dismissed
open -> fixed
open -> suppressed
fixed -> reopened
```

### RepairPlan

```text
draft -> waiting_approval -> approved -> applied
draft -> waiting_approval -> rejected
applied -> rolled_back
```

## 关系

```text
Workspace 1--N CodeChange
CodeChange 1--N CodeEvidence
CodeChange 1--N ReviewJob
ReviewJob 1--N AgentRun
ReviewJob 1--N ReviewFinding
ReviewFinding N--N CodeEvidence
ReviewFinding N--1 Rule
ReviewJob 1--N GateDecision
GateDecision N--N ReviewFinding
AuditEvent N--1 GateDecision
RepairPlan N--N ReviewFinding
```

## 不变量

- Finding 必须至少关联一个 `CodeEvidence`，不能只有模型判断。
- Critical 或 High finding 如果产生 block，必须形成 `GateDecision` 和 `AuditEvent`。
- 客户模型密钥只能以 `secret_ref` 形式引用，不能进入 finding、audit、日志或导出文件。
- 自修复 apply 前必须存在 `RepairPlan`、patch proposal、验证结果和审批状态。
- dismissed/suppressed 必须记录 actor、理由和时间，不能静默消失。
