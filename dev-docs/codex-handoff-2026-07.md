# Codex 5 会话交接方案（2026-07）

## 背景与现状

当前基线：`cargo test --lib` 477 passed / 0 failed（刚修完 doctor 测试，已可作为起点）。
本次会话已做的改动（未提交）：
- `README.md`：删除"受控自修复"宣传，加了本地 Quick Start 段落。
- `engine/src/cli.rs`：pre-commit hook 找不到二进制时改为 `exit 1`（原来是 `exit 0` 静默放行）；`tour_text()` 里 `watch` 从主路径移到"另开终端"备注；doctor 删除 cargo/node 依赖检查，重写了可选项显示逻辑。

这些改动**保留，不要回滚**，各会话在此基础上继续。

codex 深入体验后给出的结论：**产品成熟度约 3/10，No-Go**。检测内核（找密钥、找注入）本身可用，但门禁、审计、审批、CI 没有形成一套自洽系统。以下是分给 5 个 codex 会话的任务包，每包互不依赖文件（除会话 A 和 B 有一处强依赖，见下），可以并行开。

---

## 通用规则（每个会话开头都要读一遍）

1. 只在 `/Users/liupeicheng/Documents/New project 13/hologram-audit-risk/` 这一个目录下工作。**不要碰 `/Users/liupeicheng/Desktop/` 下任何目录**——那里有一份名字带 `full-copy` 的旧快照，里面还残留 Node.js/TypeScript/repair 相关的已废弃代码，不是当前项目。
2. 改完必须跑 `cd engine && cargo test --lib`，确认不比接手时的通过数更少。
3. 不要引入新的 Node.js/TypeScript 依赖或子进程调用——项目本次会话已经把 report/audit/rules/verify 迁移成 Rust 原生实现，任何"调用外部脚本"的写法都是回退。
4. 改动后在 commit message 里写清楚"改了什么、为什么"，不要用"fix bug"这种空话。
5. 不确定某个改动是否会影响其他会话正在改的文件时，先搜一下这份文档里其他任务包有没有提到同一个函数/文件，避免两个会话同时改同一处产生冲突。

---

## 会话 A：策略统一（check 与 report 的 gate 决策必须一致）

**问题**：`run_check_command`（`engine/src/cli.rs:663`）完全不读取 `.hologram/delivery.json`，也不加载 `rule_packages`（团队禁用的规则、每条规则的 gate_effect）。而 `run_report_command_native`（`cli.rs:1253`）会调用 `effective_delivery_config`（`cli.rs:1819`）+ `resolve_rule_policy`（`cli.rs:1892`），并通过 `apply_report_review_policy`（`cli.rs:1429`）重新计算 gate_decision——这个函数对不在 policy 里的 `rule_id` 直接判 `allow`（`cli.rs:1451` 的 `filter_map` 会把它们过滤掉，不进入 `max_by_key`）。

**后果（已用代码验证，不是猜测）**：团队在 delivery.json 里禁用某条规则后，`check` 命令完全不知道，拿到默认引擎判定继续 block；`report` 命令用同一批 findings 套用 policy，判定 allow。`check` 和 `report` 对同一份代码给出相反的门禁结论。pre-commit hook 和 CI 走的都是 `check`，团队配置的规则策略在最容易被信任的路径上根本不生效。

**修复方向**：让 `run_check_command` 也走 `effective_delivery_config` → `resolve_rule_policy` → `apply_report_review_policy` 这条链路，而不是反向给 report 抄一份 check 的逻辑。具体：
1. 在 `build_workspace_check_payload`（`cli.rs:2024`）里增加读取 delivery.json 和 rule policy 的步骤（参考 `run_report_command_native` 里 `cli.rs:1265-1269` 怎么做的）。
2. 把 `apply_report_review_policy` 的调用挪到 check 的 payload 构建里，或者把它提炼成一个通用函数，check 和 report 都调用它。
3. `run_watch_command`（`cli.rs:713`）内部循环里也调用了 `build_workspace_check_payload`（`cli.rs:753`），改完 check 之后 watch 自动跟着对齐，不用单独改。
4. 配置文件损坏或缺失时，`effective_delivery_config` 目前的报错行为要确认：`doctor`/`report` 遇到损坏配置会失败，但 `check` 之前完全不读配置所以不会失败。改完之后要明确：**配置损坏时 check 应该 fail closed（报错阻断），不能悄悄回退到"不应用团队策略"的默认行为**——否则等于把这个 bug 换了个位置。
5. 写一个回归测试：禁用某条规则的 delivery.json 场景下，`audit-risk check` 和 `audit-risk report` 对同一 workspace 必须给出相同的 `gate_decision`。

**依赖关系**：会话 B 也会改 `build_workspace_check_payload` 附近（写审计链）。两个会话如果同时改，建议会话 A 先完成并提交，会话 B 在此基础上继续，避免大范围冲突。如果必须并行，两边都要在动手前重新 `grep -n "fn build_workspace_check_payload"` 确认对方有没有已经改过这段。

---

## 会话 B：补齐审计链写入（当前完全是空的）

**问题**：全代码库唯一读取 audit.jsonl 的函数是 `read_audit_jsonl`（`cli.rs:1558`），**没有任何函数往 audit.jsonl 写入内容**（已用 `grep -rn "fn.*append.*audit\|fn write_audit"` 确认为零匹配）。`run_check_command`、`run_report_command_native`、`run_watch_command` 正常扫描流程结束后都不会记录任何东西。

同时 `build_delivery_audit_integrity`（`cli.rs:1701`）在 `entries.is_empty()` 时直接返回 `{"status": "empty", "verified": true, ...}`（`cli.rs:1703`）——空审计链被标记为"已验证"，报告和 doctor 会显示这是"通过"状态，而 README 原文宣称"所有决策进入 SHA-256 哈希链审计日志，可用于复盘和留痕"。这是产品最核心承诺的空心化，不是小问题，应该视为最高优先级。

**修复方向**：
1. 新增一个 `append_audit_entry(workspace: &Path, entry: Value) -> Result<...>` 之类的函数，用 `OpenOptions::new().append(true).create(true)` 打开 audit.jsonl，写入一行 JSON。写入的条目结构要参考 `build_delivery_audit_integrity` 期望读到的字段（`integrity_hash`、`tool`、`ts`/`timestamp`），可以照抄之前 repair 相关代码用过的 hash 计算方式——搜 `compute_audit_integrity_hash` 和 `canonical_audit_integrity_payload`（都在 `cli.rs` 里，`grep -n` 找到具体行号），这两个函数目前只在"校验"侧用，逻辑可以复用于"写入"侧。
2. 在 `run_check_command`、`run_report_command_native`、`run_watch_command`（每次循环命中新 finding 时,不要每次 tick 都写)三个地方,扫描完成后调用这个写入函数,记录本次 gate_decision、finding_count、workspace、时间戳。
3. `build_delivery_audit_integrity` 的空链判定要改：`entries.is_empty()` 不应该直接给 `verified: true`。如果产品语义是"从未跑过审计所以谈不上验证",应该是一个第三种状态(比如 `"status": "not_started"`, `verified: false` 或者干脆不给 verified 字段),不能让空等价于通过。同步检查 `render_report_screen`(`cli.rs:4617` 附近)和 `render_doctor_screen`(`cli.rs:4539`)有没有直接把 `verified` 字段原样展示给用户,确认修改后文案依然准确。
4. 写审计链要考虑并发/性能：`watch` 命令是循环执行 check,不能每次 tick 都写一行,应该只在 gate_decision 发生变化或者有新增 finding 时才写。参考 `run_watch_command` 里已有的 `emitted_findings` 去重逻辑(`cli.rs:749` 附近)。
5. 回归测试：跑一次 `check`,再跑 `audit --query review`,应该能查到刚才那次扫描的记录,不再是 0 条。

---

## 会话 C：安全泄露修复 + 输入边界问题

这一包是"检测功能本身制造新泄露面"和几个具体输入处理 bug,改动分散但都不大,适合一个会话集中处理。

**C1：密钥在代码片段里被完整打印回显**
`read_code_snippet`（`cli.rs:4251`)把命中密钥那一行原样打印到终端（`cli.rs:4268`)。这个函数被 `render_check_screen` 调用用于展示 finding 的上下文,如果这个 finding 本身就是"这一行有硬编码密钥",那密钥就被完整打到终端和任何重定向的日志文件里(包括 CI 日志,CI 日志权限往往比源码仓库还宽松)。
修复方向:在拼接 snippet 前,如果这一行对应的 finding 类型是密钥/高熵字符串类,把命中的具体子串替换成 `***REDACTED***` 再显示,而不是整行隐藏(整行隐藏会丢失定位信息)。需要能拿到具体命中的字符串范围——检查 `routing/secrets.rs` 里 finding 的 JSON 结构有没有记录命中子串的起止位置(`start_col`/`end_col` 或类似字段),没有的话要先加上。

**C2：文件名带空格时密钥检测失效**
codex 报告：`bad file.py`（文件名含空格）里的硬编码密钥没被检出。定位方向：`git_changed_files`（`cli.rs:2871`)调 `git status --short --untracked-files=all`,`parse_git_status_changed_files`（`cli.rs:2890`)按固定列宽 `line[3..]` 切分路径。Git 对含特殊字符的路径会加引号转义（`core.quotepath`),这个解析器没处理转义,导致带空格/特殊字符的文件名解析错误,进而这个文件根本没被扫描到,不是密钥规则本身漏检。
修复方向:改用 `git status --short -z --untracked-files=all`（`-z` 用 NUL 分隔,不做引号转义),配套修改 `parse_git_status_changed_files` 按 NUL 分割而不是按行分割。写一个回归测试,workspace 里放一个文件名带空格且含硬编码密钥的文件,确认能被检出。

**C3：pre-commit hook 扫全树而不是 staged diff**
当前生成的 hook（`cli.rs` 里 `DEFAULT_PRE_COMMIT_PATH` 对应内容,搜 `"#!/bin/sh\\nset -eu"` 定位)执行 `audit-risk check "$ROOT"`,扫的是整个工作区,不是本次要提交的内容。后果:工作区里任何未提交的旧文件有风险,都会阻断一次完全无关的干净提交。
修复方向:这个改动比较大,需要新增一种"只看 staged 内容"的扫描模式。最小改法:在 hook 脚本里先用 `git diff --cached --name-only` 拿到 staged 文件列表,把这个列表传给 `audit-risk check` 的一个新参数(比如 `--files-from -`或类似),让 check 命令支持"只扫描指定文件列表"而不是"扫描 git status 里的所有变更"。这需要给 `run_check_command`/`build_workspace_check_payload` 加一个可选的文件白名单参数。如果时间不够,至少要在生成的 hook 注释里明确写清楚"当前会扫描整个工作区,不仅是本次提交"，不要让用户误以为只检查了自己要提交的内容。

**C4（低优先级,时间充裕再做）：`.env` 无引号密码等场景的其他边界**
如果 C1-C3 做完还有余量,回头看 codex 报告里点名的"结构风险显示 `app.js:0`、描述含内部节点名"这类展示层小问题,顺手清理,但不要为了这个花太多时间,优先级明显低于上面三条。

---

## 会话 D：init 原子化 + doctor 只读化 + 审批/豁免入口

**D1：`run_init_command`（`cli.rs:855`)非原子**
当前逐文件写(`cli.rs:871` 的循环),某个文件已存在且没加 `--force` 时直接 `return Err`（`cli.rs:874`),但循环前面已经成功写入的文件不会被清理,留下"半初始化"目录。`--force`（`cli.rs:873` 判断)直接跳过存在性检查覆写,没有备份,也没有 `--dry-run`。

修复方向:
1. 改成两阶段:第一阶段只做检查（每个目标文件是否存在、是否可写),收集所有冲突,一次性报告给用户,不动手写任何文件。
2. 第二阶段才真正写入。写入时如果中途失败,把本次已写入的文件全部删除回滚（可以先写到临时文件,全部成功后再一次性 rename,或者记录本次创建的文件列表,失败时逐个删除)。
3. `--force` 覆盖前,把要被覆盖的文件备份成 `.bak`(比如 `.githooks/pre-commit.bak.<timestamp>`),而不是直接覆盖丢失用户自定义内容。
4. 新增 `--dry-run`：只打印将要创建/覆盖哪些文件,不实际执行。当前 `reject_unknown_flags(rest, &["--force", "--json"])`（搜这行定位在哪个函数)需要把 `--dry-run` 加入合法参数列表。
5. **`check` 和 `init` 抢占同一个文件路径**（已用代码核实确认)：`ensure_hologram_gitignore`（`cli.rs:2053`,被 `check` 命令调用的 `build_workspace_check_payload` 内部调用)会自动创建 `.hologram/.gitignore`；而 `build_default_init_files`（`cli.rs:2685`)里 `init` 命令的默认文件清单里也包含这同一个相对路径（`cli.rs:2739`)。用户如果先跑过一次 `check`（哪怕只是想看看效果,还没想好要不要正式接入),再跑 `init` 想正式接入,会因为 `.hologram/.gitignore` 已存在、没加 `--force`,直接报错退出（`cli.rs:873-878`的判断),而且报错文案不会说明"这是你自己之前用 check 创建的",用户会以为是别的什么东西提前初始化过。这条要在做第 1-4 点的原子化改造时一并考虑：`init` 遇到 `.hologram/.gitignore` 已存在但内容和它自己会写的内容完全一致时,应该视为"已就位",跳过而不是报错;或者干脆让 `check` 不再自己创建这个文件,统一交给 `init` 来管。

**D2：`run_doctor_command`（`cli.rs:960`)有副作用,不是只读诊断**
`ensure_hologram_gitignore`(`cli.rs:2053`,被 `build_workspace_check_payload` 调用)以及 doctor 命令自身路径上的 `fs::create_dir_all(&hologram_dir)`（在 doctor 函数体内,具体行号改动后需重新 grep 确认)会无条件创建目录,即使传入路径本身不存在。后果:对一个拼错的路径跑第一次 doctor,虽然当次报错,但已经把该目录物理创建出来;第二次对同一路径跑 doctor,`workspace_path.exists()` 已为真,不再报错,被误判为合法 workspace。

修复方向:doctor 命令语义上应该是纯诊断,不应该创建任何东西。把"创建 .hologram 目录"这个副作用挪到 `init` 命令里去做,doctor 只检查"这个目录存不存在、可不可写",用 `Path::exists()` 判断,不调用任何 `create_dir_all`。同时要检查 doctor 是否验证了 pre-commit hook 是否真正激活（`core.hooksPath` 是否指向 `.githooks`),codex 报告里提到"自定义 `core.hooksPath` 时仍报告 Core 就绪"——如果 doctor 目前没检查这一项，参考 `activate_pre_commit_hook`（`cli.rs:925`)里已经写好的读取 `core.hooksPath` 的逻辑，在 doctor 里加一项对应的只读检查。

**D3：审批/豁免入口完全不存在**
`routing/secrets.rs:2193` 的提示文案让用户加 `audit-risk:ignore` 注释，但整个扫描逻辑里没有任何代码解析这个字符串（已用 grep 确认零匹配）。`require_approval` 目前只是一个退出码（2），没有审批人、理由、有效期、commit SHA 绑定，也没有实际的放行命令。

这个功能本身工作量较大，如果时间不够可以只做最小闸门版本：
1. 先决定要不要真的实现行内注释解析（简单）还是做一套独立的 `accept-risk`/`suppress` 命令（更完整但工作量大）。**这个决策请先跟主项目负责人确认，不要自己定，因为它涉及产品形态**（是"代码里加注释绕过"还是"必须走审批命令留痕"，两种设计对"不可绕过治理门"这个产品主张的含义完全不同）。
2. 如果决定做注释语法：在 `routing/secrets.rs` 扫描逻辑里，检测到某一行有 finding 时，检查同一行或上一行是否包含 `audit-risk:ignore` 注释，如果有就跳过该 finding，但必须把"这条 finding 被注释豁免"这件事写入审计链（依赖会话 B 的审计写入函数）——否则豁免本身也是不留痕的，等于产品自己开了后门。
3. 如果决定做审批命令：新增子命令，比如 `audit-risk approve --finding <id> --reason <text> --expires <date>`，写入一条审批记录到 audit.jsonl（同样依赖会话 B），后续 check/report 读取到匹配的审批记录时把该 finding 的 gate_effect 降级。

**依赖关系**：D3 第 2、3 步都依赖会话 B 先把审计写入函数做出来。如果会话 B 还没完成，D3 先做 D1、D2，D3 留到最后，或者先把 D3 的接口签名定义好但暂时打日志代替真正写审计链，等 B 完成后再接上。

---

## 会话 E：GitHub Action 修复 + CI 审查模型 + 产品口径统一

**E1：`action.yml` 重复实现了一套更弱的下载逻辑**
`.github/actions/audit-risk/action.yml:86` 直接 `curl -fsSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/audit-risk"`，全程没有下载或校验 `checksums.txt`。对比项目根目录的 `install.sh:94-129`，那边已经写好了完整的 checksum 校验逻辑（下载 checksums.txt、sha256sum/shasum 比对、不匹配则 `err` 退出）。

**注意：不是"两边都没做"，是 action.yml 这一处独立重复实现，且漏了 install.sh 已经有的东西。** 修复方向优先考虑让 action.yml 直接调用 `install.sh`（比如 `curl ... install.sh | sh -s -- --version "$VERSION" --prefix "$INSTALL_DIR"`），而不是在 yml 里维护第二份下载脚本逻辑。这样两处永远同步，不会再出现类似遗漏。

**E2：action.yml 版本号解析会撞 GitHub API 限流**
`action.yml:76-78` 用 `curl https://api.github.com/repos/.../releases/latest -H "Authorization: token ${{ github.token }}"` 拿最新版本号。`install.sh` 那边同样的问题在本次会话中已经改成用 `curl -sSfL -o /dev/null -w '%{url_effective}' https://github.com/.../releases/latest | sed 's|.*/tag/||'`（走 redirect URL，不查 API，不受 API rate limit 影响，见 `install.sh:74-79`）绕开了。action.yml 里这处没同步这个修复方案。如果按 E1 的方向改成直接调用 install.sh，这个问题自动解决；如果不采用 E1，至少要把这个 redirect 方案搬过来。

**E0：stderr 和 JSON 输出混进同一个文件，会污染报告解析（已用代码核实确认）**
`action.yml:106` 这一行：`audit-risk check "$WORKSPACE" --json --fail-on "$FAIL_ON" > "$REPORT_PATH" 2>&1`，`2>&1` 把 stderr 重定向到跟 stdout（JSON 输出）同一个文件里。如果 `check` 命令执行过程中往 stderr 打了任何一行文字（比如 baseline 文件损坏的警告、某个规则加载失败的提示），这行文字会插进 JSON 文件中间，后面 `action.yml:111` 的 `jq -r '.review.gate_decision.decision // "unknown"' "$REPORT_PATH"` 解析就会失败，`jq` 报错但 `2>/dev/null || echo "unknown"` 这个兜底（`action.yml:112-113`）会悄悄吞掉错误，把结果显示成 `unknown`——用户看到的是"门禁状态未知"，但真实原因是一条无关的警告污染了 JSON，不是扫描本身有问题。
修复方向：改成 `audit-risk check "$WORKSPACE" --json --fail-on "$FAIL_ON" > "$REPORT_PATH" 2>"$RUNNER_TEMP/audit-risk-stderr.log"`，把 stderr 单独写到另一个文件，JSON 文件只保留纯 JSON。如果后续需要展示警告信息，从这个单独的 stderr 文件读取，不要和结构化输出混在一起。这条如果按 E1 的方向改成直接调用 install.sh 之外的部分（这条跟 install.sh 无关，是 action.yml 自己调用 audit-risk 的那一步），务必同步修掉，不要漏。

**E3：PR 评论字段可能读到空值 + 声称的 artifact 未上传**
action.yml:112-113 用 `jq -r '.review.gate_decision.decision // "unknown"'` 和 `.review.findings | length // 0` 解析报告。要核实这两个 JSON path 是否和 `run_check_command`（`--json` 模式下输出的 payload 结构，`cli.rs:704` 附近 `CommandOutcome::json(exit_code, payload)`）真实字段路径一致——如果 check 命令的 JSON 输出结构后续被会话 A 改动过（比如加了 policy 相关字段），这里的 jq path 要跟着核对，不要假设没变。另外评论文案里如果提到"完整报告见 Actions 产物"，要确认真的有一步 `actions/upload-artifact` 上传了 report，如果没有就要加上，或者去掉这句误导性的文案。

**E4：CI 审查模型的本质问题——没有真正比较 base 和 head**
当前 CI workflow 模板（`DEFAULT_CI_WORKFLOW_PATH` 对应内容，搜索定位）和 action.yml 都是对 clean checkout 跑一次 `check`，`check` 内部靠 `git status --short` 判断"变更文件"，但 PR 场景下 checkout 后工作区是干净的，`git status` 不会有任何输出，`changed_files` 会是空列表。这意味着 CI 现在很可能压根没有正确识别出 PR 引入了哪些新文件/新风险，只是在扫整个仓库当前状态或者扫出空结果。

修复方向：CI 场景需要显式对比 `base SHA` 到 `head SHA` 的 diff（`git diff --name-only ${{ github.event.pull_request.base.sha }} ${{ github.event.pull_request.head.sha }}`），而不是依赖 `git status`。这需要给 `check` 命令加一个"从 diff 而不是 status 拿变更文件列表"的模式，或者复用会话 C3 里可能已经加的"接受外部文件列表"参数（如果 C 会话先做完，E 会话应该直接用那个参数，不要重复造轮子——**动手前先确认 C 会话有没有做完，避免重复实现**）。同时要建立"旧风险走 baseline 渐进收紧、新引入的风险严格阻断"的区分，这也依赖能明确知道"这个 finding 是不是本次 diff 新引入的"。

**E5：产品口径统一（这是决策事项，不是纯技术修复）**
`cli.rs:34` 定义 `PRO_PERSONAL_PRICE_LABEL = "29 元/月"`，`pro_status_label`（`cli.rs:5484`）、`render_help_screen`（`cli.rs:4188`）、`render_home_screen` 等多处都在展示"Pro 个人版 29 元"这套定价（`cli.rs:3418`、`4193`、`4226`、`4768`、`4823` 等，用 `grep -n "Pro 个人版\|29 元"` 能全部定位到）。而 README 现在是 `Core / Team ¥299/月 / Enterprise` 的团队治理路线，`tour_text()`（本次会话已改）也用的是 Team 口径。**这是两套完全不同的产品线并存，不是文案不统一这么简单——`entitlement.rs` 里的 plan 常量、gate 逻辑都是按"个人 Pro 订阅"设计的，不会自动对应"团队按仓库数收费"的模型。**

这一条**不要直接动手改代码**，先向主项目负责人确认最终商业路线（个人订阅 vs 团队治理），因为改动方向完全不同：如果走团队路线，`entitlement.rs` 里 `PRO_PERSONAL_PLAN`、`ENTITLEMENT_GRACE_HOURS` 这套个人订阅的授权模型可能要整体换成"按仓库/按 workspace 授权"的模型，工作量远超文案替换。这个会话如果被分配到 E5，应该先输出一份"两种路线各自要改哪些文件、工作量差异"的评估报告，而不是直接选一个方向动手改。

---

## 补充：会话 C 追加两条 P1（已用代码核实，非报告转述）

如果会话 C 做完 C1-C3 还有余量，按顺序追加这两条，比 C4 优先：

**C5：`NO_COLOR`/`CLICOLOR`/`TERM` 环境变量完全不生效**
`decide_render_mode`（`cli.rs:3923`）只根据终端宽度和是否为 TTY 二选一分支（`Plain` vs `Boxed`），不检查任何颜色相关环境变量——全文件 grep `NO_COLOR`/`CLICOLOR`/`TERM` 零匹配。更关键的是：即便窄终端触发降级到 `Plain` 模式（`cli.rs:3951-3969`），代码里仍然照样拼接 `title`/`text`/`muted`/`bright`/`green`/`yellow`/`blue` 这些 ANSI 颜色变量——`Plain` 只去掉了边框绘制字符，没去掉颜色码。也就是说不管终端宽度、不管是否重定向到文件，颜色码永远输出。
修复方向：在 `render_product_shell`（`cli.rs:3930`）入口处检测 `std::env::var("NO_COLOR").is_ok()`（NO_COLOR 标准约定：只要这个变量存在且非空字符串就应禁用颜色，不需要判断具体值）或 `std::env::var("TERM").as_deref() == Ok("dumb")`，命中时把 `bg`/`panel`/`border`/`title`/`text`/`muted`/`bright`/`green`/`yellow`/`blue`/`reset` 这些变量全部替换成空字符串，而不是引入新的颜色库。`read_code_snippet`（`cli.rs:4268`）和 `render_check_screen` 里 `cli.rs:4325` 那两处硬编码的 `\x1b[31m`/`\x1b[33m` 转义也要同步处理，可以抽一个 `fn color_enabled() -> bool` 共享判断逻辑，避免每处各写一份。

**C6：`--fail-on off` 时屏幕显示和退出码矛盾**
`gate_exit_code`（`cli.rs:2274`）：`fail_on == FailGate::Off` 时直接 `return 0`，不管实际 gate 判定是什么。但 `render_check_screen`（`cli.rs:4372` 起）显示"审查结论"的文案完全基于原始 `gate` 值（比如 block 时显示"这次变更已经达到阻断阈值，继续提交会把风险带进主线"），不看 `fail_on` 参数。后果：用户传了 `--fail-on off`，退出码是 0（脚本认为"通过"），但屏幕上原样显示"已阻断"的强烈措辞，人读了会以为出了问题，跟脚本的判断相反。
修复方向：`render_check_screen` 需要知道调用时传入的 `fail_on` 是什么（目前这个函数签名里没有这个参数，只接收 `payload` 和 `verbose`，需要加一个参数或者把 `fail_on` 塞进 payload 里），当 `fail_on == Off` 时，在"审查结论"这行后面附加一句类似"（--fail-on off：本次不会导致退出码非零，仅供参考）"，避免文案和退出码传达相反的信息。

## 补充：本次交接**未覆盖**、故意跳过的 P1 项（记录在案，不阻塞本轮）

以下是 codex 报告里点出但这次没有分给任何会话、纯口头判断"改动量大、暂不做"的项，列出来是为了不让它们悄悄消失，等这轮 P0 收尾后再排期：

- 大多数子命令 `--help` 报错（比如 `check --help` 会报缺 workspace 而不是打印帮助），这是整个 CLI 参数解析层的问题，不是一两行能改完的，需要单独立项评估工作量。
- 报告不脱敏（含用户绝对路径），没有对外分享模式。
- 没有 `upgrade`、配置迁移、`rollback`、`uninstall`、`support-bundle`、shell completion。
- Windows 只有文件名下载，没有 PowerShell/PATH/升级/卸载文档，没有 Homebrew/Scoop/winget。
- `SECURITY.md`/`CHANGELOG.md`/`CONTRIBUTING.md` 里混有旧 HoloGram、0.1.x、桌面端遗留内容和失效链接，没重新审阅。
- 没有 Required Status Check / Branch Protection 的接入指引，"不可绕过"这个产品主张目前只是 hook 层面的，没有 GitHub 侧强制。
- `watch` 命令重复展示同一批 finding、Ctrl-C 退出码和"已停止"确认缺失、结构风险的 `location` 显示 `app.js:0` 这类内部占位符——这几条影响面偏视觉/体验，优先级明显低于本轮所有 P0 和已列的 P1，暂不安排。

## 收尾：所有会话完成后

1. 主项目负责人逐个 review 5 个会话的 diff，重点看会话 A 和 B 是否有冲突（都改了 `build_workspace_check_payload` 附近）。
2. 全部合并后跑一次完整 `cargo test --lib`，确认通过数不低于 477。
3. 重新走一遍最小验收路径：`init → doctor → check → report`，确认 `report --output` 生成的报告里 `audit.entry_count` 不再是 0（验证会话 B 的成果），`check` 和 `report` 对同一 workspace 的 `gate_decision` 一致（验证会话 A 的成果）。
4. E5 的产品路线决策定下来后，回头统一清理所有定价文案，这一步依赖决策结果，不要提前动手。
