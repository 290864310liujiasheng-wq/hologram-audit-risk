import type { Provider, Request } from '../provider/types';
import { ChunkType } from '../provider/types';
import {
  applyRepairPlan,
  buildRepairPreflightSummary,
  buildRepairGenerationMetadata,
  deriveRepairFilePaths,
  buildRepairIssueFromPreflight,
  getRepairGenerationBlocker,
  RepairApplyError,
  RepairApplyExecutionError,
  createRepairIssue,
  approveRepairPlan,
  attachPatchProposal,
  createRepairPlan,
  generatePatchProposalFromModel,
  parsePatchProposal,
  rejectRepairPlan,
  rollbackRepairPlan,
  runRepairPreflight,
} from './self-heal';
import type { ReviewFinding } from './review-core';
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

test('createRepairPlan keeps required_tests empty when there are no repair findings', () => {
  const plan = createRepairPlan({
    job_id: 'job-empty',
    findings: [],
    workspace_path: '/tmp/workspace',
  });

  assert.deepEqual(plan.required_tests, []);
  assert.equal(plan.strategy, 'No findings selected for repair.');
  assert.equal(plan.risk_note, 'No repair risk identified.');
});

test('createRepairPlan adds a minimal git diff gate for critical config changes', () => {
  const plan = createRepairPlan({
    job_id: 'job-config',
    findings: [
      finding({
        locations: [{ file_path: 'config.yaml', start_line: 1, end_line: 1 }],
      }),
    ],
    workspace_path: '/tmp/workspace',
  });

  assert.equal(plan.required_tests.includes('git diff --check'), true);
});

test('buildRepairGenerationMetadata summarizes provider and high-severity focus for audit', () => {
  const metadata = buildRepairGenerationMetadata({
    repair_plan_id: 'job-1:repair',
    provider_name: 'anthropic',
    model: 'claude-sonnet-4-6',
    files: [
      { file_path: 'src/a.ts', content: 'a' },
      { file_path: 'src/b.ts', content: 'b' },
    ],
    findings: [
      finding({
        finding_id: 'finding-crit',
        severity: 'critical',
        locations: [{ file_path: 'src/a.ts', start_line: 1, end_line: 1 }],
      }),
      finding({
        finding_id: 'finding-low',
        severity: 'low',
        locations: [{ file_path: 'src/b.ts', start_line: 2, end_line: 2 }],
      }),
    ],
    generated_at: '2026-06-21T00:00:00Z',
  });

  assert.equal(metadata.provider_name, 'anthropic');
  assert.equal(metadata.model, 'claude-sonnet-4-6');
  assert.equal(metadata.file_count, 2);
  assert.deepEqual(metadata.high_severity_finding_ids, ['finding-crit']);
  assert.deepEqual(metadata.focus_file_paths, ['src/a.ts', 'src/b.ts']);
});

test('getRepairGenerationBlocker rejects empty finding sets before provider generation starts', () => {
  const blocker = getRepairGenerationBlocker({
    findings: [],
    files: [],
  });

  assert.equal(blocker?.code, 'invalid_request');
  assert.equal(blocker?.message, 'No findings selected for repair.');
  assert.equal(blocker?.retryable, false);
});

test('getRepairGenerationBlocker rejects missing readable source files with evidence context', () => {
  const blocker = getRepairGenerationBlocker({
    findings: [finding()],
    files: [],
  });

  assert.equal(blocker?.code, 'missing_evidence');
  assert.equal(blocker?.message, 'Current findings do not map to readable source files for repair planning.');
  assert.deepEqual(blocker?.evidence_ids, ['evidence-1']);
});

test('deriveRepairFilePaths falls back to changed files when findings do not carry usable file paths', () => {
  const files = deriveRepairFilePaths({
    findings: [
      finding({
        locations: [{ file_path: 'unknown', start_line: 1, end_line: 1 }],
      }),
    ],
    changed_files: ['src/cache.ts', 'src/serializer.ts'],
  });

  assert.deepEqual(files, ['src/cache.ts', 'src/serializer.ts']);
});

test('deriveRepairFilePaths keeps finding-backed files and deduplicates overlaps', () => {
  const files = deriveRepairFilePaths({
    findings: [
      finding({
        locations: [{ file_path: 'src/cache.ts', start_line: 1, end_line: 1 }],
      }),
    ],
    changed_files: ['src/cache.ts', 'src/serializer.ts'],
  });

  assert.deepEqual(files, ['src/cache.ts', 'src/serializer.ts']);
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

test('parsePatchProposal rejects proposal summaries that are not human-readable', () => {
  let failed = false;
  try {
    parsePatchProposal(
      '{"summary":"fix","rationale":"保持 owner 不变","operations":[{"file_path":"src/app.ts","summary":"tighten guard","new_content":"export const ok = true;"}]}',
      {
        repair_plan_id: 'job-1:repair',
        generated_at: '2026-06-20T00:00:00Z',
      },
    );
  } catch (error) {
    failed = String((error as Error).message || error).includes('Patch proposal summary must contain a concrete human-readable explanation');
  }

  assert.equal(failed, true);
});

test('parsePatchProposal rejects rationales that are only short tokens', () => {
  let failed = false;
  try {
    parsePatchProposal(
      '{"summary":"修复高风险写入","rationale":"todo","operations":[{"file_path":"src/app.ts","summary":"tighten guard","new_content":"export const ok = true;"}]}',
      {
        repair_plan_id: 'job-1:repair',
        generated_at: '2026-06-20T00:00:00Z',
      },
    );
  } catch (error) {
    failed = String((error as Error).message || error).includes('Patch proposal rationale must explain why the change repairs the risk');
  }

  assert.equal(failed, true);
});

test('parsePatchProposal rejects operation summaries that stay at placeholder level', () => {
  let failed = false;
  try {
    parsePatchProposal(
      '{"summary":"修复高风险写入","rationale":"把危险写入改成受控路径并保留审批链。","operations":[{"file_path":"src/app.ts","summary":"update","new_content":"export const ok = true;"}]}',
      {
        repair_plan_id: 'job-1:repair',
        generated_at: '2026-06-20T00:00:00Z',
      },
    );
  } catch (error) {
    failed = String((error as Error).message || error).includes('Patch operation summary must describe the concrete repair action');
  }

  assert.equal(failed, true);
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

test('generatePatchProposalFromModel rejects operations outside the provided repair file set', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"保持 patch 小而可回滚","operations":[{"file_path":"src/other.ts","summary":"tighten guard","new_content":"export const repaired = true;"}]}',
  );

  let failed = false;
  try {
    await generatePatchProposalFromModel(new AbortController().signal, provider, {
      repair_plan_id: 'job-1:repair',
      files: [{ file_path: 'src/app.ts', content: 'export const repaired = false;' }],
      findings: [finding()],
      generated_at: '2026-06-20T00:00:00Z',
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('outside the provided repair file set');
  }

  assert.equal(failed, true);
});

test('generatePatchProposalFromModel rejects no-op file rewrites', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"保持 patch 小而可回滚","operations":[{"file_path":"src/app.ts","summary":"tighten guard","new_content":"export const repaired = false;"}]}',
  );

  let failed = false;
  try {
    await generatePatchProposalFromModel(new AbortController().signal, provider, {
      repair_plan_id: 'job-1:repair',
      files: [{ file_path: 'src/app.ts', content: 'export const repaired = false;' }],
      findings: [finding()],
      generated_at: '2026-06-20T00:00:00Z',
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('does not change file content');
  }

  assert.equal(failed, true);
});

test('generatePatchProposalFromModel rejects proposals that ignore critical finding files', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"保持 patch 小而可回滚","operations":[{"file_path":"src/secondary.ts","summary":"tighten guard","new_content":"export const secondary = true;"}]}',
  );

  let failed = false;
  try {
    await generatePatchProposalFromModel(new AbortController().signal, provider, {
      repair_plan_id: 'job-1:repair',
      files: [
        { file_path: 'src/critical.ts', content: 'export const critical = false;' },
        { file_path: 'src/secondary.ts', content: 'export const secondary = false;' },
      ],
      findings: [
        finding({
          severity: 'critical',
          locations: [{ file_path: 'src/critical.ts', start_line: 1, end_line: 1 }],
        }),
        finding({
          finding_id: 'finding-2',
          severity: 'low',
          locations: [{ file_path: 'src/secondary.ts', start_line: 1, end_line: 1 }],
        }),
      ],
      generated_at: '2026-06-20T00:00:00Z',
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('must modify every high-severity finding file');
  }

  assert.equal(failed, true);
});

test('generatePatchProposalFromModel rejects proposals that only cover part of the high-severity file set', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"保持 patch 小而可回滚","operations":[{"file_path":"src/critical-a.ts","summary":"tighten guard","new_content":"export const criticalA = true;"}]}',
  );

  let failed = false;
  try {
    await generatePatchProposalFromModel(new AbortController().signal, provider, {
      repair_plan_id: 'job-1:repair',
      files: [
        { file_path: 'src/critical-a.ts', content: 'export const criticalA = false;' },
        { file_path: 'src/critical-b.ts', content: 'export const criticalB = false;' },
      ],
      findings: [
        finding({
          severity: 'critical',
          locations: [{ file_path: 'src/critical-a.ts', start_line: 1, end_line: 1 }],
        }),
        finding({
          finding_id: 'finding-2',
          severity: 'high',
          locations: [{ file_path: 'src/critical-b.ts', start_line: 1, end_line: 1 }],
        }),
      ],
      generated_at: '2026-06-20T00:00:00Z',
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('must modify every high-severity finding file');
  }

  assert.equal(failed, true);
});

test('generatePatchProposalFromModel rejects proposals that do not touch the high-severity finding line range', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"保持 patch 小而可回滚","operations":[{"file_path":"src/critical.ts","summary":"touch unrelated lines","new_content":"line 1 changed\\nline 2\\nline 3\\nline 4\\n"}]}',
  );

  let failed = false;
  try {
    await generatePatchProposalFromModel(new AbortController().signal, provider, {
      repair_plan_id: 'job-1:repair',
      files: [{
        file_path: 'src/critical.ts',
        content: 'line 1\nline 2\nline 3\nline 4\n',
      }],
      findings: [
        finding({
          severity: 'critical',
          locations: [{ file_path: 'src/critical.ts', start_line: 3, end_line: 4 }],
        }),
      ],
      generated_at: '2026-06-20T00:00:00Z',
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('must materially change every line in high-severity finding ranges');
  }

  assert.equal(failed, true);
});

test('generatePatchProposalFromModel accepts proposals that touch the high-severity finding line range', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"保持 patch 小而可回滚","operations":[{"file_path":"src/critical.ts","summary":"fix risky lines","new_content":"line 1\\nline 2\\nline 3 fixed\\nline 4 fixed\\n"}]}',
  );

  const proposal = await generatePatchProposalFromModel(new AbortController().signal, provider, {
    repair_plan_id: 'job-1:repair',
    files: [{
      file_path: 'src/critical.ts',
      content: 'line 1\nline 2\nline 3\nline 4\n',
    }],
    findings: [
      finding({
        severity: 'critical',
        locations: [{ file_path: 'src/critical.ts', start_line: 3, end_line: 4 }],
      }),
    ],
    generated_at: '2026-06-20T00:00:00Z',
  });

  assert.equal(proposal.operations[0]?.file_path, 'src/critical.ts');
});

test('generatePatchProposalFromModel rejects proposals that only touch part of a multi-line high-severity range', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"只改一半","operations":[{"file_path":"src/critical.ts","summary":"fix one line only","new_content":"line 1\\nline 2\\nline 3 fixed\\nline 4\\n"}]}',
  );

  let failed = false;
  try {
    await generatePatchProposalFromModel(new AbortController().signal, provider, {
      repair_plan_id: 'job-1:repair',
      files: [{
        file_path: 'src/critical.ts',
        content: 'line 1\nline 2\nline 3\nline 4\n',
      }],
      findings: [
        finding({
          severity: 'critical',
          locations: [{ file_path: 'src/critical.ts', start_line: 3, end_line: 4 }],
        }),
      ],
      generated_at: '2026-06-20T00:00:00Z',
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('must materially change every line in high-severity finding ranges');
  }

  assert.equal(failed, true);
});

test('generatePatchProposalFromModel rejects disjoint edits that only span across the risk range', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"保持 patch 小而可回滚","operations":[{"file_path":"src/critical.ts","summary":"edit top and bottom only","new_content":"line 1 changed\\nline 2\\nline 3\\nline 4\\nline 5\\nline 6 changed\\n"}]}',
  );

  let failed = false;
  try {
    await generatePatchProposalFromModel(new AbortController().signal, provider, {
      repair_plan_id: 'job-1:repair',
      files: [{
        file_path: 'src/critical.ts',
        content: 'line 1\nline 2\nline 3\nline 4\nline 5\nline 6\n',
      }],
      findings: [
        finding({
          severity: 'critical',
          locations: [{ file_path: 'src/critical.ts', start_line: 3, end_line: 4 }],
        }),
      ],
      generated_at: '2026-06-20T00:00:00Z',
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('must materially change every line in high-severity finding ranges');
  }

  assert.equal(failed, true);
});

test('generatePatchProposalFromModel rejects whitespace-only edits on the high-severity finding lines', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"保持 patch 小而可回滚","operations":[{"file_path":"src/critical.ts","summary":"reformat risky lines only","new_content":"line 1\\nline 2\\n  line 3  \\nline 4\\n"}]}',
  );

  let failed = false;
  try {
    await generatePatchProposalFromModel(new AbortController().signal, provider, {
      repair_plan_id: 'job-1:repair',
      files: [{
        file_path: 'src/critical.ts',
        content: 'line 1\nline 2\nline 3\nline 4\n',
      }],
      findings: [
        finding({
          severity: 'critical',
          locations: [{ file_path: 'src/critical.ts', start_line: 3, end_line: 3 }],
        }),
      ],
      generated_at: '2026-06-20T00:00:00Z',
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('must materially change every line in high-severity finding ranges');
  }

  assert.equal(failed, true);
});

test('generatePatchProposalFromModel accepts proposals that delete the high-severity finding line', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"删除危险调用","operations":[{"file_path":"src/critical.ts","summary":"remove risky line","new_content":"line 1\\nline 2\\nline 4\\n"}]}',
  );

  const proposal = await generatePatchProposalFromModel(new AbortController().signal, provider, {
    repair_plan_id: 'job-1:repair',
    files: [{
      file_path: 'src/critical.ts',
      content: 'line 1\nline 2\nline 3\nline 4\n',
    }],
    findings: [
      finding({
        severity: 'critical',
        locations: [{ file_path: 'src/critical.ts', start_line: 3, end_line: 3 }],
      }),
    ],
    generated_at: '2026-06-20T00:00:00Z',
  });

  assert.equal(proposal.operations[0]?.file_path, 'src/critical.ts');
});

test('generatePatchProposalFromModel rejects comment-only edits when code on the high-severity line stays the same', async () => {
  const provider = new FakeProvider(
    '{"summary":"修复风险","rationale":"只改注释文本","operations":[{"file_path":"src/critical.ts","summary":"rewrite comment only","new_content":"line 1\\nline 2\\ndanger(); // clarified note\\nline 4\\n"}]}',
  );

  let failed = false;
  try {
    await generatePatchProposalFromModel(new AbortController().signal, provider, {
      repair_plan_id: 'job-1:repair',
      files: [{
        file_path: 'src/critical.ts',
        content: 'line 1\nline 2\ndanger(); // risky\nline 4\n',
      }],
      findings: [
        finding({
          severity: 'critical',
          locations: [{ file_path: 'src/critical.ts', start_line: 3, end_line: 3 }],
        }),
      ],
      generated_at: '2026-06-20T00:00:00Z',
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('must materially change every line in high-severity finding ranges');
  }

  assert.equal(failed, true);
});

test('createRepairIssue classifies missing provider key as a non-retryable provider_unavailable degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('No provider API key available for repair planner.'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'provider_unavailable');
  assert.equal(issue.error.retryable, false);
  assert.equal(issue.stage, 'proposal_generation');
});

test('createRepairIssue classifies authentication failure as non-retryable provider_auth_invalid', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('authentication failed for "anthropic" (HTTP 401): API key is invalid or expired'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'provider_auth_invalid');
  assert.equal(issue.error.retryable, false);
});

test('createRepairIssue classifies upstream timeout as retryable timeout degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('provider request timeout after 30000ms'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'timeout');
  assert.equal(issue.error.retryable, true);
});

test('createRepairIssue classifies HTTP 429 as retryable rate_limited degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('openai-compatible: status 429: rate limit exceeded'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'rate_limited');
  assert.equal(issue.error.retryable, true);
});

test('createRepairIssue classifies upstream 5xx as retryable provider_upstream_failed degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('anthropic: status 503: service temporarily unavailable'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'provider_upstream_failed');
  assert.equal(issue.error.retryable, true);
});

test('createRepairIssue classifies dns/network unreachable as retryable network_unreachable degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('openai-compatible: request failed: getaddrinfo ENOTFOUND api.example.com'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'network_unreachable');
  assert.equal(issue.error.retryable, true);
});

test('createRepairIssue classifies missing readable source files as non-retryable missing_evidence degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('No readable source files available for repair planner.'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'missing_evidence');
  assert.equal(issue.error.retryable, false);
});

test('createRepairIssue classifies ECONNRESET as retryable connection_interrupted degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('anthropic: request failed: read ECONNRESET'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'connection_interrupted');
  assert.equal(issue.error.retryable, true);
});

test('createRepairIssue classifies unexpected eof as retryable connection_interrupted degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('anthropic: upstream stream terminated unexpectedly: unexpected EOF while reading response body'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'connection_interrupted');
  assert.equal(issue.error.retryable, true);
});

test('createRepairIssue classifies TLS certificate failure as non-retryable tls_handshake_failed degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('request failed: certificate verify failed: self signed certificate'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'tls_handshake_failed');
  assert.equal(issue.error.retryable, false);
});

test('createRepairIssue classifies issuer certificate chain failure as non-retryable tls_handshake_failed degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('request failed: unable to get local issuer certificate'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'tls_handshake_failed');
  assert.equal(issue.error.retryable, false);
});

test('createRepairIssue classifies proxy rejection as retryable proxy_rejected degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('request failed: proxy connect ECONNREFUSED 127.0.0.1:7890'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'proxy_rejected');
  assert.equal(issue.error.retryable, true);
});

test('createRepairIssue classifies HTTP 407 as retryable proxy_rejected degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('anthropic: status 407: proxy authentication required'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'proxy_rejected');
  assert.equal(issue.error.retryable, true);
});

test('createRepairIssue classifies certificate revoked as non-retryable tls_cert_revoked degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('request failed: x509: certificate has been revoked'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'tls_cert_revoked');
  assert.equal(issue.error.retryable, false);
});

test('createRepairIssue classifies socket hang up as retryable connection_interrupted degradation', () => {
  const issue = createRepairIssue({
    stage: 'proposal_generation',
    repair_plan_id: 'job-1:repair',
    error: new Error('request failed: socket hang up'),
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.error.code, 'connection_interrupted');
  assert.equal(issue.error.retryable, true);
});

test('buildRepairIssueFromPreflight maps blocked gate decisions to policy_blocked issues', () => {
  const issue = buildRepairIssueFromPreflight({
    repair_plan_id: 'job-1:repair',
    preflight: {
      repair_plan_id: 'job-1:repair',
      findings: [finding()],
      gate_decision: {
        decision_id: 'decision-1',
        job_id: 'job-1',
        subject_type: 'repair_apply',
        subject_ref: 'job-1:repair:proposal',
        decision: 'block',
        reason: '修复 patch 超出当前 finding 范围',
        finding_ids: ['finding-1'],
        policy_snapshot_id: buildRulePolicySnapshotId({ plane: 'repair' }),
        decided_at: '2026-06-21T00:00:00Z',
      },
      test_results: [],
    },
    now: '2026-06-21T00:00:00Z',
  });

  assert.equal(issue.stage, 'preflight');
  assert.equal(issue.error.code, 'policy_blocked');
  assert.equal(issue.error.retryable, false);
});

test('buildRepairPreflightSummary exposes failed commands and blocking rule ids', () => {
  const summary = buildRepairPreflightSummary({
    repair_plan_id: 'job-1:repair',
    findings: [
      finding({ finding_id: 'finding-1', rule_id: 'repair.scope.out_of_scope_write' }),
      finding({ finding_id: 'finding-2', rule_id: 'repair.test.required_command_failed' }),
    ],
    gate_decision: {
      decision_id: 'decision-1',
      job_id: 'job-1',
      subject_type: 'repair_apply',
      subject_ref: 'job-1:repair:proposal',
      decision: 'block',
      reason: '修复 patch 超出当前 finding 范围',
      finding_ids: ['finding-1', 'finding-2'],
      policy_snapshot_id: buildRulePolicySnapshotId({ plane: 'repair' }),
      decided_at: '2026-06-21T00:00:00Z',
    },
    test_results: [
      { command: 'npm run test:risk', passed: false, stdout: '', stderr: 'failed' },
      { command: 'npx tsc --noEmit', passed: true, stdout: 'ok', stderr: '' },
    ],
  });

  assert.deepEqual(summary.failed_commands, ['npm run test:risk']);
  assert.deepEqual(summary.blocking_rule_ids, [
    'repair.scope.out_of_scope_write',
    'repair.test.required_command_failed',
  ]);
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
    findings: [finding({ locations: [{ file_path: 'src/app.ts', start_line: 1, end_line: 1 }] })],
    policy_snapshot_id: 'policy-1',
    now: '2026-06-20T00:00:10Z',
    runTest: async (command) => ({
      command,
      passed: true,
      stdout: 'ok',
      stderr: '',
    }),
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

test('runRepairPreflight blocks proposals that write outside the repair finding scope', async () => {
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
      file_path: 'src/other.ts',
      new_content: 'export const repaired = true;',
      summary: 'touch unrelated file',
    }],
  });
  const approved = approveRepairPlan(waiting);
  let executedTests = 0;

  const report = await runRepairPreflight({
    plan: approved,
    proposal: {
      patch_proposal_id: 'job-1:repair:proposal',
      repair_plan_id: draft.repair_plan_id,
      summary: '修复风险',
      rationale: '保持 patch 小而可回滚',
      generated_at: '2026-06-20T00:00:00Z',
      operations: [{
        operation_id: 'op-1',
        file_path: 'src/other.ts',
        new_content: 'export const repaired = true;',
        summary: 'touch unrelated file',
      }],
    },
    findings: [finding({ locations: [{ file_path: 'src/app.ts', start_line: 1, end_line: 1 }] })],
    policy_snapshot_id: 'policy-1',
    now: '2026-06-20T00:00:10Z',
    runTest: async () => {
      executedTests += 1;
      return { command: 'npm run test:risk', passed: true, stdout: 'ok', stderr: '' };
    },
  });

  assert.equal(report.gate_decision.decision, 'block');
  assert.equal(executedTests, 0);
});

test('applyRepairPlan refuses to write when required tests fail during preflight', async () => {
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

  let failed = false;
  let blockedDecision: string | undefined;
  try {
    await applyRepairPlan({
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
      findings: [finding({ locations: [{ file_path: 'src/app.ts', start_line: 1, end_line: 1 }] })],
      policy_snapshot_id: 'policy-1',
      now: '2026-06-20T00:00:10Z',
      runTest: async (command) => ({
        command,
        passed: false,
        stdout: '',
        stderr: 'failing verification gate',
      }),
      readFile: async (filePath) => writes.get(filePath) || '',
      writeFile: async (filePath, content) => {
        writes.set(filePath, content);
      },
    });
  } catch (error) {
    failed = String((error as Error).message || error).includes('Repair preflight failed');
    blockedDecision = error instanceof RepairApplyError ? error.preflight.gate_decision.decision : undefined;
  }

  assert.equal(failed, true);
  assert.equal(blockedDecision, 'block');
  assert.equal(writes.get('src/app.ts'), 'export const repaired = false;');
});

test('applyRepairPlan rolls back already-written files when a later write fails', async () => {
  const writes = new Map<string, string>([
    ['src/a.ts', 'export const a = false;'],
    ['src/b.ts', 'export const b = false;'],
  ]);
  const draft = createRepairPlan({
    job_id: 'job-1',
    findings: [
      finding({ locations: [{ file_path: 'src/a.ts', start_line: 1, end_line: 1 }] }),
      finding({ finding_id: 'finding-2', locations: [{ file_path: 'src/b.ts', start_line: 1, end_line: 1 }] }),
    ],
    workspace_path: '/tmp/workspace',
  });
  const waiting = attachPatchProposal(draft, {
    patch_proposal_id: 'job-1:repair:proposal',
    repair_plan_id: draft.repair_plan_id,
    summary: '修复风险',
    rationale: '保持 patch 小而可回滚',
    generated_at: '2026-06-20T00:00:00Z',
    operations: [
      {
        operation_id: 'op-1',
        file_path: 'src/a.ts',
        new_content: 'export const a = true;',
        summary: 'fix a',
      },
      {
        operation_id: 'op-2',
        file_path: 'src/b.ts',
        new_content: 'export const b = true;',
        summary: 'fix b',
      },
    ],
  });
  const approved = approveRepairPlan(waiting);

  let rollbackId: string | undefined;
  try {
    await applyRepairPlan({
      plan: approved,
      proposal: {
        patch_proposal_id: 'job-1:repair:proposal',
        repair_plan_id: draft.repair_plan_id,
        summary: '修复风险',
        rationale: '保持 patch 小而可回滚',
        generated_at: '2026-06-20T00:00:00Z',
        operations: [
          {
            operation_id: 'op-1',
            file_path: 'src/a.ts',
            new_content: 'export const a = true;',
            summary: 'fix a',
          },
          {
            operation_id: 'op-2',
            file_path: 'src/b.ts',
            new_content: 'export const b = true;',
            summary: 'fix b',
          },
        ],
      },
      findings: [
        finding({ locations: [{ file_path: 'src/a.ts', start_line: 1, end_line: 1 }] }),
        finding({ finding_id: 'finding-2', locations: [{ file_path: 'src/b.ts', start_line: 1, end_line: 1 }] }),
      ],
      policy_snapshot_id: 'policy-1',
      now: '2026-06-20T00:00:10Z',
      runTest: async (command) => ({
        command,
        passed: true,
        stdout: 'ok',
        stderr: '',
      }),
      readFile: async (filePath) => writes.get(filePath) || '',
      writeFile: async (filePath, content) => {
        if (filePath === 'src/b.ts') {
          throw new Error('disk full');
        }
        writes.set(filePath, content);
      },
    });
  } catch (error) {
    rollbackId = error instanceof RepairApplyExecutionError ? error.rollback.rollback_id : undefined;
  }

  assert.equal(rollbackId, 'job-1:repair:rollback');
  assert.equal(writes.get('src/a.ts'), 'export const a = false;');
  assert.equal(writes.get('src/b.ts'), 'export const b = false;');
});

test('applyRepairPlan reports rollback failures when cleanup cannot fully restore files', async () => {
  const writes = new Map<string, string>([
    ['src/a.ts', 'export const a = false;'],
    ['src/b.ts', 'export const b = false;'],
  ]);
  const draft = createRepairPlan({
    job_id: 'job-1',
    findings: [
      finding({ locations: [{ file_path: 'src/a.ts', start_line: 1, end_line: 1 }] }),
      finding({ finding_id: 'finding-2', locations: [{ file_path: 'src/b.ts', start_line: 1, end_line: 1 }] }),
    ],
    workspace_path: '/tmp/workspace',
  });
  const waiting = attachPatchProposal(draft, {
    patch_proposal_id: 'job-1:repair:proposal',
    repair_plan_id: draft.repair_plan_id,
    summary: '修复风险',
    rationale: '保持 patch 小而可回滚',
    generated_at: '2026-06-20T00:00:00Z',
    operations: [
      {
        operation_id: 'op-1',
        file_path: 'src/a.ts',
        new_content: 'export const a = true;',
        summary: 'fix a',
      },
      {
        operation_id: 'op-2',
        file_path: 'src/b.ts',
        new_content: 'export const b = true;',
        summary: 'fix b',
      },
    ],
  });
  const approved = approveRepairPlan(waiting);

  let rollbackFailures: string[] | undefined;
  try {
    await applyRepairPlan({
      plan: approved,
      proposal: {
        patch_proposal_id: 'job-1:repair:proposal',
        repair_plan_id: draft.repair_plan_id,
        summary: '修复风险',
        rationale: '保持 patch 小而可回滚',
        generated_at: '2026-06-20T00:00:00Z',
        operations: [
          {
            operation_id: 'op-1',
            file_path: 'src/a.ts',
            new_content: 'export const a = true;',
            summary: 'fix a',
          },
          {
            operation_id: 'op-2',
            file_path: 'src/b.ts',
            new_content: 'export const b = true;',
            summary: 'fix b',
          },
        ],
      },
      findings: [
        finding({ locations: [{ file_path: 'src/a.ts', start_line: 1, end_line: 1 }] }),
        finding({ finding_id: 'finding-2', locations: [{ file_path: 'src/b.ts', start_line: 1, end_line: 1 }] }),
      ],
      policy_snapshot_id: 'policy-1',
      now: '2026-06-20T00:00:10Z',
      runTest: async (command) => ({
        command,
        passed: true,
        stdout: 'ok',
        stderr: '',
      }),
      readFile: async (filePath) => writes.get(filePath) || '',
      writeFile: async (filePath, content) => {
        if (filePath === 'src/b.ts' && content === 'export const b = true;') {
          throw new Error('disk full');
        }
        if (filePath === 'src/a.ts' && content === 'export const a = false;') {
          throw new Error('rollback failed');
        }
        writes.set(filePath, content);
      },
    });
  } catch (error) {
    rollbackFailures = error instanceof RepairApplyExecutionError ? error.rollback_failures : undefined;
  }

  assert.deepEqual(rollbackFailures, ['src/a.ts']);
});
