import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';

interface CommandReport {
  command: string;
  cwd: string;
  exit_code: number | null;
  passed: boolean;
  stdout_tail: string[];
  stderr_tail: string[];
}

interface PreviewSmokeReport {
  url: string;
  passed: boolean;
  status_code?: number;
  title?: string;
  body_markers: string[];
  stdout_tail: string[];
  stderr_tail: string[];
  note?: string;
}

interface ManualUiCheck {
  required: boolean;
  command: string;
  url: string;
  expected_markers: string[];
  note: string;
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const uiRoot = resolve(scriptDir, '..');
const repoRoot = resolve(uiRoot, '..');
const tauriRoot = resolve(repoRoot, 'src-tauri');
const evidenceDir = resolve(repoRoot, 'dev-docs', 'evidence');
const outputPath = resolve(evidenceDir, 'phase4-verify.json');

function run(command: string, cwd: string): CommandReport {
  const result = spawnSync(command, {
    cwd,
    shell: true,
    encoding: 'utf8',
  });

  return {
    command,
    cwd,
    exit_code: result.status,
    passed: result.status === 0,
    stdout_tail: tailLines(result.stdout || ''),
    stderr_tail: tailLines(result.stderr || ''),
  };
}

function tailLines(text: string, limit = 30): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter((line) => line.length > 0)
    .slice(-limit);
}

function mustPass(report: CommandReport): void {
  if (!report.passed) {
    throw new Error(`${report.command} failed with exit code ${report.exit_code}`);
  }
}

async function runPreviewSmoke(): Promise<PreviewSmokeReport> {
  const port = 4174;
  const url = `http://127.0.0.1:${port}/`;
  const child = spawn('npm', ['run', 'preview', '--', '--host', '127.0.0.1', '--port', String(port)], {
    cwd: uiRoot,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: process.env,
  });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (chunk) => { stdout += String(chunk); });
  child.stderr.on('data', (chunk) => { stderr += String(chunk); });

  try {
    const started = await waitForHttp(url, 15000);
    if (!started.ok) {
      return {
        url,
        passed: false,
        body_markers: [],
        stdout_tail: tailLines(stdout),
        stderr_tail: tailLines(stderr),
        note: started.message,
      };
    }

    const response = await fetch(url);
    const html = await response.text();
    return {
      url,
      passed: response.ok,
      status_code: response.status,
      title: html.match(/<title>(.*?)<\/title>/i)?.[1] || '',
      body_markers: [
        html.includes('id="app"') ? 'app-root' : '',
        html.includes('/assets/index-') ? 'built-assets' : '',
      ].filter(Boolean),
      stdout_tail: tailLines(stdout),
      stderr_tail: tailLines(stderr),
      note: response.ok ? 'Preview server served the built UI bundle.' : 'Preview server responded but did not return HTTP 200.',
    };
  } finally {
    child.kill('SIGTERM');
  }
}

async function waitForHttp(url: string, timeoutMs: number): Promise<{ ok: true } | { ok: false; message: string }> {
  const deadline = Date.now() + timeoutMs;
  let lastError = 'Preview server did not start.';

  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.status >= 200) {
        return { ok: true };
      }
      lastError = `Preview server responded with ${response.status}`;
    } catch (error) {
      lastError = String(error);
    }
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 300));
  }

  return { ok: false, message: lastError };
}

async function main(): Promise<void> {
  const generatedAt = new Date().toISOString();
  const branch = run('git branch --show-current', repoRoot);
  const lastCommit = run('git log -1 --oneline', repoRoot);
  const gitStatus = run('git status --short', repoRoot);
  const commands = [
    run('node --import tsx src/risk/test-risk.ts', uiRoot),
    run('npx tsc --noEmit', uiRoot),
    run('npm run build', uiRoot),
    run('cargo check', tauriRoot),
  ];

  commands.forEach(mustPass);
  const preview_smoke = await runPreviewSmoke();
  const manual_ui_check: ManualUiCheck = {
    required: !preview_smoke.passed,
    command: 'npm run dev -- --host 127.0.0.1 --port 4173',
    url: 'http://127.0.0.1:4173/',
    expected_markers: [
      'Review Queue',
      '门禁决策',
      '多代理审计',
      '自修复闭环',
      '看证据 · 已就绪',
      'Repair patch applied.',
    ],
    note: preview_smoke.passed
      ? 'Preview smoke passed; manual localhost UI check is optional.'
      : 'Preview smoke is blocked by the current environment. Start a local dev server and verify the listed markers in Chrome as the fallback UI acceptance path.',
  };

  mkdirSync(evidenceDir, { recursive: true });
  writeFileSync(outputPath, JSON.stringify({
    generated_at: generatedAt,
    repo_root: repoRoot,
    ui_root: uiRoot,
    tauri_root: tauriRoot,
    git: {
      branch: branch.stdout_tail.at(-1) || '',
      last_commit: lastCommit.stdout_tail.at(-1) || '',
      status_short: gitStatus.stdout_tail,
    },
    commands,
    preview_smoke,
    manual_ui_check,
  }, null, 2));

  console.log(`phase4 verification written to ${outputPath}`);
}

await main();
