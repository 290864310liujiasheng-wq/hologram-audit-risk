# 验收口径

## Stop Conditions

历史文档接管阶段只验收真源和架构边界；第三阶段收口已完成。当前第四阶段已覆盖规则系统、审计系统、工作台路径与稳定交付能力四个面，并继续以 fresh evidence 做增量收口。

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
- 第三阶段收口：fresh `npm run test:risk`、`npx tsc --noEmit`、`npm run build`、`cargo check` 全绿；live provider 成功样本与 provider failure 稳定复现样本口径一致。

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

## Drift Lock

以下检查项用于防止后续 agent 偏离当前主线。

## Drift Checklist

- 不把本项目重新漂回“通用 3D 代码星图”。
- 不把客户自带模型 API 改成平台统一供应模型。
- 不把规则、审计、审批、自修复语义藏在 prompt 或 UI 文案。
- 不在缺少 evidence、gate decision、audit event 时宣称风险审查闭环完成。
- 不在缺少 repair plan、测试和审批时宣称自动修复可用。
