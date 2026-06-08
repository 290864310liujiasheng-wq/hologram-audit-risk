"""
图数据结构：节点、边、图。
系统无关的中间表示——所有语言适配器产出的统一格式。
"""

from __future__ import annotations

import json
import uuid
from dataclasses import dataclass, field, asdict
from enum import Enum
from typing import Any, Dict, List, Optional, Set


# ============================================================
# 枚举定义
# ============================================================

class NodeType(str, Enum):
    SYMBOL = "symbol"
    MEDIUM = "medium"
    TEMPORAL = "temporal"


class SymbolKind(str, Enum):
    FUNCTION = "function"
    CLASS = "class"
    MODULE = "module"
    CONSTANT = "constant"
    INTERFACE = "interface"
    VARIABLE = "variable"


class MediumKind(str, Enum):
    FILE = "file"
    DATABASE = "database"
    QUEUE = "queue"
    CACHE = "cache"
    NETWORK = "network"
    SHARED_MEMORY = "shared_memory"


class TemporalKind(str, Enum):
    THREAD = "thread"
    TIMER = "timer"
    EVENT_LOOP = "event_loop"
    TRIGGER = "trigger"


class EdgeType(str, Enum):
    STRUCTURAL = "structural"
    DATA = "data"
    TEMPORAL = "temporal"


class StructuralDirection(str, Enum):
    CALL = "call"
    INHERIT = "inherit"
    IMPLEMENT = "implement"
    IMPORT = "import"
    REFERENCE = "reference"
    INSTANTIATE = "instantiate"


class DataDirection(str, Enum):
    READ = "read"
    WRITE = "write"
    SUBSCRIBE = "subscribe"


class TemporalDirection(str, Enum):
    EXECUTES_ON = "executes_on"
    TRIGGERS = "triggers"
    BLOCKS = "blocks"


# ============================================================
# 节点
# ============================================================

@dataclass
class Node:
    """图中的节点——可以是符号、介质或时间结构。"""
    id: str
    type: NodeType
    name: str
    location: str              # 文件路径:行号
    language: str
    kind: str                  # SymbolKind / MediumKind / TemporalKind 的值
    community_id: Optional[str] = None
    properties: Dict[str, Any] = field(default_factory=dict)

    @staticmethod
    def make_id() -> str:
        return f"node_{uuid.uuid4().hex[:8]}"

    def to_dict(self) -> Dict[str, Any]:
        d = asdict(self)
        # 兼容：type 可能已是字符串（从 JSON 反序列化后）
        d["type"] = self.type.value if isinstance(self.type, NodeType) else self.type
        return d

    def __hash__(self) -> int:
        return hash(self.id)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Node):
            return False
        return self.id == other.id


# ============================================================
# 边
# ============================================================

@dataclass
class Edge:
    """图中连接两个节点的边。"""
    id: str
    type: EdgeType
    direction: str                # StructuralDirection / DataDirection / TemporalDirection 的值
    source: str                   # Node.id
    target: str                   # Node.id
    temporal_delay_sec: Optional[float] = None
    medium_node_id: Optional[str] = None
    properties: Dict[str, Any] = field(default_factory=dict)

    @staticmethod
    def make_id() -> str:
        return f"edge_{uuid.uuid4().hex[:8]}"

    def to_dict(self) -> Dict[str, Any]:
        d = asdict(self)
        # 兼容：type 可能已是字符串（从 JSON 反序列化后）
        d["type"] = self.type.value if isinstance(self.type, EdgeType) else self.type
        return d

    def __hash__(self) -> int:
        return hash(self.id)

    def __eq__(self, other: object) -> bool:
        if not isinstance(other, Edge):
            return False
        return self.id == other.id


# ============================================================
# 社区
# ============================================================

@dataclass
class Community:
    """由社区发现算法（Leiden）识别的节点聚类。"""
    id: str
    level: int                  # 层级（0 = 最粗）
    label: str                  # 自动生成的社区名
    node_ids: Set[str] = field(default_factory=set)
    parent_id: Optional[str] = None
    properties: Dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> Dict[str, Any]:
        return {
            "id": self.id,
            "level": self.level,
            "label": self.label,
            "node_ids": list(self.node_ids),
            "parent_id": self.parent_id,
            "properties": self.properties,
        }


# ============================================================
# 图
# ============================================================

@dataclass
class Graph:
    """完整的代码库依赖拓扑图。"""
    nodes: Dict[str, Node] = field(default_factory=dict)
    edges: Dict[str, Edge] = field(default_factory=dict)
    communities: List[Community] = field(default_factory=list)
    source_root: str = ""

    # -- 增删 --

    def add_node(self, node: Node) -> Node:
        if node.id in self.nodes:
            existing = self.nodes[node.id]
            existing.properties.update(node.properties)
            return existing
        self.nodes[node.id] = node
        return node

    def add_edge(self, edge: Edge) -> Optional[Edge]:
        if edge.id in self.edges:
            return None
        if edge.source not in self.nodes or edge.target not in self.nodes:
            return None
        self.edges[edge.id] = edge
        return edge

    def remove_node(self, node_id: str) -> None:
        self.nodes.pop(node_id, None)
        self.edges = {
            eid: e for eid, e in self.edges.items()
            if e.source != node_id and e.target != node_id
        }

    def remove_edge(self, edge_id: str) -> None:
        self.edges.pop(edge_id, None)

    # -- 查询 --

    def get_node(self, node_id: str) -> Optional[Node]:
        return self.nodes.get(node_id)

    def get_edge(self, edge_id: str) -> Optional[Edge]:
        return self.edges.get(edge_id)

    def find_node_by_name(self, name: str) -> List[Node]:
        return [n for n in self.nodes.values() if n.name == name]

    def find_nodes_by_location(self, file_path: str) -> List[Node]:
        """返回指定文件中的所有节点。"""
        return [n for n in self.nodes.values() if n.location.startswith(file_path)]

    def neighbors(self, node_id: str) -> List[Node]:
        """一阶邻接节点。"""
        neighbor_ids: Set[str] = set()
        for e in self.edges.values():
            if e.source == node_id:
                neighbor_ids.add(e.target)
            elif e.target == node_id:
                neighbor_ids.add(e.source)
        return [self.nodes[nid] for nid in neighbor_ids if nid in self.nodes]

    def outgoing_edges(self, node_id: str) -> List[Edge]:
        return [e for e in self.edges.values() if e.source == node_id]

    def incoming_edges(self, node_id: str) -> List[Edge]:
        return [e for e in self.edges.values() if e.target == node_id]

    def impact_bfs(self, node_id: str, max_depth: int = 3) -> List[Dict[str, Any]]:
        """
        BFS 波及分析：从 node_id 出发，按层扩散，返回每层的节点列表。
        结果格式：[{"depth": 0, "nodes": [...]}, {"depth": 1, "nodes": [...]}, ...]
        """
        if node_id not in self.nodes:
            return []
        layers: List[Dict[str, Any]] = []
        visited: Set[str] = {node_id}
        frontier: Set[str] = {node_id}
        for depth in range(max_depth + 1):
            layers.append({
                "depth": depth,
                "nodes": [self.nodes[nid].to_dict() for nid in frontier],
            })
            next_frontier: Set[str] = set()
            for nid in frontier:
                for e in self.edges.values():
                    if e.source == nid and e.target not in visited:
                        next_frontier.add(e.target)
                        visited.add(e.target)
            if not next_frontier:
                break
            frontier = next_frontier
        return layers

    def paths(self, from_id: str, to_id: str, max_len: int = 6) -> List[List[str]]:
        """两点间所有路径（DFS，限长）。"""
        if from_id not in self.nodes or to_id not in self.nodes:
            return []
        all_paths: List[List[str]] = []
        adjacency: Dict[str, List[str]] = {nid: [] for nid in self.nodes}
        for e in self.edges.values():
            adjacency[e.source].append(e.target)

        def dfs(current: str, path: List[str], visited: Set[str]) -> None:
            if len(path) > max_len:
                return
            if current == to_id:
                all_paths.append(list(path))
                return
            for neighbor in adjacency.get(current, []):
                if neighbor not in visited:
                    visited.add(neighbor)
                    path.append(neighbor)
                    dfs(neighbor, path, visited)
                    path.pop()
                    visited.discard(neighbor)

        dfs(from_id, [from_id], {from_id})
        return all_paths

    # -- 统计 --

    @property
    def node_count(self) -> int:
        return len(self.nodes)

    @property
    def edge_count(self) -> int:
        return len(self.edges)

    @property
    def community_count(self) -> int:
        return len(self.communities)

    def nodes_by_type(self) -> Dict[str, int]:
        counts: Dict[str, int] = {}
        for n in self.nodes.values():
            t = n.type.value if hasattr(n.type, 'value') else str(n.type)
            counts[t] = counts.get(t, 0) + 1
        return counts

    def edges_by_type(self) -> Dict[str, int]:
        counts: Dict[str, int] = {}
        for e in self.edges.values():
            t = e.type.value if hasattr(e.type, 'value') else str(e.type)
            counts[t] = counts.get(t, 0) + 1
        return counts

    # -- 合并 --

    def merge(self, other: Graph) -> int:
        """
        将另一个图合并到当前图中。基于 location + name 去重。
        返回新增节点数。
        """
        loc_map: Dict[str, Node] = {}
        for n in self.nodes.values():
            key = f"{n.location}::{n.name}"
            loc_map[key] = n

        added = 0
        for node in other.nodes.values():
            key = f"{node.location}::{node.name}"
            if key not in loc_map:
                self.add_node(node)
                loc_map[key] = node
                added += 1

        for edge in other.edges.values():
            if edge.source in self.nodes and edge.target in self.nodes:
                self.add_edge(edge)

        return added

    # -- 序列化 --

    def to_dict(self) -> Dict[str, Any]:
        return {
            "meta": {
                "source_root": self.source_root,
                "generated_at": "",
                "version": "0.1.0",
                "node_count": self.node_count,
                "edge_count": self.edge_count,
                "community_count": self.community_count,
            },
            "nodes": [n.to_dict() for n in self.nodes.values()],
            "edges": [e.to_dict() for e in self.edges.values()],
            "communities": [c.to_dict() for c in self.communities],
        }

    def to_json(self, file_path: str) -> None:
        import datetime
        d = self.to_dict()
        d["meta"]["generated_at"] = datetime.datetime.now().isoformat()
        with open(file_path, "w", encoding="utf-8") as f:
            json.dump(d, f, indent=2, ensure_ascii=False)

    @classmethod
    def from_dict(cls, d: Dict[str, Any]) -> Graph:
        g = cls(source_root=d.get("meta", {}).get("source_root", ""))
        for nd in d.get("nodes", []):
            g.add_node(Node(**nd))
        for ed in d.get("edges", []):
            g.add_edge(Edge(**ed))
        for cd in d.get("communities", []):
            g.communities.append(Community(
                id=cd["id"],
                level=cd["level"],
                label=cd["label"],
                node_ids=set(cd.get("node_ids", [])),
                parent_id=cd.get("parent_id"),
                properties=cd.get("properties", {}),
            ))
        return g

    @classmethod
    def from_json(cls, file_path: str) -> Graph:
        with open(file_path, "r", encoding="utf-8") as f:
            return cls.from_dict(json.load(f))
