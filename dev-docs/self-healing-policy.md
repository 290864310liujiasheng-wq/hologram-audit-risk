# 自修复策略

生成日期：2026-06-20

本文件约束 AI 编码风控平台的受控自修复。核心原则是：先审查、后计划、再审批、最后应用；禁止静默改代码。

## 目标

- 让系统可以对明确风险生成修复方案。
- 不牺牲客户可控性、审计性和回滚能力。
- 不把“模型会改代码”误当成“平台已经安全自修复”。

## 非目标

- 不追求第一版全自动修复闭环。
- 不允许无规则、无测试、无审批直接 apply patch。
- 不允许把自修复当作绕过 gate decision 的通道。

## 修复流程

```text
Finding
  -> RepairPlan
  -> PatchProposal
  -> Approval
  -> Apply-time Rule Re-check
  -> Apply-time Test Gate
  -> Apply
  -> Audit
  -> Rollback if needed
```

## 阶段定义

### `draft`

- 只有修复思路，还不能落盘

### `waiting_approval`

- 已生成 patch proposal
- 已给出风险说明和必跑测试
- 等待人工或策略审批

### `approved`

- 允许进入 apply
- 不代表已经执行
- 也不代表可以跳过 apply-time preflight

### `applied`

- patch 已落地
- 相关测试与 rule re-check 已记录

### `rolled_back`

- 应用后失败或用户撤回
- 必须记录原因和恢复状态

## 审批规则

- `critical` finding 默认需要审批
- 涉及权限、删除、批量写文件、命令执行、依赖升级的 patch 默认需要审批
- 如果当前 job 已 `block`，不能跳过审批直接 apply

## 测试要求

- 每个 repair plan 必须声明 `required_tests`
- 没有最小验证命令时，不允许进入 apply
- 若 finding 只落在 config / migration / serialization 等非代码高风险文件上，repair plan 也必须补最小验证命令；当前默认使用 `git diff --check`
- rule re-check 和 test gate 至少成功其一是不够的，必须两者都过
- apply 瞬间必须重新执行 preflight，不能只信任 proposal 生成当时的旧状态

## 当前默认 repair rule package

- `repair.scope.out_of_scope_write`：patch 不能写出当前 findings 命中的文件范围。
- `repair.scope.absolute_path_write`：patch 不能直接写绝对路径。
- `repair.scope.sensitive_path_write`：patch 不能直接改 `.env`、lockfile、密钥类文件。
- `repair.scope.duplicate_file_write`：同一文件不应出现重复写操作。
- `repair.scope.large_patch_blast_radius`：单次 patch 波及面过大时必须显式暴露风险。
- `repair.test.required_command_failed`：任何必跑验证命令失败都会阻断 apply。

## proposal generation 质量要求

- model 返回的 `operations[].file_path` 必须落在本轮提供给 repair planner 的文件集合内。
- 任何超出输入文件集的 proposal 都直接判为无效，不进入审批或 apply。
- `operations[].new_content` 必须与原文件内容不同；no-op rewrite 直接判为无效。
- `summary`、`rationale`、`operations[].summary` 不能只写 `fix`、`todo`、`update` 一类占位词，必须最少说明修了哪类风险、为什么这么改。
- 若本轮 findings 含 `high/critical` 文件，proposal 必须覆盖全部相关文件，不能只挑其中一个或改低风险文件做表面修复。
- 若本轮 findings 含 `high/critical` 行范围，proposal 的实际改动区间必须触达这些行范围；只改同文件其他位置不算真正修复。
- 若对高风险行范围的改动只是空白、缩进、换行或纯格式化差异，也不算真正修复。
- 若对高风险行范围只改注释文本、代码本体不变，也不算真正修复；但直接删除高风险行本身算合法修复。

## 审计要求

- 生成 repair plan 要写审计事件
- proposal generation 失败也要写 repair audit，并保留错误码、是否可重试和阶段
- 审批通过/拒绝要写审计事件
- apply 成功/失败/rollback 都要写审计事件
- 审计失败时不能假装修复成功

## 回滚要求

- apply 前必须有可回滚路径
- rollback 失败也要记录
- 高风险 patch 若没有明确 rollback 策略，不允许 apply

## UI 要求

- 用户必须能看到：finding、rule、risk note、patch proposal、required tests、approval state
- provider 失败、无 key、超时、缺少源码上下文等 repair issue 必须在 repair 面板里可见，不能只靠 transient status bar
- proposal generation / preflight / apply / rollback 的失败阶段，以及当前 issue 是否建议重试，必须一起进入 repair audit，不能只记录错误码
- 不允许把 apply 按钮放在没有证据和审批上下文的地方
- macOS 桌面壳上的 provider key 必须可通过系统安全存储恢复；不能让 provider 凭证能力仅在 Windows/DPAPI 路径上可用。
- 在尝试 live repair proposal 之前，系统必须能回答 active provider 是否 ready，以及缺失原因是 inline key 缺失、secure store 为空，还是恢复链本身异常。

## provider 错误口径

- `provider_auth_invalid`：鉴权失败、401/403、key 无效或过期；默认不可重试，应先修配置。
- `rate_limited`：429 或明显限流信号；默认可重试。
- `timeout`：请求超时；默认可重试。
- `provider_upstream_failed`：上游 5xx、service unavailable、bad gateway、gateway timeout；默认可重试。
- `network_unreachable`：DNS、连接拒绝、网络不可达、`getaddrinfo`/`ENOTFOUND`/`ECONNREFUSED` 类错误；默认可重试。
- `tls_handshake_failed`：证书校验、TLS/SSL 握手、`x509` 类错误；默认不可重试，应先修证书或网关配置。
- `tls_cert_revoked`：证书已吊销、revocation 失败；默认不可重试，应先修证书链或网关配置。
- `proxy_rejected`：代理连接失败、代理拒绝、407、proxy connect refused；默认可重试，但通常需要先检查代理设置。
- `connection_interrupted`：socket hang up、broken pipe、connection aborted/closed、`ECONNRESET` 等中途断流；默认可重试。
- `provider_unavailable`：provider 不可达、网络或上游服务异常；根据上下文决定是否可重试。

## 第一版最低要求

- 有 `RepairPlan` 合同
- 有审批状态流转
- 有必跑测试列表
- 有 apply-time rule re-check / test gate owner
- 有 apply/rollback 审计口径
- 默认不做静默自动修复
