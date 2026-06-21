# 验收口径

## Stop Conditions

本阶段只验收真源文档和架构边界，不声明业务代码已实现。

必须满足：

- `AGENTS.md` 指向 AI 编码风控平台主线。
- `dev-docs/README.md` 索引全部 active docs。
- `product-charter.md`、`current-state-audit.md`、`architecture.md`、`domain-model.md`、`contracts.md` 相互一致。
- Sliver adoption guardrail 检查通过。
- 旧 HoloGram 语义只作为现有基座或历史外部说明出现，不覆盖当前产品边界。

## 后续代码阶段验收

按触达面选择最小相关 gate：

- Review Core：合同测试、状态流转测试、finding 必须关联 evidence。
- Rule/Policy：规则命中、gate decision、block/approval/audit 路径测试。
- Provider：无明文 key、provider 失败降级、超时和结构化错误测试。
- Audit：append-only、敏感信息过滤、失败可见。
- UI：构建通过，浏览器或桌面预览验证风险列表、逐行解释、审批和审计视图。
- Multi-agent：子代理超时、重复 finding 去重、冲突裁决、主代理汇总测试。
- Self-healing：repair plan、patch proposal、测试 gate、审批、apply、rollback 证据。

## Evidence Log

- 2026-06-20：读回 `AGENTS.md`、`dev-docs/README.md`、`architecture.md`、`contracts.md`、`product-charter.md`、`domain-model.md`。
- 2026-06-20：`rg` 检查 HoloGram/旧产品语义，确认旧语义主要留在公开 README/docs 和新文档的历史边界说明中。
- 2026-06-20：第一次 Sliver adoption guardrail 未通过，缺 `current-state-audit.md`、`acceptance.md` 和模板固定 heading；已补齐。
- 2026-06-20：Review Core RED：`npx tsx src/risk/review-core.test.ts` 失败于 `ERR_MODULE_NOT_FOUND`，确认测试先于实现。
- 2026-06-20：Review Core GREEN：`npx tsx src/risk/review-core.test.ts` 通过 3 条合同测试。
- 2026-06-20：类型门：`npx tsc --noEmit` 通过。
- 2026-06-20：前端构建：`npm run build` 通过；仍有既有 Vite dynamic/static import 与 chunk size 警告。
- 2026-06-20：Review Core 第二轮 RED：新增 `ReviewJob` 相关测试后，`node --import tsx src/risk/review-core.test.ts` 失败于缺少 `finalizeReviewJobResult` 导出。
- 2026-06-20：Review Core 第二轮 GREEN：`node --import tsx src/risk/review-core.test.ts` 通过 8 条合同测试。
- 2026-06-20：Review Core 第三轮 RED：新增 `Rule`/`AuditEvent` 相关测试后，`npm run test:risk` 失败于缺少 `createAuditEvent` 导出。
- 2026-06-20：Review Core 第三轮 GREEN：`npm run test:risk` 通过 10 条合同测试。
- 2026-06-20：Check adapter RED：`node --import tsx src/risk/check-adapter.test.ts` 失败于缺少 `check-adapter` 模块。
- 2026-06-20：Check adapter GREEN：`npm run test:risk` 通过 13 条风险核心/桥接测试。
- 2026-06-20：Check summary RED：`node --import tsx src/risk/check-adapter.test.ts` 失败于缺少 `buildCheckRiskSummary` 导出。
- 2026-06-20：Check summary GREEN：`npm run test:risk` 通过 14 条风险核心/桥接测试。
- 2026-06-20：UI 读接线：`check.ts` 已消费 `buildCheckRiskSummary`，`npx tsc --noEmit` 与 `npm run build` 通过。
- 2026-06-20：实机预览部分完成：已在本机 Chrome 打开 `http://127.0.0.1:4173/` 并确认 mock 工作台可打开；进一步 DOM 抽取受本机浏览器/脚本权限限制，未把 `风控摘要` 文本作为强证据记录。
- 2026-06-20：Audit bridge RED：`node --import tsx src/risk/audit-bridge.test.ts` 失败于缺少 `audit-bridge` 模块。
- 2026-06-20：Audit bridge GREEN：`npm run test:risk` 通过 16 条风险核心/桥接测试。
- 2026-06-20：Rust 编译门：`cargo check` 通过；为满足本机 Tauri 构建资源校验，`src-tauri/build.rs` 已在开发态自动补齐缺失的 engine resource。
- 2026-06-20：Approval audit 写路径：`Workspace` approver 已接入 `approval_requested / approval_resolved` 的 timeline + audit 落盘；`npx tsc --noEmit`、`cargo check` 通过。
- 2026-06-20：Agent 审计消费：新增 `audit_recent_reviews` 与 `current_review_summary` 工具；前端构建通过。
- 2026-06-20：强运行态 UI 证据：真实 Chrome 窗口标题显示 `🔮 风控4 审计2 — 全息观测站`；状态栏显示 `风控4 · 审计2`；简报面板可见 `最近审计`、`风控摘要`、`违规`、`自动放行` 区块。
- 2026-06-20：多代理/自修复 RED-GREEN：新增 `multi-agent.test.ts` 与 `self-heal.test.ts` 后，`npm run test:risk` 先失败再通过；当前 `test:risk` 共通过 28 条合同/桥接测试。
- 2026-06-20：current review 收口：新增 `current-review.ts` 后，`current_review_summary` 已返回 review 聚合对象而非只读摘要；`npx tsc --noEmit` 通过。
- 2026-06-20：前端代码门：`npm run build` 通过；仍有既有 Vite dynamic/static import 与大 chunk warning，无新增 build blocker。
- 2026-06-20：Rust 代码门：`cargo check` 通过；仍有既有 warning，无新增 Rust 编译失败。
- 2026-06-20：运行态 UI 证据增强：真实 Chrome 打开 `http://127.0.0.1:4174/`，窗口标题显示 `🔮 风控4 审计3 — 全息观测站`，状态栏显示 `风控4 · 审计3`，最近审计文本包含 `修复允许`，并可见 `多代理审计`、`自修复闭环` 区块。
- 2026-06-20：桌面壳启动证据：`cargo run` 在 `src-tauri/` 下完成编译并启动 debug app，日志出现 `Running target/debug/hologram` 与 `[unity-events] listening on 127.0.0.1:9776`；OS 级截图可见 `全息观测站` 桌面窗口已被拉起。
- 2026-06-20：真实 repo graph/cache：通过本机 engine TCP analyze 为 `repo/` 生成 `hologram_graph.json`，结果为 `5017` 节点 / `9525` 边；同时写入 `.last_project` 指向当前 repo。
- 2026-06-20：真实 repo 审计证据：`repo/.hologram/audit.jsonl` 存在，最新 `review_check` 记录路径为当前 repo，原文原因为 `Review check passed without blocking findings.`。
- 2026-06-20：真实桌面态 E2E：重新 `cargo run` 后，前台 `全息观测站` 窗口已加载当前 repo graph；OS 级截图可见 repo 绝对路径节点标签，底部状态显示 `风控0 · 审计1` 与 `简报已更新 · 风险0 · 审计已记录`。

## Drift Lock

以下检查项用于防止后续 agent 偏离当前主线。

## Drift Checklist

- 不把本项目重新漂回“通用 3D 代码星图”。
- 不把客户自带模型 API 改成平台统一供应模型。
- 不把规则、审计、审批、自修复语义藏在 prompt 或 UI 文案。
- 不在缺少 evidence、gate decision、audit event 时宣称风险审查闭环完成。
- 不在缺少 repair plan、测试和审批时宣称自动修复可用。
