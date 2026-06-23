# 第四阶段产品化

生成日期：2026-06-22

## 单句定义

第四阶段不是继续补零散能力，而是把 AI 编码风控平台推进到“规则、审计、工作台、交付”四个面都能持续运营、复现和交接的产品化阶段。

## 四个产品化面

1. 规则系统产品化：统一 review / gate / repair preflight 的规则真源、包边界、版本、来源、启停与优先级。
2. 审计与证据系统产品化：统一 review / gate / repair / rollback / provider failure 的证据模型、检索口径和状态前后对照。
3. 工作台运营化：把 review queue、gate 状态、repair history、provider status、evidence trace 收口成稳定主路径。
4. 稳定交付能力：把 phase4 验证脚本化，让新会话 agent 能按 `dev-docs` 与脚本复现同等级证据。

## 当前完成状态

### 已完成：规则系统产品化

- `src-ui/src/risk/review-core.ts` 已把 `Rule` 升级为带 `package_id`、`priority` 的结构化合同，并新增 `RulePackage`。
- `src-ui/src/risk/rule-package.ts` 已建立统一 registry：默认 review package、默认 repair package、扩展 package 合并、禁用 rule、policy snapshot 生成。
- `src-ui/src/risk/current-review.ts` 与 `src-ui/src/risk/self-heal.ts` 已不再各自维护独立默认规则常量，而是通过同一个 rule policy 入口取用 active rules。
- `workspace.ts`、`phase3-evidence.ts`、mock data、相关测试已切到新的 policy snapshot 口径，避免第四阶段仍混用第三阶段字符串快照。

### 已完成：审计系统产品化

- `src-ui/src/risk/audit-bridge.ts` 已补 `AuditQueryResult`、`AuditRecord`、统一 stage/status/error/state-change 归一，以及 review / approval / repair 的同口径 timeline 记录。
- `workspace.ts` 现会把 `audit_recent_reviews` 统一解析成 normalized audit query 结果再暴露给只读工具，避免 agent 继续消费各形各色的原始 JSON 细节。
- `src-ui/src/ui/check.ts` 已改为消费同一个 `AuditQueryResult.records`，最近审计视图与只读工具不再各自维护不同的解释逻辑。
- repair proposal / approval / apply / rollback 的审计详情已补 finding ids、evidence ids、error stage、retryable 和 state change，方便后续复盘。

### 已完成：稳定交付能力

- `src-ui/package.json` 已新增 `npm run phase4:verify`。
- `src-ui/scripts/phase4-verify.ts` 会顺序执行 `test:risk`、`npx tsc --noEmit`、`npm run build`、`cargo check`，并尝试起本地 preview smoke；所有 git 基线、命令尾部输出和 smoke 结果都会写入 `dev-docs/evidence/phase4-verify.json`。
- 当 preview smoke 因环境限制失败时，`phase4-verify.json.manual_ui_check` 会给出手工 localhost 验收入口、目标 URL 和预期页面标记，避免新会话 agent 只知道失败、不知道如何补证据。
- `dev-docs/phase4-manual-ui-checklist.md` 已把 fallback UI 验收步骤固定成真源，不再要求新会话 agent 从 JSON 字段里自行推导流程。
- 新会话 agent 只需读 `dev-docs/README.md` 和 `phase4-verify.json`，即可确认当前 phase4 已验证到哪一层，而不必先翻历史会话。

### 已完成：工作台运营化

- `CheckPanel` 的最近审计仍保留通用时间线，同时 `自修复闭环` 已开始消费 audit-backed 的 `repair history` 与 `evidence trace`。
- repair history 直接展示 stage / status / state change / retryability，不再只剩一条瞬时状态文案。
- evidence trace 会把当前 finding 数、evidence 数和 repair history 数并列展示，避免用户只能从 scattered panel 猜当前闭环是否真的发生过。
- `current-review.ts` 已新增 `buildWorkbenchQueue`，`CheckPanel` 顶部会直接渲染 `Review Queue`，把看风险 -> 看 gate -> 看证据 -> 审批/阻断 -> repair/rollback 固定成同一条主路径。
- `buildWorkbenchQueue` 已补空状态与降级状态收口：当 review 为 clean 时，证据/审批/repair 步骤统一显示 `not_required`；当 repair issue 可重试时，repair 步骤会直接显示 `degraded`，不再伪装成 `draft`。
- `current_review_summary` 现在不只返回裸 `review`，还会带上 `workbench_queue` 与 `repair_history`，让只读工具和 `CheckPanel` 共用同一条工作台状态真源。
- `current-review.ts` 已新增 `buildRepairWorkbenchSnapshot`，把 provider status、generation input、preflight 摘要、evidence trace、repair history 等从 `CheckPanel` 中下沉到 owner 层，减少 UI 自己推状态。
- `current_review_summary` 的 `limit` 参数合同也已补齐，避免 workspace executor、tool schema 和只读工具文案再次分叉。
- Chrome 真实 localhost 页面已确认可见 `Review Queue`、`门禁决策`、`多代理审计`、`自修复闭环`；说明第四阶段这条工作台主路径不只存在于测试和 mock。

## 已知环境限制

- `phase4:verify` 内的 preview smoke 在当前环境下仍可能因端口权限返回 `EPERM`；这不再阻断第四阶段交付能力，因为 `manual_ui_check` 已作为正式 fallback 真源落盘并验证过。

## 本阶段停止条件

- 规则系统：默认包与扩展包合同成型，current review / gate / repair preflight 共用统一规则真源，并有 fresh 验证。
- 审计系统：统一审计事件合同、检索口径、错误阶段、可重试性和状态变化，并被 UI / 只读工具共用。
- 工作台：主路径清晰，空状态 / 失败状态 / 降级状态 / 历史状态一致。
- 交付能力：至少一套 phase4 脚本化验证入口可由新会话 agent 复现。
