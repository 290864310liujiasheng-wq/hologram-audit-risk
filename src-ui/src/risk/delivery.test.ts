import { createHash } from 'crypto';
import {
  buildDeliveryDoctorReport,
  buildDeliveryInitFiles,
  buildDeliveryMachineReport,
  buildDeliveryRuleSummaries,
  createDefaultDeliveryConfig,
  resolveDeliveryPolicies,
  searchDeliveryAuditRecords,
  shouldFailDeliveryGate,
  validateDeliveryConfig,
} from './delivery';
import type { RiskCheckResult } from './check-adapter';

const assert = {
  equal(actual: unknown, expected: unknown): void {
    if (actual !== expected) {
      throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
    }
  },
  ok(value: unknown, message: string): void {
    if (!value) {
      throw new Error(message);
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

const sampleCheck: RiskCheckResult = {
  passed: false,
  timestamp: '2026-06-23T00:00:00Z',
  changed_files: ['src/auth.ts'],
  total_changed_files: 1,
  l5_violations: [{
    signal: { description: '高风险权限写入', file_path: 'src/auth.ts', line: 12 },
    message: 'block this',
    level: 5,
  }],
  l4_violations: [],
  l3_violations: [],
  l2_violations: [],
};

test('validateDeliveryConfig rejects env-backed provider config without env var', () => {
  const config = createDefaultDeliveryConfig('/tmp/workspace');
  config.provider.env_var = '';

  let error = '';
  try {
    validateDeliveryConfig(config);
  } catch (cause) {
    error = String(cause);
  }

  assert.ok(error.includes('provider.env_var'), 'expected env validation error');
});

test('createDefaultDeliveryConfig includes auth base_url placeholder', () => {
  const config = createDefaultDeliveryConfig('/tmp/workspace');

  assert.equal(config.auth.base_url, '');
});

test('resolveDeliveryPolicies loads workspace rule packages and disabled rule ids into one policy source', () => {
  const workspaceRoot = '/tmp/hologram-phase5-policy';
  const config = createDefaultDeliveryConfig(workspaceRoot);
  const reviewJsonPath = `${workspaceRoot}/.review.json`;
  const files: Record<string, string> = {
    [reviewJsonPath]: JSON.stringify({
    package_id: 'review.workspace',
    version: 'v3',
    plane: 'review',
    source: 'workspace_extension',
    enabled: true,
    description: 'workspace review override',
    rules: [{
      rule_id: 'check.l5',
      package_id: 'review.workspace',
      name: 'workspace review override',
      category: 'security',
      severity: 'high',
      priority: 20,
      scope: ['file_write'],
      trigger: { kind: 'static_signal', config: {} },
      gate_effect: 'warn',
        enabled: true,
      }],
    }, null, 2),
  };
  config.rule_packages.review_paths = ['.review.json'];
  config.rule_packages.repair_paths = [];

  const policies = resolveDeliveryPolicies({
    config,
    readFile: (path) => files[path],
  });

  assert.equal(policies.review.policy_snapshot_id, 'policy:review:review.default@v1+review.workspace@v3');
  const l5Rule = policies.review.rules.find((rule) => rule.rule_id === 'check.l5');
  assert.equal(l5Rule?.package_id, 'review.workspace');
});

test('buildDeliveryMachineReport returns machine-readable review, policy, audit, and automation status', () => {
  const workspaceRoot = '/tmp/workspace';
  const config = createDefaultDeliveryConfig(workspaceRoot);
  const auditEntries = buildChainedAuditEntries(workspaceRoot);
  const report = buildDeliveryMachineReport({
    config,
    checkResult: sampleCheck,
    auditEntries,
    generatedAt: '2026-06-23T00:00:00Z',
    env: {},
    readFile: () => JSON.stringify([]),
  });

  assert.equal(report.provider.ready, false);
  assert.equal(report.current_review.status, 'ok');
  if (report.current_review.status === 'ok') {
    assert.equal(report.current_review.review.gate_decision.decision, 'block');
  }
  assert.equal(report.audit.records[0]?.stage, 'preflight');
  assert.equal(report.automation.should_fail, true);
  assert.equal(report.audit.integrity.status, 'verified');
  assert.equal(report.audit.integrity.verified, true);
  assert.equal(report.audit.integrity.entry_count, 2);
  assert.equal(report.audit.integrity.last_hash, auditEntries[1]?.integrity_hash);
  assert.equal(report.report_signature.algorithm, 'sha256');
  assert.ok(report.report_signature.digest.length === 64, 'expected sha256 report digest');
});

test('shouldFailDeliveryGate respects the configured decision threshold', () => {
  assert.equal(shouldFailDeliveryGate({ decision: 'warn', threshold: 'block' }), false);
  assert.equal(shouldFailDeliveryGate({ decision: 'require_approval', threshold: 'warn' }), true);
});

test('buildDeliveryInitFiles emits delivery manifest, rule package stubs, hook, and CI workflow', () => {
  const files = buildDeliveryInitFiles({
    workspaceRoot: '/tmp/customer-repo',
    platformRoot: '/opt/hologram-platform',
  });

  assert.ok(Boolean(files['.hologram/delivery.json']), 'expected delivery config');
  const deliveryConfig = JSON.parse(files['.hologram/delivery.json']);
  assert.equal(deliveryConfig.auth.base_url, '');
  assert.ok(files['.githooks/pre-commit'].includes('audit-risk') || files['.githooks/pre-commit'].includes('--bin audit-risk'), 'expected pre-commit hook to call audit-risk report');
  assert.ok(files['.github/workflows/hologram-risk.yml'].includes('HOLOGRAM_PLATFORM_REPO'), 'expected workflow to declare platform repo env');
});

test('buildDeliveryRuleSummaries flattens policy state for admin rule inspection', () => {
  const report = buildDeliveryMachineReport({
    config: createDefaultDeliveryConfig('/tmp/workspace'),
    checkResult: sampleCheck,
    auditEntries: [],
    generatedAt: '2026-06-23T00:00:00Z',
    env: {},
    readFile: () => JSON.stringify([]),
  });

  const summaries = buildDeliveryRuleSummaries({ policies: report.policies });

  assert.equal(summaries[0]?.plane, 'review');
  assert.ok((summaries[0]?.rule_count || 0) > 0, 'expected review rules');
  assert.equal(summaries[1]?.plane, 'repair');
});

test('searchDeliveryAuditRecords filters normalized audit records for admin search', () => {
  const report = buildDeliveryMachineReport({
    config: createDefaultDeliveryConfig('/tmp/workspace'),
    checkResult: sampleCheck,
    auditEntries: [{
      ts: '2026-06-23T00:00:00Z',
      tool: 'repair_apply',
      path: '/tmp/workspace',
      action: 'denied',
      reason: 'Repair preflight failed.',
      details: {
        error_code: 'policy_blocked',
        error_stage: 'preflight',
        error_retryable: false,
        patch_proposal_id: 'proposal-1',
      },
    }],
    generatedAt: '2026-06-23T00:00:00Z',
    env: {},
    readFile: () => JSON.stringify([]),
  });

  const result = searchDeliveryAuditRecords({
    audit: report.audit,
    query: 'preflight',
  });

  assert.equal(result.total_matches, 1);
  assert.equal(result.records[0]?.stage, 'preflight');
});

test('buildDeliveryDoctorReport highlights provider and gate blockers for admin diagnosis', () => {
  const report = buildDeliveryMachineReport({
    config: createDefaultDeliveryConfig('/tmp/workspace'),
    checkResult: sampleCheck,
    auditEntries: [],
    generatedAt: '2026-06-23T00:00:00Z',
    env: {},
    readFile: () => JSON.stringify([]),
  });

  const doctor = buildDeliveryDoctorReport({ report });

  assert.equal(doctor.overall_status, 'needs_attention');
  assert.ok(doctor.blockers.some((item) => item.includes('DEEPSEEK_API_KEY')), 'expected provider blocker');
  assert.ok(doctor.blockers.some((item) => item.includes('block')), 'expected gate blocker');
});

test('buildDeliveryDoctorReport blocks on broken audit integrity', () => {
  const workspaceRoot = '/tmp/workspace';
  const entries = buildChainedAuditEntries(workspaceRoot);
  entries[1] = {
    ...entries[1],
    reason: 'tampered repair audit line',
  };

  const report = buildDeliveryMachineReport({
    config: createDefaultDeliveryConfig(workspaceRoot),
    checkResult: sampleCheck,
    auditEntries: entries,
    generatedAt: '2026-06-23T00:00:00Z',
    env: { DEEPSEEK_API_KEY: 'configured' },
    readFile: () => JSON.stringify([]),
  });
  const doctor = buildDeliveryDoctorReport({ report });

  assert.equal(report.audit.integrity.status, 'failed');
  assert.equal(doctor.overall_status, 'needs_attention');
  assert.ok(doctor.blockers.some((item) => item.includes('Audit log integrity verification failed')), 'expected integrity blocker');
});

function buildChainedAuditEntries(workspaceRoot: string) {
  const first = buildChainedAuditEntry({
    ts: '2026-06-23T00:00:00Z',
    tool: 'review_check',
    path: workspaceRoot,
    action: 'denied',
    reason: 'Review blocked the risky change.',
    details: {
      timestamp: '2026-06-23T00:00:00Z',
      finding_ids: ['finding-1'],
      evidence_ids: ['evidence-1'],
      gate_decision: {
        decision: 'block',
        reason: 'critical review finding',
        finding_ids: ['finding-1'],
      },
      policy_snapshot_id: 'policy:review.default@v1',
    },
  });
  const second = buildChainedAuditEntry({
    ts: '2026-06-23T00:01:00Z',
    tool: 'repair_apply',
    path: workspaceRoot,
    action: 'denied',
    reason: 'Repair preflight failed.',
    details: {
      timestamp: '2026-06-23T00:01:00Z',
      error_code: 'policy_blocked',
      error_stage: 'preflight',
      error_retryable: false,
      patch_proposal_id: 'proposal-1',
    },
    prev_hash: first.integrity_hash,
  });

  return [first, second];
}

function buildChainedAuditEntry(input: {
  ts: string;
  tool: string;
  path: string;
  action: string;
  reason: string;
  details: Record<string, unknown>;
  prev_hash?: string;
}) {
  const payload = {
    ts: input.ts,
    tool: input.tool,
    path: input.path,
    action: input.action,
    reason: input.reason,
    details: input.details,
    prev_hash: input.prev_hash ?? null,
  };

  return {
    ...payload,
    integrity_hash: sha256(JSON.stringify(payload)),
  };
}

function sha256(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}
