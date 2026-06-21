# UI 真源

生成日期：2026-06-20

本文件约束 AI 编码风控平台第一版工作台 UI。它不是视觉稿，而是信息架构和关键交互真源。

## 总体方向

- 形态：深色 IDE 工作台。
- 核心感受：安静、专业、可扫描、持续工作，不是营销页。
- 第一屏必须直接进入工作台，不做 landing page。
- 代码图谱可以存在，但只能作为 evidence 视图，不占据产品叙事中心。

## 一级布局

### 左侧

工作区与审查导航。

- Workspace 切换
- 审查任务列表
- 风险分类过滤
- 规则视图入口
- 审计视图入口

### 中央

主工作区，默认是当前任务的代码与风险上下文。

- Diff / 文件内容
- 行内风险标记
- 逐行白话解释
- 修复建议或审批提示

### 右侧

决策与证据面板。

- Finding 详情
- Evidence 列表
- Gate decision
- 审批状态
- 审计摘要

### 底部

过程与系统状态。

- Provider 状态
- 审查日志
- 多代理进度
- 降级与超时原因
- 自修复状态与操作按钮

## 默认视图

用户进入工作台后，默认看到：

1. 当前 workspace
2. 最近或正在运行的 review job
3. 当前 job 的 finding 列表
4. 选中 finding 的证据、解释和 gate decision

不默认展示：

- 纯装饰 3D 图
- 空洞 hero 文案
- 产品营销 copy
- 与当前审查任务无关的大卡片统计

## 关键对象映射

- `ReviewJob`：任务列表与状态条
- `ReviewFinding`：风险列表、行内标记、详情抽屉
- `CodeEvidence`：证据面板与跳转
- `GateDecision`：审批/阻断提示条
- `AuditEvent`：审计时间线
- `RepairPlan`：修复草案与回滚信息

## 关键交互

### 选中 finding

- 高亮对应文件和行范围
- 打开白话解释
- 展示证据列表
- 展示当前 gate decision

### 请求审批

- 只在 `require_approval` 时展示审批操作
- 审批前必须展示 rule、severity、reason、evidence
- 审批结果必须立即反映到时间线和状态区

### 查看阻断

- `block` 必须明确显示阻断原因，不允许只给红点或图标
- 必须能看到对应 rule 和 evidence

### 查看降级

- provider 失败、子代理超时、审计失败等降级必须可见
- 不允许静默失败后仍展示“全部正常”

## 第一版必须有的面板

- Review Jobs
- Findings
- Evidence
- Gate Decision
- Audit Timeline
- Provider/Execution Status
- Multi-agent Review
- Repair Plan / Apply / Rollback

## 第一版可以后补的面板

- 规则编辑器
- 多代理细粒度 trace
- 自修复 patch compare
- 审计检索与导出

## 禁止事项

- 不让 UI 自己定义风险等级或拦截逻辑
- 不把审批入口做成隐藏操作
- 不用“模型觉得有风险”替代 rule/evidence/reason
- 不把图谱动画放在主信息之上
- 不用浅色营销化布局替代深色工作台
