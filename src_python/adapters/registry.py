"""
适配器注册表：按文件扩展名自动分发到对应语言适配器。
"""

from __future__ import annotations

from typing import Dict, List, Optional

from .base import LanguageAdapter


class AdapterRegistry:
    """管理所有语言适配器，按文件扩展名查找。"""

    def __init__(self):
        self._adapters: List[LanguageAdapter] = []
        self._ext_index: Dict[str, LanguageAdapter] = {}

    def register(self, adapter: LanguageAdapter) -> None:
        self._adapters.append(adapter)
        for ext in adapter.file_extensions:
            self._ext_index[ext] = adapter

    def find(self, file_path: str) -> Optional[LanguageAdapter]:
        """按扩展名查找适配器。返回 None 表示不支持该文件类型。"""
        for ext, adapter in self._ext_index.items():
            if file_path.endswith(ext):
                return adapter
        return None

    def find_by_language(self, language: str) -> Optional[LanguageAdapter]:
        for a in self._adapters:
            if a.language == language:
                return a
        return None

    @property
    def supported_extensions(self) -> List[str]:
        return list(self._ext_index.keys())

    @property
    def languages(self) -> List[str]:
        return [a.language for a in self._adapters]

    @property
    def adapter_count(self) -> int:
        return len(self._adapters)
