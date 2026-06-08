# CLAUDE.md — 代码全息观测站

## 项目本质

系统无关的代码库交互式依赖拓扑图生成器。将代码从**序列表示（文件/目录）**转为**空间表示（节点/边/拓扑）**，同时服务人与 LLM。

- **输入**：一个目录路径（零配置）
- **输出**：可交互的全息图 + LLM 可消费的结构化 JSON
- **形态**：Tauri 桌面子应用 + MCP Server + CLI

## 核心架构

```
三层职责：
  程序层（全息仓静态分析）→ 精确穷举，不解释，不推断
  Agent 推理层（LLM）      → 自由回复，跑前参谋
  人类裁决层              → 做决定，去跑
```

## 图数据模型

```
三种节点：
  SYMBOL   — 函数/类/模块/常量/接口/变量
  MEDIUM   — 文件/数据库/队列/缓存/网络/共享内存
  TEMPORAL — 线程/定时器/事件循环/触发器

三种边：
  STRUCTURAL — call / inherit / implement / import / reference / instantiate
  DATA       — read / write / subscribe
  TEMPORAL   — executes_on / triggers / blocks
```

## 项目结构

```
src_python/
  core/          图数据结构(graph.py)、合并器(merger.py)、社区发现(community.py)、diff(diff.py)
  adapters/      语言适配器基类(base.py) + Python适配器(python_adapter.py) + TS适配器(typescript_adapter.py)
  pipeline/      文件发现(discovery.py)、流水线编排(runner.py)、增量缓存(cache.py)
  analysis/      V2 分析: 耦合深度计(coupling.py)、数据流环(dataflow.py)、线程交错(threading.py)、边界(blindspots.py)
  timeline.py    V2 因果审计时间线(SQLite)
  mcp_server.py  MCP JSON-RPC stdio, 13 tools
  cli.py         CLI: analyze/neighbors/impact/path/diff/fragile/cycle/coupling-report/serve
  watcher.py     文件监听(watchdog/轮询) + 自动重跑
```

## 关键约定

- **Windows 路径**：`location` 字段始终用 `\` 分隔，提取文件路径用 `rsplit(":", 1)` 而非 `split(":")`（避免破坏 drive letter）
- **类型兼容**：`from_json()` 反序列化后 node.type / edge.type 变为字符串，代码必须同时处理 `NodeType` 枚举和字符串
- **边分类**：`coupling_depth: 1-4` 写入 `edge.properties`（L1=公开API, L2=内部导入, L3=共享数据, L4=封装穿透）
- **置信度**：线程检测结果必须带 `[确定]/[高置信]/[中等]/[低置信]` 标签，不标注"安全"
- **程序层不做的事**：不解释、不推断、不自动推断因果、不声称找到 bug 根源

## 测试

```bash
pytest tests/ -v     # 311 tests, ~2s
pytest tests/ -q     # 快速验证
```

## 依赖

```
核心: networkx>=3.0 (图算法)
可选: leidenalg>=0.10 (社区发现), watchdog>=4.0 (文件监听)
测试: pytest>=7.0, pytest-cov
无其他外部依赖 — AST/正则/SQLite 全部标准库
```

## V2 快速参考

| 需求 | CLI 命令 | MCP 工具 |
|---|---|---|
| 最脆弱模块 | `hologram fragile -l 5` | `hologram_fragile` |
| 数据流环 | `hologram cycle -m all` | `hologram_cycle` |
| 线程冲突 | — | `hologram_thread_conflicts` |
| 耦合详情 | `hologram coupling-report <mod>` | `hologram_coupling_report` |
| 边界标注 | — | `hologram_blindspots` |
| 时间轴 | — | `hologram_timeline` |
