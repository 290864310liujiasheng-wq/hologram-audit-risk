import type { ReviewFinding, Severity } from './review-core';

export interface RiskViolation {
  signal?: {
    description?: string;
    file_path?: string;
    line?: number;
    level?: number;
  };
  message?: string;
  level?: number;
}

export interface RiskCheckResult {
  passed: boolean;
  timestamp: string;
  changed_files: string[];
  total_changed_files: number;
  l5_violations: RiskViolation[];
  l4_violations: RiskViolation[];
  l3_violations: RiskViolation[];
  l2_violations: RiskViolation[];
}

export interface CheckRiskSummaryItem {
  finding_id: string;
  severity: Severity;
  category: string;
  plain_explanation: string;
  locationLabel: string;
}

export interface CheckRiskSummary {
  total: number;
  counts: Record<Severity, number>;
  topFindings: CheckRiskSummaryItem[];
}

const severityByBucket = {
  l5: 'critical',
  l4: 'high',
  l3: 'medium',
  l2: 'low',
} as const satisfies Record<string, Severity>;

export function adaptCheckResultToFindings(
  result: RiskCheckResult,
  input: { jobId: string; evidencePrefix: string },
): ReviewFinding[] {
  return [
    ...adaptBucket(result.l5_violations, 'l5', input, result.changed_files),
    ...adaptBucket(result.l4_violations, 'l4', input, result.changed_files),
    ...adaptBucket(result.l3_violations, 'l3', input, result.changed_files),
    ...adaptBucket(result.l2_violations, 'l2', input, result.changed_files),
  ];
}

export function summarizeSeverityCounts(findings: ReviewFinding[]): Record<Severity, number> {
  const counts: Record<Severity, number> = {
    critical: 0,
    high: 0,
    medium: 0,
    low: 0,
    info: 0,
  };

  for (const finding of findings) {
    counts[finding.severity] += 1;
  }

  return counts;
}

export function buildCheckRiskSummary(findings: ReviewFinding[]): CheckRiskSummary {
  const counts = summarizeSeverityCounts(findings);
  const topFindings = [...findings]
    .sort((a, b) => severityRank(b.severity) - severityRank(a.severity))
    .slice(0, 5)
    .map((finding) => ({
      finding_id: finding.finding_id,
      severity: finding.severity,
      category: finding.category,
      plain_explanation: finding.plain_explanation,
      locationLabel: formatLocation(finding),
    }));

  return {
    total: findings.length,
    counts,
    topFindings,
  };
}

function adaptBucket(
  violations: RiskViolation[],
  bucket: keyof typeof severityByBucket,
  input: { jobId: string; evidencePrefix: string },
  changedFiles: string[],
): ReviewFinding[] {
  return violations.map((violation, index) => {
    const filePath = violation.signal?.file_path || changedFiles[0] || 'unknown';
    const line = violation.signal?.line || 1;
    const description = violation.signal?.description || violation.message || `Detected ${bucket} violation.`;
    const message = violation.message || description;

    return {
      finding_id: `${input.jobId}:${bucket}:${index}`,
      job_id: input.jobId,
      rule_id: `check.${bucket}`,
      severity: severityByBucket[bucket],
      category: 'architecture',
      locations: [{
        file_path: filePath,
        start_line: line,
        end_line: line,
      }],
      plain_explanation: description,
      impact: message,
      recommendation: `Review ${bucket.toUpperCase()} violation before continuing.`,
      evidence_ids: [`${input.evidencePrefix}:${bucket}:${index}`],
      confidence: 0.8,
      status: 'open',
    };
  });
}

function severityRank(severity: Severity): number {
  switch (severity) {
    case 'critical': return 4;
    case 'high': return 3;
    case 'medium': return 2;
    case 'low': return 1;
    default: return 0;
  }
}

function formatLocation(finding: ReviewFinding): string {
  const location = finding.locations[0];
  if (!location) return 'unknown';
  const fileName = location.file_path.replace(/\\/g, '/').split('/').pop() || location.file_path;
  return `${fileName}:${location.start_line}`;
}
