import { adaptCheckResultToFindings, buildCheckRiskSummary, type RiskCheckResult } from './check-adapter';
import { finalizeMultiAgentReview, type MultiAgentReview } from './multi-agent';
import type { LiveRepairReadiness, ProviderReadiness } from '../provider/provider-readiness';
import { DEFAULT_REVIEW_RULES } from './rule-package';
import { createRepairPlan, type RepairGenerationMetadata, type RepairGenerationReadiness } from './self-heal';
import { deriveGateDecision, type GateDecision } from './review-core';
import type { PatchProposal, RepairIssue, RepairPlan, RepairPreflightReport, RepairRollbackSnapshot, ReviewFinding } from './review-core';

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
    };

export function buildCurrentReviewState<TCheckResult extends RiskCheckResult>(input: {
  result: TCheckResult;
  workspace_path: string;
}): CurrentReviewState<TCheckResult> {
  const jobId = `check:${input.result.timestamp || 'current'}`;
  const findings = adaptCheckResultToFindings(input.result, {
    jobId,
    evidencePrefix: 'check',
  });
  const gateDecision = deriveGateDecision({
    job_id: jobId,
    subject_type: 'file_write',
    subject_ref: input.workspace_path,
    findings,
    rules: DEFAULT_REVIEW_RULES,
    policy_snapshot_id: 'policy:review-default:v1',
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
