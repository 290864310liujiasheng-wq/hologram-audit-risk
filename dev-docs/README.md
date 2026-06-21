# dev-docs 真源索引

生成日期：2026-06-20

本目录记录“AI 编码风控平台”的内部产品、架构、合同和验收真源。它用于约束后续 agent，不替代当前源码、测试、脚本和 git 状态。

## 当前阶段

- 阶段：第二阶段，多代理审计 + 自修复闭环 + 产品化 UI 路径。
- 当前状态：已完成。owner 层、控制器接线、产品化 UI 路径、真实当前 repo workspace 的桌面态审查与审计证据均已落地。
- 已完成交付：真源文档、`Review Core` 最小 owner、`CheckPanel` 风控摘要、审批写路径、审计读写链路、Agent 对当前审查/最近审计的读取能力、多代理聚合 owner、自修复 plan/proposal/apply/rollback owner、`current_review_summary` 扩展对象、CheckPanel 的多代理/自修复区块、repair audit 读路径。
- 本阶段真实项目证据：当前 repo 已生成 `hologram_graph.json`（`5017` 节点 / `9525` 边）、`.last_project`、`.hologram/audit.jsonl`；Tauri 桌面壳冷启动后已加载真实 repo 并写入 review audit。
- 当前 UI 方向：深色 IDE 工作台。
- 当前模型策略：客户接入自己的模型 API，平台不统一提供模型服务。

## Active Documents

- [product-charter.md](product-charter.md)：产品定位、客户、核心承诺、非目标和停止条件。
- [current-state-audit.md](current-state-audit.md)：当前仓库事实、旧语义边界、可复用基座和风险。
- [architecture.md](architecture.md)：单一主线架构、层级 owner、依赖方向和演进边界。
- [domain-model.md](domain-model.md)：领域对象、状态流转、身份与审计关系。
- [contracts.md](contracts.md)：审查任务、规则命中、拦截决策、审计事件、自修复计划等合同草案。
- [acceptance.md](acceptance.md)：文档阶段验收、后续代码验收口径和漂移锁。
- [rule-taxonomy.md](rule-taxonomy.md)：规则分类、严重级别、拦截语义和误报处理口径。
- [ui-truth.md](ui-truth.md)：深色 IDE 工作台的信息架构、主视图和关键交互真源。
- [multi-agent-orchestration.md](multi-agent-orchestration.md)：多代理角色、编排、去重、冲突和降级规则。
- [self-healing-policy.md](self-healing-policy.md)：受控自修复、审批、测试、审计和回滚策略。

## 可扩展文档

需要继续推进时，再补：
- 与当前代码实现直接相关的 owner 级实现计划、适配器接入设计和阶段 handoff 文档。

## Source Priority

1. 当前源码、测试、脚本、生成合同、运行日志、用户侧证据和当前 git 状态。
2. 根 [AGENTS.md](../AGENTS.md)。
3. 本索引与 active documents。
4. 仍与当前代码相符的 `README.md`、`docs/`、commit history 和旧会话记录。
5. 已被用户否定或与当前主线冲突的旧产品语义，只能作为历史证据，不能继续指导实现。

## 防漂移规则

- 不再把产品讲成“通用 3D 代码星图”。代码图谱是风控审查的证据基座，不是最终产品卖点本身。
- 不再把模型能力包装成平台统一供应。客户自带模型 API 是当前产品边界。
- 不用 prompt 或 UI 文案替代规则、合同、审计和权限系统。
- 不在 UI 层拥有风控判断语义；UI 只展示任务、证据、解释、决策和审批。
- 不在没有规则、审计、权限和回滚合同的情况下实现自动修复。
