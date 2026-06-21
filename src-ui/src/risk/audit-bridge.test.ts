import {
  buildApprovalAuditPayload,
  buildReviewAuditPayload,
  summarizeRecentAuditEntries,
} from './audit-bridge';
import { adaptCheckResultToFindings, type RiskCheckResult } from './check-adapter';

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
  const payload = buildReviewAuditPayload(sample, findings, '/mock/nebula-project');

  assert.equal(payload.action, 'denied');
  assert.equal(payload.tool, 'review_check');
  assert.equal(payload.target_path, '/mock/nebula-project');
});

test('buildReviewAuditPayload carries finding and evidence references into details', () => {
  const findings = adaptCheckResultToFindings(sample, {
    jobId: 'job-1',
    evidencePrefix: 'check',
  });
  const payload = buildReviewAuditPayload(sample, findings, '/mock/nebula-project');

  assert.deepEqual(payload.details.finding_ids, ['job-1:l5:0']);
  assert.deepEqual(payload.details.evidence_ids, ['check:l5:0']);
  assert.equal(payload.details.counts.critical, 1);
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

test('summarizeRecentAuditEntries keeps recent review and approval events in newest-first order', () => {
  const rows = summarizeRecentAuditEntries([
    {
      ts: '2026-06-20T00:00:01Z',
      tool: 'review_check',
      path: '/mock/nebula-project',
      action: 'denied',
      reason: 'Review check found 1 finding(s).',
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
      action: 'allowed',
      reason: 'Repair patch applied.',
    },
    {
      ts: '2026-06-20T00:00:00Z',
      tool: 'read',
      path: '/tmp/x',
      action: 'allowed',
      reason: '',
    },
  ]);

  assert.equal(rows.length, 3);
  assert.equal(rows[0]?.toolLabel, '修复');
  assert.equal(rows[1]?.toolLabel, '审批');
  assert.equal(rows[2]?.toolLabel, '审查');
  assert.equal(rows[1]?.subject, 'src/auth.ts');
});
