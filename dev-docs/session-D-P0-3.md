# 会话D：P0-3（审计链完整性）

**目标文件：`engine/src/cli.rs`**  
**⚠️ 在会话A/B/C全部合并到 main 之后再开本会话**  
**这是剩余任务里最复杂的一条**

---

## 背景

README 宣称"不可篡改的审计日志"，但当前实现有三个根本性缺陷叠加：

1. **完整性哈希无密钥**：用裸 SHA-256 链接，任何人改完记录后重算链即可，`verify`/`report` 照样报 verified。
2. **无哈希记录被放行**：legacy 判断让无 `integrity_hash` 字段的记录直接跳过校验。
3. **Rust 写入端根本没写哈希**：`append_repair_audit_event` 等函数写出的记录完全没有 `integrity_hash` 字段，链在实际使用中从未生效。

---

## 定位

```bash
# 找 Rust 写审计的函数
grep -n "append_repair_audit_event\|append.*audit\|audit\.jsonl\|audit_event" engine/src/cli.rs | head -20

# 找完整性哈希计算（在 P1-4 迁移后的 Rust report 实现里）
grep -n "integrity_hash\|integrity\|sha256\|sha2" engine/src/cli.rs | head -20
```

---

## 必须做的修复（最低档，成本低）

### 步骤1：统一 Rust 写入端的记录格式，加哈希字段

找到所有 `append_*_audit_event` 函数，在每条记录里加 `integrity_hash`：

```rust
// 每条审计记录写入时：
// 1. 构建记录 JSON（不含 integrity_hash）
// 2. 读取上一条记录的 integrity_hash（没有则用空串作锚点）
// 3. 计算本条哈希：sha256(record_json_without_hash + prev_hash)
// 4. 把 integrity_hash 加入记录，写入文件

fn compute_integrity_hash(record_without_hash: &Value, prev_hash: &str) -> String {
    use sha2::{Sha256, Digest};
    let canonical = format!("{}:{}", record_without_hash.to_string(), prev_hash);
    let hash = Sha256::digest(canonical.as_bytes());
    format!("{:x}", hash)
}

fn append_audit_record(workspace: &Path, mut record: Value) {
    let jsonl_path = workspace.join(".hologram/audit.jsonl");
    // 读最后一条记录的 integrity_hash 作为 prev_hash
    let prev_hash = read_last_integrity_hash(&jsonl_path).unwrap_or_default();
    let hash = compute_integrity_hash(&record, &prev_hash);
    record["integrity_hash"] = json!(hash);
    // 追加写入
    let line = serde_json::to_string(&record).expect("serialize audit record");
    let mut file = fs::OpenOptions::new().append(true).create(true).open(&jsonl_path)
        .expect("open audit.jsonl");
    writeln!(file, "{line}").expect("write audit record");
}
```

### 步骤2：report 命令的完整性校验改为"无哈希=未受保护"

在 `run_report_command_native`（P1-4 里新写的 Rust 函数）中，校验审计链时：

```rust
fn verify_audit_integrity(records: &[Value]) -> AuditIntegrityResult {
    let mut prev_hash = String::new();
    let mut unprotected_count = 0;
    let mut tampered_lines = Vec::new();

    for (idx, record) in records.iter().enumerate() {
        match record.get("integrity_hash").and_then(Value::as_str) {
            None => {
                // 无哈希 → 标记为"未受保护"，不能算作 verified
                unprotected_count += 1;
            }
            Some(stored_hash) => {
                // 重算哈希，比对
                let record_without_hash = {
                    let mut r = record.clone();
                    r.as_object_mut().unwrap().remove("integrity_hash");
                    r
                };
                let expected = compute_integrity_hash(&record_without_hash, &prev_hash);
                if expected != stored_hash {
                    tampered_lines.push(idx + 1);
                }
                prev_hash = stored_hash.to_string();
            }
        }
    }

    AuditIntegrityResult {
        status: if !tampered_lines.is_empty() {
            "failed"
        } else if unprotected_count > 0 {
            "partial"  // 有记录没哈希，只能部分校验
        } else {
            "verified"
        },
        tampered_lines,
        unprotected_count,
    }
}
```

### 步骤3：停止虚假宣传

在 `README.md` 里，找到"不可篡改"相关表述，改为更诚实的说法：
- "不可篡改的审计日志" → "可追溯的审计日志（完整性受 SHA-256 链保护；密码学绑定需服务端 HMAC 支持）"
- 或类似措辞，不夸大当前能力。

---

## Cargo.toml 依赖

确认 `sha2` crate 已在 `engine/Cargo.toml`：
```toml
sha2 = "0.10"
```
如未添加，加上。

---

## 验收标准

```bash
# 1. Rust 写入的审计记录含哈希字段
audit-risk check .
cat .hologram/audit.jsonl | python3 -c "import sys,json; [print('hash' in json.loads(l)) for l in sys.stdin if l.strip()]"
# 全部输出 True

# 2. 篡改一条记录后 report 能报出问题
# 修改 audit.jsonl 里某行的 status 字段，不重算哈希
audit-risk report . --output /tmp/r.json
cat /tmp/r.json | python3 -c "import sys,json; r=json.load(sys.stdin); print(r['audit']['integrity']['status'])"
# 输出 "failed"，不是 "verified"

# 3. 无哈希的旧记录不再被视为 verified
echo '{"event_type":"test","timestamp":"2026-01-01T00:00:00Z"}' >> .hologram/audit.jsonl
audit-risk report . --output /tmp/r2.json
cat /tmp/r2.json | python3 -c "import sys,json; r=json.load(sys.stdin); print(r['audit']['integrity']['status'])"
# 输出 "partial" 或 "unprotected_entries_found"，不是 "verified"

# 4. 全量测试
cargo test
cargo +1.97.0 clippy --all-targets -- -D warnings
```

---

## 提交信息

两个 commit：
```
fix(audit): write integrity_hash on every audit record from Rust
fix(audit): verify chain integrity in report — reject unprotected and tampered records
```
