import {
  attachLiveRepairReadinessToCurrentReview,
  attachProviderReadinessToCurrentReview,
  attachRepairIssueToCurrentReview,
  attachRepairExecutionIssueToCurrentReview,
  attachRepairPreflightIssueToCurrentReview,
  attachRepairProposalToCurrentReview,
  buildCurrentReviewSummaryResponse,
  buildCurrentReviewState,
} from './current-review';
import { type RiskCheckResult } from './check-adapter';
import type { RepairIssue } from './review-core';

const assert = {
  equal(actual: unknown, expected: unknown): void {
    if (actual !== expected) {
      throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
    }
  },
};

function test(name: string, fn: () => void): void {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (error) {
    console.error(`not ok - ${name}`);
    throw error;
  }
}

const sample: RiskCheckResult = {
  passed: false,
  timestamp: '2026-06-20T00:00:00Z',
  changed_files: ['src/auth.ts'],
  total_changed_files: 1,
  l5_violations: [{
    signal: { description: '写入敏感文件', file_path: 'src/auth.ts', line: 42 },
    message: '检测到危险写入',
    level: 5,
  }],
  l4_violations: [],
  l3_violations: [],
  l2_violations: [],
};

test('current review summary can be derived from latest check result', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });

  assert.equal(state.summary.total, 1);
  assert.equal(state.summary.topFindings[0]?.plain_explanation, '写入敏感文件');
  assert.equal(state.multi_agent_review.merged_findings.length, 1);
  assert.equal(state.repair_plan.approval_state, 'draft');
  assert.equal(state.gate_decision?.decision, 'block');
});

test('buildCurrentReviewSummaryResponse returns empty when no review exists', () => {
  const response = buildCurrentReviewSummaryResponse(null);

  assert.equal(response.status, 'empty');
});

test('attachRepairIssueToCurrentReview exposes retryable provider degradation to the UI state', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });
  const issue: RepairIssue = {
    issue_id: 'job-1:repair:proposal_generation',
    repair_plan_id: state.repair_plan.repair_plan_id,
    stage: 'proposal_generation',
    summary: 'Provider 暂时不可用，修复提案已降级为可重试状态。',
    error: {
      code: 'provider_unavailable',
      message: 'Repair planner provider is unavailable.',
      retryable: true,
    },
    created_at: '2026-06-21T00:00:00Z',
  };

  const next = attachRepairIssueToCurrentReview(state, {
    issue,
    repair_generation_meta: {
      repair_plan_id: state.repair_plan.repair_plan_id,
      provider_name: 'anthropic',
      model: 'claude-sonnet-4-6',
      file_count: 1,
      focus_file_paths: ['src/auth.ts'],
      high_severity_finding_ids: ['finding-1'],
      generated_at: '2026-06-21T00:00:00Z',
    },
  });

  assert.equal(next.repair_issue?.error.code, 'provider_unavailable');
  assert.equal(next.repair_issue?.error.retryable, true);
  assert.equal(next.repair_generation_meta?.provider_name, 'anthropic');
});

test('attachProviderReadinessToCurrentReview exposes missing live provider prerequisites to the UI state', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });
  const next = attachProviderReadinessToCurrentReview(state, {
    provider_name: 'deepseek',
    model: 'deepseek-v4-pro',
    source: 'missing',
    ready: false,
    reason: 'No provider API key available in settings or secure storage.',
    has_inline_key: false,
    has_secure_store_key: false,
  });

  assert.equal(next.provider_readiness?.ready, false);
  assert.equal(next.provider_readiness?.source, 'missing');
});

test('current review derives require_approval when the strongest finding is high severity', () => {
  const state = buildCurrentReviewState({
    result: {
      ...sample,
      l5_violations: [],
      l4_violations: [{
        signal: { description: '高风险权限变更', file_path: 'src/auth.ts', line: 12 },
        message: '需要审批',
        level: 4,
      }],
    },
    workspace_path: '/tmp/workspace',
  });

  assert.equal(state.gate_decision?.decision, 'require_approval');
});

test('attachRepairProposalToCurrentReview preserves generation metadata for the UI', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });
  const next = attachRepairProposalToCurrentReview(state, {
    repair_plan: state.repair_plan,
    patch_proposal: {
      patch_proposal_id: 'job-1:repair:proposal',
      repair_plan_id: state.repair_plan.repair_plan_id,
      summary: '修复风险',
      rationale: '缩小 patch 面',
      generated_at: '2026-06-21T00:00:00Z',
      operations: [{
        operation_id: 'op-1',
        file_path: 'src/auth.ts',
        new_content: 'export const fixed = true;',
        summary: 'tighten guard',
      }],
    },
    repair_generation_meta: {
      repair_plan_id: state.repair_plan.repair_plan_id,
      provider_name: 'anthropic',
      model: 'claude-sonnet-4-6',
      file_count: 1,
      focus_file_paths: ['src/auth.ts'],
      high_severity_finding_ids: ['finding-1'],
      generated_at: '2026-06-21T00:00:00Z',
    },
  });

  assert.equal(next.repair_generation_meta?.model, 'claude-sonnet-4-6');
});

test('attachRepairPreflightIssueToCurrentReview keeps the proposal visible while surfacing the issue', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });
  const withProposal = attachRepairProposalToCurrentReview(state, {
    repair_plan: state.repair_plan,
    patch_proposal: {
      patch_proposal_id: 'job-1:repair:proposal',
      repair_plan_id: state.repair_plan.repair_plan_id,
      summary: '修复风险',
      rationale: '缩小 patch 面',
      generated_at: '2026-06-21T00:00:00Z',
      operations: [{
        operation_id: 'op-1',
        file_path: 'src/auth.ts',
        new_content: 'export const fixed = true;',
        summary: 'tighten guard',
      }],
    },
    repair_generation_meta: {
      repair_plan_id: state.repair_plan.repair_plan_id,
      provider_name: 'anthropic',
      model: 'claude-sonnet-4-6',
      file_count: 1,
      focus_file_paths: ['src/auth.ts'],
      high_severity_finding_ids: ['finding-1'],
      generated_at: '2026-06-21T00:00:00Z',
    },
  });
  const next = attachRepairPreflightIssueToCurrentReview(withProposal, {
    issue: {
      issue_id: 'job-1:repair:preflight',
      repair_plan_id: state.repair_plan.repair_plan_id,
      stage: 'preflight',
      summary: '修复前复检失败：修复 patch 超出当前 finding 范围。',
      error: {
        code: 'policy_blocked',
        message: '修复 patch 超出当前 finding 范围。',
        retryable: false,
      },
      created_at: '2026-06-21T00:00:00Z',
    },
    preflight: {
      repair_plan_id: state.repair_plan.repair_plan_id,
      findings: state.findings,
      gate_decision: state.gate_decision,
      test_results: [{ command: 'npm run test:risk', passed: false, stdout: '', stderr: 'failed' }],
    },
  });

  assert.equal(next.patch_proposal?.patch_proposal_id, 'job-1:repair:proposal');
  assert.equal(next.repair_issue?.stage, 'preflight');
  assert.equal(next.repair_preflight?.test_results[0]?.command, 'npm run test:risk');
});

test('attachRepairExecutionIssueToCurrentReview keeps rollback evidence for the UI', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });
  const withPreflight = attachRepairPreflightIssueToCurrentReview(state, {
    issue: {
      issue_id: 'job-1:repair:preflight',
      repair_plan_id: state.repair_plan.repair_plan_id,
      stage: 'preflight',
      summary: '修复前复检失败：修复 patch 超出当前 finding 范围。',
      error: {
        code: 'policy_blocked',
        message: '修复 patch 超出当前 finding 范围。',
        retryable: false,
      },
      created_at: '2026-06-21T00:00:00Z',
    },
    preflight: {
      repair_plan_id: state.repair_plan.repair_plan_id,
      findings: state.findings,
      gate_decision: state.gate_decision,
      test_results: [{ command: 'npm run test:risk', passed: false, stdout: '', stderr: 'failed' }],
    },
  });
  const next = attachRepairExecutionIssueToCurrentReview(withPreflight, {
    issue: {
      issue_id: 'job-1:repair:apply',
      repair_plan_id: state.repair_plan.repair_plan_id,
      stage: 'apply',
      summary: '修复应用失败，已自动回滚。',
      error: {
        code: 'internal_error',
        message: 'disk full',
        retryable: true,
      },
      created_at: '2026-06-21T00:00:00Z',
    },
    rollback: {
      rollback_id: 'job-1:repair:rollback',
      repair_plan_id: state.repair_plan.repair_plan_id,
      files: [{ file_path: 'src/auth.ts', content: 'export const ok = false;' }],
      created_at: '2026-06-21T00:00:00Z',
    },
  });

  assert.equal(next.repair_issue?.stage, 'apply');
  assert.equal(next.rollback?.rollback_id, 'job-1:repair:rollback');
  assert.equal(next.repair_preflight, undefined);
  assert.equal(next.repair_plan.approval_state, 'rolled_back');
});

test('buildCurrentReviewSummaryResponse includes the gate decision and generation metadata', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });
  const next = attachRepairProposalToCurrentReview(state, {
    repair_plan: state.repair_plan,
    patch_proposal: {
      patch_proposal_id: 'job-1:repair:proposal',
      repair_plan_id: state.repair_plan.repair_plan_id,
      summary: '修复风险',
      rationale: '缩小 patch 面',
      generated_at: '2026-06-21T00:00:00Z',
      operations: [{
        operation_id: 'op-1',
        file_path: 'src/auth.ts',
        new_content: 'export const fixed = true;',
        summary: 'tighten guard',
      }],
    },
    repair_generation_meta: {
      repair_plan_id: state.repair_plan.repair_plan_id,
      provider_name: 'anthropic',
      model: 'claude-sonnet-4-6',
      file_count: 1,
      focus_file_paths: ['src/auth.ts'],
      high_severity_finding_ids: ['finding-1'],
      generated_at: '2026-06-21T00:00:00Z',
    },
  });

  const response = buildCurrentReviewSummaryResponse(next);

  assert.equal(response.status, 'ok');
  if (response.status === 'ok') {
    assert.equal(response.review.gate_decision.decision, 'block');
    assert.equal(response.review.repair_generation_meta?.provider_name, 'anthropic');
  }
});

test('buildCurrentReviewSummaryResponse includes provider readiness when attached', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });
  const next = attachProviderReadinessToCurrentReview(state, {
    provider_name: 'anthropic',
    model: 'claude-sonnet-4-6',
    source: 'secure_store',
    ready: true,
    reason: 'Provider API key can be restored from secure storage.',
    has_inline_key: false,
    has_secure_store_key: true,
  });

  const response = buildCurrentReviewSummaryResponse(next);

  assert.equal(response.status, 'ok');
  if (response.status === 'ok') {
    assert.equal(response.review.provider_readiness?.source, 'secure_store');
    assert.equal(response.review.provider_readiness?.ready, true);
  }
});

test('attachLiveRepairReadinessToCurrentReview exposes mock-browser runtime mismatch to the UI state', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/mock/nebula-project',
  });
  const next = attachLiveRepairReadinessToCurrentReview(state, {
    mode: 'mock_browser',
    eligible: false,
    reason: 'Current session is using mock browser data, so live repair evidence cannot be produced here.',
    workspace_path: '/mock/nebula-project',
    provider: {
      provider_name: 'deepseek',
      model: 'deepseek-v4-pro',
      source: 'inline',
      ready: true,
      reason: 'Provider API key is available in settings.',
      has_inline_key: true,
      has_secure_store_key: false,
    },
  });

  assert.equal(next.live_repair_readiness?.mode, 'mock_browser');
  assert.equal(next.live_repair_readiness?.eligible, false);
  assert.equal(next.provider_readiness?.ready, true);
});
