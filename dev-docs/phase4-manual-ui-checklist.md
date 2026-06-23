# 第四阶段页面级手工验收

生成日期：2026-06-23

## 用途

当 `dev-docs/evidence/phase4-verify.json.preview_smoke` 因本机环境端口权限、浏览器自动化链路或其他 OS 限制失败时，使用本清单补做页面级 UI 验收。

## 前提

- 已在 `src-ui/` 跑过：
  - `node --import tsx src/risk/test-risk.ts`
  - `npx tsc --noEmit`
  - `npm run build`
- 当前 repo 已存在最新 `dev-docs/evidence/phase4-verify.json`

## 手工步骤

1. 在 `src-ui/` 启动本地 dev server：
   - `npm run dev -- --host 127.0.0.1 --port 4173`
2. 用本机浏览器打开：
   - `http://127.0.0.1:4173/`
3. 打开 `简报` 面板并向下滚动，确认以下页面标记可见：
   - `Review Queue`
   - `门禁决策`
   - `多代理审计`
   - `自修复闭环`
   - `看证据 · 已就绪`
   - `Repair patch applied.`
4. 记录当前页面标题，预期类似：
   - `🔮 风控4 审计N — 全息观测站`
5. 验证结束后关闭本地 dev server，避免留下残留进程。

## 当前已知环境限制

- 脚本内起 `vite preview --host 127.0.0.1 --port 4174` 在当前环境可能返回 `EPERM`。
- `open` / AppleScript / JXA 的系统浏览器打开链路在当前环境不稳定。
- 因此，页面级 UI 证据以“本机浏览器手工验收 + 本线程记录”为当前 fallback 主路径。

## 验收结论模板

- `页面标题`：
- `可见标记`：
- `是否看到 Review Queue`：
- `是否看到门禁决策`：
- `是否看到多代理审计`：
- `是否看到自修复闭环`：
- `是否看到 evidence/repair 文案`：
- `是否关闭本地 dev server`：
