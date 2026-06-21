# 多代理编排

生成日期：2026-06-20

本文件定义 AI 编码风控平台的多代理审计编排边界。它约束谁负责什么，不允许“每个代理都各说一套然后 UI 自己拼”。

## 目标

- 让多个审计面可以并行执行。
- 让主智能体统一合并、去重、裁决和给出最终 gate suggestion。
- 让超时、降级、冲突都有结构化出口。

## 角色

### Lead Reviewer

唯一最终裁决者。

- 创建 review job 拆分
- 分发 evidence 子集或审计任务
- 合并 findings
- 处理冲突
- 产出最终 gate decision 建议

### Static Agent

- 消费静态结构、依赖、图谱信号
- 关注 architecture、quality、data_integrity

### Security Agent

- 关注 security、permission、供应链、敏感操作

### Test Regression Agent

- 关注测试缺失、回归风险、变更影响面

### Dependency Agent

- 关注外部依赖、升级风险、隐式耦合

### Repair Planner

- 不直接修复，只生成 repair plan 候选

## 输入输出合同

每个子代理的输入至少包含：

- `job_id`
- `agent_type`
- `input_evidence_ids`
- 当前 applicable rules
- 当前 provider/profile 限制

每个子代理的输出至少包含：

- 候选 `finding_ids`
- 对应 evidence 引用
- `degraded reason` 或 error
- 非最终的建议说明

主智能体输出：

- merged findings
- dropped duplicates
- conflicts
- final gate suggestion

## 编排流程

```text
ReviewJob
  -> evidence prefilter
  -> fan-out to specialist agents
  -> collect candidate findings
  -> deduplicate
  -> resolve conflicts
  -> compute gate decision
  -> write audit events
```

## 去重规则

两个 finding 满足以下条件时优先视为重复：

- 同一 `rule_id`
- 指向同一代码位置或高度重叠位置
- 原因与影响描述指向同一风险

处理方式：

- 保留证据更强的一条为主 finding
- 其他 finding 记录到 `dropped_duplicates`
- 不允许无记录地静默丢弃

## 冲突处理

冲突常见场景：

- 一个代理建议 `warn`，另一个建议 `block`
- 一个代理判为误报，另一个代理给出高置信度风险
- 不同代理引用的证据互相矛盾

处理原则：

- 优先保留更高风险建议进入裁决
- 由 Lead Reviewer 产出 `resolution`
- 冲突必须进入 `ReviewAggregation.conflicts`

## 超时与降级

- 单个子代理超时不能阻塞整个工作台无限等待
- 子代理失败时，job 可进入 `degraded`，不是直接假装 `completed`
- `degraded_reasons` 必须在 UI 和审计中可见

## 与自修复的关系

- 多代理审计先于自修复
- Repair Planner 只能在 findings 和 gate suggestion 基础上工作
- 未完成合并与裁决前，不进入自动 apply

## 第一版最低要求

- 至少支持 Lead Reviewer + 若干 specialist agent 的合同留口
- 支持去重和冲突记录
- 支持子代理超时降级
- 支持主智能体统一给出最终 gate suggestion
