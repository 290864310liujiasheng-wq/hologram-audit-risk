<p align="center">
  <img src="assets/banner.png" alt="audit-risk" />
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" /></a>
  <a href="https://github.com/834063245-creator/HoloGram/actions"><img src="https://img.shields.io/badge/tests-361%20passed-brightgreen" /></a>
  <a href="https://github.com/834063245-creator/HoloGram/releases"><img src="https://img.shields.io/github/v/release/834063245-creator/HoloGram?label=latest" /></a>
  <a href="https://github.com/834063245-creator/HoloGram/pulls"><img src="https://img.shields.io/badge/PRs-welcome-brightgreen" /></a>
</p>

> **AI 编码风控平台**
>
> 为 AI 生成的代码提供实时审查、规则拦截和不可篡改的审计证据。

---

## 安装

**一行安装（macOS / Linux）：**

```sh
curl -sSf https://raw.githubusercontent.com/834063245-creator/HoloGram/main/install.sh | sh
```

安装到自定义路径（不需要 sudo）：

```sh
curl -sSf https://raw.githubusercontent.com/834063245-creator/HoloGram/main/install.sh | sh -s -- --prefix ~/.local
```

安装指定版本：

```sh
curl -sSf https://raw.githubusercontent.com/834063245-creator/HoloGram/main/install.sh | sh -s -- --version v0.2.0
```

**手动下载：** 从 [Releases](https://github.com/834063245-creator/HoloGram/releases) 页面下载对应平台的预编译二进制：

| 平台 | 文件名 |
|---|---|
| macOS Apple Silicon | `audit-risk-macos-arm64` |
| macOS Intel | `audit-risk-macos-x64` |
| Linux x64 | `audit-risk-linux-x64` |
| Linux ARM64 | `audit-risk-linux-arm64` |
| Windows x64 | `audit-risk-windows-x64.exe` |

**从源码构建：**

```sh
git clone https://github.com/834063245-creator/HoloGram.git
cd HoloGram/engine
cargo build --release --bin audit-risk
# 二进制在 target/release/audit-risk
```

---

## 一句话

`audit-risk` 是一个面向 Codex、Cursor、Copilot、CodeGeeX 等 AI 编码工具的通用本地风控层。它持续审查 AI 生成或修改的代码，给出白话解释、修复建议、审批信号和可追溯的审计记录，而不是再造一个新的 IDE。

## 电梯演讲

- Apiiro 是商业 CLI，我们是开源 CLI + 纯本地合规证据链。
- Meta CodeShield 是扫描引擎，我们是在它这类底层扫描能力之上，补齐白话解释、修复方案验证、审批流和审计哈希链的风控平台。

## 我们和别人的区别

| 差异化 | 说明 |
|---|---|
| **白话解释 + 一键修复** | 不只报规则编号或漏洞名；每个 finding 都要解释位置、原因、影响和建议，并把修复方案纳入受控流程。 |
| **纯本地 + 不可篡改审计日志** | 客户接入自己的模型和环境，代码、密钥、审计记录不需要交给平台统一托管；审计链路为后续哈希链和签名校验预留边界。 |
| **跨 AI 工具通用** | 主交付形态是 CLI，可被终端、Git Hook、CI/CD 以及主流 AI 编码工具的扩展命令统一调用。 |

## 核心能力

- **实时风险审查**：对 AI 生成代码、工作区变更和 Git diff 持续审查。
- **规则拦截**：按规则把风险动作收口为 `allow`、`warn`、`require_approval`、`block`。
- **白话解释**：每个 finding 必须说明原因、影响和建议，而不是只给扫描器结果。
- **受控自修复**：生成修复方案后，先做二次审计、验证和审批，再决定是否允许应用。
- **多代理并行审计**：把静态结构、依赖影响、权限/供应链、测试回归等审计面并行化，再由主审查器汇总。
- **机器可读集成**：输出结构化 JSON 和退出码，方便接入 Git Hook、CI/CD 和外部平台。
- **本地审计证据**：所有关键决策、审批、修复和回滚都进入 append-only 审计链路。
- **轻量观察模式**：`watch --observe` 会起本地只读观察页，并打印本地/LAN 地址与二维码图片路径，便于手机或旁路观察。

## 当前产品形态

- **主产品**：通用 CLI `audit-risk`
- **主要接入方式**：终端、Git Hook、CI/CD、外部 AI 工具扩展命令
- **桌面版角色**：继续保留，但定位调整为安全负责人和团队管理者使用的管理后台，用于规则配置、审计检索、团队看板和导出报告

## 当前公开命令

- **开发者主路径**：`audit-risk check`、`audit-risk watch`、`audit-risk diff`、`audit-risk init`、`audit-risk doctor`
- **管理员/集成路径**：`audit-risk report`、`audit-risk rules`、`audit-risk audit`、`audit-risk verify`
- **授权与个人版路径**：`audit-risk`（零参数中文首页）、`audit-risk help`、`audit-risk tour`、`audit-risk auth login`、`audit-risk auth status`、`audit-risk auth logout`

## Core 与 Pro

- **Core 免费版**：保留零参数中文首页、`help/tour`、`init`、`doctor`、`check`、`watch`、`diff`、基础白话解释、基础修复建议与 diff 预览、修复方案二次审计验证、基础规则包、JSON/Markdown 基础报告。
- **Pro 个人版**：29 元/月，解锁高级规则包、历史风险对比、增强报告导出、审计日志哈希链签名导出包、`observe`、`notify`、个人规则自定义加载。
- **当前仓库边界**：这个 MIT Core 仓库先实现 CLI 侧授权合同、本地 entitlement 状态机、中文 gate 和登录入口；不会在服务端未接入时伪造 Pro 授权，也不会把闭源 Pro 简化成同仓库里的一层 if/else 开关。真实 auth/payment 服务端、29 元/月订单、支付回调、续费和撤销样本仍需做外部 fresh 验收；仓库内已提供合同样例 `docs/auth-payment-live-samples.json` 和联调脚本模板 `scripts/auth-payment-live-verification.sh`。

## 仓库当前状态

这个仓库正在把既有的 HoloGram 桌面基座收敛为 AI 编码风控平台：

- Rust 引擎、tree-sitter 多语言分析、规则命中、审计、修复链路继续保留
- 既有桌面壳和前端工作台继续作为管理后台基座
- 主交付链正在从既有桌面基座收敛到 CLI-first

换句话说，当前仓库里同时存在：

- **已保留的内核**：证据引擎、审查合同、规则系统、审计系统、自修复 owner
- **待收口的外层**：CLI 命令面、初始化/诊断体验、守护输出、对外接入文案

## 当前可复用基座

- **Rust 引擎**：代码分析、依赖影响、静态信号、watcher、MCP/TCP 服务底座
- **风险核心**：`ReviewJob`、`ReviewFinding`、`GateDecision`、`RulePackage`、`RepairPlan`、`PatchProposal`
- **审计平面**：append-only 审计日志、统一审计查询读模型、修复/审批/回滚证据
- **Provider 与代理编排**：客户自带模型 provider、多代理审计和权限门禁

## 适用场景

- 开发者在本地配合 Codex、Cursor、Copilot 等 AI 编码工具写代码
- 团队希望在提交前对 AI 生成代码做风险拦截
- 安全或平台团队需要保留审批和审计证据，而不是只跑一次扫描
- 组织希望保留纯本地部署和自带模型 API 的边界

## 技术方向

当前单一推荐架构是：

```text
AI Tool / Git Diff / Workspace Change
  -> Evidence Collector
  -> Rule + Static Signal Engine
  -> Model Review Provider
  -> Risk Aggregator
  -> Gate Decision
  -> Audit Trail
  -> CLI Output / Hook / CI / Admin Console
```

实现上继续复用：

- `engine/`：Rust 证据引擎和多语言分析底座
- `src-ui/src/risk/`：风险合同、规则、审计查询、自修复 owner
- `src-tauri/`：本地能力和既有桌面壳基座

## 设计原则

- 不做平台统一托管的模型额度服务
- 不做通用聊天机器人
- 不把风险语义藏在 prompt 或 UI 文案里
- 不让 CLI、桌面后台、Hook、CI 各自维护一套风控结论
- 不在没有证据、验证和审批的情况下宣称“自动修复”

## 当前文档口径

- 对外 README 以本页为准
- 内部产品和架构真源以 `dev-docs/README.md` 及其索引文档为准
- 旧 HoloGram 叙事只作为历史基座说明，不再作为当前产品定位
