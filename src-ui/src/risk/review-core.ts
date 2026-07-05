export type ReviewMode = 'live' | 'pre_commit' | 'manual' | 'ci' | 'repair_validation';
export type ReviewStatus = 'queued' | 'running' | 'degraded' | 'completed' | 'blocked' | 'cancelled' | 'failed';
export type Severity = 'info' | 'low' | 'medium' | 'high' | 'critical';
export type FindingStatus = 'open' | 'accepted' | 'dismissed' | 'fixed' | 'suppressed';
export type GateEffect = 'observe' | 'warn' | 'require_approval' | 'block';
export type GateSubjectType = 'tool_call' | 'file_write' | 'git_commit' | 'repair_apply' | 'release';
export type GateDecisionValue = 'allow' | 'warn' | 'require_approval' | 'block';
export type RulePlane = 'review' | 'repair';
export type RulePackageSource = 'system_default' | 'workspace_extension';

export interface ReviewJobRequest {
  workspace_id: string;
  change_id: string;
  mode: ReviewMode;
  requested_agents?: string[];
  policy_profile_id: string;
  provider_profile_id: string;
}

export interface SourceLocation {
  file_path: string;
  start_line: number;
  end_line: number;
  symbol?: string;
}

export interface ReviewFinding {
  finding_id: string;
  job_id: string;
  rule_id: string;
  severity: Severity;
  category: string;
  locations: SourceLocation[];
  plain_explanation: string;
  impact: string;
  recommendation: string;
  evidence_ids: string[];
  model_trace_id?: string;
  confidence: number;
  status: FindingStatus;
}

export interface RuleTrigger {
  kind: 'static_signal' | 'diff_pattern' | 'dependency_impact' | 'permission' | 'model_review';
  config: Record<string, unknown>;
}

export interface Rule {
  rule_id: string;
  package_id: string;
  name: string;
  category: string;
  severity: Severity;
  priority: number;
  scope: string[];
  trigger: RuleTrigger;
  gate_effect: GateEffect;
  explanation_template?: string;
  enabled: boolean;
}

export interface RulePackage {
  package_id: string;
  version: string;
  plane: RulePlane;
  source: RulePackageSource;
  enabled: boolean;
  description: string;
  rules: Rule[];
}

export interface GateDecision {
  decision_id: string;
  job_id: string;
  subject_type: GateSubjectType;
  subject_ref: string;
  decision: GateDecisionValue;
  reason: string;
  finding_ids: string[];
  policy_snapshot_id: string;
  decided_at: string;
}

export interface ReviewJobResult {
  job_id: string;
  status: ReviewStatus;
  findings: ReviewFinding[];
  gate_decision?: GateDecision;
  audit_event_ids: string[];
  degraded_reasons?: string[];
}

export interface AuditEvent {
  event_id: string;
  workspace_id: string;
  actor: string;
  event_type:
    | 'review_started'
    | 'finding_created'
    | 'gate_decided'
    | 'approval_requested'
    | 'approval_resolved'
    | 'repair_planned'
    | 'repair_applied'
    | 'repair_rolled_back';
  subject_ref: string;
  decision_id?: string;
  evidence_ids: string[];
  timestamp: string;
  integrity_hash?: string;
}

export interface AgentRun {
  agent_run_id: string;
  job_id: string;
  agent_type: string;
  status: ReviewStatus;
  input_evidence_ids: string[];
  finding_ids: string[];
  started_at: string;
  completed_at?: string;
  error?: string;
}

export interface AggregationConflict {
  finding_ids: string[];
  reason: string;
  resolution: string;
}

export interface ReviewAggregation {
  job_id: string;
  lead_agent_run_id: string;
  merged_finding_ids: string[];
  dropped_duplicates: string[];
  conflicts: AggregationConflict[];
}

export type RepairApprovalState =
  | 'draft'
  | 'waiting_approval'
  | 'approved'
  | 'rejected'
  | 'applied'
  | 'rolled_back';

export interface RepairPlan {
  repair_plan_id: string;
  job_id: string;
  finding_ids: string[];
  strategy: string;
  patch_proposal_ref: string;
  required_tests: string[];
  risk_note: string;
  approval_state: RepairApprovalState;
}

export interface PatchOperation {
  operation_id: string;
  file_path: string;
  new_content: string;
  summary: string;
}

export interface PatchProposal {
  patch_proposal_id: string;
  repair_plan_id: string;
  summary: string;
  rationale: string;
  operations: PatchOperation[];
  generated_at: string;
}

export interface RepairProposalValidationSummary {
  secondary_audit: {
    passed: boolean;
    summary: string;
  };
  syntax_check: {
    passed: boolean;
    summary: string;
  };
  logic_change: {
    summary: string;
  };
  blocked: boolean;
  blocked_reason?: string;
}

export interface RepairRollbackSnapshot {
  rollback_id: string;
  repair_plan_id: string;
  files: Array<{
    file_path: string;
    content: string;
  }>;
  created_at: string;
}

export interface ValidationCommandResult {
  command: string;
  passed: boolean;
  stdout: string;
  stderr: string;
}

export interface RepairPreflightReport {
  repair_plan_id: string;
  findings: ReviewFinding[];
  gate_decision: GateDecision;
  test_results: ValidationCommandResult[];
}

export type RepairExecutionStage =
  | 'proposal_generation'
  | 'preflight'
  | 'apply'
  | 'rollback';

export interface RepairIssue {
  issue_id: string;
  repair_plan_id: string;
  stage: RepairExecutionStage;
  summary: string;
  error: ContractError;
  created_at: string;
}

export interface ContractError {
  code:
    | 'invalid_request'
    | 'missing_evidence'
    | 'proposal_new_risk'
    | 'syntax_invalid'
    | 'provider_auth_invalid'
    | 'provider_upstream_failed'
    | 'provider_unavailable'
    | 'network_unreachable'
    | 'tls_handshake_failed'
    | 'tls_cert_revoked'
    | 'proxy_rejected'
    | 'connection_interrupted'
    | 'rate_limited'
    | 'policy_blocked'
    | 'approval_required'
    | 'audit_write_failed'
    | 'timeout'
    | 'internal_error';
  message: string;
  retryable: boolean;
  evidence_ids?: string[];
}

const decisionPriority: Record<GateDecisionValue, number> = {
  allow: 0,
  warn: 1,
  require_approval: 2,
  block: 3,
};

const effectToDecision: Record<GateEffect, GateDecisionValue> = {
  observe: 'allow',
  warn: 'warn',
  require_approval: 'require_approval',
  block: 'block',
};

export function validateReviewJobRequest(request: ReviewJobRequest): ContractError[] {
  const errors: ContractError[] = [];

  if (!request.workspace_id.trim()) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewJobRequest requires a workspace id.',
      retryable: false,
    });
  }

  if (!request.change_id.trim()) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewJobRequest requires a change id.',
      retryable: false,
    });
  }

  if (!request.policy_profile_id.trim()) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewJobRequest requires a policy profile id.',
      retryable: false,
    });
  }

  if (!request.provider_profile_id.trim()) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewJobRequest requires a provider profile id.',
      retryable: false,
    });
  }

  return errors;
}

export function validateRule(rule: Rule): ContractError[] {
  const errors: ContractError[] = [];

  if (!rule.package_id.trim()) {
    errors.push({
      code: 'invalid_request',
      message: 'Rule requires a package id.',
      retryable: false,
    });
  }

  if (!rule.scope.length) {
    errors.push({
      code: 'invalid_request',
      message: 'Rule requires at least one scope entry.',
      retryable: false,
    });
  }

  if (!rule.name.trim()) {
    errors.push({
      code: 'invalid_request',
      message: 'Rule requires a non-empty name.',
      retryable: false,
    });
  }

  if (!Number.isFinite(rule.priority)) {
    errors.push({
      code: 'invalid_request',
      message: 'Rule priority must be a finite number.',
      retryable: false,
    });
  }

  return errors;
}

export function validateReviewFinding(finding: ReviewFinding): ContractError[] {
  const errors: ContractError[] = [];

  if (finding.evidence_ids.length === 0) {
    errors.push({
      code: 'missing_evidence',
      message: 'ReviewFinding requires at least one evidence id.',
      retryable: false,
    });
  }

  if (finding.confidence < 0 || finding.confidence > 1) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewFinding confidence must be between 0 and 1.',
      retryable: false,
      evidence_ids: finding.evidence_ids,
    });
  }

  if (finding.locations.length === 0) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewFinding requires at least one source location.',
      retryable: false,
      evidence_ids: finding.evidence_ids,
    });
  }

  if (finding.locations.some((location) =>
    !location.file_path.trim()
    || location.start_line < 1
    || location.end_line < location.start_line,
  )) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewFinding source locations must use positive, ordered line ranges.',
      retryable: false,
      evidence_ids: finding.evidence_ids,
    });
  }

  const explanation = finding.plain_explanation.trim();
  if (
    explanation.length < 6
    || explanation === finding.rule_id
    || /^[A-Za-z0-9_.-]+$/.test(explanation)
  ) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewFinding plain explanation must be human-readable, not just an identifier.',
      retryable: false,
      evidence_ids: finding.evidence_ids,
    });
  }

  const impact = finding.impact.trim();
  if (
    impact.length < 8
    || /^[A-Za-z0-9_.-]+$/.test(impact)
  ) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewFinding impact must describe the user-visible or system impact in plain language.',
      retryable: false,
      evidence_ids: finding.evidence_ids,
    });
  }

  const recommendation = finding.recommendation.trim();
  if (
    recommendation.length < 8
    || /^[A-Za-z0-9_.-]+$/.test(recommendation)
  ) {
    errors.push({
      code: 'invalid_request',
      message: 'ReviewFinding recommendation must contain a concrete human-readable action.',
      retryable: false,
      evidence_ids: finding.evidence_ids,
    });
  }

  return errors;
}

export function deriveGateDecision(input: {
  job_id: string;
  subject_type: GateSubjectType;
  subject_ref: string;
  findings: ReviewFinding[];
  rules: Rule[];
  policy_snapshot_id: string;
  decided_at: string;
}): GateDecision {
  const ruleById = new Map(input.rules.filter((rule) => rule.enabled).map((rule) => [rule.rule_id, rule]));
  let decision: GateDecisionValue = 'allow';
  let winningRulePriority = Number.NEGATIVE_INFINITY;
  let reason = 'No enabled rule requires intervention.';
  const findingIds: string[] = [];

  for (const finding of input.findings) {
    const rule = ruleById.get(finding.rule_id);
    if (!rule) continue;
    findingIds.push(finding.finding_id);

    const candidateDecision = effectToDecision[rule.gate_effect];
    const outranksDecision = decisionPriority[candidateDecision] > decisionPriority[decision];
    const outranksPriority = decisionPriority[candidateDecision] === decisionPriority[decision]
      && rule.priority > winningRulePriority;
    if (outranksDecision || outranksPriority) {
      decision = candidateDecision;
      winningRulePriority = rule.priority;
      reason = rule.name;
    }
  }

  return {
    decision_id: `${input.job_id}:${input.subject_type}:${input.subject_ref}:${decision}`,
    job_id: input.job_id,
    subject_type: input.subject_type,
    subject_ref: input.subject_ref,
    decision,
    reason,
    finding_ids: findingIds,
    policy_snapshot_id: input.policy_snapshot_id,
    decided_at: input.decided_at,
  };
}

export function validateGateDecision(decision: GateDecision): ContractError[] {
  const errors: ContractError[] = [];

  if (!decision.subject_ref.trim()) {
    errors.push({
      code: 'invalid_request',
      message: 'GateDecision requires a subject reference.',
      retryable: false,
      evidence_ids: decision.finding_ids,
    });
  }

  if (!decision.policy_snapshot_id.trim()) {
    errors.push({
      code: 'invalid_request',
      message: 'GateDecision requires a policy snapshot id.',
      retryable: false,
      evidence_ids: decision.finding_ids,
    });
  }

  if (
    (decision.decision === 'block' || decision.decision === 'require_approval')
    && !decision.reason.trim()
  ) {
    errors.push({
      code: 'invalid_request',
      message: 'Blocking or approval decisions require a human-readable reason.',
      retryable: false,
      evidence_ids: decision.finding_ids,
    });
  }

  if (
    (decision.decision === 'block' || decision.decision === 'require_approval')
    && decision.finding_ids.length === 0
  ) {
    errors.push({
      code: 'invalid_request',
      message: 'Blocking or approval decisions require at least one finding id.',
      retryable: false,
      evidence_ids: decision.finding_ids,
    });
  }

  return errors;
}

export function finalizeReviewJobResult(input: {
  job_id: string;
  findings: ReviewFinding[];
  gate_decision?: GateDecision;
  audit_event_ids: string[];
  degraded_reasons?: string[];
}): ReviewJobResult {
  let status: ReviewStatus = 'completed';

  if (input.gate_decision?.decision === 'block') {
    status = 'blocked';
  } else if ((input.degraded_reasons?.length ?? 0) > 0) {
    status = 'degraded';
  }

  return {
    job_id: input.job_id,
    status,
    findings: input.findings,
    gate_decision: input.gate_decision,
    audit_event_ids: input.audit_event_ids,
    degraded_reasons: input.degraded_reasons,
  };
}

export function createAuditEvent(input: {
  workspace_id: string;
  actor: string;
  event_type: AuditEvent['event_type'];
  subject_ref: string;
  decision_id?: string;
  findings?: ReviewFinding[];
  timestamp: string;
  integrity_hash?: string;
}): AuditEvent {
  const evidenceIds = Array.from(new Set((input.findings ?? []).flatMap((finding) => finding.evidence_ids)));

  return {
    event_id: `${input.event_type}:${input.subject_ref}:${input.timestamp}`,
    workspace_id: input.workspace_id,
    actor: input.actor,
    event_type: input.event_type,
    subject_ref: input.subject_ref,
    decision_id: input.decision_id,
    evidence_ids: evidenceIds,
    timestamp: input.timestamp,
    integrity_hash: input.integrity_hash,
  };
}
