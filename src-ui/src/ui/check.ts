// Check Panel — 简报面板
// 消费 hologram check --json 的输出，渲染变更摘要面板
// 底部抽屉，保存时自动刷新

import { bus } from './events';
import { iconHtml } from './icons';
import { askAgent } from './agent-visualizer';
import { adaptCheckResultToFindings, buildCheckRiskSummary } from '../risk/check-adapter';
import { buildRepairHistory, parseAuditQueryResult, summarizeRecentAuditEntries, type AuditRecord } from '../risk/audit-bridge';
import { buildRepairWorkbenchSnapshot, buildWorkbenchQueue, type CurrentReviewState } from '../risk/current-review';

export interface Violation {
  signal?: {
    description?: string;
    file_path?: string;
    line?: number;
    level?: number;
    affected_nodes?: string[];
    graph_node_ids?: string[];
    old_value?: string;
    new_value?: string;
  };
  message?: string;
  level?: number;
}

export interface CheckResult {
  passed: boolean;
  timestamp: string;
  commit_hash?: string;
  changed_files: string[];
  total_changed_files: number;
  l5_violations: Violation[];
  l4_violations: Violation[];
  l3_violations: Violation[];
  l2_violations: Violation[];
  passed_checks: string[];
  blast_radius: number;
  cross_community_edges: number;
  new_cycles: number;
  new_thread_conflicts: number;
  api_signature_changes: number;
}

const PANEL_ID = 'check-panel';

export class CheckPanel {
  private panel!: HTMLElement;
  private content!: HTMLElement;
  private headerStatus!: HTMLElement;
  private tabLabelEl!: HTMLElement;
  private openState = false;
  private lastResult: CheckResult | null = null;
  private viewingHistory = false;
  private historyTimestamp = '';
  private lastAuditRows: ReturnType<typeof summarizeRecentAuditEntries> = [];
  private lastAuditRecords: AuditRecord[] = [];
  private currentReviewState: CurrentReviewState | null = null;

  constructor(container: HTMLElement) {
    this.buildDOM(container);
  }

  // ── Public API ──

  update(result: CheckResult, currentReviewState?: CurrentReviewState): void {
    this.lastResult = result;
    this.currentReviewState = currentReviewState || null;
    this.viewingHistory = false;
    this.historyTimestamp = '';
    this.renderResult(result);
    this.loadRecentAudit().catch(() => {});
    this.refreshTabLabel();

    // Auto-open on failure
    if (!result.passed && !this.openState) {
      this.open();
    }
  }

  showHistory(data: CheckResult, timestamp: string): void {
    this.currentReviewState = null;
    this.viewingHistory = true;
    this.historyTimestamp = timestamp;
    this.renderResult(data, true);
    if (!this.openState) this.open();
  }

  showCurrent(): void {
    this.viewingHistory = false;
    this.historyTimestamp = '';
    if (this.lastResult) {
      this.renderResult(this.lastResult);
    }
    this.refreshTabLabel();
  }

  getLastResult(): CheckResult | null {
    return this.lastResult;
  }

  toggle(): void {
    this.openState ? this.close() : this.open();
  }

  open(): void {
    this.openState = true;
    this.panel.classList.add('check-open');
    bus.emit('panel:toggle');
  }

  close(): void {
    this.openState = false;
    this.panel.classList.remove('check-open');
    bus.emit('panel:toggle');
  }

  isOpen(): boolean {
    return this.openState;
  }

  // ── Build DOM ──

  private buildDOM(container: HTMLElement): void {
    this.panel = document.createElement('div');
    this.panel.id = PANEL_ID;

    // Corner brackets
    const brackets = document.createElement('div');
    brackets.className = 'corner-brackets';
    brackets.innerHTML = '<span class="cb-bottom left"></span><span class="cb-bottom right"></span>';
    this.panel.appendChild(brackets);

    // Tab handle (always visible when results exist)
    const tab = document.createElement('div');
    tab.className = 'check-tab';
    tab.addEventListener('click', () => this.toggle());

    this.headerStatus = document.createElement('span');
    this.headerStatus.className = 'check-tab-status';
    this.headerStatus.className = 'check-tab-status check-loading';
    tab.appendChild(this.headerStatus);

    this.tabLabelEl = document.createElement('span');
    this.tabLabelEl.className = 'check-tab-label';
    this.tabLabelEl.textContent = '简报';
    tab.appendChild(this.tabLabelEl);

    const tabArrow = document.createElement('span');
    tabArrow.className = 'check-tab-arrow';
    tabArrow.innerHTML = iconHtml('chevron-up', 10);
    tab.appendChild(tabArrow);

    this.panel.appendChild(tab);

    // Content area
    this.content = document.createElement('div');
    this.content.className = 'check-content';
    this.panel.appendChild(this.content);

    container.appendChild(this.panel);
  }

  // ── Render ──

  private renderResult(r: CheckResult, isHistory = false): void {
    // Update tab status indicator
    this.headerStatus.className = r.passed ? 'check-tab-status check-pass' : 'check-tab-status check-fail';

    const totalV = (r.l5_violations?.length || 0) + (r.l4_violations?.length || 0) +
                   (r.l3_violations?.length || 0) + (r.l2_violations?.length || 0);

    this.content.innerHTML = '';

    // ── History banner ──
    if (isHistory) {
      const banner = ce('div', 'check-history-banner');
      const label = ce('span', 'check-history-label');
      label.textContent = `历史简报 — ${fmtTime(this.historyTimestamp)}`;
      banner.appendChild(label);
      const backBtn = ce('button', 'check-history-back');
      backBtn.textContent = '返回当前';
      backBtn.addEventListener('click', () => this.showCurrent());
      banner.appendChild(backBtn);
      this.content.appendChild(banner);
    }

    // ── Header ──
    const header = ce('div', 'check-header');
    const statusBadge = ce('span', r.passed ? 'check-badge-pass' : 'check-badge-fail');
    statusBadge.innerHTML = r.passed ? `${iconHtml('check-circle', 12)} 通过` : `${iconHtml('alert', 12)} 未通过`;
    header.appendChild(statusBadge);

    const ts = ce('span', 'check-ts');
    ts.textContent = fmtTime(r.timestamp);
    header.appendChild(ts);
    this.content.appendChild(header);

    if (this.lastAuditRows.length > 0) {
      this.content.appendChild(this.renderRecentAudit());
    }

    if (!isHistory && this.currentReviewState) {
      this.content.appendChild(this.renderWorkbenchQueue(this.currentReviewState));
      this.content.appendChild(this.renderCurrentGateDecision(this.currentReviewState));
      this.content.appendChild(this.renderMultiAgentReview(this.currentReviewState));
      this.content.appendChild(this.renderRepairPlan(this.currentReviewState));
    }

    // ── Files ──
    const filesSec = ce('div', 'check-section');
    const filesTitle = ce('div', 'check-section-title');
    filesTitle.innerHTML = `${iconHtml('file', 10)} 变更文件 (${r.total_changed_files})`;
    filesSec.appendChild(filesTitle);
    const filesList = ce('div', 'check-file-list');
    for (const f of r.changed_files.slice(0, 10)) {
      const item = ce('div', 'check-file-item');
      item.textContent = basename(f);
      item.title = f;
      item.style.cursor = 'pointer';
      item.addEventListener('click', () => {
        bus.emit('navigate:file', f);
      });
      filesList.appendChild(item);
    }
    if (r.changed_files.length > 10) {
      const more = ce('div', 'check-file-item check-file-more');
      more.textContent = `… 还有 ${r.changed_files.length - 10} 个文件`;
      filesList.appendChild(more);
    }
    filesSec.appendChild(filesList);
    this.content.appendChild(filesSec);

    // ── Violations ──
    if (!r.passed && totalV > 0) {
      this.content.appendChild(this.renderRiskSummary(r));

      const vSec = ce('div', 'check-section');
      const vTitle = ce('div', 'check-section-title');
      vTitle.innerHTML = `${iconHtml('alert', 11)} 违规 (${totalV})`;
      vSec.appendChild(vTitle);

      // L5 - Irreversible
      if ((r.l5_violations?.length || 0) > 0) {
        vSec.appendChild(this.renderViolationGroup('L5 不可逆', 'l5', r.l5_violations || []));
      }
      // L4 - Silent
      if ((r.l4_violations?.length || 0) > 0) {
        vSec.appendChild(this.renderViolationGroup('L4 静默', 'l4', r.l4_violations || []));
      }
      // L3 - Delayed
      if ((r.l3_violations?.length || 0) > 0) {
        vSec.appendChild(this.renderViolationGroup('L3 延迟', 'l3', r.l3_violations || []));
      }
      // L2 - Blast
      if ((r.l2_violations?.length || 0) > 0) {
        vSec.appendChild(this.renderViolationGroup('L2 波及', 'l2', r.l2_violations || []));
      }

      this.content.appendChild(vSec);
    }

    // ── Stats ──
    const statsSec = ce('div', 'check-section');
    const statsTitle = ce('div', 'check-section-title');
    statsTitle.innerHTML = `${iconHtml('chart', 11)} 统计`;
    statsSec.appendChild(statsTitle);

    const statsGrid = ce('div', 'check-stats-grid');
    statsGrid.appendChild(this.statItem('波及半径', `${r.blast_radius} nodes`));
    statsGrid.appendChild(this.statItem('跨社区边', `${r.cross_community_edges}`));
    statsGrid.appendChild(this.statItem('新环', `${r.new_cycles}`));
    statsGrid.appendChild(this.statItem('线程冲突', `${r.new_thread_conflicts}`));
    statsGrid.appendChild(this.statItem('API 签名变更', `${r.api_signature_changes}`));
    statsSec.appendChild(statsGrid);
    this.content.appendChild(statsSec);

    // ── Auto-passed ──
    if (r.passed_checks.length > 0) {
      const apSec = ce('div', 'check-section');
      const apTitle = ce('div', 'check-section-title');
      apTitle.innerHTML = `${iconHtml('check-circle', 11)} 自动放行 (${r.passed_checks.length})`;
      apSec.appendChild(apTitle);
      for (const c of r.passed_checks.slice(0, 8)) {
        const item = ce('div', 'check-passed-item');
        item.textContent = c;
        apSec.appendChild(item);
      }
      if (r.passed_checks.length > 8) {
        const more = ce('div', 'check-passed-item');
        more.textContent = `… 还有 ${r.passed_checks.length - 8} 项`;
        apSec.appendChild(more);
      }
      this.content.appendChild(apSec);
    }
  }

  private async loadRecentAudit(): Promise<void> {
    const { invoke } = await import('../bridge');
    const json = await invoke<string>('audit_recent_reviews', { limit: 6 });
    const data = parseAuditQueryResult(JSON.parse(json) as { entries?: any[]; records?: any[] });
    this.lastAuditRecords = data.records;
    this.lastAuditRows = summarizeRecentAuditEntries(data.records);
    this.refreshTabLabel();
    if (this.lastResult && !this.viewingHistory) {
      this.renderResult(this.lastResult);
    }
  }

  private renderRecentAudit(): HTMLElement {
    const sec = ce('div', 'check-section check-risk-summary');
    const title = ce('div', 'check-section-title');
    title.innerHTML = `${iconHtml('clock', 11)} 最近审计`;
    sec.appendChild(title);

    for (const row of this.lastAuditRows) {
      const item = ce('div', `check-gate-item ${row.actionLabel === '拒绝' ? 'check-gate-high' : 'check-gate-low'}`);
      const head = ce('div', 'check-gate-item-head');
      const tag = ce('span', `check-gate-risk ${row.actionLabel === '拒绝' ? 'check-gate-risk-high' : 'check-gate-risk-low'}`);
      tag.textContent = `${row.toolLabel}${row.actionLabel}`;
      head.appendChild(tag);

      const name = ce('span', 'check-gate-name');
      name.textContent = row.subject || 'workspace';
      head.appendChild(name);

      const ts = ce('span', 'check-gate-stats');
      ts.textContent = fmtTime(row.timestamp);
      head.appendChild(ts);
      item.appendChild(head);

      const reason = ce('div', 'check-gate-rec');
      reason.textContent = row.reason;
      item.appendChild(reason);
      sec.appendChild(item);
    }

    return sec;
  }

  private renderWorkbenchQueue(state: CurrentReviewState): HTMLElement {
    const sec = ce('div', 'check-section check-risk-summary');
    const title = ce('div', 'check-section-title');
    title.innerHTML = `${iconHtml('list', 11)} Review Queue`;
    sec.appendChild(title);

    const items = buildWorkbenchQueue(state, this.lastAuditRecords);
    for (const item of items) {
      const row = ce('div', `check-gate-item ${queueToneClass(item.state)}`);
      row.style.cursor = 'pointer';
      row.addEventListener('click', () => this.scrollToSection(item.section_id));
      const head = ce('div', 'check-gate-item-head');
      const tag = ce('span', `check-gate-risk ${queueRiskClass(item.state)}`);
      tag.textContent = `${item.title} · ${queueStateLabel(item.state)}`;
      head.appendChild(tag);

      const name = ce('span', 'check-gate-name');
      name.textContent = item.summary;
      head.appendChild(name);
      row.appendChild(head);

      if (item.detail) {
        const detail = ce('div', 'check-gate-rec');
        detail.textContent = item.detail;
        row.appendChild(detail);
      }

      sec.appendChild(row);
    }

    return sec;
  }

  private renderViolationGroup(
    label: string,
    level: string,
    violations: Violation[],
  ): HTMLElement {
    const group = ce('div', 'check-vgroup');
    group.dataset['checkSection'] = 'risk-summary';
    const head = ce('div', `check-vhead check-vhead-${level}`);
    head.textContent = `${label} (${violations.length})`;
    group.appendChild(head);

    for (const v of violations.slice(0, 5)) {
      const sig = v.signal || {};
      const desc = sig.description || v.message || '?';
      const fp = sig.file_path || '';
      const line = sig.line || 0;
      const loc = fp ? `${basename(fp)}${line ? ':' + line : ''}` : '';

      const item = ce('div', 'check-vitem');
      const locEl = ce('span', 'check-vloc');
      locEl.textContent = loc;
      item.appendChild(locEl);
      const descEl = ce('span', 'check-vdesc');
      descEl.textContent = desc.length > 80 ? desc.slice(0, 80) + '…' : desc;
      descEl.title = desc;
      item.appendChild(descEl);

      if (sig.affected_nodes && sig.affected_nodes.length > 0) {
        const aff = ce('div', 'check-vaffect');
        const affLabel = document.createElement('span');
        affLabel.textContent = '影响: ';
        aff.appendChild(affLabel);

        const nodeIds = sig.graph_node_ids || [];
        const displayNodes = sig.affected_nodes.slice(0, 5);
        displayNodes.forEach((name, i) => {
          const nodeLink = ce('span', 'check-node-link');
          nodeLink.textContent = name;
          const gid = nodeIds[i] || '';
          nodeLink.title = gid ? `节点ID: ${gid}\n点击跳转到星图` : '点击跳转到星图';
          nodeLink.addEventListener('click', (e) => {
            e.stopPropagation();
            bus.emit('navigate:node', name);
            this.close();
          });
          aff.appendChild(nodeLink);
          if (i < displayNodes.length - 1) {
            aff.appendChild(document.createTextNode(', '));
          }
        });

        if (sig.affected_nodes.length > 5) {
          const more = document.createElement('span');
          more.className = 'check-vmore-inline';
          more.textContent = ` … +${sig.affected_nodes.length - 5}`;
          aff.appendChild(more);
        }

        item.appendChild(aff);
      }
      if (sig.old_value && sig.new_value) {
        const chg = ce('div', 'check-vchange');
        chg.textContent = `${sig.old_value} → ${sig.new_value}`;
        item.appendChild(chg);
      }

      // "Ask Agent" button for each violation
      const askBtn = document.createElement('button');
      askBtn.className = 'check-ask-btn';
      askBtn.innerHTML = iconHtml('agent', 11);
      askBtn.title = '问 Agent 关于这条违规';
      const nodeList = (sig.affected_nodes || []).slice(0, 3).join(', ');
      askBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        const context = [
          `[${label}] ${desc}`,
          fp ? `文件: ${fp}${line ? ':' + line : ''}` : '',
          nodeList ? `影响节点: ${nodeList}` : '',
          sig.old_value ? `变更: ${sig.old_value} → ${sig.new_value}` : '',
        ].filter(Boolean).join(' | ');
        askAgent(`分析这条违规: ${context}`);
      });
      item.appendChild(askBtn);

      group.appendChild(item);
    }

    if (violations.length > 5) {
      const more = ce('div', 'check-vmore');
      more.textContent = `… 还有 ${violations.length - 5} 条`;
      group.appendChild(more);
    }

    return group;
  }

  private renderRiskSummary(result: CheckResult): HTMLElement {
    const findings = adaptCheckResultToFindings(result, {
      jobId: `check:${result.timestamp || 'current'}`,
      evidencePrefix: 'check',
    });
    const summary = buildCheckRiskSummary(findings);

    const sec = ce('div', 'check-section check-risk-summary');
    sec.dataset['checkSection'] = 'risk-summary';
    const title = ce('div', 'check-section-title');
    title.innerHTML = `${iconHtml('alert', 11)} 风控摘要 (${summary.total})`;
    sec.appendChild(title);

    const badges = ce('div', 'check-gate-summary');
    if (summary.counts.critical > 0) {
      const critical = ce('span', 'check-gate-badge check-gate-high');
      critical.textContent = `✗ ${summary.counts.critical} Critical`;
      badges.appendChild(critical);
    }
    if (summary.counts.high > 0) {
      const high = ce('span', 'check-gate-badge check-gate-high');
      high.textContent = `⚠ ${summary.counts.high} High`;
      badges.appendChild(high);
    }
    if (summary.counts.medium > 0) {
      const medium = ce('span', 'check-gate-badge check-gate-mid');
      medium.textContent = `⚡ ${summary.counts.medium} Medium`;
      badges.appendChild(medium);
    }
    if (summary.counts.low > 0) {
      const low = ce('span', 'check-gate-badge check-gate-low');
      low.textContent = `✓ ${summary.counts.low} Low`;
      badges.appendChild(low);
    }
    sec.appendChild(badges);

    for (const finding of summary.topFindings) {
      const item = ce('div', `check-gate-item ${riskClassForSeverity(finding.severity)}`);
      const head = ce('div', 'check-gate-item-head');
      const risk = ce('span', `check-gate-risk ${riskLabelClassForSeverity(finding.severity)}`);
      risk.textContent = finding.severity === 'critical' ? '阻断' : severityBadge(finding.severity);
      head.appendChild(risk);

      const name = ce('span', 'check-gate-name');
      name.textContent = finding.locationLabel;
      head.appendChild(name);

      const category = ce('span', 'check-gate-stats');
      category.textContent = finding.category;
      head.appendChild(category);
      item.appendChild(head);

      const desc = ce('div', 'check-gate-rec');
      desc.textContent = finding.plain_explanation;
      item.appendChild(desc);
      sec.appendChild(item);
    }

    return sec;
  }

  private renderMultiAgentReview(state: CurrentReviewState): HTMLElement {
    const sec = ce('div', 'check-section check-risk-summary');
    const title = ce('div', 'check-section-title');
    title.innerHTML = `${iconHtml('agent', 11)} 多代理审计`;
    sec.appendChild(title);

    const summary = ce('div', 'check-gate-summary');
    const completed = state.multi_agent_review.agent_results.filter((result) => result.run.status === 'completed').length;
    const degraded = state.multi_agent_review.agent_results.filter((result) => result.run.status === 'degraded').length;

    const completedBadge = ce('span', 'check-gate-badge check-gate-low');
    completedBadge.textContent = `✓ ${completed} 已完成`;
    summary.appendChild(completedBadge);

    if (degraded > 0) {
      const degradedBadge = ce('span', 'check-gate-badge check-gate-mid');
      degradedBadge.textContent = `⚠ ${degraded} 降级`;
      summary.appendChild(degradedBadge);
    }

    if (state.multi_agent_review.aggregation.conflicts.length > 0) {
      const conflictBadge = ce('span', 'check-gate-badge check-gate-high');
      conflictBadge.textContent = `✗ ${state.multi_agent_review.aggregation.conflicts.length} 冲突`;
      summary.appendChild(conflictBadge);
    }
    sec.appendChild(summary);

    for (const result of state.multi_agent_review.agent_results) {
      const item = ce('div', `check-gate-item ${result.run.status === 'degraded' ? 'check-gate-mid' : 'check-gate-low'}`);
      const head = ce('div', 'check-gate-item-head');
      const tag = ce('span', `check-gate-risk ${result.run.status === 'degraded' ? 'check-gate-risk-mid' : 'check-gate-risk-low'}`);
      tag.textContent = result.run.status === 'degraded' ? '降级' : '完成';
      head.appendChild(tag);

      const name = ce('span', 'check-gate-name');
      name.textContent = result.run.agent_type;
      head.appendChild(name);

      const stats = ce('span', 'check-gate-stats');
      stats.textContent = `${result.findings.length} finding(s)`;
      head.appendChild(stats);
      item.appendChild(head);

      const desc = ce('div', 'check-gate-rec');
      desc.textContent = result.run.error || `建议: ${result.suggested_decision}`;
      item.appendChild(desc);
      sec.appendChild(item);
    }

    if (state.multi_agent_review.degraded_reasons.length > 0) {
      const note = ce('div', 'check-vmore');
      note.textContent = `降级原因: ${state.multi_agent_review.degraded_reasons.join('；')}`;
      sec.appendChild(note);
    }

    return sec;
  }

  private renderCurrentGateDecision(state: CurrentReviewState): HTMLElement {
    const sec = ce('div', 'check-section check-risk-summary');
    sec.dataset['checkSection'] = 'gate-decision';
    const title = ce('div', 'check-section-title');
    title.innerHTML = `${iconHtml('block', 11)} 门禁决策`;
    sec.appendChild(title);

    const summary = ce('div', 'check-gate-summary');
    const decisionBadge = ce('span', `check-gate-badge ${gateDecisionBadgeClass(state.gate_decision.decision)}`);
    decisionBadge.textContent = gateDecisionLabel(state.gate_decision.decision);
    summary.appendChild(decisionBadge);
    sec.appendChild(summary);

    const reason = ce('div', 'check-gate-rec');
    reason.textContent = state.gate_decision.reason;
    sec.appendChild(reason);

    const policy = ce('div', 'check-vmore');
    policy.textContent = `策略快照: ${state.gate_decision.policy_snapshot_id} · finding ${state.gate_decision.finding_ids.length} 条`;
    sec.appendChild(policy);

    return sec;
  }

  private renderRepairPlan(state: CurrentReviewState): HTMLElement {
    const sec = ce('div', 'check-section check-risk-summary');
    sec.dataset['checkSection'] = 'repair-workbench';
    const title = ce('div', 'check-section-title');
    title.innerHTML = `${iconHtml('tool', 11)} 自修复闭环`;
    sec.appendChild(title);

    const plan = state.repair_plan;
    const snapshot = buildRepairWorkbenchSnapshot(state, this.lastAuditRecords);
    const noRepairCandidate = snapshot.status_state === 'not_required';
    const statusRow = ce('div', 'check-gate-summary');
    const status = ce('span', `check-gate-badge ${repairBadgeClass(plan.approval_state)}`);
    status.textContent = snapshot.status_label;
    statusRow.appendChild(status);
    if (!noRepairCandidate) {
      const tests = ce('span', 'check-gate-badge check-gate-low');
      tests.textContent = `${snapshot.test_count} 个验证命令`;
      statusRow.appendChild(tests);
    }
    sec.appendChild(statusRow);

    const strategy = ce('div', 'check-gate-rec');
    strategy.textContent = snapshot.strategy;
    sec.appendChild(strategy);

    if (!noRepairCandidate && snapshot.risk_note) {
      const risk = ce('div', 'check-vchange');
      risk.textContent = snapshot.risk_note;
      sec.appendChild(risk);
    }

    if (snapshot.required_tests.length > 0) {
      const commandList = ce('div', 'check-vaffect');
      commandList.textContent = `验证: ${snapshot.required_tests.join(' · ')}`;
      sec.appendChild(commandList);
    }

    if (snapshot.generation_input) {
      const generationReady = ce('div', 'check-vmore');
      generationReady.textContent = `Repair input: ${snapshot.generation_input.finding_count} findings · ${snapshot.generation_input.file_count} files · ${snapshot.generation_input.eligible ? 'eligible' : 'blocked'}`;
      sec.appendChild(generationReady);

      if (snapshot.generation_input.reason) {
        const generationReason = ce('div', 'check-vmore');
        generationReason.textContent = snapshot.generation_input.reason;
        sec.appendChild(generationReason);
      }
    }

    if (snapshot.provider) {
      const readiness = ce('div', 'check-vmore');
      readiness.textContent = snapshot.provider.summary;
      sec.appendChild(readiness);

      if (snapshot.provider.reason) {
        const readinessReason = ce('div', 'check-vmore');
        readinessReason.textContent = snapshot.provider.reason;
        sec.appendChild(readinessReason);
      }
    }

    if (snapshot.live_repair_reason) {
      const liveReason = ce('div', 'check-vmore');
      liveReason.textContent = snapshot.live_repair_reason;
      sec.appendChild(liveReason);
    }

    if (snapshot.generation_meta) {
      const meta = ce('div', 'check-vmore');
      meta.textContent = snapshot.generation_meta;
      sec.appendChild(meta);
    }

    if (snapshot.proposal) {
      const proposal = ce('div', 'check-vaffect');
      proposal.textContent = snapshot.proposal;
      sec.appendChild(proposal);
    }

    if (snapshot.issue_badge && snapshot.issue_stage && snapshot.issue_summary) {
      const degraded = ce('div', 'check-gate-summary');
      const issueBadge = ce('span', `check-gate-badge ${snapshot.status_state === 'degraded' ? 'check-gate-mid' : 'check-gate-high'}`);
      issueBadge.textContent = snapshot.issue_badge;
      degraded.appendChild(issueBadge);
      const stageBadge = ce('span', 'check-gate-badge check-gate-low');
      stageBadge.textContent = `阶段 ${snapshot.issue_stage}`;
      degraded.appendChild(stageBadge);
      sec.appendChild(degraded);

      const issueText = ce('div', 'check-vmore');
      issueText.textContent = snapshot.issue_summary;
      sec.appendChild(issueText);

      if (snapshot.issue_note) {
        const note = ce('div', 'check-vmore');
        note.textContent = snapshot.issue_note;
        sec.appendChild(note);
      }

      if (snapshot.preflight) {
        const preflight = ce('div', 'check-vmore');
        preflight.textContent = snapshot.preflight.summary;
        sec.appendChild(preflight);

        if (snapshot.preflight.failed_commands.length > 0) {
          const commands = ce('div', 'check-vmore');
          commands.textContent = `失败命令: ${snapshot.preflight.failed_commands.join(' · ')}`;
          sec.appendChild(commands);
        }

        if (snapshot.preflight.blocking_rule_ids.length > 0) {
          const rules = ce('div', 'check-vmore');
          rules.textContent = `阻断规则: ${snapshot.preflight.blocking_rule_ids.join(' · ')}`;
          sec.appendChild(rules);
        }
      }

      if (snapshot.rollback) {
        const rollback = ce('div', 'check-vmore');
        rollback.textContent = snapshot.rollback;
        sec.appendChild(rollback);
      }
    }

    if (snapshot.repair_history.length > 0) {
      const trace = ce('div', 'check-vmore');
      trace.textContent = `Evidence trace: finding ${snapshot.evidence_trace.finding_count} 条 · evidence ${snapshot.evidence_trace.evidence_count} 个 · repair history ${snapshot.evidence_trace.repair_history_count} 条`;
      sec.appendChild(trace);

      const historyTitle = ce('div', 'check-vmore');
      historyTitle.textContent = 'Repair 历史';
      sec.appendChild(historyTitle);

      for (const item of snapshot.repair_history.slice(0, 3)) {
        const row = ce('div', 'check-vmore');
        const stateChange = item.state_change?.from_state || item.state_change?.to_state
          ? ` · ${item.state_change?.from_state || '?'} -> ${item.state_change?.to_state || '?'}`
          : '';
        const retryable = item.error?.retryable === true ? ' · 可重试' : item.error?.retryable === false ? ' · 需修正' : '';
        row.textContent = `${fmtTime(item.timestamp)} · ${item.stage} · ${item.status}${stateChange}${retryable} · ${item.reason}`;
        sec.appendChild(row);
      }
    }

    const actions = ce('div', 'check-gate-summary');
    if (plan.approval_state === 'draft' || plan.approval_state === 'rejected' || plan.approval_state === 'rolled_back') {
      const disabledReason = state.repair_generation_readiness && !state.repair_generation_readiness.eligible
        ? state.repair_generation_readiness.reason
        : null;
      actions.appendChild(this.actionButton(
        disabledReason
          ? (state.repair_generation_readiness?.finding_count === 0 ? '当前无可修复风险' : '当前不可自动修复')
          : '生成补丁提案',
        () => bus.emit('repair:generate-proposal'),
        !disabledReason,
        disabledReason || undefined,
      ));
    }
    if (plan.approval_state === 'waiting_approval') {
      actions.appendChild(this.actionButton('审批修复', () => bus.emit('repair:request-approval')));
    }
    if (plan.approval_state === 'approved') {
      actions.appendChild(this.actionButton('应用修复', () => bus.emit('repair:apply')));
    }
    if (plan.approval_state === 'applied') {
      actions.appendChild(this.actionButton('回滚修复', () => bus.emit('repair:rollback')));
    }
    if (actions.childElementCount > 0) {
      sec.appendChild(actions);
    }

    return sec;
  }

  private actionButton(label: string, onClick: () => void, enabled = true, title?: string): HTMLButtonElement {
    const button = document.createElement('button');
    button.className = 'check-ask-btn';
    button.textContent = label;
    button.style.width = 'auto';
    button.style.padding = '6px 10px';
    button.disabled = !enabled;
    if (title) button.title = title;
    button.addEventListener('click', (event) => {
      if (!enabled) return;
      event.stopPropagation();
      onClick();
    });
    return button;
  }

  private scrollToSection(sectionId: 'risk-summary' | 'gate-decision' | 'repair-workbench'): void {
    const target = this.content.querySelector<HTMLElement>(`[data-check-section="${sectionId}"]`);
    target?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }

  private refreshTabLabel(): void {
    const totalViolations = this.lastResult
      ? (this.lastResult.l5_violations?.length || 0)
        + (this.lastResult.l4_violations?.length || 0)
        + (this.lastResult.l3_violations?.length || 0)
        + (this.lastResult.l2_violations?.length || 0)
      : 0;
    const auditCount = this.lastAuditRows.length;

    if (totalViolations === 0 && auditCount === 0) {
      this.tabLabelEl.textContent = '简报';
      const reviewStatus = document.getElementById('status-review');
      if (reviewStatus) reviewStatus.textContent = '';
      document.title = '🔮 全息观测站 — HoloGram Observatory';
      return;
    }

    this.tabLabelEl.textContent = `简报 ${totalViolations}风控 ${auditCount}审计`;
    const reviewStatus = document.getElementById('status-review');
    if (reviewStatus) reviewStatus.textContent = `风控${totalViolations} · 审计${auditCount}`;
    document.title = `🔮 风控${totalViolations} 审计${auditCount} — 全息观测站`;
  }

  private statItem(label: string, value: string): HTMLElement {
    const el = ce('div', 'check-stat');
    const lbl = ce('span', 'check-stat-label');
    lbl.textContent = label;
    const val = ce('span', 'check-stat-value');
    val.textContent = value;
    el.append(lbl, val);
    return el;
  }

  // ── P8: Gate check rendering ──

  async loadAndRenderGate(path: string): Promise<void> {
    try {
      const { invoke } = await import('../bridge');
      const json = await invoke<string>('hologram_gate_check', { path, moduleFile: null });
      const data = JSON.parse(json) as GateData;
      this.renderGate(data);
    } catch (err) {
      console.error('Gate check failed:', err);
    }
  }

  private renderGate(data: GateData): void {
    if (!data || !data.modules || data.modules.length === 0) return;

    // Remove existing gate section if any
    const existing = this.content.querySelector('.check-gate');
    if (existing) existing.remove();

    const gateSec = ce('div', 'check-section check-gate');
    const gateTitle = ce('div', 'check-section-title');
    gateTitle.innerHTML = `${iconHtml('block', 11)} 门禁评估 (${data.total_evaluated} 模块)`;
    gateSec.appendChild(gateTitle);

    // Risk summary
    const summaryRow = ce('div', 'check-gate-summary');
    if (data.high_risk > 0) {
      const hi = ce('span', 'check-gate-badge check-gate-high');
      hi.textContent = `⚠ ${data.high_risk} 高风险`;
      summaryRow.appendChild(hi);
    }
    if (data.medium_risk > 0) {
      const mi = ce('span', 'check-gate-badge check-gate-mid');
      mi.textContent = `⚡ ${data.medium_risk} 中风险`;
      summaryRow.appendChild(mi);
    }
    const lo = ce('span', 'check-gate-badge check-gate-low');
    lo.textContent = `✓ ${data.low_risk} 低风险`;
    summaryRow.appendChild(lo);
    gateSec.appendChild(summaryRow);

    // Show high/medium risk modules with details
    for (const m of data.modules) {
      if (m.risk === 'low') continue; // Skip low risk
      const item = ce('div', `check-gate-item check-gate-${m.risk}`);
      const head = ce('div', 'check-gate-item-head');
      const riskBadge = ce('span', `check-gate-risk check-gate-risk-${m.risk}`);
      riskBadge.textContent = m.risk === 'high' ? '高' : '中';
      head.appendChild(riskBadge);
      const nameEl = ce('span', 'check-gate-name');
      nameEl.textContent = m.name;
      head.appendChild(nameEl);
      const stats = ce('span', 'check-gate-stats');
      stats.textContent = `扇入${m.fan_in} 扇出${m.fan_out} L4×${m.coupling_l4}`;
      head.appendChild(stats);
      item.appendChild(head);

      if (m.recommendations && m.recommendations.length > 0) {
        for (const rec of m.recommendations) {
          const recEl = ce('div', 'check-gate-rec');
          recEl.textContent = rec;
          item.appendChild(recEl);
        }
      }
      gateSec.appendChild(item);
    }

    this.content.appendChild(gateSec);
  }
}

function repairBadgeClass(state: CurrentReviewState['repair_plan']['approval_state']): string {
  switch (state) {
    case 'approved':
    case 'applied':
      return 'check-gate-low';
    case 'waiting_approval':
      return 'check-gate-mid';
    case 'rejected':
    case 'rolled_back':
      return 'check-gate-high';
    default:
      return 'check-gate-mid';
  }
}

function queueStateLabel(state: string): string {
  switch (state) {
    case 'needs_attention': return '待处理';
    case 'clean': return '已清空';
    case 'ready': return '已就绪';
    case 'waiting_approval': return '待审批';
    case 'require_approval': return '需审批';
    case 'block': return '已阻断';
    case 'warn': return '警告';
    case 'approved': return '已批准';
    case 'applied': return '已应用';
    case 'rolled_back': return '已回滚';
    case 'failed': return '失败';
    case 'degraded': return '降级';
    case 'draft': return '草稿';
    default: return state;
  }
}

function queueToneClass(state: string): string {
  switch (state) {
    case 'block':
    case 'failed':
      return 'check-gate-high';
    case 'warn':
    case 'needs_attention':
    case 'degraded':
    case 'require_approval':
    case 'waiting_approval':
      return 'check-gate-mid';
    default:
      return 'check-gate-low';
  }
}

function queueRiskClass(state: string): string {
  switch (state) {
    case 'block':
    case 'failed':
      return 'check-gate-risk-high';
    case 'warn':
    case 'needs_attention':
    case 'degraded':
    case 'require_approval':
    case 'waiting_approval':
      return 'check-gate-risk-mid';
    default:
      return 'check-gate-risk-low';
  }
}

function gateDecisionBadgeClass(decision: CurrentReviewState['gate_decision']['decision']): string {
  switch (decision) {
    case 'allow':
      return 'check-gate-low';
    case 'warn':
      return 'check-gate-mid';
    case 'require_approval':
      return 'check-gate-mid';
    case 'block':
      return 'check-gate-high';
  }
}

function gateDecisionLabel(decision: CurrentReviewState['gate_decision']['decision']): string {
  switch (decision) {
    case 'allow':
      return '允许';
    case 'warn':
      return '警告';
    case 'require_approval':
      return '需审批';
    case 'block':
      return '阻断';
  }
}

// ── Gate data types ──

interface GateModule {
  file: string;
  name: string;
  node_count: number;
  fan_in: number;
  fan_out: number;
  coupling_l1: number;
  coupling_l2: number;
  coupling_l3: number;
  coupling_l4: number;
  risk: 'high' | 'medium' | 'low';
  recommendations: string[];
}

interface GateData {
  modules: GateModule[];
  total_evaluated: number;
  high_risk: number;
  medium_risk: number;
  low_risk: number;
  error?: string;
}

// ── Helpers ──

function ce(tag: string, cls?: string): HTMLElement {
  const el = document.createElement(tag);
  if (cls) el.className = cls;
  return el;
}

function basename(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/');
  return parts[parts.length - 1] || path;
}

function fmtTime(iso: string): string {
  if (!iso) return '';
  try {
    const d = new Date(iso);
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
  } catch {
    return iso.slice(11, 19) || iso.slice(0, 19);
  }
}

function severityBadge(severity: 'info' | 'low' | 'medium' | 'high' | 'critical'): string {
  switch (severity) {
    case 'critical': return '高';
    case 'high': return '高';
    case 'medium': return '中';
    case 'low': return '低';
    default: return '提示';
  }
}

function riskClassForSeverity(severity: 'info' | 'low' | 'medium' | 'high' | 'critical'): string {
  return severity === 'critical' || severity === 'high'
    ? 'check-gate-high'
    : severity === 'medium'
      ? 'check-gate-mid'
      : 'check-gate-low';
}

function riskLabelClassForSeverity(severity: 'info' | 'low' | 'medium' | 'high' | 'critical'): string {
  return severity === 'critical' || severity === 'high'
    ? 'check-gate-risk-high'
    : severity === 'medium'
      ? 'check-gate-risk-mid'
      : 'check-gate-risk-low';
}
