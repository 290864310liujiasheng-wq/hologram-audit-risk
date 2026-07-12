# audit-risk 产品路线图 2026

> 本文档是产品方向的唯一真源。所有新功能必须能归属到这里的某个阶段。  
> 最后更新：2026-07-12

---

## 定位

**audit-risk 是 AI 代码进入主分支前不可绕过的治理门。**  
不管代码由 Codex、Cursor、Copilot、通义灵码还是任何 AI 工具生成，高风险代码在合并前必须经过团队规则判断，留下可复核的证据链。

---

## 产品分层

| 版本 | 价格 | 包含 |
|---|---|---|
| Core | 免费 | CLI check/watch/diff + 基础规则包 |
| Team | ¥299/月（按仓库数） | + GitHub Action + MCP Server + LLM 审查 + 约束注入 |
| Enterprise | 合同制 | + HTTP 代理 + 数据飞轮 + 预测引擎 + 自进化规则 |

---

## 四层治理架构

```
【第 0 层】约束注入（预防）
  MCP Server → 拦截 AI prompt → 注入团队约束 → 转发
  HTTP 代理  → 所有 AI 工具兜底（Enterprise 专属）
  约束格式：规则 + 原因 + 替代方案（三要素，防 AI 误解）
  透明性：代码顶部自动追加注释说明已注入的约束清单

【第 1 层】生成后检测（治疗）
  一层：正则 + AST 快速过滤（毫秒级，现有引擎）
  二层：LLM 语义审查（DeepSeek，仅对第一层标记项，客户自带 Key）
  三层：AI 代码风险特征模式匹配（Month 4 开始）
  输出：allow / warn / require_approval / block
  原则：非阻塞异步检查，只有严重违规（生产密钥）才同步阻塞

【第 2 层】门禁执行（不可绕过）
  GitHub Required Status Check（管理员才能关闭）
  PR Comment：结构化风险报告 + 中文解释 + 修复建议
  CODEOWNERS 集成：自动分配审批人
  Commit 完整性验证：防止审批后偷换代码

【第 3 层】审计 + 数据飞轮（自进化）
  SHA-256 哈希链审计（已完成）
  数据采集：误报标记 / 审批结果 / 修复成功率（MCP 上线即开始）
  团队专属模型微调（Month 5，Qwen-7B 或 Llama-8B）
  预测式约束（Month 6，需 10000+ 拦截数据验证）
  自进化规则（Month 12，pattern mining，人工审核确认后部署）
```

---

## 开源策略

| 组件 | 策略 | 理由 |
|---|---|---|
| core 引擎（tree-sitter + 正则）| MIT 开源 | 建立信任，社区发现 bug |
| 基础规则包（secrets/sql/dangerous/iam）| MIT 开源 | 获客，降低接入门槛 |
| MCP Server 实现 | 闭源 | 约束注入是核心竞争力 |
| AI 代码风险特征库 | 闭源 | 护城河 |
| 审计链路 + 飞轮引擎 | 闭源 | 核心商业价值 |
| 预测式约束模型 | 闭源 | 护城河 |

---

## 交付时间线

### Week 1-2：GitHub Action（第一个可销售的团队版）

**目标**：一个 5-20 人团队能在 30 分钟内把 audit-risk 接进 CI，Required Status Check 生效。

**交付物**：
- `.github/actions/audit-risk/action.yml`（GitHub Action 定义）
- 自动下载对应平台预编译二进制
- `audit-risk check --json --fail-on require_approval` 输出解析
- PR Comment：命中规则 + 风险等级 + 中文解释 + 修复建议
- GitHub Check Run 状态上报
- README 接入文档（30 分钟接入指南）

**不做**：审批 UI、多租户、GitLab、MCP

**验收标准**：
```yaml
# 客户在 .github/workflows/audit.yml 里写这一行就接入完毕：
- uses: audit-risk/action@v1
  with:
    fail-on: require_approval
```

---

### Week 3-4：MCP Server MVP（差异化）

**目标**：Cursor / Claude Code 用户在 AI 生成代码时自动注入团队约束。

**交付物**：
- MCP Server（Rust 实现，随 CLI 二进制分发）
- 读取 `.hologram/rules/` 生成约束 prompt
- 约束格式：`规则：XXX。原因：XXX。替代方案：XXX。`
- 代码顶部注释透明化已注入约束
- 数据采集：记录每次 prompt 特征和是否触发违规（为飞轮准备）

---

### Week 5-6：LLM 审查默认开启

**目标**：check 命令默认调用客户配置的 LLM 做语义审查，不再是 opt-in。

**交付物**：
- delivery.json 里 provider 配置完整后自动启用 LLM 二层审查
- 只对第一层（正则+AST）标记的 finding 调用 LLM（控制成本）
- 中文白话解释质量提升

---

### Week 7-8：PR Comment 格式优化 + CODEOWNERS 集成

**目标**：审批人自动分配，不需要手动 at 人。

---

### Month 2-3：开源 core 引擎正式发布

**目标**：GitHub 上达到 500+ star，建立社区信任。

**交付物**：
- 独立的 `audit-risk-core` 仓库（或 monorepo 拆分）
- 完整的检测能力文档
- 贡献指南：如何添加新规则、新语言支持

---

### Month 3-4：AI 代码风险特征模式库

**目标**：识别 AI 生成代码的典型风险模式（不是识别哪个工具生成的，而是 AI 生成代码的高频风险特征）。

---

### Month 5-6：数据飞轮引擎

**前提**：MCP Server 上线后累积 10000+ 拦截记录。  
**目标**：每个团队有专属风险基线，用了 6 个月的团队规则比新团队更准。

---

### Month 6：预测式约束（满分形态）

**前提**：通过实验验证 prompt 特征与违规的相关性（不是假设，是数据结论）。  
**目标**：AI 还没生成代码，已知道要注入什么约束。

---

### Month 12：自进化规则

**目标**：pattern mining 发现新的高频违规模式 → 自动生成规则候选 → 人工审核 → 部署。  
**不是 AGI，是聚类算法 + 人工确认。**

---

## 当前已完成的技术基础

- ✅ CLI 二进制自包含（无 Node.js 依赖）
- ✅ 30 种语言 tree-sitter AST 解析
- ✅ 正则扫描：召回 100%，误报 0%（37/37 bad, 12/12 clean）
- ✅ SHA-256 哈希链审计（完整性验证）
- ✅ repair approve/apply 安全工作流
- ✅ `audit-risk init` 生成 Git Hook + CI 模板
- ✅ 476 个测试，0 失败
- ✅ 跨平台预编译二进制（macOS arm64/x64, Linux x64/arm64, Windows x64）

---

## 现在唯一悬而未决的事

在开始 Week 1-2 之前，必须找到 **至少 1 个愿意在真实仓库里接入的团队**。  
一个真实团队 Week 1 的反馈，会改变 50% 的架构决定。  
这是商业问题，不是技术问题。技术随时准备好了。
