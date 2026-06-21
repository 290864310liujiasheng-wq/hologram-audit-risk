import { adaptCheckResultToFindings, buildCheckRiskSummary, type RiskCheckResult } from './check-adapter';
import { finalizeMultiAgentReview, type MultiAgentReview } from './multi-agent';
import { createRepairPlan } from './self-heal';
import type { PatchProposal, RepairPlan, RepairRollbackSnapshot, ReviewFinding } from './review-core';

export interface CurrentReviewState<TCheckResult extends RiskCheckResult = RiskCheckResult> {
  check: TCheckResult;
  findings: ReviewFinding[];
  summary: ReturnType<typeof buildCheckRiskSummary>;
  multi_agent_review: MultiAgentReview;
  repair_plan: RepairPlan;
  patch_proposal?: PatchProposal;
  rollback?: RepairRollbackSnapshot;
}

export function buildCurrentReviewState<TCheckResult extends RiskCheckResult>(input: {
  result: TCheckResult;
  workspace_path: string;
}): CurrentReviewState<TCheckResult> {
  const jobId = `check:${input.result.timestamp || 'current'}`;
  const findings = adaptCheckResultToFindings(input.result, {
    jobId,
    evidencePrefix: 'check',
  });

  return {
    check: input.result,
    findings,
    summary: buildCheckRiskSummary(findings),
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
