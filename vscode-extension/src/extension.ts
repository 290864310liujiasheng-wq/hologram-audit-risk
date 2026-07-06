import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';

/**
 * Severity strings the audit-risk CLI's `--json` output uses. Mapped to
 * VS Code's DiagnosticSeverity so findings render with the right icon/color
 * in the Problems panel and inline squiggles — matching the severity
 * coloring the CLI itself already applies in terminal output.
 */
function severityToDiagnosticSeverity(severity: string): vscode.DiagnosticSeverity {
  switch (severity) {
    case 'critical':
    case 'high':
      return vscode.DiagnosticSeverity.Error;
    case 'medium':
      return vscode.DiagnosticSeverity.Warning;
    default:
      return vscode.DiagnosticSeverity.Information;
  }
}

interface AuditRiskFinding {
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

interface AuditRiskCheckPayload {
  schema_version: string;
  workspace_root: string;
  review: {
    findings: AuditRiskFinding[];
    gate_decision: {
      decision: string;
      reason: string;
      finding_count: number;
    };
  };
}

const OUTPUT_CHANNEL_NAME = 'audit-risk';

/** Resolve the audit-risk binary path: explicit setting first, else PATH. */
function resolveBinaryPath(): string {
  const configured = vscode.workspace.getConfiguration('auditRisk').get<string>('binaryPath');
  if (configured && configured.trim().length > 0) {
    return configured.trim();
  }
  return 'audit-risk';
}

/**
 * Run `audit-risk check <workspace> --json` and parse its stdout.
 *
 * Deliberately does NOT treat a non-zero exit code as failure on its own —
 * the CLI's gate mechanism returns exit code 2 when findings meet the
 * fail-on threshold, but still prints a complete, valid JSON payload on
 * stdout in that case (verified against the real binary). Only an empty or
 * unparseable stdout — which happens on exit codes 3 (environment error)
 * and 4 (usage error), where no JSON is produced at all — is treated as a
 * real failure.
 */
function runAuditRiskCheck(binaryPath: string, workspaceRoot: string): Promise<AuditRiskCheckPayload> {
  return new Promise((resolve, reject) => {
    cp.execFile(
      binaryPath,
      ['check', workspaceRoot, '--json', '--fail-on', 'off'],
      { maxBuffer: 1024 * 1024 * 32 },
      (error, stdout, stderr) => {
        const trimmed = stdout.trim();
        if (trimmed.length === 0) {
          const reason = stderr.trim() || (error ? error.message : 'no output');
          reject(new Error(`audit-risk produced no output: ${reason}`));
          return;
        }
        try {
          const payload = JSON.parse(trimmed) as AuditRiskCheckPayload;
          resolve(payload);
        } catch (parseError) {
          reject(new Error(`audit-risk output was not valid JSON: ${(parseError as Error).message}`));
        }
      }
    );
  });
}

function findingsToDiagnosticsByFile(
  findings: AuditRiskFinding[],
  workspaceRoot: string
): Map<string, vscode.Diagnostic[]> {
  const byFile = new Map<string, vscode.Diagnostic[]>();

  for (const finding of findings) {
    const absolutePath = path.isAbsolute(finding.location.file_path)
      ? finding.location.file_path
      : path.join(workspaceRoot, finding.location.file_path);

    // The CLI uses line 0 for file-level findings that have no specific
    // line (migration/config/coupling-style structural signals) and a real
    // 1-indexed line number for line-precise findings (e.g. secret scans).
    // VS Code Range/Position are 0-indexed, so a real line N maps to N-1;
    // a structural line-0 finding anchors to the first line since VS Code
    // needs some valid range even for whole-file findings.
    const startLine = Math.max(0, finding.location.start_line - 1);
    const endLine = Math.max(startLine, finding.location.end_line - 1);
    const range = new vscode.Range(startLine, 0, endLine, Number.MAX_SAFE_INTEGER);

    const diagnostic = new vscode.Diagnostic(
      range,
      finding.plain_explanation,
      severityToDiagnosticSeverity(finding.severity)
    );
    diagnostic.source = 'audit-risk';
    diagnostic.code = finding.rule_id;

    const existing = byFile.get(absolutePath) ?? [];
    existing.push(diagnostic);
    byFile.set(absolutePath, existing);
  }

  return byFile;
}

export function activate(context: vscode.ExtensionContext): void {
  const diagnostics = vscode.languages.createDiagnosticCollection('auditRisk');
  const output = vscode.window.createOutputChannel(OUTPUT_CHANNEL_NAME);
  context.subscriptions.push(diagnostics, output);

  async function runCheckCommand(): Promise<void> {
    const folders = vscode.workspace.workspaceFolders;
    if (!folders || folders.length === 0) {
      vscode.window.showWarningMessage('audit-risk: 当前没有打开任何工作区文件夹。');
      return;
    }
    const workspaceRoot = folders[0].uri.fsPath;
    const binaryPath = resolveBinaryPath();

    output.appendLine(`[audit-risk] 正在审查 ${workspaceRoot} ...`);

    let payload: AuditRiskCheckPayload;
    try {
      payload = await runAuditRiskCheck(binaryPath, workspaceRoot);
    } catch (error) {
      const message = (error as Error).message;
      output.appendLine(`[audit-risk] 失败：${message}`);
      const selection = await vscode.window.showErrorMessage(
        `audit-risk 运行失败：${message}`,
        '打开安装说明',
        '设置可执行文件路径'
      );
      if (selection === '打开安装说明') {
        vscode.env.openExternal(
          vscode.Uri.parse('https://github.com/834063245-creator/HoloGram#安装')
        );
      } else if (selection === '设置可执行文件路径') {
        vscode.commands.executeCommand('workbench.action.openSettings', 'auditRisk.binaryPath');
      }
      return;
    }

    diagnostics.clear();
    const byFile = findingsToDiagnosticsByFile(payload.review.findings, payload.workspace_root);
    for (const [filePath, fileDiagnostics] of byFile) {
      diagnostics.set(vscode.Uri.file(filePath), fileDiagnostics);
    }

    const gate = payload.review.gate_decision;
    const findingCount = payload.review.findings.length;
    output.appendLine(
      `[audit-risk] 完成：${findingCount} 条风险，gate=${gate.decision}（${gate.reason}）`
    );

    if (findingCount === 0) {
      vscode.window.showInformationMessage('audit-risk: 当前没有发现风险。');
    } else {
      vscode.window.showWarningMessage(
        `audit-risk: 发现 ${findingCount} 条风险（gate=${gate.decision}），已显示在“问题”面板。`
      );
    }
  }

  function clearCommand(): void {
    diagnostics.clear();
    output.appendLine('[audit-risk] 已清除审查结果。');
  }

  context.subscriptions.push(
    vscode.commands.registerCommand('auditRisk.check', runCheckCommand),
    vscode.commands.registerCommand('auditRisk.clear', clearCommand)
  );

  const runOnSave = vscode.workspace.getConfiguration('auditRisk').get<boolean>('runOnSave');
  if (runOnSave) {
    context.subscriptions.push(
      vscode.workspace.onDidSaveTextDocument(() => {
        void runCheckCommand();
      })
    );
  }
}

export function deactivate(): void {
  // No explicit teardown needed — diagnostics/output channel are disposed
  // via context.subscriptions.
}
