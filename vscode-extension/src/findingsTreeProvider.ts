import * as vscode from 'vscode';
import * as path from 'path';

export interface TreeFinding {
  finding_id: string;
  rule_id: string;
  severity: string;
  plain_explanation: string;
  location: {
    file_path: string;
    start_line: number;
    end_line: number;
  };
}

export interface TreeGateDecision {
  decision: string;
  reason: string;
  finding_count: number;
}

function severityLabel(severity: string): string {
  switch (severity) {
    case 'critical':
      return '严重';
    case 'high':
      return '高危';
    case 'medium':
      return '中危';
    default:
      return '低危';
  }
}

function gateLabel(decision: string): string {
  switch (decision) {
    case 'block':
      return '阻断';
    case 'require_approval':
      return '需要人工确认';
    case 'warn':
      return '告警';
    default:
      return '通过';
  }
}

/** Ordered so the tree always lists severity groups from worst to least. */
const SEVERITY_ORDER = ['critical', 'high', 'medium', 'low'];

type NodeKind = 'gate' | 'severityGroup' | 'finding' | 'empty';

export class FindingTreeItem extends vscode.TreeItem {
  constructor(
    public readonly kind: NodeKind,
    label: string,
    collapsibleState: vscode.TreeItemCollapsibleState,
    public readonly finding?: TreeFinding,
    public readonly severity?: string
  ) {
    super(label, collapsibleState);
  }
}

/**
 * Sidebar findings + gate decision panel (VS Code step 3). Shares the same
 * check payload the Problems panel is populated from — `update()` is called
 * right after `findingsToDiagnosticsByFile()` in extension.ts, so the two
 * views never disagree about what the last check found.
 */
export class FindingsTreeProvider implements vscode.TreeDataProvider<FindingTreeItem> {
  private readonly _onDidChangeTreeData = new vscode.EventEmitter<FindingTreeItem | undefined | void>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private findings: TreeFinding[] = [];
  private gate: TreeGateDecision | undefined;
  private workspaceRoot: string | undefined;

  update(findings: TreeFinding[], gate: TreeGateDecision, workspaceRoot: string): void {
    this.findings = findings;
    this.gate = gate;
    this.workspaceRoot = workspaceRoot;
    this._onDidChangeTreeData.fire();
  }

  clear(): void {
    this.findings = [];
    this.gate = undefined;
    this.workspaceRoot = undefined;
    this._onDidChangeTreeData.fire();
  }

  getTreeItem(element: FindingTreeItem): vscode.TreeItem {
    return element;
  }

  getChildren(element?: FindingTreeItem): FindingTreeItem[] {
    if (!this.gate) {
      const empty = new FindingTreeItem(
        'empty',
        '尚未运行审查 — 执行 "audit-risk: 审查当前工作区"',
        vscode.TreeItemCollapsibleState.None
      );
      empty.iconPath = new vscode.ThemeIcon('info');
      return [empty];
    }

    if (!element) {
      const gateItem = new FindingTreeItem(
        'gate',
        `Gate：${gateLabel(this.gate.decision)} — ${this.gate.reason}`,
        vscode.TreeItemCollapsibleState.None
      );
      gateItem.iconPath = new vscode.ThemeIcon(
        this.gate.decision === 'block'
          ? 'error'
          : this.gate.decision === 'require_approval'
          ? 'warning'
          : this.gate.decision === 'warn'
          ? 'alert'
          : 'check'
      );

      const groups = SEVERITY_ORDER.map((severity) => {
        const count = this.findings.filter((f) => f.severity === severity).length;
        return { severity, count };
      }).filter((group) => group.count > 0);

      if (groups.length === 0) {
        const clean = new FindingTreeItem('empty', '当前没有发现风险', vscode.TreeItemCollapsibleState.None);
        clean.iconPath = new vscode.ThemeIcon('pass');
        return [gateItem, clean];
      }

      const groupItems = groups.map((group) => {
        const item = new FindingTreeItem(
          'severityGroup',
          `${severityLabel(group.severity)}（${group.count}）`,
          vscode.TreeItemCollapsibleState.Expanded,
          undefined,
          group.severity
        );
        item.iconPath = new vscode.ThemeIcon(
          group.severity === 'critical' || group.severity === 'high' ? 'error' : 'warning'
        );
        return item;
      });

      return [gateItem, ...groupItems];
    }

    if (element.kind === 'severityGroup' && element.severity) {
      return this.findings
        .filter((f) => f.severity === element.severity)
        .map((finding) => {
          const label = `${finding.location.file_path}:${finding.location.start_line} — ${finding.plain_explanation}`;
          const item = new FindingTreeItem('finding', label, vscode.TreeItemCollapsibleState.None, finding);
          item.tooltip = finding.plain_explanation;
          item.command = {
            command: 'auditRisk.openFinding',
            title: '打开风险位置',
            arguments: [finding, this.workspaceRoot],
          };
          return item;
        });
    }

    return [];
  }
}

/** Open the finding's file and reveal its line — used by the tree item's click command. */
export async function openFinding(finding: TreeFinding, workspaceRoot: string | undefined): Promise<void> {
  if (!workspaceRoot) {
    return;
  }
  const absolutePath = path.isAbsolute(finding.location.file_path)
    ? finding.location.file_path
    : path.join(workspaceRoot, finding.location.file_path);
  const uri = vscode.Uri.file(absolutePath);
  const document = await vscode.workspace.openTextDocument(uri);
  const editor = await vscode.window.showTextDocument(document);
  const line = Math.max(0, finding.location.start_line - 1);
  const range = new vscode.Range(line, 0, line, 0);
  editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
  editor.selection = new vscode.Selection(range.start, range.start);
}
