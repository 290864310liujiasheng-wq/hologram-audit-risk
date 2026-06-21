import type { Provider, Request } from '../provider/types';
import { ChunkType } from '../provider/types';
import {
  applyRepairPlan,
  approveRepairPlan,
  attachPatchProposal,
  createRepairPlan,
  generatePatchProposalFromModel,
  parsePatchProposal,
  rejectRepairPlan,
  rollbackRepairPlan,
} from './self-heal';
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
    locations: [{ file_path: 'src-ui/src/risk/review-core.ts', start_line: 12, end_line: 14 }],
    plain_explanation: '写入敏感路径',
    impact: '可能覆盖核心合同。',
    recommendation: '要求审批并补回归测试。',
    evidence_ids: ['evidence-1'],
    confidence: 0.95,
    status: 'open',
    ...patch,
  };
}

class FakeProvider implements Provider {
  constructor(private readonly text: string) {}

  name(): string {
    return 'fake-provider';
  }

  async *stream(_signal: AbortSignal, _req: Request) {
    yield { type: ChunkType.Text, text: this.text };
    yield { type: ChunkType.Done };
  }
}

test('createRepairPlan derives strategy, tests, and plan storage path from findings', () => {
  const plan = createRepairPlan({
    job_id: 'job-1',
    findings: [finding()],
    workspace_path: '/tmp/workspace',
  });

  assert.equal(plan.approval_state, 'draft');
  assert.equal(plan.required_tests.includes('npm run test:risk'), true);
  assert.equal(plan.required_tests.includes('npx tsc --noEmit'), true);
  assert.equal(plan.patch_proposal_ref, '/tmp/workspace/.hologram/repair-plans/job-1:repair.json');
});

test('parsePatchProposal accepts fenced JSON and builds deterministic operation ids', () => {
  const proposal = parsePatchProposal(
    '```json\n{"summary":"修复风险","rationale":"保持 owner 不变","operations":[{"file_path":"src/app.ts","summary":"replace file","new_content":"export const ok = true;"}]}\n```',
    {
      repair_plan_id: 'job-1:repair',
      generated_at: '2026-06-20T00:00:00Z',
    },
  );

  assert.equal(proposal.patch_proposal_id, 'job-1:repair:proposal');
  assert.equal(proposal.operations[0]?.operation_id, 'job-1:repair:op:0');
});

test('generatePatchProposalFromModel parses model output into a structured proposal', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"保持 patch 小而可回滚","operations":[{"file_path":"src/app.ts","summary":"tighten guard","new_content":"export const repaired = true;"}]}',
  );
  const proposal = await generatePatchProposalFromModel(new AbortController().signal, provider, {
    repair_plan_id: 'job-1:repair',
    files: [{ file_path: 'src/app.ts', content: 'export const repaired = false;' }],
    findings: [finding()],
    generated_at: '2026-06-20T00:00:00Z',
  });

  assert.equal(proposal.summary, '修复风险');
  assert.equal(proposal.operations.length, 1);
  assert.equal(proposal.operations[0]?.file_path, 'src/app.ts');
});

test('attachPatchProposal and approval transitions move the plan through the required states', () => {
  const draft = createRepairPlan({
    job_id: 'job-1',
    findings: [finding()],
    workspace_path: '/tmp/workspace',
  });
  const attached = attachPatchProposal(draft, {
    patch_proposal_id: 'job-1:repair:proposal',
    repair_plan_id: draft.repair_plan_id,
    summary: '修复风险',
    rationale: '保持 patch 小而可回滚',
    generated_at: '2026-06-20T00:00:00Z',
    operations: [{
      operation_id: 'op-1',
      file_path: 'src/app.ts',
      new_content: 'export const repaired = true;',
      summary: 'tighten guard',
    }],
  });
  const approved = approveRepairPlan(attached);
  const rejected = rejectRepairPlan(attached);

  assert.equal(attached.approval_state, 'waiting_approval');
  assert.equal(approved.approval_state, 'approved');
  assert.equal(rejected.approval_state, 'rejected');
});

test('applyRepairPlan writes the proposal and rollbackRepairPlan restores the previous content', async () => {
  const writes = new Map<string, string>([['src/app.ts', 'export const repaired = false;']]);
  const draft = createRepairPlan({
    job_id: 'job-1',
    findings: [finding({ locations: [{ file_path: 'src/app.ts', start_line: 1, end_line: 1 }] })],
    workspace_path: '/tmp/workspace',
  });
  const waiting = attachPatchProposal(draft, {
    patch_proposal_id: 'job-1:repair:proposal',
    repair_plan_id: draft.repair_plan_id,
    summary: '修复风险',
    rationale: '保持 patch 小而可回滚',
    generated_at: '2026-06-20T00:00:00Z',
    operations: [{
      operation_id: 'op-1',
      file_path: 'src/app.ts',
      new_content: 'export const repaired = true;',
      summary: 'tighten guard',
    }],
  });
  const approved = approveRepairPlan(waiting);

  const applied = await applyRepairPlan({
    plan: approved,
    proposal: {
      patch_proposal_id: 'job-1:repair:proposal',
      repair_plan_id: draft.repair_plan_id,
      summary: '修复风险',
      rationale: '保持 patch 小而可回滚',
      generated_at: '2026-06-20T00:00:00Z',
      operations: [{
        operation_id: 'op-1',
        file_path: 'src/app.ts',
        new_content: 'export const repaired = true;',
        summary: 'tighten guard',
      }],
    },
    now: '2026-06-20T00:00:10Z',
    readFile: async (filePath) => writes.get(filePath) || '',
    writeFile: async (filePath, content) => {
      writes.set(filePath, content);
    },
  });

  assert.equal(applied.plan.approval_state, 'applied');
  assert.equal(writes.get('src/app.ts'), 'export const repaired = true;');

  const rolledBack = await rollbackRepairPlan({
    plan: applied.plan,
    rollback: applied.rollback,
    writeFile: async (filePath, content) => {
      writes.set(filePath, content);
    },
  });

  assert.equal(rolledBack.approval_state, 'rolled_back');
  assert.equal(writes.get('src/app.ts'), 'export const repaired = false;');
});
