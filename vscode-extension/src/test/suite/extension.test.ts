import * as assert from 'assert';
import * as path from 'path';
import * as vscode from 'vscode';

suite('audit-risk extension', () => {
  test('auditRisk.check populates the Problems panel with real findings', async () => {
    const workspaceRoot = process.env.AUDIT_RISK_TEST_WORKSPACE;
    assert.ok(workspaceRoot, 'AUDIT_RISK_TEST_WORKSPACE must be set');

    const extension = vscode.extensions.getExtension('audit-risk.audit-risk');
    assert.ok(extension, 'extension audit-risk.audit-risk must be discoverable');
    await extension!.activate();

    await vscode.commands.executeCommand('auditRisk.check');

    const configPyUri = vscode.Uri.file(path.join(workspaceRoot!, 'src', 'config.py'));
    const migrationUri = vscode.Uri.file(path.join(workspaceRoot!, 'migrations', '0001_init.sql'));

    const configDiagnostics = vscode.languages.getDiagnostics(configPyUri);
    const migrationDiagnostics = vscode.languages.getDiagnostics(migrationUri);

    assert.ok(
      configDiagnostics.length > 0,
      `expected diagnostics on src/config.py, got ${configDiagnostics.length}`
    );
    assert.ok(
      migrationDiagnostics.length > 0,
      `expected diagnostics on migrations/0001_init.sql, got ${migrationDiagnostics.length}`
    );

    const secretFinding = configDiagnostics.find((d) => d.message.includes('API key'));
    assert.ok(secretFinding, 'expected a diagnostic mentioning the planted API key');
    assert.strictEqual(
      secretFinding!.severity,
      vscode.DiagnosticSeverity.Error,
      'a critical secret finding must map to Error severity'
    );
    assert.strictEqual(secretFinding!.source, 'audit-risk');

    // The planted key is on line 4 (1-indexed) of src/config.py — confirm
    // the 1-indexed CLI line number was correctly converted to VS Code's
    // 0-indexed Range instead of being passed through as-is.
    assert.strictEqual(
      secretFinding!.range.start.line,
      3,
      'line 4 (1-indexed) in the CLI payload must map to line 3 (0-indexed) in the Range'
    );
  });

  test('auditRisk.clear empties the diagnostics collection', async () => {
    const workspaceRoot = process.env.AUDIT_RISK_TEST_WORKSPACE;
    const configPyUri = vscode.Uri.file(path.join(workspaceRoot!, 'src', 'config.py'));

    await vscode.commands.executeCommand('auditRisk.check');
    assert.ok(vscode.languages.getDiagnostics(configPyUri).length > 0, 'sanity check: findings exist before clear');

    await vscode.commands.executeCommand('auditRisk.clear');
    assert.strictEqual(
      vscode.languages.getDiagnostics(configPyUri).length,
      0,
      'diagnostics must be empty after auditRisk.clear'
    );
  });
});
