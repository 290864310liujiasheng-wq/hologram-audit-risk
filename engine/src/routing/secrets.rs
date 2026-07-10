/// Secret scanning engine.
///
/// Six detection layers (applied to each line of each changed file):
///
/// 1. **Known prefix patterns** — fixed patterns like `sk-`, `AKIA`, `ghp_`.
///    Very low false-positive rate; catches the most dangerous leaks.
///
/// 2. **High-entropy string detection** — Shannon entropy > 4.5 on strings
///    longer than 20 chars that look like keys (base64/hex charset).
///    Catches custom secrets without fixed prefixes.
///
/// 3. **Hardcoded assignment patterns** — variable names like `api_key`,
///    `secret`, `password` assigned string literals.
///    Catches secrets that look "innocent" in isolation.
///
/// 4. **SQL injection via string building** — a SQL keyword inside an
///    f-string/template-literal interpolation, string concatenation, Python
///    `%` formatting, or `str.format()`, instead of a parameterized query
///    placeholder passed separately to the driver.
///
/// 5. **Dangerous dynamic execution** — eval/exec/os.system/shell=True/
///    child_process.exec/new Function(...) — code that runs a string as
///    code or a shell command.
///
/// 6. **Overly permissive IAM/policy statements** — `"Action": "*"` or
///    `"Resource": "*"` in policy-shaped JSON, the single most common
///    over-privilege pattern AI assistants generate when asked for "a
///    policy that lets this service read from S3" and reach for a
///    wildcard instead of scoping it down.
///
/// Output signals use level 5 (critical) for definite known-prefix hits,
/// level 4 (high) for entropy, assignment, SQL injection, dangerous
/// execution, and permissive IAM hits.
use regex::Regex;
use serde_json::{json, Value};

/// A single secret detection finding.
#[derive(Debug, Clone)]
pub struct SecretFinding {
    pub file_path: String,
    pub line: usize,
    pub kind: SecretKind,
    pub matched_text: String,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretKind {
    KnownPrefix(&'static str),
    HighEntropy,
    HardcodedAssignment,
    SqlInjection,
    DangerousExecution(&'static str),
    PermissiveIam(&'static str),
}

pub struct SecretScanner {
    /// Each tuple: (display label, regex matching the secret value)
    known_prefixes: Vec<(&'static str, Regex)>,
    /// Matches: variable_name = "some_value" or variable_name: "some_value"
    assignment_pattern: Regex,
    /// SQL keyword + string-building patterns (f-string/template literal
    /// interpolation, or `+` concatenation) instead of a parameterized
    /// placeholder passed separately to the DB driver.
    sql_injection_patterns: Vec<Regex>,
    /// Dangerous dynamic execution: (display label, regex)
    dangerous_execution_patterns: Vec<(&'static str, Regex)>,
    /// Namespace aliases bound to Node's `child_process` module.
    child_process_alias_patterns: Vec<Regex>,
    /// Receiver and direct-require forms of child-process exec calls.
    child_process_method_pattern: Regex,
    child_process_require_exec_pattern: Regex,
    /// Overly permissive IAM/policy statement patterns: (display label, regex)
    permissive_iam_patterns: Vec<(&'static str, Regex)>,
}

impl Default for SecretScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretScanner {
    pub fn new() -> Self {
        let known_prefixes = vec![
            // OpenAI / Anthropic / generic `sk-` keys
            ("OpenAI/Anthropic API key", Regex::new(r#"(?i)(["'\s]|^)(sk-[a-zA-Z0-9\-_]{20,})"#).unwrap()),
            // AWS Access Key ID
            ("AWS access key", Regex::new(r#"(AKIA|ASIA|AROA|AIDA)[A-Z0-9]{16}"#).unwrap()),
            // AWS Secret Access Key
            ("AWS secret key", Regex::new(r#"(?i)(aws_secret|aws_secret_access_key)\s*[=:]\s*["']?([A-Za-z0-9+/]{40})["']?"#).unwrap()),
            // GitHub Personal Access Token
            ("GitHub PAT", Regex::new(r#"(ghp_|gho_|ghu_|ghs_|ghr_)[A-Za-z0-9]{36,}"#).unwrap()),
            // Google API key
            ("Google API key", Regex::new(r#"AIza[0-9A-Za-z\-_]{35}"#).unwrap()),
            // Stripe secret key
            ("Stripe secret key", Regex::new(r#"(sk_live_|rk_live_)[0-9a-zA-Z]{24,}"#).unwrap()),
            // Stripe publishable key (lower severity but still a leak)
            ("Stripe publishable key", Regex::new(r#"pk_live_[0-9a-zA-Z]{24,}"#).unwrap()),
            // Slack token
            ("Slack token", Regex::new(r#"xox[baprs]-[0-9]{10,13}-[0-9]{10,13}-[a-zA-Z0-9]{24,}"#).unwrap()),
            // Slack webhook
            ("Slack webhook", Regex::new(r#"https://hooks\.slack\.com/services/T[A-Z0-9]+/B[A-Z0-9]+/[a-zA-Z0-9]+"#).unwrap()),
            // JWT (base64url header.payload.signature — only if all three parts present)
            ("JWT token", Regex::new(r#"eyJ[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+\.[A-Za-z0-9_\-]+"#).unwrap()),
            // Private key block
            ("private key block", Regex::new(r#"-----BEGIN (RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----"#).unwrap()),
            // Generic bearer token pattern
            ("Bearer token", Regex::new(r#"(?i)(authorization|bearer)["\s:=]+['"](Bearer\s+)?[A-Za-z0-9\-._~+/]{40,}['"]"#).unwrap()),
            // Anthropic-style keys
            ("Anthropic API key", Regex::new(r#"sk-ant-[a-zA-Z0-9\-_]{20,}"#).unwrap()),
            // DeepSeek API key
            ("DeepSeek API key", Regex::new(r#"sk-[a-zA-Z0-9]{32,}"#).unwrap()),
        ];

        let assignment_pattern = Regex::new(
            r#"(?i)(?P<name>api_?key|api_?secret|app_?secret|auth_?token|access_?token|secret_?key|private_?key|client_?secret|db_?pass(?:word)?|database_?pass(?:word)?|password|passwd|credentials?|auth_?key)\s*[=:]\s*(?:"(?P<double>[^"\r\n]{8,})"|'(?P<single>[^'\r\n]{8,})')"#,
        )
        .unwrap();

        let sql_statement_for = |span: &str| {
            format!(
                r"(?i)\b(?:SELECT\b{span}{{0,200}}\bFROM\b|INSERT\b{span}{{0,100}}\bINTO\b|UPDATE\b{span}{{0,100}}\bSET\b|DELETE\b{span}{{0,100}}\bFROM\b|DROP\b{span}{{0,50}}\b(?:TABLE|DATABASE|INDEX)\b|ALTER\b{span}{{0,50}}\b(?:TABLE|DATABASE|INDEX)\b)"
            )
        };
        let sql_double_statement = sql_statement_for(r#"[^"\n]"#);
        let sql_single_statement = sql_statement_for(r"[^'\n]");
        let sql_backtick_statement = sql_statement_for(r"[^`\n]");
        let sql_injection_patterns = vec![
            // Python f-string / JS template literal: SQL keyword appears
            // before a `{...}`/`${...}` interpolation, inside the same
            // quoted/backtick span.
            Regex::new(&format!(r#"f"[^"\n]*{sql_double_statement}[^"\n]*\{{[^}}]+\}}[^"\n]*""#)).unwrap(),
            Regex::new(&format!(r"f'[^'\n]*{sql_single_statement}[^'\n]*\{{[^}}]+\}}[^'\n]*'")).unwrap(),
            Regex::new(&format!(r#"`[^`\n]*{sql_backtick_statement}[^`\n]*\$\{{[^}}]+\}}[^`\n]*`"#)).unwrap(),
            // String concatenation: a quoted string containing a SQL keyword,
            // joined with `+` and an identifier (either order). Split by quote
            // char so a SQL string that itself contains the *other* quote
            // (e.g. "... WHERE name = '" + userName) still matches — a single
            // `[^"'\n]` class would stop at the inner quote and miss it.
            Regex::new(&format!(r#""[^"\n]*{sql_double_statement}[^"\n]*"\s*\+\s*[A-Za-z_][A-Za-z0-9_.]*"#)).unwrap(),
            Regex::new(&format!(r#"'[^'\n]*{sql_single_statement}[^'\n]*'\s*\+\s*[A-Za-z_][A-Za-z0-9_.]*"#)).unwrap(),
            Regex::new(&format!(r#"[A-Za-z_][A-Za-z0-9_.]*\s*\+\s*"[^"\n]*{sql_double_statement}[^"\n]*""#)).unwrap(),
            Regex::new(&format!(r#"[A-Za-z_][A-Za-z0-9_.]*\s*\+\s*'[^'\n]*{sql_single_statement}[^'\n]*'"#)).unwrap(),
            // Python `%` string formatting: a quoted string containing a SQL
            // keyword, then the `%` operator and a variable/tuple —
            // `"... SELECT ..." % user`. The parameterized form
            // `execute("... %s", (user,))` has a `,` (not `%`) after the closing
            // quote, so it does NOT match — `%s` lives *inside* the string.
            Regex::new(&format!(r#""[^"\n]*{sql_double_statement}[^"\n]*"\s*%\s*[A-Za-z_(]"#)).unwrap(),
            Regex::new(&format!(r#"'[^'\n]*{sql_single_statement}[^'\n]*'\s*%\s*[A-Za-z_(]"#)).unwrap(),
            // str.format(): a quoted string containing a SQL keyword, then
            // `.format(` — `"... SELECT ...".format(user)`.
            Regex::new(&format!(r#""[^"\n]*{sql_double_statement}[^"\n]*"\s*\.\s*format\s*\("#)).unwrap(),
            Regex::new(&format!(r#"'[^'\n]*{sql_single_statement}[^'\n]*'\s*\.\s*format\s*\("#)).unwrap(),
        ];

        let dangerous_execution_patterns = vec![
            // Bare eval(...) only. Method calls such as model.eval() are
            // ordinary API calls, not dynamic code evaluation. Rust's regex
            // engine has no lookbehind, so require a non-dot/non-word prefix.
            ("eval()", Regex::new(r"(^|[^.\w])eval\s*\(").unwrap()),
            // Bare Python-style exec(...) — NOT preceded by a `.`, so this
            // does not double-match a method call like `child_process.exec(`.
            // The regex crate has no lookbehind, so instead of "not preceded
            // by a dot" we require the char right before `exec` to be
            // anything OTHER than `.` (or be the start of the line).
            ("exec()", Regex::new(r"(^|[^.\w])exec\s*\(").unwrap()),
            ("os.system()", Regex::new(r"\bos\.system\s*\(").unwrap()),
            ("shell=True", Regex::new(r"\bshell\s*=\s*True\b").unwrap()),
            // Bare `execSync(` / `spawnSync(` from a destructured import
            // (`import { execSync } from 'child_process'`) — no leading dot, so
            // the dotted pattern above misses it.
            ("execSync()", Regex::new(r"(^|[^.\w])(exec|spawn)Sync\s*\(").unwrap()),
            ("new Function() from string", Regex::new(r"\bnew\s+Function\s*\(").unwrap()),
            ("__import__()", Regex::new(r"__import__\s*\(").unwrap()),
        ];

        let child_process_alias_patterns = vec![
            Regex::new(
                r#"\b(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*require\s*\(\s*['"]child_process['"]\s*\)"#,
            )
            .unwrap(),
            Regex::new(
                r#"\bimport\s+\*\s+as\s+([A-Za-z_$][A-Za-z0-9_$]*)\s+from\s+['"]child_process['"]"#,
            )
            .unwrap(),
        ];
        let child_process_method_pattern = Regex::new(
            r"\b([A-Za-z_$][A-Za-z0-9_$]*)\s*\.\s*exec(?:Sync)?\s*\(",
        )
        .unwrap();
        let child_process_require_exec_pattern = Regex::new(
            r#"\brequire\s*\(\s*['"]child_process['"]\s*\)\s*\.\s*exec(?:Sync)?\s*\("#,
        )
        .unwrap();

        let permissive_iam_patterns = vec![
            (
                "IAM Action 通配符",
                Regex::new(r#""Action"\s*:\s*(\[\s*)?"\*""#).unwrap(),
            ),
            (
                "IAM Resource 通配符",
                Regex::new(r#""Resource"\s*:\s*(\[\s*)?"\*""#).unwrap(),
            ),
        ];

        Self {
            known_prefixes,
            assignment_pattern,
            sql_injection_patterns,
            dangerous_execution_patterns,
            child_process_alias_patterns,
            child_process_method_pattern,
            child_process_require_exec_pattern,
            permissive_iam_patterns,
        }
    }

    /// Scan the contents of a single file.
    /// Returns all findings in line order.
    pub fn scan_content(&self, file_path: &str, content: &str) -> Vec<SecretFinding> {
        let mut findings = Vec::new();
        let mut child_process_aliases = vec!["child_process".to_string()];
        for source_line in content.lines().filter(|line| !is_comment_only(line)) {
            for pattern in &self.child_process_alias_patterns {
                if let Some(alias) = pattern
                    .captures(source_line)
                    .and_then(|captures| captures.get(1))
                {
                    child_process_aliases.push(alias.as_str().to_string());
                }
            }
        }

        for (line_number, line) in content.lines().enumerate() {
            let line_number = line_number + 1; // 1-based

            // Skip comment-only lines (best effort — handles // # and SQL --)
            if is_comment_only(line) {
                continue;
            }

            // Layer 1: known prefixes
            let mut l1_matched_spans: Vec<(usize, usize)> = Vec::new();
            for (label, pattern) in &self.known_prefixes {
                if let Some(m) = pattern.find(line) {
                    // Skip if this span overlaps one already reported on this line.
                    let overlaps = l1_matched_spans.iter().any(|&(start, end)| {
                        m.start() < end && m.end() > start
                    });
                    if overlaps {
                        continue;
                    }
                    l1_matched_spans.push((m.start(), m.end()));
                    findings.push(SecretFinding {
                        file_path: file_path.to_string(),
                        line: line_number,
                        kind: SecretKind::KnownPrefix(label),
                        matched_text: truncate_secret(m.as_str()),
                        level: 5,
                    });
                }
            }

            // Layer 2: high-entropy strings
            for candidate in extract_string_literals(line) {
                if is_known_public_hash_context(line, &candidate) {
                    continue;
                }
                if candidate.len() >= 20
                    && looks_like_key_charset(&candidate)
                    && shannon_entropy(&candidate) > 4.5
                {
                    // Skip if already caught by layer 1
                    let already_flagged = findings
                        .iter()
                        .any(|f| f.line == line_number && f.level == 5);
                    if !already_flagged {
                        findings.push(SecretFinding {
                            file_path: file_path.to_string(),
                            line: line_number,
                            kind: SecretKind::HighEntropy,
                            matched_text: truncate_secret(&candidate),
                            level: 4,
                        });
                    }
                }
            }

            // Layer 3: hardcoded assignment patterns
            for captures in self.assignment_pattern.captures_iter(line) {
                let Some(name) = captures.name("name").map(|m| m.as_str()) else {
                    continue;
                };
                let Some(value) = captures
                    .name("double")
                    .or_else(|| captures.name("single"))
                    .map(|m| m.as_str())
                else {
                    continue;
                };
                if is_safe_fetch_credentials(name, value) || looks_like_placeholder(value) {
                    continue;
                }
                if findings.iter().any(|f| f.line == line_number) {
                    break;
                }
                findings.push(SecretFinding {
                    file_path: file_path.to_string(),
                    line: line_number,
                    kind: SecretKind::HardcodedAssignment,
                    matched_text: name.to_string(),
                    level: 4,
                });
                break;
            }

            // Layer 4: SQL injection via string building. Independent of
            // layers 1-3 — a line can leak a secret AND build a query
            // unsafely, these are different concerns and neither should
            // suppress the other.
            if self.sql_injection_patterns.iter().any(|pattern| pattern.is_match(line)) {
                findings.push(SecretFinding {
                    file_path: file_path.to_string(),
                    line: line_number,
                    kind: SecretKind::SqlInjection,
                    matched_text: truncate_line_for_display(line),
                    level: 4,
                });
            }

            // Layer 5: dangerous dynamic execution. Report at most one
            // finding per line even if multiple patterns match (e.g. a line
            // combining `eval(` and `shell=True` is unusual but would
            // otherwise double-report the same line).
            let dangerous_label = self
                .dangerous_execution_patterns
                .iter()
                .find(|(_, pattern)| pattern.is_match(line))
                .map(|(label, _)| *label)
                .or_else(|| {
                    let imported_method_call = self
                        .child_process_method_pattern
                        .captures(line)
                        .and_then(|captures| captures.get(1))
                        .is_some_and(|receiver| {
                            child_process_aliases
                                .iter()
                                .any(|alias| alias == receiver.as_str())
                        });
                    (imported_method_call
                        || self.child_process_require_exec_pattern.is_match(line))
                    .then_some("child_process.exec()")
                });
            if let Some(label) = dangerous_label {
                findings.push(SecretFinding {
                    file_path: file_path.to_string(),
                    line: line_number,
                    kind: SecretKind::DangerousExecution(label),
                    matched_text: truncate_line_for_display(line),
                    level: 4,
                });
            }

            // Layer 6: overly permissive IAM/policy statements.
            for (label, pattern) in &self.permissive_iam_patterns {
                if pattern.is_match(line) {
                    findings.push(SecretFinding {
                        file_path: file_path.to_string(),
                        line: line_number,
                        kind: SecretKind::PermissiveIam(label),
                        matched_text: truncate_line_for_display(line),
                        level: 4,
                    });
                }
            }
        }

        findings
    }
}

/// Convert a `SecretFinding` into the JSON signal format used by `run_full_check`.
pub fn finding_to_signal(f: &SecretFinding) -> Value {
    let description = match f.kind {
        SecretKind::KnownPrefix(label) => format!(
            "硬编码 {label}（{}）被检测到。密钥不得写入源码；请改用环境变量或密钥管理服务。",
            f.matched_text
        ),
        SecretKind::HighEntropy => format!(
            "高熵字符串（{}...）疑似密钥。如果不是密钥请添加 audit-risk:ignore 注释忽略此行。",
            f.matched_text
        ),
        SecretKind::HardcodedAssignment => format!(
            "敏感变量 `{}` 被直接赋值字符串字面量。密钥不得硬编码；请改用环境变量。",
            f.matched_text
        ),
        SecretKind::SqlInjection => format!(
            "疑似 SQL 注入：SQL 语句通过字符串拼接/插值构造，而不是用参数化占位符传给驱动。（{}）请改用参数化查询（如 %s / ? / :name 占位符 + 单独传参）。",
            f.matched_text
        ),
        SecretKind::DangerousExecution(label) => format!(
            "检测到危险的动态执行：{label}。（{}）如果参数包含外部输入，可能被用来执行任意代码或 shell 命令，请确认输入来源可信或改用更安全的调用方式。",
            f.matched_text
        ),
        SecretKind::PermissiveIam(label) => format!(
            "检测到过度宽松的权限声明：{label}。（{}）通配符权限违反最小权限原则，请把 Action/Resource 收窄到实际需要的范围。",
            f.matched_text
        ),
    };
    json!({
        "signal": {
            "description": description,
            "file_path": f.file_path,
            "line": f.line,
            "level": f.level,
            "affected_nodes": [],
        },
        "level": f.level,
    })
}

/// Scan the file at each `read_paths[i]` and report findings under the
/// corresponding `display_paths[i]`.
///
/// `read_paths` must be resolvable from the current process (typically
/// absolute, or relative to the caller's cwd). `display_paths` are what
/// gets shown to the user/audit log — usually the workspace-relative path,
/// so findings don't leak the developer's local absolute filesystem layout.
///
/// If the two slices have different lengths, falls back to using
/// `read_paths` for display too (defensive — should not happen in practice).
/// Files that cannot be read (binary, missing) are silently skipped.
pub fn scan_changed_files(read_paths: &[String], display_paths: &[String]) -> Vec<Value> {
    let scanner = SecretScanner::new();
    let mut signals = Vec::new();
    let use_display_paths = read_paths.len() == display_paths.len();
    for (index, read_path) in read_paths.iter().enumerate() {
        let content = match std::fs::read_to_string(read_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let display_path = if use_display_paths {
            &display_paths[index]
        } else {
            read_path
        };
        for finding in scanner.scan_content(display_path, &content) {
            signals.push(finding_to_signal(&finding));
        }
    }
    signals
}

/// Full-tree scan: walk every file under `project_root` (skipping build/dependency
/// dirs via `discovery::is_excluded`, plus audit-risk's own scaffolding) and run
/// the per-file scanner over each.
///
/// Used by `check` when there is no diff to review — the first run on an existing
/// codebase, a non-git directory, or a clean tree. Without this, `check` would
/// report "0 findings" on a repo full of leaked keys just because nothing changed
/// since the baseline — a dangerous false "all clear".
pub fn scan_workspace(project_root: &str) -> Vec<Value> {
    if project_root.is_empty() {
        return Vec::new();
    }
    let root = std::path::Path::new(project_root);
    let scanner = SecretScanner::new();
    let mut signals = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !crate::pipeline::discovery::is_excluded(e))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        let display = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        // Skip audit-risk's own managed files so scaffolding never shows as risk.
        if display.starts_with(".hologram/")
            || display.starts_with(".githooks/")
            || display == ".github/workflows/hologram-risk.yml"
        {
            continue;
        }
        // Skip dependency lockfiles — machine-generated hash blobs whose
        // high-entropy integrity hashes are pure noise, never real user secrets.
        let file_name = display.rsplit('/').next().unwrap_or("");
        if matches!(
            file_name,
            "package-lock.json"
                | "npm-shrinkwrap.json"
                | "yarn.lock"
                | "pnpm-lock.yaml"
                | "Cargo.lock"
                | "poetry.lock"
                | "Pipfile.lock"
                | "composer.lock"
                | "Gemfile.lock"
                | "go.sum"
        ) {
            continue;
        }
        // Skip very large files (generated/minified) — real secret/injection
        // sites live in human-sized source; a multi-MB regex scan is waste.
        if entry.metadata().map(|m| m.len() > 1_048_576).unwrap_or(false) {
            continue;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue, // binary / non-UTF8 → skip
        };
        for finding in scanner.scan_content(&display, &content) {
            signals.push(finding_to_signal(&finding));
        }
    }
    signals
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn is_comment_only(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("-- ")
        || trimmed == "--"
        || trimmed.starts_with('*')
}

fn is_safe_fetch_credentials(name: &str, value: &str) -> bool {
    name.eq_ignore_ascii_case("credentials")
        && matches!(
            value.to_ascii_lowercase().as_str(),
            "omit" | "same-origin" | "same-site" | "include"
        )
}

fn is_known_public_hash_context(line: &str, candidate: &str) -> bool {
    let normalized = candidate.to_ascii_lowercase();
    if !["sha256-", "sha384-", "sha512-"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
    {
        return false;
    }

    let lower_line = line.to_ascii_lowercase();
    let Some(attribute_start) = lower_line.find("integrity") else {
        return false;
    };
    let after_name = line[attribute_start + "integrity".len()..].trim_start();
    let Some(after_equals) = after_name.strip_prefix('=') else {
        return false;
    };
    let attribute_value = after_equals.trim_start();
    let Some(quote) = attribute_value.chars().next() else {
        return false;
    };
    if quote != '\'' && quote != '"' {
        return false;
    }
    attribute_value[quote.len_utf8()..]
        .split(quote)
        .next()
        .is_some_and(|value| value == candidate)
}

/// True for values that are obviously placeholders, not real secrets:
/// `changeme`, `<YOUR_API_KEY_HERE>`, `xxxxxxxx`, `example`, etc. Used to
/// suppress Layer-3 assignment false positives. Real secrets with a known
/// shape are still caught by Layer 1/2, which run first and ignore this list.
fn looks_like_placeholder(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    if v.contains('<') || v.contains('>') {
        return true;
    }
    const MARKERS: &[&str] = &[
        "placeholder", "changeme", "change_me", "change-me", "changeit", "your_", "your-",
        "yourkey", "example", "dummy", "redacted", "todo", "fixme", "sample", "insert_",
        "replace_", "n/a", "xxxx", "...", "foobar", "mysecret", "mypassword",
    ];
    if MARKERS.iter().any(|m| v.contains(m)) {
        return true;
    }
    // A run of one repeated char, e.g. "********" / "xxxxxxxx".
    let mut chars = v.chars();
    if let Some(first) = chars.next() {
        if v.chars().count() >= 4 && chars.all(|c| c == first) {
            return true;
        }
    }
    false
}

/// Truncate a matched secret to at most 12 bytes for display (never log full keys).
/// Truncates at a char boundary — matched text can contain multi-byte UTF-8
/// (e.g. Unicode whitespace matched by a regex `\s` class), and naive byte
/// slicing at a fixed offset can land mid-character and panic.
fn truncate_secret(s: &str) -> String {
    let s = s.trim();
    if s.len() <= 12 {
        return s.to_string();
    }
    let mut end = 12;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

/// Same char-boundary-safe truncation as `truncate_secret`, but with a much
/// wider window — used for findings (SQL injection, dangerous execution,
/// permissive IAM) where the displayed text is the risky *code span*, not a
/// secret value, so cutting it to 12 chars would make the finding useless
/// (e.g. an f-string SQL injection line would show only `f'SELECT * F...`).
fn truncate_line_for_display(s: &str) -> String {
    let s = s.trim();
    if s.len() <= 100 {
        return s.to_string();
    }
    let mut end = 100;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

/// Extract string literals from a line (content between matching quote pairs).
fn extract_string_literals(line: &str) -> Vec<String> {
    let mut results = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            let start = i + 1;
            let mut j = start;
            while j < chars.len() {
                if chars[j] == '\\' {
                    j += 2;
                    continue;
                }
                if chars[j] == quote {
                    break;
                }
                j += 1;
            }
            if j > start {
                // `j` can overshoot len when a trailing `\` triggers `j += 2`;
                // clamp so slicing a Vec<char> never panics out of range.
                let end = j.min(chars.len());
                results.push(chars[start..end].iter().collect());
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
    results
}

/// Returns true if the string uses only base64/hex-compatible characters.
fn looks_like_key_charset(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'))
}

/// Shannon entropy of a string (bits per character).
fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    for byte in s.bytes() {
        freq[byte as usize] += 1;
    }
    let len = s.len() as f64;
    freq.iter()
        .filter(|&&count| count > 0)
        .map(|&count| {
            let p = count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanner() -> SecretScanner {
        SecretScanner::new()
    }

    // ─── Layer 1: known prefix ────────────────────────────────────────────────

    #[test]
    fn detects_openai_sk_key() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "src/config.py",
            r#"api_key = "sk-abcdefghijklmnopqrstuvwxyz123456""#,
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].level, 5);
        assert!(matches!(findings[0].kind, SecretKind::KnownPrefix(_)));
    }

    #[test]
    fn detects_aws_akia_key() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "terraform/main.tf",
            "access_key = \"AKIAIOSFODNN7EXAMPLE\"",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].level, 5);
    }

    #[test]
    fn detects_github_pat() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            ".env",
            "GITHUB_TOKEN=ghp_16C7e42F292c6912E7710c838347Ae178B4a",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].level, 5);
    }

    #[test]
    fn detects_private_key_block() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "keys/server.pem",
            "-----BEGIN RSA PRIVATE KEY-----",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].level, 5);
    }

    #[test]
    fn detects_stripe_live_key() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "payment.js",
            r#"const stripe = Stripe("sk_live_51AbcDEFghijKLMNOP123456789");"#,
        );
        assert!(!findings.is_empty());
        assert_eq!(findings[0].level, 5);
    }

    // ─── Layer 2: entropy ─────────────────────────────────────────────────────

    #[test]
    fn detects_high_entropy_string() {
        let scanner = scanner();
        // 32-char random-looking base64 string — not a known prefix
        let findings = scanner.scan_content(
            "config.go",
            r#"secret := "xK9mP2qR7vL4nJ8wZ1yA6bC3dE5fG0hI""#,
        );
        assert!(!findings.is_empty());
        // Should be level 4 (entropy hit, not known prefix)
        assert!(findings.iter().any(|f| f.level == 4));
    }

    #[test]
    fn low_entropy_string_not_flagged() {
        let scanner = scanner();
        // "hello world" has low entropy
        let findings = scanner.scan_content("main.rs", r#"let msg = "hello world example";"#);
        // Should not be flagged by entropy (low entropy, short, common words)
        assert!(
            findings.iter().all(|f| f.kind != SecretKind::HighEntropy),
            "low-entropy string should not be flagged"
        );
    }

    // ─── Layer 3: assignment pattern ──────────────────────────────────────────

    #[test]
    fn detects_hardcoded_password_assignment() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "db.py",
            r#"password = "hunter2secretpassword""#,
        );
        assert!(!findings.is_empty());
        assert!(findings
            .iter()
            .any(|f| f.kind == SecretKind::HardcodedAssignment
                || matches!(f.kind, SecretKind::KnownPrefix(_))
                || f.kind == SecretKind::HighEntropy));
    }

    #[test]
    fn detects_api_key_colon_assignment() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "config.ts",
            r#"  apiKey: "someRandomApiKeyValue1234567""#,
        );
        assert!(!findings.is_empty());
    }

    // ─── Negative cases ───────────────────────────────────────────────────────

    #[test]
    fn comment_lines_are_skipped() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "main.py",
            "# api_key = \"sk-example-do-not-use-123456789012\"",
        );
        assert!(findings.is_empty(), "comment lines must not be flagged");
    }

    #[test]
    fn placeholder_strings_in_example_code_low_false_positive() {
        let scanner = scanner();
        // Common placeholder values — short, low entropy, no known prefix
        let findings = scanner.scan_content(
            "README.md",
            r#"api_key = "YOUR_API_KEY_HERE""#,
        );
        // Entropy of "YOUR_API_KEY_HERE" is low and length < 20 chars for the value part
        // Assignment pattern may still fire — that's acceptable (it IS a secret assignment)
        // What matters: no known-prefix L5 hit
        assert!(!findings.iter().any(|f| f.level == 5), "placeholder must not be L5");
    }

    // ─── Layer 4: SQL injection ───────────────────────────────────────────────

    #[test]
    fn detects_python_fstring_sql_injection() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "db.py",
            r#"query = f"SELECT * FROM users WHERE id = {user_id}""#,
        );
        assert!(
            findings.iter().any(|f| f.kind == SecretKind::SqlInjection),
            "f-string SQL interpolation must be flagged"
        );
    }

    #[test]
    fn detects_js_template_literal_sql_injection() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "db.js",
            "const query = `SELECT * FROM users WHERE id = ${userId}`;",
        );
        assert!(
            findings.iter().any(|f| f.kind == SecretKind::SqlInjection),
            "template literal SQL interpolation must be flagged"
        );
    }

    #[test]
    fn detects_string_concatenation_sql_injection() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "db.py",
            r#"query = "SELECT * FROM users WHERE id = " + user_id"#,
        );
        assert!(
            findings.iter().any(|f| f.kind == SecretKind::SqlInjection),
            "string concatenation building a SQL query must be flagged"
        );
    }

    #[test]
    fn detects_reversed_string_concatenation_sql_injection() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "db.py",
            r#"query = prefix + "DROP TABLE users""#,
        );
        assert!(
            findings.iter().any(|f| f.kind == SecretKind::SqlInjection),
            "variable + SQL-keyword-string concatenation must be flagged (reversed order)"
        );
    }

    #[test]
    fn parameterized_query_is_not_flagged_as_sql_injection() {
        let scanner = scanner();
        // The canonical SAFE pattern: placeholder in the query string,
        // parameters passed separately to the driver. Must not false-positive.
        let findings = scanner.scan_content(
            "db.py",
            r#"cursor.execute("SELECT * FROM users WHERE id = %s", (user_id,))"#,
        );
        assert!(
            !findings.iter().any(|f| f.kind == SecretKind::SqlInjection),
            "parameterized query with a placeholder must not be flagged"
        );
    }

    #[test]
    fn plain_sql_string_without_interpolation_is_not_flagged() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "db.py",
            r#"query = "SELECT * FROM users WHERE active = true""#,
        );
        assert!(
            !findings.iter().any(|f| f.kind == SecretKind::SqlInjection),
            "a static SQL string with no interpolation/concatenation must not be flagged"
        );
    }

    // ─── Layer 5: dangerous dynamic execution ─────────────────────────────────

    #[test]
    fn detects_eval() {
        let scanner = scanner();
        let findings = scanner.scan_content("app.py", "result = eval(user_input)");
        assert!(findings
            .iter()
            .any(|f| matches!(f.kind, SecretKind::DangerousExecution("eval()"))));
    }

    #[test]
    fn detects_bare_python_exec() {
        let scanner = scanner();
        let findings = scanner.scan_content("app.py", "exec(compile(source, '<string>', 'exec'))");
        assert!(findings
            .iter()
            .any(|f| matches!(f.kind, SecretKind::DangerousExecution("exec()"))));
    }

    #[test]
    fn detects_os_system() {
        let scanner = scanner();
        let findings = scanner.scan_content("app.py", "os.system(f'rm -rf {path}')");
        assert!(findings
            .iter()
            .any(|f| matches!(f.kind, SecretKind::DangerousExecution("os.system()"))));
    }

    #[test]
    fn detects_shell_true() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "app.py",
            "subprocess.run(cmd, shell=True)",
        );
        assert!(findings
            .iter()
            .any(|f| matches!(f.kind, SecretKind::DangerousExecution("shell=True"))));
    }

    #[test]
    fn detects_js_child_process_exec_method_call() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "app.js",
            "child_process.exec(`ls ${dir}`, callback);",
        );
        assert!(
            findings
                .iter()
                .any(|f| matches!(f.kind, SecretKind::DangerousExecution("child_process.exec()"))),
            "child_process.exec( method call must be flagged"
        );
        // Must not ALSO double-report as the bare exec() pattern.
        assert!(
            !findings.iter().any(|f| matches!(f.kind, SecretKind::DangerousExecution("exec()"))),
            "a method call must not also match the bare exec() pattern"
        );
    }

    #[test]
    fn detects_new_function_from_string() {
        let scanner = scanner();
        let findings = scanner.scan_content("app.js", "const fn = new Function('return ' + userCode);");
        assert!(findings
            .iter()
            .any(|f| matches!(f.kind, SecretKind::DangerousExecution("new Function() from string"))));
    }

    #[test]
    fn bare_exec_and_method_exec_are_mutually_exclusive() {
        let scanner = scanner();
        // Pure Python exec() at start of line — no leading dot.
        let bare = scanner.scan_content("a.py", "exec(user_code)");
        assert!(bare.iter().any(|f| matches!(f.kind, SecretKind::DangerousExecution("exec()"))));
        assert!(!bare.iter().any(|f| matches!(f.kind, SecretKind::DangerousExecution("child_process.exec()"))));

        // Known child_process namespace method call — leading dot.
        let method = scanner.scan_content("a.js", "child_process.exec(cmd)");
        assert!(method.iter().any(|f| matches!(f.kind, SecretKind::DangerousExecution("child_process.exec()"))));
        assert!(!method.iter().any(|f| matches!(f.kind, SecretKind::DangerousExecution("exec()"))));
    }

    #[test]
    fn safe_function_calls_are_not_flagged() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "app.py",
            "result = calculate_total(items)\nlogger.execute_query(sql, params)",
        );
        assert!(
            findings.is_empty(),
            "ordinary function calls that merely contain 'exec' as a substring of a longer identifier must not be flagged: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn p1_6_method_eval_is_not_flagged() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "pytorch_eval.py",
            "import torch\nmodel = torch.nn.Linear(10, 1)\nmodel.eval()\nloss_val = criterion.eval()",
        );
        assert!(
            findings.is_empty(),
            "object eval methods are not dynamic code evaluation: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn p1_6_regex_exec_is_not_flagged() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "regex_exec.js",
            r#"const pattern = /hello/g;
const match = pattern.exec(inputString);
const re = new RegExp('\\d+');
re.exec(text);"#,
        );
        assert!(
            findings.is_empty(),
            "RegExp.exec calls are not child-process execution: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn p1_6_commented_child_process_alias_does_not_change_exec_semantics() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "regex_exec.js",
            "// const pattern = require('child_process');\npattern.exec(inputString);",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn p1_6_ui_sql_words_are_not_flagged() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "ui_text.ts",
            r#"const label = "Select an option";
const btn = "Update profile";
const msg = "Delete this item?";
const help = "Insert your name here";
const dynamicLabel = "Select " + option;
const splitUiCopy = "Select " + option + " from " + source;"#,
        );
        assert!(
            findings.is_empty(),
            "UI copy containing isolated SQL verbs must not be treated as SQL injection: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn p1_6_fetch_credentials_enums_are_not_flagged() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "fetch_api.ts",
            r#"fetch('/api/data', { credentials: 'same-origin' });
fetch('/api/auth', { credentials: 'include' });
fetch('/api/public', { credentials: 'omit' });"#,
        );
        assert!(
            findings.is_empty(),
            "Fetch credentials enums are transport policy, not secrets: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn p1_6_fetch_credentials_enum_does_not_hide_another_secret() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "fetch_api.ts",
            r#"const options = { credentials: 'include', password: 'ActualSecret123' };"#,
        );
        assert!(
            findings
                .iter()
                .any(|f| f.kind == SecretKind::HardcodedAssignment),
            "a safe Fetch enum must not suppress a separate hardcoded secret on the same line"
        );
    }

    #[test]
    fn p1_6_sri_hash_is_not_flagged() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "sri_hash.html",
            r#"<script src="jquery.js"
  integrity="sha384-oqVuAfXRKap7fdgcCY5uykM6+R9GqQ8K/uxFMUFe7bPWwGa5R2UgfyAkOmDr6Gq"
  crossorigin="anonymous"></script>"#,
        );
        assert!(
            findings.is_empty(),
            "public SRI hashes must not be treated as secrets: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn p1_6_sri_hash_does_not_hide_another_high_entropy_value() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "sri_hash.html",
            r#"<script integrity="sha384-oqVuAfXRKap7fdgcCY5uykM6+R9GqQ8K/uxFMUFe7bPWwGa5R2UgfyAkOmDr6Gq" data-secret="xK9mP2qR7vL4nJ8wZ1yA6bC3dE5fG0h"></script>"#,
        );
        assert_eq!(
            findings
                .iter()
                .filter(|f| f.kind == SecretKind::HighEntropy)
                .count(),
            1,
            "only the public integrity hash may be suppressed"
        );
    }

    // ─── Layer 6: permissive IAM ───────────────────────────────────────────────

    #[test]
    fn detects_wildcard_iam_action() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "policy.json",
            r#"    "Action": "*","#,
        );
        assert!(findings
            .iter()
            .any(|f| matches!(f.kind, SecretKind::PermissiveIam("IAM Action 通配符"))));
    }

    #[test]
    fn detects_wildcard_iam_resource() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "policy.json",
            r#"    "Resource": "*""#,
        );
        assert!(findings
            .iter()
            .any(|f| matches!(f.kind, SecretKind::PermissiveIam("IAM Resource 通配符"))));
    }

    #[test]
    fn detects_wildcard_iam_action_array_form() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "policy.json",
            r#"    "Action": ["*"],"#,
        );
        assert!(findings
            .iter()
            .any(|f| matches!(f.kind, SecretKind::PermissiveIam("IAM Action 通配符"))));
    }

    #[test]
    fn scoped_iam_action_is_not_flagged() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "policy.json",
            r#"    "Action": "s3:GetObject","#,
        );
        assert!(
            !findings.iter().any(|f| matches!(f.kind, SecretKind::PermissiveIam(_))),
            "a scoped IAM action must not be flagged as permissive"
        );
    }

    #[test]
    fn scoped_iam_resource_is_not_flagged() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "policy.json",
            r#"    "Resource": "arn:aws:s3:::my-bucket/*""#,
        );
        assert!(
            !findings.iter().any(|f| matches!(f.kind, SecretKind::PermissiveIam(_))),
            "a resource ARN that merely ends with a wildcard suffix (scoped to one bucket) must not be flagged"
        );
    }

    #[test]
    fn finding_to_signal_describes_new_kinds_correctly() {
        let sql = SecretFinding {
            file_path: "db.py".to_string(),
            line: 1,
            kind: SecretKind::SqlInjection,
            matched_text: "f-string example".to_string(),
            level: 4,
        };
        let signal = finding_to_signal(&sql);
        assert!(signal["signal"]["description"].as_str().unwrap().contains("SQL 注入"));

        let exec_finding = SecretFinding {
            file_path: "app.py".to_string(),
            line: 1,
            kind: SecretKind::DangerousExecution("eval()"),
            matched_text: "eval example".to_string(),
            level: 4,
        };
        let signal = finding_to_signal(&exec_finding);
        assert!(signal["signal"]["description"].as_str().unwrap().contains("危险的动态执行"));
        assert!(signal["signal"]["description"].as_str().unwrap().contains("eval()"));

        let iam_finding = SecretFinding {
            file_path: "policy.json".to_string(),
            line: 1,
            kind: SecretKind::PermissiveIam("IAM Action 通配符"),
            matched_text: "\"Action\": \"*\"".to_string(),
            level: 4,
        };
        let signal = finding_to_signal(&iam_finding);
        assert!(signal["signal"]["description"].as_str().unwrap().contains("过度宽松"));
    }

    #[test]
    fn scan_changed_files_skips_missing_file() {
        // Should not panic on a file that doesn't exist
        let paths = vec!["/tmp/nonexistent_audit_risk_test_file.rs".to_string()];
        let signals = scan_changed_files(&paths, &paths);
        assert!(signals.is_empty());
    }

    #[test]
    fn scan_changed_files_reports_under_display_path_not_read_path() {
        let dir = std::env::temp_dir().join(format!("audit-risk-secrets-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let abs_file = dir.join("config.py");
        std::fs::write(&abs_file, r#"api_key = "sk-abcdefghijklmnopqrstuvwxyz123456""#).expect("write file");

        let read_paths = vec![abs_file.to_string_lossy().into_owned()];
        let display_paths = vec!["src/config.py".to_string()];
        let signals = scan_changed_files(&read_paths, &display_paths);

        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0]["signal"]["file_path"], "src/config.py");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_content_does_not_panic_on_multibyte_utf8_near_truncation_boundary() {
        // Regression test: a Bearer-token match whose regex `\s` class consumes
        // a non-ASCII whitespace character (U+00A0) positioned so the matched
        // text's byte 12 lands mid-character. Before the fix, truncate_secret's
        // fixed `&s[..12]` byte slice panicked with "not a char boundary" and
        // crashed the whole `audit-risk check`/`watch` process on this input.
        let scanner = scanner();
        let line = "Bearer:====\u{00A0}'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'";
        let findings = scanner.scan_content("weird.txt", line);
        // Must not panic; content of findings is secondary to survival here.
        assert!(!findings.is_empty());
    }

    #[test]
    fn truncate_secret_handles_multibyte_boundary_without_panicking() {
        // Direct unit coverage of the helper across a range of boundary offsets.
        for pad in 0..16 {
            let padding = "x".repeat(pad);
            let s = format!("{padding}\u{00A0}{}", "A".repeat(40));
            // Must not panic regardless of where the multi-byte char lands.
            let _ = truncate_secret(&s);
        }
    }

    // ─── Helpers ──────────────────────────────────────────────────────────────

    #[test]
    fn shannon_entropy_of_random_string_is_high() {
        // A truly random 32-char base64 string should have entropy > 4.5
        let s = "xK9mP2qR7vL4nJ8wZ1yA6bC3dE5fG0h";
        assert!(shannon_entropy(s) > 4.0, "random string entropy={}", shannon_entropy(s));
    }

    #[test]
    fn shannon_entropy_of_repeated_string_is_low() {
        let s = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(shannon_entropy(s) < 1.0);
    }

    #[test]
    fn extract_string_literals_handles_escape() {
        let line = r#"let x = "hello \"world\""; let y = 'test';"#;
        let literals = extract_string_literals(line);
        assert!(literals.iter().any(|l| l.contains("hello")));
        assert!(literals.iter().any(|l| l == "test"));
    }

    #[test]
    fn extract_string_literals_trailing_backslash_no_panic() {
        // A trailing `\` used to push the char index past the end → slice panic.
        // Multibyte (Chinese) content made it worse. These must not panic.
        let _ = extract_string_literals("x = \"abc\\");
        let _ = extract_string_literals("q = '值值值\\");
        let scanner = SecretScanner::new();
        let _ = scanner.scan_content("f.py", "密码 = \"值值值\\");
        let _ = scanner.scan_content("q.py", "sql = \"SELECT * FROM 用户表\\");
    }

    #[test]
    fn finding_to_signal_produces_correct_level() {
        let finding = SecretFinding {
            file_path: "src/main.rs".to_string(),
            line: 42,
            kind: SecretKind::KnownPrefix("GitHub PAT"),
            matched_text: "ghp_abc123...".to_string(),
            level: 5,
        };
        let signal = finding_to_signal(&finding);
        assert_eq!(signal["level"], 5);
        assert_eq!(signal["signal"]["line"], 42);
        assert!(signal["signal"]["description"]
            .as_str()
            .unwrap()
            .contains("GitHub PAT"));
    }
}
