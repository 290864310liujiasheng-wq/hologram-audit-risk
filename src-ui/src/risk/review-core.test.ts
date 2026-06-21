import {
  createAuditEvent,
  deriveGateDecision,
  finalizeReviewJobResult,
  validateRule,
  validateReviewFinding,
  validateReviewJobRequest,
  type ReviewFinding,
  type ReviewJobRequest,
  type Rule,
} from './review-core';

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

function baseFinding(patch: Partial<ReviewFinding> = {}): ReviewFinding {
  return {
    finding_id: 'finding-1',
    job_id: 'job-1',
    rule_id: 'rule-critical',
    severity: 'critical',
    category: 'unsafe-write',
    locations: [{ file_path: 'src/app.ts', start_line: 10, end_line: 12 }],
    plain_explanation: '这段改动会写入敏感路径。',
    impact: '可能覆盖客户文件。',
    recommendation: '改为受控写入并要求审批。',
    evidence_ids: ['evidence-1'],
    confidence: 0.9,
    status: 'open',
    ...patch,
  };
}

const blockingRule: Rule = {
  rule_id: 'rule-critical',
  name: '危险写入必须拦截',
  category: 'permission',
  severity: 'critical',
  scope: ['file_write'],
  trigger: { kind: 'permission', config: {} },
  gate_effect: 'block',
  enabled: true,
};

function baseReviewJobRequest(patch: Partial<ReviewJobRequest> = {}): ReviewJobRequest {
  return {
    workspace_id: 'workspace-1',
    change_id: 'change-1',
    mode: 'live',
    policy_profile_id: 'policy-1',
    provider_profile_id: 'provider-1',
    ...patch,
  };
}

function test(name: string, fn: () => void): void {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (error) {
    console.error(`not ok - ${name}`);
    throw error;
  }
}

test('validateReviewFinding rejects findings without evidence', () => {
  const errors = validateReviewFinding(baseFinding({ evidence_ids: [] }));

  assert.deepEqual(errors, [
    {
      code: 'missing_evidence',
      message: 'ReviewFinding requires at least one evidence id.',
      retryable: false,
    },
  ]);
});

test('validateReviewFinding rejects confidence outside the contract range', () => {
  const errors = validateReviewFinding(baseFinding({ confidence: 1.2 }));

  assert.equal(errors[0]?.code, 'invalid_request');
  assert.equal(errors[0]?.retryable, false);
});

test('validateReviewFinding rejects findings without source locations', () => {
  const errors = validateReviewFinding(baseFinding({ locations: [] }));

  assert.equal(errors[0]?.code, 'invalid_request');
  assert.equal(errors[0]?.message, 'ReviewFinding requires at least one source location.');
});

test('validateReviewJobRequest rejects missing policy and provider profiles', () => {
  const errors = validateReviewJobRequest(baseReviewJobRequest({
    policy_profile_id: '',
    provider_profile_id: '',
  }));

  assert.deepEqual(errors.map((error) => error.code), ['invalid_request', 'invalid_request']);
});

test('validateRule rejects block rules without scope', () => {
  const errors = validateRule({
    ...blockingRule,
    scope: [],
  });

  assert.equal(errors[0]?.code, 'invalid_request');
  assert.equal(errors[0]?.message, 'Rule requires at least one scope entry.');
});

test('deriveGateDecision blocks when an enabled matched rule has block effect', () => {
  const decision = deriveGateDecision({
    job_id: 'job-1',
    subject_type: 'file_write',
    subject_ref: 'src/app.ts',
    findings: [baseFinding()],
    rules: [blockingRule],
    policy_snapshot_id: 'policy-1',
    decided_at: '2026-06-20T00:00:00Z',
  });

  assert.equal(decision.decision, 'block');
  assert.deepEqual(decision.finding_ids, ['finding-1']);
  assert.equal(decision.reason, '危险写入必须拦截');
});

test('deriveGateDecision keeps every matched finding id even when block is the strongest outcome', () => {
  const warningRule: Rule = {
    ...blockingRule,
    rule_id: 'rule-warn',
    name: '普通提醒',
    gate_effect: 'warn',
    severity: 'low',
  };
  const decision = deriveGateDecision({
    job_id: 'job-1',
    subject_type: 'file_write',
    subject_ref: 'src/app.ts',
    findings: [
      baseFinding(),
      baseFinding({ finding_id: 'finding-2', rule_id: 'rule-warn', severity: 'low' }),
    ],
    rules: [blockingRule, warningRule],
    policy_snapshot_id: 'policy-1',
    decided_at: '2026-06-20T00:00:00Z',
  });

  assert.deepEqual(decision.finding_ids, ['finding-1', 'finding-2']);
  assert.equal(decision.decision, 'block');
});

test('finalizeReviewJobResult becomes blocked when gate decision blocks', () => {
  const result = finalizeReviewJobResult({
    job_id: 'job-1',
    findings: [baseFinding()],
    gate_decision: {
      decision_id: 'decision-1',
      job_id: 'job-1',
      subject_type: 'file_write',
      subject_ref: 'src/app.ts',
      decision: 'block',
      reason: '危险写入必须拦截',
      finding_ids: ['finding-1'],
      policy_snapshot_id: 'policy-1',
      decided_at: '2026-06-20T00:00:00Z',
    },
    audit_event_ids: ['audit-1'],
  });

  assert.equal(result.status, 'blocked');
});

test('finalizeReviewJobResult becomes degraded when degraded reasons exist without a block', () => {
  const result = finalizeReviewJobResult({
    job_id: 'job-1',
    findings: [baseFinding({ severity: 'low' })],
    audit_event_ids: ['audit-1'],
    degraded_reasons: ['provider timeout'],
  });

  assert.equal(result.status, 'degraded');
});

test('createAuditEvent includes decision and evidence references for gate decisions', () => {
  const event = createAuditEvent({
    workspace_id: 'workspace-1',
    actor: 'lead-reviewer',
    event_type: 'gate_decided',
    subject_ref: 'src/app.ts',
    decision_id: 'decision-1',
    findings: [
      baseFinding(),
      baseFinding({ finding_id: 'finding-2', evidence_ids: ['evidence-2'] }),
    ],
    timestamp: '2026-06-20T00:00:00Z',
  });

  assert.equal(event.decision_id, 'decision-1');
  assert.deepEqual(event.evidence_ids, ['evidence-1', 'evidence-2']);
});
