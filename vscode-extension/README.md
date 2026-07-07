# audit-risk for VS Code

在 VS Code 里直接看到 [audit-risk](https://github.com/834063245-creator/HoloGram) CLI 的风险审查结果，不需要切到终端。

## 当前能力

- `audit-risk: 审查当前工作区` 命令：对当前打开的工作区跑一次 `audit-risk check --json`，把返回的 findings 转成 VS Code 诊断，显示在原生的"问题"面板里。
- `audit-risk: 清除审查结果` 命令：清空当前显示的诊断和侧边栏结果。
- 严重程度映射：`critical`/`high` → Error，`medium` → Warning，`low` → Information，跟 CLI 终端输出的配色逻辑一致。
- 行内诊断悬浮说明：把 CLI 已有的白话解释与 `严重/高危/中危/低危 · rule_id` 一并放进 hover，编辑器里悬浮 squiggle 就能看到和终端一致的上下文。
- 侧边栏"audit-risk 审查结果"面板：展示本轮 gate 决策，按严重程度分组列出 findings，点击可跳转到 `file:line`。
- 保存时自动审查（默认关闭，见下方设置）。

## 前置条件

需要先在本机安装 `audit-risk` CLI：

```sh
curl -sSf https://raw.githubusercontent.com/834063245-creator/HoloGram/main/install.sh | sh
```

如果 `audit-risk` 不在 PATH 里，或者你想指定某个特定版本的二进制，用下面的设置显式指定路径。

## 设置

| 设置项 | 说明 | 默认值 |
|---|---|---|
| `auditRisk.binaryPath` | `audit-risk` 可执行文件的绝对路径 | 空（回退到 PATH 查找） |
| `auditRisk.runOnSave` | 保存文件时自动审查 | `false` |

## 开发

```sh
npm install
npm run compile
```

按 F5（或 `.vscode/launch.json` 里的 "Run Extension"）启动扩展开发宿主窗口进行调试。

## 测试

```sh
AUDIT_RISK_TEST_WORKSPACE=/path/to/some/test/workspace npm test
```

测试会下载一个真实的 VS Code 实例（首次运行较慢，之后会缓存在 `.vscode-test/`），加载扩展，执行审查命令，断言"问题"面板里的诊断内容和行号是否正确。

## 后续方向

已交付：问题面板集成、行内诊断悬浮说明、侧边栏 gate + findings 面板。后续计划：
- 修复建议的 CodeAction 集成（复用 CLI 已有的 self-heal repair proposal，让编辑器里就地触发受控修复）
- 保存时自动触发默认开启（当前默认关闭，避免大项目卡顿）
