import type { ReviewFinding, Severity } from './review-core';
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
): ReviewAuditPayload {
  const counts = summarizeSeverityCounts(findings);
  const evidenceIds = Array.from(new Set(findings.flatMap((finding) => finding.evidence_ids)));
  const findingIds = findings.map((finding) => finding.finding_id);
  const action = result.passed ? 'allowed' : 'denied';
  const reason = result.passed
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
      actionLabel: entry.action === 'allowed' ? '允许' : '拒绝',
      subject: String(entry.details?.subject || entry.path || ''),
      reason: entry.reason,
    }));
}
