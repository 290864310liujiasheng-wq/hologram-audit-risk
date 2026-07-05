import type { Chunk, Provider, ProviderFailureCode } from '../provider/types';
import { ChunkType } from '../provider/types';
import ts from 'typescript';
import type {
  ContractError,
  PatchOperation,
  PatchProposal,
  RepairProposalValidationSummary,
  RepairExecutionStage,
  RepairIssue,
  RepairPreflightReport,
  RepairPlan,
  RepairRollbackSnapshot,
  ReviewFinding,
  Rule,
  ValidationCommandResult,
} from './review-core';
import { deriveGateDecision } from './review-core';
import {
  createRepairGateFailureFinding,
  evaluateRepairProposal,
  resolveRulePolicy,
} from './rule-package';

export interface RepairGenerationInput {
  repair_plan_id: string;
  files: Array<{
    file_path: string;
    content: string;
  }>;
  findings: ReviewFinding[];
  generated_at: string;
}

export interface RepairGenerationMetadata {
  repair_plan_id: string;
  provider_name: string;
  model: string;
  file_count: number;
  focus_file_paths: string[];
  high_severity_finding_ids: string[];
  generated_at: string;
}

export interface RepairPreflightSummary {
  decision: RepairPreflightReport['gate_decision']['decision'];
  reason: string;
  failed_commands: string[];
  blocking_rule_ids: string[];
}

export interface RepairGenerationReadiness {
  eligible: boolean;
  reason: string;
  finding_count: number;
  file_count: number;
}

export interface RepairProposalReviewInspection {
  summary: RepairProposalValidationSummary;
  findings: ReviewFinding[];
  gate_decision: ReturnType<typeof deriveGateDecision>;
}

export function deriveRepairFilePaths(input: {
  findings: ReviewFinding[];
  changed_files: string[];
}): string[] {
  const candidates = [
    ...input.findings.flatMap((finding) => finding.locations.map((location) => location.file_path)),
    ...input.changed_files,
  ];

  return Array.from(new Set(
    candidates
      .map((filePath) => filePath.trim())
      .filter((filePath) => filePath.length > 0 && filePath !== 'unknown'),
  ));
}

export function getRepairGenerationBlocker(input: {
  findings: ReviewFinding[];
  files: RepairGenerationInput['files'];
}): ContractError | null {
  if (input.findings.length === 0) {
    return {
      code: 'invalid_request',
      message: 'No findings selected for repair.',
      retryable: false,
    };
  }

  if (input.files.length === 0) {
    return {
      code: 'missing_evidence',
      message: 'Current findings do not map to readable source files for repair planning.',
      retryable: false,
      evidence_ids: input.findings.flatMap((finding) => finding.evidence_ids),
    };
  }

  return null;
}

export function buildRepairGenerationReadiness(input: {
  findings: ReviewFinding[];
  files: RepairGenerationInput['files'];
}): RepairGenerationReadiness {
  const blocker = getRepairGenerationBlocker(input);
  return {
    eligible: !blocker,
    reason: blocker?.message || 'Repair planner can generate a proposal from the current findings.',
    finding_count: input.findings.length,
    file_count: input.files.length,
  };
}

export class RepairApplyError extends Error {
  readonly preflight: RepairPreflightReport;

  constructor(message: string, preflight: RepairPreflightReport) {
    super(message);
    this.name = 'RepairApplyError';
    this.preflight = preflight;
  }
}

export class RepairApplyExecutionError extends Error {
  readonly rollback: RepairRollbackSnapshot;
  readonly rollback_failures: string[];

  constructor(message: string, rollback: RepairRollbackSnapshot, rollback_failures: string[]) {
    super(message);
    this.name = 'RepairApplyExecutionError';
    this.rollback = rollback;
    this.rollback_failures = rollback_failures;
  }
}

export class RepairProposalValidationError extends Error {
  readonly summary: RepairProposalValidationSummary;
  readonly findings: ReviewFinding[];
  readonly gate_decision: ReturnType<typeof deriveGateDecision>;
  readonly contract_error: ContractError;

  constructor(input: {
    message: string;
    summary: RepairProposalValidationSummary;
    findings: ReviewFinding[];
    gate_decision: ReturnType<typeof deriveGateDecision>;
    contract_error: ContractError;
  }) {
    super(input.message);
    this.name = 'RepairProposalValidationError';
    this.summary = input.summary;
    this.findings = input.findings;
    this.gate_decision = input.gate_decision;
    this.contract_error = input.contract_error;
  }
}

export function buildRepairGenerationMetadata(input: {
  repair_plan_id: string;
  provider_name: string;
  model: string;
  files: RepairGenerationInput['files'];
  findings: ReviewFinding[];
  generated_at: string;
}): RepairGenerationMetadata {
  return {
    repair_plan_id: input.repair_plan_id,
    provider_name: input.provider_name,
    model: input.model,
    file_count: input.files.length,
    focus_file_paths: input.files.map((file) => file.file_path),
    high_severity_finding_ids: input.findings
      .filter((finding) => finding.severity === 'critical' || finding.severity === 'high')
      .map((finding) => finding.finding_id),
    generated_at: input.generated_at,
  };
}

export function inspectRepairProposalForReview(input: {
  job_id: string;
  repair_plan_id: string;
  proposal: PatchProposal;
  findings: ReviewFinding[];
  policy_snapshot_id: string;
  now: string;
}): RepairProposalReviewInspection {
  const validationFindings = evaluateRepairProposal({
    plan_id: input.repair_plan_id,
    proposal: input.proposal,
    findings: input.findings,
  });
  const gateDecision = deriveGateDecision({
    job_id: input.job_id,
    subject_type: 'repair_apply',
    subject_ref: input.proposal.patch_proposal_id,
    findings: validationFindings,
    rules: resolveRulePolicy({ plane: 'repair' }).rules,
    policy_snapshot_id: input.policy_snapshot_id,
    decided_at: input.now,
  });
  const syntaxCheck = validatePatchProposalSyntax(input.proposal);
  const highSeverityCount = input.findings.filter((finding) => finding.severity === 'critical' || finding.severity === 'high').length;
  const logicSummary = `⚠️ 逻辑变更提示：提案会改动 ${input.proposal.operations.length} 个文件并触达 ${highSeverityCount} 条高风险 finding，请在审批前人工复核业务语义。`;
  const secondaryAuditPassed = gateDecision.decision !== 'block' && gateDecision.decision !== 'require_approval';

  let blockedReason: string | undefined;
  if (!secondaryAuditPassed) {
    blockedReason = '该修复方案引入了新的风险，已被系统自动拦截';
  } else if (!syntaxCheck.passed) {
    blockedReason = '该修复方案未通过语法检查，已被系统自动拦截';
  }

  return {
    summary: {
      secondary_audit: {
        passed: secondaryAuditPassed,
        summary: secondaryAuditPassed ? '✅ 二次审计通过' : '❌ 二次审计未通过',
      },
      syntax_check: {
        passed: syntaxCheck.passed,
        summary: syntaxCheck.passed ? '✅ 语法检查通过' : '❌ 语法检查未通过',
      },
      logic_change: {
        summary: logicSummary,
      },
      blocked: Boolean(blockedReason),
      blocked_reason: blockedReason,
    },
    findings: validationFindings,
    gate_decision: gateDecision,
  };
}

export function validateRepairProposalForReview(input: {
  job_id: string;
  repair_plan_id: string;
  proposal: PatchProposal;
  findings: ReviewFinding[];
  policy_snapshot_id: string;
  now: string;
}): RepairProposalValidationSummary {
  const inspection = inspectRepairProposalForReview(input);
  if (!inspection.summary.blocked) {
    return inspection.summary;
  }

  throw new RepairProposalValidationError({
    message: inspection.summary.blocked_reason || 'Repair proposal validation failed.',
    summary: inspection.summary,
    findings: inspection.findings,
    gate_decision: inspection.gate_decision,
    contract_error: {
      code: inspection.findings.length > 0 ? 'proposal_new_risk' : 'syntax_invalid',
      message: inspection.summary.blocked_reason || 'Repair proposal validation failed.',
      retryable: false,
      evidence_ids: inspection.findings.flatMap((finding) => finding.evidence_ids),
    },
  });
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

  const proposal = parsePatchProposal(raw, {
    repair_plan_id: input.repair_plan_id,
    generated_at: input.generated_at,
  });
  validatePatchProposalAgainstInput(proposal, input);
  return proposal;
}

export function createRepairIssue(input: {
  stage: RepairExecutionStage;
  repair_plan_id: string;
  error: unknown;
  now: string;
}): RepairIssue {
  const normalized = normalizeRepairError(input.error);
  return {
    issue_id: `${input.repair_plan_id}:${input.stage}:${input.now}`,
    repair_plan_id: input.repair_plan_id,
    stage: input.stage,
    summary: summarizeRepairError(input.stage, normalized),
    error: normalized,
    created_at: input.now,
  };
}

export function buildRepairIssueFromPreflight(input: {
  repair_plan_id: string;
  preflight: RepairPreflightReport;
  now: string;
}): RepairIssue {
  const blocked = input.preflight.gate_decision.decision === 'block';
  return {
    issue_id: `${input.repair_plan_id}:preflight:${input.now}`,
    repair_plan_id: input.repair_plan_id,
    stage: 'preflight',
    summary: `修复前复检失败：${input.preflight.gate_decision.reason}`,
    error: {
      code: blocked ? 'policy_blocked' : 'approval_required',
      message: input.preflight.gate_decision.reason,
      retryable: false,
      evidence_ids: input.preflight.findings.flatMap((finding) => finding.evidence_ids),
    },
    created_at: input.now,
  };
}

export function buildRepairPreflightSummary(preflight: RepairPreflightReport): RepairPreflightSummary {
  return {
    decision: preflight.gate_decision.decision,
    reason: preflight.gate_decision.reason,
    failed_commands: preflight.test_results.filter((result) => !result.passed).map((result) => result.command),
    blocking_rule_ids: Array.from(new Set(preflight.findings.map((finding) => finding.rule_id))),
  };
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
  findings: ReviewFinding[];
  policy_snapshot_id: string;
  now: string;
  runTest: (command: string) => Promise<ValidationCommandResult>;
  readFile: (filePath: string) => Promise<string>;
  writeFile: (filePath: string, content: string) => Promise<void>;
}): Promise<{
  plan: RepairPlan;
  rollback: RepairRollbackSnapshot;
  preflight: RepairPreflightReport;
}> {
  if (input.plan.approval_state !== 'approved') {
    throw new Error(`Repair plan must be approved before apply, got ${input.plan.approval_state}.`);
  }

  const preflight = await runRepairPreflight({
    plan: input.plan,
    proposal: input.proposal,
    findings: input.findings,
    policy_snapshot_id: input.policy_snapshot_id,
    now: input.now,
    runTest: input.runTest,
  });

  if (preflight.gate_decision.decision === 'block' || preflight.gate_decision.decision === 'require_approval') {
    throw new RepairApplyError(`Repair preflight failed: ${preflight.gate_decision.reason}`, preflight);
  }

  const files = [];
  try {
    for (const operation of input.proposal.operations) {
      const previous = await input.readFile(operation.file_path);
      files.push({
        file_path: operation.file_path,
        content: previous,
      });
      await input.writeFile(operation.file_path, operation.new_content);
    }
  } catch (error) {
    const rollback: RepairRollbackSnapshot = {
      rollback_id: `${input.plan.repair_plan_id}:rollback`,
      repair_plan_id: input.plan.repair_plan_id,
      files,
      created_at: input.now,
    };
    const rollbackFailures: string[] = [];

    for (const file of [...files].reverse()) {
      try {
        await input.writeFile(file.file_path, file.content);
      } catch {
        rollbackFailures.push(file.file_path);
      }
    }

    throw new RepairApplyExecutionError(
      `Repair apply failed after partial write: ${String((error as Error)?.message || error)}`,
      rollback,
      rollbackFailures,
    );
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
    preflight,
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
    const operationSummary = operation.summary?.trim() || `Update ${operation.file_path}`;
    if (!isHumanReadableRepairText(operationSummary, 8)) {
      throw new Error('Patch operation summary must describe the concrete repair action.');
    }
    return {
      operation_id: `${input.repair_plan_id}:op:${index}`,
      file_path: operation.file_path,
      new_content: operation.new_content,
      summary: operationSummary,
    } satisfies PatchOperation;
  });

  if (operations.length === 0) {
    throw new Error('Patch proposal must contain at least one operation.');
  }

  const summary = parsed.summary?.trim() || 'Repair proposal';
  if (!isHumanReadableRepairText(summary, 8)) {
    throw new Error('Patch proposal summary must contain a concrete human-readable explanation.');
  }

  const rationale = parsed.rationale?.trim() || 'Repair planner generated a structured patch proposal.';
  if (!isHumanReadableRepairText(rationale, 12)) {
    throw new Error('Patch proposal rationale must explain why the change repairs the risk.');
  }

  return {
    patch_proposal_id: `${input.repair_plan_id}:proposal`,
    repair_plan_id: input.repair_plan_id,
    summary,
    rationale,
    operations,
    generated_at: input.generated_at,
  };
}

export async function runRepairPreflight(input: {
  plan: RepairPlan;
  proposal: PatchProposal;
  findings: ReviewFinding[];
  policy_snapshot_id?: string;
  now: string;
  runTest: (command: string) => Promise<ValidationCommandResult>;
  rules?: Rule[];
}): Promise<RepairPreflightReport> {
  const resolvedPolicy = input.rules
    ? {
        rules: input.rules,
        policy_snapshot_id: input.policy_snapshot_id || 'policy:repair:custom',
      }
    : resolveRulePolicy({ plane: 'repair' });
  const rules = resolvedPolicy.rules;
  const proposalFindings = evaluateRepairProposal({
    plan_id: input.plan.repair_plan_id,
    proposal: input.proposal,
    findings: input.findings,
  });

  let gateDecision = deriveGateDecision({
    job_id: input.plan.job_id,
    subject_type: 'repair_apply',
    subject_ref: input.proposal.patch_proposal_id,
    findings: proposalFindings,
    rules,
    policy_snapshot_id: resolvedPolicy.policy_snapshot_id,
    decided_at: input.now,
  });

  if (gateDecision.decision === 'block' || gateDecision.decision === 'require_approval') {
    return {
      repair_plan_id: input.plan.repair_plan_id,
      findings: proposalFindings,
      gate_decision: gateDecision,
      test_results: [],
    };
  }

  if (input.plan.required_tests.length === 0) {
    const noTestFinding = createRepairGateFailureFinding({
      plan_id: input.plan.repair_plan_id,
      command: 'required_tests',
      stdout: '',
      stderr: 'Repair plan is missing required_tests.',
    });
    gateDecision = deriveGateDecision({
      job_id: input.plan.job_id,
      subject_type: 'repair_apply',
      subject_ref: input.proposal.patch_proposal_id,
      findings: [...proposalFindings, noTestFinding],
      rules,
      policy_snapshot_id: resolvedPolicy.policy_snapshot_id,
      decided_at: input.now,
    });
    return {
      repair_plan_id: input.plan.repair_plan_id,
      findings: [...proposalFindings, noTestFinding],
      gate_decision: gateDecision,
      test_results: [],
    };
  }

  const testResults: ValidationCommandResult[] = [];
  const testFindings: ReviewFinding[] = [];

  for (const command of input.plan.required_tests) {
    const result = await input.runTest(command);
    testResults.push(result);
    if (!result.passed) {
      testFindings.push(createRepairGateFailureFinding({
        plan_id: input.plan.repair_plan_id,
        command: result.command,
        stdout: result.stdout,
        stderr: result.stderr,
      }));
    }
  }

  gateDecision = deriveGateDecision({
    job_id: input.plan.job_id,
    subject_type: 'repair_apply',
    subject_ref: input.proposal.patch_proposal_id,
    findings: [...proposalFindings, ...testFindings],
    rules,
    policy_snapshot_id: resolvedPolicy.policy_snapshot_id,
    decided_at: input.now,
  });

  return {
    repair_plan_id: input.plan.repair_plan_id,
    findings: [...proposalFindings, ...testFindings],
    gate_decision: gateDecision,
    test_results: testResults,
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
  if (findings.length === 0) {
    return [];
  }

  const required = new Set<string>();

  for (const finding of findings) {
    for (const location of finding.locations) {
      const normalizedPath = location.file_path.replace(/\\/g, '/');
      if (/\.(ts|tsx|js|jsx|mjs)$/i.test(location.file_path)) {
        required.add('npx tsc --noEmit');
      }
      if (/\.(rs)$/i.test(location.file_path)) {
        required.add('cargo check');
      }
      if (/src-ui\/src\/risk\//.test(normalizedPath)) {
        required.add('npm run test:risk');
      }
      if (/(package(-lock)?\.json|pnpm-lock\.yaml|yarn\.lock|cargo\.(toml|lock))$/i.test(location.file_path)) {
        required.add('npm run build');
      }
      if (isDiffSensitiveRepairPath(normalizedPath)) {
        required.add('git diff --check');
      }
    }
  }
  return Array.from(required);
}

function isDiffSensitiveRepairPath(filePath: string): boolean {
  return /(?:^|\/)(?:migrations?|alembic)\//i.test(filePath)
    || /\b\d{4,}_.*\.(?:py|sql)$/i.test(filePath)
    || /\.(?:sql|proto|fbs|avsc|thrift|capnp|yaml|yml|toml|json|ini|cfg|conf)$/i.test(filePath)
    || /(?:^|\/)\.env(?:\.|$)/i.test(filePath)
    || /(?:^|\/)(?:settings|config)\.py$/i.test(filePath);
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

function validatePatchProposalAgainstInput(
  proposal: PatchProposal,
  input: RepairGenerationInput,
): void {
  const allowedFiles = new Map(
    input.files.map((file) => [normalizeRepairPath(file.file_path), file.content]),
  );
  const modifiedPaths = new Set<string>();
  const semanticChangedLinesByPath = new Map<string, {
    previous: Set<number>;
    next: Set<number>;
  }>();

  for (const operation of proposal.operations) {
    const normalizedPath = normalizeRepairPath(operation.file_path);
    if (!allowedFiles.has(normalizedPath)) {
      throw new Error(`Patch proposal operation for ${operation.file_path} is outside the provided repair file set.`);
    }

    const previousContent = allowedFiles.get(normalizedPath);
    if (previousContent === undefined) {
      throw new Error(`Patch proposal operation for ${operation.file_path} is missing source context.`);
    }
    if (previousContent === operation.new_content) {
      throw new Error(`Patch proposal operation for ${operation.file_path} does not change file content.`);
    }

    modifiedPaths.add(normalizedPath);
    semanticChangedLinesByPath.set(
      normalizedPath,
      detectChangedLines(previousContent, operation.new_content, normalizeMeaningfulLine),
    );
  }

  const focusFiles = new Set(
    input.findings
      .filter((finding) => finding.severity === 'critical' || finding.severity === 'high')
      .flatMap((finding) => finding.locations.map((location) => normalizeRepairPath(location.file_path)))
      .filter((filePath) => allowedFiles.has(filePath)),
  );

  if (focusFiles.size > 0 && !Array.from(focusFiles).every((filePath) => modifiedPaths.has(filePath))) {
    throw new Error('Patch proposal must modify every high-severity finding file.');
  }

  const focusLocations = input.findings
    .filter((finding) => finding.severity === 'critical' || finding.severity === 'high')
    .flatMap((finding) => finding.locations)
    .filter((location) => semanticChangedLinesByPath.has(normalizeRepairPath(location.file_path)));

  if (!focusLocations.every((location) => {
    const changedLines = semanticChangedLinesByPath.get(normalizeRepairPath(location.file_path));
    return changedLines ? changedLinesTouchLocation(changedLines, location) : false;
  })) {
    throw new Error('Patch proposal must materially change every line in high-severity finding ranges.');
  }
}

function normalizeRepairError(error: unknown): ContractError {
  if (error instanceof RepairProposalValidationError) {
    return error.contract_error;
  }

  if (isContractError(error)) {
    return error;
  }

  if (isProviderRequestError(error)) {
    return {
      code: providerFailureCodeToContractCode(error.code),
      message: error.message,
      retryable: error.retryable,
    };
  }

  const message = String((error as Error)?.message || error || 'Unknown repair failure.');
  const normalizedMessage = message.toLowerCase();

  if (
    normalizedMessage.includes('authentication failed')
    || normalizedMessage.includes('invalid or expired')
    || normalizedMessage.includes('unauthorized')
    || normalizedMessage.includes('http 401')
    || normalizedMessage.includes('http 403')
  ) {
    return {
      code: 'provider_auth_invalid',
      message,
      retryable: false,
    };
  }

  if (normalizedMessage.includes('api key')) {
    return {
      code: 'provider_unavailable',
      message,
      retryable: false,
    };
  }

  if (normalizedMessage.includes('timeout') || normalizedMessage.includes('timed out') || normalizedMessage.includes('超时')) {
    return {
      code: 'timeout',
      message,
      retryable: true,
    };
  }

  if (
    normalizedMessage.includes('rate limit')
    || normalizedMessage.includes('too many requests')
    || normalizedMessage.includes('http 429')
    || normalizedMessage.includes('status 429')
  ) {
    return {
      code: 'rate_limited',
      message,
      retryable: true,
    };
  }

  if (
    normalizedMessage.includes('revoked')
    || normalizedMessage.includes('revocation')
  ) {
    return {
      code: 'tls_cert_revoked',
      message,
      retryable: false,
    };
  }

  if (
    normalizedMessage.includes('socket hang up')
    || normalizedMessage.includes('broken pipe')
    || normalizedMessage.includes('connection closed')
    || normalizedMessage.includes('connection aborted')
    || normalizedMessage.includes('econnreset')
    || normalizedMessage.includes('unexpected eof')
    || normalizedMessage.includes('stream terminated unexpectedly')
  ) {
    return {
      code: 'connection_interrupted',
      message,
      retryable: true,
    };
  }

  if (
    normalizedMessage.includes('proxy')
    && (
      normalizedMessage.includes('econnrefused')
      || normalizedMessage.includes('status 407')
      || normalizedMessage.includes('407')
      || normalizedMessage.includes('proxy connect')
    )
  ) {
    return {
      code: 'proxy_rejected',
      message,
      retryable: true,
    };
  }

  if (
    normalizedMessage.includes('certificate')
    || normalizedMessage.includes('ssl')
    || normalizedMessage.includes('tls')
    || normalizedMessage.includes('x509')
  ) {
    return {
      code: 'tls_handshake_failed',
      message,
      retryable: false,
    };
  }

  if (
    normalizedMessage.includes('enotfound')
    || normalizedMessage.includes('econnrefused')
    || normalizedMessage.includes('econnreset')
    || normalizedMessage.includes('network is unreachable')
    || normalizedMessage.includes('dns')
    || normalizedMessage.includes('getaddrinfo')
  ) {
    return {
      code: 'network_unreachable',
      message,
      retryable: true,
    };
  }

  if (
    normalizedMessage.includes('status 500')
    || normalizedMessage.includes('status 502')
    || normalizedMessage.includes('status 503')
    || normalizedMessage.includes('status 504')
    || normalizedMessage.includes('service unavailable')
    || normalizedMessage.includes('bad gateway')
    || normalizedMessage.includes('gateway timeout')
  ) {
    return {
      code: 'provider_upstream_failed',
      message,
      retryable: true,
    };
  }

  if (normalizedMessage.includes('no readable source files')) {
    return {
      code: 'missing_evidence',
      message,
      retryable: false,
    };
  }

  if (
    normalizedMessage.includes('stream failed')
    || normalizedMessage.includes('provider')
    || normalizedMessage.includes('fetch')
    || normalizedMessage.includes('network')
  ) {
    return {
      code: 'provider_unavailable',
      message,
      retryable: true,
    };
  }

  return {
    code: 'internal_error',
    message,
    retryable: true,
  };
}

function summarizeRepairError(stage: RepairExecutionStage, error: ContractError): string {
  const prefix = stage === 'proposal_generation'
    ? '修复提案已降级'
    : stage === 'preflight'
      ? '修复前复检失败'
      : stage === 'apply'
        ? '修复应用失败'
        : '修复回滚失败';

  const suffix = error.retryable ? '可重试。' : '需要先修正输入或配置。';
  return `${prefix}：${error.message} ${suffix}`;
}

function validatePatchProposalSyntax(proposal: PatchProposal): { passed: boolean; detail?: string } {
  const failures: string[] = [];

  for (const operation of proposal.operations) {
    const filePath = normalizeRepairPath(operation.file_path);
    const extension = filePath.split('.').pop()?.toLowerCase() || '';
    const content = operation.new_content;

    try {
      if (extension === 'json') {
        JSON.parse(content);
        continue;
      }

      if (['ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs'].includes(extension)) {
        const sourceFile = ts.createSourceFile(
          filePath,
          content,
          ts.ScriptTarget.ES2022,
          true,
          extension === 'tsx'
            ? ts.ScriptKind.TSX
            : extension === 'jsx'
              ? ts.ScriptKind.JSX
              : extension === 'js' || extension === 'mjs' || extension === 'cjs'
                ? ts.ScriptKind.JS
                : ts.ScriptKind.TS,
        );
        const syntaxDiagnostics = ((sourceFile as unknown as { parseDiagnostics?: ts.Diagnostic[] }).parseDiagnostics || [])
          .filter((diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error);
        if (syntaxDiagnostics.length > 0) {
          failures.push(`${filePath}: ${ts.flattenDiagnosticMessageText(syntaxDiagnostics[0].messageText, '\n')}`);
        }
      }
    } catch (error) {
      failures.push(`${filePath}: ${String((error as Error).message || error)}`);
    }
  }

  if (failures.length > 0) {
    return {
      passed: false,
      detail: failures[0],
    };
  }

  return {
    passed: true,
  };
}

function isContractError(value: unknown): value is ContractError {
  return Boolean(
    value
    && typeof value === 'object'
    && 'code' in value
    && 'message' in value
    && 'retryable' in value,
  );
}

function isProviderRequestError(value: unknown): value is { code: ProviderFailureCode; message: string; retryable: boolean } {
  return Boolean(
    value
    && typeof value === 'object'
    && 'code' in value
    && 'message' in value
    && 'retryable' in value
    && typeof (value as { code?: unknown }).code === 'string',
  );
}

function providerFailureCodeToContractCode(code: ProviderFailureCode): ContractError['code'] {
  return code;
}

function normalizeRepairPath(filePath: string): string {
  return filePath.replace(/\\/g, '/').replace(/^\.\//, '');
}

function detectChangedLines(
  previousContent: string,
  nextContent: string,
  normalize: (line: string) => string = identityLine,
): { previous: Set<number>; next: Set<number> } {
  const previousLines = previousContent.split('\n');
  const nextLines = nextContent.split('\n');
  const normalizedPreviousLines = previousLines.map(normalize);
  const normalizedNextLines = nextLines.map(normalize);
  const lcs = buildLcsTable(normalizedPreviousLines, normalizedNextLines);
  const changedPrevious = new Set<number>();
  const changedNext = new Set<number>();

  let i = 0;
  let j = 0;
  while (i < previousLines.length && j < nextLines.length) {
    if (normalizedPreviousLines[i] === normalizedNextLines[j]) {
      i += 1;
      j += 1;
      continue;
    }

    if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      changedPrevious.add(i + 1);
      i += 1;
    } else {
      changedNext.add(j + 1);
      j += 1;
    }
  }

  while (i < previousLines.length) {
    changedPrevious.add(i + 1);
    i += 1;
  }

  while (j < nextLines.length) {
    changedNext.add(j + 1);
    j += 1;
  }

  return {
    previous: changedPrevious,
    next: changedNext,
  };
}

function changedLinesTouchLocation(
  changedLines: { previous: Set<number>; next: Set<number> },
  target: { start_line: number; end_line: number },
): boolean {
  for (let line = target.start_line; line <= target.end_line; line += 1) {
    if (!(changedLines.previous.has(line) || changedLines.next.has(line))) {
      return false;
    }
  }
  return true;
}

function buildLcsTable(previousLines: string[], nextLines: string[]): number[][] {
  const table = Array.from({ length: previousLines.length + 1 }, () =>
    Array.from({ length: nextLines.length + 1 }, () => 0),
  );

  for (let i = previousLines.length - 1; i >= 0; i -= 1) {
    for (let j = nextLines.length - 1; j >= 0; j -= 1) {
      table[i][j] = previousLines[i] === nextLines[j]
        ? table[i + 1][j + 1] + 1
        : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }

  return table;
}

function identityLine(line: string): string {
  return line;
}

function normalizeMeaningfulLine(line: string): string {
  return line
    .replace(/\/\/.*$/g, '')
    .replace(/#.*$/g, '')
    .replace(/\/\*.*?\*\//g, '')
    .replace(/\s+/g, '');
}

function isHumanReadableRepairText(text: string, minLength: number): boolean {
  const normalized = text.trim();
  if (/^[A-Za-z0-9_.-]+$/.test(normalized)) {
    return false;
  }

  const lowered = normalized.toLowerCase();
  if ([
    'fix',
    'update',
    'patch',
    'todo',
    'tbd',
    'n/a',
    'ok',
  ].includes(lowered)) {
    return false;
  }

  const compact = normalized.replace(/\s+/g, '');
  const hasCjk = /[\u3400-\u9fff]/.test(normalized);
  if (compact.length < minLength) {
    return hasCjk && compact.length >= 4;
  }

  return true;
}
