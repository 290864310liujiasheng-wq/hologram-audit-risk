"""
文件发现：扫描项目目录，找出所有可分析的文件。
"""

from __future__ import annotations

import os
from typing import List, Set, Optional

from ..adapters.registry import AdapterRegistry


# 默认排除的目录和文件
DEFAULT_EXCLUDE_DIRS: Set[str] = {
    ".git", ".svn", ".hg",
    "__pycache__", ".pytest_cache", ".mypy_cache", ".tox", ".eggs",
    "node_modules", ".next", ".nuxt",
    "venv", ".venv", "env", ".env", "virtualenv",
    "dist", "build", "target", "out",
    ".idea", ".vscode",
    ".claude",
}

DEFAULT_EXCLUDE_FILES: Set[str] = {
    ".DS_Store", "Thumbs.db",
}


def discover_files(
    root: str,
    registry: AdapterRegistry,
    exclude_dirs: Optional[Set[str]] = None,
    exclude_files: Optional[Set[str]] = None,
    max_depth: int = 50,
) -> List[str]:
    """
    递归扫描目录，返回所有匹配适配器的文件路径列表。
    跳过排除目录中的文件。
    """
    if exclude_dirs is None:
        exclude_dirs = DEFAULT_EXCLUDE_DIRS
    if exclude_files is None:
        exclude_files = DEFAULT_EXCLUDE_FILES

    supported_exts = set(registry.supported_extensions)
    if not supported_exts:
        return []

    found: List[str] = []

    def _walk(current: str, depth: int) -> None:
        if depth > max_depth:
            return
        try:
            entries = os.scandir(current)
        except (PermissionError, OSError):
            return

        for entry in entries:
            try:
                if entry.is_dir(follow_symlinks=False):
                    if entry.name not in exclude_dirs and not entry.name.startswith("."):
                        _walk(entry.path, depth + 1)
                elif entry.is_file(follow_symlinks=False):
                    if entry.name in exclude_files:
                        continue
                    _, ext = os.path.splitext(entry.name)
                    if ext in supported_exts:
                        found.append(entry.path)
            except OSError:
                continue

    _walk(os.path.abspath(root), 0)
    return sorted(found)
