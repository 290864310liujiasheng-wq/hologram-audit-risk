# 第五阶段交付化

生成日期：2026-06-23

## 单句定义

第五阶段不是重做前四阶段内核，而是把 AI 编码风控平台推进到“别人可以初始化、集成、自动化调用、导出管理报告”的交付化阶段。

## 本阶段四个面

1. 外部化与接入化：把 workspace/provider/rule-package/audit 的最小接入合同沉到可复用 manifest。
2. 发布与部署能力：提供稳定的 `init/report/verify` 入口，而不是要求用户读源码猜流程。
3. CI / Hook / 自动化接入：让 headless check 与 machine-readable report 能进 pre-commit / CI。
4. 运维与管理员视角：让管理员能直接导出 normalized audit、active policy 和 current review 主路径。

## 当前 owner

- `engine/src/bin/hologram-risk-check.rs`：headless workspace check 入口。
- `src-ui/src/risk/delivery.ts`：delivery manifest、policy resolve、machine report、hook/CI 模板真源。
- `src-ui/scripts/phase5-delivery.ts`：`phase5:init` / `phase5:report` / `phase5:verify` 执行入口。
- `.githooks/pre-commit`：本仓库 pre-commit 接入样例。
- `docs/phase5-delivery.md`：外部接入与运维说明。

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
  - `phase5:report` 统一导出 provider、policy、current review、audit 四个面
  - `phase5:rules` / `phase5:audit` / `phase5:doctor` 已提供规则检查、审计检索与运行诊断命令入口

## fresh 证据

- 已对真实 repo 跑通 `phase5:report`
- 已对外部临时 git repo 跑通 `phase5:init + pre-commit hook + phase5:report + phase5:rules + phase5:audit + phase5:doctor`
- 已通过 `phase5:verify` 把上述 smoke 与 gates 写回 `dev-docs/evidence/phase5-delivery.json`

## 停止条件

- 不看源码也能知道如何初始化一个 workspace。
- 至少一条自动化路径真实可跑通。
- 至少一条管理员导出路径真实可跑通。
- fresh evidence 已写回 `dev-docs/evidence/phase5-delivery.json`。
