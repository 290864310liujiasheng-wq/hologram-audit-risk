import { buildCurrentReviewState } from './current-review';
import { type RiskCheckResult } from './check-adapter';

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
});
