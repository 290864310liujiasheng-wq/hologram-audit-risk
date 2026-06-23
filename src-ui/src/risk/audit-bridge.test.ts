import {
  buildRepairHistory,
  buildAuditQueryResult,
  buildApprovalAuditPayload,
  buildRepairAuditPayload,
  buildReviewAuditPayload,
  summarizeRecentAuditEntries,
} from './audit-bridge';
import { adaptCheckResultToFindings, type RiskCheckResult } from './check-adapter';
import type { GateDecision } from './review-core';
import { buildRulePolicySnapshotId } from './rule-package';

const assert = {
  equal(actual: unknown, expected: unknown): void {
    if (actual !== expected) {
      throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
    }
  },
  deepEqual(actual: unknown, expected: unknown): void {
    const actualJson = JSON.stringify(actual);
    const expectedJson = JSON.stringify(expected);
    if (actualJson !== expectedJson) {
      throw new Error(`Expected ${expectedJson}, got ${actualJson}`);
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
    signal: {
      description: '写入敏感文件',
      file_path: 'src/auth.ts',
      line: 42,
    },
    message: '检测到危险写入',
    level: 5,
  }],
  l4_violations: [],
  l3_violations: [],
  l2_violations: [],
};

test('buildReviewAuditPayload marks failed checks as denied review actions', () => {
  const findings = adaptCheckResultToFindings(sample, {
    jobId: 'job-1',
    evidencePrefix: 'check',
  });
  const payload = buildReviewAuditPayload(sample, findings, '/mock/nebula-project', {
    decision_id: 'decision-1',
    job_id: 'job-1',
    subject_type: 'file_write',
    subject_ref: '/mock/nebula-project',
    decision: 'block',
    reason: 'L5 不可逆风险默认阻断',
    finding_ids: ['job-1:l5:0'],
    policy_snapshot_id: buildRulePolicySnapshotId({ plane: 'review' }),
    decided_at: '2026-06-20T00:00:00Z',
  });

  assert.equal(payload.action, 'denied');
  assert.equal(payload.tool, 'review_check');
  assert.equal(payload.target_path, '/mock/nebula-project');
});

test('buildReviewAuditPayload keeps warn decisions as allowed actions with gate reason', () => {
  const findings = adaptCheckResultToFindings({
    ...sample,
    l5_violations: [],
    l4_violations: [],
    l3_violations: [{
      signal: { description: '回归风险扩大', file_path: 'src/test.ts', line: 7 },
      message: '可能缺少验证',
      level: 3,
    }],
  }, {
    jobId: 'job-2',
    evidencePrefix: 'check',
  });
  const payload = buildReviewAuditPayload({
    ...sample,
    passed: false,
    l5_violations: [],
    l4_violations: [],
    l3_violations: [{
      signal: { description: '回归风险扩大', file_path: 'src/test.ts', line: 7 },
      message: '可能缺少验证',
      level: 3,
    }],
  }, findings, '/mock/nebula-project', {
    decision_id: 'decision-2',
    job_id: 'job-2',
    subject_type: 'file_write',
    subject_ref: '/mock/nebula-project',
    decision: 'warn',
    reason: 'L3 延迟风险需要告警',
    finding_ids: ['job-2:l3:0'],
    policy_snapshot_id: buildRulePolicySnapshotId({ plane: 'review' }),
    decided_at: '2026-06-20T00:00:00Z',
  });

  assert.equal(payload.action, 'allowed');
  assert.equal(payload.reason, 'L3 延迟风险需要告警');
});

test('buildReviewAuditPayload carries finding and evidence references into details', () => {
  const findings = adaptCheckResultToFindings(sample, {
    jobId: 'job-1',
    evidencePrefix: 'check',
  });
  const payload = buildReviewAuditPayload(sample, findings, '/mock/nebula-project', {
    decision_id: 'decision-1',
    job_id: 'job-1',
    subject_type: 'file_write',
    subject_ref: '/mock/nebula-project',
    decision: 'block',
    reason: 'L5 不可逆风险默认阻断',
    finding_ids: ['job-1:l5:0'],
    policy_snapshot_id: buildRulePolicySnapshotId({ plane: 'review' }),
    decided_at: '2026-06-20T00:00:00Z',
  } satisfies GateDecision);

  assert.deepEqual(payload.details.finding_ids, ['job-1:l5:0']);
  assert.deepEqual(payload.details.evidence_ids, ['check:l5:0']);
  assert.equal(payload.details.counts.critical, 1);
  assert.equal(payload.details.gate_decision?.decision, 'block');
  assert.equal(payload.details.policy_snapshot_id, buildRulePolicySnapshotId({ plane: 'review' }));
});

test('buildApprovalAuditPayload marks denied approvals and preserves subject context', () => {
  const payload = buildApprovalAuditPayload({
    workspacePath: '/mock/nebula-project',
    toolName: 'write_file_content',
    subject: 'src/auth.ts',
    allow: false,
    remember: false,
  });

  assert.equal(payload.action, 'denied');
  assert.equal(payload.tool, 'approval.write_file_content');
  assert.equal(payload.details.subject, 'src/auth.ts');
});

test('buildRepairAuditPayload stamps timestamp and keeps structured generation metadata', () => {
  const payload = buildRepairAuditPayload({
    tool: 'repair_plan',
    workspacePath: '/mock/nebula-project',
    action: 'allowed',
    reason: 'Repair proposal generated.',
    now: '2026-06-21T00:00:00Z',
    details: {
      approval_state: 'waiting_approval',
      patch_proposal_id: 'job-1:repair:proposal',
      generation_meta: {
        repair_plan_id: 'job-1:repair',
        provider_name: 'anthropic',
        model: 'claude-sonnet-4-6',
        file_count: 2,
        focus_file_paths: ['src/a.ts', 'src/b.ts'],
        high_severity_finding_ids: ['finding-1'],
        generated_at: '2026-06-21T00:00:00Z',
      },
    },
  });

  assert.equal(payload.details.timestamp, '2026-06-21T00:00:00Z');
  assert.equal(payload.details.generation_meta?.provider_name, 'anthropic');
});

test('buildRepairAuditPayload keeps repair issue stage and retryability for degraded proposal runs', () => {
  const payload = buildRepairAuditPayload({
    tool: 'repair_plan',
    workspacePath: '/mock/nebula-project',
    action: 'denied',
    reason: 'Repair proposal generation degraded.',
    now: '2026-06-21T00:00:00Z',
    details: {
      approval_state: 'draft',
      error_code: 'proxy_rejected',
      error_stage: 'proposal_generation',
      error_retryable: true,
    },
  });

  assert.equal(payload.details.error_code, 'proxy_rejected');
  assert.equal(payload.details.error_stage, 'proposal_generation');
  assert.equal(payload.details.error_retryable, true);
});

test('buildAuditQueryResult normalizes review, approval, and repair audit entries into one evidence timeline', () => {
  const result = buildAuditQueryResult({
    entries: [
      {
        ts: '2026-06-20T00:00:01Z',
        tool: 'review_check',
        path: '/mock/nebula-project',
        action: 'denied',
        reason: 'L5 不可逆风险默认阻断',
        details: {
          finding_ids: ['job-1:l5:0'],
          evidence_ids: ['check:l5:0'],
          gate_decision: {
            decision: 'block',
            reason: 'L5 不可逆风险默认阻断',
            finding_ids: ['job-1:l5:0'],
          },
          policy_snapshot_id: buildRulePolicySnapshotId({ plane: 'review' }),
        },
      },
      {
        ts: '2026-06-20T00:00:02Z',
        tool: 'repair_apply',
        path: '/mock/nebula-project',
        action: 'denied',
        reason: 'Repair preflight failed.',
        details: {
          approval_state: 'approved',
          gate_decision: 'block',
          gate_reason: '修复前验证命令必须全部通过',
          error_code: 'policy_blocked',
          error_stage: 'preflight',
          error_retryable: false,
          preflight_findings: [{ finding_id: 'finding-1', rule_id: 'repair.test.required_command_failed' }],
          validation_results: [{ command: 'git diff --check', passed: false, stdout: '', stderr: 'failed' }],
          state_change: {
            from_state: 'approved',
            to_state: 'approved',
          },
        },
      },
    ],
  });

  assert.equal(result.records.length, 2);
  assert.equal(result.records[0]?.plane, 'repair');
  assert.equal(result.records[0]?.stage, 'preflight');
  assert.equal(result.records[0]?.error?.code, 'policy_blocked');
  assert.equal(result.records[0]?.state_change?.from_state, 'approved');
  assert.deepEqual(result.records[1]?.evidence_ids, ['check:l5:0']);
  assert.equal(result.records[1]?.policy_snapshot_id, buildRulePolicySnapshotId({ plane: 'review' }));
});

test('summarizeRecentAuditEntries keeps recent review and approval events in newest-first order', () => {
  const rows = summarizeRecentAuditEntries(buildAuditQueryResult({
    entries: [
    {
      ts: '2026-06-20T00:00:01Z',
      tool: 'review_check',
      path: '/mock/nebula-project',
      action: 'denied',
      reason: 'L5 不可逆风险默认阻断',
      details: {
        gate_decision: {
          decision: 'block',
          reason: 'L5 不可逆风险默认阻断',
          finding_ids: ['job-1:l5:0'],
        },
      },
    },
    {
      ts: '2026-06-20T00:00:02Z',
      tool: 'approval.write_file_content',
      path: '/mock/nebula-project',
      action: 'allowed',
      reason: 'User approved tool execution.',
      details: { subject: 'src/auth.ts', remember: false },
    },
    {
      ts: '2026-06-20T00:00:03Z',
      tool: 'repair_apply',
      path: '/mock/nebula-project',
      action: 'denied',
      reason: 'Repair preflight failed.',
      details: {
        gate_decision: 'block',
      },
    },
    {
      ts: '2026-06-20T00:00:00Z',
      tool: 'read',
      path: '/tmp/x',
      action: 'allowed',
      reason: '',
    },
    ],
  }).records);

  assert.equal(rows.length, 3);
  assert.equal(rows[0]?.toolLabel, '修复');
  assert.equal(rows[0]?.actionLabel, '阻断');
  assert.equal(rows[1]?.toolLabel, '审批');
  assert.equal(rows[2]?.toolLabel, '审查');
  assert.equal(rows[1]?.subject, 'src/auth.ts');
  assert.equal(rows[2]?.actionLabel, '阻断');
});

test('buildRepairHistory keeps newest repair stages with state changes and retryability', () => {
  const query = buildAuditQueryResult({
    entries: [
      {
        ts: '2026-06-20T00:00:02Z',
        tool: 'repair_apply',
        path: '/mock/nebula-project',
        action: 'denied',
        reason: 'Repair preflight failed.',
        details: {
          approval_state: 'approved',
          gate_decision: 'block',
          gate_reason: '修复前验证命令必须全部通过',
          error_code: 'policy_blocked',
          error_stage: 'preflight',
          error_retryable: false,
          state_change: {
            from_state: 'approved',
            to_state: 'approved',
          },
        },
      },
      {
        ts: '2026-06-20T00:00:01Z',
        tool: 'repair_plan',
        path: '/mock/nebula-project',
        action: 'allowed',
        reason: 'Repair proposal generated.',
        details: {
          approval_state: 'waiting_approval',
          state_change: {
            from_state: 'draft',
            to_state: 'waiting_approval',
          },
        },
      },
    ],
  });

  const history = buildRepairHistory(query.records);

  assert.equal(history.length, 2);
  assert.equal(history[0]?.stage, 'preflight');
  assert.equal(history[0]?.status, 'failed');
  assert.equal(history[0]?.state_change?.from_state, 'approved');
  assert.equal(history[1]?.status, 'waiting_approval');
});
