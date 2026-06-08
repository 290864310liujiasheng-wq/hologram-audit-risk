"""
python -m src_python 入口。

两种模式：
  1. python -m src_python <project_root> --format json  → 输出 JSON 到 stdout
  2. python -m src_python <project_root>               → 输出到 hologram_graph.json
  3. python -m src_python analyze <args>               → 等同于 CLI hologram 命令
"""

import os
import sys
import json

from .adapters import AdapterRegistry, PythonAdapter
from .adapters.typescript_adapter import TypeScriptAdapter
from .core.graph import Graph
from .core.merger import GraphMerger, CrossFileResolver
from .core.community import CommunityDetector
from .core.diff import GraphDiffer
from .pipeline import PipelineRunner, IncrementalCache
from .analysis.coupling import CouplingDepthAnalyzer


def _analyze_and_output(root: str, output_json: bool = False, output_path: str = "") -> Graph:
    """分析项目并输出。使用增量缓存，重复打开只分析变更文件。"""
    root = os.path.abspath(root)

    # 持久化缓存：<project>/.hologram/cache/
    cache_dir = os.path.join(root, ".hologram", "cache")
    cache = IncrementalCache(cache_dir)

    registry = AdapterRegistry()
    registry.register(PythonAdapter())
    registry.register(TypeScriptAdapter())

    runner = PipelineRunner(registry, cache)
    graph, report = runner.run(root)
    cache.save_to_disk()
    print(f"[{report.elapsed_sec:.2f}s] {graph.node_count} nodes / {graph.edge_count} edges  (cached: {report.cached_files})", file=sys.stderr)

    # Cross-file resolution
    resolver = CrossFileResolver()
    cross_added = resolver.resolve(graph)
    if cross_added:
        print(f"  cross-file edges: {cross_added}", file=sys.stderr)

    # Coupling depth analysis — classify every structural edge L1-L4
    try:
        coupler = CouplingDepthAnalyzer()
        # Collect file sources for AST-based detection
        sources = {}
        for fp in report.files:
            try:
                with open(fp, "r", encoding="utf-8", errors="replace") as f:
                    sources[fp] = f.read()
            except (OSError, PermissionError):
                pass
        for fp, src in sources.items():
            coupler.pre_scan_file(fp, src)
        cr = coupler.analyze(graph, sources)
        graph.coupling_summary = cr  # stash for JSON output
        print(f"  coupling: L1={cr['total_l1']} L2={cr['total_l2']} L3={cr['total_l3']} L4={cr['total_l4']}", file=sys.stderr)
    except Exception as exc:
        print(f"  coupling analysis skipped: {exc}", file=sys.stderr)
    except Exception as exc:
        print(f"  coupling analysis skipped: {exc}", file=sys.stderr)

    # Community detection (graceful degradation)
    try:
        detector = CommunityDetector()
        communities = detector.detect(graph)
        if communities:
            print(f"  communities: {len(communities)}", file=sys.stderr)
    except Exception as exc:
        print(f"  community detection skipped: {exc}", file=sys.stderr)

    # Output
    if output_json:
        # JSON to stdout
        d = graph.to_dict()
        import datetime
        d["meta"]["generated_at"] = datetime.datetime.now().isoformat()
        # Attach coupling summary for frontend status bar
        if hasattr(graph, 'coupling_summary'):
            d["meta"]["coupling"] = {k: graph.coupling_summary[k] for k in
                ('total_l1', 'total_l2', 'total_l3', 'total_l4') if k in graph.coupling_summary}
        json.dump(d, sys.stdout, indent=2, ensure_ascii=False)
    else:
        path = output_path or os.path.join(root, "hologram_graph.json")
        graph.to_json(path)
        print(f"  saved: {path}", file=sys.stderr)

    if report.errors:
        for e in report.errors[:5]:
            print(f"  ! {e}", file=sys.stderr)

    return graph


def main():
    # 支持直接传参
    if len(sys.argv) > 1:
        cmd = sys.argv[1]

        # 子命令通过 CLI 处理
        if cmd in ("analyze", "neighbors", "impact", "path", "diff", "serve",
                    "fragile", "cycle", "coupling-report", "check", "constraints"):
            from .cli import main as cli_main
            cli_main()
            return

        # python -m src_python <project_root> [--format json] [-o output.json]
        root = cmd
        output_json = "--format" in sys.argv and "json" in sys.argv
        output_path = ""
        if "-o" in sys.argv:
            idx = sys.argv.index("-o")
            if idx + 1 < len(sys.argv):
                output_path = sys.argv[idx + 1]

        _analyze_and_output(root, output_json, output_path)
    else:
        # 默认分析当前目录
        _analyze_and_output(".", output_path="hologram_graph.json")


if __name__ == "__main__":
    main()
