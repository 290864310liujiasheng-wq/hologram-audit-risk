import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createOpenAIProvider } from '../src/provider/openai';
import type { Provider, Request } from '../src/provider/types';
import { ChunkType } from '../src/provider/types';
import { buildRepairAuditPayload, buildReviewAuditPayload } from '../src/risk/audit-bridge';
import type { RiskCheckResult } from '../src/risk/check-adapter';
import {
  attachRepairIssueToCurrentReview,
  attachRepairPreflightIssueToCurrentReview,
  attachRepairProposalToCurrentReview,
  buildCurrentReviewState,
  buildCurrentReviewSummaryResponse,
  type CurrentReviewState,
} from '../src/risk/current-review';
import {
  applyRepairPlan,
  approveRepairPlan,
  attachPatchProposal,
  buildRepairGenerationMetadata,
  buildRepairIssueFromPreflight,
  buildRepairPreflightSummary,
  createRepairIssue,
  generatePatchProposalFromModel,
  RepairApplyError,
} from '../src/risk/self-heal';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, '..', '..');
const engineRoot = resolve(repoRoot, 'engine');
const evidenceDir = resolve(repoRoot, 'dev-docs', 'evidence');
const evidencePath = resolve(evidenceDir, 'phase3-runtime-samples.json');

class FixtureProvider implements Provider {
  constructor(
    private readonly providerName: string,
    private readonly text: string,
  ) {}

  name(): string {
    return this.providerName;
  }

  async *stream(_signal: AbortSignal, _req: Request) {
    yield { type: ChunkType.Text, text: this.text };
    yield { type: ChunkType.Done };
  }
}

function run(command: string, args: string[], cwd: string): string {
  return execFileSync(command, args, {
    cwd,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  }).trim();
}

function runShell(command: string, cwd: string): {
  passed: boolean;
  stdout: string;
  stderr: string;
} {
  try {
    const stdout = execFileSync('/bin/zsh', ['-lc', command], {
      cwd,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return {
      passed: true,
      stdout: stdout.trim(),
      stderr: '',
    };
  } catch (error) {
    const execError = error as {
      stdout?: string | Buffer;
      stderr?: string | Buffer;
      message?: string;
    };
    return {
      passed: false,
      stdout: String(execError.stdout || '').trim(),
      stderr: String(execError.stderr || execError.message || error).trim(),
    };
  }
}

function ensureDir(path: string): void {
  mkdirSync(path, { recursive: true });
}

function initGitRepo(input: {
  sample: string;
  filePath: string;
  baselineContent: string;
  riskyContent: string;
}): {
  root: string;
  baselineContent: string;
  riskyContent: string;
} {
  const root = mkdtempSync(join(tmpdir(), `hologram-phase3-${input.sample}-`));
  ensureDir(dirname(join(root, input.filePath)));
  writeFileSync(join(root, input.filePath), input.baselineContent, 'utf8');

  run('git', ['init', '-q'], root);
  run('git', ['config', 'user.email', 'phase3@example.com'], root);
  run('git', ['config', 'user.name', 'Phase3 Evidence'], root);
  run('git', ['add', '.'], root);
  run('git', ['commit', '-q', '-m', 'baseline'], root);

  writeFileSync(join(root, input.filePath), input.riskyContent, 'utf8');

  return {
    root,
    baselineContent: input.baselineContent,
    riskyContent: input.riskyContent,
  };
}

function readWorkspaceFile(root: string, filePath: string): string {
  return readFileSync(join(root, filePath), 'utf8');
}

function writeWorkspaceFile(root: string, filePath: string, content: string): void {
  writeFileSync(join(root, filePath), content, 'utf8');
}

function changedFiles(root: string): string[] {
  const output = run('git', ['diff', '--name-only'], root);
  return output
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);
}

function probeCheckResult(scenario: 'quiet' | 'l5_config' | 'l5_migration' | 'l4_coupling', files: string[]): RiskCheckResult {
  const raw = run('cargo', ['run', '--quiet', '--example', 'phase3_preflight_probe', '--', scenario, ...files], engineRoot);
  return JSON.parse(raw) as RiskCheckResult;
}

function keychainHasProvider(provider: string): boolean {
  try {
    execFileSync('/usr/bin/security', ['find-generic-password', '-s', 'com.hologram.app', '-a', provider], {
      stdio: ['ignore', 'ignore', 'ignore'],
    });
    return true;
  } catch {
    return false;
  }
}

function readKeychainProviderKey(provider: string): string {
  return execFileSync('/usr/bin/security', ['find-generic-password', '-w', '-s', 'com.hologram.app', '-a', provider], {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  }).trim();
}

function projectCheckPanel(state: CurrentReviewState): Record<string, unknown> {
  const projection: Record<string, unknown> = {
    approval_state: state.repair_plan.approval_state,
    required_tests: state.repair_plan.required_tests,
    summary_status: buildCurrentReviewSummaryResponse(state).status,
  };

  if (state.repair_generation_meta) {
    projection.generation = {
      provider_name: state.repair_generation_meta.provider_name,
      model: state.repair_generation_meta.model,
      file_count: state.repair_generation_meta.file_count,
      high_severity_finding_ids: state.repair_generation_meta.high_severity_finding_ids,
    };
  }

  if (state.repair_issue) {
    projection.issue = {
      badge: state.repair_issue.error.retryable ? '提案降级，可重试' : '提案失败，需修正',
      stage: `阶段 ${state.repair_issue.stage}`,
      summary: state.repair_issue.summary,
      code: state.repair_issue.error.code,
      retryable: state.repair_issue.error.retryable,
    };
  }

  if (state.repair_preflight) {
    const summary = buildRepairPreflightSummary(state.repair_preflight);
    projection.preflight = {
      reason: summary.reason,
      failed_commands: summary.failed_commands,
      blocking_rule_ids: summary.blocking_rule_ids,
      decision: summary.decision,
    };
  }

  return projection;
}

async function buildProposal(input: {
  providerName: string;
  providerModel: string;
  repairPlanId: string;
  filePath: string;
  currentContent: string;
  repairedContent: string;
  findings: CurrentReviewState['multi_agent_review']['merged_findings'];
}): Promise<{
  proposal: Awaited<ReturnType<typeof generatePatchProposalFromModel>>;
  generationMeta: ReturnType<typeof buildRepairGenerationMetadata>;
}> {
  const generatedAt = new Date().toISOString();
  const provider = new FixtureProvider(
    input.providerName,
    JSON.stringify({
      summary: `恢复 ${input.filePath} 到安全基线`,
      rationale: '撤回当前高风险改动，让当前 review 集合不再包含这次 critical 变更。',
      operations: [{
        file_path: input.filePath,
        summary: `恢复 ${input.filePath} 的安全内容`,
        new_content: input.repairedContent,
      }],
    }),
  );

  const files = [{
    file_path: input.filePath,
    content: input.currentContent,
  }];

  const proposal = await generatePatchProposalFromModel(new AbortController().signal, provider, {
    repair_plan_id: input.repairPlanId,
    files,
    findings: input.findings,
    generated_at: generatedAt,
  });

  const generationMeta = buildRepairGenerationMetadata({
    repair_plan_id: input.repairPlanId,
    provider_name: input.providerName,
    model: input.providerModel,
    files,
    findings: input.findings,
    generated_at: generatedAt,
  });

  return { proposal, generationMeta };
}

async function runCriticalRevertScenario(input: {
  sample: string;
  scenario: 'l5_config' | 'l5_migration';
  filePath: string;
  baselineContent: string;
  riskyContent: string;
}): Promise<Record<string, unknown>> {
  const repo = initGitRepo(input);
  try {
    const beforeCheck = probeCheckResult(input.scenario, [input.filePath]);
    const beforeState = buildCurrentReviewState({
      result: beforeCheck,
      workspace_path: repo.root,
    });
    const findings = beforeState.multi_agent_review.merged_findings.slice(0, 5);
    const { proposal, generationMeta } = await buildProposal({
      providerName: 'fixture-provider',
      providerModel: 'phase3-fixture-v1',
      repairPlanId: beforeState.repair_plan.repair_plan_id,
      filePath: input.filePath,
      currentContent: readWorkspaceFile(repo.root, input.filePath),
      repairedContent: repo.baselineContent,
      findings,
    });
    const planWaiting = attachPatchProposal(beforeState.repair_plan, proposal);
    const stateWithProposal = attachRepairProposalToCurrentReview(beforeState, {
      repair_plan: planWaiting,
      patch_proposal: proposal,
      repair_generation_meta: generationMeta,
    });
    const approvedPlan = approveRepairPlan(planWaiting);
    const applied = await applyRepairPlan({
      plan: approvedPlan,
      proposal,
      findings,
      policy_snapshot_id: 'policy:repair-apply:v1',
      now: new Date().toISOString(),
      runTest: async (command) => {
        const result = runShell(command, repo.root);
        return {
          command,
          passed: result.passed,
          stdout: result.stdout,
          stderr: result.stderr,
        };
      },
      readFile: async (filePath) => readWorkspaceFile(repo.root, filePath),
      writeFile: async (filePath, content) => writeWorkspaceFile(repo.root, filePath, content),
    });
    const afterChangedFiles = changedFiles(repo.root);
    const afterCheck = afterChangedFiles.length > 0
      ? probeCheckResult(input.scenario, afterChangedFiles)
      : probeCheckResult('quiet', []);

    return {
      finding_source: 'engine.run_full_check',
      workspace: repo.root,
      file_path: input.filePath,
      before_review: {
        gate_decision: beforeState.gate_decision,
        findings: beforeState.findings,
        audit: buildReviewAuditPayload(beforeCheck, beforeState.findings, repo.root, beforeState.gate_decision),
      },
      proposal_generation: {
        proposal,
        generation_meta: generationMeta,
        audit: buildRepairAuditPayload({
          tool: 'repair_plan',
          workspacePath: repo.root,
          action: 'allowed',
          reason: 'Repair proposal generated.',
          now: generationMeta.generated_at,
          details: {
            approval_state: planWaiting.approval_state,
            patch_proposal_id: proposal.patch_proposal_id,
            operation_count: proposal.operations.length,
            required_tests: planWaiting.required_tests,
            generation_meta: generationMeta,
          },
        }),
        check_panel: projectCheckPanel(stateWithProposal),
      },
      apply_success: {
        preflight: applied.preflight,
        audit: buildRepairAuditPayload({
          tool: 'repair_apply',
          workspacePath: repo.root,
          action: 'allowed',
          reason: 'Repair patch applied.',
          now: new Date().toISOString(),
          details: {
            approval_state: applied.plan.approval_state,
            rollback_id: applied.rollback.rollback_id,
            operation_count: proposal.operations.length,
            gate_decision: applied.preflight.gate_decision.decision,
            preflight_findings: applied.preflight.findings.map((finding) => ({
              finding_id: finding.finding_id,
              rule_id: finding.rule_id,
            })),
            validation_results: applied.preflight.test_results,
          },
        }),
      },
      semantic_recheck: {
        changed_files_before: beforeCheck.changed_files,
        changed_files_after: afterChangedFiles,
        findings_before: beforeCheck.l5_violations.length + beforeCheck.l4_violations.length,
        findings_after: afterCheck.l5_violations.length + afterCheck.l4_violations.length,
        passed_after: afterCheck.passed,
        one_line_after: (afterCheck as unknown as { one_line?: string }).one_line || '',
      },
    };
  } finally {
    rmSync(repo.root, { recursive: true, force: true });
  }
}

async function runBlockedPreflightScenario(): Promise<Record<string, unknown>> {
  const repo = initGitRepo({
    sample: 'preflight-blocked',
    filePath: 'config.yaml',
    baselineContent: 'featureFlag: false\n',
    riskyContent: 'featureFlag: true  \n',
  });

  try {
    const beforeCheck = probeCheckResult('l5_config', ['config.yaml']);
    const beforeState = buildCurrentReviewState({
      result: beforeCheck,
      workspace_path: repo.root,
    });
    const findings = beforeState.multi_agent_review.merged_findings.slice(0, 5);
    const { proposal, generationMeta } = await buildProposal({
      providerName: 'fixture-provider',
      providerModel: 'phase3-fixture-v1',
      repairPlanId: beforeState.repair_plan.repair_plan_id,
      filePath: 'config.yaml',
      currentContent: readWorkspaceFile(repo.root, 'config.yaml'),
      repairedContent: repo.baselineContent,
      findings,
    });
    const planWaiting = attachPatchProposal(beforeState.repair_plan, proposal);
    const approvedPlan = approveRepairPlan(planWaiting);
    const stateWithProposal = attachRepairProposalToCurrentReview(beforeState, {
      repair_plan: planWaiting,
      patch_proposal: proposal,
      repair_generation_meta: generationMeta,
    });

    try {
      await applyRepairPlan({
        plan: approvedPlan,
        proposal,
        findings,
        policy_snapshot_id: 'policy:repair-apply:v1',
        now: new Date().toISOString(),
        runTest: async (command) => {
          const result = runShell(command, repo.root);
          return {
            command,
            passed: result.passed,
            stdout: result.stdout,
            stderr: result.stderr,
          };
        },
        readFile: async (filePath) => readWorkspaceFile(repo.root, filePath),
        writeFile: async (filePath, content) => writeWorkspaceFile(repo.root, filePath, content),
      });
      throw new Error('expected preflight to block');
    } catch (error) {
      if (!(error instanceof RepairApplyError)) {
        throw error;
      }
      const issue = buildRepairIssueFromPreflight({
        repair_plan_id: approvedPlan.repair_plan_id,
        preflight: error.preflight,
        now: new Date().toISOString(),
      });
      const state = attachRepairPreflightIssueToCurrentReview(stateWithProposal, {
        issue,
        preflight: error.preflight,
      });

      return {
        finding_source: 'engine.run_full_check',
        workspace: repo.root,
        file_path: 'config.yaml',
        preflight: error.preflight,
        current_review: {
          repair_issue: state.repair_issue,
          repair_preflight: state.repair_preflight,
        },
        check_panel: projectCheckPanel(state),
        audit: buildRepairAuditPayload({
          tool: 'repair_apply',
          workspacePath: repo.root,
          action: 'denied',
          reason: 'Repair preflight failed.',
          now: issue.created_at,
          details: {
            approval_state: approvedPlan.approval_state,
            gate_decision: error.preflight.gate_decision.decision,
            gate_reason: error.preflight.gate_decision.reason,
            error_code: issue.error.code,
            error_stage: issue.stage,
            error_retryable: issue.error.retryable,
            preflight_findings: error.preflight.findings.map((finding) => ({
              finding_id: finding.finding_id,
              rule_id: finding.rule_id,
            })),
            validation_results: error.preflight.test_results,
          },
        }),
      };
    }
  } finally {
    rmSync(repo.root, { recursive: true, force: true });
  }
}

function providerFailureCases(): Array<{ name: string; error: Error }> {
  return [
    { name: 'auth_invalid', error: new Error('authentication failed for "anthropic" (HTTP 401): API key is invalid or expired') },
    { name: 'rate_limit', error: new Error('deepseek: status 429: rate limit exceeded') },
    { name: 'timeout', error: new Error('anthropic: request timed out after 30000ms') },
    { name: 'upstream_5xx', error: new Error('anthropic: status 503: service unavailable') },
    { name: 'proxy_407', error: new Error('deepseek: status 407: proxy authentication required') },
    { name: 'tls_handshake_failed', error: new Error('request failed: unable to get local issuer certificate') },
    { name: 'tls_cert_revoked', error: new Error('x509: certificate has been revoked') },
    { name: 'connection_reset', error: new Error('anthropic: request failed: read ECONNRESET') },
    { name: 'socket_hang_up', error: new Error('request failed: socket hang up') },
  ];
}

function buildProviderFailureEvidence(): Record<string, unknown> {
  const baseCheck = probeCheckResult('l4_coupling', ['src-ui/src/risk/phase3-evidence.ts']);
  const baseState = buildCurrentReviewState({
    result: baseCheck,
    workspace_path: '/tmp/phase3-provider-evidence',
  });
  const generationMeta = buildRepairGenerationMetadata({
    repair_plan_id: baseState.repair_plan.repair_plan_id,
    provider_name: 'fixture-provider',
    model: 'phase3-fixture-v1',
    files: [{ file_path: 'src-ui/src/risk/phase3-evidence.ts', content: 'export const risky = true;\n' }],
    findings: baseState.multi_agent_review.merged_findings.slice(0, 5),
    generated_at: new Date().toISOString(),
  });

  return Object.fromEntries(providerFailureCases().map(({ name, error }) => {
    const issue = createRepairIssue({
      stage: 'proposal_generation',
      repair_plan_id: baseState.repair_plan.repair_plan_id,
      error,
      now: new Date().toISOString(),
    });
    const state = attachRepairIssueToCurrentReview(baseState, {
      issue,
      repair_generation_meta: generationMeta,
    });

    return [
      name,
      {
        current_review: {
          issue,
          generation_meta: generationMeta,
        },
        check_panel: projectCheckPanel(state),
        audit: buildRepairAuditPayload({
          tool: 'repair_plan',
          workspacePath: '/tmp/phase3-provider-evidence',
          action: 'denied',
          reason: 'Repair proposal generation degraded.',
          now: issue.created_at,
          details: {
            approval_state: state.repair_plan.approval_state,
            error_code: issue.error.code,
            error_stage: issue.stage,
            error_retryable: issue.error.retryable,
            generation_meta: generationMeta,
          },
        }),
      },
    ];
  }));
}

async function runLiveProviderScenario(): Promise<Record<string, unknown>> {
  if (!keychainHasProvider('deepseek')) {
    return {
      status: 'blocked',
      reason: 'macOS Keychain does not currently expose a recoverable deepseek credential.',
    };
  }

  const repo = initGitRepo({
    sample: 'live-provider',
    filePath: 'config.yaml',
    baselineContent: 'featureFlag: false\n',
    riskyContent: 'featureFlag: true\n',
  });

  try {
    const beforeCheck = probeCheckResult('l5_config', ['config.yaml']);
    const beforeState = buildCurrentReviewState({
      result: beforeCheck,
      workspace_path: repo.root,
    });
    const findings = beforeState.multi_agent_review.merged_findings.slice(0, 5);
    const currentContent = readWorkspaceFile(repo.root, 'config.yaml');
    const generatedAt = new Date().toISOString();
    const provider = createOpenAIProvider({
      name: 'deepseek',
      apiKey: readKeychainProviderKey('deepseek'),
      baseUrl: 'https://api.deepseek.com/v1',
      model: 'deepseek-v4-pro',
    });
    const files = [{ file_path: 'config.yaml', content: currentContent }];

    try {
      const proposal = await generatePatchProposalFromModel(new AbortController().signal, provider, {
        repair_plan_id: beforeState.repair_plan.repair_plan_id,
        files,
        findings,
        generated_at: generatedAt,
      });
      const generationMeta = buildRepairGenerationMetadata({
        repair_plan_id: beforeState.repair_plan.repair_plan_id,
        provider_name: 'deepseek',
        model: 'deepseek-v4-pro',
        files,
        findings,
        generated_at: generatedAt,
      });
      const planWaiting = attachPatchProposal(beforeState.repair_plan, proposal);
      const stateWithProposal = attachRepairProposalToCurrentReview(beforeState, {
        repair_plan: planWaiting,
        patch_proposal: proposal,
        repair_generation_meta: generationMeta,
      });
      const approvedPlan = approveRepairPlan(planWaiting);
      const applied = await applyRepairPlan({
        plan: approvedPlan,
        proposal,
        findings,
        policy_snapshot_id: 'policy:repair-apply:v1',
        now: new Date().toISOString(),
        runTest: async (command) => {
          const result = runShell(command, repo.root);
          return {
            command,
            passed: result.passed,
            stdout: result.stdout,
            stderr: result.stderr,
          };
        },
        readFile: async (filePath) => readWorkspaceFile(repo.root, filePath),
        writeFile: async (filePath, content) => writeWorkspaceFile(repo.root, filePath, content),
      });
      const afterChangedFiles = changedFiles(repo.root);
      const afterCheck = afterChangedFiles.length > 0
        ? probeCheckResult('l5_config', afterChangedFiles)
        : probeCheckResult('quiet', []);

      return {
        status: 'success',
        provider_name: 'deepseek',
        model: 'deepseek-v4-pro',
        current_review: buildCurrentReviewSummaryResponse(stateWithProposal),
        check_panel: projectCheckPanel(stateWithProposal),
        audit: {
          repair_plan: buildRepairAuditPayload({
            tool: 'repair_plan',
            workspacePath: repo.root,
            action: 'allowed',
            reason: 'Repair proposal generated.',
            now: generatedAt,
            details: {
              approval_state: planWaiting.approval_state,
              patch_proposal_id: proposal.patch_proposal_id,
              operation_count: proposal.operations.length,
              required_tests: planWaiting.required_tests,
              generation_meta: generationMeta,
            },
          }),
          repair_apply: buildRepairAuditPayload({
            tool: 'repair_apply',
            workspacePath: repo.root,
            action: 'allowed',
            reason: 'Repair patch applied.',
            now: new Date().toISOString(),
            details: {
              approval_state: applied.plan.approval_state,
              rollback_id: applied.rollback.rollback_id,
              operation_count: proposal.operations.length,
              gate_decision: applied.preflight.gate_decision.decision,
              preflight_findings: applied.preflight.findings.map((finding) => ({
                finding_id: finding.finding_id,
                rule_id: finding.rule_id,
              })),
              validation_results: applied.preflight.test_results,
            },
          }),
        },
        semantic_recheck: {
          changed_files_before: beforeCheck.changed_files,
          changed_files_after: afterChangedFiles,
          findings_before: beforeCheck.l5_violations.length,
          findings_after: afterCheck.l5_violations.length,
          passed_after: afterCheck.passed,
          one_line_after: (afterCheck as unknown as { one_line?: string }).one_line || '',
        },
      };
    } catch (error) {
      const issue = createRepairIssue({
        stage: 'proposal_generation',
        repair_plan_id: beforeState.repair_plan.repair_plan_id,
        error,
        now: new Date().toISOString(),
      });
      const generationMeta = buildRepairGenerationMetadata({
        repair_plan_id: beforeState.repair_plan.repair_plan_id,
        provider_name: 'deepseek',
        model: 'deepseek-v4-pro',
        files,
        findings,
        generated_at: issue.created_at,
      });
      const state = attachRepairIssueToCurrentReview(beforeState, {
        issue,
        repair_generation_meta: generationMeta,
      });

      return {
        status: 'degraded',
        provider_name: 'deepseek',
        model: 'deepseek-v4-pro',
        current_review: buildCurrentReviewSummaryResponse(state),
        check_panel: projectCheckPanel(state),
        audit: buildRepairAuditPayload({
          tool: 'repair_plan',
          workspacePath: repo.root,
          action: 'denied',
          reason: 'Repair proposal generation degraded.',
          now: issue.created_at,
          details: {
            approval_state: state.repair_plan.approval_state,
            error_code: issue.error.code,
            error_stage: issue.stage,
            error_retryable: issue.error.retryable,
            generation_meta: generationMeta,
          },
        }),
      };
    }
  } finally {
    rmSync(repo.root, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  ensureDir(evidenceDir);

  const output = {
    generated_at: new Date().toISOString(),
    live_provider_blockers: {
      environment: {
        ANTHROPIC_API_KEY: Boolean(process.env.ANTHROPIC_API_KEY),
        OPENAI_API_KEY: Boolean(process.env.OPENAI_API_KEY),
        DEEPSEEK_API_KEY: Boolean(process.env.DEEPSEEK_API_KEY),
      },
      keychain: {
        anthropic: keychainHasProvider('anthropic'),
        deepseek: keychainHasProvider('deepseek'),
      },
      note: 'Current shell does not expose a live provider key; live evidence must therefore come from recoverable secure-store credentials when available.',
    },
    live_provider_attempt: await runLiveProviderScenario(),
    semantic_repair_samples: {
      critical_config_revert: await runCriticalRevertScenario({
        sample: 'critical-config',
        scenario: 'l5_config',
        filePath: 'config.yaml',
        baselineContent: 'featureFlag: false\n',
        riskyContent: 'featureFlag: true\n',
      }),
      critical_migration_revert: await runCriticalRevertScenario({
        sample: 'critical-migration',
        scenario: 'l5_migration',
        filePath: 'migrations/0001_init.sql',
        baselineContent: '-- baseline migration\nCREATE TABLE widgets (id INTEGER PRIMARY KEY);\n',
        riskyContent: '-- risky migration\nDROP TABLE widgets;\n',
      }),
    },
    preflight_block_sample: await runBlockedPreflightScenario(),
    provider_failure_samples: buildProviderFailureEvidence(),
  };

  writeFileSync(evidencePath, `${JSON.stringify(output, null, 2)}\n`, 'utf8');
  console.log(`phase3 evidence written to ${evidencePath}`);
}

await main();
