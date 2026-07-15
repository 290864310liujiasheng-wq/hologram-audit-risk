/// Secret scanning engine.
///
/// Seven detection layers. The first six scan each changed line; the seventh
/// uses bounded content windows for multi-line LLM calls:
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
/// 7. **Prompt injection residue** — user-controlled values concatenated into
///    prompts or passed directly inside bounded LLM API call windows.
///
/// Output signals use level 5 (critical) for definite known-prefix hits,
/// level 4 (high) for entropy, assignment, SQL injection, dangerous
/// execution, and permissive IAM hits.
use regex::Regex;
use serde_json::{json, Value};

const AI_001_RULE_ID: &str = "AI-001";
const AI_001_SEVERITY: &str = "high";
const AI_001_EXPLANATION: &str = "用户输入未经过滤直接拼入 LLM prompt（Prompt Injection 风险）。攻击者可通过构造输入操控模型行为，泄露系统提示或执行越权操作。应对用户输入进行长度限制、特殊字符转义，并将系统提示与用户输入严格分离。";

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
    PromptInjection,
}

pub struct SecretScanner {
    /// Each tuple: (display label, regex matching the secret value)
    known_prefixes: Vec<(&'static str, Regex)>,
    /// Quoted assignments for established credential variable names.
    assignment_pattern: Regex,
    /// Unquoted scalar assignments in env files.
    env_assignment_pattern: Regex,
    /// Hex credentials tied to a sensitive variable name.
    hex_assignment_pattern: Regex,
    /// Plaintext passwords embedded in database connection strings.
    connection_password_pattern: Regex,
    mongodb_connection_pattern: Regex,
    azure_account_key_pattern: Regex,
    /// SQL statement grammar and dynamic string-building forms. A finding
    /// requires both to occur in the same string-building expression.
    sql_statement_pattern: Regex,
    sql_string_building_patterns: Vec<Regex>,
    sql_concatenation_pattern: Regex,
    /// Dangerous dynamic execution: (display label, regex)
    dangerous_execution_patterns: Vec<(&'static str, Regex)>,
    /// Namespace bindings and rebinding for Node's `child_process` module.
    child_process_alias_pattern: Regex,
    declaration_target_pattern: Regex,
    assignment_target_pattern: Regex,
    /// Receiver and direct-require forms of child-process exec calls.
    child_process_method_pattern: Regex,
    child_process_require_exec_pattern: Regex,
    /// Language-scoped command execution APIs for PHP and C-family sources.
    system_call_pattern: Regex,
    system_declaration_pattern: Regex,
    shell_exec_pattern: Regex,
    /// Overly permissive IAM/policy statement patterns: (display label, regex)
    permissive_iam_patterns: Vec<(&'static str, Regex)>,
    /// AI-001 prompt construction and bounded LLM call patterns.
    prompt_assignment_pattern: Regex,
    messages_assignment_pattern: Regex,
    user_message_content_pattern: Regex,
    llm_call_pattern: Regex,
    llm_prompt_argument_pattern: Regex,
    user_input_pattern: Regex,
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
            r#"(?i)\b(?P<name>api_?key|api_?secret|app_?secret|auth_?token|access_?token|secret_?token|secret_?key|private_?key|client_?secret|db_?pass(?:word)?|database_?pass(?:word)?|password|passwd|credentials?|auth_?key)\b\s*[=:]\s*(?:"(?P<double>[^"\r\n]{8,})"|'(?P<single>[^'\r\n]{8,})')"#,
        )
        .unwrap();
        let env_assignment_pattern = Regex::new(
            r#"(?i)^\s*(?:export\s+)?(?P<name>api_?key|api_?secret|app_?secret|auth_?token|access_?token|secret_?token|secret_?key|private_?key|client_?secret|db_?pass(?:word)?|database_?pass(?:word)?|password|passwd|credentials?|auth_?key|secret|token)\s*=\s*(?P<value>[^\s#]{8,})\s*(?:#.*)?$"#,
        )
        .unwrap();
        let hex_assignment_pattern = Regex::new(
            r#"(?i)\b(?P<name>api_?key|api_?secret|secret_?token|secret_?key|auth_?token|access_?token|password|secret|token)\b\s*[=:]\s*(?:"(?P<double>[0-9a-f]{32,})"|'(?P<single>[0-9a-f]{32,})'|(?P<bare>[0-9a-f]{32,})(?:\s*(?:[,;}\]#]|$)))"#,
        )
        .unwrap();
        let connection_password_pattern = Regex::new(
            r#"(?i)\b(?P<name>password|pwd)\s*=\s*(?P<value>[^;'"&\s]{6,})"#,
        )
        .unwrap();
        let mongodb_connection_pattern = Regex::new(
            r#"(?i)\bmongodb(?:\+srv)?://[^:\s/@]+:(?P<value>[^"'\s]{6,})@[A-Za-z0-9.-]+(?::\d+)?(?:/|["']|$)"#,
        )
        .unwrap();
        let azure_account_key_pattern = Regex::new(
            r#"(?i)\bAccountKey\s*=\s*(?P<value>[A-Za-z0-9+/]{20,}={0,2})(?:;|["']|$)"#,
        )
        .unwrap();

        // Require SQL grammar, not merely English words that overlap SQL
        // verbs. Dynamic identifiers cover f-string and template placeholders.
        let sql_name = r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*";
        let sql_dynamic_name = r"(?:\{[^{}\r\n]+\}|\$\{[^{}\r\n]+\})";
        let sql_identifier = format!(r"(?:{sql_name}|{sql_dynamic_name})");
        let sql_strong_select = format!(
            r"(?:\*|{sql_name}\s*\([^()\r\n]*\)(?:\s+AS\s+[A-Za-z_][A-Za-z0-9_]*)?|{sql_identifier}\s+AS\s+[A-Za-z_][A-Za-z0-9_]*|{sql_identifier}\s*,\s*{sql_identifier}(?:\s*,\s*{sql_identifier})*)"
        );
        let sql_statement_pattern = Regex::new(&format!(
            r"(?x)\b(?:
                (?i:SELECT)\s+(?i:DISTINCT\s+)?{sql_strong_select}\s+(?i:FROM)\s+{sql_identifier}(?:\s+(?i:AS)\s+[A-Za-z_][A-Za-z0-9_]*)?
                |SELECT\s+(?:DISTINCT\s+)?{sql_identifier}\s+FROM\s+{sql_identifier}(?:\s+AS\s+[A-Za-z_][A-Za-z0-9_]*)?
                |(?i:INSERT\s+INTO)\s+{sql_identifier}
                |(?i:UPDATE)\s+{sql_identifier}(?:\s+(?i:AS)\s+[A-Za-z_][A-Za-z0-9_]*)?\s+(?i:SET)\s+{sql_identifier}\s*=
                |(?i:DELETE\s+FROM)\s+{sql_identifier}
                |(?i:DROP\s+(?:TABLE|DATABASE|INDEX))\s+{sql_identifier}
                |(?i:ALTER\s+(?:TABLE|DATABASE|INDEX))\s+{sql_identifier}
            )"
        ))
        .unwrap();
        let sql_string_building_patterns = vec![
            Regex::new(r#"(?i)f"[^"\n]*\{[^}\n]+\}[^"\n]*""#).unwrap(),
            Regex::new(r"(?i)f'[^'\n]*\{[^}\n]+\}[^'\n]*'").unwrap(),
            Regex::new(r#"`[^`\n]*\$\{[^}\n]+\}[^`\n]*`"#).unwrap(),
            Regex::new(r#""[^"\n]*"\s*%\s*[A-Za-z_(]"#).unwrap(),
            Regex::new(r#"'[^'\n]*'\s*%\s*[A-Za-z_(]"#).unwrap(),
            Regex::new(r#""[^"\n]*"\s*\.\s*format\s*\("#).unwrap(),
            Regex::new(r#"'[^'\n]*'\s*\.\s*format\s*\("#).unwrap(),
        ];
        let sql_concatenation_operand =
            r#"(?:"[^"\n]*"|'[^'\n]*'|[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*)"#;
        let sql_concatenation_pattern = Regex::new(&format!(
            r"{sql_concatenation_operand}(?:\s*\+\s*{sql_concatenation_operand})+"
        ))
        .unwrap();

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
            ("os.popen()", Regex::new(r"\bos\.popen\s*\(").unwrap()),
            (
                "subprocess.getoutput()",
                Regex::new(r"\bsubprocess\.getoutput\s*\(").unwrap(),
            ),
            ("shell=True", Regex::new(r"\bshell\s*=\s*True\b").unwrap()),
            // Bare `execSync(` / `spawnSync(` from a destructured import
            // (`import { execSync } from 'child_process'`) — no leading dot, so
            // the dotted pattern above misses it.
            ("execSync()", Regex::new(r"(^|[^.\w])(exec|spawn)Sync\s*\(").unwrap()),
            ("new Function() from string", Regex::new(r"\bnew\s+Function\s*\(").unwrap()),
            ("__import__()", Regex::new(r"__import__\s*\(").unwrap()),
        ];

        let child_process_alias_pattern = Regex::new(
            r#"^\s*(?:(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=\s*require\s*\(\s*['"](?:node:)?child_process['"]\s*\)|import\s+\*\s+as\s+([A-Za-z_$][A-Za-z0-9_$]*)\s+from\s+['"](?:node:)?child_process['"])"#,
        )
        .unwrap();
        let declaration_target_pattern = Regex::new(
            r"^\s*(?:const|let|var)\s+([A-Za-z_$][A-Za-z0-9_$]*)\s*=",
        )
        .unwrap();
        let assignment_target_pattern =
            Regex::new(r"^\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*=").unwrap();
        let child_process_method_pattern = Regex::new(
            r"\b([A-Za-z_$][A-Za-z0-9_$]*)\s*\.\s*exec(?:Sync)?\s*\(",
        )
        .unwrap();
        let child_process_require_exec_pattern = Regex::new(
            r#"\brequire\s*\(\s*['"](?:node:)?child_process['"]\s*\)\s*\.\s*exec(?:Sync)?\s*\("#,
        )
        .unwrap();
        let system_call_pattern =
            Regex::new(r"(^|[^.\w])(?P<call>system)\s*\(").unwrap();
        let system_declaration_pattern = Regex::new(
            r"(?ix)^\s*(?:
                (?:(?:extern|static|inline)\s+)*(?:(?:unsigned|signed)\s+)?(?:int|void|char|long|short|size_t)\s+\**\s*
                |(?:(?:public|protected|private|static|final|abstract)\s+)*function\s+
            )(?P<declaration>system)\s*\([^;{}]*\)\s*(?:;|\{)",
        )
        .unwrap();
        let shell_exec_pattern = Regex::new(r"(^|[^.\w])shell_exec\s*\(").unwrap();

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

        let prompt_assignment_pattern =
            Regex::new(r"(?i)\b(?:prompt|messages?)(?:_[A-Za-z0-9]+)?\s*=").unwrap();
        let messages_assignment_pattern =
            Regex::new(r"(?i)\bmessages?(?:_[A-Za-z0-9]+)?\s*=\s*\[").unwrap();
        let user_input_pattern = Regex::new(
            r"(?ix)\b(?:
                user[_A-Za-z0-9]*(?:input|message|query|request|body|param|data)
                |(?:input|message|query|request|body|param|data)[_A-Za-z0-9]*user
                |(?:request|req)\s*\.\s*(?:body|json)(?:\s*\.\s*[A-Za-z_][A-Za-z0-9_]*)?
                |(?:flask|django)\s*\.\s*request(?:\s*\.\s*[A-Za-z_][A-Za-z0-9_]*)?
            )\b",
        )
        .unwrap();
        let user_message_content_pattern =
            Regex::new(r#"(?isx)["']role["']\s*:\s*["']user["'][^}]{0,512}["']content["']\s*:"#)
                .unwrap();
        let llm_call_pattern = Regex::new(
            r"(?ix)\b(?:
                (?:openai\s*\.\s*)?chat\s*\.\s*completions\s*\.\s*create
                |openai\s*\.\s*completions\s*\.\s*create
                |(?:anthropic\s*\.\s*)?messages\s*\.\s*create
                |langchain(?:\s*\.\s*[A-Za-z_][A-Za-z0-9_]*){1,6}
                |llm(?:\s*\.\s*[A-Za-z_][A-Za-z0-9_]*){1,6}
                |chat_completion(?:s)?(?:\s*\.\s*[A-Za-z_][A-Za-z0-9_]*){0,3}
            )\s*\(",
        )
        .unwrap();
        let llm_prompt_argument_pattern = Regex::new(
            r#"(?i)(?:\bmessages?\b|\bprompt\b|["']content["'])\s*(?:=|:)"#,
        )
        .unwrap();

        Self {
            known_prefixes,
            assignment_pattern,
            env_assignment_pattern,
            hex_assignment_pattern,
            connection_password_pattern,
            mongodb_connection_pattern,
            azure_account_key_pattern,
            sql_statement_pattern,
            sql_string_building_patterns,
            sql_concatenation_pattern,
            dangerous_execution_patterns,
            child_process_alias_pattern,
            declaration_target_pattern,
            assignment_target_pattern,
            child_process_method_pattern,
            child_process_require_exec_pattern,
            system_call_pattern,
            system_declaration_pattern,
            shell_exec_pattern,
            permissive_iam_patterns,
            prompt_assignment_pattern,
            messages_assignment_pattern,
            user_message_content_pattern,
            llm_call_pattern,
            llm_prompt_argument_pattern,
            user_input_pattern,
        }
    }

    /// Scan the contents of a single file.
    /// Returns all findings in line order.
    pub fn scan_content(&self, file_path: &str, content: &str) -> Vec<SecretFinding> {
        let mut findings = Vec::new();
        let mut child_process_bindings =
            vec![("child_process".to_string(), 0usize, true)];
        let mut brace_depth = 0usize;
        let mut in_block_comment = false;

        for (line_number, line) in content.lines().enumerate() {
            let line_number = line_number + 1; // 1-based
            let execution_line = strip_js_comments(line, &mut in_block_comment);
            let execution_code = mask_string_literals(&execution_line);
            let executable_code = mask_literals_preserving_interpolations(&execution_line);

            if let Some(captures) = self.child_process_alias_pattern.captures(&execution_line) {
                if let Some(alias) = captures.get(1).or_else(|| captures.get(2)) {
                    child_process_bindings.retain(|(current, depth, _)| {
                        current != alias.as_str() || *depth != brace_depth
                    });
                    child_process_bindings.push((
                        alias.as_str().to_string(),
                        brace_depth,
                        true,
                    ));
                }
            } else if let Some(target) = self
                .declaration_target_pattern
                .captures(&execution_code)
                .and_then(|captures| captures.get(1))
            {
                child_process_bindings.retain(|(name, depth, _)| {
                    name != target.as_str() || *depth != brace_depth
                });
                child_process_bindings.push((
                    target.as_str().to_string(),
                    brace_depth,
                    false,
                ));
            } else if let Some(target) = self
                .assignment_target_pattern
                .captures(&execution_code)
                .and_then(|captures| captures.get(1))
            {
                child_process_bindings.retain(|(name, depth, _)| {
                    name != target.as_str() || *depth != brace_depth
                });
                child_process_bindings.push((
                    target.as_str().to_string(),
                    brace_depth,
                    false,
                ));
            }

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
                let is_expandable_env_reference = supports_unquoted_secrets(file_path)
                    && captures.name("double").is_some()
                    && is_environment_reference(value);
                if is_safe_fetch_credentials(name, value)
                    || is_expandable_env_reference
                    || looks_like_placeholder(value)
                {
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

            if supports_unquoted_secrets(file_path)
                && !findings.iter().any(|f| f.line == line_number)
            {
                if let Some(captures) = self.env_assignment_pattern.captures(line) {
                    if let (Some(name), Some(value)) =
                        (captures.name("name"), captures.name("value"))
                    {
                        if !is_environment_reference(value.as_str())
                            && !looks_like_placeholder(value.as_str())
                        {
                            findings.push(SecretFinding {
                                file_path: file_path.to_string(),
                                line: line_number,
                                kind: SecretKind::HardcodedAssignment,
                                matched_text: name.as_str().to_string(),
                                level: 4,
                            });
                        }
                    }
                }
            }

            if !findings.iter().any(|f| f.line == line_number) {
                if let Some(captures) = self.hex_assignment_pattern.captures(line) {
                    if let Some(name) = captures.name("name") {
                        findings.push(SecretFinding {
                            file_path: file_path.to_string(),
                            line: line_number,
                            kind: SecretKind::HardcodedAssignment,
                            matched_text: name.as_str().to_string(),
                            level: 4,
                        });
                    }
                }
            }

            if !findings.iter().any(|f| f.line == line_number) {
                let connection_secret = self
                    .connection_password_pattern
                    .captures(line)
                    .filter(|_| is_database_connection_context(line))
                    .and_then(|captures| {
                        Some((
                            captures.name("name")?.as_str(),
                            captures.name("value")?.as_str(),
                        ))
                    })
                    .or_else(|| {
                        self.mongodb_connection_pattern
                            .captures(line)
                            .and_then(|captures| {
                                Some((
                                    "mongodb password",
                                    captures.name("value")?.as_str(),
                                ))
                            })
                    })
                    .or_else(|| {
                        self.azure_account_key_pattern
                            .captures(line)
                            .and_then(|captures| {
                                Some(("Azure AccountKey", captures.name("value")?.as_str()))
                            })
                    });
                if let Some((name, value)) = connection_secret {
                    if !looks_like_placeholder(value) {
                        findings.push(SecretFinding {
                            file_path: file_path.to_string(),
                            line: line_number,
                            kind: SecretKind::HardcodedAssignment,
                            matched_text: name.to_string(),
                            level: 4,
                        });
                    }
                }
            }

            // Layer 4: SQL injection via string building. Independent of
            // layers 1-3 — a line can leak a secret AND build a query
            // unsafely, these are different concerns and neither should
            // suppress the other.
            let has_dynamic_sql = self.sql_string_building_patterns.iter().any(|pattern| {
                pattern
                    .find_iter(line)
                    .any(|matched| self.sql_statement_pattern.is_match(matched.as_str()))
            }) || self.sql_concatenation_pattern.find_iter(line).any(|matched| {
                let normalized = normalize_string_concatenation(matched.as_str());
                self.sql_statement_pattern.is_match(&normalized)
            });
            if has_dynamic_sql {
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
                .find(|(label, pattern)| {
                    if *label == "eval()" {
                        has_bare_call(&executable_code, "eval")
                    } else if *label == "exec()" {
                        has_bare_call(&executable_code, "exec")
                    } else {
                        pattern.is_match(&executable_code)
                    }
                })
                .map(|(label, _)| *label)
                .or_else(|| {
                    let imported_method_call = self
                        .child_process_method_pattern
                        .captures(&executable_code)
                        .and_then(|captures| captures.get(1))
                        .is_some_and(|receiver| {
                            child_process_bindings
                                .iter()
                                .rev()
                                .find(|(name, _, _)| name == receiver.as_str())
                                .is_some_and(|(_, _, is_child_process)| *is_child_process)
                        });
                    (imported_method_call
                        || pattern_starts_in_code(
                            &execution_line,
                            &self.child_process_require_exec_pattern,
                        ))
                    .then_some("child_process.exec()")
                })
                .or_else(|| {
                    if is_php_source(file_path) && self.shell_exec_pattern.is_match(&executable_code)
                    {
                        Some("shell_exec()")
                    } else if is_system_call_source(file_path)
                        && has_system_call(
                            &executable_code,
                            &self.system_call_pattern,
                            &self.system_declaration_pattern,
                        )
                    {
                        Some("system()")
                    } else {
                        None
                    }
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

            brace_depth = update_brace_depth(brace_depth, &execution_code);
            child_process_bindings
                .retain(|(_, declared_depth, _)| *declared_depth <= brace_depth);
        }

        self.scan_prompt_injection(file_path, content, &mut findings);
        findings.sort_by_key(|finding| finding.line);
        findings
    }

    fn scan_prompt_injection(
        &self,
        file_path: &str,
        content: &str,
        findings: &mut Vec<SecretFinding>,
    ) {
        let code_context = build_code_context(content);
        let mut line_start = 0usize;
        for (index, segment) in content.split_inclusive('\n').enumerate() {
            let line = segment.strip_suffix('\n').unwrap_or(segment);
            let line = line.strip_suffix('\r').unwrap_or(line);
            let mut detected = false;

            for assignment in self.prompt_assignment_pattern.find_iter(line) {
                if !is_code_offset(&code_context, line_start + assignment.start()) {
                    continue;
                }
                let expression = &line[assignment.end()..];
                let has_code_plus = expression.match_indices('+').any(|(offset, _)| {
                    is_code_offset(&code_context, line_start + assignment.end() + offset)
                });
                detected = self.user_input_pattern.find_iter(expression).any(|user_input| {
                    let absolute_offset = line_start + assignment.end() + user_input.start();
                    (has_code_plus && is_code_offset(&code_context, absolute_offset))
                        || is_interpolated_user_input(
                            expression,
                            user_input.start(),
                            user_input.end(),
                        )
                });
                if detected {
                    break;
                }
            }

            if !detected {
                for content_field in self.user_message_content_pattern.find_iter(line) {
                    let separator = line_start + content_field.end() - 1;
                    if !is_code_offset(&code_context, separator) {
                        continue;
                    }
                    let (value_start, value_end) =
                        bounded_argument_value_span(line, content_field.end());
                    let value = &line[value_start..value_end];
                    detected = self.user_input_pattern.find_iter(value).any(|user_input| {
                        let absolute_offset = line_start + value_start + user_input.start();
                        is_code_offset(&code_context, absolute_offset)
                            || is_interpolated_user_input(
                                value,
                                user_input.start(),
                                user_input.end(),
                            )
                    });
                    if detected {
                        break;
                    }
                }
            }

            if detected {
                push_prompt_injection_finding(
                    findings,
                    file_path,
                    index + 1,
                    truncate_line_for_display(line),
                );
            }
            line_start += segment.len();
        }

        for assignment in self.prompt_assignment_pattern.find_iter(content) {
            if !is_code_offset(&code_context, assignment.start()) {
                continue;
            }
            let remaining = &content[assignment.end()..];
            let whitespace = remaining.len() - remaining.trim_start().len();
            let open_paren = assignment.end() + whitespace;
            if content.as_bytes().get(open_paren) != Some(&b'(') {
                continue;
            }
            let window = bounded_parenthesized_window(content, open_paren, 24);
            for user_input in self.user_input_pattern.find_iter(window) {
                let absolute_offset = open_paren + user_input.start();
                let is_concatenated_user = window.contains('+')
                    && is_code_offset(&code_context, absolute_offset);
                let is_interpolated_user = is_interpolated_user_input(
                    window,
                    user_input.start(),
                    user_input.end(),
                );
                if is_concatenated_user || is_interpolated_user {
                    let line = line_number_at_offset(content, absolute_offset);
                    push_prompt_injection_finding(
                        findings,
                        file_path,
                        line,
                        truncate_line_for_display(
                            content.lines().nth(line - 1).unwrap_or_default(),
                        ),
                    );
                    break;
                }
            }
        }

        for assignment in self.messages_assignment_pattern.find_iter(content) {
            if !is_code_offset(&code_context, assignment.start()) {
                continue;
            }
            let open_bracket = assignment.end() - 1;
            let window = bounded_delimited_window(content, open_bracket, '[', ']', 24);
            for content_field in self.user_message_content_pattern.find_iter(window) {
                let separator = open_bracket + content_field.end() - 1;
                if !is_code_offset(&code_context, separator) {
                    continue;
                }
                let (value_start, value_end) =
                    bounded_argument_value_span(window, content_field.end());
                let value = &window[value_start..value_end];
                if let Some(user_input) = self.user_input_pattern.find_iter(value).find(|matched| {
                    let absolute_offset = open_bracket + value_start + matched.start();
                    is_code_offset(&code_context, absolute_offset)
                        || is_interpolated_user_input(value, matched.start(), matched.end())
                }) {
                    let absolute_offset = open_bracket + value_start + user_input.start();
                    let line = line_number_at_offset(content, absolute_offset);
                    push_prompt_injection_finding(
                        findings,
                        file_path,
                        line,
                        truncate_line_for_display(
                            content.lines().nth(line - 1).unwrap_or_default(),
                        ),
                    );
                    break;
                }
            }
        }

        for llm_call in self.llm_call_pattern.find_iter(content) {
            if !is_code_offset(&code_context, llm_call.start()) {
                continue;
            }
            let open_paren = llm_call.end() - 1;
            let window = bounded_parenthesized_window(content, open_paren, 24);
            for argument in self.llm_prompt_argument_pattern.find_iter(window) {
                let separator = open_paren + argument.end() - 1;
                if !is_code_offset(&code_context, separator) {
                    continue;
                }
                let (value_start, value_end) =
                    bounded_argument_value_span(window, argument.end());
                let value = &window[value_start..value_end];
                if let Some(user_input) = self.user_input_pattern.find_iter(value).find(|matched| {
                    let absolute_offset = open_paren + value_start + matched.start();
                    is_code_offset(&code_context, absolute_offset)
                        || is_interpolated_user_input(value, matched.start(), matched.end())
                }) {
                    let absolute_offset = open_paren + value_start + user_input.start();
                    let line = line_number_at_offset(content, absolute_offset);
                    push_prompt_injection_finding(
                        findings,
                        file_path,
                        line,
                        truncate_line_for_display(
                            content.lines().nth(line - 1).unwrap_or_default(),
                        ),
                    );
                    break;
                }
            }
        }
    }
}

fn is_interpolated_user_input(window: &str, start: usize, end: usize) -> bool {
    let before = &window[..start];
    let after = &window[end..];
    let Some(open_brace) = before.rfind('{') else {
        return false;
    };
    let Some(close_brace) = after.find('}') else {
        return false;
    };
    if before[open_brace + 1..].contains('}') {
        return false;
    }

    let bytes = before.as_bytes();
    let is_javascript_interpolation = open_brace > 0
        && bytes[open_brace - 1] == b'$'
        && preceding_backslashes_are_even(bytes, open_brace - 1);
    if is_javascript_interpolation {
        return true;
    }

    let escaped_open = open_brace > 0 && bytes[open_brace - 1] == b'{';
    let escaped_close = after.as_bytes().get(close_brace + 1) == Some(&b'}');
    !escaped_open && !escaped_close && has_open_fstring(&before[..open_brace])
}

fn preceding_backslashes_are_even(bytes: &[u8], offset: usize) -> bool {
    let count = bytes[..offset]
        .iter()
        .rev()
        .take_while(|byte| **byte == b'\\')
        .count();
    count % 2 == 0
}

fn has_open_fstring(prefix: &str) -> bool {
    for (marker, delimiter) in [
        ("f\"\"\"", "\"\"\""),
        ("F\"\"\"", "\"\"\""),
        ("f'''", "'''"),
        ("F'''", "'''"),
        ("f\"", "\""),
        ("F\"", "\""),
        ("f'", "'"),
        ("F'", "'"),
    ] {
        if let Some(start) = prefix.rfind(marker) {
            let content_start = start + marker.len();
            if !prefix[content_start..].contains(delimiter) {
                return true;
            }
        }
    }
    false
}

fn push_prompt_injection_finding(
    findings: &mut Vec<SecretFinding>,
    file_path: &str,
    line: usize,
    matched_text: String,
) {
    if findings
        .iter()
        .any(|finding| finding.line == line && finding.kind == SecretKind::PromptInjection)
    {
        return;
    }
    findings.push(SecretFinding {
        file_path: file_path.to_string(),
        line,
        kind: SecretKind::PromptInjection,
        matched_text,
        level: 4,
    });
}

fn bounded_parenthesized_window(content: &str, open_paren: usize, max_lines: usize) -> &str {
    bounded_delimited_window(content, open_paren, '(', ')', max_lines)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DelimitedScanState {
    Code,
    Quoted(char),
    LineComment,
    BlockComment,
}

fn bounded_delimited_window(
    content: &str,
    open_offset: usize,
    open: char,
    close: char,
    max_lines: usize,
) -> &str {
    let tail = &content[open_offset..];
    let mut depth = 0usize;
    let mut lines = 0usize;
    let mut state = DelimitedScanState::Code;
    let mut chars = tail.char_indices().peekable();

    while let Some((offset, ch)) = chars.next() {
        if ch == '\n' {
            lines += 1;
            if lines >= max_lines {
                return &tail[..offset];
            }
            if state == DelimitedScanState::LineComment {
                state = DelimitedScanState::Code;
            }
        }

        match state {
            DelimitedScanState::Quoted(quote) => {
                if ch == '\\' {
                    if let Some((escaped_offset, escaped)) = chars.next() {
                        if escaped == '\n' {
                            lines += 1;
                            if lines >= max_lines {
                                return &tail[..escaped_offset];
                            }
                        }
                    }
                } else if ch == quote {
                    state = DelimitedScanState::Code;
                }
            }
            DelimitedScanState::LineComment => {}
            DelimitedScanState::BlockComment => {
                if ch == '*' && chars.peek().is_some_and(|(_, next)| *next == '/') {
                    chars.next();
                    state = DelimitedScanState::Code;
                }
            }
            DelimitedScanState::Code => {
                if matches!(ch, '\'' | '"' | '`') {
                    state = DelimitedScanState::Quoted(ch);
                } else if ch == '#'
                    || (ch == '/' && chars.peek().is_some_and(|(_, next)| *next == '/'))
                {
                    if ch == '/' {
                        chars.next();
                    }
                    state = DelimitedScanState::LineComment;
                } else if ch == '/'
                    && chars.peek().is_some_and(|(_, next)| *next == '*')
                {
                    chars.next();
                    state = DelimitedScanState::BlockComment;
                } else if ch == open {
                    depth += 1;
                } else if ch == close {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return &tail[..offset + ch.len_utf8()];
                    }
                }
            }
        }
    }
    tail
}

fn bounded_argument_value_span(input: &str, start: usize) -> (usize, usize) {
    let remaining = &input[start..];
    let whitespace = remaining.len() - remaining.trim_start().len();
    let value_start = start + whitespace;
    let value = &input[value_start..];
    let mut state = DelimitedScanState::Code;
    let mut depths = [0usize; 3];
    let mut chars = value.char_indices().peekable();

    while let Some((offset, ch)) = chars.next() {
        match state {
            DelimitedScanState::Quoted(quote) => {
                if ch == '\\' {
                    chars.next();
                } else if ch == quote {
                    state = DelimitedScanState::Code;
                }
            }
            DelimitedScanState::LineComment => {
                if ch == '\n' {
                    state = DelimitedScanState::Code;
                }
            }
            DelimitedScanState::BlockComment => {
                if ch == '*' && chars.peek().is_some_and(|(_, next)| *next == '/') {
                    chars.next();
                    state = DelimitedScanState::Code;
                }
            }
            DelimitedScanState::Code => {
                if matches!(ch, '\'' | '"' | '`') {
                    state = DelimitedScanState::Quoted(ch);
                } else if ch == '#'
                    || (ch == '/' && chars.peek().is_some_and(|(_, next)| *next == '/'))
                {
                    if ch == '/' {
                        chars.next();
                    }
                    state = DelimitedScanState::LineComment;
                } else if ch == '/'
                    && chars.peek().is_some_and(|(_, next)| *next == '*')
                {
                    chars.next();
                    state = DelimitedScanState::BlockComment;
                } else if ch == '(' {
                    depths[0] += 1;
                } else if ch == '[' {
                    depths[1] += 1;
                } else if ch == '{' {
                    depths[2] += 1;
                } else if ch == ')' {
                    if depths[0] == 0 && depths[1] == 0 && depths[2] == 0 {
                        return (value_start, value_start + offset);
                    }
                    depths[0] = depths[0].saturating_sub(1);
                } else if ch == ']' {
                    if depths[0] == 0 && depths[1] == 0 && depths[2] == 0 {
                        return (value_start, value_start + offset);
                    }
                    depths[1] = depths[1].saturating_sub(1);
                } else if ch == '}' {
                    if depths[0] == 0 && depths[1] == 0 && depths[2] == 0 {
                        return (value_start, value_start + offset);
                    }
                    depths[2] = depths[2].saturating_sub(1);
                } else if ch == ',' && depths == [0, 0, 0] {
                    return (value_start, value_start + offset);
                }
            }
        }
    }

    (value_start, input.len())
}

fn line_number_at_offset(content: &str, offset: usize) -> usize {
    content[..offset].bytes().filter(|byte| *byte == b'\n').count() + 1
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CodeContextState {
    Code,
    Quoted(u8),
    TripleQuoted(u8),
    LineComment,
    BlockComment,
}

fn build_code_context(content: &str) -> Vec<bool> {
    let bytes = content.as_bytes();
    let mut code = vec![false; bytes.len()];
    let mut state = CodeContextState::Code;
    let mut index = 0usize;

    while index < bytes.len() {
        match state {
            CodeContextState::Code => {
                let byte = bytes[index];
                let is_triple_quote = matches!(byte, b'\'' | b'"')
                    && bytes.get(index + 1) == Some(&byte)
                    && bytes.get(index + 2) == Some(&byte);
                if is_triple_quote {
                    state = CodeContextState::TripleQuoted(byte);
                    index += 3;
                } else if matches!(byte, b'\'' | b'"' | b'`') {
                    state = CodeContextState::Quoted(byte);
                    index += 1;
                } else if byte == b'#'
                    || (byte == b'/' && bytes.get(index + 1) == Some(&b'/'))
                {
                    state = CodeContextState::LineComment;
                    index += if byte == b'/' { 2 } else { 1 };
                } else if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
                    state = CodeContextState::BlockComment;
                    index += 2;
                } else {
                    code[index] = true;
                    index += 1;
                }
            }
            CodeContextState::Quoted(quote) => {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == quote {
                    state = CodeContextState::Code;
                    index += 1;
                } else {
                    index += 1;
                }
            }
            CodeContextState::TripleQuoted(quote) => {
                let closes_triple = bytes[index] == quote
                    && bytes.get(index + 1) == Some(&quote)
                    && bytes.get(index + 2) == Some(&quote);
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if closes_triple {
                    state = CodeContextState::Code;
                    index += 3;
                } else {
                    index += 1;
                }
            }
            CodeContextState::LineComment => {
                if bytes[index] == b'\n' {
                    state = CodeContextState::Code;
                }
                index += 1;
            }
            CodeContextState::BlockComment => {
                if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    state = CodeContextState::Code;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        }
    }

    code
}

fn is_code_offset(code_context: &[bool], offset: usize) -> bool {
    code_context.get(offset).copied().unwrap_or(false)
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
        SecretKind::PromptInjection => AI_001_EXPLANATION.to_string(),
    };
    let mut output = json!({
        "signal": {
            "description": description,
            "file_path": f.file_path,
            "line": f.line,
            "level": f.level,
            "affected_nodes": [],
        },
        "level": f.level,
    });
    if f.kind == SecretKind::PromptInjection {
        output["signal"]["rule_id"] = json!(AI_001_RULE_ID);
        output["signal"]["severity"] = json!(AI_001_SEVERITY);
        output["signal"]["plain_explanation"] = json!(AI_001_EXPLANATION);
    }
    output
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

fn strip_js_comments(line: &str, in_block_comment: &mut bool) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut quote = None;
    let mut index = 0;

    while index < chars.len() {
        if *in_block_comment {
            if index + 1 < chars.len() && chars[index] == '*' && chars[index + 1] == '/' {
                *in_block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }

        if let Some(active_quote) = quote {
            out.push(chars[index]);
            if chars[index] == '\\' && index + 1 < chars.len() {
                out.push(chars[index + 1]);
                index += 2;
                continue;
            }
            if chars[index] == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }

        if matches!(chars[index], '\'' | '"' | '`') {
            quote = Some(chars[index]);
            out.push(chars[index]);
            index += 1;
        } else if index + 1 < chars.len() && chars[index] == '/' && chars[index + 1] == '/' {
            break;
        } else if index + 1 < chars.len() && chars[index] == '/' && chars[index + 1] == '*' {
            *in_block_comment = true;
            index += 2;
        } else {
            out.push(chars[index]);
            index += 1;
        }
    }

    out
}

fn mask_string_literals(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote = None;
    let mut escaped = false;

    for ch in line.chars() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            out.push(' ');
        } else if matches!(ch, '\'' | '"' | '`') {
            quote = Some(ch);
            out.push(' ');
        } else {
            out.push(ch);
        }
    }

    out
}

fn normalize_string_concatenation(expression: &str) -> String {
    let chars: Vec<char> = expression.chars().collect();
    let mut normalized = String::with_capacity(expression.len());
    let mut index = 0;

    while index < chars.len() {
        if chars[index].is_whitespace() || chars[index] == '+' {
            index += 1;
            continue;
        }
        if matches!(chars[index], '\'' | '"') {
            let quote = chars[index];
            index += 1;
            while index < chars.len() && chars[index] != quote {
                if chars[index] == '\\' && index + 1 < chars.len() {
                    normalized.push(chars[index]);
                    index += 1;
                }
                normalized.push(chars[index]);
                index += 1;
            }
            index += usize::from(index < chars.len());
            continue;
        }
        if chars[index] == '_' || chars[index].is_ascii_alphabetic() {
            while index < chars.len()
                && (chars[index] == '_'
                    || chars[index] == '.'
                    || chars[index].is_ascii_alphanumeric())
            {
                index += 1;
            }
            normalized.push_str("{dynamic}");
            continue;
        }
        index += 1;
    }

    normalized
}

fn mask_literals_preserving_interpolations(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = vec![' '; chars.len()];
    let mut index = 0;

    while index < chars.len() {
        let quote = chars[index];
        if !matches!(quote, '\'' | '"' | '`') {
            out[index] = chars[index];
            index += 1;
            continue;
        }

        let python_fstring = quote != '`' && is_python_fstring_prefix(&chars, index);
        let delimiter_width = if quote != '`'
            && chars.get(index + 1) == Some(&quote)
            && chars.get(index + 2) == Some(&quote)
        {
            3
        } else {
            1
        };
        index += delimiter_width;
        while index < chars.len() {
            if chars[index] == '\\' {
                index = (index + 2).min(chars.len());
                continue;
            }
            let closes_literal = chars[index] == quote
                && (delimiter_width == 1
                    || (chars.get(index + 1) == Some(&quote)
                        && chars.get(index + 2) == Some(&quote)));
            if closes_literal {
                index += delimiter_width;
                break;
            }

            let expression_start = if quote == '`'
                && chars[index] == '$'
                && chars.get(index + 1) == Some(&'{')
            {
                Some(index + 2)
            } else if python_fstring
                && chars[index] == '{'
                && chars.get(index + 1) != Some(&'{')
            {
                Some(index + 1)
            } else {
                None
            };

            if let Some(start) = expression_start {
                index = copy_interpolation_expression(&chars, &mut out, start);
            } else if python_fstring
                && matches!(chars[index], '{' | '}')
                && chars.get(index + 1) == Some(&chars[index])
            {
                index += 2;
            } else {
                index += 1;
            }
        }
    }

    out.into_iter().collect()
}

fn is_python_fstring_prefix(chars: &[char], quote_index: usize) -> bool {
    let mut prefix_start = quote_index;
    while prefix_start > 0
        && quote_index - prefix_start < 2
        && chars[prefix_start - 1].is_ascii_alphabetic()
    {
        prefix_start -= 1;
    }
    let prefix: String = chars[prefix_start..quote_index]
        .iter()
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(prefix.as_str(), "f" | "fr" | "rf")
        && (prefix_start == 0
            || !matches!(chars[prefix_start - 1], '_' | 'a'..='z' | 'A'..='Z' | '0'..='9'))
}

fn copy_interpolation_expression(chars: &[char], out: &mut [char], start: usize) -> usize {
    let mut index = start;
    let mut depth = 1usize;

    while index < chars.len() {
        if chars[index] == '`' {
            index = copy_template_literal(chars, out, index);
            continue;
        }
        if matches!(chars[index], '\'' | '"') {
            let quote = chars[index];
            index += 1;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index = (index + 2).min(chars.len());
                } else if chars[index] == quote {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            continue;
        }

        match chars[index] {
            '{' => {
                depth += 1;
                out[index] = chars[index];
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
                out[index] = chars[index];
            }
            _ => out[index] = chars[index],
        }
        index += 1;
    }

    index
}

fn copy_template_literal(chars: &[char], out: &mut [char], start: usize) -> usize {
    let mut index = start + 1;
    while index < chars.len() {
        if chars[index] == '\\' {
            index = (index + 2).min(chars.len());
        } else if chars[index] == '`' {
            return index + 1;
        } else if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
            index = copy_interpolation_expression(chars, out, index + 2);
        } else {
            index += 1;
        }
    }
    index
}

fn pattern_starts_in_code(line: &str, pattern: &Regex) -> bool {
    pattern
        .find_iter(line)
        .any(|matched| byte_offset_is_code(line, matched.start()))
}

fn byte_offset_is_code(line: &str, target: usize) -> bool {
    let bytes = line.as_bytes();
    let mut quote = None;
    let mut escaped = false;

    for (index, byte) in bytes.iter().copied().enumerate() {
        if index == target {
            return quote.is_none();
        }
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
        }
    }

    target == bytes.len() && quote.is_none()
}

fn update_brace_depth(current: usize, code: &str) -> usize {
    code.bytes().fold(current, |depth, byte| match byte {
        b'{' => depth.saturating_add(1),
        b'}' => depth.saturating_sub(1),
        _ => depth,
    })
}

fn has_bare_call(line: &str, name: &str) -> bool {
    line.match_indices(name).any(|(start, _)| {
        let before = &line[..start];
        let after = &line[start + name.len()..];
        let immediate_prefix_is_identifier = before
            .chars()
            .next_back()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphanumeric());
        let receiver_is_method = before
            .chars()
            .rev()
            .find(|ch| !ch.is_whitespace())
            .is_some_and(|ch| ch == '.');
        !immediate_prefix_is_identifier
            && !receiver_is_method
            && after.trim_start().starts_with('(')
    })
}

fn has_system_call(line: &str, call_pattern: &Regex, declaration_pattern: &Regex) -> bool {
    let declaration_start = declaration_pattern
        .captures(line)
        .and_then(|captures| captures.name("declaration"))
        .map(|matched| matched.start());
    call_pattern
        .captures_iter(line)
        .filter_map(|captures| captures.name("call"))
        .any(|matched| Some(matched.start()) != declaration_start)
}

fn is_safe_fetch_credentials(name: &str, value: &str) -> bool {
    name.eq_ignore_ascii_case("credentials")
        && matches!(
            value.to_ascii_lowercase().as_str(),
            "omit" | "same-origin" | "same-site" | "include"
        )
}

fn is_environment_reference(value: &str) -> bool {
    let trimmed = value.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(trimmed);

    let variable = unquoted
        .strip_prefix("${")
        .and_then(|inner| inner.strip_suffix('}'))
        .or_else(|| unquoted.strip_prefix('$'));
    variable.is_some_and(is_environment_variable_name)
}

fn is_environment_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn supports_unquoted_secrets(file_path: &str) -> bool {
    let file_name = file_path.rsplit(['/', '\\']).next().unwrap_or(file_path);
    file_name == ".env" || file_name.ends_with(".env") || file_name.starts_with(".env.")
}

fn is_database_connection_context(line: &str) -> bool {
    let normalized = line.to_ascii_lowercase();
    normalized.contains("jdbc:")
        || ["server=", "data source=", "database="]
            .iter()
            .any(|marker| normalized.contains(marker))
}

fn is_php_source(file_path: &str) -> bool {
    file_path.to_ascii_lowercase().ends_with(".php")
}

fn is_system_call_source(file_path: &str) -> bool {
    let normalized = file_path.to_ascii_lowercase();
    [".php", ".c", ".h", ".cc", ".cpp", ".cxx", ".hpp"]
        .iter()
        .any(|extension| normalized.ends_with(extension))
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

    #[test]
    fn p1_5_detects_unquoted_env_secrets() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "env_no_quotes.env",
            "DATABASE_PASSWORD=SuperSecret123\nAPI_KEY=abcdefghijklmnop\nSECRET=my_hard_coded_secret\nTOKEN=ghp_realtoken12345678",
        );
        let mut lines: Vec<_> = findings
            .iter()
            .filter(|f| f.kind == SecretKind::HardcodedAssignment)
            .map(|f| f.line)
            .collect();
        lines.sort_unstable();
        lines.dedup();
        assert_eq!(lines, vec![1, 2, 3, 4]);
    }

    #[test]
    fn p1_5_detects_each_connection_string_password() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "connection_strings.cs",
            r#"var conn = "Server=prod.db.com;Database=app;User Id=admin;Password=Pr0dP@ssw0rd!;";
var jdbc = "jdbc:mysql://localhost:3306/mydb?user=root&password=rootpassword";
var mongo = "mongodb://appuser:S3cr3tP@ss@cluster.example.com:27017/mydb";"#,
        );
        let mut lines: Vec<_> = findings
            .iter()
            .filter(|f| f.kind == SecretKind::HardcodedAssignment)
            .map(|f| f.line)
            .collect();
        lines.sort_unstable();
        lines.dedup();
        assert_eq!(lines, vec![1, 2, 3]);
    }

    #[test]
    fn p1_5_detects_each_python_command_execution_api() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "dangerous_exec_python.py",
            "result = os.popen(user_input).read()\nout = subprocess.getoutput(f\"ls {user_dir}\")",
        );
        assert!(findings.iter().any(|f| {
            matches!(f.kind, SecretKind::DangerousExecution("os.popen()")) && f.line == 1
        }));
        assert!(findings.iter().any(|f| {
            matches!(
                f.kind,
                SecretKind::DangerousExecution("subprocess.getoutput()")
            ) && f.line == 2
        }));
    }

    #[test]
    fn p1_5_detects_php_and_c_system_calls_with_language_context() {
        let scanner = scanner();
        let php = scanner.scan_content(
            "dangerous_exec.php",
            "<?php\n$result = system($_GET['cmd']);\n$out = shell_exec($userInput);",
        );
        assert!(php.iter().any(|f| {
            matches!(f.kind, SecretKind::DangerousExecution("system()")) && f.line == 2
        }));
        assert!(php.iter().any(|f| {
            matches!(f.kind, SecretKind::DangerousExecution("shell_exec()")) && f.line == 3
        }));

        let c = scanner.scan_content("dangerous_exec.c", "int rc = system(user_input);");
        assert!(c
            .iter()
            .any(|f| matches!(f.kind, SecretKind::DangerousExecution("system()"))));

        assert!(scanner
            .scan_content("business.py", "result = system(user_input)")
            .is_empty());
    }

    #[test]
    fn p1_5_detects_each_hex_secret_assignment() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "hex_key.py",
            "API_KEY = \"a3f2b1c4d5e6f7a8b9c0d1e2f3a4b5c6\"\nSECRET_TOKEN = \"deadbeefcafebabe1234567890abcdef\"",
        );
        let mut lines: Vec<_> = findings.iter().map(|f| f.line).collect();
        lines.sort_unstable();
        lines.dedup();
        assert_eq!(lines, vec![1, 2]);
    }

    #[test]
    fn review_spaced_method_calls_are_not_treated_as_bare_execution() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "methods.py",
            "model . eval()\npattern . exec(text)",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn review_child_process_context_ignores_comments_strings_and_rebinding() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "regex_exec.js",
            r#"const note = "const cp = require('child_process')";
/*
const blocked = require('child_process');
*/
cp.exec(text);
blocked.exec(text);
const actual = require('node:child_process');
actual.exec(userInput);
actual = /safe/g;
actual.exec(text);"#,
        );
        let exec_lines: Vec<_> = findings
            .iter()
            .filter(|f| {
                matches!(
                    f.kind,
                    SecretKind::DangerousExecution("child_process.exec()")
                )
            })
            .map(|f| f.line)
            .collect();
        assert_eq!(exec_lines, vec![8]);
    }

    #[test]
    fn review_child_process_direct_require_inside_string_is_not_execution() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "documentation.js",
            r#"const example = "require('child_process').exec(userInput)";"#,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn review_child_process_alias_does_not_escape_block_scope() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "regex_exec.js",
            r#"{
  const cp = require('child_process');
  cp.exec(userInput);
}
cp.exec(text);"#,
        );
        let exec_lines: Vec<_> = findings
            .iter()
            .filter(|f| {
                matches!(
                    f.kind,
                    SecretKind::DangerousExecution("child_process.exec()")
                )
            })
            .map(|f| f.line)
            .collect();
        assert_eq!(exec_lines, vec![3]);
    }

    #[test]
    fn review_child_process_alias_respects_inner_shadowing() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "regex_exec.js",
            r#"const cp = require('child_process');
{
  const cp = /safe/g;
  cp.exec(text);
}
cp.exec(userInput);"#,
        );
        let exec_lines: Vec<_> = findings
            .iter()
            .filter(|f| {
                matches!(
                    f.kind,
                    SecretKind::DangerousExecution("child_process.exec()")
                )
            })
            .map(|f| f.line)
            .collect();
        assert_eq!(exec_lines, vec![6]);
    }

    #[test]
    fn review_select_from_ui_copy_is_not_sql_injection() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "ui_text.ts",
            r#"const msg = "Select a plan from " + catalog;"#,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn review_detects_azure_storage_account_key() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "azure_connection_string.cs",
            r#"var conn = "DefaultEndpointsProtocol=https;AccountName=prod;AccountKey=Zm9vYmFyYmF6cXV4MTIzNDU2Nzg5MA==;";"#,
        );
        assert!(findings
            .iter()
            .any(|f| f.kind == SecretKind::HardcodedAssignment));
    }

    #[test]
    fn review_detects_unquoted_hex_secret_outside_env_files() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "config.yml",
            "secret_token: deadbeefcafebabe1234567890abcdef",
        );
        assert!(findings
            .iter()
            .any(|f| f.kind == SecretKind::HardcodedAssignment));
    }

    #[test]
    fn review_c_system_declaration_is_not_a_call() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "stdlib.h",
            "int system(const char *command);",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn review_env_secret_before_inline_comment_is_detected() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            ".env.production",
            "PASSWORD=SuperSecret123 # deployed",
        );
        assert!(findings
            .iter()
            .any(|f| f.kind == SecretKind::HardcodedAssignment));
    }

    #[test]
    fn review_business_token_and_secret_values_are_not_credentials() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "lexer.ts",
            "const token = \"identifier\";\nconst secret = \"treasure-map\";",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn review_detects_dynamic_sql_with_common_select_and_update_grammar() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "queries.py",
            r#"query = f"SELECT count(*) FROM users WHERE role={role}"
query = f"SELECT id AS user_id FROM users WHERE id={user_id}"
query = f"SELECT {column} FROM users"
query = f"SELECT * FROM {table} WHERE id={user_id}"
query = f"UPDATE users AS u SET role={role} WHERE id={user_id}""#,
        );
        let sql_lines: Vec<_> = findings
            .iter()
            .filter(|finding| finding.kind == SecretKind::SqlInjection)
            .map(|finding| finding.line)
            .collect();
        assert_eq!(sql_lines, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn review_detects_execution_inside_template_and_fstring_expressions() {
        let scanner = scanner();
        let javascript = scanner.scan_content(
            "template.js",
            r#"const evaluated = `${eval(userInput)}`;
const executed = `${child_process.exec(userInput)}`;
const inert = `eval(userInput)`;
const escaped = `\${eval(userInput)}`;"#,
        );
        let javascript_exec_lines: Vec<_> = javascript
            .iter()
            .filter(|finding| matches!(finding.kind, SecretKind::DangerousExecution(_)))
            .map(|finding| finding.line)
            .collect();
        assert_eq!(javascript_exec_lines, vec![1, 2]);

        let python = scanner.scan_content(
            "template.py",
            "evaluated = f\"{eval(user_input)}\"\ninert = \"{eval(user_input)}\"",
        );
        let python_exec_lines: Vec<_> = python
            .iter()
            .filter(|finding| matches!(finding.kind, SecretKind::DangerousExecution("eval()")))
            .map(|finding| finding.line)
            .collect();
        assert_eq!(python_exec_lines, vec![1]);
    }

    #[test]
    fn review_env_variable_references_are_not_hardcoded_secrets() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            ".env",
            "DATABASE_PASSWORD=${DB_PASSWORD}\nAPI_KEY=$API_KEY\nDATABASE_PASSWORD=\"${DB_PASSWORD}\"",
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn review_system_function_definitions_are_not_calls() {
        let scanner = scanner();
        let c = scanner.scan_content(
            "compat.c",
            "int system(const char *command) { return 0; }",
        );
        assert!(c.is_empty());

        let php = scanner.scan_content(
            "compat.php",
            "function system($command) { return 0; }",
        );
        assert!(php.is_empty());
    }

    #[test]
    fn review_detects_sql_across_multi_operand_concatenation() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "queries.py",
            r#"query = "SELECT * FROM " + table + " WHERE id = " + user_id
query = "SELECT " + column + " FROM users""#,
        );
        let sql_lines: Vec<_> = findings
            .iter()
            .filter(|finding| finding.kind == SecretKind::SqlInjection)
            .map(|finding| finding.line)
            .collect();
        assert_eq!(sql_lines, vec![1, 2]);
    }

    #[test]
    fn review_dynamic_ui_copy_is_not_sql_injection() {
        let scanner = scanner();
        let findings = scanner.scan_content(
            "ui_text.py",
            r#"label = f"Select {item} from menu""#,
        );
        assert!(findings.is_empty());
    }

    #[test]
    fn review_detects_execution_in_triple_fstrings_and_nested_templates() {
        let scanner = scanner();
        let python = scanner.scan_content(
            "template.py",
            r#"query = f"""{eval(user_input)}""""#,
        );
        assert!(python
            .iter()
            .any(|finding| matches!(finding.kind, SecretKind::DangerousExecution("eval()"))));

        let javascript = scanner.scan_content(
            "template.js",
            r#"const value = `${`nested ${eval(userInput)}`}`;"#,
        );
        assert!(javascript
            .iter()
            .any(|finding| matches!(finding.kind, SecretKind::DangerousExecution("eval()"))));
    }

    #[test]
    fn review_system_declaration_does_not_hide_later_calls_on_the_same_line() {
        let scanner = scanner();
        let c = scanner.scan_content(
            "compat.c",
            "int system(const char *command); int rc = system(user_input);\nint system(const char *command) { return system(command); }",
        );
        let c_lines: Vec<_> = c
            .iter()
            .filter(|finding| matches!(finding.kind, SecretKind::DangerousExecution("system()")))
            .map(|finding| finding.line)
            .collect();
        assert_eq!(c_lines, vec![1, 2]);

        let php = scanner.scan_content(
            "compat.php",
            r#"function system($command) { return \system($command); }"#,
        );
        assert!(php
            .iter()
            .any(|finding| matches!(finding.kind, SecretKind::DangerousExecution("system()"))));
    }

    #[test]
    fn review_literal_environment_syntax_outside_shell_expansion_is_flagged() {
        let scanner = scanner();
        let env = scanner.scan_content(".env", "PASSWORD='$DB_PASSWORD'");
        assert!(env
            .iter()
            .any(|finding| finding.kind == SecretKind::HardcodedAssignment));

        let javascript = scanner.scan_content(
            "config.js",
            "const password = '$DB_PASSWORD';\nconst api_key = \"${API_KEY}\";",
        );
        let assignment_lines: Vec<_> = javascript
            .iter()
            .filter(|finding| finding.kind == SecretKind::HardcodedAssignment)
            .map(|finding| finding.line)
            .collect();
        assert_eq!(assignment_lines, vec![1, 2]);
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
    fn detects_multiline_openai_call_with_direct_user_message() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"response = openai.chat.completions.create(
    model="gpt-4",
    messages=[{"role": "user", "content": user_message}]
)"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 3);
    }

    #[test]
    fn detects_client_chat_completion_with_direct_user_message() {
        let findings = scanner().scan_content(
            "chat.py",
            "response = client.chat.completions.create(messages=[user_message])",
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn detects_preconstructed_messages_with_direct_request_body() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"messages = [{"role": "user", "content": req.body.message}]"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn detects_multiline_preconstructed_messages_with_direct_request_body() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"messages = [
    {"role": "user",
     "content": req.body.message}
]
response = client.chat.completions.create(messages=messages)"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 3);
    }

    #[test]
    fn llm_call_window_ignores_parentheses_inside_strings() {
        let balanced = scanner().scan_content(
            "chat.py",
            r#"client.chat.completions.create(system="Answer (briefly)", messages=[user_message])"#,
        );
        assert_eq!(balanced.len(), 1);
        assert_eq!(balanced[0].kind, SecretKind::PromptInjection);

        let unmatched_close = scanner().scan_content(
            "chat.py",
            r#"client.chat.completions.create(system="Answer ) briefly", messages=[user_message])"#,
        );
        assert_eq!(unmatched_close.len(), 1);
        assert_eq!(unmatched_close[0].kind, SecretKind::PromptInjection);
    }

    #[test]
    fn detects_interpolated_user_input_in_preconstructed_messages() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"messages = [{"content": f"Answer: {user_input}"}]"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn detects_prompt_concatenated_with_user_input() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"prompt = "You are helpful. Answer: " + user_input"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn ignores_prompt_assignment_in_line_comment() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"# prompt = "x" + user_input"#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_prompt_comment_with_user_input_and_legacy_plus() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"prompt = "safe"  # user_input + legacy"#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_comment_user_input_after_sanitized_message_content() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"messages = [{"role": "user", "content": sanitized}]  # user_input"#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_llm_call_in_python_triple_quoted_docstring() {
        let findings = scanner().scan_content(
            "chat.py",
            r#""""Example only:
client.chat.completions.create(messages=[user_input])
""""#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_llm_call_in_javascript_block_comment() {
        let findings = scanner().scan_content(
            "chat.js",
            r#"/*
client.chat.completions.create(messages=[user_input]);
*/"#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_openai_audio_transcription_with_user_file_data() {
        let findings = scanner().scan_content(
            "audio.py",
            "openai.audio.transcriptions.create(file=user_input_data)",
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_openai_file_upload_with_user_file_data() {
        let findings = scanner().scan_content(
            "files.py",
            "openai.files.create(file=user_input_data)",
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn ignores_user_input_in_non_prompt_api_parameter() {
        let findings = scanner().scan_content(
            "chat.py",
            "client.chat.completions.create(messages=safe_messages, user=user_input_id)",
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn detects_valid_api_message_after_invalid_user_input_matches() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"client.chat.completions.create(
    note="user_input is documented",
    # user_input is sanitized elsewhere
    messages=[user_input]
)"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 4);
    }

    #[test]
    fn detects_multiline_prompt_concatenation() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"prompt = (
    "Answer: "
    + user_input
)
client.chat.completions.create(prompt=prompt)"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 3);
    }

    #[test]
    fn detects_multiline_python_fstring_prompt() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"prompt = (
    f"Answer: {user_input}"
)"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 2);
    }

    #[test]
    fn ignores_user_input_mentioned_in_comment_after_static_fstring() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"prompt = (
    f"Static text"
    # user_input is sanitized elsewhere
)"#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn detects_valid_interpolation_after_user_input_mentioned_in_comment() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"prompt = (
    # user_input is untrusted
    f"Answer: {user_input}"
)"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 3);
    }

    #[test]
    fn ignores_user_input_text_outside_fstring_interpolation() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"prompt = (
    f"Static text"
    "Document user_input as a variable name"
)"#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn detects_multiline_javascript_template_prompt() {
        let findings = scanner().scan_content(
            "chat.js",
            r#"const prompt = (
    `Answer: ${user_input}`
);"#,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 2);
    }

    #[test]
    fn detects_openai_completions_prompt_with_direct_user_input() {
        let findings = scanner().scan_content(
            "chat.py",
            "openai.completions.create(prompt=user_input)",
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, SecretKind::PromptInjection);
        assert_eq!(findings[0].line, 1);
    }

    #[test]
    fn ignores_sanitized_user_message_in_llm_call() {
        let findings = scanner().scan_content(
            "chat.py",
            r#"sanitized = user_message[:500].replace("<", "").replace(">", "")
response = openai.chat.completions.create(
    model="gpt-4",
    messages=[
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": sanitized}
    ]
)"#,
        );

        assert!(findings.is_empty());
    }

    #[test]
    fn finding_to_signal_carries_ai_001_metadata() {
        const EXPLANATION: &str = "用户输入未经过滤直接拼入 LLM prompt（Prompt Injection 风险）。攻击者可通过构造输入操控模型行为，泄露系统提示或执行越权操作。应对用户输入进行长度限制、特殊字符转义，并将系统提示与用户输入严格分离。";
        let findings = scanner().scan_content(
            "chat.py",
            r#"prompt = "You are helpful. Answer: " + user_input"#,
        );

        assert_eq!(findings.len(), 1);
        let signal = finding_to_signal(&findings[0]);
        assert_eq!(signal["level"], 4);
        assert_eq!(signal["signal"]["level"], 4);
        assert_eq!(signal["signal"]["rule_id"], "AI-001");
        assert_eq!(signal["signal"]["severity"], "high");
        assert_eq!(signal["signal"]["plain_explanation"], EXPLANATION);
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
