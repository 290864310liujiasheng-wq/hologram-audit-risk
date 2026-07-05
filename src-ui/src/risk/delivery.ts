import { createHash } from 'crypto';
import {
  buildAuditQueryResult,
  type AuditQueryResult,
  type RecentAuditEntry,
} from './audit-bridge';
import type { RiskCheckResult } from './check-adapter';
import { buildCurrentReviewState, buildCurrentReviewSummaryResponse } from './current-review';
import { resolveRulePolicy, type ResolvedRulePolicy } from './rule-package';
import type { GateDecisionValue, RulePackage } from './review-core';

export interface DeliveryConfig {
  version: 'phase5.v1';
  workspace: {
    root: string;
    changed_files_source: 'git_status';
  };
  provider: {
    name: string;
    model: string;
    base_url: string;
    key_source: 'env' | 'secure_store';
    env_var?: string;
  };
  rule_packages: {
    review_paths: string[];
    repair_paths: string[];
    disabled_review_rule_ids: string[];
    disabled_repair_rule_ids: string[];
  };
  audit: {
    jsonl_path: string;
    report_output_path: string;
    recent_limit: number;
  };
  auth: {
    base_url: string;
  };
  automation: {
    verify_commands: string[];
    pre_commit_hook: string;
    ci_workflow: string;
    fail_on_decision: GateDecisionValue;
  };
}

export interface DeliveryProviderStatus {
  name: string;
  model: string;
  base_url: string;
  key_source: DeliveryConfig['provider']['key_source'];
  ready: boolean;
  reason: string;
  env_var?: string;
}

export interface DeliveryMachineReport {
  generated_at: string;
  workspace: DeliveryConfig['workspace'] & {
    audit_jsonl_path: string;
    report_output_path: string;
  };
  provider: DeliveryProviderStatus;
  policies: {
    review: ResolvedRulePolicy;
    repair: ResolvedRulePolicy;
  };
  current_review: ReturnType<typeof buildCurrentReviewSummaryResponse>;
  audit: AuditQueryResult & {
    integrity: DeliveryAuditIntegritySummary;
  };
  automation: DeliveryConfig['automation'] & {
    should_fail: boolean;
  };
  report_signature: DeliveryReportSignature;
}

type StringEnv = Record<string, string | undefined>;

export interface DeliveryRuleSummary {
  plane: 'review' | 'repair';
  policy_snapshot_id: string;
  package_ids: string[];
  rule_count: number;
  top_rule_ids: string[];
}

export interface DeliveryAuditSearchResult {
  query: string;
  total_matches: number;
  records: AuditQueryResult['records'];
}

export interface DeliveryDoctorReport {
  overall_status: 'ready' | 'needs_attention';
  blockers: string[];
  notes: string[];
}

export interface DeliveryAuditIntegritySummary {
  status: 'empty' | 'verified' | 'legacy_anchor' | 'failed';
  verified: boolean;
  entry_count: number;
  chained_entry_count: number;
  legacy_entry_count: number;
  last_hash?: string;
  issues: string[];
}

export interface DeliveryReportSignature {
  algorithm: 'sha256';
  digest: string;
}

export function createDefaultDeliveryConfig(workspaceRoot: string): DeliveryConfig {
  return {
    version: 'phase5.v1',
    workspace: {
      root: workspaceRoot,
      changed_files_source: 'git_status',
    },
    provider: {
      name: 'deepseek',
      model: 'deepseek-v4-pro',
      base_url: 'https://api.deepseek.com',
      key_source: 'env',
      env_var: 'DEEPSEEK_API_KEY',
    },
    rule_packages: {
      review_paths: ['.hologram/rules/review.workspace.json'],
      repair_paths: ['.hologram/rules/repair.workspace.json'],
      disabled_review_rule_ids: [],
      disabled_repair_rule_ids: [],
    },
    audit: {
      jsonl_path: '.hologram/audit.jsonl',
      report_output_path: '.hologram/latest-risk-report.json',
      recent_limit: 20,
    },
    auth: {
      base_url: '',
    },
    automation: {
      verify_commands: [
        'audit-risk check . --fail-on block',
        'audit-risk doctor .',
      ],
      pre_commit_hook: '.githooks/pre-commit',
      ci_workflow: '.github/workflows/hologram-risk.yml',
      fail_on_decision: 'block',
    },
  };
}

export function validateDeliveryConfig(config: DeliveryConfig): void {
  if (config.version !== 'phase5.v1') {
    throw new Error(`Unsupported delivery config version: ${config.version}`);
  }
  if (!config.workspace.root.trim()) {
    throw new Error('Delivery config requires a workspace root.');
  }
  if (!config.provider.name.trim() || !config.provider.model.trim() || !config.provider.base_url.trim()) {
    throw new Error('Delivery config requires provider name, model, and base URL.');
  }
  if (config.provider.key_source === 'env' && !config.provider.env_var?.trim()) {
    throw new Error('Delivery config requires provider.env_var when key_source is env.');
  }
  if (!config.audit.jsonl_path.trim() || !config.audit.report_output_path.trim()) {
    throw new Error('Delivery config requires audit jsonl and report output paths.');
  }
  if (typeof config.auth.base_url !== 'string') {
    throw new Error('Delivery config requires auth.base_url.');
  }
  if (config.audit.recent_limit <= 0) {
    throw new Error('Delivery config requires audit.recent_limit > 0.');
  }
  if (config.automation.verify_commands.length === 0) {
    throw new Error('Delivery config requires at least one verification command.');
  }
}

export function resolveDeliveryPolicies(input: {
  config: DeliveryConfig;
  readFile?: (path: string) => string;
}): {
  review: ResolvedRulePolicy;
  repair: ResolvedRulePolicy;
} {
  const readFile = input.readFile || ((path: string) => {
    throw new Error(`No readFile implementation provided for ${path}`);
  });

  const reviewPackages = loadRulePackages({
    workspaceRoot: input.config.workspace.root,
    paths: input.config.rule_packages.review_paths,
    readFile,
  });
  const repairPackages = loadRulePackages({
    workspaceRoot: input.config.workspace.root,
    paths: input.config.rule_packages.repair_paths,
    readFile,
  });

  return {
    review: resolveRulePolicy({
      plane: 'review',
      extension_packages: reviewPackages,
      disabled_rule_ids: input.config.rule_packages.disabled_review_rule_ids,
    }),
    repair: resolveRulePolicy({
      plane: 'repair',
      extension_packages: repairPackages,
      disabled_rule_ids: input.config.rule_packages.disabled_repair_rule_ids,
    }),
  };
}

export function buildDeliveryProviderStatus(
  config: DeliveryConfig,
  env: StringEnv = currentProcessEnv(),
): DeliveryProviderStatus {
  if (config.provider.key_source === 'env') {
    const envVar = config.provider.env_var || '';
    const ready = Boolean(env[envVar]?.trim());
    return {
      name: config.provider.name,
      model: config.provider.model,
      base_url: config.provider.base_url,
      key_source: 'env',
      ready,
      reason: ready
        ? `Provider key is available via ${envVar}.`
        : `Provider key is missing from ${envVar}.`,
      env_var: envVar,
    };
  }

  return {
    name: config.provider.name,
    model: config.provider.model,
    base_url: config.provider.base_url,
    key_source: 'secure_store',
    ready: true,
    reason: 'Provider key should be restored from secure storage.',
  };
}

export function shouldFailDeliveryGate(input: {
  decision: GateDecisionValue;
  threshold: GateDecisionValue;
}): boolean {
  return decisionRank(input.decision) >= decisionRank(input.threshold);
}

export function buildDeliveryMachineReport(input: {
  config: DeliveryConfig;
  checkResult: RiskCheckResult;
  auditEntries: RecentAuditEntry[];
  generatedAt: string;
  env?: StringEnv;
  readFile?: (path: string) => string;
}): DeliveryMachineReport {
  validateDeliveryConfig(input.config);
  const policies = resolveDeliveryPolicies({
    config: input.config,
    readFile: input.readFile,
  });
  const provider = buildDeliveryProviderStatus(input.config, input.env);
  const audit = buildAuditQueryResult({ entries: input.auditEntries });
  const auditIntegrity = buildDeliveryAuditIntegritySummary(input.auditEntries);
  const reviewState = buildCurrentReviewState({
    result: input.checkResult,
    workspace_path: input.config.workspace.root,
    review_policy: policies.review,
  });
  const currentReview = buildCurrentReviewSummaryResponse(reviewState, audit.records);
  const reportWithoutSignature = {
    generated_at: input.generatedAt,
    workspace: {
      ...input.config.workspace,
      audit_jsonl_path: input.config.audit.jsonl_path,
      report_output_path: input.config.audit.report_output_path,
    },
    provider,
    policies,
    current_review: currentReview,
    audit: {
      ...audit,
      integrity: auditIntegrity,
    },
    automation: {
      ...input.config.automation,
      should_fail: shouldFailDeliveryGate({
        decision: reviewState.gate_decision.decision,
        threshold: input.config.automation.fail_on_decision,
      }),
    },
  };

  return {
    ...reportWithoutSignature,
    report_signature: buildDeliveryReportSignature(reportWithoutSignature),
  };
}

export function buildDeliveryRuleSummaries(input: {
  policies: DeliveryMachineReport['policies'];
}): DeliveryRuleSummary[] {
  return (['review', 'repair'] as const).map((plane) => {
    const policy = input.policies[plane];
    return {
      plane,
      policy_snapshot_id: policy.policy_snapshot_id,
      package_ids: policy.packages.map((pkg) => pkg.package_id),
      rule_count: policy.rules.length,
      top_rule_ids: policy.rules.slice(0, 5).map((rule) => rule.rule_id),
    };
  });
}

export function searchDeliveryAuditRecords(input: {
  audit: AuditQueryResult;
  query: string;
  limit?: number;
}): DeliveryAuditSearchResult {
  const normalizedQuery = input.query.trim().toLowerCase();
  const limit = input.limit ?? 20;
  const records = normalizedQuery.length === 0
    ? input.audit.records.slice(0, limit)
    : input.audit.records.filter((record) => {
      const haystack = [
        record.plane,
        record.stage,
        record.status,
        record.subject,
        record.reason,
        record.error?.code || '',
      ].join(' ').toLowerCase();
      return haystack.includes(normalizedQuery);
    }).slice(0, limit);

  return {
    query: input.query,
    total_matches: normalizedQuery.length === 0 ? input.audit.records.length : input.audit.records.filter((record) => {
      const haystack = [
        record.plane,
        record.stage,
        record.status,
        record.subject,
        record.reason,
        record.error?.code || '',
      ].join(' ').toLowerCase();
      return haystack.includes(normalizedQuery);
    }).length,
    records,
  };
}

export function buildDeliveryDoctorReport(input: {
  report: DeliveryMachineReport;
}): DeliveryDoctorReport {
  const blockers: string[] = [];
  const notes: string[] = [];

  if (!input.report.provider.ready) {
    blockers.push(input.report.provider.reason);
  }

  if (input.report.audit.integrity.status === 'failed') {
    blockers.push(`Audit log integrity verification failed: ${input.report.audit.integrity.issues.join('; ')}`);
  } else if (input.report.audit.integrity.status === 'legacy_anchor') {
    notes.push('Audit log integrity is verified from the first chained entry forward; earlier legacy lines are only anchor-linked.');
  }

  if (input.report.current_review.status === 'ok') {
    const decision = input.report.current_review.review.gate_decision.decision;
    if (decision === 'block' || decision === 'require_approval') {
      blockers.push(`Current review gate decision is ${decision}.`);
    } else if (decision === 'warn') {
      notes.push('Current review contains warnings but is below the configured fail gate.');
    }
  } else {
    notes.push('Current review is empty; no live findings are available yet.');
  }

  const repairFailures = input.report.audit.records.filter((record) =>
    record.plane === 'repair' && (record.status === 'failed' || record.status === 'degraded'),
  );
  if (repairFailures.length > 0) {
    notes.push(`Recent repair audit contains ${repairFailures.length} failed/degraded record(s).`);
  }

  if (!input.report.automation.should_fail) {
    notes.push('Automation fail gate is currently passing.');
  }

  return {
    overall_status: blockers.length > 0 ? 'needs_attention' : 'ready',
    blockers,
    notes,
  };
}

function buildDeliveryAuditIntegritySummary(entries: RecentAuditEntry[]): DeliveryAuditIntegritySummary {
  if (entries.length === 0) {
    return {
      status: 'empty',
      verified: true,
      entry_count: 0,
      chained_entry_count: 0,
      legacy_entry_count: 0,
      issues: [],
    };
  }

  const issues: string[] = [];
  let chainedEntryCount = 0;
  let legacyEntryCount = 0;

  for (let index = 0; index < entries.length; index += 1) {
    const entry = normalizeAuditIntegrityEntry(entries[index]);
    const previous = index > 0 ? normalizeAuditIntegrityEntry(entries[index - 1]) : undefined;

    if (!entry.integrity_hash) {
      legacyEntryCount += 1;
      continue;
    }

    chainedEntryCount += 1;
    const expectedPrevHash = previous
      ? previous.integrity_hash || hashAuditLine(previous.raw_line || JSON.stringify(buildRawAuditLine(previous)))
      : null;
    if ((entry.prev_hash || null) !== expectedPrevHash) {
      issues.push(`${entry.tool}@${entry.ts} has mismatched prev_hash.`);
    }

    const expectedIntegrityHash = hashAuditLine(JSON.stringify(buildRawAuditLine(entry)));
    if (entry.integrity_hash !== expectedIntegrityHash) {
      issues.push(`${entry.tool}@${entry.ts} has mismatched integrity_hash.`);
    }
  }

  const lastHash = [...entries]
    .reverse()
    .map((entry) => normalizeAuditIntegrityEntry(entry))
    .find((entry) => entry.integrity_hash)?.integrity_hash;
  const status = issues.length > 0
    ? 'failed'
    : legacyEntryCount > 0
      ? 'legacy_anchor'
      : 'verified';

  return {
    status,
    verified: issues.length === 0,
    entry_count: entries.length,
    chained_entry_count: chainedEntryCount,
    legacy_entry_count: legacyEntryCount,
    last_hash: lastHash,
    issues,
  };
}

function buildDeliveryReportSignature(report: Omit<DeliveryMachineReport, 'report_signature'>): DeliveryReportSignature {
  return {
    algorithm: 'sha256',
    digest: hashAuditLine(JSON.stringify(report)),
  };
}

function normalizeAuditIntegrityEntry(entry: RecentAuditEntry) {
  return {
    ts: entry.ts,
    tool: entry.tool,
    path: entry.path,
    action: entry.action,
    reason: entry.reason,
    details: entry.details,
    prev_hash: entry.prev_hash ?? null,
    integrity_hash: typeof entry.integrity_hash === 'string' ? entry.integrity_hash : undefined,
    raw_line: typeof entry.raw_line === 'string' ? entry.raw_line : undefined,
  };
}

function buildRawAuditLine(entry: {
  ts: string;
  tool: string;
  path: string;
  action: string;
  reason: string;
  details?: Record<string, unknown>;
  prev_hash?: string | null;
}) {
  return {
    ts: entry.ts,
    tool: entry.tool,
    path: entry.path,
    action: entry.action,
    reason: entry.reason,
    details: entry.details,
    prev_hash: entry.prev_hash ?? null,
  };
}

function hashAuditLine(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

export function buildDeliveryInitFiles(input: {
  workspaceRoot: string;
  platformRoot: string;
  config?: DeliveryConfig;
}): Record<string, string> {
  const config = input.config || createDefaultDeliveryConfig(input.workspaceRoot);
  validateDeliveryConfig(config);
  const normalizedPlatformRoot = normalizePath(input.platformRoot);

  return {
    '.hologram/delivery.json': `${JSON.stringify(config, null, 2)}\n`,
    '.hologram/rules/review.workspace.json': `${JSON.stringify(createWorkspaceRulePackage({
      plane: 'review',
      package_id: 'review.workspace',
      description: 'Workspace-specific review overrides for external delivery.',
    }), null, 2)}\n`,
    '.hologram/rules/repair.workspace.json': `${JSON.stringify(createWorkspaceRulePackage({
      plane: 'repair',
      package_id: 'repair.workspace',
      description: 'Workspace-specific repair overrides for external delivery.',
    }), null, 2)}\n`,
    '.githooks/pre-commit': buildPreCommitHook(normalizedPlatformRoot),
    '.github/workflows/hologram-risk.yml': buildCiWorkflow(normalizedPlatformRoot),
  };
}

function buildPreCommitHook(platformRoot: string): string {
  return `#!/bin/sh
set -eu

PLATFORM_ROOT="\${AUDIT_RISK_PLATFORM_ROOT:-\${HOLOGRAM_PLATFORM_ROOT:-${platformRoot}}}"
WORKSPACE_ROOT="\${1:-$PWD}"

if [ -n "\${AUDIT_RISK_BIN:-}" ]; then
  "\$AUDIT_RISK_BIN" report "$WORKSPACE_ROOT" --config "$WORKSPACE_ROOT/.hologram/delivery.json" --output "$WORKSPACE_ROOT/.hologram/latest-risk-report.json" --fail-on block
else
  cargo run --quiet --manifest-path "$PLATFORM_ROOT/engine/Cargo.toml" --bin audit-risk -- report "$WORKSPACE_ROOT" --config "$WORKSPACE_ROOT/.hologram/delivery.json" --output "$WORKSPACE_ROOT/.hologram/latest-risk-report.json" --fail-on block
fi
`;
}

function buildCiWorkflow(_platformRoot: string): string {
  return `name: Hologram Risk Delivery

on:
  pull_request:
  push:
    branches: [main]

env:
  HOLOGRAM_PLATFORM_REPO: your-org/hologram-risk-platform
  HOLOGRAM_PLATFORM_REF: main

jobs:
  risk-delivery:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/checkout@v4
        with:
          repository: \${{ env.HOLOGRAM_PLATFORM_REPO }}
          ref: \${{ env.HOLOGRAM_PLATFORM_REF }}
          path: .hologram/platform
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: .hologram/platform/src-ui/package-lock.json
      - uses: dtolnay/rust-toolchain@stable
      - name: Install Hologram platform UI dependencies
        run: npm --prefix "$GITHUB_WORKSPACE/.hologram/platform/src-ui" ci
      - name: Run machine-readable risk delivery report
        run: cargo run --quiet --manifest-path "$GITHUB_WORKSPACE/.hologram/platform/engine/Cargo.toml" --bin audit-risk -- report "$GITHUB_WORKSPACE" --config "$GITHUB_WORKSPACE/.hologram/delivery.json" --output "$RUNNER_TEMP/phase5-risk-report.json" --fail-on block
      - name: Upload risk delivery artifact
        uses: actions/upload-artifact@v4
        with:
          name: phase5-risk-report
          path: \${{ runner.temp }}/phase5-risk-report.json
`;
}

function createWorkspaceRulePackage(input: {
  plane: 'review' | 'repair';
  package_id: string;
  description: string;
}): RulePackage {
  return {
    package_id: input.package_id,
    version: 'v1',
    plane: input.plane,
    source: 'workspace_extension',
    enabled: true,
    description: input.description,
    rules: [],
  };
}

function loadRulePackages(input: {
  workspaceRoot: string;
  paths: string[];
  readFile: (path: string) => string;
}): RulePackage[] {
  return input.paths.flatMap((relativePath) => {
    let raw = '';
    try {
      raw = input.readFile(resolveWorkspacePath(input.workspaceRoot, relativePath));
    } catch {
      return [];
    }
    const parsed = JSON.parse(raw) as RulePackage | RulePackage[];
    return Array.isArray(parsed) ? parsed : [parsed];
  });
}

function normalizePath(path: string): string {
  return path.replace(/\\/g, '/');
}

function decisionRank(decision: GateDecisionValue): number {
  switch (decision) {
    case 'allow':
      return 0;
    case 'warn':
      return 1;
    case 'require_approval':
      return 2;
    case 'block':
      return 3;
    default:
      return 99;
  }
}

function resolveWorkspacePath(workspaceRoot: string, relativePath: string): string {
  if (!relativePath) {
    return normalizePath(workspaceRoot);
  }
  if (relativePath.startsWith('/')) {
    return normalizePath(relativePath);
  }
  const root = normalizePath(workspaceRoot).replace(/\/+$/, '');
  const suffix = normalizePath(relativePath).replace(/^\/+/, '');
  return `${root}/${suffix}`;
}

function currentProcessEnv(): StringEnv {
  const runtime = globalThis as { process?: { env?: StringEnv } };
  return runtime.process?.env || {};
}
