import {
  adaptCheckResultToFindings,
  buildCheckRiskSummary,
  summarizeSeverityCounts,
  type RiskCheckResult,
} from './check-adapter';

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
  l4_violations: [{
    signal: {
      description: '静默破坏接口行为',
      file_path: 'src/auth.ts',
      line: 18,
    },
    message: '潜在静默破坏',
    level: 4,
  }],
  l3_violations: [],
  l2_violations: [],
};

test('adaptCheckResultToFindings converts violations into ReviewFinding records', () => {
  const findings = adaptCheckResultToFindings(sample, {
    jobId: 'job-1',
    evidencePrefix: 'check',
  });

  assert.equal(findings.length, 2);
  assert.equal(findings[0]?.severity, 'critical');
  assert.equal(findings[0]?.rule_id, 'check.l5');
  assert.equal(findings[1]?.severity, 'high');
});

test('adaptCheckResultToFindings creates stable evidence ids from violation groups', () => {
  const findings = adaptCheckResultToFindings(sample, {
    jobId: 'job-1',
    evidencePrefix: 'check',
  });

  assert.deepEqual(findings[0]?.evidence_ids, ['check:l5:0']);
  assert.deepEqual(findings[1]?.evidence_ids, ['check:l4:0']);
});

test('summarizeSeverityCounts counts findings by severity', () => {
  const findings = adaptCheckResultToFindings(sample, {
    jobId: 'job-1',
    evidencePrefix: 'check',
  });
  const summary = summarizeSeverityCounts(findings);

  assert.deepEqual(summary, {
    critical: 1,
    high: 1,
    medium: 0,
    low: 0,
    info: 0,
  });
});

test('buildCheckRiskSummary orders critical findings first and exposes display locations', () => {
  const findings = adaptCheckResultToFindings(sample, {
    jobId: 'job-1',
    evidencePrefix: 'check',
  });
  const summary = buildCheckRiskSummary(findings);

  assert.equal(summary.total, 2);
  assert.equal(summary.topFindings[0]?.severity, 'critical');
  assert.equal(summary.topFindings[0]?.locationLabel, 'auth.ts:42');
  assert.equal(summary.topFindings[1]?.locationLabel, 'auth.ts:18');
});
