# 验收口径

## Stop Conditions

历史文档接管阶段只验收真源和架构边界；第三、第四阶段收口已完成。当前第五阶段继续在前四阶段内核之上验收外部接入、headless report、CI/hook 与管理员导出。

必须满足：

- `AGENTS.md` 指向 AI 编码风控平台主线。
- `dev-docs/README.md` 索引全部 active docs。
- `product-charter.md`、`current-state-audit.md`、`architecture.md`、`domain-model.md`、`contracts.md` 相互一致。
- Sliver adoption guardrail 检查通过。
- 旧 HoloGram 语义只作为现有基座或历史外部说明出现，不覆盖当前产品边界。
- `dev-docs/evidence/phase3-runtime-samples.json` 必须包含至少 1 组 live provider proposal 成功样本、2 组 high/critical finding 修复后 re-check 归零样本、1 组 preflight 阻断样本与 provider failure 分类矩阵。

## 后续代码阶段验收

按触达面选择最小相关 gate：

- Review Core：合同测试、状态流转测试、finding 必须关联 evidence。
- Rule/Policy：规则命中、gate decision、block/approval/audit 路径测试。
- 第四阶段规则系统：统一 rule registry / package / snapshot 合同、扩展包合并、禁用 rule、优先级裁决，以及 current review / repair preflight 共用规则真源。
- 第四阶段审计系统：统一 `AuditQueryResult` / `AuditRecord`、review/approval/repair 的 stage/status/error/state change，且 UI 与只读工具共用同一查询真源。
- Provider：无明文 key、provider 失败降级、超时和结构化错误测试。
- Audit：append-only、敏感信息过滤、失败可见。
- UI：构建通过，浏览器或桌面预览验证风险列表、逐行解释、审批和审计视图。
- Multi-agent：子代理超时、重复 finding 去重、冲突裁决、主代理汇总测试。
- Self-healing：repair plan、patch proposal、测试 gate、审批、apply、rollback 证据。
- Repair proposal validation：展示前二次审计、快速语法检查、逻辑变更提示、新风险拦截固定文案。
- 第三阶段收口：fresh `npm run test:risk`、`npx tsc --noEmit`、`npm run build`、`cargo check` 全绿；live provider 成功样本与 provider failure 稳定复现样本口径一致。
- 第五阶段交付化：`audit-risk init` 能生成 manifest / rule package / hook / workflow；`audit-risk report` 能对真实 workspace 导出 machine-readable report；`audit-risk rules` / `audit-risk audit` / `audit-risk doctor` 能提供管理员规则检查、审计检索、运行诊断；`audit-risk verify` 能把 `test:risk`、`tsc`、`build`、engine bin test、`cargo check` 与当前 smoke 写回 `dev-docs/evidence/phase5-delivery.json`。
- Live Auth/Payment：真实 auth/payment 服务端可访问后，必须按 `dev-docs/auth-payment-live-verification.md` fresh 验收 `poll -> exchange -> payment query -> refresh/revoked -> observe gate`，并采集 29 元/月订单、支付渠道回调、`next_billing_at`、取消/撤销样本；当前仓库内没有服务端或支付平台实现时，该 gate 必须保持 pending，不能用 `mock://...` 证据替代。
- 第四个 CLI 体验面（`init` / `doctor` / `watch` 的首次体验）完成时，除功能验证外，必须额外做一次实现审计：确认命令能用、没有明显无效 bug、没有无用代码、没有垃圾防御逻辑，并把发现的问题或确认结果写回真源。

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
- 2026-06-21：Repair preflight RED：新增 `rule-package.test.ts` 与 `self-heal` preflight 用例后，`npm run test:risk` 首次失败于缺少 `rule-package` owner，确认第三阶段仍遵守 test-first。
- 2026-06-21：Repair preflight GREEN：新增 `src-ui/src/risk/rule-package.ts`、`ValidationCommandResult`、`RepairPreflightReport`、`runRepairPreflight` 后，`npm run test:risk` 通过 32 条风险核心/修复测试。
- 2026-06-21：第三阶段类型门：`npx tsc --noEmit` 通过。
- 2026-06-21：第三阶段前端构建：`npm run build` 通过；仍有既有 Vite dynamic/static import 与 chunk size warning，无新增 build blocker。
- 2026-06-21：Repair degrade RED：新增 `createRepairIssue` 与 `attachRepairIssueToCurrentReview` 用例后，`npm run test:risk` 首次失败于缺少 `RepairIssue` / `current-review` 接口导出，确认仍按 test-first 推进。
- 2026-06-21：Repair degrade GREEN：新增 `RepairIssue`、`RepairExecutionStage`、`createRepairIssue`、`attachRepairIssueToCurrentReview` 后，`npm run test:risk` 通过 35 条风险核心/修复测试。
- 2026-06-21：Repair degrade fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Patch proposal path guard RED：新增“拒绝写出 provided repair file set”用例后，`npm run test:risk` 先失败于缺少 path guard，再失败于 `normalizePath` 未定义；均为当前 slice 的真实缺口。
- 2026-06-21：Patch proposal path guard GREEN：补齐 generation-time 文件集校验与本地 path normalize helper 后，`npm run test:risk` 通过 36 条风险核心/修复测试。
- 2026-06-21：Patch proposal path guard fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Provider error precision RED：新增 auth invalid 与 HTTP 429 的 repair issue 用例后，`npm run test:risk` 首次失败于仍返回泛化 `provider_unavailable`，说明错误分类粒度不足。
- 2026-06-21：Provider error precision GREEN：扩展 `ContractError.code` 与 `normalizeRepairError` 后，`npm run test:risk` 通过 38 条风险核心/修复测试。
- 2026-06-21：Provider error precision fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Patch proposal content guard RED：新增“拒绝 no-op rewrite / 忽略 critical 文件”的用例后，`npm run test:risk` 首次失败，确认当前 generation-time 校验仍不足。
- 2026-06-21：Patch proposal content guard GREEN：补齐 no-op rewrite 校验与 high-severity focus file 校验后，`npm run test:risk` 通过 40 条风险核心/修复测试。
- 2026-06-21：Patch proposal content guard fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Patch proposal full high-severity coverage RED：新增“只覆盖部分 high/critical 文件”用例后，`npm run test:risk` 首次失败，确认当前约束仍停留在 `some()` 级别。
- 2026-06-21：Patch proposal full high-severity coverage GREEN：把高风险文件覆盖约束收紧为 `every()` 后，`npm run test:risk` 通过 41 条风险核心/修复测试。
- 2026-06-21：Patch proposal full high-severity coverage fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Patch proposal line-range RED：新增“改了高风险文件但没触达高风险行范围”的用例后，`npm run test:risk` 首次失败，确认文件级覆盖仍不足以证明修复语义。
- 2026-06-21：Patch proposal line-range GREEN：补齐 changed-line range 计算与高风险行范围重叠校验后，`npm run test:risk` 通过 42 条风险核心/修复测试。
- 2026-06-21：Patch proposal line-range fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Patch proposal semantic-line RED：新增“只在高风险行做 whitespace-only 改动”的用例后，`npm run test:risk` 首次失败，确认当前 changed-line 集合还会把空白改动误判为已修复。
- 2026-06-21：Patch proposal semantic-line GREEN：补齐 whitespace-insensitive changed-line 集合后，`npm run test:risk` 通过 43 条风险核心/修复测试。
- 2026-06-21：Patch proposal semantic-line fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：ReviewFinding contract RED：新增非法行号范围与规则编号式解释用例后，`npm run test:risk` 首次失败，确认合同校验仍过宽。
- 2026-06-21：ReviewFinding contract GREEN：补齐正序行号与白话解释质量校验后，`npm run test:risk` 通过 49 条风险核心/修复测试。
- 2026-06-21：ReviewFinding contract fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：ReviewFinding narrative quality RED：新增 `impact` / `recommendation` 退化成短 token 的用例后，`npm run test:risk` 首次失败，确认说明文字质量门还不完整。
- 2026-06-21：ReviewFinding narrative quality GREEN：补齐 `impact` / `recommendation` 的最小白话质量校验后，`npm run test:risk` 通过 52 条风险核心/修复测试。
- 2026-06-21：ReviewFinding narrative quality fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：GateDecision contract RED：新增缺少 subject/policy/reason/finding 的 block decision 用例后，`npm run test:risk` 首次失败于缺少 `validateGateDecision` 导出。
- 2026-06-21：GateDecision contract GREEN：补齐 `validateGateDecision` 后，`npm run test:risk` 通过 50 条风险核心/修复测试。
- 2026-06-21：GateDecision contract fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Current review gate RED：新增 `current-review` 的 `block` / `require_approval` 用例与默认 review 规则包断言后，`npm run test:risk` 首次失败于缺少 `DEFAULT_REVIEW_RULES` 与 `gate_decision` 派生。
- 2026-06-21：Current review gate GREEN：补齐默认 review 规则包、`CurrentReviewState.gate_decision` 与 `CheckPanel` 门禁决策展示后，`npm run test:risk` 通过 54 条风险核心/修复测试。
- 2026-06-21：Current review gate fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Current review summary response GREEN：`buildCurrentReviewSummaryResponse` 已补 `empty/ok` 响应合同，并确保 `gate_decision` 与 `repair_generation_meta` 一起暴露给只读工具。
- 2026-06-21：Review audit gate-consistency RED：新增 `warn` 决策仍应记为 `allowed` 的用例后，`npm run test:risk` 首次失败于 audit action 仍被 `passed` 绑死。
- 2026-06-21：Review audit gate-consistency GREEN：`buildReviewAuditPayload` 现按结构化 `gate_decision` 写 `allowed/denied`，`npm run test:risk` 通过 55 条风险核心/修复测试。
- 2026-06-21：Repair generation metadata GREEN：`RepairGenerationMetadata` 已进入 current review state、repair audit 与面板展示，`npm run test:risk` 通过 56 条风险核心/修复测试。
- 2026-06-21：Repair preflight summary GREEN：`buildRepairPreflightSummary` 与 preflight issue 状态收口已完成，失败命令/阻断 rule 可直接进入当前 review 读模型与 UI。
- 2026-06-21：Repair preflight issue RED：新增结构化 `RepairApplyError` / preflight issue 用例后，`npm run test:risk` 首次失败于缺少相关导出与当前 review helper。
- 2026-06-21：Repair preflight issue GREEN：补齐 `RepairApplyError`、`buildRepairIssueFromPreflight`、`attachRepairPreflightIssueToCurrentReview` 以及失败 audit 细节后，`npm run test:risk` 通过 56 条风险核心/修复测试。
- 2026-06-21：Repair preflight issue fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：ProviderRequestError GREEN：provider 层已补结构化错误对象和分类函数，`provider-error.test.ts` 已覆盖 401/429/503/ENOTFOUND/证书吊销/代理拒绝等路径。
- 2026-06-21：Repair execution rollback GREEN：`RepairApplyExecutionError` 已覆盖部分写入失败自动回滚场景，`npm run test:risk` 通过 57 条风险核心/修复测试。
- 2026-06-21：Provider upstream/network RED：新增 upstream 503 与 DNS/ENOTFOUND 用例后，`npm run test:risk` 首次失败于仍落到 `internal_error` 或泛化错误，确认 5xx/网络层分类不足。
- 2026-06-21：Provider upstream/network GREEN：扩展 `ContractError.code` 与 `normalizeRepairError` 后，`npm run test:risk` 通过 45 条风险核心/修复测试。
- 2026-06-21：Provider upstream/network fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Provider TLS/proxy RED：新增证书失败与代理拒绝用例后，`npm run test:risk` 首次失败于仍落到泛化错误，确认 TLS/代理层合同未单独建模。
- 2026-06-21：Provider TLS/proxy GREEN：扩展 `ContractError.code` 与 `normalizeRepairError` 后，`npm run test:risk` 通过 47 条风险核心/修复测试。
- 2026-06-21：Provider TLS/proxy fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Provider cert-revoked/interrupted RED：新增证书吊销与 socket hang up 用例后，`npm run test:risk` 首次失败于仍落到泛化 TLS/网络错误，确认语义粒度不足。
- 2026-06-21：Provider cert-revoked/interrupted GREEN：扩展 `ContractError.code` 与 `normalizeRepairError` 后，`npm run test:risk` 通过 52 条风险核心/修复测试。
- 2026-06-21：Provider cert-revoked/interrupted fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning。
- 2026-06-21：Provider 407/reset/source-context RED：新增 `HTTP 407`、`ECONNRESET`、`No readable source files` 用例，以及 repair audit 必须保留 `error_stage/error_retryable` 的断言后，`npm run test:risk` 首次失败于 `407` 仍被归到 `provider_unavailable`。
- 2026-06-21：Provider 407/reset/source-context GREEN：补齐 `classifyProviderFailure` 的 407 与 `ECONNRESET` 归类、`normalizeRepairError` 的 source-context / 407 / reset 归一，以及 degraded repair audit 的 `error_stage/error_retryable` 字段后，`npm run test:risk` 通过 89 条风险核心/修复测试。
- 2026-06-21：Provider 407/reset/source-context fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning，无新增 build blocker。
- 2026-06-21：Patch proposal narrative RED：新增 `summary` / `rationale` / `operation.summary` 不能退化成 `fix` / `todo` / `update` 的用例后，`npm run test:risk` 首次失败，确认当前 proposal 解释层还会接受占位级伪语义文本。
- 2026-06-21：Patch proposal narrative GREEN：补齐 `parsePatchProposal` 的最小可读解释校验后，`npm run test:risk` 通过 94 条风险核心/修复测试。
- 2026-06-21：Patch proposal narrative fresh gate：再次执行 `npx tsc --noEmit` 与 `npm run build` 通过；仍仅存在既有 Vite 动态导入与大 chunk warning，无新增 build blocker。
- 2026-06-21：macOS credential restore RED：只读核对 `src-tauri/src/credential.rs` 与当前机器环境后确认，provider secret restore 仅实现 Windows DPAPI，macOS 路径会直接返回 `unsupported platform`，导致桌面壳无法稳定恢复 live provider key。
- 2026-06-21：macOS credential restore GREEN：补齐 macOS Keychain 存取路径与 provider manifest，新增 fake `security` CLI 单测后，`cargo test credential -- --nocapture` 通过 2 条凭证相关测试。
- 2026-06-21：macOS credential restore fresh gate：`cargo check` 通过；当前仅剩既有 warning，无新增 Rust 编译 blocker。
- 2026-06-21：provider readiness GREEN：新增 `active_provider_readiness` / `credential_has` 只读链路与 `provider-readiness.test.ts` 后，前端可结构化返回当前 active provider 的 readiness/source/reason，不再只能靠 live proposal 抛错判断。
- 2026-06-21：provider readiness UI/current-review GREEN：`provider_readiness` 已进入 `CurrentReviewState`、`current_review_summary` 与 `CheckPanel` 的 `自修复闭环` 区块；再次执行 `npm run test:risk`、`npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-21：live repair readiness GREEN：新增 mock-browser / tauri-workspace 区分后，系统能明确提示“聊天助手可用但当前仍是 mock workspace，因此不具备真实 live repair 证据资格”；再次执行 `npm run test:risk`、`npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-22：repair generation readiness GREEN：新增 `deriveRepairFilePaths` 与 repair input readiness 读模型后，系统会在运行时明确区分“无可修复风险”“有风险但无可编辑源码输入”和“可生成 proposal”三种状态；再次执行 `npm run test:risk`、`npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-22：repair generation fallback GREEN：当结构性 finding 自身缺少稳定 file path 时，repair planner 现会回退使用 `changed_files` 作为候选输入；再次执行 `npm run test:risk`、`npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-22：changed-files fallback GREEN：`hologram_run_check` 在 watcher 未带到变更文件时会回退读取 git working tree 变更；相关 Rust 单测与 `cargo check` 通过。
- 2026-06-22：repair empty-state GREEN：真实运行态下，`自修复闭环` 已明确显示“当前无可修复风险”与 `Repair input: 0 findings · 0 files · blocked`，不再把空输入误导成 provider/source-file 故障。
- 2026-06-22：stage completion cleanup GREEN：移除仅用于 live 验证的临时 migration 输入；当前工作台在真实 repo 上会把“无可修复风险 / 不可自动修复”前置收口，不再误导成 provider 或 source-file 故障。
- 2026-06-22：Repair non-code gate RED：新增 config critical repair 用例后，`node --import tsx src/risk/test-risk.ts` 首次失败于 `required_tests` 仍为空，确认非代码高风险 repair 无法形成通过样本。
- 2026-06-22：Repair non-code gate GREEN：`self-heal.ts` 补齐 config / migration / serialization 的 `git diff --check` gate 后，`node --import tsx src/risk/test-risk.ts` 通过。
- 2026-06-22：第三阶段 fresh gate：再次执行 `npm run test:risk`、`npx tsc --noEmit`、`npm run build`、`cargo check` 通过；仍仅剩既有 Vite chunk warning 与 Rust warning，无新增 blocker。
- 2026-06-22：phase3 runtime evidence GREEN：`npm run phase3:evidence` 生成 `dev-docs/evidence/phase3-runtime-samples.json`，包含 live provider、semantic repair、preflight 与 provider failure 样本。
- 2026-06-22：live provider success GREEN：macOS Keychain fresh audit 显示 `deepseek=present`、`anthropic=missing`；基于 secure-store 恢复的 `deepseek / deepseek-v4-pro` 已真实生成 `config.yaml` repair proposal，并进入 current review / CheckPanel / repair audit，随后 apply 成功。
- 2026-06-22：semantic repair proof GREEN：以 `engine.run_full_check` 真实产出的 `config.yaml` 与 `migrations/0001_init.sql` 两组 L5 finding 为输入，proposal apply 后 git diff 归零，re-check 从 `1 finding` 下降到 `0`，`one_line` 均返回 `无新变更`。
- 2026-06-22：preflight allow/block GREEN：allow 样本中 `git diff --check` 通过且 `repair_apply` gate 为 `allow`；block 样本中 `repair.test.required_command_failed`、失败命令 `git diff --check` 与 gate reason 已在 current review / CheckPanel / repair audit 三处保持一致。
- 2026-06-22：provider failure matrix GREEN：`auth_invalid`、`rate_limit`、`timeout`、`upstream_5xx`、`proxy_407`、`tls_handshake_failed`、`tls_cert_revoked`、`connection_reset`、`socket_hang_up` 均已有稳定复现样本，并在 current review / CheckPanel / repair audit 三处保持同码值口径。
- 2026-06-22：phase4 rule registry RED：新增 `RulePackage` / policy snapshot / disabled rule / priority 裁决测试后，`node --import tsx src/risk/test-risk.ts` 首次失败于缺少 `buildRulePolicySnapshotId` 与统一 package 入口，确认第四阶段仍按 test-first 推进。
- 2026-06-22：phase4 rule registry GREEN：补齐 `Rule.package_id`、`Rule.priority`、`RulePackage`、统一 rule registry、扩展包合并、禁用 rule、policy snapshot 生成，以及 `current-review.ts` / `self-heal.ts` 的统一 policy 消费后，`node --import tsx src/risk/test-risk.ts` 通过。
- 2026-06-22：phase4 rule registry fresh gate：再次执行 `npx tsc --noEmit`、`npm run build` 通过；仍仅存在既有 Vite dynamic/static import 与 chunk size warning，无新增 build blocker。
- 2026-06-22：phase4 audit query RED：新增 `AuditQueryResult` / `AuditRecord` / unified timeline 测试后，`node --import tsx src/risk/test-risk.ts` 首次失败于缺少 `buildAuditQueryResult`，确认最近审计仍缺统一查询层。
- 2026-06-22：phase4 audit query GREEN：补齐 `AuditQueryResult`、`AuditRecord`、repair 审计 state/evidence 补充、workspace tool 拦截归一与 `CheckPanel` 共用 query parser 后，`node --import tsx src/risk/test-risk.ts` 通过。
- 2026-06-22：phase4 audit query fresh gate：再次执行 `npx tsc --noEmit`、`npm run build` 通过；仍仅存在既有 Vite dynamic/static import 与 chunk size warning，无新增 build blocker。
- 2026-06-22：phase4 workbench history GREEN：`CheckPanel` 已新增 audit-backed 的 `repair history` 与 `evidence trace`；再次执行 `node --import tsx src/risk/test-risk.ts`、`npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-22：phase4 review queue GREEN：`current-review.ts` 已新增 `buildWorkbenchQueue`，`CheckPanel` 顶部已渲染 contract-backed `Review Queue` 主路径；再次执行 `node --import tsx src/risk/test-risk.ts`、`npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-22：phase4 review queue empty/degraded GREEN：补齐 clean review 时 `not_required` 空状态、以及 retryable repair issue 时的 `degraded` 状态后，再次执行 `node --import tsx src/risk/test-risk.ts`、`npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-22：phase4 workbench summary GREEN：`current_review_summary` 已开始直接返回 `workbench_queue` 与 `repair_history`，workspace agent tool 与 `CheckPanel` 现在共用同一条工作台主路径真源；再次执行 `node --import tsx src/risk/test-risk.ts`、`npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-22：phase4 repair snapshot GREEN：`current-review.ts` 已新增 `buildRepairWorkbenchSnapshot`，`CheckPanel` 的 provider/evidence/preflight/history 读路径已改为消费 snapshot；再次执行 `node --import tsx src/risk/test-risk.ts`、`npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-22：phase4 summary-tool parity GREEN：`current_review_summary` 已补 `repair_workbench` 与 `limit` 参数合同，mock/runtime/tool schema 不再分叉；再次执行 `npx tsc --noEmit`、`npm run build` 通过。
- 2026-06-22：phase4 verify entry GREEN：新增 `npm run phase4:verify` 与 `src-ui/scripts/phase4-verify.ts`，并成功落盘 `dev-docs/evidence/phase4-verify.json`；产物包含 `main` 分支、最近提交 `c42326e`、当前 dirty tree、`test:risk`、`tsc`、`build` 与 `cargo check` 的 fresh 证据尾部输出。
- 2026-06-22：phase4 preview smoke PARTIAL：`phase4:verify` 已尝试起本地 preview 并抓取 `127.0.0.1`，但当前环境里 preview 子进程绑定 `127.0.0.1:4174` 返回 `EPERM`；该失败已写入 `phase4-verify.json.preview_smoke`，目前只能作为“已尝试但受环境限制”的证据，不能算页面级 UI 验收通过。
- 2026-06-23：phase4 localhost UI GREEN：通过本机 Chrome 打开 `http://127.0.0.1:4173/`，实机可见 `Review Queue`、`门禁决策`、`多代理审计`、`自修复闭环` 区块；继续下滚后可见 `看证据 · 已就绪 4 条 finding · 4 个 evidence` 与 repair/apply 历史文案。页面标题刷新为 `🔮 风控4 审计3 — 全息观测站`。
- 2026-06-23：phase4 manual-ui checklist GREEN：已新增 `dev-docs/phase4-manual-ui-checklist.md`，把 `npm run dev -- --host 127.0.0.1 --port 4173`、localhost 地址与页面级标记固化成 fallback 验收步骤，避免新会话 agent 只凭 `phase4-verify.json` 自行猜流程。
- 2026-06-23：phase5 init GREEN：交付化 init 已真实生成 `.hologram/delivery.json`、workspace rule package stub、`.githooks/pre-commit` 与 `.github/workflows/hologram-risk.yml`；当前公共命令面已冻结为 `audit-risk init`。
- 2026-06-23：phase5 report GREEN：交付化 report 已对真实 repo 导出 machine-readable delivery report；当前公共命令面已冻结为 `audit-risk report`。
- 2026-06-23：phase5 external workspace GREEN：external workspace smoke 已证明初始化与 report 路径可走通；当前公共命令面已冻结为 `audit-risk init/report`。
- 2026-06-23：phase5 hook GREEN：外部临时 git repo 上的 `.githooks/pre-commit` 已真实执行通过；当前生成模板已切到 `audit-risk report`。
- 2026-06-23：phase5 admin path GREEN：外部临时 git repo 已真实跑通管理员规则检查、审计检索与运行诊断；当前公共命令面已冻结为 `audit-risk rules/audit/doctor`。
- 2026-06-26：task2 CLI freeze GREEN：已新增 `audit-risk` Rust 主入口、统一 `audit-risk.cli.v1` envelope、primary/secondary 命令分层，以及 `check` / `doctor` / `report` / `watch --jsonl` / `init` / `diff` / `rules` / `audit` 的 fresh 运行证据；`verify` 已收口到 Rust 公共入口，但未在本轮做 fresh destructive smoke。
- 2026-06-26：task2 verify GREEN：`cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- verify repo` 已 fresh 通过，并把 external smoke 命令记录为 `audit-risk init/report/rules/audit/doctor`，不再残留旧 `phase5-delivery.ts` 公共命令链。
- 2026-06-26：task4 doctor/init/watch GREEN：`audit-risk init` 已在临时 workspace 真实生成 5 个接入文件；`audit-risk doctor` 已在新初始化 workspace 上返回 engine 版本、`git/cargo/node` 依赖、review/repair rule package 版本与 provider/audit 路径状态；`audit-risk watch` 已在人类模式与 `--jsonl` 模式下真实输出首轮扫描结果。
- 2026-06-26：task4 code audit GREEN：这轮专项审计已发现并修掉两条真实问题：subdir workspace 会把父仓库 `../` 变更误带进 `check/watch`，以及 pre-commit 模板会生成错误的平台根路径/二进制路径；同时移除了 Rust CLI 中已确认无用的死代码 helper，没有额外引入垃圾防御分支。
- 2026-06-26：task5 watch output GREEN：`watch` 默认仅展示 `critical/high/medium`，`--verbose` 才显示 `low`；同一文件同一规则 10 分钟内只输出一次，重复命中会进入 `finding_suppressed` / `suppressed=n` 路径，相关 Rust 单测与人类模式 smoke 已通过。
- 2026-06-26：task6 observe/notify GREEN：`watch --observe` 已在真实主机环境下打印本地观察页地址、LAN 地址与二维码图片路径；`notify --test` 已对临时本地 webhook 真实发出测试请求并收到 HTTP 200，返回结构化 `tested_url/http_status/ok` 结果。
- 2026-06-27：task7 CLI commercial shell GREEN：新增 Rust CLI 测试后，`cargo test --manifest-path engine/Cargo.toml cli::tests:: -- --nocapture` 通过 16 条用例，覆盖零参数中文首页、`help/tour`、`auth login/logout/status`、`observe` 解析，以及 `observe/notify` 的中文 Pro gate。随后 fresh 执行 `cargo run --manifest-path engine/Cargo.toml --bin audit-risk --` 与 `cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- auth status`，确认零参数不再直接报错、首页会展示 Core/Pro 状态和新手引导，未登录状态会返回中文 `Core 免费版` 提示。当前仍未做服务端登录/支付链路验收，原因是本仓库只实现了 CLI 侧合同与本地状态机，未接入真实 `/api/auth/*` 与 `/api/payment/*` 服务。
- 2026-06-27：task8 auth local contract GREEN：新增 auth 本地合同测试后，`cargo test --manifest-path engine/Cargo.toml cli::tests::auth_ -- --nocapture` 通过 2 条用例，覆盖 `auth login` 生成 `device_secret/session.json` 与 `auth status` 的 `grace/expired/revoked/device_mismatch` 文案分支。随后 fresh 执行 `AUDIT_RISK_ENTITLEMENT_DIR=/private/tmp/audit-risk-auth-smoke cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- auth login` 与 `... auth status`，确认 CLI 会真实落本地 `session_id`、`session.json`、`device_secret`，但在服务端未接入前仍保持 `pending/Core 免费版`，不会伪造 Pro 成功态。
- 2026-06-27：task9 auth pending-state GREEN：新增 `auth_status_shows_pending_login_when_session_exists_but_entitlement_is_missing` 测试后，`cargo test --manifest-path engine/Cargo.toml cli::tests::auth_ -- --nocapture` 通过 3 条用例。随后顺序 fresh 执行 `AUDIT_RISK_ENTITLEMENT_DIR=/private/tmp/audit-risk-auth-smoke-3 cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- auth login`、`... auth status`、`... auth logout`，确认 CLI 会真实生成 `poll_url / exchange_url / expires_at`，`auth status` 会显示“登录进行中”，`auth logout` 会清理本地会话与授权缓存。浏览器自动拉起在当前主机上尝试过，但 `open` 返回 `kLSExecutableIncorrectFormat`，因此本轮只记录为“已尝试，需手动打开登录地址”，不能算浏览器拉起验收通过。
- 2026-06-27：task10 auth success-chain GREEN：新增 `auth_login_exchanges_entitlement_when_auth_server_is_configured` 与 `entitlement_status_detects_device_mismatch_when_device_id_does_not_match` 后，`cargo test --manifest-path engine/Cargo.toml cli::tests::auth_ -- --nocapture` 通过 4 条用例，覆盖 `poll -> exchange -> entitlement` 成功链路和 `device_id` 不匹配分支。随后 fresh 执行 `AUDIT_RISK_ENTITLEMENT_DIR=/private/tmp/audit-risk-auth-approved AUDIT_RISK_AUTH_BASE_URL=mock://approved cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- auth login` 与 `... auth status`，确认 CLI 能真实落 `entitlement.json / entitlement.sig`，并把 `auth status` 切到 `已登录 / Pro 个人版`。继续执行 `AUDIT_RISK_ENTITLEMENT_DIR=/private/tmp/audit-risk-auth-approved cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- observe /private/tmp/audit-risk-auth-approved`，确认授权 gate 已放行，命令继续进入 observe 运行时；当前失败点是端口绑定 `0.0.0.0:8787` 被环境拒绝，而不是授权链路失败。
- 2026-06-27：task11 entitlement refresh/revoked GREEN：新增 `refresh_entitlement_updates_stale_active_entitlement` 与 `refresh_entitlement_surfaces_revoked_state` 后，`cargo test --manifest-path engine/Cargo.toml cli::tests::refresh_entitlement_ -- --nocapture` 通过 2 条用例，覆盖 stale active entitlement 自动 refresh 和 refresh 后转 `revoked` 的状态迁移。随后 fresh 执行两组顺序命令：1) 基于 `/private/tmp/audit-risk-auth-approved` 的真实成功授权样本，把 `last_refresh_time` 回拨后运行 `AUDIT_RISK_ENTITLEMENT_DIR=/private/tmp/audit-risk-auth-approved AUDIT_RISK_AUTH_BASE_URL=mock://refresh-active cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- auth status`，确认状态仍为 `已登录`，且本地 `entitlement.json` 已刷新出 `notify` feature，`entitlement.sig` 更新为 `refreshed-signature`；2) 在同一真实授权样本上切到 `AUDIT_RISK_AUTH_BASE_URL=mock://refresh-revoked` 再运行 `auth status`，确认状态切为 `授权已撤销`。这说明 CLI 侧 refresh/revoked 链路已收口；当前仍未接入 `GET /api/payment/query` 的支付确认兜底。 
- 2026-06-27：task12 payment query fallback GREEN：新增 `auth_login_queries_payment_until_plan_becomes_pro` 与 `auth_login_returns_waiting_message_when_payment_query_does_not_confirm_in_time` 后，`cargo test --manifest-path engine/Cargo.toml payment_ -- --nocapture` 通过 2 条用例，覆盖 exchange 返回非 Pro 时触发 `GET /api/payment/query` 的升级路径，以及查询超时后的中文等待提示。随后 fresh 执行两组顺序命令：1) `AUDIT_RISK_ENTITLEMENT_DIR=/private/tmp/audit-risk-payment-pending AUDIT_RISK_AUTH_BASE_URL=mock://payment-pending cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- auth login`，确认 CLI 直接落成 `pro_personal_monthly`，随后 `auth status` 返回 `已登录 / Pro 个人版`，本地 `entitlement.json` 带 `observe/notify` feature，`entitlement.sig` 为 `paid-signature`；2) `AUDIT_RISK_ENTITLEMENT_DIR=/private/tmp/audit-risk-payment-timeout AUDIT_RISK_AUTH_BASE_URL=mock://payment-timeout cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- auth login` 返回中文 `支付确认中，请稍后运行 audit-risk auth status 查看状态`，随后 `auth status` 会固定显示 `登录状态：支付确认中`，而不是退回泛化未登录。 
- 2026-06-27：task13 auth diagnostics GREEN：执行 `cargo test --manifest-path engine/Cargo.toml auth_status_shows_payment_pending_when_cached_entitlement_is_waiting_for_payment_confirmation -- --nocapture` 后，`payment_pending` 状态文案保持通过。随后 fresh 执行 `AUDIT_RISK_ENTITLEMENT_DIR=/private/tmp/audit-risk-payment-timeout AUDIT_RISK_AUTH_BASE_URL=mock://payment-timeout cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- doctor /private/tmp/audit-risk-payment-timeout`，确认 `doctor` 新增 `auth_service=mock://payment-timeout` 与 `entitlement_cache` 检查项，后者会显式展示 `plan=core_free`、`payment_pending=true` 与缓存目录路径。
- 2026-06-27：task14 auth transport diagnostics GREEN：执行 `cargo test --manifest-path engine/Cargo.toml auth_http_json_classifies_network_unreachable -- --nocapture`、`cargo test --manifest-path engine/Cargo.toml auth_http_json_classifies_bad_json -- --nocapture` 与 `cargo test --manifest-path engine/Cargo.toml doctor_surfaces_auth_service_error_code_when_configured_service_is_unreachable -- --nocapture`，确认 auth transport 已能把服务错误收口成 `network_unreachable / bad_json / timeout / auth_service_error` 级别的结构化码。随后 fresh 执行 `AUDIT_RISK_AUTH_BASE_URL=mock://network-unreachable cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- doctor /private/tmp/audit-risk-payment-timeout`，确认 `doctor.checks.auth_service` 现在会返回 `status=error`，并在 `detail` 里同时带 `base_url/code/message`。
- 2026-06-27：task15 auth config source GREEN：执行 `cargo test --manifest-path engine/Cargo.toml auth_base_url_prefers_delivery_config_when_env_is_missing -- --nocapture` 与 `cargo test --manifest-path engine/Cargo.toml doctor_prefers_delivery_config_auth_service_base_url -- --nocapture`，确认 auth 服务地址现在会优先读取 workspace `delivery.json.auth.base_url`。随后 fresh 执行 `env -u AUDIT_RISK_AUTH_BASE_URL cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- doctor /private/tmp/audit-risk-doctor-delivery-auth`，其中该 workspace 的 `.hologram/delivery.json` 仅配置 `auth.base_url=mock://network-unreachable`，确认 `doctor.checks.auth_service.detail.base_url` 真实显示为 `mock://network-unreachable`，说明 `doctor` 已不再依赖环境变量独占配置。
- 2026-06-27：task16 delivery auth config GREEN：执行 `cargo test --manifest-path engine/Cargo.toml init_files_include_observe_defaults_in_delivery_config -- --nocapture`，确认 `audit-risk init` 生成的 `.hologram/delivery.json` 现在已包含 `auth.base_url` 空字段，后续 workspace 可以直接把 auth 服务地址写进交付真源，而不是额外依赖环境变量约定。
- 2026-06-27：task17 delivery auth schema parity GREEN：执行 `cd src-ui && node --import tsx src/risk/delivery.test.ts`，确认 TypeScript delivery 合同、默认配置和 init 产物也已包含 `auth.base_url`，Rust CLI 模板与 TS `DeliveryConfig/createDefaultDeliveryConfig/buildDeliveryInitFiles` 对 `delivery.json` 的 auth 字段已保持同一口径。
- 2026-06-27：task18 auth typed contract GREEN：执行 `cargo test --manifest-path engine/Cargo.toml auth_ -- --nocapture`、`cargo test --manifest-path engine/Cargo.toml payment_ -- --nocapture`、`cargo test --manifest-path engine/Cargo.toml refresh_entitlement_ -- --nocapture` 与 `cd src-ui && node --import tsx src/risk/delivery.test.ts`，确认在把 auth/poll/exchange/refresh/payment query 的关键 JSON 形状沉成 Rust 结构化合同类型后，现有 CLI 授权、支付兜底、refresh/revoked 与 TS delivery 合同都未被打坏。
- 2026-06-27：task19 live-readiness cleanup GREEN：新增 Rust CLI 单测后，先确认 `auth status` stale entitlement refresh 不能从当前 workspace `delivery.json.auth.base_url` 取服务地址；随后修正为 `auth status` 显式使用当前 workspace 配置，并补齐未知远端 entitlement status 进入 `invalid`、过期 pending session 不再显示“登录进行中”的状态收敛。执行 `cargo test --manifest-path engine/Cargo.toml cli::tests:: -- --nocapture` 通过 35 条 CLI 用例；`docs/auth-payment-live-samples.json` 通过 JSON 解析。文档侧同步收口样例为合同样例而非真实采样，补 `payment_pending=true`，并把真实 auth/payment 与 29 元/月支付平台样本列为独立 pending gate。
- 2026-06-27：task20 auth session URL source GREEN：新增 `auth_login_session_urls_follow_configured_auth_base_url` 后，先确认配置了 auth 服务地址时，`session.json` 仍错误写入默认 `auth.audit-risk.local`；随后修正为 `poll_url / exchange_url / login_url` 跟随 `delivery.json.auth.base_url` 或环境变量生成。执行 `cargo test --manifest-path engine/Cargo.toml auth_login_session_urls_follow_configured_auth_base_url -- --nocapture` 通过。
- 2026-06-27：task21 live verification script GREEN：新增 `scripts/auth-payment-live-verification.sh`，把 `summary / cli_login / cli_status / observe_gate / poll / exchange / payment_query / refresh / evidence_template` 收口成可执行 Bash 模板，并在 `dev-docs/auth-payment-live-verification.md` 增加对应入口。执行 `bash -n scripts/auth-payment-live-verification.sh` 与 `./scripts/auth-payment-live-verification.sh` 通过；当前脚本只输出步骤和调用真实 CLI / curl，不会伪造任何远端结果。
- 2026-06-27：task22 live verification script executable GREEN：把 `scripts/auth-payment-live-verification.sh` 权限收紧为可直接执行入口，当前文件模式为 `755`。执行 `./scripts/auth-payment-live-verification.sh` 通过，默认输出 live 验收摘要，不需要额外包一层 `bash`。
- 2026-06-27：task23 live verification script autofill GREEN：新增 `tests/e2e/test_auth_payment_live_verification.sh`，先确认 `poll` 仍要求手工传 `SESSION_ID`；随后把 `scripts/auth-payment-live-verification.sh` 收口为可从 `session.json` 自动读取 `session_id`，并从 `entitlement.json` 自动读取 `user_id/device_id`，显式环境变量仍优先。执行 `bash tests/e2e/test_auth_payment_live_verification.sh` 通过。
- 2026-06-27：task24 live verification script config source GREEN：继续收口脚本配置来源，`scripts/auth-payment-live-verification.sh` 现在会像 Rust CLI 一样优先读取 `WORKSPACE_ROOT/.hologram/delivery.json.auth.base_url`，再回退到 `AUTH_BASE_URL` 环境变量。扩展 `tests/e2e/test_auth_payment_live_verification.sh` 后再次执行通过，确认脚本不再要求每一步都手工补 auth 服务地址。
- 2026-06-27：task25 live sample pending semantics GREEN：修正 `docs/auth-payment-live-samples.json` 的语义错误，成功的 `auth_exchange_pro` 样例不再带 `payment_pending=true`，真正的 `payment_query_pending` 样例则显式补齐 `payment_pending=true`。新增 `tests/test_auth_payment_live_samples.py` 后，执行 `python3 -m unittest tests.test_auth_payment_live_samples` 与 JSON 解析检查通过。
- 2026-06-27：task26 live evidence template GREEN：`scripts/auth-payment-live-verification.sh` 新增 `VERIFY_STEP=evidence_template`，可直接输出真实联调证据骨架，并自动带入当前发现到的 `base_url / session_id / user_id / device_id`。扩展 `tests/e2e/test_auth_payment_live_verification.sh` 后再次执行通过，确认后续不必再手工拼 live evidence JSON 结构。
- 2026-06-27：task27 live script usage drift cleanup GREEN：把 `dev-docs/auth-payment-live-verification.md` 与脚本摘要里的旧示例统一切到当前真实用法：直接执行 `./scripts/auth-payment-live-verification.sh`，并优先依赖 `delivery.json.auth.base_url` 与本地缓存自动发现，而不是反复手填 `AUTH_BASE_URL / USER_ID / DEVICE_ID`。执行脚本语法检查、`./scripts/auth-payment-live-verification.sh`、脚本级 e2e 与 `rg` 漂移搜索后，确认旧写法已清掉。
- 2026-06-27：task28 live evidence JSON assertion GREEN：把 `tests/e2e/test_auth_payment_live_verification.sh` 里对 `evidence_template` 的校验从字符串包含收紧为 `python3` 解析 JSON 后逐字段断言 `meta.base_url`、`cli.login.session_id`、`http.poll.session_id`、`http.payment_query.*` 与 `http.refresh.*`。再次执行脚本级 e2e 通过，说明 evidence 模板现在不仅“看起来像 JSON”，而且字段结构稳定。
- 2026-06-27：task29 docs delivery auth entry GREEN：把 `docs/phase5-delivery.md` 与 `docs/README.md` 里的 auth/payment 接入入口补齐到当前真实产物：公开暴露 `docs/auth-payment-live-samples.json` 合同样例和 `./scripts/auth-payment-live-verification.sh` 可执行脚本模板，并明确它们仍不是 live 远端验收结果。通过 `rg` 读回确认 `docs/` 已不再只把人导向内部文档。
- 2026-06-27：task30 root/docs auth entry GREEN：继续把最外层索引拉平，`README.md` 与 `docs/README.md` 现在都明确挂出了 auth/payment 合同样例和联调脚本模板入口，不再只给内部 `dev-docs` 路径。通过 `rg` 读回确认两个入口已可见。
- 2026-06-27：task31 e2e aggregator exit-code fix GREEN：修正 `tests/e2e/run_all.sh` 会把失败脚本退出码打印成 `0` 的问题，并允许用 `TEST_E2E_DIR` 覆盖扫描目录做隔离测试。新增 `tests/e2e/test_run_all_reports_failure_exit_code.sh` 后通过；随后 fresh 执行 `bash tests/e2e/run_all.sh`，确认新的 auth/payment 脚本 e2e 已被聚合入口拾取，且失败脚本现在会正确显示 `exit code 127`。当前聚合入口仍有既有 `test_multi_workspace.sh` 环境失败，不属于本轮新增回归。
- 2026-06-27：task32 live summary autofill GREEN：`scripts/auth-payment-live-verification.sh` 的 `summary` 模式现在会直接打印本地自动发现到的 `session_id / user_id / device_id`，避免联调前还要手动翻 `session.json` / `entitlement.json`。扩展 `tests/e2e/test_auth_payment_live_verification.sh` 后再次执行通过。
- 2026-06-27：task33 local core acceptance script GREEN：新增 `scripts/verify-local-cli-core.sh`，把本地 CLI/Core 产品验收收口成单一入口，覆盖 `cli::tests::`、auth/payment 样例校验、联调脚本 e2e、e2e 聚合器回归，以及临时 workspace 上的 `init / doctor / report` smoke。fresh 执行 `./scripts/verify-local-cli-core.sh` 通过；当前仍有既有 Rust warning 与 provider 未配置导致的 `doctor needs_attention`，但不构成本地 Core 验收失败。
- 2026-06-27：task34 local auth placeholder browser fix GREEN：新增 `auth_login_without_auth_service_does_not_attempt_to_open_placeholder_browser_page` 后，先确认未配置 auth 服务时 `auth login` 仍会诱导打开 `auth.audit-risk.local` 占位页；随后修正为只生成本地 `session.json / device_secret`，并明确提示“当前不会自动打开浏览器，需要先配置 auth 服务地址”。执行 `cargo test --manifest-path engine/Cargo.toml auth_login_without_auth_service_does_not_attempt_to_open_placeholder_browser_page -- --nocapture` 与临时 entitlement 目录下的 `cargo run --manifest-path engine/Cargo.toml --bin audit-risk -- auth login` 通过。
- 2026-07-17：session D1 init atomicity GREEN：`audit-risk init` 已改为全量预检后再 staging/提交；多冲突一次性返回且零写入，后续目标无效时不再留下半初始化文件，`--force` 覆盖前保留 `.bak.<timestamp>-<uuid>`，`--dry-run` 在 JSON/人类模式都只展示计划且不写文件、不修改 `core.hooksPath`。`check` 生成的托管 `.hologram/.gitignore` 已与 init 模板统一，旧版精确托管内容也可安全迁移。fresh 执行 `cargo test --manifest-path engine/Cargo.toml init_ -- --nocapture` 通过 14 条相关测试；临时 Git workspace smoke 证明 dry-run 后磁盘只有原 `.git`，实际 init 生成 6 个接入文件，force 仅备份并替换被修改的 `delivery.json`，无 `.tmp` 残留。
- 2026-07-17：session D2 doctor read-only GREEN：`audit-risk doctor` 已移除 `.hologram` 与 audit parent 的目录创建副作用，缺失路径只报 `needs_attention`、只读目录报 `error`，并新增只读的 `core.hooksPath + .githooks/pre-commit` 联合诊断。fresh 执行 `cargo test --manifest-path engine/Cargo.toml doctor_ -- --nocapture` 通过 7 条相关测试；对不存在路径的真实 CLI smoke 返回 exit 3 后路径仍不存在；初始化 workspace 的 hook check 返回 `status=ok`。
- 2026-07-17：session D1/D2 verification：`cargo check --manifest-path engine/Cargo.toml` fresh 通过。完整 `cargo test --manifest-path engine/Cargo.toml cli::tests:: -- --nocapture` 为 107 通过、1 失败；唯一失败是会话 C3 范围的 `workspace_check_payload_uses_explicit_files_without_git_status_fallback`，D1/D2 本轮未跨范围修改。
- 2026-07-17：session D3 independent approval GREEN：产品入口确定为独立 `audit-risk approve` 命令，不实现 `audit-risk:ignore` 注释豁免。审批只接受最近 `review_check` 中存在的 finding，通过会话 B 的 `append_audit_entry` 写入 `approval.granted` 哈希链记录；`check` / `report` 仅在 canonical workspace root、finding_id、结构化指纹、证据文件内容哈希和有效期同时匹配时将该 finding 降为 `allow`，并在后续 `review_check.details.applied_approvals` 留痕。共享绝对 audit 路径的双 workspace RED 测试先复现了跨 workspace 错误放行，加入 workspace 隔离后转 GREEN。真实 CLI smoke 验证首轮 check exit 2、approve 后 check exit 0、审计查询可见完整 approval 记录、修改证据后再次 exit 2；fresh `cargo test --manifest-path engine/Cargo.toml --lib` 为 536 passed、0 failed。
- 2026-07-17：session E0-E4 GitHub Action / CI GREEN（文件级增量）：Composite Action 已复用同一 ref 内的 `install.sh`，CI 强制 checksum 可验证，latest 不再调用 GitHub Releases API；扫描 stdout JSON 与 stderr 日志分离，JSON 合同解析失败按环境错误关闭，报告与 stderr 通过 artifact 上传，PR 评论读取真实 `plain_explanation` 且无效报告不再误报通过。PR/push 会在 `fetch-depth: 0` 的 head checkout 上显式比较 base/head，以 NUL 清单复用 C3 `--files-from`，初次 push、rename、delete 和未知事件均有明确边界；生成的 init workflow 同步采用相同文件级模型，并固定到准备发布的 `v0.4.2` Action ref，不再复制 installer 或回退到源码构建。fresh 执行 `cargo test --manifest-path engine/Cargo.toml --lib -- --test-threads=1` 为 536 通过、0 失败，`cargo check --manifest-path engine/Cargo.toml`、`audit-risk --version`（0.4.2）、`sh -n install.sh`、安装脚本分离参数 help、fake curl 下 strict checksum 缺失 exit 1、三个仓库 YAML 解析、真实临时 workspace `init` 后生成 workflow YAML 解析与 `git diff --check` 均通过；本机未安装 `actionlint`。默认并行 `cargo test --lib` 当前会因 engine/MCP/CLI 测试共享全局 workspace 发生跨模块竞态，曾出现 534/536 与 535/536，通过串行模式规避；该测试基础设施缺口不属于 E0-E4。当前 `external_nul_list` 扫描的是变更文件完整内容，尚无稳定 finding 指纹/行级 baseline 合同，因此不得宣称只阻断 diff 新增行中的 finding。
- 2026-07-18：task3 check progress GREEN：`audit-risk check` 人类模式的 spinner 在同步扫描期间显示“正在读取项目文件并分析改动...”，payload 返回后切换为“正在生成扫描结果...”，JSON 模式与扫描、审计文件读写顺序保持不变。新增阶段文案回归测试；fresh `cargo test --lib` 为 541 通过、0 失败，高于改动前 539 通过的工作树基线。

## Drift Lock

以下检查项用于防止后续 agent 偏离当前主线。

## Drift Checklist

- 不把本项目重新漂回“通用 3D 代码星图”。
- 不把客户自带模型 API 改成平台统一供应模型。
- 不把规则、审计、审批、自修复语义藏在 prompt 或 UI 文案。
- 不在缺少 evidence、gate decision、audit event 时宣称风险审查闭环完成。
- 不在缺少 repair plan、测试和审批时宣称自动修复可用。
