# Phase 5 交付与接入

## 单句定义

第五阶段把平台从“桌面内工作台”推进到“外部仓库可初始化、可调用、可落 CI、可导出审计报告”的可交付产品。

## 接入对象

- `workspace`：被审查的真实 git 仓库。
- `provider`：客户自带模型配置，不由平台统一供给。
- `rule-package`：workspace 级 review / repair 扩展包。
- `audit`：workspace 内 `.hologram/audit.jsonl` 与 machine-readable report 输出。

## 最小初始化

1. 在平台仓库执行：
   - `npm --prefix src-ui run phase5:init -- --workspace /absolute/path/to/customer-repo`
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
- `rule_packages.review_paths`：workspace review 扩展包路径。
- `rule_packages.repair_paths`：workspace repair 扩展包路径。
- `audit.jsonl_path`：append-only 审计日志路径。
- `audit.report_output_path`：machine-readable report 默认输出路径。
- `automation.fail_on_decision`：hook / CI 在什么 gate 决策下失败，默认 `block`。

## 验证与报告

- 本地导出：
  - `npm --prefix src-ui run phase5:report -- --workspace /absolute/path/to/customer-repo --config /absolute/path/to/customer-repo/.hologram/delivery.json --output /absolute/path/to/customer-repo/.hologram/latest-risk-report.json`
- 只导出不阻断：
  - 追加 `--fail-on off`
- 当前 report 会包含：
  - workspace 接入口径
  - provider 就绪说明
  - active review / repair policy snapshot
  - current review summary
  - normalized audit records
  - automation fail gate 结果

## 管理员命令

- 规则视图：
  - `npm --prefix src-ui run phase5:rules -- --workspace /absolute/path/to/customer-repo --config /absolute/path/to/customer-repo/.hologram/delivery.json`
- 审计检索：
  - `npm --prefix src-ui run phase5:audit -- --workspace /absolute/path/to/customer-repo --config /absolute/path/to/customer-repo/.hologram/delivery.json --query preflight --limit 20`
- 运行诊断：
  - `npm --prefix src-ui run phase5:doctor -- --workspace /absolute/path/to/customer-repo --config /absolute/path/to/customer-repo/.hologram/delivery.json`

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
- 当前 review 导出：
  - report 中的 `current_review` 已收口 `workbench_queue` 和 `repair_workbench`
- 排障顺序：
  - 先看 `provider.reason`
  - 再看 `policies.review.policy_snapshot_id` / `policies.repair.policy_snapshot_id`
  - 再看 `audit.records[*].error`
  - 最后看 `.hologram/audit.jsonl` 原始 JSONL

## 当前 fresh 验证

- `phase5:init` 已在临时 workspace 真实生成接入文件
- `phase5:report` 已对真实 repo 和一个外部临时 git repo 真实导出 report
- 外部临时 git repo 的 `.githooks/pre-commit` 已真实执行通过
- `phase5:rules` / `phase5:audit` / `phase5:doctor` 已对外部临时 git repo 真实跑通
- `phase5:verify` 已把上述 smoke、风险测试、构建和 engine bin test 写回 `dev-docs/evidence/phase5-delivery.json`
