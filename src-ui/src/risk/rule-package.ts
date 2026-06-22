import type { PatchProposal, ReviewFinding, Rule, Severity } from './review-core';

const reviewRuleMeta = {
  'check.l5': {
    name: 'L5 不可逆风险默认阻断',
    category: 'data_integrity',
    severity: 'critical' as Severity,
    gate_effect: 'block' as const,
    recommendation: '先阻断当前改动，并补齐审计与回滚证据后再继续。',
  },
  'check.l4': {
    name: 'L4 静默风险需要审批',
    category: 'security',
    severity: 'high' as Severity,
    gate_effect: 'require_approval' as const,
    recommendation: '先人工审批并确认不会引入静默安全或行为破坏。',
  },
  'check.l3': {
    name: 'L3 延迟风险需要告警',
    category: 'quality',
    severity: 'medium' as Severity,
    gate_effect: 'warn' as const,
    recommendation: '补充最小验证并确认回归风险已收口。',
  },
  'check.l2': {
    name: 'L2 波及风险需要告警',
    category: 'operability',
    severity: 'low' as Severity,
    gate_effect: 'warn' as const,
    recommendation: '收窄波及面并确认不会影响客户当前流程。',
  },
} satisfies Record<string, {
  name: string;
  category: string;
  severity: Severity;
  gate_effect: Rule['gate_effect'];
  recommendation: string;
}>;

export type ReviewBucket = 'l5' | 'l4' | 'l3' | 'l2';

const bucketToReviewRuleId = {
  l5: 'check.l5',
  l4: 'check.l4',
  l3: 'check.l3',
  l2: 'check.l2',
} as const satisfies Record<ReviewBucket, keyof typeof reviewRuleMeta>;

const repairRuleMeta = {
  'repair.scope.out_of_scope_write': {
    name: '修复 patch 不得写出命中的风险文件范围',
    category: 'repair_scope',
    severity: 'critical' as Severity,
    gate_effect: 'block' as const,
  },
  'repair.scope.absolute_path_write': {
    name: '修复 patch 不得直接写绝对路径',
    category: 'repair_scope',
    severity: 'critical' as Severity,
    gate_effect: 'block' as const,
  },
  'repair.scope.sensitive_path_write': {
    name: '修复 patch 不得直接改动敏感配置或锁文件',
    category: 'repair_scope',
    severity: 'high' as Severity,
    gate_effect: 'block' as const,
  },
  'repair.scope.duplicate_file_write': {
    name: '修复 patch 不应对同一文件生成重复写操作',
    category: 'repair_quality',
    severity: 'medium' as Severity,
    gate_effect: 'warn' as const,
  },
  'repair.scope.large_patch_blast_radius': {
    name: '修复 patch 的文件波及面过大',
    category: 'repair_quality',
    severity: 'medium' as Severity,
    gate_effect: 'warn' as const,
  },
  'repair.test.required_command_failed': {
    name: '修复前验证命令必须全部通过',
    category: 'repair_gate',
    severity: 'critical' as Severity,
    gate_effect: 'block' as const,
  },
} satisfies Record<string, {
  name: string;
  category: string;
  severity: Severity;
  gate_effect: Rule['gate_effect'];
}>;

export const DEFAULT_REVIEW_RULES: Rule[] = Object.entries(reviewRuleMeta).map(([rule_id, meta]) => ({
  rule_id,
  name: meta.name,
  category: meta.category,
  severity: meta.severity,
  scope: ['file_write'],
  trigger: {
    kind: 'static_signal',
    config: {},
  },
  gate_effect: meta.gate_effect,
  enabled: true,
}));

export function getReviewBucketDefinition(bucket: ReviewBucket) {
  return reviewRuleMeta[bucketToReviewRuleId[bucket]];
}

export const DEFAULT_REPAIR_RULES: Rule[] = Object.entries(repairRuleMeta).map(([rule_id, meta]) => ({
  rule_id,
  name: meta.name,
  category: meta.category,
  severity: meta.severity,
  scope: ['repair_apply'],
  trigger: {
    kind: rule_id === 'repair.test.required_command_failed' ? 'static_signal' : 'diff_pattern',
    config: {},
  },
  gate_effect: meta.gate_effect,
  enabled: true,
}));

export function evaluateRepairProposal(input: {
  plan_id: string;
  proposal: PatchProposal;
  findings: ReviewFinding[];
}): ReviewFinding[] {
  const matches: ReviewFinding[] = [];
  const allowedPaths = new Set(
    input.findings
      .flatMap((finding) => finding.locations.map((location) => normalizePath(location.file_path))),
  );
  const seenPaths = new Set<string>();
  const uniqueOperationPaths = new Set(input.proposal.operations.map((operation) => normalizePath(operation.file_path)));

  input.proposal.operations.forEach((operation, index) => {
    const filePath = normalizePath(operation.file_path);
    const evidenceId = `repair-proposal:${input.plan_id}:op:${index}`;

    if (isAbsolutePath(operation.file_path)) {
      matches.push(createRepairRuleFinding({
        plan_id: input.plan_id,
        rule_id: 'repair.scope.absolute_path_write',
        file_path: operation.file_path,
        evidence_id: evidenceId,
        explanation: '补丁提案试图直接写绝对路径，这会绕开当前 workspace 的受控范围。',
        impact: '一旦路径解析偏离当前项目，修复可能覆盖客户不打算修改的文件。',
        recommendation: '把 file_path 改成当前 workspace 内、且已命中风险的相对路径。',
      }));
    }

    if (!allowedPaths.has(filePath)) {
      matches.push(createRepairRuleFinding({
        plan_id: input.plan_id,
        rule_id: 'repair.scope.out_of_scope_write',
        file_path: operation.file_path,
        evidence_id: evidenceId,
        explanation: '补丁提案修改了未被当前 finding 命中的文件。',
        impact: '修复 blast radius 超出当前审查证据，apply 前无法证明这次改动是必要且可控的。',
        recommendation: '把 patch 限制在当前 findings 涉及的文件内，或先重新生成更完整的 repair plan。',
      }));
    }

    if (isSensitivePath(filePath)) {
      matches.push(createRepairRuleFinding({
        plan_id: input.plan_id,
        rule_id: 'repair.scope.sensitive_path_write',
        file_path: operation.file_path,
        evidence_id: evidenceId,
        explanation: '补丁提案直接改动了锁文件、环境文件或密钥相关路径。',
        impact: '这类文件会扩大运行态和供应链风险，不能在当前 repair apply 中静默落盘。',
        recommendation: '把敏感文件修改拆出当前 repair 流程，单独审查并显式批准。',
      }));
    }

    if (seenPaths.has(filePath)) {
      matches.push(createRepairRuleFinding({
        plan_id: input.plan_id,
        rule_id: 'repair.scope.duplicate_file_write',
        file_path: operation.file_path,
        evidence_id: evidenceId,
        explanation: '补丁提案对同一文件生成了重复写操作。',
        impact: '重复写会让最终内容依赖操作顺序，降低 patch proposal 的可审查性。',
        recommendation: '合并同一文件的写操作，确保每个文件只有一份最终内容。',
      }));
    }

    seenPaths.add(filePath);
  });

  const maxOperationFiles = Math.max(3, allowedPaths.size || 1);
  if (uniqueOperationPaths.size > maxOperationFiles) {
    matches.push(createRepairRuleFinding({
      plan_id: input.plan_id,
      rule_id: 'repair.scope.large_patch_blast_radius',
      file_path: input.proposal.operations[0]?.file_path || 'patch proposal',
      evidence_id: `repair-proposal:${input.plan_id}:summary`,
      explanation: `补丁提案一次修改了 ${uniqueOperationPaths.size} 个文件，超过当前 repair plan 的保守边界。`,
      impact: '文件波及面越大，误修、漏测和回滚失败的概率越高。',
      recommendation: '把修复拆成更小的提案，先处理最高风险 finding。',
    }));
  }

  return dedupeFindings(matches);
}

export function createRepairGateFailureFinding(input: {
  plan_id: string;
  command: string;
  stdout: string;
  stderr: string;
}): ReviewFinding {
  return createRepairRuleFinding({
    plan_id: input.plan_id,
    rule_id: 'repair.test.required_command_failed',
    file_path: input.command,
    evidence_id: `repair-test:${input.plan_id}:${input.command}`,
    explanation: `必跑验证命令失败：${input.command}`,
    impact: '当前 patch proposal 无法证明它在 apply 前仍满足最小安全门。',
    recommendation: `先修复命令失败原因，再重新执行 ${input.command}。`,
    detail: [input.stderr.trim(), input.stdout.trim()].filter(Boolean).join('\n').slice(0, 400),
  });
}

function createRepairRuleFinding(input: {
  plan_id: string;
  rule_id: keyof typeof repairRuleMeta;
  file_path: string;
  evidence_id: string;
  explanation: string;
  impact: string;
  recommendation: string;
  detail?: string;
}): ReviewFinding {
  const meta = repairRuleMeta[input.rule_id];

  return {
    finding_id: `${input.plan_id}:${input.rule_id}:${normalizePath(input.file_path)}`,
    job_id: input.plan_id.replace(/:repair$/, ''),
    rule_id: input.rule_id,
    severity: meta.severity,
    category: meta.category,
    locations: [{
      file_path: input.file_path,
      start_line: 1,
      end_line: 1,
      symbol: 'repair_preflight',
    }],
    plain_explanation: input.detail
      ? `${input.explanation}\n${input.detail}`
      : input.explanation,
    impact: input.impact,
    recommendation: input.recommendation,
    evidence_ids: [input.evidence_id],
    confidence: 0.99,
    status: 'open',
  };
}

function dedupeFindings(findings: ReviewFinding[]): ReviewFinding[] {
  const seen = new Set<string>();
  return findings.filter((finding) => {
    if (seen.has(finding.finding_id)) {
      return false;
    }
    seen.add(finding.finding_id);
    return true;
  });
}

function isSensitivePath(filePath: string): boolean {
  return /(^|\/)\.env(\.|$)|(^|\/)(package-lock\.json|pnpm-lock\.yaml|yarn\.lock|Cargo\.lock)$|(^|\/).+\.(pem|key|p12)$/i.test(filePath);
}

function isAbsolutePath(filePath: string): boolean {
  return /^(?:[A-Za-z]:[\\/]|\/)/.test(filePath);
}

function normalizePath(filePath: string): string {
  return filePath.replace(/\\/g, '/').replace(/^\.\//, '');
}
