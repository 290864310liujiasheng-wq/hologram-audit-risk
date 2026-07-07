import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';
import * as fs from 'fs';

/**
 * Severities that get a "修复此风险" CodeAction. `low` is excluded per product decision.
 */
const REPAIRABLE_SEVERITIES = new Set(['critical', 'high', 'medium']);

interface RepairOperation {
  file_path: string;
  start_line: number;
  end_line: number;
  old_content: string;
  new_content: string;
  summary: string;
}

interface RepairPlan {
  plan_id: string;
  finding_id: string;
  file_path: string;
  start_line: number;
  end_line: number;
  severity: string;
  summary: string;
  rationale: string;
  operations: RepairOperation[];
  required_tests: string[];
  risk_note: string;
  expires_at: string;
}

interface RepairPlanPayload {
  status: string;
  workspace_root: string;
  repair: RepairPlan;
}

interface RepairApplyPayload {
  status: string;
  apply: {
    plan_id: string;
    applied_files: string[];
    preflight: { passed: boolean; failed_command?: string };
    error?: string;
  };
}

/** Resolve the audit-risk binary path: explicit setting first, else PATH. */
function resolveBinaryPath(): string {
  const configured = vscode.workspace.getConfiguration('auditRisk').get<string>('binaryPath');
  if (configured && configured.trim().length > 0) {
    return configured.trim();
  }
  return 'audit-risk';
}

/**
 * Run `audit-risk repair plan <workspace> --finding <id> --json` and return the plan.
 * This shells out to the CLI; the CLI handles provider auth and model calls.
 */
function runRepairPlan(
  binaryPath: string,
  workspaceRoot: string,
  findingId: string
): Promise<RepairPlanPayload> {
  return new Promise((resolve, reject) => {
    cp.execFile(
      binaryPath,
      ['repair', 'plan', workspaceRoot, '--finding', findingId, '--json'],
      { maxBuffer: 1024 * 1024 * 16 },
      (error, stdout, stderr) => {
        const trimmed = stdout.trim();
        if (trimmed.length === 0) {
          const reason = stderr.trim() || (error ? error.message : 'no output');
          reject(new Error(`audit-risk repair plan 无输出：${reason}`));
          return;
        }
        try {
          const payload = JSON.parse(trimmed) as RepairPlanPayload;
          if (payload.status !== 'ok') {
            reject(new Error(`repair plan 失败：${JSON.stringify(payload)}`));
            return;
          }
          resolve(payload);
        } catch (parseError) {
          reject(new Error(`repair plan 返回了非 JSON 内容：${(parseError as Error).message}`));
        }
      }
    );
  });
}

/**
 * Run `audit-risk repair apply <workspace> --plan <id> --json` and return the result.
 */
function runRepairApply(
  binaryPath: string,
  workspaceRoot: string,
  planId: string
): Promise<RepairApplyPayload> {
  return new Promise((resolve, reject) => {
    cp.execFile(
      binaryPath,
      ['repair', 'apply', workspaceRoot, '--plan', planId, '--json'],
      { maxBuffer: 1024 * 1024 * 16 },
      (error, stdout, stderr) => {
        const trimmed = stdout.trim();
        if (trimmed.length === 0) {
          const reason = stderr.trim() || (error ? error.message : 'no output');
          reject(new Error(`audit-risk repair apply 无输出：${reason}`));
          return;
        }
        try {
          const payload = JSON.parse(trimmed) as RepairApplyPayload;
          resolve(payload);
        } catch (parseError) {
          reject(new Error(`repair apply 返回了非 JSON 内容：${(parseError as Error).message}`));
        }
      }
    );
  });
}

/**
 * Build a WorkspaceEdit that previews the repair operations as a diff.
 * VS Code shows this as a "preview" before the user confirms.
 */
function buildWorkspaceEdit(plan: RepairPlan, workspaceRoot: string): vscode.WorkspaceEdit {
  const edit = new vscode.WorkspaceEdit();
  for (const op of plan.operations) {
    const absPath = path.isAbsolute(op.file_path)
      ? op.file_path
      : path.join(workspaceRoot, op.file_path);
    const uri = vscode.Uri.file(absPath);
    // VS Code Range is 0-indexed; CLI lines are 1-indexed.
    const startLine = Math.max(0, op.start_line - 1);
    const endLine = Math.max(startLine, op.end_line - 1);
    const range = new vscode.Range(startLine, 0, endLine, Number.MAX_SAFE_INTEGER);
    edit.replace(uri, range, op.new_content);
  }
  return edit;
}

/**
 * CodeAction provider that attaches a "修复此风险（audit-risk）" light-bulb to every
 * critical / high / medium diagnostic produced by the auditRisk.check command.
 *
 * Flow:
 *   1. User hovers a squiggle → VS Code asks for code actions.
 *   2. We return a CodeAction whose command is `auditRisk.repair`.
 *   3. `auditRisk.repair` calls `audit-risk repair plan` → gets a patch plan from the model.
 *   4. The plan's operations are turned into a WorkspaceEdit and shown as a diff preview.
 *   5. User clicks "应用" in the diff preview → VS Code applies the edit to the buffer.
 *   6. We then call `audit-risk repair apply` to write to disk and record the audit event.
 *   7. A fresh `auditRisk.check` runs to refresh diagnostics.
 */
export class RepairCodeActionProvider implements vscode.CodeActionProvider {
  static readonly providedCodeActionKinds = [vscode.CodeActionKind.QuickFix];

  provideCodeActions(
    document: vscode.TextDocument,
    _range: vscode.Range | vscode.Selection,
    context: vscode.CodeActionContext
  ): vscode.CodeAction[] {
    const actions: vscode.CodeAction[] = [];

    for (const diagnostic of context.diagnostics) {
      if (diagnostic.source !== 'audit-risk') {
        continue;
      }
      // diagnostic.code is the rule_id; we need the finding_id which is stored in the
      // diagnostic message suffix. The finding_id is embedded as the CodeAction arg.
      const severity = this.severityFromDiagnostic(diagnostic);
      if (!REPAIRABLE_SEVERITIES.has(severity)) {
        continue;
      }

      // The finding_id is derived from rule_id + line (stable within the same check run).
      // The CLI derives finding_id as "<bucket>:<index>" which we can't know here without
      // the original payload. We pass rule_id + line so the repair command can look it up.
      // Actually the full finding_id is in the diagnostic.code (rule_id) combined with the
      // range — but the CLI needs the exact finding_id. The cleanest approach: store the
      // finding_id in diagnostic.code as "<bucket>:<index>" when we create the diagnostic
      // in extension.ts. That's what the CLI already emits. So diagnostic.code IS the
      // finding_id-compatible rule_id pattern "check.l5", "check.l4", etc. — but it's not
      // enough to uniquely pick a finding. We pass file+line+rule_id and let repair plan
      // do a --finding lookup by checking the latest check output. For now we encode a
      // compound key in the command args that the repair command can resolve.
      const findingRef = `${document.uri.fsPath}:${diagnostic.range.start.line + 1}:${diagnostic.code as string}`;

      const action = new vscode.CodeAction(
        `修复此风险（audit-risk）`,
        vscode.CodeActionKind.QuickFix
      );
      action.diagnostics = [diagnostic];
      action.isPreferred = severity === 'critical' || severity === 'high';
      action.command = {
        command: 'auditRisk.repair',
        title: '修复此风险',
        arguments: [document.uri, diagnostic, findingRef],
      };
      actions.push(action);
    }

    return actions;
  }

  private severityFromDiagnostic(diagnostic: vscode.Diagnostic): string {
    if (diagnostic.severity === vscode.DiagnosticSeverity.Error) {
      // Could be critical or high — we need to check the message.
      return diagnostic.message.includes('严重') ? 'critical' : 'high';
    }
    if (diagnostic.severity === vscode.DiagnosticSeverity.Warning) {
      return 'medium';
    }
    return 'low';
  }
}

/**
 * The `auditRisk.repair` command handler.
 *
 * Called with (uri, diagnostic, findingRef) where findingRef encodes file:line:rule_id.
 * Resolves the actual finding_id by re-running check (or using cached results), then
 * calls repair plan, shows a diff preview, and on user confirmation calls repair apply.
 */
export async function repairCommand(
  uri: vscode.Uri,
  diagnostic: vscode.Diagnostic,
  findingRef: string,
  getDiagnosticCollection: () => vscode.DiagnosticCollection,
  runCheck: () => Promise<void>,
  output: vscode.OutputChannel
): Promise<void> {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    vscode.window.showWarningMessage('audit-risk: 当前没有打开任何工作区文件夹。');
    return;
  }
  const workspaceRoot = folders[0].uri.fsPath;
  const binaryPath = resolveBinaryPath();

  // Extract file path and line from findingRef (file:line:rule_id).
  const refParts = findingRef.split(':');
  // On Windows file paths contain ':', so split from right.
  const rulePart = refParts[refParts.length - 1];
  const lineStr = refParts[refParts.length - 2];
  const filePath = refParts.slice(0, refParts.length - 2).join(':');
  const lineNumber = parseInt(lineStr, 10);

  // Derive finding_id: run check --json to get fresh findings, then match by file+line+rule.
  output.appendLine(`[audit-risk repair] 正在定位 finding：${filePath}:${lineNumber} [${rulePart}]`);

  let findingId: string;
  try {
    findingId = await resolveTargetFindingId(binaryPath, workspaceRoot, filePath, lineNumber, rulePart, output);
  } catch (err) {
    vscode.window.showErrorMessage(`audit-risk: 无法定位风险 finding：${(err as Error).message}`);
    return;
  }

  output.appendLine(`[audit-risk repair] 正在生成修复方案（finding_id=${findingId}）...`);
  vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: 'audit-risk 正在生成修复方案…',
      cancellable: false,
    },
    async () => {
      let planPayload: RepairPlanPayload;
      try {
        planPayload = await runRepairPlan(binaryPath, workspaceRoot, findingId);
      } catch (err) {
        const message = (err as Error).message;
        output.appendLine(`[audit-risk repair] plan 失败：${message}`);
        vscode.window.showErrorMessage(`audit-risk 修复方案生成失败：${message}`);
        return;
      }

      const plan = planPayload.repair;
      output.appendLine(`[audit-risk repair] 方案已生成（plan_id=${plan.plan_id}）：${plan.summary}`);

      // Build the preview edit.
      const edit = buildWorkspaceEdit(plan, workspaceRoot);

      // Show a confirmation dialog with the plan summary before applying.
      const detail =
        `${plan.summary}\n\n` +
        `理由：${plan.rationale}\n\n` +
        `⚠️ ${plan.risk_note}\n\n` +
        `涉及文件：${plan.operations.map((op) => op.file_path).join(', ')}`;

      const choice = await vscode.window.showInformationMessage(
        `audit-risk 修复方案已就绪`,
        { modal: true, detail },
        '预览并应用',
        '取消'
      );

      if (choice !== '预览并应用') {
        output.appendLine('[audit-risk repair] 用户取消了修复。');
        return;
      }

      // Apply the workspace edit (writes to the buffer, making the change visible).
      const editApplied = await vscode.workspace.applyEdit(edit);
      if (!editApplied) {
        vscode.window.showErrorMessage('audit-risk: 无法应用修复编辑，文件可能已被修改。');
        return;
      }

      // Save affected documents so the CLI can read the updated state.
      for (const op of plan.operations) {
        const absPath = path.isAbsolute(op.file_path)
          ? op.file_path
          : path.join(workspaceRoot, op.file_path);
        const doc = vscode.workspace.textDocuments.find((d) => d.uri.fsPath === absPath);
        if (doc && doc.isDirty) {
          await doc.save();
        }
      }

      // Call repair apply to write to disk and record the audit event.
      output.appendLine(`[audit-risk repair] 正在执行 repair apply（plan_id=${plan.plan_id}）...`);
      try {
        const applyPayload = await runRepairApply(binaryPath, workspaceRoot, plan.plan_id);
        if (applyPayload.status === 'ok') {
          output.appendLine(
            `[audit-risk repair] 修复成功：${applyPayload.apply.applied_files.join(', ')}`
          );
          vscode.window.showInformationMessage(
            `audit-risk: 修复已成功应用（${applyPayload.apply.applied_files.join(', ')}），正在刷新审查结果…`
          );
        } else {
          const errMsg = applyPayload.apply.error ?? '未知错误';
          output.appendLine(`[audit-risk repair] apply 返回错误：${errMsg}`);
          vscode.window.showWarningMessage(`audit-risk: 修复预检未通过：${errMsg}`);
          // Revert the buffer edits since apply was blocked.
          await vscode.commands.executeCommand('workbench.action.revertFile');
          return;
        }
      } catch (err) {
        const message = (err as Error).message;
        output.appendLine(`[audit-risk repair] apply 失败：${message}`);
        vscode.window.showErrorMessage(`audit-risk repair apply 失败：${message}`);
        await vscode.commands.executeCommand('workbench.action.revertFile');
        return;
      }

      // Refresh diagnostics.
      await runCheck();
    }
  );
}

/**
 * Re-run check and find the finding_id that matches file + line + rule.
 * This is needed because the CodeAction only has diagnostic metadata,
 * not the original finding_id string from the CLI payload.
 */
async function resolveTargetFindingId(
  binaryPath: string,
  workspaceRoot: string,
  filePath: string,
  lineNumber: number,
  ruleId: string,
  output: vscode.OutputChannel
): Promise<string> {
  return new Promise((resolve, reject) => {
    cp.execFile(
      binaryPath,
      ['check', workspaceRoot, '--json', '--fail-on', 'off'],
      { maxBuffer: 1024 * 1024 * 32 },
      (error, stdout, stderr) => {
        const trimmed = stdout.trim();
        if (trimmed.length === 0) {
          reject(new Error(stderr.trim() || (error ? error.message : 'no output')));
          return;
        }
        try {
          const payload = JSON.parse(trimmed);
          const findings: Array<{
            finding_id: string;
            rule_id: string;
            location: { file_path: string; start_line: number };
          }> = payload?.review?.findings ?? [];

          // Match by rule_id and line proximity (within 2 lines to handle minor drift).
          const normalizedFile = filePath.replace(/\\/g, '/');
          const match = findings.find((f) => {
            const fFile = (f.location?.file_path ?? '').replace(/\\/g, '/');
            const sameLine = Math.abs((f.location?.start_line ?? 0) - lineNumber) <= 2;
            const sameRule = f.rule_id === ruleId;
            const sameFile =
              fFile === normalizedFile ||
              normalizedFile.endsWith(fFile) ||
              fFile.endsWith(path.basename(normalizedFile));
            return sameFile && sameLine && sameRule;
          });

          if (!match) {
            output.appendLine(
              `[audit-risk repair] 未找到匹配 finding，可用 finding_ids：${findings.map((f) => f.finding_id).join(', ')}`
            );
            reject(
              new Error(
                `未找到匹配 finding（${path.basename(filePath)}:${lineNumber} rule=${ruleId}）。` +
                  `当前工作区可能没有相关 finding，请先运行 audit-risk check 确认。`
              )
            );
            return;
          }

          resolve(match.finding_id);
        } catch (parseError) {
          reject(new Error(`check 返回了非 JSON 内容：${(parseError as Error).message}`));
        }
      }
    );
  });
}

/**
 * Check whether the workspace has a delivery.json with provider config.
 * Used to show a friendly error before attempting repair if not initialized.
 */
export function hasDeliveryConfig(workspaceRoot: string): boolean {
  const deliveryPath = path.join(workspaceRoot, '.hologram', 'delivery.json');
  return fs.existsSync(deliveryPath);
}
