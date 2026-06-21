import type { Chunk, Provider } from '../provider/types';
import { ChunkType } from '../provider/types';
import type {
  PatchOperation,
  PatchProposal,
  RepairPlan,
  RepairRollbackSnapshot,
  ReviewFinding,
} from './review-core';

export interface RepairGenerationInput {
  repair_plan_id: string;
  files: Array<{
    file_path: string;
    content: string;
  }>;
  findings: ReviewFinding[];
  generated_at: string;
}

export async function generatePatchProposalFromModel(
  signal: AbortSignal,
  provider: Provider,
  input: RepairGenerationInput,
): Promise<PatchProposal> {
  const prompt = [
    '你是 AI 编码风控平台的 Repair Planner。',
    '只输出 JSON，不要输出 markdown，不要解释。',
    '你必须返回一个 patch proposal，字段包括 summary、rationale、operations。',
    'operations 里的每个元素必须包含 file_path、summary、new_content，new_content 必须是完整文件内容。',
    '禁止新增未提供的文件路径，禁止返回空 operations。',
  ].join('\n');

  const transcript = JSON.stringify({
    repair_plan_id: input.repair_plan_id,
    findings: input.findings.map((finding) => ({
      finding_id: finding.finding_id,
      severity: finding.severity,
      explanation: finding.plain_explanation,
      recommendation: finding.recommendation,
      locations: finding.locations,
    })),
    files: input.files,
  });

  const chunks = provider.stream(signal, {
    messages: [
      { role: 'system', content: prompt },
      { role: 'user', content: transcript },
    ],
    tools: [],
    temperature: 0.1,
    max_tokens: 4000,
  });

  let raw = '';
  for await (const chunk of chunks) {
    if (chunk.type === ChunkType.Text && chunk.text) {
      raw += chunk.text;
    }
    if (chunk.type === ChunkType.Error) {
      throw chunk.err ?? new Error('repair planner stream failed');
    }
  }

  return parsePatchProposal(raw, {
    repair_plan_id: input.repair_plan_id,
    generated_at: input.generated_at,
  });
}

export function createRepairPlan(input: {
  job_id: string;
  findings: ReviewFinding[];
  workspace_path: string;
}): RepairPlan {
  const repairPlanId = `${input.job_id}:repair`;
  return {
    repair_plan_id: repairPlanId,
    job_id: input.job_id,
    finding_ids: input.findings.map((finding) => finding.finding_id),
    strategy: describeRepairStrategy(input.findings),
    patch_proposal_ref: `${input.workspace_path.replace(/\\/g, '/')}/.hologram/repair-plans/${repairPlanId}.json`,
    required_tests: deriveRequiredTests(input.findings),
    risk_note: buildRiskNote(input.findings),
    approval_state: 'draft',
  };
}

export function attachPatchProposal(
  plan: RepairPlan,
  proposal: PatchProposal,
): RepairPlan {
  if (proposal.operations.length === 0) {
    throw new Error('Patch proposal requires at least one file operation.');
  }

  return {
    ...plan,
    patch_proposal_ref: proposal.patch_proposal_id,
    approval_state: 'waiting_approval',
  };
}

export function approveRepairPlan(plan: RepairPlan): RepairPlan {
  if (plan.approval_state !== 'waiting_approval') {
    throw new Error(`Repair plan must be waiting_approval before approve, got ${plan.approval_state}.`);
  }
  return {
    ...plan,
    approval_state: 'approved',
  };
}

export function rejectRepairPlan(plan: RepairPlan): RepairPlan {
  if (plan.approval_state !== 'waiting_approval') {
    throw new Error(`Repair plan must be waiting_approval before reject, got ${plan.approval_state}.`);
  }
  return {
    ...plan,
    approval_state: 'rejected',
  };
}

export async function applyRepairPlan(input: {
  plan: RepairPlan;
  proposal: PatchProposal;
  now: string;
  readFile: (filePath: string) => Promise<string>;
  writeFile: (filePath: string, content: string) => Promise<void>;
}): Promise<{
  plan: RepairPlan;
  rollback: RepairRollbackSnapshot;
}> {
  if (input.plan.approval_state !== 'approved') {
    throw new Error(`Repair plan must be approved before apply, got ${input.plan.approval_state}.`);
  }

  const files = [];
  for (const operation of input.proposal.operations) {
    const previous = await input.readFile(operation.file_path);
    files.push({
      file_path: operation.file_path,
      content: previous,
    });
    await input.writeFile(operation.file_path, operation.new_content);
  }

  return {
    plan: {
      ...input.plan,
      approval_state: 'applied',
    },
    rollback: {
      rollback_id: `${input.plan.repair_plan_id}:rollback`,
      repair_plan_id: input.plan.repair_plan_id,
      files,
      created_at: input.now,
    },
  };
}

export async function rollbackRepairPlan(input: {
  plan: RepairPlan;
  rollback: RepairRollbackSnapshot;
  writeFile: (filePath: string, content: string) => Promise<void>;
}): Promise<RepairPlan> {
  for (const file of input.rollback.files) {
    await input.writeFile(file.file_path, file.content);
  }

  return {
    ...input.plan,
    approval_state: 'rolled_back',
  };
}

export function parsePatchProposal(
  raw: string,
  input: { repair_plan_id: string; generated_at: string },
): PatchProposal {
  const cleaned = raw.trim().replace(/^```json\s*/i, '').replace(/^```\s*/i, '').replace(/\s*```$/i, '');
  const parsed = JSON.parse(cleaned) as {
    summary?: string;
    rationale?: string;
    operations?: Array<{
      file_path?: string;
      summary?: string;
      new_content?: string;
    }>;
  };

  const operations = (parsed.operations || []).map((operation, index) => {
    if (!operation.file_path?.trim()) {
      throw new Error(`Patch operation ${index} is missing file_path.`);
    }
    if (operation.new_content === undefined) {
      throw new Error(`Patch operation ${index} is missing new_content.`);
    }
    return {
      operation_id: `${input.repair_plan_id}:op:${index}`,
      file_path: operation.file_path,
      new_content: operation.new_content,
      summary: operation.summary?.trim() || `Update ${operation.file_path}`,
    } satisfies PatchOperation;
  });

  if (operations.length === 0) {
    throw new Error('Patch proposal must contain at least one operation.');
  }

  return {
    patch_proposal_id: `${input.repair_plan_id}:proposal`,
    repair_plan_id: input.repair_plan_id,
    summary: parsed.summary?.trim() || 'Repair proposal',
    rationale: parsed.rationale?.trim() || 'Repair planner generated a structured patch proposal.',
    operations,
    generated_at: input.generated_at,
  };
}

function describeRepairStrategy(findings: ReviewFinding[]): string {
  const highest = [...findings].sort((left, right) => severityRank(right.severity) - severityRank(left.severity))[0];
  if (!highest) {
    return 'No findings selected for repair.';
  }

  if (highest.severity === 'critical') {
    return 'Prioritize the highest-risk finding first, keep blast radius narrow, and re-run the strongest local gates before any apply.';
  }
  if (highest.severity === 'high') {
    return 'Repair the highest-risk findings in a single bounded patch and re-check the same evidence path before apply.';
  }
  return 'Repair medium-risk findings conservatively and keep the proposal small enough for explicit review.';
}

function buildRiskNote(findings: ReviewFinding[]): string {
  const highest = [...findings].sort((left, right) => severityRank(right.severity) - severityRank(left.severity))[0];
  if (!highest) {
    return 'No repair risk identified.';
  }
  return `${highest.severity.toUpperCase()} finding requires explicit review: ${highest.plain_explanation}`;
}

function deriveRequiredTests(findings: ReviewFinding[]): string[] {
  const required = new Set<string>();

  for (const finding of findings) {
    for (const location of finding.locations) {
      if (/\.(ts|tsx|js|jsx|mjs)$/i.test(location.file_path)) {
        required.add('npx tsc --noEmit');
      }
      if (/\.(rs)$/i.test(location.file_path)) {
        required.add('cargo check');
      }
      if (/src-ui\/src\/risk\//.test(location.file_path.replace(/\\/g, '/'))) {
        required.add('npm run test:risk');
      }
      if (/(package(-lock)?\.json|pnpm-lock\.yaml|yarn\.lock|cargo\.(toml|lock))$/i.test(location.file_path)) {
        required.add('npm run build');
      }
    }
  }

  if (required.size === 0) {
    required.add('npm run build');
  }

  return Array.from(required);
}

function severityRank(severity: ReviewFinding['severity']): number {
  switch (severity) {
    case 'critical': return 4;
    case 'high': return 3;
    case 'medium': return 2;
    case 'low': return 1;
    default: return 0;
  }
}
