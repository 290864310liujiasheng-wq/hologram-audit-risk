import type { GateDecision, RepairPlan, ReviewFinding, Severity, ValidationCommandResult } from './review-core';
import type { RepairGenerationMetadata } from './self-heal';
import type { RiskCheckResult } from './check-adapter';
import { summarizeSeverityCounts } from './check-adapter';

export interface ReviewAuditPayload {
  tool: string;
  target_path: string;
  action: 'allowed' | 'denied';
  reason: string;
  details: {
    timestamp: string;
    finding_ids: string[];
    evidence_ids: string[];
    counts: Record<Severity, number>;
    state_change?: AuditStateChange;
    gate_decision?: {
      decision: GateDecision['decision'];
      reason: string;
      finding_ids: string[];
    };
    policy_snapshot_id?: string;
  };
}

export interface ApprovalAuditPayload {
  tool: string;
  target_path: string;
  action: 'allowed' | 'denied';
  reason: string;
  details: {
    subject: string;
    remember: boolean;
  };
}

export interface AuditStateChange {
  from_state?: string;
  to_state?: string;
}

export interface RepairAuditPayload {
  tool: string;
  target_path: string;
  action: 'allowed' | 'denied';
  reason: string;
  details: {
    timestamp: string;
    approval_state?: RepairPlan['approval_state'];
    patch_proposal_id?: string;
    operation_count?: number;
    required_tests?: string[];
    generation_meta?: RepairGenerationMetadata;
    remember?: boolean;
    rollback_id?: string;
    gate_decision?: string;
    gate_reason?: string;
    error_code?: string;
    error_stage?: 'proposal_generation' | 'preflight' | 'apply' | 'rollback';
    error_retryable?: boolean;
    finding_ids?: string[];
    evidence_ids?: string[];
    state_change?: AuditStateChange;
    preflight_findings?: Array<{
      finding_id: string;
      rule_id: string;
    }>;
    validation_results?: ValidationCommandResult[];
  };
}

export interface RecentAuditEntry {
  ts: string;
  tool: string;
  path: string;
  action: string;
  reason: string;
  details?: Record<string, unknown>;
}

export interface AuditRecord {
  event_id: string;
  timestamp: string;
  plane: 'review' | 'approval' | 'repair';
  stage: string;
  status: string;
  subject: string;
  reason: string;
  finding_ids: string[];
  evidence_ids: string[];
  policy_snapshot_id?: string;
  state_change?: AuditStateChange;
  error?: {
    code?: string;
    stage?: string;
    retryable?: boolean;
  };
  raw: RecentAuditEntry;
}

export interface AuditQueryResult {
  entries: RecentAuditEntry[];
  records: AuditRecord[];
}

export interface AuditDisplayRow {
  timestamp: string;
  toolLabel: string;
  actionLabel: string;
  subject: string;
  reason: string;
}

export interface RepairHistoryItem {
  timestamp: string;
  stage: string;
  status: string;
  subject: string;
  reason: string;
  state_change?: AuditStateChange;
  error?: AuditRecord['error'];
}

export function buildReviewAuditPayload(
  result: RiskCheckResult,
  findings: ReviewFinding[],
  targetPath: string,
  gateDecision?: GateDecision,
): ReviewAuditPayload {
  const counts = summarizeSeverityCounts(findings);
  const evidenceIds = Array.from(new Set(findings.flatMap((finding) => finding.evidence_ids)));
  const findingIds = findings.map((finding) => finding.finding_id);
  const action = gateDecision
    ? (gateDecision.decision === 'block' || gateDecision.decision === 'require_approval' ? 'denied' : 'allowed')
    : (result.passed ? 'allowed' : 'denied');
  const reason = gateDecision
    ? gateDecision.reason
    : result.passed
      ? 'Review check passed without blocking findings.'
      : `Review check found ${findings.length} finding(s).`;

  return {
    tool: 'review_check',
    target_path: targetPath,
    action,
    reason,
    details: {
      timestamp: result.timestamp,
      finding_ids: findingIds,
      evidence_ids: evidenceIds,
      counts,
      state_change: {
        from_state: 'running',
        to_state: gateDecision?.decision || (action === 'allowed' ? 'allow' : 'block'),
      },
      gate_decision: gateDecision
        ? {
            decision: gateDecision.decision,
            reason: gateDecision.reason,
            finding_ids: gateDecision.finding_ids,
          }
        : undefined,
      policy_snapshot_id: gateDecision?.policy_snapshot_id,
    },
  };
}

export function buildApprovalAuditPayload(input: {
  workspacePath: string;
  toolName: string;
  subject: string;
  allow: boolean;
  remember: boolean;
}): ApprovalAuditPayload {
  return {
    tool: `approval.${input.toolName}`,
    target_path: input.workspacePath,
    action: input.allow ? 'allowed' : 'denied',
    reason: input.allow ? 'User approved tool execution.' : 'User denied tool execution.',
    details: {
      subject: input.subject,
      remember: input.remember,
    },
  };
}

export function buildRepairAuditPayload(input: {
  tool: string;
  workspacePath: string;
  action: 'allowed' | 'denied';
  reason: string;
  now: string;
  details: Omit<RepairAuditPayload['details'], 'timestamp'>;
}): RepairAuditPayload {
  return {
    tool: input.tool,
    target_path: input.workspacePath,
    action: input.action,
    reason: input.reason,
    details: {
      timestamp: input.now,
      ...input.details,
    },
  };
}

export function buildAuditQueryResult(input: {
  entries: RecentAuditEntry[];
}): AuditQueryResult {
  const records = input.entries
    .filter((entry) =>
      entry.tool === 'review_check'
      || entry.tool.startsWith('approval.')
      || entry.tool.startsWith('repair_'),
    )
    .sort((a, b) => b.ts.localeCompare(a.ts))
    .map((entry) => normalizeAuditRecord(entry));

  return {
    entries: input.entries,
    records,
  };
}

export function parseAuditQueryResult(data: {
  entries?: RecentAuditEntry[];
  records?: AuditRecord[];
}): AuditQueryResult {
  const entries = data.entries || [];
  const records = data.records || buildAuditQueryResult({ entries }).records;

  return {
    entries,
    records,
  };
}

export function summarizeRecentAuditEntries(records: AuditRecord[]): AuditDisplayRow[] {
  return records
    .slice(0, 5)
    .map((record) => ({
      timestamp: record.timestamp,
      toolLabel: record.plane === 'review'
        ? '审查'
        : record.plane === 'approval'
          ? '审批'
          : '修复',
      actionLabel: gateDecisionActionLabel(record.status),
      subject: record.subject,
      reason: record.reason,
    }));
}

export function buildRepairHistory(records: AuditRecord[]): RepairHistoryItem[] {
  return records
    .filter((record) => record.plane === 'repair')
    .slice(0, 5)
    .map((record) => ({
      timestamp: record.timestamp,
      stage: record.stage,
      status: record.status,
      subject: record.subject,
      reason: record.reason,
      state_change: record.state_change,
      error: record.error,
    }));
}

function gateDecisionActionLabel(decision: string): string {
  switch (decision) {
    case 'block':
      return '阻断';
    case 'require_approval':
      return '需审批';
    case 'warn':
      return '警告';
    case 'approved':
      return '已批准';
    case 'rejected':
      return '已拒绝';
    case 'applied':
      return '已应用';
    case 'rolled_back':
      return '已回滚';
    case 'degraded':
      return '降级';
    case 'failed':
      return '失败';
    default:
      return '允许';
  }
}

function normalizeAuditRecord(entry: RecentAuditEntry): AuditRecord {
  const details = entry.details || {};
  const gateDecision = details.gate_decision;
  const plane = entry.tool === 'review_check'
    ? 'review'
    : entry.tool.startsWith('approval.')
      ? 'approval'
      : 'repair';
  const stage = deriveAuditStage(entry);
  const status = deriveAuditStatus(entry);
  const subject = String(
    details.subject
    || details.patch_proposal_id
    || entry.path
    || 'workspace',
  );
  const findingIds = asStringArray(details.finding_ids)
    || (isRecord(gateDecision) ? asStringArray(gateDecision.finding_ids) : undefined)
    || asStringArray(details.preflight_findings, 'finding_id')
    || asStringArray(details.generation_meta && isRecord(details.generation_meta) ? details.generation_meta.high_severity_finding_ids : undefined)
    || [];
  const evidenceIds = asStringArray(details.evidence_ids) || [];
  const policySnapshotId = typeof details.policy_snapshot_id === 'string'
    ? details.policy_snapshot_id
    : undefined;
  const stateChange = readStateChange(details) || inferStateChange(entry);
  const error = typeof details.error_code === 'string'
    ? {
        code: details.error_code,
        stage: typeof details.error_stage === 'string' ? details.error_stage : undefined,
        retryable: typeof details.error_retryable === 'boolean' ? details.error_retryable : undefined,
      }
    : undefined;

  return {
    event_id: `${entry.tool}:${entry.ts}:${subject}`,
    timestamp: entry.ts,
    plane,
    stage,
    status,
    subject,
    reason: entry.reason,
    finding_ids: findingIds,
    evidence_ids: evidenceIds,
    policy_snapshot_id: policySnapshotId,
    state_change: stateChange,
    error,
    raw: entry,
  };
}

function deriveAuditStage(entry: RecentAuditEntry): string {
  const details = entry.details || {};
  if (entry.tool === 'review_check') return 'review';
  if (entry.tool.startsWith('approval.')) return 'approval';
  if (entry.tool === 'repair_plan') {
    return typeof details.error_stage === 'string' ? details.error_stage : 'proposal_generation';
  }
  if (entry.tool === 'repair_approval') return 'approval';
  if (entry.tool === 'repair_rollback') return 'rollback';
  if (entry.tool === 'repair_apply') {
    return typeof details.error_stage === 'string' ? details.error_stage : 'apply';
  }
  return entry.tool;
}

function deriveAuditStatus(entry: RecentAuditEntry): string {
  const details = entry.details || {};
  if (typeof details.error_code === 'string') {
    return details.error_retryable === true ? 'degraded' : 'failed';
  }
  if (entry.tool === 'review_check' && isRecord(details.gate_decision) && typeof details.gate_decision.decision === 'string') {
    return details.gate_decision.decision;
  }
  if (entry.tool.startsWith('repair_') && typeof details.approval_state === 'string') {
    return details.approval_state;
  }
  if (entry.tool.startsWith('repair_') && typeof details.gate_decision === 'string') {
    return details.gate_decision;
  }
  if (entry.tool.startsWith('approval.')) {
    return entry.action === 'allowed' ? 'approved' : 'rejected';
  }
  return entry.action === 'allowed' ? 'allow' : 'block';
}

function readStateChange(details: Record<string, unknown>): AuditStateChange | undefined {
  if (!isRecord(details.state_change)) return undefined;
  return {
    from_state: typeof details.state_change.from_state === 'string' ? details.state_change.from_state : undefined,
    to_state: typeof details.state_change.to_state === 'string' ? details.state_change.to_state : undefined,
  };
}

function inferStateChange(entry: RecentAuditEntry): AuditStateChange | undefined {
  const details = entry.details || {};
  if (entry.tool === 'review_check') {
    return {
      from_state: 'running',
      to_state: deriveAuditStatus(entry),
    };
  }
  if (entry.tool === 'repair_plan') {
    return {
      from_state: 'draft',
      to_state: typeof details.approval_state === 'string' ? details.approval_state : 'waiting_approval',
    };
  }
  if (entry.tool === 'repair_approval') {
    return {
      from_state: 'waiting_approval',
      to_state: typeof details.approval_state === 'string' ? details.approval_state : undefined,
    };
  }
  if (entry.tool === 'repair_apply' && typeof details.approval_state === 'string') {
    return {
      from_state: 'approved',
      to_state: details.approval_state,
    };
  }
  if (entry.tool === 'repair_rollback' && typeof details.approval_state === 'string') {
    return {
      from_state: 'applied',
      to_state: details.approval_state,
    };
  }
  return undefined;
}

function asStringArray(value: unknown, field?: string): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  if (!field) {
    return value.filter((entry): entry is string => typeof entry === 'string');
  }
  return value
    .map((entry) => isRecord(entry) ? entry[field] : undefined)
    .filter((entry): entry is string => typeof entry === 'string');
}

function isRecord(value: unknown): value is Record<string, any> {
  return typeof value === 'object' && value !== null;
}
