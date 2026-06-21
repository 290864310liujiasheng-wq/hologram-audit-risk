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
  -> Rule Re-check
  -> Test Gate
  -> Approval
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
- rule re-check 和 test gate 至少成功其一是不够的，必须两者都过

## 审计要求

- 生成 repair plan 要写审计事件
- 审批通过/拒绝要写审计事件
- apply 成功/失败/rollback 都要写审计事件
- 审计失败时不能假装修复成功

## 回滚要求

- apply 前必须有可回滚路径
- rollback 失败也要记录
- 高风险 patch 若没有明确 rollback 策略，不允许 apply

## UI 要求

- 用户必须能看到：finding、rule、risk note、patch proposal、required tests、approval state
- 不允许把 apply 按钮放在没有证据和审批上下文的地方

## 第一版最低要求

- 有 `RepairPlan` 合同
- 有审批状态流转
- 有必跑测试列表
- 有 apply/rollback 审计口径
- 默认不做静默自动修复
