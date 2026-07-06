import * as assert from 'assert';
import * as fs from 'fs';
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

    const configPyPath = path.join(workspaceRoot!, 'src', 'config.py');
    const configPyUri = vscode.Uri.file(configPyPath);
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

    // Derive the expected line from the actual fixture file instead of a
    // hardcoded line number — a hardcoded constant silently goes stale the
    // moment anyone reformats the fixture (this happened once already: a
    // fixture rewrite dropped a blank line and the hardcoded "line 4"
    // assertion started failing even though the extension's line-number
    // conversion was correct the whole time).
    const configPySource = fs.readFileSync(configPyPath, 'utf8');
    const oneIndexedApiKeyLine = configPySource.split('\n').findIndex((line) => line.includes('api_key')) + 1;
    assert.ok(oneIndexedApiKeyLine > 0, 'fixture must contain an api_key line to anchor this assertion');
    assert.strictEqual(
      secretFinding!.range.start.line,
      oneIndexedApiKeyLine - 1,
      `1-indexed line ${oneIndexedApiKeyLine} in the CLI payload must map to 0-indexed line ${oneIndexedApiKeyLine - 1} in the Range`
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

  test('inline diagnostics attach to the real open editor with an enriched hover message', async () => {
    const workspaceRoot = process.env.AUDIT_RISK_TEST_WORKSPACE;
    const configPyPath = path.join(workspaceRoot!, 'src', 'config.py');
    const configPyUri = vscode.Uri.file(configPyPath);

    await vscode.commands.executeCommand('auditRisk.check');

    // Actually open the document in an editor — this is what step 2 is
    // about: confirming the diagnostic is attached to a live, visible
    // document (which is what makes VS Code render the inline squiggle),
    // not just present in the headless getDiagnostics() query used by the
    // other tests.
    const document = await vscode.workspace.openTextDocument(configPyUri);
    const editor = await vscode.window.showTextDocument(document);
    assert.strictEqual(editor.document.uri.fsPath, configPyPath);

    const diagnostics = vscode.languages.getDiagnostics(configPyUri);
    const secretFinding = diagnostics.find((d) => d.message.includes('API key'));
    assert.ok(secretFinding, 'expected the secret finding to still be present with the editor open');

    // The hover message must carry both the human-readable explanation
    // (unchanged from the CLI) AND the enriched severity/rule_id line added
    // in this step — confirms the tooltip shown when hovering the inline
    // squiggle has real context, not just a bare sentence.
    assert.ok(
      secretFinding!.message.includes('严重') || secretFinding!.message.includes('高危'),
      `enriched message must include a Chinese severity label, got: ${secretFinding!.message}`
    );
    assert.ok(
      secretFinding!.message.includes(secretFinding!.code as string),
      'enriched message must include the rule_id shown alongside the diagnostic code'
    );

    // The diagnostic's range must fall within the document's actual line
    // count — if the range pointed past the end of the file, VS Code would
    // silently fail to render the squiggle at all.
    assert.ok(
      secretFinding!.range.start.line < document.lineCount,
      'diagnostic range must point at a real line within the open document'
    );
  });
});
