"""
V3 路由层 — 约束框架 + 变更摘要 + 例外路由

将 V1/V2 的分析结果重新组合为 L5-L1 破坏信号，
应用用户设定的约束阈值，决定自动放行还是路由给人。
"""

from .patterns import PatternMatcher
from .signals import SignalGenerator
from .constraints import ConstraintChecker
from .summary import ChangeSummary, ChangeSummaryGenerator

__all__ = [
    "PatternMatcher",
    "SignalGenerator",
    "ConstraintChecker",
    "ChangeSummary",
    "ChangeSummaryGenerator",
]
