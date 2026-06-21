import type {
  AgentRun,
  AggregationConflict,
  ReviewAggregation,
  ReviewFinding,
  ReviewStatus,
} from './review-core';

export type SpecialistAgentType =
  | 'static'
  | 'security'
  | 'test_regression'
  | 'dependency'
  | 'repair_planner';

export interface SpecialistAgentResult {
  run: AgentRun;
  findings: ReviewFinding[];
  suggested_decision: 'allow' | 'warn' | 'require_approval' | 'block';
}

export interface MultiAgentReview {
  job_id: string;
  requested_agents: SpecialistAgentType[];
  agent_results: SpecialistAgentResult[];
  merged_findings: ReviewFinding[];
  aggregation: ReviewAggregation;
  degraded_reasons: string[];
}

const DEFAULT_SPECIALISTS: SpecialistAgentType[] = [
  'static',
  'security',
  'test_regression',
  'dependency',
  'repair_planner',
];

const decisionPriority = {
  allow: 0,
  warn: 1,
  require_approval: 2,
  block: 3,
} as const;

export function buildSpecialistAgentRuns(input: {
  job_id: string;
  findings: ReviewFinding[];
  requested_agents?: SpecialistAgentType[];
  started_at: string;
  completed_at: string;
  failed_agents?: Partial<Record<SpecialistAgentType, string>>;
}): SpecialistAgentResult[] {
  const agents = input.requested_agents?.length ? input.requested_agents : DEFAULT_SPECIALISTS;

  return agents.map((agentType) => {
    const error = input.failed_agents?.[agentType];
    const scopedFindings = error ? [] : selectFindingsForAgent(agentType, input.findings);
    const status: ReviewStatus = error ? 'degraded' : 'completed';
    const run = {
      agent_run_id: `${input.job_id}:${agentType}`,
      job_id: input.job_id,
      agent_type: agentType,
      status,
      input_evidence_ids: Array.from(new Set(input.findings.flatMap((finding) => finding.evidence_ids))),
      finding_ids: scopedFindings.map((finding) => finding.finding_id),
      started_at: input.started_at,
      completed_at: input.completed_at,
      error,
    } satisfies AgentRun;

    return {
      run,
      findings: scopedFindings,
      suggested_decision: suggestDecision(scopedFindings),
    };
  });
}

export function aggregateAgentRuns(input: {
  job_id: string;
  agent_results: SpecialistAgentResult[];
}): {
  merged_findings: ReviewFinding[];
  aggregation: ReviewAggregation;
} {
  const groups = new Map<string, ReviewFinding[]>();

  for (const result of input.agent_results) {
    for (const finding of result.findings) {
      const key = findingMergeKey(finding);
      const list = groups.get(key);
      if (list) {
        list.push(finding);
      } else {
        groups.set(key, [finding]);
      }
    }
  }

  const merged_findings: ReviewFinding[] = [];
  const dropped_duplicates: string[] = [];
  const conflicts: AggregationConflict[] = [];

  for (const duplicates of groups.values()) {
    const ordered = [...duplicates].sort((left, right) => {
      const severityDelta = severityRank(right.severity) - severityRank(left.severity);
      if (severityDelta !== 0) return severityDelta;
      return right.confidence - left.confidence;
    });
    const primary = ordered[0];
    merged_findings.push(primary);

    for (const duplicate of ordered.slice(1)) {
      dropped_duplicates.push(duplicate.finding_id);
    }

    const distinctSeverities = Array.from(new Set(ordered.map((finding) => finding.severity)));
    if (distinctSeverities.length > 1) {
      conflicts.push({
        finding_ids: ordered.map((finding) => finding.finding_id),
        reason: 'Multiple specialist agents produced the same risk with different severities.',
        resolution: `Lead reviewer kept ${primary.severity} because it is the highest-risk interpretation with the strongest confidence.`,
      });
    }
  }

  merged_findings.sort((left, right) => {
    const severityDelta = severityRank(right.severity) - severityRank(left.severity);
    if (severityDelta !== 0) return severityDelta;
    return right.confidence - left.confidence;
  });

  return {
    merged_findings,
    aggregation: {
      job_id: input.job_id,
      lead_agent_run_id: `${input.job_id}:lead-reviewer`,
      merged_finding_ids: merged_findings.map((finding) => finding.finding_id),
      dropped_duplicates,
      conflicts,
    },
  };
}

export function finalizeMultiAgentReview(input: {
  job_id: string;
  findings: ReviewFinding[];
  requested_agents?: SpecialistAgentType[];
  started_at: string;
  completed_at: string;
  failed_agents?: Partial<Record<SpecialistAgentType, string>>;
}): MultiAgentReview {
  const agent_results = buildSpecialistAgentRuns(input);
  const { merged_findings, aggregation } = aggregateAgentRuns({
    job_id: input.job_id,
    agent_results,
  });

  return {
    job_id: input.job_id,
    requested_agents: input.requested_agents?.length ? input.requested_agents : DEFAULT_SPECIALISTS,
    agent_results,
    merged_findings,
    aggregation,
    degraded_reasons: agent_results
      .filter((result) => result.run.status === 'degraded' && result.run.error)
      .map((result) => `${result.run.agent_type}: ${result.run.error}`),
  };
}

function selectFindingsForAgent(
  agentType: SpecialistAgentType,
  findings: ReviewFinding[],
): ReviewFinding[] {
  switch (agentType) {
    case 'static':
      return findings.filter((finding) => {
        if (finding.category === 'test_regression' || finding.category === 'dependency') {
          return false;
        }
        return ['architecture', 'quality', 'data_integrity'].includes(finding.category)
          || finding.rule_id.startsWith('check.');
      });
    case 'security':
      return findings.filter((finding) =>
        severityRank(finding.severity) >= severityRank('high')
        || includesKeyword(finding, ['permission', '敏感', '危险', 'secret', 'token', 'write', '删除', 'exec']),
      );
    case 'test_regression':
      return findings.filter((finding) =>
        finding.category === 'test_regression'
        || includesKeyword({
          ...finding,
          recommendation: '',
        }, ['test', '测试', '回归', 'regression'])
      );
    case 'dependency':
      return findings.filter((finding) =>
        finding.locations.some((location) => /(package(-lock)?\.json|pnpm-lock\.yaml|yarn\.lock|cargo\.(toml|lock))$/i.test(location.file_path))
        || includesKeyword(finding, ['dependency', '供应链', '依赖']),
      );
    case 'repair_planner':
      return findings.filter((finding) => severityRank(finding.severity) >= severityRank('medium'));
  }
}

function includesKeyword(finding: ReviewFinding, keywords: string[]): boolean {
  const haystack = [
    finding.category,
    finding.plain_explanation,
    finding.impact,
    finding.recommendation,
    ...finding.locations.map((location) => location.file_path),
  ].join(' ').toLowerCase();

  return keywords.some((keyword) => haystack.includes(keyword.toLowerCase()));
}

function findingMergeKey(finding: ReviewFinding): string {
  const location = finding.locations[0];
  return [
    finding.rule_id,
    location?.file_path || '',
    location?.start_line || 0,
    normalizeText(finding.plain_explanation),
  ].join(':');
}

function normalizeText(text: string): string {
  return text.trim().replace(/\s+/g, ' ').toLowerCase();
}

function suggestDecision(findings: ReviewFinding[]): 'allow' | 'warn' | 'require_approval' | 'block' {
  let decision: 'allow' | 'warn' | 'require_approval' | 'block' = 'allow';

  for (const finding of findings) {
    const candidate = decisionForSeverity(finding.severity);
    if (decisionPriority[candidate] > decisionPriority[decision]) {
      decision = candidate;
    }
  }

  return decision;
}

function decisionForSeverity(severity: ReviewFinding['severity']): 'allow' | 'warn' | 'require_approval' | 'block' {
  switch (severity) {
    case 'critical':
      return 'block';
    case 'high':
      return 'require_approval';
    case 'medium':
      return 'warn';
    default:
      return 'allow';
  }
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
