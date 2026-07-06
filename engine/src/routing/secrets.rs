/// Secret scanning engine.
///
/// Three detection layers (applied to each line of each changed file):
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
/// Output signals use level 5 (critical) for definite known-prefix hits,
/// level 4 (high) for entropy and assignment pattern hits.
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
}

pub struct SecretScanner {
    /// Each tuple: (display label, regex matching the secret value)
    known_prefixes: Vec<(&'static str, Regex)>,
    /// Matches: variable_name = "some_value" or variable_name: "some_value"
    assignment_pattern: Regex,
    /// Sensitive variable name keywords
    sensitive_var_names: Regex,
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
            r#"(?i)(api_?key|api_?secret|app_?secret|auth_?token|access_?token|secret_?key|private_?key|client_?secret|db_?pass(word)?|database_?pass(word)?|password|passwd|credentials?|auth_?key)\s*[=:]\s*["'][^"']{8,}["']"#,
        ).unwrap();

        let sensitive_var_names = Regex::new(
            r#"(?i)(api_?key|api_?secret|app_?secret|auth_?token|access_?token|secret_?key|private_?key|client_?secret|db_?pass|database_?pass|password|passwd|credentials?)"#,
        ).unwrap();

        Self {
            known_prefixes,
            assignment_pattern,
            sensitive_var_names,
        }
    }

    /// Scan the contents of a single file.
    /// Returns all findings in line order.
    pub fn scan_content(&self, file_path: &str, content: &str) -> Vec<SecretFinding> {
        let mut findings = Vec::new();

        for (line_number, line) in content.lines().enumerate() {
            let line_number = line_number + 1; // 1-based

            // Skip comment-only lines (best effort — handles // # and SQL --)
            let trimmed = line.trim();
            if trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with("-- ")  // SQL comment: must have space after --
                || trimmed == "--"              // bare SQL comment
                || trimmed.starts_with('*')
            {
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
            if self.assignment_pattern.is_match(line) {
                let already_flagged = findings.iter().any(|f| f.line == line_number);
                if !already_flagged {
                    let var_match = self
                        .sensitive_var_names
                        .find(line)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_else(|| "secret variable".to_string());
                    findings.push(SecretFinding {
                        file_path: file_path.to_string(),
                        line: line_number,
                        kind: SecretKind::HardcodedAssignment,
                        matched_text: var_match,
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

// ─── Helpers ─────────────────────────────────────────────────────────────────

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
                results.push(chars[start..j].iter().collect());
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
