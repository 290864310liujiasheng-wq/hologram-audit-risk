"""测试核心图数据结构。"""

import pytest
import json
import os
import tempfile

from src_python.core.graph import (
    Graph, Node, Edge, Community,
    NodeType, EdgeType,
    SymbolKind, MediumKind, TemporalKind,
    StructuralDirection, DataDirection, TemporalDirection,
)


class TestNode:
    def test_create_symbol_node(self):
        node = Node(
            id=Node.make_id(),
            type=NodeType.SYMBOL,
            name="handle_request",
            location="src/handler.py:63",
            language="python",
            kind=SymbolKind.FUNCTION.value,
        )
        assert node.type == NodeType.SYMBOL
        assert node.name == "handle_request"
        assert node.location == "src/handler.py:63"
        assert node.kind == "function"

    def test_create_medium_node(self):
        node = Node(
            id=Node.make_id(),
            type=NodeType.MEDIUM,
            name="/var/log/app.log",
            location="src/logger.py:0",
            language="python",
            kind=MediumKind.FILE.value,
        )
        assert node.type == NodeType.MEDIUM
        assert node.kind == "file"

    def test_create_temporal_node(self):
        node = Node(
            id=Node.make_id(),
            type=NodeType.TEMPORAL,
            name="BackgroundScheduler",
            location="src/scheduler.py",
            language="python",
            kind=TemporalKind.TIMER.value,
            properties={"delay_sec": 3600},
        )
        assert node.type == NodeType.TEMPORAL
        assert node.properties["delay_sec"] == 3600

    def test_node_to_dict(self):
        node = Node("n1", NodeType.SYMBOL, "foo", "foo.py:1", "python", "function")
        d = node.to_dict()
        assert d["id"] == "n1"
        assert d["type"] == "symbol"

    def test_node_hash_eq(self):
        n1 = Node("n1", NodeType.SYMBOL, "a", "a.py:1", "python", "function")
        n2 = Node("n1", NodeType.SYMBOL, "b", "b.py:1", "python", "class")
        n3 = Node("n2", NodeType.SYMBOL, "a", "a.py:1", "python", "function")
        assert n1 == n2  # same id
        assert n1 != n3
        assert len({n1, n2}) == 1  # hash based on id


class TestEdge:
    def test_create_structural_edge(self):
        edge = Edge(
            id=Edge.make_id(),
            type=EdgeType.STRUCTURAL,
            direction=StructuralDirection.CALL.value,
            source="node_a",
            target="node_b",
        )
        assert edge.type == EdgeType.STRUCTURAL
        assert edge.direction == "call"

    def test_create_data_edge(self):
        edge = Edge(
            id=Edge.make_id(),
            type=EdgeType.DATA,
            direction=DataDirection.WRITE.value,
            source="node_a",
            target="node_medium",
            medium_node_id="node_medium",
        )
        assert edge.medium_node_id == "node_medium"

    def test_create_temporal_edge(self):
        edge = Edge(
            id=Edge.make_id(),
            type=EdgeType.TEMPORAL,
            direction=TemporalDirection.EXECUTES_ON.value,
            source="node_a",
            target="node_thread",
            temporal_delay_sec=3600,
        )
        assert edge.temporal_delay_sec == 3600


class TestGraph:
    @pytest.fixture
    def empty_graph(self):
        return Graph()

    @pytest.fixture
    def sample_graph(self):
        g = Graph(source_root="/test")
        # 添加节点
        n1 = Node("n1", NodeType.SYMBOL, "main", "main.py:1", "python", "function")
        n2 = Node("n2", NodeType.SYMBOL, "helper", "main.py:10", "python", "function")
        n3 = Node("n3", NodeType.SYMBOL, "Worker", "worker.py:5", "python", "class")
        g.add_node(n1)
        g.add_node(n2)
        g.add_node(n3)
        # 添加边: n1 calls n2, n1 calls n3
        e1 = Edge("e1", EdgeType.STRUCTURAL, "call", "n1", "n2")
        e2 = Edge("e2", EdgeType.STRUCTURAL, "call", "n1", "n3")
        g.add_edge(e1)
        g.add_edge(e2)
        return g

    def test_add_node(self, empty_graph):
        node = Node("n1", NodeType.SYMBOL, "f", "f.py:1", "python", "function")
        empty_graph.add_node(node)
        assert empty_graph.node_count == 1
        assert "n1" in empty_graph.nodes

    def test_add_duplicate_node_merges_properties(self, empty_graph):
        n1 = Node("n1", NodeType.SYMBOL, "f", "f.py:1", "python", "function", properties={"a": 1})
        n2 = Node("n1", NodeType.SYMBOL, "f", "f.py:1", "python", "function", properties={"b": 2})
        empty_graph.add_node(n1)
        empty_graph.add_node(n2)
        assert empty_graph.node_count == 1
        assert empty_graph.nodes["n1"].properties == {"a": 1, "b": 2}

    def test_add_edge_fails_without_nodes(self, empty_graph):
        edge = Edge("e1", EdgeType.STRUCTURAL, "call", "missing_a", "missing_b")
        result = empty_graph.add_edge(edge)
        assert result is None
        assert empty_graph.edge_count == 0

    def test_remove_node_removes_edges(self, sample_graph):
        sample_graph.remove_node("n1")
        assert "n1" not in sample_graph.nodes
        # edges involving n1 should be removed
        assert "e1" not in sample_graph.edges
        assert "e2" not in sample_graph.edges
        assert sample_graph.edge_count == 0

    def test_neighbors(self, sample_graph):
        neighbors = sample_graph.neighbors("n1")
        assert len(neighbors) == 2
        names = {n.name for n in neighbors}
        assert names == {"helper", "Worker"}

    def test_outgoing_edges(self, sample_graph):
        edges = sample_graph.outgoing_edges("n1")
        assert len(edges) == 2

    def test_incoming_edges(self, sample_graph):
        edges = sample_graph.incoming_edges("n2")
        assert len(edges) == 1
        assert edges[0].source == "n1"

    def test_find_node_by_name(self, sample_graph):
        nodes = sample_graph.find_node_by_name("helper")
        assert len(nodes) == 1
        assert nodes[0].id == "n2"

    def test_find_nodes_by_location(self, sample_graph):
        nodes = sample_graph.find_nodes_by_location("main.py")
        assert len(nodes) == 2

    def test_impact_bfs(self, sample_graph):
        layers = sample_graph.impact_bfs("n1", max_depth=2)
        assert len(layers) >= 1
        assert layers[0]["depth"] == 0
        assert len(layers[0]["nodes"]) == 1  # source node

    def test_impact_bfs_unknown_node(self, sample_graph):
        layers = sample_graph.impact_bfs("nonexistent")
        assert layers == []

    def test_paths(self, sample_graph):
        paths = sample_graph.paths("n1", "n3")
        assert len(paths) == 1
        assert paths[0] == ["n1", "n3"]

    def test_paths_no_connection(self, sample_graph):
        paths = sample_graph.paths("n2", "n3")
        assert paths == []

    def test_merge(self, sample_graph):
        other = Graph()
        n4 = Node("n4", NodeType.SYMBOL, "new_func", "new.py:1", "python", "function")
        other.add_node(n4)
        added = sample_graph.merge(other)
        assert added == 1
        assert sample_graph.node_count == 4

    def test_merge_dedup(self, sample_graph):
        """合并同名同位置的节点不应重复。"""
        other = Graph()
        n1_dup = Node("n99", NodeType.SYMBOL, "main", "main.py:1", "python", "function")
        other.add_node(n1_dup)
        added = sample_graph.merge(other)
        assert added == 0

    def test_nodes_by_type(self, sample_graph):
        counts = sample_graph.nodes_by_type()
        assert counts["symbol"] == 3

    def test_edges_by_type(self, sample_graph):
        counts = sample_graph.edges_by_type()
        assert counts["structural"] == 2

    def test_to_dict_and_back(self, sample_graph):
        d = sample_graph.to_dict()
        assert d["meta"]["node_count"] == 3
        assert d["meta"]["edge_count"] == 2

        g2 = Graph.from_dict(d)
        assert g2.node_count == 3
        assert g2.edge_count == 2

    def test_to_json_and_back(self, sample_graph):
        with tempfile.NamedTemporaryFile(suffix=".json", delete=False, mode="w", encoding="utf-8") as f:
            path = f.name
        try:
            sample_graph.to_json(path)
            g2 = Graph.from_json(path)
            assert g2.node_count == 3
            assert g2.edge_count == 2
        finally:
            os.unlink(path)

    def test_community_to_dict(self, sample_graph):
        c = Community(id="c1", level=0, label="core", node_ids={"n1", "n2"})
        d = c.to_dict()
        assert d["label"] == "core"
        assert set(d["node_ids"]) == {"n1", "n2"}
