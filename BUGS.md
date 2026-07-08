# BUGS.md — 活 bug 清单

> 用户写，Agent 修。格式：`[级别] 面板/功能 — 现象`  
> 级别：A=核心三条挂了 · B=每天用但坏了 · C=能忍 · D=几乎不用

---

## 待修（B 级 — 每周最多修 1 个）

**当前焦点：简报 + 时间轴 — 2026-06-18 已修（待验收）**

### 简报（CheckPanel） — 已修

- [x] 信号改为 **delta**（L2 循环 / L4 耦合只在比上次基线增加时报警）
- [x] 打开项目：无变更 →「无新变更」；首次 → 建基线，不扫全项目
- [x] 基线 `.hologram/baseline.json`：分析后 seed + 每次简报后 advance（不再「只有通过才写」）

### 时间轴（TimelinePanel） — 已修

- [x] `runCheck` / 图更新后 `bus.emit('timeline:refresh')`
- [x] 简报结果写入 timeline 时带完整 CheckResult properties（点时间轴可打开历史简报）

<!-- 新 B 级 bug 写下面 -->

## 待修（C 级 — backlog，30 天不动）

> ⏸ 已冻结（见 FROZEN.md，复审 2026-10-08）：星图/桌面端相关，解冻桌面端时一并处理。

- [C] ⏸ 折叠视图 — 星系叠加过曝
- [C] ⏸ 星图 — 跨星系连线/粒子看不见

## 待修（A 级 — 立刻修，别的全停）

<!-- 例：[A] 打开项目 — 分析完 0 节点 -->



## 已修（归档）

### 2026-07-08 — 聚焦收敛（修断点 + 降噪 + 检测质量基线 + 冻结）

按「一条主线」原则收敛（见 FROZEN.md）：

- [x] [A] 对外链路断点：README/install.sh/CONTRIBUTING/SECURITY/vscode 全部从旧仓库 `834063245-creator/HoloGram` 改指向 `290864310liujiasheng-wq/hologram-audit-risk`；README 安装以源码构建为首选（Release 未发布）
- [x] [A] 建检测质量基线：`engine/tests/detection_corpus/`（35 必检出 / 12 干净 / 10 已知盲区）+ `detection_quality` 回归测试；实测召回 94.3%→**100%**、误报 8.3%→**0%**
- [x] [B] 检测漏报：bare `execSync(` 未识别 → 补规则；SQL 拼接字符串内含引号导致漏检 → 按引号类型拆分规则
- [x] [B] 检测误报：占位符（`changeme`/`<YOUR_KEY_HERE>`）被当密钥 → 加占位符denylist
- [x] [B] check 噪音：默认只显示高置信度（密钥/注入/危险执行），结构耦合信号折叠，`--verbose` 看全部
- [x] [B] CI 空壳：`init` 生成的 workflow 从 `echo TODO` 改为可运行的 install + `check --fail-on block`
- [x] 冻结桌面端/observe/notify/auth-payment/vscode 新功能（FROZEN.md，复审 2026-10-08）

### 2026-07-08 — 客户视角走查 P0-P2 全面收口（CLI 上手体验）

以真实客户身份从 README → init → doctor → check → diff 走查，修掉「前 10 分钟必踩」的坑（提交 `5f0e34b`）：

- [x] [A] check — 未跟踪文件扫不到密钥：`git status` 加 `--untracked-files=all`，AI 新生成文件现在能扫出密钥/注入
- [x] [A] check — 工具自身脚手架（`.hologram/`、生成的 hook/CI）被误报为「严重」并阻断：`run_full_check` 过滤自身文件，噪音 5→0
- [x] [A] check — 致命发现被噪音淹没：finding 按严重度+具体位置+语义排序，密钥/注入置顶；L3「shared data」改可读中文、清理节点 ID
- [x] [A] init — 生成的 pre-commit 钩子在客户机必坏（硬编码开发机路径 + cargo run）：改用 PATH 里的 audit-risk；init 自动设 `core.hooksPath` 激活钩子
- [x] [B] diff — 无参/`--help` 报英文无提示：改中文用法+示例，提示「审查 Git 改动用 check .」
- [x] [B] diff — 人类模式只报数量不展示明细：现在展示 finding 列表
- [x] [B] init — db/baseline 脏化 git：生成 `.hologram/.gitignore`
- [x] [C] doctor — 建议蹦英文：改中文 + 补上下文（provider 密钥、auth 地址）
- [x] [C] help — 缺 diff、无用途说明：补齐并标注 check/diff 区别
- [x] [C] 节点 ID 泄露绝对路径：`short_symbol` 只取末段符号名

### 更早

- [x] 变更 diff 基线空图 — 2026-06-17
- [x] 权限卡片键盘残留 — 2026-06-17
- [x] 文件树不实时更新 — 2026-06-17
