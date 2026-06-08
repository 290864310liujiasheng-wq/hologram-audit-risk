"""
文件监听器：基于 watchdog 监听文件变更，自动触发图重跑。
"""

from __future__ import annotations

import os
import sys
import time
import threading
from typing import Callable, Optional, Set

from .core.graph import Graph
from .pipeline import PipelineRunner, IncrementalCache
from .adapters import AdapterRegistry


class FileWatcher:
    """
    监听项目文件变更，自动触发增量重分析。

    使用 watchdog（如果可用）监听文件系统事件，
    在变更发生后延迟（debounce）重跑分析。
    """

    def __init__(
        self,
        root: str,
        registry: AdapterRegistry,
        cache: Optional[IncrementalCache] = None,
        debounce_sec: float = 2.0,
    ):
        self.root = os.path.abspath(root)
        self.registry = registry
        self.cache = cache or IncrementalCache()
        self.debounce_sec = debounce_sec
        self._runner = PipelineRunner(registry, self.cache)
        self._graph: Optional[Graph] = None
        self._callbacks: list[Callable[[Graph], None]] = []
        self._pending: Set[str] = set()
        self._timer: Optional[threading.Timer] = None
        self._lock = threading.Lock()

    @property
    def graph(self) -> Optional[Graph]:
        return self._graph

    def on_graph_updated(self, callback: Callable[[Graph], None]) -> None:
        """注册回调：图更新时调用。"""
        self._callbacks.append(callback)

    def start(self, blocking: bool = True) -> None:
        """启动文件监听。"""
        try:
            from watchdog.observers import Observer
            from watchdog.events import FileSystemEventHandler
        except ImportError:
            print("watchdog not installed. Falling back to polling mode.", file=sys.stderr)
            self._run_polling()
            return

        # 初始分析
        self._full_rebuild()

        class Handler(FileSystemEventHandler):
            def __init__(self, watcher: FileWatcher):
                self._w = watcher

            def on_modified(self, event):
                if not event.is_directory:
                    self._w._on_change(event.src_path)

            def on_created(self, event):
                if not event.is_directory:
                    self._w._on_change(event.src_path)

            def on_deleted(self, event):
                if not event.is_directory:
                    self._w._on_change(event.src_path)

        observer = Observer()
        observer.schedule(Handler(self), self.root, recursive=True)
        observer.start()

        print(f"Watching {self.root} for changes...", file=sys.stderr)

        try:
            if blocking:
                while True:
                    time.sleep(1)
        except KeyboardInterrupt:
            observer.stop()
        observer.join()

    def _on_change(self, file_path: str) -> None:
        """文件变更事件处理（debounce）。"""
        ext = os.path.splitext(file_path)[1]
        if ext not in self.registry.supported_extensions:
            return

        with self._lock:
            self._pending.add(file_path)
            if self._timer is not None:
                self._timer.cancel()
            self._timer = threading.Timer(self.debounce_sec, self._process_pending)
            self._timer.start()

    def _process_pending(self) -> None:
        """处理积累的变更文件列表。"""
        with self._lock:
            files = list(self._pending)
            self._pending.clear()
            self._timer = None

        if not files:
            return

        print(f"Re-analyzing {len(files)} changed file(s)...", file=sys.stderr)
        for fp in files:
            self.cache.invalidate(fp)

        self._full_rebuild()

    def _full_rebuild(self) -> None:
        """全量重建图。"""
        graph, report = self._runner.run(self.root)
        self._graph = graph
        for cb in self._callbacks:
            try:
                cb(graph)
            except Exception:
                pass

    def _run_polling(self) -> None:
        """退化为轮询模式（无 watchdog 时）。"""
        self._full_rebuild()
        last_mtimes: dict[str, float] = {}
        for dirpath, _, filenames in os.walk(self.root):
            for fn in filenames:
                fp = os.path.join(dirpath, fn)
                try:
                    last_mtimes[fp] = os.path.getmtime(fp)
                except OSError:
                    pass

        print(f"Polling {self.root} every {self.debounce_sec}s...", file=sys.stderr)
        try:
            while True:
                time.sleep(self.debounce_sec)
                changed = False
                for fp, old_mtime in list(last_mtimes.items()):
                    try:
                        new_mtime = os.path.getmtime(fp)
                        if new_mtime > old_mtime:
                            last_mtimes[fp] = new_mtime
                            self._on_change(fp)
                            changed = True
                    except OSError:
                        last_mtimes.pop(fp, None)
                if changed:
                    self._full_rebuild()
        except KeyboardInterrupt:
            pass
