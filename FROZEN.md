# FROZEN.md — 冻结清单（聚焦决定）

> 目的：把「一个人的产能」集中到唯一主线上。冻结 ≠ 删除。下面的东西**保留代码、不删**，
> 但在解冻前不做新功能、不修它们的非阻断 bug、不为它们写文档或做演示。
> 复审日期：**2026-10-08**（三个月后）。

## 唯一主线（只做这条）

**`init → check → 提交拦截`**，围绕它的三件事：

1. 检测质量（`engine/tests/detection_corpus/` 语料 + `detection_quality` 基线，召回 100% / 零误报，只增不降）
2. 降噪后的 CLI 体验（默认只报高置信度，`--verbose` 看全部）
3. Git Hook / CI 集成（`init` 自动激活 pre-commit；生成可运行的 CI workflow）

## 冻结项（保留代码，暂不投入）

| 冻结项 | 位置 | 解冻条件 |
|---|---|---|
| 桌面工作台 / 星图新功能 | `src-tauri/`、`src-ui/`（非 CLI 部分） | 出现真实团队买家要「管理后台」 |
| observe / notify / 二维码旁路 | CLI observe/notify 子命令 | 主线跑通且有用户明确要旁路观察 |
| auth / payment 服务端联调 | `docs/auth-payment-*`、`scripts/auth-payment-*` | 决定正式收费、且有付费意向用户 |
| VS Code 插件新功能 | `vscode-extension/` | 主线稳定后，作为分发渠道再投入 |
| 星图类 C 级 bug | 见 `BUGS.md` C 级 | 解冻桌面端时一并处理 |

## 判断规则

任何新想法先问一句：**它是否直接让「检测更准 / 拦截更稳 / 集成更顺」？**
- 是 → 可以做。
- 否 → 记进 backlog，等 2026-10-08 复审，别现在动。
