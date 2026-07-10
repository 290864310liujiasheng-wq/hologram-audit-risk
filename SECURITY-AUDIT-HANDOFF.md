# audit-risk 安全审计整改交接文档

> 面向执行者（Codex 或其他编码代理）。本文件自包含：你**不需要**任何对话上下文即可开始。
> 目标：修复一次黑盒安全审计发现的问题。所有问题都由独立测试复现过，但**行号是审计当时的近似值**——动手前请先用 `grep`/搜索定位真实代码，再改。

---

## 0. 项目背景（30 秒）

- `audit-risk` 是一个"AI 编码风控 / 代码安全扫描" CLI 工具。
- 主体是 Rust 引擎，在 `engine/`（二进制名 `audit-risk`）。
- 还有一个**已冻结的** TS 前端 `src-ui/`（旧 "HoloGram" 桌面基座）。**关键架构事实**：很多"聪明的"安全校验逻辑只写在 `src-ui/src/risk/`（TS），而真正发行的 **Rust CLI 二进制根本不执行这些 TS 逻辑**。本次多个严重问题正源于此。
- 部分二级命令（`report`/`audit`/`rules`/`verify`）由 Rust 进程 fork 一个 Node 脚本 `src-ui/scripts/phase5-delivery.ts` 实现。

---

## 1. 如何构建 / 测试 / 验证（务必遵守）

```bash
# 构建 CLI
cd engine && cargo build --release --bin audit-risk      # 产物：engine/target/release/audit-risk

# 全量测试（必须全绿）
cd engine && cargo test

# 检测质量基线（必须保持：召回 100%、误报 0）
cd engine && cargo test --test detection_quality -- --nocapture
```

### CI 门禁（改动必须同时满足，否则 CI 红）
1. **`rust` job**：`cd engine && cargo clippy --all-targets -- -D warnings` 且 `cargo test` 全过。
   - ⚠️ CI 用 **Rust stable 1.97**，其 clippy 比 1.96 更严格。请本地也用 1.97 验证：
     `rustup toolchain install 1.97.0 --component clippy` 然后 `cargo +1.97.0 clippy --all-targets -- -D warnings`。
2. **`Build audit-risk CLI binary` job**：release 构建 + smoke test。
3. **`risk-delivery` job**：`npm --prefix src-ui run phase5:report -- ... --fail-on off`（会拿工具扫自己仓库）。

### 绝对不能弄坏的东西
- **检测质量基线**：`engine/tests/detection_corpus/` 下 `bad/` 每个文件必须≥1 命中（召回 100%），`clean/` 每个文件必须 0 命中（0 误报）。`detection_quality.rs` 里有断言锁死。
  - 新增检测能力 → 在 `bad/` 加样本（并把对应 `gaps/` 样本移过去）。
  - 减少误报 → 改完必须确认 `bad/` 仍 100%。
- 现有 387 个测试。
- 不要提交 `.hologram/`、`target/`、`node_modules/`（都在 gitignore）。仓库根有 `clean.sh` 可清理产物。

### 工作规范
- 每个修复独立成一个 commit，信息写清"修了什么、为什么"。
- 改前先复现、改后按每条的"验收标准"验证。
- 优先级从 P0 开始，P0 未清零不要碰 P1。

---

## 2. 修复清单（按优先级）

---

## 🔴 P0-1 ｜ 删除 `mock://` 授权后门（免费永久解锁 Pro）

**位置**：`engine/src/cli.rs`，函数 `auth_http_json`（审计时约 `cli.rs:4899`）；`mock://approved/api/auth/exchange` 分支（约 `:4941`）返回带**合法签名**的 Pro 授权文档（约 `:4954`）。base_url 可被环境变量 `AUDIT_RISK_AUTH_BASE_URL` 覆盖（约 `:3437`）。

**根因**：`auth_http_json` 没有 `#[cfg(test)]` 保护，是编译进 release 二进制的正常函数。它对 `mock://` URL 直接返回 `plan: pro_personal_monthly`、`valid_until: 2999-01-01`、且**自带能通过验签的签名**。

**复现**：
```bash
export AUDIT_RISK_ENTITLEMENT_DIR=/tmp/ent
export AUDIT_RISK_AUTH_BASE_URL="mock://approved"
audit-risk auth login
unset AUDIT_RISK_AUTH_BASE_URL
audit-risk auth status     # → 显示 Pro 个人版 / 授权有效（白嫖成功）
```

**必须做的修复**：
1. 把所有 `mock://` 分支移到 **`#[cfg(test)]`** 或 test-only 模块，使 **release 二进制绝不返回任何带签名的 Pro 文档**。测试可以继续用 mock（它们以 cfg(test) 编译）。
2. **【重点排查】确认客户端二进制里没有嵌入签名私钥**。签名方案要成立，**只有服务端能持私钥**，客户端只能有公钥用于验签。请检查 mock 的"合法签名"是怎么来的：
   - 如果是**预计算的固定签名**（针对 mock 文档）→ 随 mock 一起进 cfg(test) 即可。
   - 如果客户端**内嵌了私钥能现场签任意文档** → 这是独立的灾难级问题，必须移除私钥，改为只保留公钥。
3. （建议）release 下把 `AUDIT_RISK_AUTH_BASE_URL` 限制为 `https://` scheme。

**验收标准**：
- release 构建后：`AUDIT_RISK_AUTH_BASE_URL=mock://approved audit-risk auth login` **不产生 Pro 授权**（应失败或保持 Core）。
- `cargo test` 仍全绿（mock 相关测试在 cfg(test) 下仍可用）。
- `grep` 整个 `engine/src` 确认 release 路径无任何私钥常量。

---

## 🔴 P0-2 ｜ `repair apply` 加安全校验（当前=任意文件写入 + RCE）

**位置**：`engine/src/cli.rs`，`run_repair_apply_command`（约 `:1538`）。
- plan 加载：`workspace_path.join(".hologram/repair-plans/{plan_id}.json")`（约 `:1542`）——`plan_id` 未过滤。
- 文件写入：对每个 `operations[].file_path` 无范围校验（约 `:1613-1656`）——绝对路径直接用、相对路径无脑 `join`、`..` 不拦。
- preflight：`Command::new(prog).args(args)` 直接执行 plan 里 `required_tests` 的字符串（约 `:1574-1580`）。
- 回滚快照：`fs::read_to_string(&abs_path).unwrap_or_default()`（约 `:1622`）。

**根因**：README（`README.md` 约 :96/:132）宣称"修复方案先做二次审计、验证、审批"，但那些校验只在 `src-ui/src/risk/self-heal.ts`（TS/UI）里，**CLI apply 路径完全不走**，是裸奔。

**复现（每条都真实成功过）**：
- 绝对路径写工作区外：plan 里 `file_path:"/tmp/outside/pwned.txt"` → 文件被写到工作区外。
- `../` 穿越 + 覆盖敏感文件：`file_path:"../../x"`、`.env`、`.git/config` 全部被覆盖。
- 注入新漏洞：`new_content` 塞硬编码密钥+SQL注入 → 直接落地，无二次审计。
- `plan_id` 穿越：`--plan "../../../../tmp/outside/evilplan"` → 加载工作区外任意 JSON。
- RCE：`required_tests:["touch /tmp/PROOF"]` → 命令真的被执行。

**必须做的修复**：
1. **路径校验**：对每个 `operations[].file_path`——
   - 拒绝绝对路径；拒绝含 `..` 的路径。
   - 解析为 `workspace_path.join(file_path)` 后做 `canonicalize`（或等价规范化），断言结果**仍在 canonicalized workspace 内**，否则拒绝整个 apply。
   - 建议额外拒绝写入 `.git/`、锁文件、`.env` 等敏感路径（或至少在工作区内也要显式确认）。
2. **`plan_id` 清洗**：只允许安全字符集（如 `[A-Za-z0-9_.-]`），拒绝任何路径分隔符和 `..`。
3. **`required_tests` 不得执行来自 plan 的任意命令**：改为固定白名单命令，或从**可信配置**（如 `.hologram/delivery.json`）读取而非 plan，或直接移除该 preflight。
4. **写入前跑真正的二次审计**：对补丁 `new_content` 复用/移植 self-heal 的 secondary audit（扫一遍补丁本身，若引入新风险则阻断 apply）。
5. **回滚修复**：
   - 记录每个目标文件"原本是否存在"；回滚时对**新建的文件执行删除**，而不是写回空串。
   - 快照用 `fs::read`（字节）而非 `read_to_string`，避免二进制/非 UTF-8 文件被清空且无法还原。

**验收标准**（新增测试覆盖）：
- 含绝对路径 / `../` / 敏感路径的 plan → apply 被拒绝，工作区外/敏感文件**零改动**。
- `plan_id` 含 `../` → 被拒绝。
- `required_tests` 无法执行任意命令（写个 plan 试图 `touch` 一个文件，验证不被执行）。
- 引入新密钥/注入的补丁 → 被二次审计阻断。
- 部分失败回滚后：新建文件被删除、二进制文件内容完好、审计不谎报"已回滚"。

---

## 🔴 P0-3 ｜ 审计链"不可篡改"目前是假的

**位置**：`src-ui/src/risk/delivery.ts`（`buildDeliveryAuditIntegritySummary`）。Rust 写审计：`engine/src/cli.rs` `append_repair_audit_event`（约 `:1773`）。

**根因（三条叠加）**：
1. 完整性用**无密钥的公开 SHA-256** 链（`integrity_hash = sha256({ts,tool,...,prev_hash})`）。任何人改完记录、用同样公式重算整条链即可，`verify` 照样报 verified。
2. **"无 `integrity_hash` 字段的记录直接跳过校验"**（约 `delivery.ts:369-373`，标为 legacy）。
3. **Rust 写入端 `append_repair_audit_event` 写出的记录根本没有哈希字段**——即真实运行产生的每条记录都是"legacy"，链在实际使用中从未生效。

**复现**：改一条 audit.jsonl 记录并重算链 → `verify` 报 verified；或写两条无哈希记录 → 被当正常记录、`status: legacy_anchor / verified: true`。

**修复分两档**：

**最低（必须做，成本低）**：
1. **立刻停止虚假宣传**：在真正的完整性保护落地前，从 README / 产品文案里删掉或弱化"不可篡改的审计"表述（改为"审计留痕"之类不夸大的说法）。诚实是底线。
2. **移除 legacy 放行后门**：无哈希的记录不能默默通过，应显式标记为"未受保护/无法校验"。
3. **统一格式并在写入端上链**：让 Rust 写入端与 TS 读取端用**同一套字段**，且 Rust 在写入时就计算哈希/链（否则链永远空转）。
4. 顺带修 P1-8 的崩溃（读取端容错）。

**真正做到（需要服务端）**：
- 用 **HMAC（密钥只在服务端）** 或**数字签名**替换裸 SHA-256，客户端无法伪造。没有服务端就先不要声称"不可篡改"。

**验收标准**：
- 篡改 + 重算链后，`verify`/`report` 能报出问题（或明确标注"审计未受密码学保护"）。
- 无哈希记录不再被静默当作"verified"。
- Rust 真实写入的审计记录能被读取端正确解析并纳入完整性判断。

---

## 🟠 P1-4 ｜ `report`/`audit`/`rules`/`verify` 对真实安装用户完全不可用

**位置**：`engine/src/cli.rs` `run_phase5_secondary_command`（约 `:1867`）、`resolve_platform_root`（约 `:3106`）。

**根因**：这四个命令 fork `node --import tsx .../src-ui/scripts/phase5-delivery.ts`，靠"二进制往上找 `src-ui/`"定位脚本。但 `install.sh` 只装一个二进制，用户机器没有 `src-ui/`、没有 node/tsx。结果这些命令永远报 `failed to locate platform root`（退出 3）。

**修复（三选一，按投入排序，需产品决策）**：
- **A（推荐，长期正确）**：把这些命令的核心逻辑用 **Rust 重写**进引擎，去掉对 node/src-ui 的运行时依赖。
- **B**：把所需 TS 逻辑与 node 运行时一起**打包进发行物**，让 `install.sh` 一并安装。
- **C（临时止血）**：当定位不到 platform root 时，给出**清晰的中文错误**说明该命令当前需要开发环境，并在 README 明确标注这些命令的限制，别让用户以为坏了。

**验收标准**：把 release 二进制单独拷到一个干净目录（无 `src-ui/`），这四个命令要么正常工作（A/B），要么给出清晰可理解的说明而非裸报错（C）。

---

## 🟠 P1-5 ｜ 大量真实风险漏检（检测器覆盖太窄）

**位置**：`engine/src/routing/secrets.rs`（扫描器 `SecretScanner`）。

**根因与具体盲区**（每条建议新增 `bad/` 语料并实现检测；注意别引入误报，改完 `cargo test --test detection_quality` 必须仍 100%/0）：
- 熵检测 `looks_like_key_charset`（约 `:570`）把含 `;:/@.` 的**连接串**判为"不像密钥" → 数据库/Azure 连接串里的明文密码全漏。
- 赋值正则（约 `:114/:118`）要求值**带引号** → `.env`/`.ini`/shell 里 `PASSWORD=xxx`（无引号）全漏。
- 熵阈值 `> 4.5`（约 `:238`）高于纯 hex 的理论上限 4.0 → **hex 密钥永远漏**；变量名单缺裸 `token`/`secret`。
- 危险执行清单（约 `:150-169`）缺 `os.popen`、`subprocess.getoutput`、裸 `system(`（PHP/C/Perl）、`spawn(...,{shell:true})`、反引号命令执行。
- IAM 正则（约 `:174`）只认 JSON 带引号 key → YAML/CloudFormation/serverless、Terraform(HCL) 的通配符全漏；且不认 `Principal:"*"`、服务级 `"s3:*"`、数组里非首位的 `"*"`。
- SQL 注入逐行匹配（约 `:197/:121`）→ **多行/三引号** f-string 漏；PHP `"...$id"`、C# `$"...{id}"`、Ruby `"...#{id}"`、Go `Sprintf("...%s",id)` 全漏。

**现实提醒（务必转达产品方）**：这是正则扫描器的固有局限，逐条堵洞是长期工作，且很难追平 Semgrep/CodeQL 这类语义/数据流工具。建议**按真实高频场景挑几类优先做**（如 `.env` 无引号密钥、连接串、`os.popen`、YAML/HCL 的 IAM），而不是追求全覆盖。

**验收标准**：每新增一类检测，`bad/` 加样本、`clean/` 不回归误报，`detection_quality` 保持 100%/0。

---

## 🟠 P1-6 ｜ 致命误报（把最高危标签贴到正常代码上）

**位置**：`engine/src/routing/secrets.rs`。

**根因与修复**：
- `\beval\s*\(`（约 `:151`）会命中 **`model.eval()`**（PyTorch）等方法调用。修复：像已有的 `exec()` 规则那样，要求 `eval` **前面不是 `.`**（裸 eval 才报）。
- `\.exec(Sync)?\s*\(`（约 `:162`）会命中 **JS 正则 `regex.exec()`**。修复：需要上下文限定（如仅在 `child_process`/`require('child_process')` 语境，或排除明显的正则变量），降低误报。
- SQL 关键字**大小写不敏感** `(?i)\b(SELECT|...)`（约 `:121`）会把界面文案里的 **Update/Select/Delete** 报成 SQL 注入。修复：要求更强的 SQL 上下文（如同时出现 `FROM/WHERE/VALUES/SET` 等，或大写关键字），避免匹配英文普通词。
- 敏感变量名单含 `credentials`（约 `:118`）会把 `fetch(..., {credentials:"same-origin"})` 报成硬编码密钥。修复：排除已知的 Fetch API 枚举值（`omit`/`same-origin`/`same-site`/`include`）。
- 熵检测（约 `:233`）把 **SRI 完整性哈希 `integrity="sha384-..."`**、公开哈希报成密钥。修复：对已知公开哈希场景（`integrity=`、SRI、锁文件——锁文件已跳过）加白。

**验收标准**：为每条误报在 `clean/` 加对应样本，改完 `detection_quality` 的 `clean/` 保持 0 误报，且 `bad/` 不回归。

---

## 🟠 P1-7 ｜ 授权可整包拷贝共享（device 绑定形同虚设）

**位置**：`engine/src/entitlement.rs`（签名 canonical payload，约 `:16-20` 与字段列表 `:53`）；device 校验 `engine/src/cli.rs`（约 `:3585-3620`）。

**根因**：签名 payload **不含 `device_id`**，device 校验只是本地自比。把 `entitlement.json + entitlement.sig + device_secret` 三个文件一起拷走，签名依旧有效 → 一份 Pro 可发给无数机器。

**修复**：把 `device_id` 纳入签名 payload，并由**服务端签发时绑定真实设备**（本项需要服务端配合；无服务端时至少把 device_id 纳入签名，减少裸拷贝）。

**验收标准**：把整包授权拷到另一"设备"（不同 device_secret）应验签失败/降级 Core。

---

## 🟠 P1-8 ｜ `report`/`audit` 遇到真实记录或坏 JSON 直接崩溃

**位置**：`src-ui/src/risk/audit-bridge.ts`（约 `:218`，`entry.tool.startsWith(...)`）；`src-ui/scripts/phase5-delivery.ts` `readAuditEntries`（约 `:299`，逐行 `JSON.parse` 无 try/catch）。

**根因**：① Rust 写的记录用 `event_type` 字段、没有 `tool` 字段 → 读取端 `entry.tool.startsWith` 抛 `Cannot read properties of undefined`。② 日志最后一行半截（断电/进程被杀常见）→ `JSON.parse` 抛异常、整个命令崩。也是注入型 DoS（追加一字节垃圾即可让审计永久不可用）。

**修复**：① 读取端对缺失字段做防御（`entry.tool?.startsWith` 或字段兼容）。② `readAuditEntries` 对每行 `JSON.parse` 加 try/catch，坏行跳过并显式提示"日志末尾损坏"，而不是崩溃。（与 P0-3 的"统一格式"一起做更好。）

**验收标准**：audit.jsonl 里放一条 Rust 格式记录 + 一条半截 JSON，`report`/`audit` 不崩，给出可用输出或清晰提示。

---

## 🟠 P1-9 ｜ 修复回滚不完整
见 P0-2 第 5 点（同一处，已合并到 P0-2 一起修）。

---

## 🟡 P2-10 ｜ 并发运行 → `database is locked` 直接失败

**位置**：引擎 SQLite 初始化（`engine init failed: pragma: database is locked`）。

**修复**：给 SQLite 连接设置 `busy_timeout`（如 5000ms）或对拿锁失败做有限重试，避免并发（watch + 手动 check、CI 并行）时直接失败退出。

**验收**：并发跑 3 个 `check` 同一 workspace，不再有实例因锁失败退出。

---

## 🟡 P2-11 ｜ `check` 污染 git 工作区

**位置**：`check` 命令流程；`git_changed_files`/`is_tool_artifact`（`engine/src/routing/preflight.rs` 与 `cli.rs`）。

**根因**：单跑 `check`（不经 `init`）不生成 `.hologram/.gitignore`，且把自己的 `.hologram/hologram.db*`、`baseline.json` 当作"变更文件"报进 `changed_files`。

**修复**：`check` 首次生成 `.hologram/` 时也写入 `.hologram/.gitignore`；并在 `changed_files`/git status 结果里过滤掉 `.hologram/` 自身产物。

**验收**：干净 git 仓库跑 `check` 后 `git status` 不出现未忽略的 `.hologram/`；`changed_files` 不含 `.hologram/*`。

---

## 🟡 P2-12 ｜ 默认 `check` 对 `require_approval` 级高危仍退出 0

**位置**：默认 `--fail-on` = `block`（`engine/src/cli.rs` 约 `:5180`）。

**根因**：high 级风险只到 `require_approval`，`require_approval < block`，默认挂 CI/pre-commit 时会绿灯放行如 `os.system(input())`。

**修复（需产品决策）**：把默认 `--fail-on` 调成 `require_approval`，或在 `init` 生成的 hook/CI 模板里默认用 `--fail-on require_approval`，并在文档明确说明各级别含义。

**验收**：默认配置下，含 high 危风险的代码能让 CI/hook 失败。

---

## 🟡 P2-13 ｜ 基线被篡改/损坏被静默吞掉

**位置**：`engine/src/routing/preflight.rs` `load_baseline`（约 `:14`，`...ok().and_then(...).unwrap_or_default()`）。

**根因**：baseline.json 非法/截断时静默当"无基线"，check 正常跑完又覆盖写回 → 可被用来悄悄重置对比基准、抹掉"相对基线的风险增长"。

**修复**：基线解析失败时至少**告警**（而非静默），可选加完整性保护。

**验收**：损坏 baseline 后 `check` 给出可见提示。

---

## 🟢 P3 ｜ 小瑕疵
- **P3-14**：耦合类(l3) findings 的 `location.file_path` 是空串，IDE/CI 无法跳转。补上文件定位。
- **P3-15**：多余位置参数被静默忽略（`check dir extra` 不报错），可能掩盖打字错误。改为对未知位置参数报用法错误。
- **P3-16**：损坏 JSON 的 repair plan 报错信息误导（说"过期或 ID 有误"），应区分"JSON 损坏"。

---

## 3. 建议的执行顺序

1. **P0-1**（删后门）——最快、最丢人、必须先做。
2. **P0-2**（repair 校验 + 回滚）——安全责任最重。
3. **P0-3 最低档**（停止虚假宣传 + 移除 legacy 放行 + 统一格式 + 顺带 P1-8 容错）。
4. **P1-6**（误报）——直接影响"用户会不会当场弃用"，且改动相对可控。
5. **P1-4 / P1-5 / P1-7**——需产品决策或较大工作量，排期做。
6. **P2 / P3**——收尾。

每完成一项：跑 `cargo test`、`cargo +1.97.0 clippy --all-targets -- -D warnings`、`cargo test --test detection_quality`，确认三样都过再提交。

---

## 4. 一句话给执行者
> 最严重的三条（P0）根子都是同一件事：**真正发行的 Rust CLI 缺了本应有的安全校验，而这些校验要么只存在于不发行的 TS 代码里、要么压根没做。** 修复的核心是把安全边界补回 CLI 本身，别依赖那个冻结的前端。
