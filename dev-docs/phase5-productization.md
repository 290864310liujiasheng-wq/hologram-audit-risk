# 第五阶段交付化

生成日期：2026-06-23

## 单句定义

第五阶段不是重做前四阶段内核，而是把 AI 编码风控平台推进到“CLI 可以初始化、集成、自动化调用、导出管理报告，桌面壳退到管理后台角色”的交付化阶段。

## 本阶段四个面

1. 外部化与接入化：把 workspace/provider/rule-package/audit 的最小接入合同沉到可复用 manifest，并由 CLI 主路径暴露。
2. 发布与部署能力：提供稳定的 `init/report/verify` 入口，而不是要求用户读源码猜流程。
3. CI / Hook / 自动化接入：让 headless check 与 machine-readable report 能进 pre-commit / CI。
4. 运维与管理员视角：让桌面管理后台和管理员命令都能直接导出 normalized audit、active policy 和 current review 主路径。

## 当前投入边界

- 当前阶段默认优先把功能投资放在 CLI 主路径，不继续把前端工作台当作功能主战场。
- 前端若需要改动，目标只能是支撑管理后台、验收证据或 CLI 相关 owner 状态展示；不能为了“做页面”本身继续扩面。
- 与 CLI 首次体验直接相关的实现完成后，必须补一次代码审计，清理明显 bug、无用代码和垃圾防御逻辑，避免把阶段产物堆成新的技术债。
- 当前商业化边界也优先从 CLI 主路径落地：先完成中文首页/帮助/新手引导，再落 CLI 侧 entitlement owner、`auth` 命令面和 Pro gate；服务端未接入前，不在 Core 仓库里伪造付费成功。

## 当前 owner

- `engine/src/bin/audit-risk.rs`：公共 CLI 主入口；`engine/src/bin/hologram-risk-check.rs` 仅保留迁移期兼容壳。
- `engine/src/cli.rs`：CLI 命令合同、零参数首页、`help/tour/auth/observe/notify`、本地 entitlement 状态读取与 Pro gate 的当前 owner。
- `src-ui/src/risk/delivery.ts`：delivery manifest、policy resolve、machine report、hook/CI 模板真源；现阶段已作为 secondary 命令兼容层被 CLI 包装。
- `src-ui/scripts/phase5-delivery.ts`：现阶段的兼容/验证壳；后续不再定义公共命令面。
- `.githooks/pre-commit`：本仓库 pre-commit 接入样例。
- `docs/phase5-delivery.md`：外部接入与运维说明。
- `dev-docs/personal-pro-auth-contract.md`：个人版授权、支付确认、entitlement 缓存、refresh/revoked、doctor 诊断的内部合同真源。

## Core / Pro 边界

- Core 免费版保留：零参数中文首页、`help/tour`、`init`、`doctor`、`check`、`watch`、`diff`、基础白话解释、基础修复建议与 diff 预览、修复方案二次审计验证、基础规则包、JSON/Markdown 基础报告。
- Pro 个人版定价固定为 29 元/月：高级规则包、历史风险对比、增强报告导出、审计日志哈希链签名导出包、`observe`、`notify`、个人规则自定义加载。
- `observe`、`notify`、`watch --observe` 必须受 entitlement gate 控制；修复验证继续保持免费。
- MIT Core 与收费 Pro 必须按双层产品边界处理：当前开源仓库只落 CLI 侧授权合同、本地缓存与 gate，不把“同仓库 if-else 锁收费功能”当成完成态。

## 接入合同

- `delivery.json` 是第五阶段外部化真源，当前至少覆盖：
  - workspace root
  - provider source/env contract
  - review / repair rule package path
  - audit jsonl path 与 report output path
  - automation verify commands / fail gate
- workspace extension rule package 缺失时，不再把它视为硬失败；系统回退到默认 policy。

## 自动化路径

- pre-commit：
  - 提交前生成 `.hologram/latest-risk-report.json`
  - gate 达到 `block` 时阻断提交
  - 当前已在外部临时 git repo 上真实执行生成后的 `.githooks/pre-commit`
- CI：
  - 外部 repo 的 `.github/workflows/hologram-risk.yml` 会 checkout 平台 repo 后导出 machine-readable report artifact
- 管理员导出：
  - `audit-risk report` 统一导出 provider、policy、current review、audit 四个面
  - `audit-risk rules` / `audit-risk audit` / `audit-risk doctor` 已提供规则检查、审计检索与运行诊断命令入口
  - `audit-risk doctor` 现已补最小环境体检：CLI 版本、`git`/`cargo` 依赖、rule package 版本、provider 配置与 audit 路径状态

## 当前迁移方向

- 主产品叙事已切到 `audit-risk` CLI。
- 桌面壳继续保留，但定位切换为管理后台，不再作为日常编码入口。
- 现有 phase5 交付脚本和 delivery 逻辑仍可复用，但公共命令面和主输出合同已收口到 CLI。

## fresh 证据

- 已对真实 repo 跑通 `audit-risk report`
- 已对外部临时 git repo 跑通 `audit-risk init + pre-commit hook + audit-risk report + audit-risk rules + audit-risk audit + audit-risk doctor`
- 已通过 `phase5:verify` 把上述 smoke 与 gates 写回 `dev-docs/evidence/phase5-delivery.json`
- 已对 task4 的 CLI 首次体验做专项审计：`audit-risk init` 能在临时 workspace 生成完整接入文件；`audit-risk doctor` 能返回 engine 版本、`git/cargo/node` 依赖状态、rule package `package_id/version`、provider 与 audit 路径状态；`audit-risk watch` 在人类模式下已补启动提示与首轮扫描反馈。
- 这轮 task4 审计中已发现并修掉两个真实问题：subdir workspace 会把父仓库 `../` 变更误带进 `check/watch`，以及 pre-commit 模板会生成错误的平台根路径/二进制路径；当前 fresh verify 已覆盖这两条回归。
- task5 守护模式输出优化已收口到 CLI owner：`watch` 默认仅展示中高危摘要，`--verbose` 才展开低危；同一文件同一规则 10 分钟内只输出一次，重复命中走 suppressed 路径而不是持续刷屏。
- task6 手机观察与 webhook 简化已收口到 CLI owner：`watch --observe` 会起本地只读观察页，打印 `local/LAN` 地址与二维码图片路径；`notify --test` 会用已配置或显式传入的 webhook URL 做连通性测试，并返回结构化结果。
- task7 CLI 商业化首段 GREEN：`audit-risk` 零参数已切到中文新手首页；`help` / `tour` / `auth login` / `auth status` / `auth logout` / `observe` 已进入 Rust CLI owner；`observe` / `notify` / `watch --observe` 在未授权时会被中文 Pro gate 拦截；本地 entitlement 状态机已冻结为 `active / grace / expired / revoked / device_mismatch / missing`，但服务端未接入前不会伪造 Pro 成功态。
- task22 CLI 统一产品壳 GREEN：`check` / `diff` / `init` / `doctor` / `report` / `notify --test` 默认已切到统一中文产品壳，和首页 / `help` / `tour` / auth / Pro gate 保持同一头部与“状态区 / 主内容区 / 下一步区”布局；机器消费路径改为显式 `--json`，并已同步修正 Rust init 生成的 pre-commit 模板，避免把人类页误写进 JSON 报告文件。
- task8 auth local contract GREEN：`auth login` 现在会在本地 entitlement 目录生成 `device_secret` 与 `session.json`，冻结 `session_id / status=pending / timeout_seconds=300 / poll_interval_seconds=2 / login_url` 合同；`auth status` 已能区分 `active / grace / expired / revoked / device_mismatch / missing / invalid` 并返回中文状态，但仍未接入真实浏览器拉起、`/api/auth/poll` 轮询和 `exchange/refresh` 服务端链路。
- task9 auth pending state GREEN：`auth login` 现会尝试拉起默认浏览器，并把 `poll_url / exchange_url / expires_at` 写进 `session.json`；`auth status` 在 entitlement 缺失但 session 仍为 `pending` 时，会固定显示“登录进行中 / 会话 ID / 浏览器地址 / 会话有效期 / 轮询说明”，`auth logout` 会清理 `session.json` 与本地授权缓存。当前仍未接入真实 `/api/auth/poll`、`/api/auth/exchange`、`/api/entitlement/refresh` HTTP 交互。
- task10 auth success chain GREEN：CLI 侧已补最小 auth transport，支持 `mock://approved` 成功链路与真实 `curl` JSON transport；`auth login` 可在配置 `AUDIT_RISK_AUTH_BASE_URL` 时完成 `poll -> exchange -> entitlement.json / entitlement.sig` 本地落盘，并在 `auth status` 中返回 `已登录`。`device_id` 现由 `device_secret + os + hostname` 派生并参与 entitlement 校验，不匹配时进入 `device_mismatch`。在已授权状态下，`observe` 不再卡在 Pro gate，而会继续进入真实运行时；当前 fresh smoke 在这一步暴露的是环境端口绑定失败，而不是授权问题。
- task11 entitlement refresh/revoked GREEN：CLI 侧已补 `refresh_entitlement_for_dir`，当 entitlement 处于 `active/grace` 且 `last_refresh_time` 超过 6 小时时，会按 `POST /api/entitlement/refresh` 合同尝试刷新，并把结果写回 `entitlement.json / entitlement.sig`。`auth status` 现在会区分 `未登录 / 登录进行中 / 已登录 / 授权已过期 / 授权已撤销 / 设备绑定异常 / 授权文件无效`。当前 fresh 证据已覆盖 active refresh 成功和 revoked refresh 成功；支付查询兜底 `GET /api/payment/query` 仍未接入。
- task12 payment query fallback GREEN：当 `auth exchange` 返回的 entitlement 还不是 `pro_personal_monthly` 时，CLI 现在会按冻结方案走 `GET /api/payment/query`。若查询到 Pro，就把本地 entitlement 升级为 `pro_personal_monthly`；若查询超时，则返回中文“支付确认中”提示，同时把基础 entitlement 以 `payment_pending=true` 形式留在本地，后续 `auth status` 会明确显示“支付确认中”，而不是重新退回泛化未登录。
- task13 auth diagnostics GREEN：`auth status` 已把 `payment_pending` 的中文文案收紧为“已拿到基础授权缓存，但支付结果还没有确认完成”；`audit-risk doctor` 现已补 `auth_service` 与 `entitlement_cache` 两项检查，可直接暴露 auth base URL、缓存状态、当前 plan 和 `payment_pending` 标记，避免后续接真实服务端时只能盲查本地文件。
- task14 auth transport diagnostics GREEN：CLI 侧 auth transport 现在会把服务错误收口成结构化错误码，至少区分 `network_unreachable`、`bad_json`、`timeout`、`auth_service_error`。`audit-risk doctor` 在配置了 auth 服务地址但探测失败时，会把 `auth_service.status=error` 并连同 `code/message` 一起暴露出来，后续接真实远端服务时不再只能看 curl 原始 stderr。
- task15 auth config source GREEN：CLI 侧 auth 服务地址不再只看 `AUDIT_RISK_AUTH_BASE_URL`，现在会优先读取 workspace `delivery.json.auth.base_url`，再回退到环境变量；`auth login`、entitlement refresh、`doctor` 的 auth 探测已统一消费这一处来源，避免后续接真实服务端时配置分叉。
- task16 delivery auth schema parity GREEN：`engine/src/cli.rs` 的 init 模板与 `src-ui/src/risk/delivery.ts` 的 `DeliveryConfig/createDefaultDeliveryConfig/buildDeliveryInitFiles` 已统一补上 `auth.base_url`，Rust/TS 不再对 `delivery.json` 是否包含 auth 服务配置产生分叉。
- task17 auth typed contract GREEN：CLI owner 已把 `session.json`、entitlement envelope、poll/exchange/refresh/payment query 的关键 JSON 形状沉成结构化 Rust 合同类型，不再完全依赖裸 `serde_json::Value` 手拼字段；后续接真实远端样本时，可直接围绕这些类型做 contract-level 验证，而不是继续放大字符串解析分叉。
- task18 live sample truth GREEN：`docs/auth-payment-live-samples.json` 已明确标记为合同样例而非真实远端采样，并补齐 `payment_query_pending.payment_pending=true`；`docs/phase5-delivery.md` 也已同步声明当前样例只用于字段形状对齐，避免对外误读为商业闭环已在本仓库内完成。
- task19 live readiness cleanup GREEN：`auth status` stale entitlement refresh 已显式改为读取当前 workspace `delivery.json.auth.base_url`；未知远端 entitlement `status` 现在进入 `invalid` 而不是按未来 `valid_until` 误放行；过期 pending session 不再继续显示“登录进行中”。相关 Rust CLI 用例已并入 `cli::tests::`，fresh 通过 35 条。
- task20 auth session URL source GREEN：配置了 auth 服务地址后，`auth login` 生成的 `session.json` 现在会把 `poll_url / exchange_url / login_url` 同步切到该 `auth.base_url`，不再继续写死 `auth.audit-risk.local` 占位域名；这让后续真实远端联调时的会话文件和 curl 步骤保持同一来源。
- task21 live verification script GREEN：已新增 `scripts/auth-payment-live-verification.sh`，把文档里的 live auth/payment 验收顺序收口成可执行模板，当前覆盖 `summary / cli_login / cli_status / observe_gate / poll / exchange / payment_query / refresh / evidence_template` 九个动作；默认 `summary` 模式只打印环境变量、支付证据清单和自动发现到的关键字段，不伪造远端样本。
- task22 live verification script executable GREEN：`scripts/auth-payment-live-verification.sh` 已补可执行权限，当前可直接 `./scripts/auth-payment-live-verification.sh` 输出摘要；后续真实联调环境里不需要再手工包一层 `bash`。
- task23 live verification script autofill GREEN：脚本现在会优先复用本地 `session.json` 与 `entitlement.json`，自动补 `poll` 所需的 `session_id` 和 `payment_query / refresh` 所需的 `user_id/device_id`；这让真实联调时不必再手工从缓存文件里抄字段。
- task24 live verification script config source GREEN：live 验收脚本已对齐 Rust CLI 的配置来源，当前会优先读取 `WORKSPACE_ROOT/.hologram/delivery.json.auth.base_url`，只有缺失时才回退到 `AUTH_BASE_URL` 环境变量；后续真实联调时不再需要在 CLI 和脚本之间维护两套 auth 服务地址来源。
- task25 live sample pending semantics GREEN：`docs/auth-payment-live-samples.json` 已修正 success/pending 样例的 `payment_pending` 语义，避免把成功的 Pro entitlement 样例误标为支付确认中；并新增一个标准库 `unittest` 级校验，确保后续不会再把这两个样例写反。
- task26 live evidence template GREEN：live 验收脚本已新增 `evidence_template` 输出，可直接生成结构化 JSON 骨架，承接 CLI 输出、HTTP 响应、订单与回调证据；同时会自动带入当前可推导的 `base_url / session_id / user_id / device_id`，减少真实联调时的手工整理成本。
- task27 live script usage drift cleanup GREEN：脚本文档和脚本自身的摘要输出已统一切到当前真实入口：直接执行 `./scripts/auth-payment-live-verification.sh`，并优先依赖 delivery config 与本地缓存自动发现；不再默认要求每一步都手工传 `AUTH_BASE_URL / USER_ID / DEVICE_ID`。
- task28 live evidence JSON assertion GREEN：脚本级 e2e 已开始把 `evidence_template` 输出当成真实 JSON 结构校验，而不再只做字符串包含判断；这让后续有人改坏 evidence 字段名、层级或自动发现结果时，会第一时间在本地测试里暴露。
- task29 docs delivery auth entry GREEN：`docs/phase5-delivery.md` 与 `docs/README.md` 已补齐 auth/payment 对外交付入口，现在公开暴露合同样例 `docs/auth-payment-live-samples.json` 与可执行模板 `./scripts/auth-payment-live-verification.sh`，避免联调人员只能先钻内部文档找入口。
- task30 root/docs auth entry GREEN：根 `README.md` 与 `docs/README.md` 现在都明确挂出 auth/payment 合同样例和联调脚本模板，最外层入口也已与当前真实产物对齐。
- task31 e2e aggregator exit-code fix GREEN：`tests/e2e/run_all.sh` 已修正失败退出码打印错误，并支持 `TEST_E2E_DIR` 覆盖测试目录；auth/payment 脚本 e2e 现在不只是单独可跑，也已被总入口稳定拾取。
- task32 live summary autofill GREEN：live 验收脚本的 `summary` 模式现在会把当前自动发现到的 `session_id / user_id / device_id` 一起打印出来，联调前不必再手工打开缓存文件确认关键字段。
- task33 local core acceptance script GREEN：已新增 `scripts/verify-local-cli-core.sh` 作为本地 CLI/Core 产品验收入口，当前会串行执行 CLI 单测、样例语义校验、联调脚本 e2e、聚合器回归，以及临时 workspace 上的 `init / doctor / report` smoke。这样后续本地验收不再依赖会话消息里的命令清单。
- task34 local auth placeholder browser fix GREEN：当未配置 auth 服务地址时，`auth login` 现在不再自动打开 `auth.audit-risk.local` 占位页，而是明确停在本地模式，只生成 `session.json / device_secret` 并提示先配置 `delivery.json.auth.base_url` 或 `AUDIT_RISK_AUTH_BASE_URL`。
- live auth/payment gate PENDING：当前仓库内仍无真实 auth/payment 服务端、微信/支付宝回调、29 元/月订单、`next_billing_at` 续费样本与取消/撤销样本。该部分必须按 `dev-docs/auth-payment-live-verification.md` 在外部真实环境验收，不能继续用 `mock://...` 证明完成。

## 停止条件

- 不看源码也能知道如何通过 CLI 初始化一个 workspace。
- 至少一条自动化路径真实可跑通。
- 至少一条管理员导出路径真实可跑通。
- fresh evidence 已写回 `dev-docs/evidence/phase5-delivery.json`。
- 若要宣称个人版 29 元/月商业闭环完成，还必须额外拿到真实 auth/payment 与支付平台样本；当前仓库内只能证明 CLI 侧准备度与验收脚本齐备。
