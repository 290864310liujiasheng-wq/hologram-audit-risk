import { adaptCheckResultToFindings, buildCheckRiskSummary, type RiskCheckResult } from './check-adapter';
import { buildRepairHistory, type AuditRecord, type RepairHistoryItem } from './audit-bridge';
import { finalizeMultiAgentReview, type MultiAgentReview } from './multi-agent';
import type { LiveRepairReadiness, ProviderReadiness } from '../provider/provider-readiness';
import { resolveRulePolicy } from './rule-package';
import { buildRepairPreflightSummary, createRepairPlan, type RepairGenerationMetadata, type RepairGenerationReadiness } from './self-heal';
import { deriveGateDecision, type GateDecision } from './review-core';
import type { PatchProposal, RepairIssue, RepairPlan, RepairPreflightReport, RepairRollbackSnapshot, ReviewFinding } from './review-core';

export interface WorkbenchQueueItem {
  step_id: 'review' | 'gate' | 'evidence' | 'approval' | 'repair';
  title: string;
  state: string;
  summary: string;
  detail?: string;
  section_id: 'risk-summary' | 'gate-decision' | 'repair-workbench';
}

export interface RepairWorkbenchSnapshot {
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
  provider?: {
    summary: string;
    reason?: string;
  };
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

export interface CurrentReviewState<TCheckResult extends RiskCheckResult = RiskCheckResult> {
  check: TCheckResult;
  findings: ReviewFinding[];
  summary: ReturnType<typeof buildCheckRiskSummary>;
  gate_decision: GateDecision;
  multi_agent_review: MultiAgentReview;
  repair_plan: RepairPlan;
  provider_readiness?: ProviderReadiness;
  live_repair_readiness?: LiveRepairReadiness;
  repair_generation_readiness?: RepairGenerationReadiness;
  patch_proposal?: PatchProposal;
  rollback?: RepairRollbackSnapshot;
  repair_issue?: RepairIssue;
  repair_generation_meta?: RepairGenerationMetadata;
  repair_preflight?: RepairPreflightReport;
}

export type CurrentReviewSummaryResponse<TCheckResult extends RiskCheckResult = RiskCheckResult> =
  | {
      status: 'empty';
      message: string;
    }
  | {
      status: 'ok';
      review: CurrentReviewState<TCheckResult>;
      workbench_queue: WorkbenchQueueItem[];
      repair_history: RepairHistoryItem[];
      repair_workbench: RepairWorkbenchSnapshot;
    };

export function buildCurrentReviewState<TCheckResult extends RiskCheckResult>(input: {
  result: TCheckResult;
  workspace_path: string;
}): CurrentReviewState<TCheckResult> {
  const jobId = `check:${input.result.timestamp || 'current'}`;
  const reviewPolicy = resolveRulePolicy({ plane: 'review' });
  const findings = adaptCheckResultToFindings(input.result, {
    jobId,
    evidencePrefix: 'check',
  });
  const gateDecision = deriveGateDecision({
    job_id: jobId,
    subject_type: 'file_write',
    subject_ref: input.workspace_path,
    findings,
    rules: reviewPolicy.rules,
    policy_snapshot_id: reviewPolicy.policy_snapshot_id,
    decided_at: input.result.timestamp || new Date().toISOString(),
  });

  return {
    check: input.result,
    findings,
    summary: buildCheckRiskSummary(findings),
    gate_decision: gateDecision,
    multi_agent_review: finalizeMultiAgentReview({
      job_id: jobId,
      findings,
      started_at: input.result.timestamp || new Date().toISOString(),
      completed_at: input.result.timestamp || new Date().toISOString(),
    }),
    repair_plan: createRepairPlan({
      job_id: jobId,
      findings,
      workspace_path: input.workspace_path,
    }),
  };
}

export function buildCurrentReviewSummaryResponse<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult> | null,
  auditRecords: AuditRecord[] = [],
): CurrentReviewSummaryResponse<TCheckResult> {
  if (!state) {
    return {
      status: 'empty',
      message: 'No review result available yet.',
    };
  }

  return {
    status: 'ok',
    review: state,
    workbench_queue: buildWorkbenchQueue(state, auditRecords),
    repair_history: buildRepairHistory(auditRecords),
    repair_workbench: buildRepairWorkbenchSnapshot(state, auditRecords),
  };
}

export function buildWorkbenchQueue<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult>,
  auditRecords: AuditRecord[] = [],
): WorkbenchQueueItem[] {
  const evidenceCount = Array.from(new Set(state.findings.flatMap((finding) => finding.evidence_ids))).length;
  const repairHistory = buildRepairHistory(auditRecords);
  const latestRepair = repairHistory[0];
  const noRisk = state.findings.length === 0;
  const repairDegraded = !!state.repair_issue?.error.retryable;

  return [
    {
      step_id: 'review',
      title: '看风险',
      state: state.findings.length > 0 ? 'needs_attention' : 'clean',
      summary: state.findings.length > 0
        ? `${state.findings.length} 条风险待处理`
        : '当前 review 无新增风险',
      detail: state.summary.topFindings[0]?.plain_explanation,
      section_id: 'risk-summary',
    },
    {
      step_id: 'gate',
      title: '看 gate',
      state: state.gate_decision.decision,
      summary: state.gate_decision.reason,
      detail: `策略 ${state.gate_decision.policy_snapshot_id}`,
      section_id: 'gate-decision',
    },
    {
      step_id: 'evidence',
      title: '看证据',
      state: noRisk ? 'not_required' : (evidenceCount > 0 ? 'ready' : 'missing'),
      summary: noRisk
        ? '当前无风险，无需额外证据闭环'
        : `${state.findings.length} 条 finding · ${evidenceCount} 个 evidence`,
      detail: noRisk
        ? '空状态已收口'
        : (evidenceCount > 0 ? '证据可追溯' : '当前 finding 缺少 evidence 引用'),
      section_id: 'repair-workbench',
    },
    {
      step_id: 'approval',
      title: '审批/阻断',
      state: noRisk ? 'not_required' : state.repair_plan.approval_state,
      summary: describeApprovalStep(state),
      detail: state.repair_issue?.stage === 'preflight' ? state.repair_issue.summary : undefined,
      section_id: 'repair-workbench',
    },
    {
      step_id: 'repair',
      title: 'repair/rollback',
      state: noRisk
        ? 'not_required'
        : (latestRepair?.status || (repairDegraded ? 'degraded' : state.repair_plan.approval_state)),
      summary: latestRepair
        ? `${latestRepair.stage} · ${latestRepair.reason}`
        : describeRepairStep(state),
      detail: latestRepair?.state_change
        ? `${latestRepair.state_change.from_state || '?'} -> ${latestRepair.state_change.to_state || '?'}`
        : (state.rollback ? `rollback ${state.rollback.rollback_id}` : undefined),
      section_id: 'repair-workbench',
    },
  ];
}

export function buildRepairWorkbenchSnapshot<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult>,
  auditRecords: AuditRecord[] = [],
): RepairWorkbenchSnapshot {
  const repairHistory = buildRepairHistory(auditRecords);
  const evidenceCount = Array.from(new Set(state.findings.flatMap((finding) => finding.evidence_ids))).length;
  const noRepairCandidate = (state.repair_generation_readiness?.finding_count ?? state.repair_plan.finding_ids.length) === 0
    && !state.patch_proposal
    && !state.repair_issue;

  const preflightSummary = state.repair_preflight
    ? buildRepairPreflightSummary(state.repair_preflight)
    : undefined;

  return {
    status_state: noRepairCandidate
      ? 'not_required'
      : (state.repair_issue?.error.retryable ? 'degraded' : state.repair_plan.approval_state),
    status_label: noRepairCandidate ? '当前无可修复风险' : `状态 ${state.repair_plan.approval_state}`,
    test_count: noRepairCandidate ? 0 : state.repair_plan.required_tests.length,
    strategy: noRepairCandidate
      ? '当前 review 没有进入自修复闭环的风险，等待新的可修复 finding 后再生成提案。'
      : state.repair_plan.strategy,
    risk_note: noRepairCandidate ? undefined : state.repair_plan.risk_note,
    required_tests: state.repair_plan.required_tests,
    generation_input: state.repair_generation_readiness
      ? {
          finding_count: state.repair_generation_readiness.finding_count,
          file_count: state.repair_generation_readiness.file_count,
          eligible: state.repair_generation_readiness.eligible,
          reason: state.repair_generation_readiness.eligible ? undefined : state.repair_generation_readiness.reason,
        }
      : undefined,
    provider: state.provider_readiness
      ? {
          summary: `Provider: ${state.provider_readiness.provider_name} / ${state.provider_readiness.model} · ${state.provider_readiness.ready ? 'ready' : 'not ready'} · ${state.provider_readiness.source}`,
          reason: state.provider_readiness.ready ? undefined : state.provider_readiness.reason,
        }
      : undefined,
    live_repair_reason: state.live_repair_readiness && !state.live_repair_readiness.eligible
      ? `Live repair: ${state.live_repair_readiness.reason}`
      : undefined,
    generation_meta: state.repair_generation_meta
      ? `生成: ${state.repair_generation_meta.provider_name} / ${state.repair_generation_meta.model} · 文件 ${state.repair_generation_meta.file_count} 个 · 高风险 ${state.repair_generation_meta.high_severity_finding_ids.length} 条`
      : undefined,
    proposal: state.patch_proposal
      ? `补丁提案: ${state.patch_proposal.summary} · ${state.patch_proposal.operations.length} 个文件操作`
      : undefined,
    issue_badge: state.repair_issue
      ? (state.repair_issue.error.retryable ? '提案降级，可重试' : '提案失败，需修正')
      : undefined,
    issue_stage: state.repair_issue?.stage,
    issue_summary: state.repair_issue?.summary,
    issue_note: state.repair_issue?.error.code === 'missing_evidence'
      ? '当前风险已识别，但还没有收口到可直接修改的源码文件；这不是 provider/key 故障。'
      : undefined,
    preflight: preflightSummary
      ? {
          summary: `复检: ${preflightSummary.reason} · finding ${state.repair_preflight?.findings.length || 0} 条 · 验证 ${state.repair_preflight?.test_results.length || 0} 条`,
          failed_commands: preflightSummary.failed_commands,
          blocking_rule_ids: preflightSummary.blocking_rule_ids,
        }
      : undefined,
    rollback: state.repair_issue?.stage === 'apply' && state.rollback
      ? `已自动回滚: ${state.rollback.rollback_id}`
      : undefined,
    evidence_trace: {
      finding_count: state.findings.length,
      evidence_count: evidenceCount,
      repair_history_count: repairHistory.length,
    },
    repair_history: repairHistory,
  };
}

export function attachProviderReadinessToCurrentReview<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult>,
  providerReadiness: ProviderReadiness,
): CurrentReviewState<TCheckResult> {
  return {
    ...state,
    provider_readiness: providerReadiness,
  };
}

export function attachLiveRepairReadinessToCurrentReview<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult>,
  liveRepairReadiness: LiveRepairReadiness,
): CurrentReviewState<TCheckResult> {
  return {
    ...state,
    live_repair_readiness: liveRepairReadiness,
    provider_readiness: liveRepairReadiness.provider,
  };
}

export function attachRepairGenerationReadinessToCurrentReview<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult>,
  readiness: RepairGenerationReadiness,
): CurrentReviewState<TCheckResult> {
  return {
    ...state,
    repair_generation_readiness: readiness,
  };
}

function describeApprovalStep<TCheckResult extends RiskCheckResult>(state: CurrentReviewState<TCheckResult>): string {
  if (state.findings.length === 0) {
    return '当前无风险，无需进入审批链';
  }
  switch (state.repair_plan.approval_state) {
    case 'waiting_approval':
      return '修复提案已生成，等待审批';
    case 'approved':
      return '审批已通过，允许进入 apply';
    case 'rejected':
      return '审批已拒绝，需重新生成或调整提案';
    case 'applied':
      return '审批链已闭环，修复已应用';
    case 'rolled_back':
      return '修复已回滚，本轮审批链已结束';
    default:
      return state.gate_decision.decision === 'require_approval'
        ? '当前 gate 需要审批后才能继续'
        : '当前尚未进入审批阶段';
  }
}

function describeRepairStep<TCheckResult extends RiskCheckResult>(state: CurrentReviewState<TCheckResult>): string {
  if (state.findings.length === 0) {
    return '当前无可修复风险，repair/rollback 不适用';
  }
  if (state.repair_issue) {
    return state.repair_issue.summary;
  }
  if (state.rollback) {
    return `最近一次修复已生成回滚快照 ${state.rollback.rollback_id}`;
  }
  if (state.patch_proposal) {
    return `当前提案包含 ${state.patch_proposal.operations.length} 个文件操作`;
  }
  if (state.repair_generation_readiness && !state.repair_generation_readiness.eligible) {
    return state.repair_generation_readiness.reason;
  }
  return '当前尚未进入 repair apply / rollback 历史';
}

export function attachRepairIssueToCurrentReview<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult>,
  input: {
    issue: RepairIssue;
    repair_generation_meta?: RepairGenerationMetadata;
  },
): CurrentReviewState<TCheckResult> {
  return {
    ...state,
    patch_proposal: undefined,
    repair_issue: input.issue,
    repair_generation_meta: input.repair_generation_meta,
    repair_preflight: undefined,
  };
}

export function attachRepairProposalToCurrentReview<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult>,
  input: {
    repair_plan: RepairPlan;
    patch_proposal: PatchProposal;
    repair_generation_meta?: RepairGenerationMetadata;
  },
): CurrentReviewState<TCheckResult> {
  return {
    ...state,
    repair_plan: input.repair_plan,
    patch_proposal: input.patch_proposal,
    repair_issue: undefined,
    repair_generation_meta: input.repair_generation_meta,
    repair_preflight: undefined,
  };
}

export function attachRepairPreflightIssueToCurrentReview<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult>,
  input: {
    issue: RepairIssue;
    preflight: RepairPreflightReport;
  },
): CurrentReviewState<TCheckResult> {
  return {
    ...state,
    repair_issue: input.issue,
    repair_preflight: input.preflight,
  };
}

export function attachRepairExecutionIssueToCurrentReview<TCheckResult extends RiskCheckResult>(
  state: CurrentReviewState<TCheckResult>,
  input: {
    issue: RepairIssue;
    rollback: RepairRollbackSnapshot;
  },
): CurrentReviewState<TCheckResult> {
  return {
    ...state,
    repair_plan: {
      ...state.repair_plan,
      approval_state: 'rolled_back',
    },
    repair_issue: input.issue,
    rollback: input.rollback,
    repair_preflight: undefined,
  };
}
