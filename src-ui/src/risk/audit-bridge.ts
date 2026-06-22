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

export interface AuditDisplayRow {
  timestamp: string;
  toolLabel: string;
  actionLabel: string;
  subject: string;
  reason: string;
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

export function summarizeRecentAuditEntries(entries: RecentAuditEntry[]): AuditDisplayRow[] {
  return entries
    .filter((entry) =>
      entry.tool === 'review_check'
      || entry.tool.startsWith('approval.')
      || entry.tool.startsWith('repair_'),
    )
    .sort((a, b) => b.ts.localeCompare(a.ts))
    .slice(0, 5)
    .map((entry) => ({
      timestamp: entry.ts,
      toolLabel: entry.tool === 'review_check'
        ? '审查'
        : entry.tool.startsWith('approval.')
          ? '审批'
          : '修复',
      actionLabel: (entry.tool === 'review_check' && typeof entry.details?.gate_decision === 'object')
        || (entry.tool.startsWith('repair_') && typeof entry.details?.gate_decision === 'string')
        ? gateDecisionActionLabel(
            entry.tool === 'review_check'
              ? String((entry.details?.gate_decision as Record<string, unknown>).decision || '')
              : String(entry.details?.gate_decision || ''),
          )
        : entry.action === 'allowed'
          ? '允许'
          : '拒绝',
      subject: String(entry.details?.subject || entry.path || ''),
      reason: entry.reason,
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
    default:
      return '允许';
  }
}
