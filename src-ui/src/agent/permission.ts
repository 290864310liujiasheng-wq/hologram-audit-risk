// Permission system — allow/ask/deny per tool call
// Design adapted from Reasonix (internal/permission/permission.go)
// Pure policy evaluation + interactive approve modal + persistent rules

type Decision = 'allow' | 'ask' | 'deny';

interface Rule {
  tool: string;
  subject?: string; // glob pattern matching a tool arg (command, filePath, etc.)
}

interface PolicyData {
  defaultMode: Decision; // fallback for write tools not in any list
  allow: Rule[];
  ask: Rule[];
  deny: Rule[];
}

// ── Glob matching ──

function matchGlob(pattern: string, name: string): boolean {
  let px = 0, nx = 0, starPx = -1, starNx = -1;
  while (nx < name.length) {
    if (px < pattern.length && (pattern[px] === '?' || pattern[px] === name[nx])) {
      px++; nx++;
    } else if (px < pattern.length && pattern[px] === '*') {
      starPx = px; starNx = nx;
      px++;
    } else if (starPx !== -1) {
      px = starPx + 1;
      starNx++;
      nx = starNx;
    } else {
      return false;
    }
  }
  while (px < pattern.length && pattern[px] === '*') px++;
  return px === pattern.length;
}

// ── Subject extraction ──

const subjectKeys = ['command', 'filePath', 'path', 'pattern', 'file_path'];

function extractSubject(args: Record<string, unknown>): string {
  for (const k of subjectKeys) {
    const v = args[k];
    if (typeof v === 'string' && v.length > 0) return v;
  }
  return '';
}

// ── Rule matching ──

function matchAny(rules: Rule[], toolName: string, subject: string): boolean {
  for (const r of rules) {
    if (r.tool !== toolName) continue;
    if (!r.subject) return true;
    if (subject && matchGlob(r.subject, subject)) return true;
  }
  return false;
}

// ── Policy ──

export class PermissionPolicy {
  private data: PolicyData;

  constructor(defaultMode: Decision = 'ask') {
    this.data = {
      defaultMode,
      allow: [],
      ask: [],
      deny: [],
    };
  }

  /** Set rules from a flat config (e.g. from settings) */
  configure(cfg: { allow?: string[]; ask?: string[]; deny?: string[]; defaultMode?: Decision }): void {
    this.data.defaultMode = cfg.defaultMode || 'ask';
    this.data.allow = parseRules(cfg.allow || []);
    this.data.ask = parseRules(cfg.ask || []);
    this.data.deny = parseRules(cfg.deny || []);
  }

  /** Add a remembered allow rule */
  rememberAllow(toolName: string, subject: string): void {
    const rule: Rule = subject ? { tool: toolName, subject } : { tool: toolName };
    // Remove from ask/deny first, then add to allow
    this.data.ask = this.data.ask.filter(r => !isSameRule(r, rule));
    this.data.deny = this.data.deny.filter(r => !isSameRule(r, rule));
    this.data.allow.push(rule);
  }

  /** Export rules for persistence */
  exportRules(): { allow: string[]; deny: string[] } {
    return {
      allow: this.data.allow.map(r => r.subject ? `${r.tool}(${r.subject})` : r.tool),
      deny: this.data.deny.map(r => r.subject ? `${r.tool}(${r.subject})` : r.tool),
    };
  }

  /** Load rules from persisted format */
  importRules(rules: { allow?: string[]; deny?: string[] }): void {
    this.data.allow = parseRules(rules.allow || []);
    this.data.deny = parseRules(rules.deny || []);
  }

  decide(toolName: string, readOnly: boolean, args: Record<string, unknown>): Decision {
    const subject = extractSubject(args);
    if (matchAny(this.data.deny, toolName, subject)) return 'deny';
    if (matchAny(this.data.ask, toolName, subject)) return 'ask';
    if (matchAny(this.data.allow, toolName, subject)) return 'allow';
    if (readOnly) return 'allow';
    return this.data.defaultMode;
  }
}

function parseRules(strings: string[]): Rule[] {
  const rules: Rule[] = [];
  for (const s of strings) {
    const trimmed = s.trim();
    if (!trimmed) continue;
    const i = trimmed.indexOf('(');
    if (i >= 0 && trimmed.endsWith(')')) {
      const tool = trimmed.slice(0, i).trim();
      if (tool) {
        rules.push({ tool, subject: trimmed.slice(i + 1, -1) });
      }
    } else {
      rules.push({ tool: trimmed });
    }
  }
  return rules;
}

function isSameRule(a: Rule, b: Rule): boolean {
  return a.tool === b.tool && (a.subject || '') === (b.subject || '');
}

// ── Gate (Policy + Approver) ──

export type ApproveCallback = (
  toolName: string,
  description: string,
  args: Record<string, unknown>,
) => Promise<{ allow: boolean; remember: boolean }>;

export class PermissionGate {
  policy: PermissionPolicy;
  private approve: ApproveCallback | null;
  onRemember: ((rule: string) => void) | null = null;

  constructor(policy: PermissionPolicy, approve?: ApproveCallback) {
    this.policy = policy;
    this.approve = approve || null;
  }

  setApprover(fn: ApproveCallback): void { this.approve = fn; }

  async check(
    toolName: string,
    toolDescription: string,
    args: Record<string, unknown>,
    readOnly: boolean,
  ): Promise<{ allow: boolean; reason?: string }> {
    const decision = this.policy.decide(toolName, readOnly, args);

    switch (decision) {
      case 'deny':
        return {
          allow: false,
          reason: '此工具被权限策略拒绝。请选择其他方式。',
        };
      case 'ask':
        if (!this.approve) return { allow: true }; // non-interactive: allow
        const result = await this.approve(toolName, toolDescription, args);
        if (!result.allow) {
          return {
            allow: false,
            reason: '用户拒绝了此工具调用。请选择其他方式或询问用户。',
          };
        }
        if (result.remember && this.onRemember) {
          const subject = extractSubject(args);
          const rule = subject ? `${toolName}(${subject})` : toolName;
          this.onRemember(rule);
        }
        return { allow: true };
      default: // allow
        return { allow: true };
    }
  }
}

// ── Interactive approval modal ──

export function showApprovalDialog(
  toolName: string,
  description: string,
  args: Record<string, unknown>,
): Promise<{ allow: boolean; remember: boolean }> {
  return new Promise((resolve) => {
    const overlay = document.createElement('div');
    Object.assign(overlay.style, {
      position: 'fixed', top: '0', left: '0', right: '0', bottom: '0',
      background: 'rgba(0,0,0,0.65)', zIndex: '9998',
      display: 'flex', alignItems: 'center', justifyContent: 'center',
    });
    const dialog = document.createElement('div');
    Object.assign(dialog.style, {
      background: 'var(--panel-bg, rgba(6,12,24,0.97))',
      border: '1px solid var(--panel-edge, rgba(54,82,128,0.35))',
      borderRadius: '12px', padding: '20px 24px', maxWidth: '480px', minWidth: '340px',
      color: 'var(--starlight, #e2edff)',
      fontFamily: 'var(--font-mono, monospace)',
      boxShadow: '0 16px 64px rgba(0,0,0,0.5)',
    });

    const header = document.createElement('div');
    header.innerHTML = `⚠️ <strong style="color:var(--sol, #f0b848)">${toolName}</strong> 请求执行`;
    Object.assign(header.style, { fontSize: '14px', marginBottom: '8px' });

    const desc = document.createElement('div');
    desc.textContent = description;
    Object.assign(desc.style, { fontSize: '12px', color: 'var(--starlight-dim, #c9d1d9)', marginBottom: '12px' });

    const argsPre = document.createElement('pre');
    argsPre.textContent = JSON.stringify(args, null, 2).slice(0, 300);
    Object.assign(argsPre.style, {
      fontSize: '11px', color: 'var(--text-muted, #6b7d90)',
      background: 'rgba(0,0,0,0.3)', padding: '8px', borderRadius: '6px',
      maxHeight: '120px', overflow: 'auto', marginBottom: '16px',
    });

    const btnRow = document.createElement('div');
    Object.assign(btnRow.style, { display: 'flex', gap: '8px' });

    const makeBtn = (text: string, bg: string, color: string) => {
      const btn = document.createElement('button');
      btn.textContent = text;
      Object.assign(btn.style, {
        flex: '1', padding: '8px', fontSize: '13px', fontWeight: '600',
        background: bg, color, border: 'none', borderRadius: '6px', cursor: 'pointer',
      });
      return btn;
    };

    const alwaysBtn = makeBtn('始终允许', 'rgba(61,165,93,0.2)', 'var(--pass, #3da55d)');
    const onceBtn = makeBtn('这次允许', 'rgba(80,140,240,0.15)', 'var(--signal, #68a8ff)');
    const denyBtn = makeBtn('拒绝', 'rgba(217,68,68,0.15)', 'var(--fail, #d94444)');

    alwaysBtn.addEventListener('click', () => { overlay.remove(); resolve({ allow: true, remember: true }); });
    onceBtn.addEventListener('click', () => { overlay.remove(); resolve({ allow: true, remember: false }); });
    denyBtn.addEventListener('click', () => { overlay.remove(); resolve({ allow: false, remember: false }); });

    btnRow.appendChild(alwaysBtn);
    btnRow.appendChild(onceBtn);
    btnRow.appendChild(denyBtn);

    dialog.appendChild(header);
    dialog.appendChild(desc);
    dialog.appendChild(argsPre);
    dialog.appendChild(btnRow);
    overlay.appendChild(dialog);

    overlay.addEventListener('click', (e) => {
      if (e.target === overlay) { overlay.remove(); resolve({ allow: false, remember: false }); }
    });
    document.addEventListener('keydown', function escHandler(e) {
      if (e.key === 'Escape') { overlay.remove(); resolve({ allow: false, remember: false }); document.removeEventListener('keydown', escHandler); }
    });
    document.body.appendChild(overlay);
  });
}
