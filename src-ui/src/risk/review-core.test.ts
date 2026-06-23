import {
  createAuditEvent,
  deriveGateDecision,
  finalizeReviewJobResult,
  validateGateDecision,
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
  package_id: 'review.default',
  name: '危险写入必须拦截',
  category: 'permission',
  severity: 'critical',
  priority: 100,
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

test('validateReviewFinding rejects invalid source line ranges', () => {
  const errors = validateReviewFinding(baseFinding({
    locations: [{ file_path: 'src/app.ts', start_line: 12, end_line: 10 }],
  }));

  assert.equal(errors[0]?.code, 'invalid_request');
  assert.equal(errors[0]?.message, 'ReviewFinding source locations must use positive, ordered line ranges.');
});

test('validateReviewFinding rejects plain explanations that are only rule identifiers', () => {
  const errors = validateReviewFinding(baseFinding({
    plain_explanation: 'rule-critical',
  }));

  assert.equal(errors[0]?.code, 'invalid_request');
  assert.equal(errors[0]?.message, 'ReviewFinding plain explanation must be human-readable, not just an identifier.');
});

test('validateReviewFinding rejects impact that is not human-readable', () => {
  const errors = validateReviewFinding(baseFinding({
    impact: 'impact.high',
  }));

  assert.equal(errors[0]?.code, 'invalid_request');
  assert.equal(errors[0]?.message, 'ReviewFinding impact must describe the user-visible or system impact in plain language.');
});

test('validateReviewFinding rejects empty recommendations', () => {
  const errors = validateReviewFinding(baseFinding({
    recommendation: 'fix',
  }));

  assert.equal(errors[0]?.code, 'invalid_request');
  assert.equal(errors[0]?.message, 'ReviewFinding recommendation must contain a concrete human-readable action.');
});

test('validateReviewJobRequest rejects missing policy and provider profiles', () => {
  const errors = validateReviewJobRequest(baseReviewJobRequest({
    policy_profile_id: '',
    provider_profile_id: '',
  }));

  assert.deepEqual(errors.map((error) => error.code), ['invalid_request', 'invalid_request']);
});

test('validateReviewJobRequest rejects missing workspace and change ids', () => {
  const errors = validateReviewJobRequest(baseReviewJobRequest({
    workspace_id: '',
    change_id: '',
  }));

  assert.deepEqual(errors.map((error) => error.message), [
    'ReviewJobRequest requires a workspace id.',
    'ReviewJobRequest requires a change id.',
  ]);
});

test('validateRule rejects block rules without scope', () => {
  const errors = validateRule({
    ...blockingRule,
    scope: [],
  });

  assert.equal(errors[0]?.code, 'invalid_request');
  assert.equal(errors[0]?.message, 'Rule requires at least one scope entry.');
});

test('validateRule rejects rules without a package id or priority', () => {
  const errors = validateRule({
    ...blockingRule,
    package_id: '',
    priority: Number.NaN,
  });

  assert.deepEqual(errors.map((error) => error.message), [
    'Rule requires a package id.',
    'Rule priority must be a finite number.',
  ]);
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
    package_id: 'review.default',
    name: '普通提醒',
    gate_effect: 'warn',
    severity: 'low',
    priority: 10,
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

test('deriveGateDecision prefers the higher-priority rule when matched effects are equally blocking', () => {
  const lowerPriorityRule: Rule = {
    ...blockingRule,
    rule_id: 'rule-low-priority',
    name: '低优先级阻断',
    priority: 20,
  };
  const higherPriorityRule: Rule = {
    ...blockingRule,
    rule_id: 'rule-high-priority',
    name: '高优先级阻断',
    priority: 90,
  };

  const decision = deriveGateDecision({
    job_id: 'job-1',
    subject_type: 'file_write',
    subject_ref: 'src/app.ts',
    findings: [
      baseFinding({ finding_id: 'finding-1', rule_id: 'rule-low-priority' }),
      baseFinding({ finding_id: 'finding-2', rule_id: 'rule-high-priority' }),
    ],
    rules: [lowerPriorityRule, higherPriorityRule],
    policy_snapshot_id: 'policy-1',
    decided_at: '2026-06-20T00:00:00Z',
  });

  assert.equal(decision.decision, 'block');
  assert.equal(decision.reason, '高优先级阻断');
});

test('validateGateDecision rejects block decisions without reason or findings', () => {
  const errors = validateGateDecision({
    decision_id: 'decision-1',
    job_id: 'job-1',
    subject_type: 'file_write',
    subject_ref: '',
    decision: 'block',
    reason: '',
    finding_ids: [],
    policy_snapshot_id: '',
    decided_at: '2026-06-20T00:00:00Z',
  });

  assert.deepEqual(errors.map((error) => error.message), [
    'GateDecision requires a subject reference.',
    'GateDecision requires a policy snapshot id.',
    'Blocking or approval decisions require a human-readable reason.',
    'Blocking or approval decisions require at least one finding id.',
  ]);
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
