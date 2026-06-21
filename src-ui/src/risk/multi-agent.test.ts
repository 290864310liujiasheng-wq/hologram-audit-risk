import {
  aggregateAgentRuns,
  buildSpecialistAgentRuns,
  finalizeMultiAgentReview,
  type SpecialistAgentResult,
} from './multi-agent';
import type { ReviewFinding } from './review-core';

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

function finding(patch: Partial<ReviewFinding> = {}): ReviewFinding {
  return {
    finding_id: 'finding-1',
    job_id: 'job-1',
    rule_id: 'check.l5',
    severity: 'critical',
    category: 'architecture',
    locations: [{ file_path: 'src/auth.ts', start_line: 42, end_line: 42 }],
    plain_explanation: '写入敏感文件',
    impact: '可能覆盖认证逻辑。',
    recommendation: '要求审批并补测试。',
    evidence_ids: ['evidence-1'],
    confidence: 0.95,
    status: 'open',
    ...patch,
  };
}

test('buildSpecialistAgentRuns routes findings to matching specialist agents', () => {
  const results = buildSpecialistAgentRuns({
    job_id: 'job-1',
    findings: [
      finding(),
      finding({
        finding_id: 'finding-2',
        severity: 'medium',
        category: 'test_regression',
        plain_explanation: '缺少回归测试',
        recommendation: '补一条回归测试',
      }),
    ],
    started_at: '2026-06-20T00:00:00Z',
    completed_at: '2026-06-20T00:00:05Z',
  });

  assert.equal(results.find((result) => result.run.agent_type === 'static')?.findings.length, 1);
  assert.equal(results.find((result) => result.run.agent_type === 'security')?.findings.length, 1);
  assert.equal(results.find((result) => result.run.agent_type === 'test_regression')?.findings.length, 1);
  assert.equal(results.find((result) => result.run.agent_type === 'repair_planner')?.findings.length, 2);
});

test('buildSpecialistAgentRuns degrades the requested agent when execution failed', () => {
  const results = buildSpecialistAgentRuns({
    job_id: 'job-1',
    findings: [finding()],
    started_at: '2026-06-20T00:00:00Z',
    completed_at: '2026-06-20T00:00:05Z',
    failed_agents: {
      dependency: 'dependency audit timed out',
    },
  });

  const dependency = results.find((result) => result.run.agent_type === 'dependency');
  assert.equal(dependency?.run.status, 'degraded');
  assert.equal(dependency?.run.error, 'dependency audit timed out');
  assert.equal(dependency?.findings.length, 0);
});

test('aggregateAgentRuns deduplicates overlapping specialist findings and records conflicts', () => {
  const duplicateHigh = finding({
    finding_id: 'finding-2',
    severity: 'high',
    confidence: 0.8,
  });
  const input: SpecialistAgentResult[] = [
    {
      run: {
        agent_run_id: 'job-1:static',
        job_id: 'job-1',
        agent_type: 'static',
        status: 'completed',
        input_evidence_ids: ['evidence-1'],
        finding_ids: ['finding-1'],
        started_at: '2026-06-20T00:00:00Z',
        completed_at: '2026-06-20T00:00:05Z',
      },
      findings: [finding()],
      suggested_decision: 'block',
    },
    {
      run: {
        agent_run_id: 'job-1:security',
        job_id: 'job-1',
        agent_type: 'security',
        status: 'completed',
        input_evidence_ids: ['evidence-1'],
        finding_ids: ['finding-2'],
        started_at: '2026-06-20T00:00:00Z',
        completed_at: '2026-06-20T00:00:05Z',
      },
      findings: [duplicateHigh],
      suggested_decision: 'require_approval',
    },
  ];

  const { merged_findings, aggregation } = aggregateAgentRuns({
    job_id: 'job-1',
    agent_results: input,
  });

  assert.equal(merged_findings.length, 1);
  assert.deepEqual(aggregation.dropped_duplicates, ['finding-2']);
  assert.equal(aggregation.conflicts.length, 1);
  assert.deepEqual(aggregation.conflicts[0]?.finding_ids, ['finding-1', 'finding-2']);
});

test('finalizeMultiAgentReview exposes degraded reasons for downstream UI and audit', () => {
  const review = finalizeMultiAgentReview({
    job_id: 'job-1',
    findings: [finding()],
    started_at: '2026-06-20T00:00:00Z',
    completed_at: '2026-06-20T00:00:05Z',
    failed_agents: {
      dependency: 'dependency audit timed out',
    },
  });

  assert.equal(review.merged_findings.length, 1);
  assert.deepEqual(review.degraded_reasons, ['dependency: dependency audit timed out']);
  assert.equal(review.aggregation.lead_agent_run_id, 'job-1:lead-reviewer');
});
