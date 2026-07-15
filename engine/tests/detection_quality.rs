//! Detection quality baseline — measures the secret/injection/exec scanner
//! against a fixed corpus so recall and false-positive rate are *numbers*,
//! not vibes, and can't silently regress.
//!
//! Corpus layout (engine/tests/detection_corpus/):
//!   bad/   — every file MUST produce ≥1 finding (measures recall)
//!   clean/ — every file MUST produce 0 findings (measures false positives)
//!   gaps/  — real risks a regex scanner likely misses; informational only,
//!            printed so the coverage boundary stays honest.
//!
//! Run just this with `cargo test --test detection_quality -- --nocapture`
//! to see the full report.

use std::fs;
use std::path::{Path, PathBuf};

use hologram_engine::routing::secrets::SecretScanner;

fn corpus_dir(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/detection_corpus")
        .join(sub)
}

/// A few `bad/` fixtures would themselves be provider-verifiable secrets on
/// disk (Slack webhook/token, Stripe key), which trips GitHub push protection
/// on every push. We instead commit a `__CORPUS_SECRET__` placeholder and
/// reassemble the real trigger value here from fragments — so nothing scannable
/// is ever committed, yet the scanner is still tested against the real shape.
fn inject_secret(name: &str, content: &str) -> String {
    let real = match name {
        "secret_slack_webhook.js" => Some(format!(
            "https://hooks.slack.com/services/{}/{}/{}",
            "T0AAAA1BB", "B0CCCC2DD", "XxYyZz123456AaBbCc789012"
        )),
        "secret_slack_token.py" => Some(format!(
            "{}-{}-{}-{}",
            "xoxb", "1234567890123", "1234567890123", "AbCdEfGhIjKlMnOpQrStUvWx"
        )),
        "secret_stripe_live.js" => Some(format!("{}{}", "sk_live_", "51NxYzABCdefGHIjklMNOpqr98765")),
        _ => None,
    };
    match real {
        Some(v) => content.replace("__CORPUS_SECRET__", &v),
        None => content.to_string(),
    }
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read corpus dir {}: {e}", dir.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == ".hologram") {
                continue;
            }
            collect_files(&path, files);
        } else if path.is_file() {
            files.push(path);
        }
    }
}

/// (relative_path, finding_count) for every file in a corpus subdir, sorted by path.
fn scan_dir(sub: &str) -> Vec<(String, usize)> {
    let scanner = SecretScanner::new();
    let dir = corpus_dir(sub);
    let mut out = Vec::new();
    let mut entries = Vec::new();
    collect_files(&dir, &mut entries);
    entries.sort();
    for path in entries {
        let name = path
            .strip_prefix(&dir)
            .expect("corpus entry must be under its root")
            .to_string_lossy()
            .replace('\\', "/");
        let content = inject_secret(&name, &fs::read_to_string(&path).unwrap_or_default());
        let findings = scanner.scan_content(&name, &content);
        out.push((name, findings.len()));
    }
    out
}

#[test]
fn detection_quality_report() {
    let bad = scan_dir("bad");
    let clean = scan_dir("clean");
    let gaps = scan_dir("gaps");

    let bad_hit = bad.iter().filter(|(_, n)| *n > 0).count();
    let bad_total = bad.len();
    let clean_fp = clean.iter().filter(|(_, n)| *n > 0).count();
    let clean_total = clean.len();
    let gaps_hit = gaps.iter().filter(|(_, n)| *n > 0).count();
    let gaps_total = gaps.len();

    let recall = 100.0 * bad_hit as f64 / bad_total as f64;
    let fp_rate = 100.0 * clean_fp as f64 / clean_total as f64;

    println!("\n════════ 检测质量基线 ════════");
    println!("召回率 (bad):        {bad_hit}/{bad_total} = {recall:.1}%  （越高越好）");
    println!("误报率 (clean):      {clean_fp}/{clean_total} = {fp_rate:.1}%  （越低越好，目标 0）");
    println!("覆盖边界 (gaps):     {gaps_hit}/{gaps_total} 被捕获  （已知盲区，纯参考）");

    println!("\n-- bad 漏报（应检出却没检出）--");
    for (name, _) in bad.iter().filter(|(_, n)| *n == 0) {
        println!("  ✗ MISS  {name}");
    }
    println!("\n-- clean 误报（不该报却报了）--");
    for (name, n) in clean.iter().filter(|(_, n)| *n > 0) {
        println!("  ✗ FALSE+ {name}  ({n} findings)");
    }
    println!("\n-- gaps 覆盖情况 --");
    for (name, n) in &gaps {
        let mark = if *n > 0 { "✓ 捕获" } else { "· 盲区" };
        println!("  {mark}  {name}");
    }
    println!("════════════════════════════\n");

    // Regression guards — locked at the current measured baseline so any change
    // that weakens detection or adds noise fails CI. Update deliberately, in the
    // same commit as a corpus or scanner change.
    //   bad/   → 100% recall (every must-catch sample is caught)
    //   clean/ → 0 false positives
    // gaps/ is intentionally NOT gated: it tracks known blind spots (semantic
    // dataflow injection, cloud connection strings, weak crypto, …). When the
    // scanner grows to cover one, move that file from gaps/ into bad/.
    assert!(
        clean_fp == 0,
        "clean 语料出现误报：{clean_fp}/{clean_total}。安全工具误报是致命伤，请修规则或调整语料。"
    );
    assert!(
        bad_hit == bad_total,
        "bad 召回率退化：{bad_hit}/{bad_total}（{recall:.1}%），基线要求 100%。有必检出样本被漏掉。"
    );
}
