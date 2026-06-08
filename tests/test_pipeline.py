"""测试流水线：文件发现、缓存、编排器。"""

import os
import tempfile
import pytest

from src_python.pipeline.discovery import discover_files, DEFAULT_EXCLUDE_DIRS
from src_python.pipeline.cache import IncrementalCache
from src_python.pipeline.runner import PipelineRunner, PipelineReport
from src_python.adapters import AdapterRegistry, PythonAdapter
from src_python.core.graph import Graph


class TestDiscoverFiles:
    @pytest.fixture
    def registry(self):
        reg = AdapterRegistry()
        reg.register(PythonAdapter())
        return reg

    @pytest.fixture
    def temp_dir(self):
        d = tempfile.mkdtemp()
        yield d
        import shutil
        shutil.rmtree(d, ignore_errors=True)

    def mock_file(self, base, path):
        """创建空文件及其父目录。"""
        full = os.path.join(base, path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w") as f:
            f.write("")
        return full

    def test_discovers_py_files(self, registry, temp_dir):
        self.mock_file(temp_dir, "a.py")
        self.mock_file(temp_dir, "b.py")
        self.mock_file(temp_dir, "c.txt")

        files = discover_files(temp_dir, registry)
        assert len(files) == 2
        assert all(f.endswith(".py") for f in files)

    def test_excludes_dirs(self, registry, temp_dir):
        self.mock_file(temp_dir, "src/main.py")
        self.mock_file(temp_dir, ".git/config.py")       # 应跳过
        self.mock_file(temp_dir, "__pycache__/cache.py")  # 应跳过
        self.mock_file(temp_dir, "venv/lib/site.py")      # 应跳过

        files = discover_files(temp_dir, registry)
        paths = [os.path.relpath(f, temp_dir).replace("\\", "/") for f in files]
        assert "src/main.py" in paths
        assert not any(".git" in p for p in paths)
        assert not any("__pycache__" in p for p in paths)
        assert not any("venv" in p for p in paths)

    def test_empty_dir(self, registry, temp_dir):
        files = discover_files(temp_dir, registry)
        assert files == []

    def test_no_matching_files(self, registry, temp_dir):
        self.mock_file(temp_dir, "readme.md")
        self.mock_file(temp_dir, "config.yaml")

        files = discover_files(temp_dir, registry)
        assert files == []

    def test_nested_dirs(self, registry, temp_dir):
        self.mock_file(temp_dir, "a/b/c/d.py")
        self.mock_file(temp_dir, "a/e.py")
        self.mock_file(temp_dir, "f/g.py")

        files = discover_files(temp_dir, registry)
        assert len(files) == 3

    def test_max_depth(self, registry, temp_dir):
        self.mock_file(temp_dir, "a/b/c/d/e/f/g.py")  # depth 7

        files_shallow = discover_files(temp_dir, registry, max_depth=3)
        files_deep = discover_files(temp_dir, registry, max_depth=10)
        assert len(files_shallow) == 0
        assert len(files_deep) == 1

    def test_custom_exclude_dirs(self, registry, temp_dir):
        self.mock_file(temp_dir, "src/main.py")
        self.mock_file(temp_dir, "test/test_main.py")

        files = discover_files(temp_dir, registry, exclude_dirs={"test"})
        paths = [os.path.relpath(f, temp_dir).replace("\\", "/") for f in files]
        assert "test/test_main.py" not in paths
        assert "src/main.py" in paths


class TestIncrementalCache:
    @pytest.fixture
    def cache(self):
        return IncrementalCache()

    def test_hash_consistency(self):
        h1 = IncrementalCache.hash_source("hello world")
        h2 = IncrementalCache.hash_source("hello world")
        h3 = IncrementalCache.hash_source("different")
        assert h1 == h2
        assert h1 != h3
        assert len(h1) == 16

    def test_set_and_get(self, cache):
        g = Graph()
        cache.set("test.py", "abc123", g)
        assert cache.has("test.py")
        assert cache.get_hash("test.py") == "abc123"
        assert cache.get_graph("test.py") is g

    def test_miss(self, cache):
        assert not cache.has("nope.py")
        assert cache.get_hash("nope.py") is None
        assert cache.get_graph("nope.py") is None

    def test_invalidate(self, cache):
        g = Graph()
        cache.set("test.py", "abc", g)
        cache.invalidate("test.py")
        assert not cache.has("test.py")

    def test_clear(self, cache):
        cache.set("a.py", "111", Graph())
        cache.set("b.py", "222", Graph())
        cache.clear()
        assert cache.size == 0

    def test_size(self, cache):
        assert cache.size == 0
        cache.set("a.py", "111", Graph())
        assert cache.size == 1

    def test_save_load_disk(self):
        d = tempfile.mkdtemp()
        try:
            c1 = IncrementalCache(cache_dir=d)
            g = Graph()
            from src_python.core.graph import Node, NodeType
            g.add_node(Node("n1", NodeType.SYMBOL, "f", "f.py:1", "python", "function"))
            c1.set("f.py", "hash123", g)
            c1.save_to_disk()

            c2 = IncrementalCache(cache_dir=d)
            assert c2.has("f.py")
            restored = c2.get_graph("f.py")
            assert restored is not None
            assert restored.node_count == 1
        finally:
            import shutil
            shutil.rmtree(d, ignore_errors=True)


class TestPipelineRunner:
    @pytest.fixture
    def registry(self):
        reg = AdapterRegistry()
        reg.register(PythonAdapter())
        return reg

    @pytest.fixture
    def runner(self, registry):
        return PipelineRunner(registry)

    @pytest.fixture
    def temp_project(self):
        d = tempfile.mkdtemp()
        # Create a small Python project
        os.makedirs(os.path.join(d, "mypkg"), exist_ok=True)
        with open(os.path.join(d, "mypkg", "__init__.py"), "w") as f:
            f.write("from .core import run\n")
        with open(os.path.join(d, "mypkg", "core.py"), "w") as f:
            f.write("""
def helper(x):
    return x * 2

def run(data):
    result = helper(data)
    with open("out.txt", "w") as f:
        f.write(str(result))
    return result
""")
        yield d
        import shutil
        shutil.rmtree(d, ignore_errors=True)

    def test_finds_and_analyzes(self, runner, temp_project):
        graph, report = runner.run(temp_project)
        assert report.total_files >= 2
        assert report.processed_files >= 2
        assert graph.node_count > 0
        assert graph.edge_count > 0

    def test_report_stats(self, runner, temp_project):
        _, report = runner.run(temp_project)
        d = report.to_dict()
        assert "total_files" in d
        assert "processed_files" in d
        assert "elapsed_sec" in d
        assert report.elapsed_sec >= 0

    def test_empty_dir(self, runner):
        d = tempfile.mkdtemp()
        try:
            graph, report = runner.run(d)
            assert graph.node_count == 0
            assert report.total_files == 0
        finally:
            import shutil
            shutil.rmtree(d, ignore_errors=True)

    def test_progress_callback(self, runner, temp_project):
        progress = []
        runner.run(temp_project, on_progress=lambda f, i, t: progress.append((f, i, t)))
        assert len(progress) > 0
        assert all(len(p) == 3 for p in progress)

    def test_incremental_cache_reuse(self, registry, temp_project):
        cache = IncrementalCache()
        r1 = PipelineRunner(registry, cache)
        g1, rep1 = r1.run(temp_project)

        r2 = PipelineRunner(registry, cache)
        g2, rep2 = r2.run(temp_project)

        # 第二次应全部命中缓存
        assert rep2.cached_files == rep2.total_files
        assert rep2.processed_files == 0  # all cached

    def test_errors_reported(self, registry):
        d = tempfile.mkdtemp()
        try:
            with open(os.path.join(d, "broken.py"), "w") as f:
                f.write("def broken(:")  # syntax error
            runner = PipelineRunner(registry)
            graph, report = runner.run(d)
            assert report.error_files >= 1 or len(report.errors) >= 1
        finally:
            import shutil
            shutil.rmtree(d, ignore_errors=True)


class TestPipelineReport:
    def test_defaults(self):
        r = PipelineReport()
        assert r.phase == "init"
        assert r.total_files == 0
        assert r.errors == []

    def test_repr(self):
        r = PipelineReport()
        s = repr(r)
        assert "PipelineReport" in s
        assert "phase=init" in s
