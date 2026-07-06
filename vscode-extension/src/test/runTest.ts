import * as path from 'path';
import { runTests } from '@vscode/test-electron';

async function main(): Promise<void> {
  try {
    const extensionDevelopmentPath = path.resolve(__dirname, '../../');
    const extensionTestsPath = path.resolve(__dirname, './suite/index');
    // Point VS Code at a real workspace with planted findings, created by
    // the test suite itself before this runs (see suite/index.ts).
    const testWorkspace = process.env.AUDIT_RISK_TEST_WORKSPACE;
    if (!testWorkspace) {
      throw new Error('AUDIT_RISK_TEST_WORKSPACE env var must point at a workspace directory');
    }

    await runTests({
      extensionDevelopmentPath,
      extensionTestsPath,
      launchArgs: [testWorkspace, '--disable-extensions'],
    });
  } catch (err) {
    console.error('Failed to run tests', err);
    process.exit(1);
  }
}

main();
