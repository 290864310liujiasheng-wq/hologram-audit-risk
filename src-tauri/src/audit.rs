// v4 Phase 5 — 审计日志：每次文件/Git/Shell 操作留痕
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// One audit record.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: String,
    pub tool: String,
    pub target_path: String,
    pub action: String,   // "allowed" | "denied" | "user_approved" | "user_denied"
    pub reason: String,
    pub details: Option<serde_json::Value>,
}

/// Append-only JSONL audit logger.
pub struct AuditLogger {
    log_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAuditEntry {
    ts: String,
    tool: String,
    path: String,
    action: String,
    reason: String,
    details: Option<serde_json::Value>,
    prev_hash: Option<String>,
    integrity_hash: String,
}

#[derive(Serialize)]
struct AuditIntegrityPayload<'a> {
    ts: &'a str,
    tool: &'a str,
    path: &'a str,
    action: &'a str,
    reason: &'a str,
    details: &'a Option<serde_json::Value>,
    prev_hash: &'a Option<String>,
}

impl AuditLogger {
    pub fn new(project_root: &std::path::Path) -> Self {
        let log_dir = project_root.join(".hologram");
        let _ = fs::create_dir_all(&log_dir);
        Self {
            log_path: log_dir.join("audit.jsonl"),
        }
    }

    /// Append an audit entry.
    pub fn log(&self, entry: &AuditEntry) {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&self.log_path) {
            let prev_hash = self.previous_hash_anchor();
            let mut stored = StoredAuditEntry {
                ts: entry.timestamp.clone(),
                tool: entry.tool.clone(),
                path: entry.target_path.clone(),
                action: entry.action.clone(),
                reason: entry.reason.clone(),
                details: entry.details.clone(),
                prev_hash,
                integrity_hash: String::new(),
            };
            stored.integrity_hash = compute_integrity_hash(&stored);
            if let Ok(line) = serde_json::to_string(&stored) {
                let _ = writeln!(f, "{line}");
            }
        }
    }

    /// Read recent entries (for frontend audit panel).
    #[allow(dead_code)]
    pub fn recent(&self, limit: usize) -> Vec<String> {
        let content = fs::read_to_string(&self.log_path).unwrap_or_default();
        let lines: Vec<&str> = content.lines().collect();
        let start = if lines.len() > limit { lines.len() - limit } else { 0 };
        lines[start..].iter().map(|s| s.to_string()).collect()
    }

    pub fn recent_json(&self, limit: usize) -> Vec<serde_json::Value> {
        self.recent(limit)
            .into_iter()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
            .collect()
    }

    fn previous_hash_anchor(&self) -> Option<String> {
        let line = fs::read_to_string(&self.log_path)
            .ok()?
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())?
            .trim()
            .to_string();

        serde_json::from_str::<StoredAuditEntry>(&line)
            .ok()
            .map(|entry| entry.integrity_hash)
            .or_else(|| Some(hash_text(&line)))
    }
}

/// Helper to build a timestamp string.
pub fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn compute_integrity_hash(entry: &StoredAuditEntry) -> String {
    let payload = AuditIntegrityPayload {
        ts: &entry.ts,
        tool: &entry.tool,
        path: &entry.path,
        action: &entry.action,
        reason: &entry.reason,
        details: &entry.details,
        prev_hash: &entry.prev_hash,
    };
    let serialized = serde_json::to_string(&payload).unwrap_or_default();
    hash_text(&serialized)
}

fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{compute_integrity_hash as compute_entry_integrity_hash, hash_text, AuditEntry, AuditLogger, StoredAuditEntry};
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn log_writes_prev_hash_and_integrity_hash_for_chained_entries() {
        let root = unique_test_root("chain");
        let logger = AuditLogger::new(&root);

        logger.log(&sample_entry("2026-06-27T00:00:00Z", "review_check", "first"));
        logger.log(&sample_entry("2026-06-27T00:01:00Z", "repair_apply", "second"));

        let lines = read_audit_lines(&root);
        assert_eq!(lines.len(), 2);

        let first = parse_line(&lines[0]);
        let second = parse_line(&lines[1]);

        assert!(first.prev_hash.is_none(), "first entry should not point to a previous hash");
        assert_eq!(second.prev_hash.as_deref(), Some(first.integrity_hash.as_str()));
        assert_eq!(first.integrity_hash, compute_integrity_hash(&first));
        assert_eq!(second.integrity_hash, compute_integrity_hash(&second));

        cleanup(&root);
    }

    #[test]
    fn log_anchors_new_chain_entries_to_legacy_lines() {
        let root = unique_test_root("legacy-anchor");
        let log_dir = root.join(".hologram");
        fs::create_dir_all(&log_dir).expect("create log dir");
        let legacy_line = r#"{"ts":"2026-06-27T00:00:00Z","tool":"review_check","path":"/tmp/workspace","action":"denied","reason":"legacy entry","details":{"timestamp":"2026-06-27T00:00:00Z"}}"#;
        fs::write(log_dir.join("audit.jsonl"), format!("{legacy_line}\n")).expect("write legacy audit log");

        let logger = AuditLogger::new(&root);
        logger.log(&sample_entry("2026-06-27T00:01:00Z", "repair_apply", "second"));

        let lines = read_audit_lines(&root);
        assert_eq!(lines.len(), 2);

        let second = parse_line(&lines[1]);
        let expected_prev_hash = hash_text(lines[0].as_str());
        let expected_integrity_hash = compute_integrity_hash(&second);
        assert_eq!(second.prev_hash.as_deref(), Some(expected_prev_hash.as_str()));
        assert_eq!(second.integrity_hash, expected_integrity_hash);

        cleanup(&root);
    }

    #[test]
    fn tampering_with_a_logged_entry_breaks_its_integrity_hash() {
        let root = unique_test_root("tamper");
        let logger = AuditLogger::new(&root);

        logger.log(&sample_entry("2026-06-27T00:00:00Z", "review_check", "original"));

        let log_path = root.join(".hologram").join("audit.jsonl");
        let original = fs::read_to_string(&log_path).expect("read audit log");
        let tampered = original.replace("original", "tampered");
        fs::write(&log_path, tampered).expect("write tampered audit log");

        let line = read_audit_lines(&root).pop().expect("tampered line");
        let parsed = parse_line(&line);
        let expected_integrity_hash = compute_integrity_hash(&parsed);
        assert_ne!(parsed.integrity_hash, expected_integrity_hash);

        cleanup(&root);
    }

    fn sample_entry(timestamp: &str, tool: &str, reason: &str) -> AuditEntry {
        AuditEntry {
            timestamp: timestamp.to_string(),
            tool: tool.to_string(),
            target_path: "/tmp/workspace".to_string(),
            action: "denied".to_string(),
            reason: reason.to_string(),
            details: Some(serde_json::json!({
                "timestamp": timestamp,
                "finding_ids": ["finding-1"],
            })),
        }
    }

    fn read_audit_lines(root: &Path) -> Vec<String> {
        fs::read_to_string(root.join(".hologram").join("audit.jsonl"))
            .expect("read audit lines")
            .lines()
            .map(|line| line.to_string())
            .collect()
    }

    fn parse_line(line: &str) -> StoredAuditEntry {
        serde_json::from_str(line).expect("parse audit json")
    }

    fn compute_integrity_hash(entry: &StoredAuditEntry) -> String {
        compute_entry_integrity_hash(entry)
    }

    fn unique_test_root(label: &str) -> std::path::PathBuf {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix time")
            .as_nanos();
        std::env::temp_dir().join(format!("audit-risk-audit-{label}-{now}-{id}"))
    }

    fn cleanup(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }
}
