# 规则分类

生成日期：2026-06-20

本文件定义 AI 编码风控平台第一版的规则分类、严重级别、拦截语义和误报处理口径。它是 `Rule` 合同的业务真源补充，不替代代码实现。

## 分类目标

- 统一 `category`、`severity`、`gate_effect` 的使用方式。
- 防止 UI、prompt、provider 或人工描述各写一套风险语言。
- 为后续多代理审计和受控自修复提供稳定归并口径。

## 一级分类

### `permission`

关注危险工具调用、文件写入、命令执行、权限提升、越权访问。

- 典型规则：写入敏感目录、执行高风险 shell、尝试绕过审批。
- 默认优先级：高。
- 常见 `gate_effect`：`require_approval` 或 `block`。

### `security`

关注密钥、凭证、注入、反序列化、供应链、远程执行、权限边界破坏。

- 典型规则：明文 key、危险依赖升级、拼接命令、未转义输入。
- 默认优先级：高到 critical。
- 常见 `gate_effect`：`warn`、`require_approval`、`block`。

### `architecture`

关注 owner 越界、跨层语义泄露、核心逻辑进入 UI/prompt/adapter、无意义兼容分支。

- 典型规则：UI 自定义 gate decision、provider 决定业务放行、临时脚本成为真源。
- 默认优先级：medium 到 high。
- 常见 `gate_effect`：`warn` 或 `require_approval`。

### `quality`

关注测试缺失、回归风险、无证据完成声明、关键状态流转不闭合。

- 典型规则：没有 failing test 就写核心逻辑、缺少最小验证命令、异常路径未收口。
- 默认优先级：medium。
- 常见 `gate_effect`：`warn`。

### `data_integrity`

关注状态丢失、审计缺失、错误降级不透明、finding 与 evidence 脱钩。

- 典型规则：block 决策无审计、finding 无 evidence、自修复无 rollback。
- 默认优先级：high。
- 常见 `gate_effect`：`require_approval` 或 `block`。

### `operability`

关注客户使用不中断、超时、降级、失败可见性、资源滥用。

- 典型规则：provider 超时导致工作台不可用、多代理无限等待、后台任务不可取消。
- 默认优先级：medium 到 high。
- 常见 `gate_effect`：`warn` 或 `require_approval`。

## 严重级别

### `info`

- 仅提示，不影响流程。
- 适合低风险建议、可读性或观察项。
- `gate_effect` 通常是 `observe`。

### `low`

- 风险明确但影响有限，不应中断客户流程。
- `gate_effect` 通常是 `warn`。

### `medium`

- 可能引入回归、边界漂移或后续治理成本。
- `gate_effect` 通常是 `warn`，必要时 `require_approval`。

### `high`

- 对权限、安全、数据完整性或关键架构边界有明显威胁。
- `gate_effect` 通常是 `require_approval`，部分场景 `block`。

### `critical`

- 一旦执行就可能造成高损失或不可接受风险。
- `gate_effect` 默认是 `block`。

## `gate_effect` 语义

### `observe`

- 仅记录和展示，不改变执行路径。
- 适用于 `info` 级提示或暂时只监控的规则。

### `warn`

- 允许继续，但必须向用户明确展示原因和证据。
- 适用于低到中风险规则。

### `require_approval`

- 不自动继续，要求明确审批。
- 适用于高风险但允许人工覆盖的规则。

### `block`

- 默认阻断，不允许自动继续。
- 适用于 critical 风险，或高风险且无安全兜底的场景。

## 默认映射

| severity | 默认 gate_effect |
| --- | --- |
| `info` | `observe` |
| `low` | `warn` |
| `medium` | `warn` |
| `high` | `require_approval` |
| `critical` | `block` |

说明：

- 这是默认映射，不是强制唯一映射。
- 若规则要偏离默认映射，必须在规则定义里说明原因。

## 误报处理

- `dismissed`：当前 finding 被人工判定为误报，但规则本身仍有效。
- `suppressed`：当前场景被策略性忽略，通常要求更细粒度作用域说明。
- 任何 `dismissed` 或 `suppressed` 都必须记录 actor、理由、时间和对应 evidence。
- 不允许直接删除 finding 代替误报处理。

## 第一版最低要求

- 每条规则必须声明 `category`、`severity`、`gate_effect`。
- 每个高或 critical finding 必须能回溯到具体 rule。
- `block` 和 `require_approval` 必须进入审计。
- 后续代码实现若出现新的 `category`，先更新本文件。
