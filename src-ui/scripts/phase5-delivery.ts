import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

import {
  buildDeliveryDoctorReport,
  buildDeliveryInitFiles,
  buildDeliveryMachineReport,
  buildDeliveryRuleSummaries,
  createDefaultDeliveryConfig,
  searchDeliveryAuditRecords,
  type DeliveryConfig,
} from '../src/risk/delivery';
import type { RecentAuditEntry } from '../src/risk/audit-bridge';
import type { GateDecisionValue } from '../src/risk/review-core';

interface CommandReport {
  command: string;
  cwd: string;
  exit_code: number | null;
  passed: boolean;
  stdout_tail: string[];
  stderr_tail: string[];
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const uiRoot = resolve(scriptDir, '..');
const repoRoot = resolve(uiRoot, '..');
const engineRoot = resolve(repoRoot, 'engine');
const tauriRoot = resolve(repoRoot, 'src-tauri');
const phase5EvidencePath = resolve(repoRoot, 'dev-docs', 'evidence', 'phase5-delivery.json');

async function main(): Promise<void> {
  const [command = 'verify', ...rest] = process.argv.slice(2);
  const args = parseArgs(rest);

  if (command === 'init') {
    runInitCommand(args);
    return;
  }

  if (command === 'report') {
    runReportCommand(args);
    return;
  }

  if (command === 'verify') {
    runVerifyCommand();
    return;
  }

  if (command === 'rules') {
    runRulesCommand(args);
    return;
  }

  if (command === 'audit') {
    runAuditCommand(args);
    return;
  }

  if (command === 'doctor') {
    runDoctorCommand(args);
    return;
  }

  throw new Error(`Unknown phase5-delivery command: ${command}`);
}

function runInitCommand(args: Record<string, string | boolean>): void {
  const workspaceRoot = resolve(String(args.workspace || repoRoot));
  const files = buildDeliveryInitFiles({
    workspaceRoot,
    platformRoot: repoRoot,
  });
  const force = Boolean(args.force);

  for (const [relativePath, content] of Object.entries(files)) {
    const absolutePath = resolve(workspaceRoot, relativePath);
    if (existsSync(absolutePath) && !force) {
      throw new Error(`Refusing to overwrite existing file without --force: ${absolutePath}`);
    }
    mkdirSync(dirname(absolutePath), { recursive: true });
    writeFileSync(absolutePath, content, 'utf8');
    if (relativePath === '.githooks/pre-commit') {
      chmodSync(absolutePath, 0o755);
    }
  }

  console.log(JSON.stringify({
    workspace_root: workspaceRoot,
    created_files: Object.keys(files),
  }, null, 2));
}

function runReportCommand(args: Record<string, string | boolean>): void {
  const workspaceRoot = resolve(String(args.workspace || repoRoot));
  const config = loadDeliveryConfig(workspaceRoot, maybeString(args.config));
  if (args.failOn && typeof args.failOn === 'string' && args.failOn !== 'off') {
    config.automation.fail_on_decision = args.failOn as GateDecisionValue;
  }
  const outputPath = resolve(workspaceRoot, String(args.output || config.audit.report_output_path));
  const report = generateMachineReport(workspaceRoot, config);

  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, JSON.stringify(report, null, 2), 'utf8');
  console.log(`phase5 delivery report written to ${outputPath}`);

  if (args.failOn !== 'off' && report.automation.should_fail) {
    process.exitCode = 2;
  }
}

function runVerifyCommand(): void {
  const generatedAt = new Date().toISOString();
  const commands = [
    run('node --import tsx src/risk/test-risk.ts', uiRoot),
    run('npx tsc --noEmit', uiRoot),
    run('npm run build', uiRoot),
    run('cargo test --manifest-path ../engine/Cargo.toml --bin audit-risk -- --nocapture', uiRoot),
    run('cargo check', tauriRoot),
  ];

  commands.forEach(mustPass);

  const initWorkspace = mkdtempSync(resolve(tmpdir(), 'hologram-phase5-init-'));
  const initFiles = buildDeliveryInitFiles({
    workspaceRoot: initWorkspace,
    platformRoot: repoRoot,
  });
  for (const [relativePath, content] of Object.entries(initFiles)) {
    const absolutePath = resolve(initWorkspace, relativePath);
    mkdirSync(dirname(absolutePath), { recursive: true });
    writeFileSync(absolutePath, content, 'utf8');
  }
  const externalWorkspace = createExternalSmokeWorkspace();
  const externalInit = run(
    `cargo run --quiet --manifest-path "${resolve(engineRoot, 'Cargo.toml')}" --bin audit-risk -- init "${externalWorkspace}" --force`,
    uiRoot,
  );
  mustPass(externalInit);
  const externalReportPath = resolve(externalWorkspace, '.hologram', 'latest-risk-report.json');
  const externalReport = run(
    `cargo run --quiet --manifest-path "${resolve(engineRoot, 'Cargo.toml')}" --bin audit-risk -- report "${externalWorkspace}" --config "${resolve(externalWorkspace, '.hologram/delivery.json')}" --output "${externalReportPath}" --fail-on off`,
    uiRoot,
  );
  mustPass(externalReport);
  const externalHook = run(
    `HOLOGRAM_PLATFORM_ROOT="${repoRoot}" sh "${resolve(externalWorkspace, '.githooks/pre-commit')}" "${externalWorkspace}"`,
    externalWorkspace,
  );
  mustPass(externalHook);
  const externalRules = run(
    `cargo run --quiet --manifest-path "${resolve(engineRoot, 'Cargo.toml')}" --bin audit-risk -- rules "${externalWorkspace}" --config "${resolve(externalWorkspace, '.hologram/delivery.json')}"`,
    uiRoot,
  );
  mustPass(externalRules);
  const externalAudit = run(
    `cargo run --quiet --manifest-path "${resolve(engineRoot, 'Cargo.toml')}" --bin audit-risk -- audit "${externalWorkspace}" --config "${resolve(externalWorkspace, '.hologram/delivery.json')}" --query review --limit 5`,
    uiRoot,
  );
  mustPass(externalAudit);
  const externalDoctor = run(
    `cargo run --quiet --manifest-path "${resolve(engineRoot, 'Cargo.toml')}" --bin audit-risk -- doctor "${externalWorkspace}"`,
    uiRoot,
  );
  mustPass(externalDoctor);
  const externalReportJson = JSON.parse(readFileSync(externalReportPath, 'utf8')) as {
    current_review?: {
      status?: string;
      review?: {
        gate_decision?: {
          decision?: string;
        };
      };
    };
  };

  const report = generateMachineReport(repoRoot, createDefaultDeliveryConfig(repoRoot));

  const gitBranch = run('git branch --show-current', repoRoot);
  const lastCommit = run('git log -1 --oneline', repoRoot);
  const gitStatus = run('git status --short', repoRoot);

  mkdirSync(dirname(phase5EvidencePath), { recursive: true });
  writeFileSync(phase5EvidencePath, JSON.stringify({
    generated_at: generatedAt,
    repo_root: repoRoot,
    ui_root: uiRoot,
    tauri_root: tauriRoot,
    engine_root: engineRoot,
    git: {
      branch: gitBranch.stdout_tail.at(-1) || '',
      last_commit: lastCommit.stdout_tail.at(-1) || '',
      status_short: gitStatus.stdout_tail,
    },
    commands,
    init_smoke: {
      workspace_root: initWorkspace,
      created_files: Object.keys(initFiles),
    },
    external_workspace_smoke: {
      workspace_root: externalWorkspace,
      init_command: externalInit.command,
      report_command: externalReport.command,
      hook_command: externalHook.command,
      rules_command: externalRules.command,
      audit_command: externalAudit.command,
      doctor_command: externalDoctor.command,
      report_output_path: externalReportPath,
      gate_decision: externalReportJson.current_review?.status === 'ok'
        ? (externalReportJson.current_review.review?.gate_decision?.decision || 'unknown')
        : 'empty',
    },
    machine_report: report,
  }, null, 2), 'utf8');

  console.log(`phase5 verification written to ${phase5EvidencePath}`);
}

function runRulesCommand(args: Record<string, string | boolean>): void {
  const workspaceRoot = resolve(String(args.workspace || repoRoot));
  const config = loadDeliveryConfig(workspaceRoot, maybeString(args.config));
  const report = generateMachineReport(workspaceRoot, config);
  console.log(JSON.stringify(buildDeliveryRuleSummaries({ policies: report.policies }), null, 2));
}

function runAuditCommand(args: Record<string, string | boolean>): void {
  const workspaceRoot = resolve(String(args.workspace || repoRoot));
  const config = loadDeliveryConfig(workspaceRoot, maybeString(args.config));
  const report = generateMachineReport(workspaceRoot, config);
  const query = maybeString(args.query) || '';
  const limit = maybeNumber(args.limit) || 20;
  console.log(JSON.stringify(searchDeliveryAuditRecords({
    audit: report.audit,
    query,
    limit,
  }), null, 2));
}

function runDoctorCommand(args: Record<string, string | boolean>): void {
  const workspaceRoot = resolve(String(args.workspace || repoRoot));
  const config = loadDeliveryConfig(workspaceRoot, maybeString(args.config));
  const report = generateMachineReport(workspaceRoot, config);
  console.log(JSON.stringify(buildDeliveryDoctorReport({ report }), null, 2));
}

function generateMachineReport(workspaceRoot: string, config: DeliveryConfig) {
  const deliveryCheck = run(
    `cargo run --quiet --manifest-path "${resolve(engineRoot, 'Cargo.toml')}" --bin audit-risk -- check "${workspaceRoot}" --json`,
    repoRoot,
  );
  if (!deliveryCheck.passed && deliveryCheck.exit_code !== 2) {
    mustPass(deliveryCheck);
  }
  const payload = JSON.parse(deliveryCheck.stdout_tail.join('\n') || '{}') as {
    review?: {
      raw_check?: unknown;
    };
    check?: unknown;
  };
  const checkResult = payload.review?.raw_check || payload.check;
  if (!checkResult || typeof checkResult !== 'object') {
    throw new Error('Headless delivery check did not return a check result.');
  }

  const auditEntries = readAuditEntries(resolve(workspaceRoot, config.audit.jsonl_path), config.audit.recent_limit);
  return buildDeliveryMachineReport({
    config,
    checkResult: checkResult as any,
    auditEntries,
    generatedAt: new Date().toISOString(),
    env: process.env,
    readFile: (path) => readFileSync(path, 'utf8'),
  });
}

function loadDeliveryConfig(workspaceRoot: string, configPath?: string): DeliveryConfig {
  if (!configPath) {
    return createDefaultDeliveryConfig(workspaceRoot);
  }
  const absolutePath = resolve(configPath);
  return JSON.parse(readFileSync(absolutePath, 'utf8')) as DeliveryConfig;
}

function readAuditEntries(path: string, limit: number): RecentAuditEntry[] {
  if (!existsSync(path)) {
    return [];
  }

  return readFileSync(path, 'utf8')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(-limit)
    .map((line) => ({
      ...(JSON.parse(line) as RecentAuditEntry),
      raw_line: line,
    }));
}

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
    stdout_tail: tailLines(result.stdout || '', 120),
    stderr_tail: tailLines(result.stderr || '', 40),
  };
}

function mustPass(report: CommandReport): void {
  if (!report.passed) {
    throw new Error(`${report.command} failed with exit code ${report.exit_code}`);
  }
}

function tailLines(text: string, limit = 40): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter((line) => line.length > 0)
    .slice(-limit);
}

function parseArgs(args: string[]): Record<string, string | boolean> {
  const output: Record<string, string | boolean> = {};
  for (let index = 0; index < args.length; index += 1) {
    const current = args[index];
    if (!current.startsWith('--')) continue;
    const key = current.slice(2).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());
    const next = args[index + 1];
    if (!next || next.startsWith('--')) {
      output[key] = true;
      continue;
    }
    output[key] = next;
    index += 1;
  }
  return output;
}

function maybeString(value: string | boolean | undefined): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function maybeNumber(value: string | boolean | undefined): number | undefined {
  if (typeof value !== 'string') return undefined;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
}

function createExternalSmokeWorkspace(): string {
  const workspaceRoot = mkdtempSync(resolve(tmpdir(), 'hologram-phase5-external-'));
  mkdirSync(resolve(workspaceRoot, 'src'), { recursive: true });
  writeFileSync(resolve(workspaceRoot, 'src/index.ts'), 'export const smoke = false;\n', 'utf8');
  mustPass(run('git init -q', workspaceRoot));
  mustPass(run('git config user.email phase5@example.com', workspaceRoot));
  mustPass(run('git config user.name "Phase5 Verify"', workspaceRoot));
  mustPass(run('git add .', workspaceRoot));
  mustPass(run('git commit -q -m "baseline"', workspaceRoot));
  writeFileSync(resolve(workspaceRoot, 'src/index.ts'), 'export const smoke = true;\n', 'utf8');
  return workspaceRoot;
}

await main();
