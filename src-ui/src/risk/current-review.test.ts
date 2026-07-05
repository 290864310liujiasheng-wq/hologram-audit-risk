import {
  attachLiveRepairReadinessToCurrentReview,
  attachProviderReadinessToCurrentReview,
  attachRepairIssueToCurrentReview,
  attachRepairExecutionIssueToCurrentReview,
  attachRepairPreflightIssueToCurrentReview,
  attachRepairProposalToCurrentReview,
  buildRepairWorkbenchSnapshot,
  buildWorkbenchQueue,
  buildCurrentReviewSummaryResponse,
  buildCurrentReviewState,
} from './current-review';
import { buildRulePolicySnapshotId, resolveRulePolicy } from './rule-package';
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
  assert.equal(state.gate_decision?.policy_snapshot_id, buildRulePolicySnapshotId({
    plane: 'review',
  }));
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

test('current review accepts a workspace review policy override for external delivery surfaces', () => {
  const policy = resolveRulePolicy({
    plane: 'review',
    extension_packages: [{
      package_id: 'review.workspace',
      version: 'v2',
      plane: 'review',
      source: 'workspace_extension',
      enabled: true,
      description: 'workspace override',
      rules: [{
        rule_id: 'check.l5',
        package_id: 'review.workspace',
        name: 'workspace l5 override',
        category: 'security',
        severity: 'high',
        priority: 5,
        scope: ['file_write'],
        trigger: {
          kind: 'static_signal',
          config: {},
        },
        gate_effect: 'warn',
        enabled: true,
      }],
    }],
  });
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
    review_policy: policy,
  });

  assert.equal(state.gate_decision?.decision, 'warn');
  assert.equal(state.gate_decision?.policy_snapshot_id, 'policy:review:review.default@v1+review.workspace@v2');
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
    repair_proposal_validation: {
      secondary_audit: {
        passed: true,
        summary: '✅ 二次审计通过',
      },
      syntax_check: {
        passed: true,
        summary: '✅ 语法检查通过',
      },
      logic_change: {
        summary: '⚠️ 逻辑变更提示：提案会改动 1 个文件并触达 1 条高风险 finding，请在审批前人工复核业务语义。',
      },
      blocked: false,
    },
  });

  assert.equal(next.repair_generation_meta?.model, 'claude-sonnet-4-6');
  assert.equal(next.repair_proposal_validation?.secondary_audit.summary, '✅ 二次审计通过');
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
    repair_proposal_validation: {
      secondary_audit: {
        passed: true,
        summary: '✅ 二次审计通过',
      },
      syntax_check: {
        passed: true,
        summary: '✅ 语法检查通过',
      },
      logic_change: {
        summary: '⚠️ 逻辑变更提示：提案会改动 1 个文件并触达 1 条高风险 finding，请在审批前人工复核业务语义。',
      },
      blocked: false,
    },
  });

  const response = buildCurrentReviewSummaryResponse(next);

  assert.equal(response.status, 'ok');
  if (response.status === 'ok') {
    assert.equal(response.review.gate_decision.decision, 'block');
    assert.equal(response.review.repair_generation_meta?.provider_name, 'anthropic');
    assert.equal(response.review.repair_proposal_validation?.syntax_check.summary, '✅ 语法检查通过');
    assert.equal(response.workbench_queue[0]?.step_id, 'review');
    assert.equal(response.repair_history.length, 0);
    assert.equal(response.repair_workbench.status_state, 'draft');
  }
});

test('buildRepairWorkbenchSnapshot exposes explicit proposal validation lines for the UI', () => {
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
    repair_proposal_validation: {
      secondary_audit: {
        passed: true,
        summary: '✅ 二次审计通过',
      },
      syntax_check: {
        passed: true,
        summary: '✅ 语法检查通过',
      },
      logic_change: {
        summary: '⚠️ 逻辑变更提示：提案会改动 1 个文件并触达 1 条高风险 finding，请在审批前人工复核业务语义。',
      },
      blocked: false,
    },
  });

  const snapshot = buildRepairWorkbenchSnapshot(withProposal);

  assert.equal(snapshot.proposal_validation?.secondary_audit, '✅ 二次审计通过');
  assert.equal(snapshot.proposal_validation?.syntax_check, '✅ 语法检查通过');
  assert.equal(snapshot.proposal_validation?.logic_change, '⚠️ 逻辑变更提示：提案会改动 1 个文件并触达 1 条高风险 finding，请在审批前人工复核业务语义。');
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
    assert.equal(response.workbench_queue.length, 5);
  }
});

test('buildCurrentReviewSummaryResponse includes repair history when audit records are provided', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });

  const response = buildCurrentReviewSummaryResponse(state, [
    {
      event_id: 'repair:1',
      timestamp: '2026-06-22T00:00:00Z',
      plane: 'repair',
      stage: 'preflight',
      status: 'failed',
      subject: 'job-1:repair:proposal',
      reason: 'Repair preflight failed.',
      finding_ids: ['finding-1'],
      evidence_ids: ['evidence-1'],
      state_change: {
        from_state: 'approved',
        to_state: 'approved',
      },
      error: {
        code: 'policy_blocked',
        stage: 'preflight',
        retryable: false,
      },
      raw: {
        ts: '2026-06-22T00:00:00Z',
        tool: 'repair_apply',
        path: '/tmp/workspace',
        action: 'denied',
        reason: 'Repair preflight failed.',
        details: {},
      },
    },
  ]);

  assert.equal(response.status, 'ok');
  if (response.status === 'ok') {
    assert.equal(response.repair_history[0]?.stage, 'preflight');
    assert.equal(response.workbench_queue[4]?.state, 'failed');
    assert.equal(response.repair_workbench.repair_history[0]?.stage, 'preflight');
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

test('buildWorkbenchQueue exposes the review-to-repair main path with contract-backed states', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });

  const queue = buildWorkbenchQueue(state);

  assert.equal(queue[0]?.step_id, 'review');
  assert.equal(queue[0]?.state, 'needs_attention');
  assert.equal(queue[0]?.section_id, 'risk-summary');
  assert.equal(queue[1]?.state, 'block');
  assert.equal(queue[1]?.section_id, 'gate-decision');
  assert.equal(queue[2]?.state, 'ready');
  assert.equal(queue[2]?.section_id, 'repair-workbench');
  assert.equal(queue[3]?.state, 'draft');
  assert.equal(queue[4]?.state, 'draft');
});

test('buildWorkbenchQueue prefers audit-backed repair history when present', () => {
  const state = buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  });

  const queue = buildWorkbenchQueue(state, [
    {
      event_id: 'repair:1',
      timestamp: '2026-06-22T00:00:00Z',
      plane: 'repair',
      stage: 'preflight',
      status: 'failed',
      subject: 'job-1:repair:proposal',
      reason: 'Repair preflight failed.',
      finding_ids: ['finding-1'],
      evidence_ids: ['evidence-1'],
      state_change: {
        from_state: 'approved',
        to_state: 'approved',
      },
      error: {
        code: 'policy_blocked',
        stage: 'preflight',
        retryable: false,
      },
      raw: {
        ts: '2026-06-22T00:00:00Z',
        tool: 'repair_apply',
        path: '/tmp/workspace',
        action: 'denied',
        reason: 'Repair preflight failed.',
        details: {},
      },
    },
  ]);

  assert.equal(queue[4]?.state, 'failed');
  assert.equal(queue[4]?.detail, 'approved -> approved');
});

test('buildWorkbenchQueue keeps empty-state steps out of missing/error wording when review is clean', () => {
  const state = buildCurrentReviewState({
    result: {
      ...sample,
      passed: true,
      l5_violations: [],
      changed_files: [],
      total_changed_files: 0,
    },
    workspace_path: '/tmp/workspace',
  });

  const queue = buildWorkbenchQueue(state);

  assert.equal(queue[0]?.state, 'clean');
  assert.equal(queue[1]?.state, 'allow');
  assert.equal(queue[2]?.state, 'not_required');
  assert.equal(queue[3]?.state, 'not_required');
  assert.equal(queue[4]?.state, 'not_required');
});

test('buildWorkbenchQueue surfaces retryable repair degradation as degraded instead of draft', () => {
  const state = attachRepairIssueToCurrentReview(buildCurrentReviewState({
    result: sample,
    workspace_path: '/tmp/workspace',
  }), {
    issue: {
      issue_id: 'job-1:repair:proposal_generation',
      repair_plan_id: 'job-1:repair',
      stage: 'proposal_generation',
      summary: 'Provider 暂时不可用，修复提案已降级。',
      error: {
        code: 'provider_upstream_failed',
        message: '503',
        retryable: true,
      },
      created_at: '2026-06-22T00:00:00Z',
    },
  });

  const queue = buildWorkbenchQueue(state);

  assert.equal(queue[4]?.state, 'degraded');
});

test('buildRepairWorkbenchSnapshot keeps clean reviews in a not-required empty state', () => {
  const state = buildCurrentReviewState({
    result: {
      ...sample,
      passed: true,
      l5_violations: [],
      changed_files: [],
      total_changed_files: 0,
    },
    workspace_path: '/tmp/workspace',
  });

  const snapshot = buildRepairWorkbenchSnapshot(state, []);

  assert.equal(snapshot.status_state, 'not_required');
  assert.equal(snapshot.status_label, '当前无可修复风险');
  assert.equal(snapshot.evidence_trace.repair_history_count, 0);
});

test('buildRepairWorkbenchSnapshot surfaces retryable issues, preflight summary, and repair history', () => {
  const state = attachRepairPreflightIssueToCurrentReview(
    attachRepairIssueToCurrentReview(buildCurrentReviewState({
      result: sample,
      workspace_path: '/tmp/workspace',
    }), {
      issue: {
        issue_id: 'job-1:repair:proposal_generation',
        repair_plan_id: 'job-1:repair',
        stage: 'proposal_generation',
        summary: 'Provider 暂时不可用，修复提案已降级。',
        error: {
          code: 'provider_upstream_failed',
          message: '503',
          retryable: true,
        },
        created_at: '2026-06-22T00:00:00Z',
      },
    }),
    {
      issue: {
        issue_id: 'job-1:repair:preflight',
        repair_plan_id: 'job-1:repair',
        stage: 'preflight',
        summary: '修复前复检失败：修复前验证命令必须全部通过',
        error: {
          code: 'policy_blocked',
          message: '修复前验证命令必须全部通过',
          retryable: false,
        },
        created_at: '2026-06-22T00:00:00Z',
      },
      preflight: {
        repair_plan_id: 'job-1:repair',
        findings: [{
          finding_id: 'finding-1',
          job_id: 'job-1',
          rule_id: 'repair.test.required_command_failed',
          severity: 'critical',
          category: 'repair_gate',
          locations: [{ file_path: 'src/auth.ts', start_line: 1, end_line: 1 }],
          plain_explanation: '必跑验证命令失败：git diff --check',
          impact: '当前 patch proposal 无法证明它在 apply 前仍满足最小安全门。',
          recommendation: '先修复命令失败原因，再重新执行 git diff --check。',
          evidence_ids: ['repair-test:1'],
          confidence: 0.99,
          status: 'open',
        }],
        gate_decision: {
          decision_id: 'decision-1',
          job_id: 'job-1',
          subject_type: 'repair_apply',
          subject_ref: 'job-1:repair:proposal',
          decision: 'block',
          reason: '修复前验证命令必须全部通过',
          finding_ids: ['finding-1'],
          policy_snapshot_id: 'policy:repair:repair.default@v1',
          decided_at: '2026-06-22T00:00:00Z',
        },
        test_results: [{ command: 'git diff --check', passed: false, stdout: '', stderr: 'failed' }],
      },
    },
  );

  const snapshot = buildRepairWorkbenchSnapshot(state, [
    {
      event_id: 'repair:1',
      timestamp: '2026-06-22T00:00:00Z',
      plane: 'repair',
      stage: 'preflight',
      status: 'failed',
      subject: 'job-1:repair:proposal',
      reason: 'Repair preflight failed.',
      finding_ids: ['finding-1'],
      evidence_ids: ['repair-test:1'],
      state_change: {
        from_state: 'approved',
        to_state: 'approved',
      },
      error: {
        code: 'policy_blocked',
        stage: 'preflight',
        retryable: false,
      },
      raw: {
        ts: '2026-06-22T00:00:00Z',
        tool: 'repair_apply',
        path: '/tmp/workspace',
        action: 'denied',
        reason: 'Repair preflight failed.',
        details: {},
      },
    },
  ]);

  assert.equal(snapshot.issue_badge, '提案失败，需修正');
  assert.equal(snapshot.preflight?.failed_commands[0], 'git diff --check');
  assert.equal(snapshot.repair_history.length, 1);
});
