# Phase 5 交付与接入

## 单句定义

第五阶段把平台从“桌面内工作台”推进到“通过 `audit-risk` CLI 初始化、调用、落 CI、导出审计报告”的可交付产品。

## 接入对象

- `workspace`：被审查的真实 git 仓库。
- `provider`：客户自带模型配置，不由平台统一供给。
- `rule-package`：workspace 级 review / repair 扩展包。
- `audit`：workspace 内 `.hologram/audit.jsonl` 与 machine-readable report 输出。

## 最小初始化

1. 在平台仓库执行：
   - `audit-risk init /absolute/path/to/customer-repo`
2. 生成物：
   - `.hologram/delivery.json`
   - `.hologram/rules/review.workspace.json`
   - `.hologram/rules/repair.workspace.json`
   - `.githooks/pre-commit`
   - `.github/workflows/hologram-risk.yml`
3. 若要启用 pre-commit：
   - `git -C /absolute/path/to/customer-repo config core.hooksPath .githooks`

## delivery.json 口径

- `workspace.root`：被审查仓库根目录。
- `provider`：`name` / `model` / `base_url` / `key_source`。
- `auth`：`base_url`，用于 CLI 浏览器登录、轮询、exchange、refresh 与支付确认兜底的服务地址。
- auth/payment 字段形状可参考 [auth-payment-live-samples.json](./auth-payment-live-samples.json)；该文件当前是合同样例，不是真实远端采集样本。
- `rule_packages.review_paths`：workspace review 扩展包路径。
- `rule_packages.repair_paths`：workspace repair 扩展包路径。
- `audit.jsonl_path`：append-only 审计日志路径。
- `audit.report_output_path`：machine-readable report 默认输出路径。
- `automation.fail_on_decision`：hook / CI 在什么 gate 决策下失败，默认 `block`。

## 验证与报告

- 本地 CLI / Core 验收：
  - `./scripts/verify-local-cli-core.sh`
- 本地导出：
  - `audit-risk report /absolute/path/to/customer-repo --config /absolute/path/to/customer-repo/.hologram/delivery.json --output /absolute/path/to/customer-repo/.hologram/latest-risk-report.json`
- 只导出不阻断：
  - 追加 `--fail-on off`
- 当前 report 会包含：
  - workspace 接入口径
  - provider 就绪说明
  - active review / repair policy snapshot
  - current review summary
  - normalized audit records
  - audit integrity summary
  - report signature digest
  - automation fail gate 结果

## 管理员命令

- 规则视图：
  - `audit-risk rules /absolute/path/to/customer-repo --config /absolute/path/to/customer-repo/.hologram/delivery.json`
- 审计检索：
  - `audit-risk audit /absolute/path/to/customer-repo --config /absolute/path/to/customer-repo/.hologram/delivery.json --query preflight --limit 20`
- 运行诊断：
  - `audit-risk doctor /absolute/path/to/customer-repo`

## Auth / Payment 联调

- 合同样例：
  - [auth-payment-live-samples.json](./auth-payment-live-samples.json)
  - 当前文件只用于字段形状对齐，不代表真实远端已验收
- 可执行脚本模板：
  - `./scripts/auth-payment-live-verification.sh`
  - 默认 `VERIFY_STEP=summary`
  - 可切到 `cli_login / cli_status / observe_gate / poll / exchange / payment_query / refresh / evidence_template`
  - 脚本会优先读取 `WORKSPACE_ROOT/.hologram/delivery.json.auth.base_url`，并尽量自动发现本地 `session.json / entitlement.json`
- 若要对真实 auth/payment 服务做 fresh 联调验收，内部步骤与证据要求见 `dev-docs/auth-payment-live-verification.md`。

## CI / Hook 路径

- pre-commit：
  - `.githooks/pre-commit` 会在提交前生成 `.hologram/latest-risk-report.json`
  - 当 gate 决策达到 `automation.fail_on_decision` 时返回非零退出码
  - 当前 fresh verify 已在外部临时 git repo 上真实执行这个 hook
- GitHub Actions：
  - `.github/workflows/hologram-risk.yml` 会先 checkout customer repo，再 checkout 风控平台 repo 到 `.hologram/platform`
  - 需要把 `HOLOGRAM_PLATFORM_REPO` / `HOLOGRAM_PLATFORM_REF` 改成你的平台仓库地址与版本

## 管理员导出与排障

- 审计导出：
  - report 中的 `audit.records` 是统一后的管理员读模型
  - report 中的 `audit.integrity` 会返回 `status`、`entry_count`、`last_hash` 和校验问题列表
  - report 中的 `report_signature.digest` 是导出 JSON 的 SHA-256 摘要，可用于二次校验
- 当前 review 导出：
  - report 中的 `current_review` 已收口 `workbench_queue` 和 `repair_workbench`
- 排障顺序：
  - 先看 `provider.reason`
  - 再看 `audit.integrity.status` 与 `audit.integrity.issues`
  - 再看 `policies.review.policy_snapshot_id` / `policies.repair.policy_snapshot_id`
  - 再看 `audit.records[*].error`
  - 最后看 `.hologram/audit.jsonl` 原始 JSONL

## 审计日志完整性

- 每条新的 `.hologram/audit.jsonl` 记录都会附带：
  - `prev_hash`：上一条记录的哈希；如果前一条还是旧格式，则锚定前一行原始 JSON 文本的 SHA-256
  - `integrity_hash`：当前记录核心字段的 SHA-256
- `audit-risk report` 导出时会重放最近的审计记录并校验整条链：
  - `verified`：所有链式记录都通过
  - `legacy_anchor`：新链条已通过，但前面还存在旧格式历史行
  - `failed`：发现 `prev_hash` 或 `integrity_hash` 不匹配
- 详细设计见 [审计日志不可篡改性技术说明](./审计日志不可篡改性技术说明.md)。

## 当前 fresh 验证

- `audit-risk init` 已在临时 workspace 真实生成接入文件
- `audit-risk report` 已对真实 repo 和一个外部临时 git repo 真实导出 report
- 外部临时 git repo 的 `.githooks/pre-commit` 已真实执行通过
- `audit-risk rules` / `audit-risk audit` / `audit-risk doctor` 已对外部临时 git repo 真实跑通
- `audit-risk verify` 已把上述 smoke、风险测试、构建和 engine bin test 写回 `dev-docs/evidence/phase5-delivery.json`
- 真实 auth/payment 服务端与支付平台样本尚未在本仓库验收；当前仓库只提供 CLI 侧合同、状态机、本地缓存、诊断、合同样例和可执行验收脚本模板。
