# 会话A：P1-6（致命误报）+ P1-5（漏检）

**目标文件：`engine/src/routing/secrets.rs`（唯一需要改的文件）**  
**验证命令：`cargo test --test detection_quality`（必须保持召回100%、误报0）**

---

## 背景

`SecretScanner` 是检测硬编码密钥/危险函数/SQL注入的核心正则扫描器。  
当前有两类问题：**把正常代码误报成高危**（P1-6），以及**漏掉真实风险**（P1-5）。  
P1-6 必须先做（误报直接导致用户弃用），P1-5 在同一文件紧接着做。

---

## P1-6：修5类误报（先做）

### 误报1：`model.eval()` 被报为危险命令执行

**位置**：`eval` 正则（约 `secrets.rs:151`）  
`\beval\s*\(` 会命中 Python 的 `model.eval()`、`loss.eval()` 等方法调用。

**修复**：要求 `eval` 前面**不是点号**，即裸调用才报：
```rust
// 改前（近似）：
r"\beval\s*\("
// 改后：
r"(?<!\.)eval\s*\("
```

**验收**：在 `engine/tests/detection_corpus/clean/` 新增文件 `pytorch_eval.py`：
```python
import torch
model = torch.nn.Linear(10, 1)
model.eval()
loss_val = criterion.eval()
```
`detection_quality` 对该文件报 0 命中。

---

### 误报2：`regex.exec()` 被报为Shell命令执行

**位置**：`\.exec(Sync)?\s*\(` 正则（约 `secrets.rs:162`）  
会命中 JS 正则的 `.exec()` 方法：`pattern.exec(str)`。

**修复**：排除明显的正则变量（前面是单词字符/`)`/`]`的 `.exec`），或要求前面是 `child_process` 相关上下文。最简单的修复：把 `\.exec` 改为要求前面不是标识符结尾的点调用：
```rust
// 改前（近似）：
r"\.exec(?:Sync)?\s*\("
// 改后（排除正则方法调用，要求前面是 child_process 上下文或 exec 独立出现）：
// 方案：改为只匹配 exec( 作为语句开头或 require('child_process') 后的调用
r"(?:child_process|require\s*\(\s*['\"]child_process['\"]\s*\))[\s\S]{0,200}\.exec(?:Sync)?\s*\("
```
若上述正则太复杂，最低要求：排除 `varName.exec(` 这种纯方法调用形式（前面是标识符或 `)`）。

**验收**：在 `clean/` 新增 `regex_exec.js`：
```javascript
const pattern = /hello/g;
const match = pattern.exec(inputString);
const re = new RegExp('\\d+');
re.exec(text);
```
`detection_quality` 对该文件报 0 命中。

---

### 误报3：界面文案里的 Update/Select 被报成SQL注入

**位置**：SQL注入正则 `(?i)\b(SELECT|INSERT|UPDATE|DELETE|DROP|...)\b`（约 `secrets.rs:121`）  
纯大小写不敏感匹配会把 "Click **Update** button" 报成SQL注入。

**修复**：要求**更强的SQL上下文**，必须同时出现 SQL 结构词（`FROM`/`WHERE`/`VALUES`/`SET`/`INTO`/`TABLE`）之一，而非孤立的单个关键字：
```rust
// 改为：要求匹配含有 SQL 关键字 + SQL 结构词的行
// 例如：SELECT ... FROM，UPDATE ... SET，INSERT INTO，DELETE FROM
r"(?i)\b(SELECT\b.{0,200}\bFROM\b|INSERT\b.{0,100}\bINTO\b|UPDATE\b.{0,100}\bSET\b|DELETE\b.{0,100}\bFROM\b|DROP\b.{0,50}\b(?:TABLE|DATABASE|INDEX)\b)"
```

**验收**：在 `clean/` 新增 `ui_text.ts`：
```typescript
const label = "Select an option";
const btn = "Update profile";
const msg = "Delete this item?";
const help = "Insert your name here";
```
`detection_quality` 对该文件报 0 命中；同时确认 `bad/` 里真实SQL注入样本仍被命中。

---

### 误报4：`{credentials: "same-origin"}` 被报成硬编码密钥

**位置**：敏感变量名正则，`credentials` 在名单中（约 `secrets.rs:118`）  
`fetch(url, { credentials: "same-origin" })` 是标准 Fetch API，不是密钥。

**修复**：排除已知的 Fetch API 枚举值：
```rust
// 在 credentials 的匹配之后，排除值为以下枚举时：
// "omit" | "same-origin" | "same-site" | "include"
// 方案：若行中 credentials 后跟的值是这4个之一，跳过
```
实现：在命中 `credentials` 之后检查值，若匹配 `(?i)(omit|same-origin|same-site|include)` 则过滤掉该命中。

**验收**：在 `clean/` 新增 `fetch_api.ts`：
```typescript
fetch('/api/data', { credentials: 'same-origin' });
fetch('/api/auth', { credentials: 'include' });
fetch('/api/public', { credentials: 'omit' });
```
`detection_quality` 对该文件报 0 命中。

---

### 误报5：SRI 完整性哈希被报成密钥泄露

**位置**：熵检测（约 `secrets.rs:233`）  
`integrity="sha384-abc123..."` 里的 base64 哈希熵值高，被误报为密钥。

**修复**：在熵检测的白名单里加 SRI 场景：
```rust
// 现有锁文件跳过逻辑附近，增加：
// 若行中含 integrity= 或 sha256-/sha384-/sha512- 前缀，跳过熵检测
fn is_known_public_hash_context(line: &str) -> bool {
    line.contains("integrity=") ||
    line.contains("sha256-") ||
    line.contains("sha384-") ||
    line.contains("sha512-")
}
```

**验收**：在 `clean/` 新增 `sri_hash.html`：
```html
<script src="jquery.js" 
  integrity="sha384-oqVuAfXRKap7fdgcCY5uykM6+R9GqQ8K/uxFMUFe7bPWwGa5R2UgfyAkOmDr6Gq"
  crossorigin="anonymous"></script>
```
`detection_quality` 对该文件报 0 命中。

---

## P1-5：补漏检（P1-6全部通过后再做）

以下每一条：先在 `engine/tests/detection_corpus/bad/` 新增样本，再修正则，再确认 `detection_quality` 仍然 100%/0。

### 漏检1：`.env` 无引号密码

**现象**：`PASSWORD=mysecretpassword`（无引号）漏检。  
**修复**：赋值正则同时匹配有引号和无引号形式：
```rust
// 改前：要求带引号
r#"(?i)(password|secret|api_key)\s*=\s*["'][^"']{8,}["']"#
// 改后：引号可选
r#"(?i)(password|secret|api_key|token)\s*=\s*["']?([^"'\s]{8,})["']?"#
```
在 `bad/` 新增 `env_no_quotes.env`：
```
DATABASE_PASSWORD=SuperSecret123
API_KEY=abcdefghijklmnop
SECRET=my_hard_coded_secret
TOKEN=ghp_realtoken12345678
```

### 漏检2：数据库/Azure 连接串里的明文密码

**现象**：`Server=...;Password=abc123;` 漏检（含 `;:/@.` 的串被判为"不像密钥"）。  
**修复**：`looks_like_key_charset`（约 `:570`）对连接串放宽：
```rust
// 新增：连接串模式单独检测
r"(?i)(password|pwd)\s*=\s*([^;'""\s]{6,})"
// 覆盖 JDBC: jdbc:postgresql://host/db?password=xxx
// 覆盖 ADO.NET: Server=x;Password=xxx;
// 覆盖 MongoDB: mongodb://user:pass@host/
```
在 `bad/` 新增 `connection_strings.cs`：
```csharp
var conn = "Server=prod.db.com;Database=app;User Id=admin;Password=Pr0dP@ssw0rd!;";
var jdbc = "jdbc:mysql://localhost:3306/mydb?user=root&password=rootpassword";
var mongo = "mongodb://appuser:S3cr3tP@ss@cluster.example.com:27017/mydb";
```

### 漏检3：`os.popen` 和 `subprocess.getoutput`

在 `bad/` 新增 `dangerous_exec_python.py`：
```python
import os, subprocess
result = os.popen(user_input).read()
out = subprocess.getoutput(f"ls {user_dir}")
```
在 `secrets.rs` 危险执行清单里补上：
```rust
r"\bos\.popen\s*\(",
r"\bsubprocess\.getoutput\s*\(",
```

### 漏检4：PHP/C 的 `system(` 调用

在 `bad/` 新增 `dangerous_exec.php`：
```php
<?php
$result = system($_GET['cmd']);
$out = shell_exec($userInput);
```
补正则：`r"\bsystem\s*\("`, `r"\bshell_exec\s*\("`（注意用上下文限定减少误报）。

### 漏检5：Hex 密钥（熵阈值问题）

**现象**：`熵阈值 > 4.5`，纯 hex 字符串理论熵上限 4.0，所以所有 hex 格式密钥漏检。  
**修复**：对 hex 格式单独检测：
```rust
// hex 密钥：32位以上连续十六进制字符，且变量名是敏感词
r#"(?i)(api_key|secret|token|password)\s*=\s*["']?([0-9a-f]{32,})["']?"#
```
在 `bad/` 新增 `hex_key.py`：
```python
API_KEY = "a3f2b1c4d5e6f7a8b9c0d1e2f3a4b5c6"
SECRET_TOKEN = "deadbeefcafebabe1234567890abcdef"
```

---

## 执行顺序

1. 做P1-6全部5条误报修复
2. `cargo test --test detection_quality` 确认 clean/ 全0
3. `cargo test` 确认全绿
4. 做P1-5漏检，每加一条检查一次
5. 最终 `cargo test --test detection_quality`（召回100%、误报0）
6. `cargo +1.97.0 clippy --all-targets -- -D warnings`
7. 两个修复分开提交：
   - `fix(detection): remove false positives for eval/exec/SQL/credentials/SRI`
   - `feat(detection): add detection for unquoted secrets, connection strings, os.popen, hex keys`
