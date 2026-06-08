# 🔮 Hologram — 代码全息观测站

<p align="center">
  <em>把代码库从"一维序列"变成"三维星图"——同时服务人与 LLM。</em>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/python-≥3.10-blue.svg" alt="Python">
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg" alt="Platform">
  <img src="https://img.shields.io/badge/license-MIT-green.svg" alt="License">
  <img src="https://img.shields.io/badge/tests-311%20passed-brightgreen.svg" alt="Tests">
  <img src="https://img.shields.io/badge/tauri-2.0-orange.svg" alt="Tauri">
  <img src="https://img.shields.io/badge/MCP-tools%2013-ff69b4.svg" alt="MCP">
</p>

---

## 📖 目录

- [这是什么](#-这是什么)
- [为什么你需要它](#-为什么你需要它)
- [核心概念](#-核心概念)
- [架构概览](#-架构概览)
- [快速开始](#-快速开始)
- [CLI 命令参考](#-cli-命令参考)
- [MCP Server — LLM 的代码地图](#-mcp-server--llm-的代码地图)
- [桌面应用](#️-桌面应用)
- [V2 分析能力](#-v2-分析能力)
- [V3 约束校验](#-v3-约束校验)
- [因果审计时间线](#-因果审计时间线)
- [项目结构](#-项目结构)
- [开发指南](#-开发指南)
- [路线图](#-路线图)
- [FAQ](#-faq)
- [License](#-license)

---

## 🤔 这是什么

**Hologram（代码全息观测站）** 是一个系统无关的、零配置的代码库依赖拓扑图生成器。

传统的代码浏览方式——目录树、文件列表、grep 搜索——都是**一维序列表示**。它们让你看到文件在哪里，但无法回答：

> "如果我改了 `task_scheduler.py` 的 `schedule_next_run` 函数，会波及哪些模块？"
>
> "哪些模块的封装被穿透了（直接访问了别人的私有成员）？"
>
> "为什么这个共享 JSON 文件被三个线程并发写入，却没有锁？"

Hologram 将代码库转换为**空间表示**——一个由节点（符号、文件、线程）和边（调用、读写、时间触发）组成的交互式拓扑图。你可以像观察星座一样观察你的代码库。

**三种交付形态：**

| 形态 | 用途 | 入口 |
|---|---|---|
| 🖥️ **Tauri 桌面应用** | 交互式星图探索、文件预览、终端内嵌、时间轴 | `cargo tauri dev` |
| 🔌 **MCP Server** | LLM Agent 查询全息图的标准化通道（13 个工具） | `hologram serve` |
| ⌨️ **CLI 工具** | 快速分析、查询、diff、V2/V3 报告 | `hologram analyze / fragile / cycle ...` |

---

## 💡 为什么你需要它

### 当前状态 vs Hologram

| 你现在的做法 | Hologram 的做法 |
|---|---|
| `grep` 搜索符号引用 —— 只能找到文本匹配，不理解语义 | 语言适配器解析 AST，精确提取符号、调用、导入关系 |
| 在目录树里翻文件 —— 看不到模块间的关系 | 交互式拓扑图，双击节点看到所有邻居和波及路径 |
| 靠经验判断"改了这里会坏哪里" | `impact` 命令 BFS 波及分析，按层展开所有受影响节点 |
| LLM 读代码时缺少全局拓扑上下文 | MCP 13 个工具让 Agent 先查询全息图再推理 |
| 不知道代码的"脆弱点"在哪里 | V2 耦合深度分析，L4 封装穿透自动排序 Top N |
| 难以发现线程冲突 | 静态检测线程×资源冲突矩阵，标注无锁并发写入 |
| 不知道变更后引入了什么问题 | V3 `check` 命令：5 级信号 → 约束校验 → 变更摘要面板 |

### 核心设计原则

```
三层职责：
  程序层（全息仓静态分析）→ 精确穷举，不解释，不推断
  Agent 推理层（LLM）      → 自由回复，跑前参谋
  人类裁决层              → 做决定，去跑
```

Hologram **不会替你做决定**。它只会告诉你"代码长这样"，不会说"这是个 bug"或"你应该这样改"。因果推断由人类完成，Hologram 负责把证据在时间轴上对齐。

---

## 🧠 核心概念

### 图数据模型

Hologram 用三种节点和三种边描述一个代码库的完整拓扑：

#### 三种节点

| 类型 | 含义 | 示例 |
|---|---|---|
| **SYMBOL** | 函数/类/模块/常量/接口/变量 | `task_scheduler.SchedulerEngine.schedule_next_run` |
| **MEDIUM** | 文件/数据库/队列/缓存/网络/共享内存 | `config.json`, `postgres://...`, `redis://...` |
| **TEMPORAL** | 线程/定时器/事件循环/触发器 | `BackgroundWorker`, `@scheduled(cron="*/5 * * * *")` |

#### 三种边

| 类型 | 关系 | 示例 |
|---|---|---|
| **STRUCTURAL** | call / inherit / implement / import / reference / instantiate | `A.__init__` calls `B.load_config` |
| **DATA** | read / write / subscribe | `BackgroundWorker` writes `config.json` |
| **TEMPORAL** | executes_on / triggers / blocks | `@scheduled` triggers `cleanup_task` |

### 耦合深度分层 (L1–L4)

Hologram 把模块间的边按耦合深度分为四级，写入 `edge.properties.coupling_depth`：

| 级别 | 含义 | 视觉标记 |
|---|---|---|
| **L1** 公开 API | 通过 `__all__` / `export` / public 接口 | 蓝色实线 |
| **L2** 内部导入 | 同包内部模块间的导入 | 浅蓝实线 |
| **L3** 共享数据 | 通过共享文件/数据库/队列通信 | 橙色虚线 |
| **L4** 封装穿透 | 直接访问私有成员 (`_method`, `__field`)，绕过公开接口 | 🔴 红色闪烁虚线 |

**L4 密度最高的模块 = 最脆弱的模块**，这就是 `hologram fragile` 的排序依据。

---

## 🏗 架构概览

```
┌──────────────────────────────────────────────────────────┐
│                    Tauri 桌面应用                          │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌────────────┐  │
│  │ 星图面板  │ │ 文件窗口  │ │ 终端面板  │ │ 时间轴面板  │  │
│  │ Cytoscape│ │ Monaco   │ │  xterm   │ │  Timeline  │  │
│  └──────────┘ └──────────┘ └──────────┘ └────────────┘  │
│                         │                                 │
│               Rust Backend (tauri commands)               │
│          ┌──────┴──────┐          ┌──────────┐           │
│          │ analyze/    │          │ save/load│           │
│          │ read_file/  │          │ snapshot │           │
│          │ list_files  │          │ timeline │           │
│          └──────┬──────┘          └──────────┘           │
└─────────────────┼────────────────────────────────────────┘
                  │ subprocess / MCP
┌─────────────────┴────────────────────────────────────────┐
│                  Python 分析引擎                           │
│                                                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐               │
│  │ Adapters │  │ Pipeline │  │ Analysis │               │
│  │          │  │          │  │          │               │
│  │ Python   │→│ Discovery│→│ Coupling │──→ CLI/MCP     │
│  │ TS/JS    │  │ → Analyze│  │ Dataflow │               │
│  │ (ext.)   │  │ → Merge  │  │ Threading│               │
│  └──────────┘  └──────────┘  │ Timeline │               │
│                               │ Blindspot│               │
│                               └──────────┘               │
│                                                           │
│  ┌──────────────────────────────────────────┐            │
│  │           MCP Server (stdio)             │            │
│  │  13 tools: neighbors / impact / path /   │            │
│  │  fragile / cycle / coupling_report /     │            │
│  │  thread_conflicts / blindspots / ...     │            │
│  └──────────────────────────────────────────┘            │
└──────────────────────────────────────────────────────────┘
```

**流水线三阶段：**

```
Phase 1: 文件发现  →  按扩展名分发给对应适配器
Phase 2: 逐文件分析  →  符号提取 → 介质提取 → 时间提取（三阶段 per file）
Phase 3: 跨文件解析  →  补全导入/调用关系 → 社区发现 → 输出 JSON
```

---

## 🚀 快速开始

### 安装

```bash
# 从源码安装（推荐）
git clone https://github.com/your-org/hologram.git
cd hologram
pip install -e .

# 安装完整依赖（社区发现 + 文件监听）
pip install -e ".[full]"

# 安装开发依赖
pip install -e ".[dev]"
```

### 第一次使用

```bash
# 1. 分析你的项目
hologram analyze ./my-project

# 输出：
#   Analyzing: /abs/path/to/my-project
#     [1/42] src/utils.py
#     [2/42] src/models.py
#     ...
#   Cross-file edges resolved: 156
#   Communities detected: 8
#   Graph saved: /abs/path/to/my-project/hologram_graph.json
#     Nodes: 847, Edges: 2103
#     Communities: 8
#     Time: 2.31s

# 2. 查询某个函数的邻居
hologram neighbors schedule_next_run -g my-project/hologram_graph.json

# 3. 看波及范围
hologram impact schedule_next_run --depth 3 -g my-project/hologram_graph.json

# 4. 启动 MCP Server（供 LLM 使用）
hologram serve -g my-project/hologram_graph.json
```

### 桌面应用

```bash
# 需要安装 Rust 和 Node.js
cd hologram
npm --prefix src-ui install
cargo tauri dev
```

桌面应用启动后：
1. 在终端面板输入 `analyze /path/to/project` 开始分析
2. 左侧出现全息星图，节点按社区着色
3. 点击节点 → 右侧面板显示邻居、波及路径
4. 双击文件节点 → Monaco 编辑器打开源码
5. 底部时间轴记录每次变更

---

## ⌨️ CLI 命令参考

### V1 — 基础图谱分析

```bash
# 分析项目，生成 hologram_graph.json
hologram analyze <project_root> [-o output.json]

# 查询一阶邻接（按边类型分组）
hologram neighbors <node_name> [-g graph.json]

# BFS 波及分析
hologram impact <node_name> [--depth 3] [-g graph.json]

# 两点间所有路径
hologram path <from_node> <to_node> [-g graph.json]

# 两张图快照 diff
hologram diff <before.json> <after.json>

# 启动 MCP Server (stdio)
hologram serve [-g graph.json]
```

### V2 — 深度分析

```bash
# Top N 最脆弱模块（按 L4 封装穿透密度排序）
hologram fragile [-l 5] [-g graph.json]

# 数据流环检测
hologram cycle [-m all|data|llm] [-g graph.json]

# 模块耦合深度详情
hologram coupling-report <module_name> [-g graph.json]
```

### V3 — 约束校验

```bash
# 运行约束校验并输出变更摘要
# 正常流（99%）：输出 "✅ 通过"
# 例外流（1%）：展开变更摘要面板
hologram check <project_root> [-g graph.json]

# 管理约束配置
hologram constraints [project_root] [--init]
```

---

## 🔌 MCP Server — LLM 的代码地图

Hologram 实现了 [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) 的 `tools/list` 和 `tools/call` 语义。LLM Agent 可以通过标准输入/输出（stdio）直接查询全息图。

### 在 Claude Code 中配置

在 `~/.claude/claude_desktop_config.json` 或项目的 `.claude/mcp.json` 中添加：

```json
{
  "mcpServers": {
    "hologram": {
      "command": "python",
      "args": ["-m", "src_python.cli", "serve", "-g", "/path/to/hologram_graph.json"]
    }
  }
}
```

### 13 个 MCP 工具

#### V1 — 基础查询（7 个）

| 工具 | 描述 | 参数 |
|---|---|---|
| `hologram_neighbors` | 一阶邻接，按边类型分组 | `node_id` |
| `hologram_impact` | BFS 波及分层，含延迟节点 | `node_id`, `depth?` |
| `hologram_path` | 两点间所有路径 | `from_id`, `to_id` |
| `hologram_history` | 节点的决策历史 | `node_id` |
| `hologram_community` | 所属社区 + 兄弟节点 | `node_id` |
| `hologram_delayed` | 所有含时间延迟边的节点 | — |
| `hologram_changes` | 上次变更的回看标记 | `project_root?` |

#### V2 — 分析工具（6 个）

| 工具 | 描述 | 参数 |
|---|---|---|
| `hologram_fragile` | Top N 最脆弱模块（L4 密度） | `limit?` |
| `hologram_cycle` | 数据流环列表 | `mode?` |
| `hologram_thread_conflicts` | 线程×资源冲突矩阵 | `node_id?` |
| `hologram_coupling_report` | 模块 L1-L4 耦合分布 | `module_name` |
| `hologram_blindspots` | 边界列表：L4 / 线程 / 环 | `filter?` |
| `hologram_timeline` | 因果审计时间线查询 | `limit?`, `since?` |

### 使用示例

```python
# Agent 调用 MCP 工具的例子

# 1. 查询某个函数的全部邻居
result = mcp_call("hologram_neighbors", {"node_id": "node_a1b2c3d4"})
# → { node, neighbors: [...], incoming: {structural: [...], data: [...]}, outgoing: {...} }

# 2. BFS 波及分析
result = mcp_call("hologram_impact", {"node_id": "node_a1b2c3d4", "depth": 3})
# → { layers: [{depth:0, nodes:[...]}, {depth:1, nodes:[...]}, ...], delayed_nodes: [...] }

# 3. 找最脆弱的 5 个模块
result = mcp_call("hologram_fragile", {"limit": 5})
# → { top_fragile_modules: [...], summary: {total_l4: 23, ...} }

# 4. 检查线程冲突
result = mcp_call("hologram_thread_conflicts", {})
# → { resources: {...}, unlocked_concurrent_writes: 3, unlocked_resources: ["config.json", ...] }

# 5. 查看时间轴
result = mcp_call("hologram_timeline", {"limit": 50, "since": "2026-06-01T00:00:00"})
# → { events: [...], stats: {total_events: 1423, by_type: {...}} }
```

---

## 🖥️ 桌面应用

### 技术栈

| 层 | 技术 |
|---|---|
| 前端框架 | TypeScript + Vite |
| 图可视化 | Cytoscape.js + cose-bilkent 布局 |
| 代码编辑器 | Monaco Editor（VS Code 内核） |
| 终端模拟 | xterm.js |
| 桌面壳 | Tauri 2.0 (Rust) |
| 后端引擎 | Python（子进程） |

### 面板说明

```
┌─────────────────────────────────────────────────────┐
│  [搜索框]  [analyze] [reload]  [fragile] [cycle]    │
├───────────────────────┬─────────────────────────────┤
│                       │                             │
│    全息星图面板        │     文件预览面板             │
│    (Cytoscape.js)     │     (Monaco Editor)         │
│                       │                             │
│    - 节点按社区着色     │     - 双击星图节点打开       │
│    - 拖拽/缩放/平移    │     - 语法高亮               │
│    - 点击看邻居        │     - 行号定位               │
│    - 右键菜单操作      │                             │
│                       │                             │
├───────────────────────┴─────────────────────────────┤
│  ┌─────────────────────────────────────────────────┐│
│  │              终端面板 (xterm.js)                 ││
│  │  > analyze /path/to/project                     ││
│  │  > fragile -l 10                                 ││
│  │  > coupling-report task_scheduler.py             ││
│  └─────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────┤
│              决策时间轴 (Timeline)                    │
│  [commit] [file_changed] [data_changed] [blindspot] │
└─────────────────────────────────────────────────────┘
```

---

## 🔬 V2 分析能力

### 1. 耦合深度分析 (`coupling.py`)

```
输入: 完整的依赖拓扑图
输出: 每个模块的 L1/L2/L3/L4 边分布 + 脆弱度评分

fragility_score = L4_density × (1 + L3_density × 0.5)
```

L4 边（封装穿透）是脆弱度的主要驱动因素——它意味着外部代码绕过了模块的公开 API 直接访问内部实现。

### 2. 数据流环检测 (`dataflow.py`)

三种环类型：

| 类型 | 含义 | 风险 |
|---|---|---|
| **纯代码环** (pure_code) | A 调用 B 调用 C 调用 A | 可能导致无限递归或循环依赖 |
| **数据持久环** (data_persistent) | 代码 → 共享数据 → 代码 → 同一份数据 | 数据一致性问题 |
| **LLM 参与环** (llm_involved) | 代码 → LLM → 生成代码 → 原代码 | 叠加态加剧：LLM 在自身输出上迭代 |

### 3. 线程×资源冲突矩阵 (`threading.py`)

```
静态检测线程间的共享资源访问模式：

资源: config.json
  ├── BackgroundWorker    [W]    # 写入
  ├── HealthCheckThread   [R]    # 读取
  └── MainLoop            [R/W]  # 读写

⚠ 无锁保护: config.json 被两个线程并发写入，未检测到锁
```

**置信度标签：**
- `[确定]` — 线程声明来自静态字面量（`threading.Thread`、`@scheduled`）
- `[高置信]` — 同一文件路径出现在两个线程中
- `[中等]` — 全局变量被两个线程引用，无法静态确定是否真的并发访问
- `[低置信]` — `while+sleep` 模式被识别为轮询，但可能是普通循环

**关键设计约束：不标注"安全"**——只标注"检测到的风险"和"检测不到的区域"。

### 4. 盲区/边界检测 (`blindspots.py`)

综合三种边界类型：
- **L4 封装穿透** — 绕过公开 API 的私有成员访问
- **无锁并发写入** — 多线程写入无锁共享资源
- **LLM 反馈环** — 代码 → LLM → 生成代码 → 原代码

---

## 🛡️ V3 约束校验

V3 在 V1 图谱 + V2 分析之上增加了**约束路由层**：

```
变更文件 → 代码读取 → V2 分析 → L5-L1 信号生成 → 约束校验 → 变更摘要
```

### 5 级信号

| 级别 | 含义 | 触发条件 |
|---|---|---|
| **L5** 🔴 | 阻塞级 | L4 穿透 + 数据流环 + 锁定冲突同时命中 |
| **L4** 🟠 | 危险级 | 新增 L4 封装穿透 + 共享数据变更 |
| **L3** 🟡 | 警告级 | 新增 L3 共享依赖 |
| **L2** 🔵 | 提示级 | 新增 L2 内部导入 |
| **L1** ⚪ | 信息级 | 纯公开 API 变更，无耦合加深 |

### 变更摘要面板

```bash
$ hologram check ./my-project

✅ 通过  ← 99% 的情况，一行输出

# 例外流（1%）：
⚠ 变更摘要 — 2 项违规
  [L4] task_scheduler.py: 新增 3 条封装穿透
        - 直接访问 utils._InternalCache.set(key, val)
        - 绕过 SchedulerEngine.schedule() 直接调用 _execute_task()
  [L3] config.json: 新增写入方 activity_tracker.py
```

### 约束配置

```bash
# 生成默认配置
hologram constraints --init

# 编辑 .hologram/hologram.constraints.yaml:
#   routing:
#     L5: true    # 阻塞级 → 必须展开面板
#     L4: true    # 危险级 → 必须展开面板
#     L3: true    # 警告级 → 默认显示
#     L2: false   # 提示级 → 可抑制
#     L1: false   # 信息级 → 可抑制
#   thresholds:
#     max_L4_per_module: 5
#     max_concurrent_writers: 2
#   allowlist_modules:
#     - "test_*.py"
#     - "migrations/*"
#   denylist_keywords:
#     - "password"
#     - "secret"
```

---

## ⏱️ 因果审计时间线

Hologram 自动记录（不自动推断）代码库的完整变更历史：

### 记录的事件类型

| 事件 | 触发方式 |
|---|---|
| `file_changed` | watchdog 检测到代码文件变更 |
| `data_file_changed` | `.json`/`.db`/`.sqlite` 等共享数据文件的 mtime/size 变更 |
| `commit` | git commit 事件 |
| `blindspot_detected` | V2 分析检测到新边界 |
| `user_action` | 用户在 GUI 中的操作记录 |

### 时间轴查询

```sql
-- 时间轴存储在 .hologram/timeline.db (SQLite)

-- 查看最近的变更
SELECT timestamp, event_type, file, summary
FROM events
ORDER BY timestamp DESC
LIMIT 20;

-- 查看某个数据文件的所有读写时序
SELECT timestamp, event_type, changed_by, summary
FROM events
WHERE file = 'config.json'
ORDER BY timestamp;
```

### 设计约束

- ❌ **不自动推断因果关系** — 不会说"这个变更导致了那个 bug"
- ❌ **不声称"找到了根源"** — 程序层只记录事实
- ✅ **在时间轴上对齐** — 代码变更、数据变更、用户操作排成统一时间线
- ✅ **高亮共享热点** — 被多个线程读写的文件自动标记
- ✅ **让人类自己判断** — 提供时序证据，因果推断留给人类和 LLM

---

## 📁 项目结构

```
hologram/
├── README.md                      # 你正在看的这个文件
├── CLAUDE.md                      # Claude Code 项目指令
├── pyproject.toml                 # Python 项目配置
│
├── src_python/                    # Python 分析引擎
│   ├── core/                      # 核心图数据结构
│   │   ├── graph.py               #   节点/边/图/社区的 dataclass 定义
│   │   ├── merger.py              #   跨文件图合并 + 去重
│   │   ├── community.py           #   社区发现（Leiden 算法）
│   │   └── diff.py                #   图快照 diff
│   │
│   ├── adapters/                  # 语言适配器
│   │   ├── base.py                #   适配器基类（ABC）
│   │   ├── registry.py            #   适配器注册表（按扩展名分发）
│   │   ├── python_adapter.py      #   Python AST 适配器
│   │   └── typescript_adapter.py  #   TypeScript/JavaScript AST 适配器
│   │
│   ├── pipeline/                  # 流水线编排
│   │   ├── discovery.py           #   文件发现（按扩展名收集）
│   │   ├── runner.py              #   三阶段流水线编排器
│   │   └── cache.py               #   增量缓存（按文件 hash 跳过）
│   │
│   ├── analysis/                  # V2 深度分析
│   │   ├── coupling.py            #   耦合深度分析 (L1-L4)
│   │   ├── dataflow.py            #   数据流环检测
│   │   ├── threading.py           #   线程×资源冲突矩阵
│   │   └── blindspots.py          #   盲区/边界检测
│   │
│   ├── routing/                   # V3 约束校验
│   │   ├── patterns.py            #   变更模式定义
│   │   ├── signals.py             #   L5-L1 信号生成
│   │   ├── constraints.py         #   约束校验器
│   │   └── summary.py             #   变更摘要面板生成
│   │
│   ├── cli.py                     # CLI 入口（14 个子命令）
│   ├── mcp_server.py              # MCP JSON-RPC Server（13 个工具）
│   ├── watcher.py                 # 文件监听器（watchdog/轮询）
│   └── timeline.py                # 因果审计时间轴（SQLite）
│
├── src-tauri/                     # Rust/Tauri 桌面壳
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       └── main.rs                #   11 个 Tauri 命令
│
├── src-ui/                        # TypeScript 前端
│   ├── package.json               #   Vite + Cytoscape + Monaco + xterm
│   └── src/
│       ├── app.ts                 #   主控制器
│       ├── hologram/              #   星图渲染 (Cytoscape.js)
│       │   ├── graph.ts           #     图渲染引擎
│       │   └── algorithms.ts      #     布局和聚类算法
│       ├── terminal/terminal.ts   #   终端面板 (xterm.js)
│       ├── files/file-window.ts   #   浮动文件窗口 (Monaco Editor)
│       ├── timeline/timeline.ts   #   决策时间轴面板
│       └── components/            #   V2/V3 覆盖层组件
│           ├── overlays.ts        #     盲区/耦合/环覆盖层
│           └── summary-panel.ts   #     变更摘要面板
│
└── tests/                         # 测试
    ├── test_graph.py              #   图数据结构测试
    └── test_diff.py               #   图 diff 测试
```

---

## 🔧 开发指南

### 环境要求

- **Python** ≥ 3.10
- **Rust** (msvc toolchain, for Tauri)
- **Node.js** ≥ 18 (for frontend)
- **可选**：`leidenalg`（社区发现）、`watchdog`（文件监听）

### 运行测试

```bash
# 全部测试（311 tests, ~2s）
pytest tests/ -v

# 快速验证
pytest tests/ -q

# 带覆盖率
pytest tests/ --cov=src_python --cov-report=html
```

### 添加新语言适配器

1. 继承 `src_python/adapters/base.py` 的 `LanguageAdapter`
2. 实现三个方法：
   - `extract_symbols(file_path, source)` → 符号 + 结构边
   - `extract_media(file_path, source, graph)` → 介质 + 数据边
   - `extract_temporal(file_path, source, graph)` → 时间 + 时间边
3. 在 `src_python/adapters/registry.py` 注册（或在 `cli.py` 的 `cmd_analyze` 中注册）
4. 写测试

```python
# 示例：添加 Go 适配器
class GoAdapter(LanguageAdapter):
    language = "go"
    file_extensions = [".go"]

    def extract_symbols(self, file_path, source):
        # 用 go/ast 库解析
        ...
```

### 架构约定

- **Windows 路径**：`location` 字段始终用 `\` 分隔。提取文件路径用 `rsplit(":", 1)` 而非 `split(":")`，避免破坏 drive letter（`D:\foo.py:42` → `D:\foo.py`）
- **类型兼容**：`from_json()` 反序列化后 `node.type` / `edge.type` 变为字符串，代码必须同时处理 `NodeType` 枚举和字符串
- **边分类**：`coupling_depth: 1-4` 写入 `edge.properties`
- **置信度**：线程检测结果必须带 `[确定]/[高置信]/[中等]/[低置信]` 标签，不标注"安全"
- **程序层不做的事**：不解释、不推断、不自动推断因果、不声称找到 bug 根源

---

## 🗺️ 路线图

- [x] **V1** — 基础图谱：Python/TS 适配器、图合并、社区发现、CLI/MCP/Desktop
- [x] **V2** — 深度分析：耦合深度、数据流环、线程冲突、盲区检测、时间轴
- [x] **V3** — 约束校验：信号生成、约束路由、变更摘要面板
- [ ] **V4** — 更多语言适配器：Rust、Go、Java、C/C++
- [ ] **V4** — 增量分析性能优化（大型仓库秒级重跑）
- [ ] **V4** — MCP 流式输出（长查询进度推送）
- [ ] **V4** — CI/CD 集成（GitHub Action / GitLab CI 模板）
- [ ] **V5** — 跨仓库拓扑（微服务依赖图）
- [ ] **V5** — 运行时动态追踪（eBPF / OpenTelemetry 集成）

---

## ❓ FAQ

### Q: Hologram 和 IDE 的 "Go to Definition" 有什么区别？

IDE 的跳转定义是**点对点的**——从 A 跳到 B。Hologram 给出的是**全局拓扑**——A 的所有邻居、A 到 Z 的所有路径、改 A 会波及的所有节点。它回答的是 IDE 不回答的问题，比如"最脆弱的模块是什么"和"这个共享资源被哪些线程读写了"。

### Q: 支持哪些语言？

当前适配了 **Python**（AST 解析）和 **TypeScript/JavaScript**（TS Compiler API 解析）。更多语言适配器（Rust、Go、Java）在 V4 路线图中。适配器架构是开放的——你可以在不修改核心代码的情况下添加新语言。

### Q: 大型项目（10 万+文件）能跑吗？

当前版本面向中型项目（< 5000 文件）优化，有增量缓存机制（按文件 hash 跳过未变更文件）。大型项目的秒级增量重跑在 V4 路线图中。

### Q: Hologram 会修改我的代码吗？

**不会。** Hologram 是纯只读的——它只解析 AST 和读取文件、分析依赖关系、生成图 JSON。它不写入任何项目源码文件（只在 `.hologram/` 目录下存储自己的数据）。

### Q: MCP Server 怎么用？

`hologram serve` 启动 stdio 模式的 MCP Server，你的 LLM Agent（如 Claude Code、Cursor、Continue.dev）可以通过 MCP 协议调用 13 个工具来查询全息图。

### Q: 为什么选择 Tauri 而不是 Electron？

Tauri 2.0 使用系统原生 WebView，体积小（~5MB vs ~150MB），内存占用低。前端仍然使用 Web 技术栈（TypeScript + Cytoscape + Monaco），兼容性不受影响。

### Q: 时间轴记录了什么？

一切变更——代码文件变更、共享数据文件变更（`.json`/`.db`/`.sqlite`）、git commit、盲区检测、用户操作。它**不推断因果**，只在时间轴上对齐，让人自己做判断。

---

## 📄 License

MIT License — 详见 [LICENSE](LICENSE) 文件。

---

<p align="center">
  <sub>Built with ❤️ by the Hologram Team · <a href="https://github.com/your-org/hologram">GitHub</a></sub>
</p>
