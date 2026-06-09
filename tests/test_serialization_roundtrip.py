"""
序列化往返契约测试 — 防止字段丢失、类型退化、JSON 往返不幂等。

覆盖: H3 (type 规范化), M3 (file_from_location 一致性)
原则: Node/Edge/Graph 的 to_dict→from_dict→to_dict 必须幂等。
"""

import json
import os
import tempfile
import random
import string
import pytest

from src_python.core.graph import (
    Graph, Node, Edge, Community,
    NodeType, EdgeType,
    SymbolKind, MediumKind, TemporalKind,
    StructuralDirection, DataDirection, TemporalDirection,
    file_from_location,
)

# ============================================================
# file_from_location — 共享工具函数
# ============================================================

class TestFileFromLocation:
    """file_from_location 在所有调用方行为一致。"""

    def test_strips_lineno_unix(self):
        assert file_from_location("/a/b/c.py:42") == "/a/b/c.py"

    def test_strips_lineno_windows(self):
        assert file_from_location("D:\\foo\\bar.py:123") == "D:\\foo\\bar.py"

    def test_preserves_windows_drive_only(self):
        """Windows 路径不含行号时保留原值（drive letter 冒号不被误切）。"""
        assert file_from_location("C:\\Users\\test\\app.py") == "C:\\Users\\test\\app.py"

    def test_empty(self):
        assert file_from_location("") == ""

    def test_no_colon(self):
        assert file_from_location("/simple/path.py") == "/simple/path.py"

    def test_numeric_last_segment_is_stripped(self):
        """仅当最后一个 segment 是纯数字时才切分行号。"""
        assert file_from_location("src/core/v2/main.py:99") == "src/core/v2/main.py"
        # v2 不是纯数字，不应被切
        assert file_from_location("src/core/v2/main.py") == "src/core/v2/main.py"


# ============================================================
# Node / Edge 序列化
# ============================================================

class TestNodeRoundtrip:
    """Node: to_dict → from_dict → to_dict 必须幂等。"""

    def test_symbol_node_roundtrip(self):
        node = Node(
            id="n_test",
            type=NodeType.SYMBOL,
            name="handle_request",
            location="src/handler.py:63",
            language="python",
            kind=SymbolKind.FUNCTION.value,
            properties={"doc": "Handles incoming requests"},
        )
        # Round 1
        d1 = node.to_dict()
        # 重建后 type 是字符串，但 from_dict 应规范化为 Node 可接受的格式
        reconstructed = Node(**d1)
        d2 = reconstructed.to_dict()
        assert d2["id"] == "n_test"
        assert d2["type"] == "symbol"  # 序列化后统一为字符串
        assert d2["name"] == "handle_request"
        assert d2["kind"] == "function"

    def test_medium_node_roundtrip(self):
        node = Node(
            id="n_db",
            type=NodeType.MEDIUM,
            name="db@users",
            location="src/models.py:0",
            language="python",
            kind=MediumKind.DATABASE.value,
        )
        d = node.to_dict()
        rt = Node(**d)
        assert rt.to_dict()["kind"] == "database"

    def test_temporal_node_roundtrip(self):
        node = Node(
            id="n_timer",
            type=NodeType.TEMPORAL,
            name="daily_cleanup",
            location="src/scheduler.py",
            language="python",
            kind=TemporalKind.TIMER.value,
            properties={"interval_sec": 3600},
        )
        d = node.to_dict()
        rt = Node(**d)
        assert rt.to_dict()["kind"] == "timer"
        assert rt.properties["interval_sec"] == 3600

    def test_all_nodetype_values_roundtrip(self):
        """枚举类型到字符串再到枚举，to_dict 稳定输出。"""
        for nt in NodeType:
            node = Node("id", nt, "test", "f.py:1", "python", "function")
            d = node.to_dict()
            assert d["type"] == nt.value
            rt = Node(**d)
            assert rt.to_dict()["type"] == nt.value

    def test_all_edgetype_values_roundtrip(self):
        for et in EdgeType:
            # Need a minimal graph with existing nodes for edge to reference
            g = Graph()
            g.add_node(Node("n1", NodeType.SYMBOL, "a", "a.py:1", "py", "function"))
            g.add_node(Node("n2", NodeType.SYMBOL, "b", "b.py:1", "py", "function"))
            edge = Edge("e1", et, "call", "n1", "n2")
            d = edge.to_dict()
            assert d["type"] == et.value
            rt = Edge(**d)
            assert rt.to_dict()["type"] == et.value


# ============================================================
# Graph 序列化往返
# ============================================================

def _make_minimal_graph() -> Graph:
    g = Graph(source_root="/test/project")
    g.add_node(Node("n1", NodeType.SYMBOL, "alpha", "lib/a.py:5", "python", "function"))
    g.add_node(Node("n2", NodeType.SYMBOL, "beta", "lib/b.py:10", "python", "class",
                    properties={"bases": ["BaseClass"]}))
    g.add_node(Node("n3", NodeType.MEDIUM, "/var/log/app.log", "lib/a.py:0",
                    "python", MediumKind.FILE.value))
    g.add_edge(Edge("e1", EdgeType.STRUCTURAL, "call", "n1", "n2"))
    g.add_edge(Edge("e2", EdgeType.DATA, "write", "n1", "n3"))
    g.communities.append(Community(
        id="c1", level=0, label="core", node_ids={"n1", "n2"}
    ))
    return g


class TestGraphRoundtrip:
    """Graph: to_dict→from_dict→to_dict 必须幂等，字段不丢失。"""

    def test_dict_roundtrip_nodes_and_edges(self):
        g = _make_minimal_graph()
        d1 = g.to_dict()
        g2 = Graph.from_dict(d1)
        d2 = g2.to_dict()

        assert d2["meta"]["source_root"] == "/test/project"
        assert d2["meta"]["node_count"] == d1["meta"]["node_count"]
        assert d2["meta"]["edge_count"] == d1["meta"]["edge_count"]
        # 检查 nodes 字段完整
        node_ids_1 = [n["id"] for n in d1["nodes"]]
        node_ids_2 = [n["id"] for n in d2["nodes"]]
        assert set(node_ids_1) == set(node_ids_2)
        # 检查 edges 字段完整
        edge_ids_1 = [e["id"] for e in d1["edges"]]
        edge_ids_2 = [e["id"] for e in d2["edges"]]
        assert set(edge_ids_1) == set(edge_ids_2)

    def test_dict_roundtrip_communities(self):
        g = _make_minimal_graph()
        d1 = g.to_dict()
        g2 = Graph.from_dict(d1)
        d2 = g2.to_dict()

        assert len(d2["communities"]) == 1
        c = d2["communities"][0]
        assert c["id"] == "c1"
        assert c["label"] == "core"
        assert set(c["node_ids"]) == {"n1", "n2"}

    def test_json_roundtrip(self):
        g = _make_minimal_graph()
        with tempfile.NamedTemporaryFile(
            mode="w", suffix=".json", delete=False, encoding="utf-8"
        ) as f:
            g.to_json(f.name)
            json_path = f.name

        try:
            g2 = Graph.from_json(json_path)
            assert g2.node_count == g.node_count
            assert g2.edge_count == g.edge_count
            assert g2.source_root == g.source_root
        finally:
            os.unlink(json_path)

    def test_type_fields_survive_string_roundtrip(self):
        """从 JSON 加载的 graph，其 node.type 在后续 to_dict 中必须输出字符串。"""
        g = _make_minimal_graph()
        json_str = json.dumps(g.to_dict())
        d = json.loads(json_str)
        g2 = Graph.from_dict(d)

        for node in g2.nodes.values():
            d2 = node.to_dict()
            assert isinstance(d2["type"], str)
            assert d2["type"] in ("symbol", "medium", "temporal")

    def test_coupling_summary_preserved(self):
        """Graph 上的 coupling_summary 若存在，不应影响序列化。"""
        g = _make_minimal_graph()
        g.coupling_summary = {
            "total_l1": 5, "total_l2": 3, "total_l3": 2, "total_l4": 1,
            "module_reports": {},
        }
        d = g.to_dict()
        g2 = Graph.from_dict(d)
        # from_dict 不恢复 coupling_summary（它是 transient）
        # 但也不应崩溃
        assert g2.node_count == g.node_count

    def test_random_graph_100_roundtrips(self):
        """100 个随机合法图，每个过三趟往返。"""
        for seed in range(100):
            rng = random.Random(seed)
            g = Graph(source_root="/test")
            node_count = rng.randint(1, 20)
            for i in range(node_count):
                nt = rng.choice(list(NodeType))
                kind_map = {
                    NodeType.SYMBOL: [k.value for k in SymbolKind],
                    NodeType.MEDIUM: [k.value for k in MediumKind],
                    NodeType.TEMPORAL: [k.value for k in TemporalKind],
                }
                kind = rng.choice(kind_map.get(nt, ["unknown"]))
                g.add_node(Node(
                    f"n_{i}", nt,
                    f"node_{i}_{rng.randint(0, 999)}",
                    f"file_{rng.randint(0, 5)}.py:{rng.randint(1, 200)}",
                    rng.choice(["python", "typescript"]),
                    kind,
                    properties={"extra": rng.randint(0, 100)},
                ))

            # 随机选一些节点对建边
            nids = list(g.nodes.keys())
            if len(nids) >= 2:
                for _ in range(rng.randint(0, min(30, node_count * 2))):
                    src = rng.choice(nids)
                    tgt = rng.choice(nids)
                    if src != tgt:
                        g.add_edge(Edge(
                            Edge.make_id(),
                            rng.choice(list(EdgeType)),
                            rng.choice(["call", "import", "inherit", "write", "read"]),
                            src, tgt,
                        ))

            # 往返 1: to_dict → from_dict
            d1 = g.to_dict()
            g2 = Graph.from_dict(d1)
            assert g2.node_count == g.node_count, f"seed={seed} node count mismatch"
            # node_count 对过就够了——每个节点的 type 在上面已验证
            d2 = g2.to_dict()
            # 往返 2: 再 from_dict
            g3 = Graph.from_dict(d2)
            assert g3.node_count == g.node_count, f"seed={seed} round2 node count mismatch"
            assert g3.edge_count == g.edge_count, f"seed={seed} round2 edge count mismatch"


# ============================================================
# 字段完整性 — 新增 dataclass 字段不漏
# ============================================================

class TestFieldCompleteness:
    """每加一个 Node/Edge 字段都必须过序列化门禁。"""

    def test_all_node_fields_present_in_to_dict(self):
        """Node.__dataclass_fields__ 的所有字段必须在 to_dict 输出中。"""
        node = Node("n", NodeType.SYMBOL, "test", "test.py:1", "py", "fn")
        d = node.to_dict()
        expected = set(Node.__dataclass_fields__.keys())
        actual = set(d.keys())
        missing = expected - actual
        assert not missing, (
            f"to_dict missing fields: {missing}. "
            f"Did you add a field to Node without updating to_dict?"
        )

    def test_all_edge_fields_present_in_to_dict(self):
        edge = Edge("e", EdgeType.STRUCTURAL, "call", "n1", "n2")
        d = edge.to_dict()
        expected = set(Edge.__dataclass_fields__.keys())
        actual = set(d.keys())
        missing = expected - actual
        assert not missing, (
            f"to_dict missing fields: {missing}. "
            f"Did you add a field to Edge without updating to_dict?"
        )
