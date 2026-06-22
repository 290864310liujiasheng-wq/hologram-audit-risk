import {
  DEFAULT_REPAIR_RULES,
  DEFAULT_REVIEW_RULES,
  getReviewBucketDefinition,
  evaluateRepairProposal,
} from './rule-package';
import type { PatchProposal, ReviewFinding } from './review-core';

const assert = {
  equal(actual: unknown, expected: unknown): void {
    if (actual !== expected) {
      throw new Error(`Expected ${String(expected)}, got ${String(actual)}`);
    }
  },
};

function test(name: string, fn: () => Promise<void> | void): void {
  Promise.resolve()
    .then(fn)
    .then(() => {
      console.log(`ok - ${name}`);
    })
    .catch((error) => {
      console.error(`not ok - ${name}`);
      throw error;
    });
}

function finding(patch: Partial<ReviewFinding> = {}): ReviewFinding {
  return {
    finding_id: 'finding-1',
    job_id: 'job-1',
    rule_id: 'check.l5',
    severity: 'critical',
    category: 'architecture',
    locations: [{ file_path: 'src/safe.ts', start_line: 10, end_line: 12 }],
    plain_explanation: '当前修复只允许修改命中的风险文件。',
    impact: '超出范围的 patch 会放大 blast radius。',
    recommendation: '把修复限制在已命中的文件内。',
    evidence_ids: ['evidence-1'],
    confidence: 0.95,
    status: 'open',
    ...patch,
  };
}

function proposal(patch: Partial<PatchProposal> = {}): PatchProposal {
  return {
    patch_proposal_id: 'job-1:repair:proposal',
    repair_plan_id: 'job-1:repair',
    summary: '修复提案',
    rationale: '保持 patch 小而可回滚',
    generated_at: '2026-06-21T00:00:00Z',
    operations: [{
      operation_id: 'job-1:repair:op:0',
      file_path: 'src/unsafe.ts',
      new_content: 'export const repaired = true;\n',
      summary: 'touch unrelated file',
    }],
    ...patch,
  };
}

test('DEFAULT_REPAIR_RULES includes a block rule for out-of-scope writes', () => {
  const rule = DEFAULT_REPAIR_RULES.find((entry) => entry.rule_id === 'repair.scope.out_of_scope_write');

  assert.equal(rule?.gate_effect, 'block');
});

test('DEFAULT_REVIEW_RULES maps check.l5 to block and check.l4 to require_approval', () => {
  assert.equal(DEFAULT_REVIEW_RULES.find((entry) => entry.rule_id === 'check.l5')?.gate_effect, 'block');
  assert.equal(DEFAULT_REVIEW_RULES.find((entry) => entry.rule_id === 'check.l4')?.gate_effect, 'require_approval');
});

test('getReviewBucketDefinition centralizes category and recommendation for each review bucket', () => {
  assert.equal(getReviewBucketDefinition('l5').category, 'data_integrity');
  assert.equal(getReviewBucketDefinition('l4').category, 'security');
  assert.equal(getReviewBucketDefinition('l3').recommendation, '补充最小验证并确认回归风险已收口。');
  assert.equal(getReviewBucketDefinition('l2').recommendation, '收窄波及面并确认不会影响客户当前流程。');
});

test('evaluateRepairProposal flags patch writes outside the finding scope', () => {
  const findings = evaluateRepairProposal({
    plan_id: 'job-1:repair',
    proposal: proposal(),
    findings: [finding()],
  });

  assert.equal(findings[0]?.rule_id, 'repair.scope.out_of_scope_write');
});
