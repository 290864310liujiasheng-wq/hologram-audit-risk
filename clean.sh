#!/usr/bin/env bash
#
# clean.sh — 清理项目里可自动重建的构建产物 / 依赖 / 缓存，回收磁盘空间。
#
# 删除的东西都能自动重建，不会影响项目运行：
#   - Rust 构建产物 target/        → 下次 cargo build/run/test 重建
#   - 依赖 node_modules/           → 下次 npm ci / npm install 重装
#   - 前端构建输出 dist/           → 下次 npm run build 重建
#   - 应用运行时缓存 .hologram/    → 下次运行应用重建
#   - 生成的图数据 hologram_graph* → 下次运行应用重建
#   - 插件测试下载 .vscode-test/   → 下次跑插件测试重新下载
#
# 用法:
#   ./clean.sh            # 直接清理
#   ./clean.sh --dry-run  # 只列出会删什么、能省多少，不实际删除
#
set -euo pipefail

# 切到脚本所在目录（= 仓库根），保证在别处调用也安全
cd "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(pwd)"

DRY_RUN=0
if [[ "${1:-}" == "--dry-run" || "${1:-}" == "-n" ]]; then
  DRY_RUN=1
fi

# 要清理的具体路径（都是 .gitignore 忽略的可重建产物）
TARGETS=(
  "engine/target"
  "src-tauri/target"
  "src-ui/node_modules"
  "src-ui/dist"
  "vscode-extension/node_modules"
  "vscode-extension/.vscode-test"
  "vscode-extension/out"
  "hologram_graph.json"
  "hologram_graph_files.json"
)

human() { du -sh "$1" 2>/dev/null | cut -f1; }

echo "项目根: $ROOT"
[[ $DRY_RUN -eq 1 ]] && echo "模式: DRY-RUN（只预览，不删除）" || echo "模式: 实际删除"
echo "清理前总体积: $(human "$ROOT")"
echo "----------------------------------------"

removed_any=0

remove_path() {
  local p="$1"
  [[ -e "$p" ]] || return 0
  local sz; sz="$(human "$p")"
  if [[ $DRY_RUN -eq 1 ]]; then
    printf "  会删除  %-40s %s\n" "$p" "$sz"
  else
    rm -rf "$p"
    printf "  已删除  %-40s %s\n" "$p" "$sz"
  fi
  removed_any=1
}

# 固定清单
for t in "${TARGETS[@]}"; do
  remove_path "$t"
done

# 递归找出所有 .hologram 运行时缓存目录（可能分布在子目录里）
while IFS= read -r d; do
  remove_path "$d"
done < <(find . -type d -name ".hologram" -prune 2>/dev/null)

echo "----------------------------------------"
if [[ $removed_any -eq 0 ]]; then
  echo "没有可清理的东西，项目已经很干净。"
else
  echo "清理后总体积: $(human "$ROOT")"
  [[ $DRY_RUN -eq 1 ]] && echo "（这是预览。去掉 --dry-run 才会真正删除。）"
fi
