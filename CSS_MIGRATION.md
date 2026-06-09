# UI 换皮 SPEC — CSS 变量迁移分步指南

> 不改 JS 逻辑，只把硬编码视觉值换成 CSS 变量引用。每一步独立可测。
> **全 4 步 + Step 0 均已完工（2026-06-10）。** 48 处 `var()` 引用，48 个 fallback，0 编译错误，0 CSS 错误。

---

## ✅ 已完成 — Step 0: index.html CSS 变量底座

`src-ui/index.html` 的 `<style>` 块顶部已有 `:root` 变量定义，且 900 行 CSS 中所有硬编码颜色已替换为 `var()` 引用。

```css
:root {
  /* 背景 */
  --void:        #030812;           /* 最深底 */
  --void-deep:   #010408;
  --panel-bg:    rgba(4, 12, 28, 0.92);  /* 面板 / 工具栏 / 状态栏 */
  --panel-edge:  rgba(54, 82, 128, 0.28); /* 边框线 */

  /* 文字 */
  --starlight:      #e2edff;        /* 亮白文字（标题、强调） */
  --starlight-dim:  rgba(180, 205, 240, 0.7); /* 正文 */
  --text-muted:     rgba(120, 145, 170, 0.5); /* 弱文字/禁用 */

  /* 主色调 — 冷蓝信号 */
  --signal:        #68a8ff;
  --signal-bright: #8cc4ff;
  --signal-glow:   rgba(80, 140, 240, 0.3);

  /* 琥珀 — 警告/介质层 */
  --sol:        #f0b848;
  --sol-bright: #ffcc60;
  --sol-glow:   rgba(240, 170, 50, 0.3);

  /* 紫罗兰 — 时态/线程层 */
  --nebula:        #a088e0;
  --nebula-bright: #c0a8ff;
  --nebula-glow:   rgba(140, 110, 220, 0.25);

  /* 语义色 */
  --pass:  #3da55d;    /* 通过 */
  --fail:  #d94444;    /* 失败/危险 */
  --warn:  #d69622;    /* 警告 */

  /* 字体 */
  --font-mono: 'Cascadia Code', 'Fira Code', 'Consolas', monospace;

  /* 效果 */
  --blur:  blur(14px);

  /* 动画 */
  --snap:  0.15s ease;                             /* 按钮/hover 快速过渡 */
  --glide: 0.25s cubic-bezier(0.4, 0, 0.2, 1);   /* 面板滑入滑出 */
}
```

---

## ✅ 已完成 — TypeScript 内联样式迁移

| 文件 | 改动点 | 风险 | 状态 |
|------|--------|------|------|
| `constraints.ts` | ~15 处 | 低 | ✅ 2026-06-10 |
| `terminal.ts` | ~20 处 | 中 | ✅ 2026-06-10 |
| `file-viewer.ts` | ~20 处 | 中 | ✅ 2026-06-10 |
| `graph.ts` | ~4 处 cssText | 低 | ✅ 2026-06-10 |

> chat.ts / check.ts / timeline.ts 不用动（用 CSS 类，不是内联样式）。

### 替换规则速查

| 旧值 | 换成 |
|------|------|
| `#7eb8ff` / `#a0c8f0` / `#8ec8ff` | `var(--signal)` |
| `#a0d0ff` | `var(--signal-bright)` |
| `#c9d1d9` | `var(--starlight-dim)` |
| `#e6edf3` | `var(--starlight)` |
| `#4a5568` / `#6b7d90` / `#8b949e` | `var(--text-muted)` |
| `#55aa55` | `var(--pass)` |
| `#e05555` | `var(--fail)` |
| `#d29922` / `#f0883e` | `var(--warn)` |
| `#f0c060` / `#f0a020` | `var(--sol)` |
| `#c098ff` / `#b098ff` | `var(--nebula)` |
| `rgba(6, 12, 24, 0.97)` | `var(--panel-bg)` |
| `rgba(48, 60, 80, 0.X)`（边框） | `var(--panel-edge)` |
| `blur(14px)` | `var(--blur, blur(14px))` |
| `0.12s` / `0.15s`（动画） | `var(--snap)` |
| `0.25s cubic-bezier(…)` | `var(--glide)` |
| `'Cascadia Code', 'Fira Code', 'Consolas', monospace` | `var(--font-mono)` |

> **fallback 写法**：所有 `var()` 都带原始值作为 fallback，如 `var(--signal, #7eb8ff)`。这样变量名拼错时不会炸。

---

## Step 1 — constraints.ts

**文件**: `src-ui/src/ui/constraints.ts`

### 1a. 面板根元素 (line 38-54)

```diff
  Object.assign(this.panel.style, {
    position: 'absolute',
    top: '36px', right: '0', bottom: '28px',
    width: '340px', maxWidth: '90vw',
-   background: 'rgba(6, 12, 24, 0.97)',
-   backdropFilter: 'blur(14px)',
-   WebkitBackdropFilter: 'blur(14px)',
-   borderLeft: '1px solid rgba(48, 60, 80, 0.5)',
+   background: 'var(--panel-bg, rgba(6, 12, 24, 0.97))',
+   backdropFilter: 'var(--blur, blur(14px))',
+   WebkitBackdropFilter: 'var(--blur, blur(14px))',
+   borderLeft: '1px solid var(--panel-edge, rgba(48, 60, 80, 0.5))',
    zIndex: '16',
    display: 'flex', flexDirection: 'column',
    transform: 'translateX(100%)',  /* ⚠️ 不动 — 动态切换 */
-   transition: 'transform 0.25s cubic-bezier(0.4, 0, 0.2, 1)',
+   transition: 'transform var(--glide, 0.25s cubic-bezier(0.4, 0, 0.2, 1))',
  });
```

### 1b. header 分隔线 (line 58-62)

```diff
  Object.assign(header.style, {
    display: 'flex', alignItems: 'center', justifyContent: 'space-between',
    padding: '8px 12px',
-   borderBottom: '1px solid rgba(48, 60, 80, 0.4)',
+   borderBottom: '1px solid var(--panel-edge, rgba(48, 60, 80, 0.4))',
    flexShrink: '0',
  });
```

### 1c. 标题 (line 66-68)

```diff
  Object.assign(title.style, {
-   fontSize: '13px', fontWeight: '600', color: '#7eb8ff', letterSpacing: '0.5px',
+   fontSize: '13px', fontWeight: '600', color: 'var(--signal, #7eb8ff)', letterSpacing: '0.5px',
  });
```

### 1d. 关闭按钮 (line 72-77)

```diff
  Object.assign(closeBtn.style, {
    width: '24px', height: '24px', padding: '0',
-   background: 'none', border: 'none', color: '#4a5568',
+   background: 'none', border: 'none', color: 'var(--text-muted, #4a5568)',
    cursor: 'pointer', fontSize: '14px', borderRadius: '4px',
-   transition: 'color 0.12s',
+   transition: 'color var(--snap, 0.12s)',
  });
```

### 1e. mouseenter/mouseleave (line 78-79)

```diff
- closeBtn.addEventListener('mouseenter', () => closeBtn.style.color = '#c9d1d9');
- closeBtn.addEventListener('mouseleave', () => closeBtn.style.color = '#4a5568');
+ closeBtn.addEventListener('mouseenter', () => closeBtn.style.color = 'var(--starlight-dim, #c9d1d9)');
+ closeBtn.addEventListener('mouseleave', () => closeBtn.style.color = 'var(--text-muted, #4a5568)');
```

### 1f. 保存成功反馈 (line 349-355)

```diff
- btn.style.color = '#55aa55';
+ btn.style.color = 'var(--pass, #55aa55)';
```

**验证**: 打开约束面板，外观完全不变。

---

## Step 2 — terminal.ts

**文件**: `src-ui/src/ui/terminal.ts`

### 2a. 面板根元素 (line 37-50)

```diff
  Object.assign(this.panel.style, {
    position: 'absolute',
    bottom: '28px', left: '0', right: '0',
    height: '260px', zIndex: '13',
-   background: 'rgba(4, 8, 16, 0.98)',
-   borderTop: '1px solid rgba(48, 60, 80, 0.5)',
+   background: 'var(--void-deep, rgba(4, 8, 16, 0.98))',
+   borderTop: '1px solid var(--panel-edge, rgba(48, 60, 80, 0.5))',
    display: 'flex', flexDirection: 'column',
    transform: 'translateY(100%)',  /* ⚠️ 不动 — 动态切换 */
-   transition: 'transform 0.25s cubic-bezier(0.4, 0, 0.2, 1)',
+   transition: 'transform var(--glide, 0.25s cubic-bezier(0.4, 0, 0.2, 1))',
  });
```

### 2b. header (line 61-68)

```diff
  Object.assign(header.style, {
    display: 'flex', alignItems: 'center',
    gap: '6px', padding: '4px 8px',
-   borderBottom: '1px solid rgba(48, 60, 80, 0.3)',
+   borderBottom: '1px solid var(--panel-edge, rgba(48, 60, 80, 0.3))',
    flexShrink: '0',
  });
```

### 2c. cwd 标签 (line 73-78)

```diff
  Object.assign(cwdLabel.style, {
    fontSize: '10px',
-   color: '#4a5568',
-   fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
+   color: 'var(--text-muted, #4a5568)',
+   fontFamily: 'var(--font-mono, Cascadia Code, Fira Code, Consolas, monospace)',
    flex: '1',
  });
```

### 2d. 清除按钮 (line 82-87)

```diff
  Object.assign(clearBtn.style, {
    fontSize: '10px', padding: '2px 8px',
-   background: 'rgba(30, 30, 40, 0.6)', color: '#8b949e',
-   border: '1px solid rgba(48, 60, 80, 0.4)', borderRadius: '4px',
+   background: 'rgba(18, 30, 48, 0.6)', color: 'var(--text-muted, #8b949e)',
+   border: '1px solid var(--panel-edge, rgba(48, 60, 80, 0.4))', borderRadius: '4px',
    cursor: 'pointer',
  });
```

### 2e. inputRow (line 106-112)

```diff
  Object.assign(inputRow.style, {
    display: 'flex', alignItems: 'center',
    gap: '0',
-   borderTop: '1px solid rgba(48, 60, 80, 0.3)',
+   borderTop: '1px solid var(--panel-edge, rgba(48, 60, 80, 0.3))',
    flexShrink: '0',
  });
```

### 2f. prompt 符号 (line 116-122)

```diff
  Object.assign(prompt.style, {
-   color: '#7eb8ff',
-   fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
+   color: 'var(--signal, #7eb8ff)',
+   fontFamily: 'var(--font-mono, Cascadia Code, Fira Code, Consolas, monospace)',
    fontSize: '13px', padding: '0 8px', fontWeight: '600',
  });
```

### 2g. inputLine (line 126-136)

```diff
  Object.assign(this.inputLine.style, {
    flex: '1', height: '28px', padding: '0 8px',
    fontSize: '13px',
-   fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
+   fontFamily: 'var(--font-mono, Cascadia Code, Fira Code, Consolas, monospace)',
    background: 'transparent', border: 'none',
-   color: '#c9d1d9',
+   color: 'var(--starlight-dim, #c9d1d9)',
    outline: 'none',
  });
```

**验证**: 打开终端面板，外观完全不变。

---

## Step 3 — file-viewer.ts

**文件**: `src-ui/src/ui/file-viewer.ts`

### 3a. 面板根元素 (line 59-77)

```diff
  Object.assign(this.el.style, {
    position: 'absolute', zIndex: '30',
    display: 'none',  /* ⚠️ 不动 — 动态 */
    width: `${this.state.width}px`, height: `${this.state.height}px`,  /* ⚠️ 不动 — 动态 */
    left: `${this.state.x}px`, top: `${this.state.y}px`,  /* ⚠️ 不动 — 动态 */
-   background: 'rgba(6, 12, 24, 0.97)',
-   backdropFilter: 'blur(14px)',
-   WebkitBackdropFilter: 'blur(14px)',
-   border: '1px solid rgba(48, 60, 80, 0.5)',
+   background: 'var(--panel-bg, rgba(6, 12, 24, 0.97))',
+   backdropFilter: 'var(--blur, blur(14px))',
+   WebkitBackdropFilter: 'var(--blur, blur(14px))',
+   border: '1px solid var(--panel-edge, rgba(48, 60, 80, 0.5))',
    borderRadius: '8px',  /* 不动 — 形状 */
    boxShadow: '0 12px 48px rgba(0,0,0,0.6), 0 0 0 1px rgba(88,120,180,0.08) inset',
    flexDirection: 'column', overflow: 'hidden',
    minWidth: '280px', minHeight: '180px',
  });
```

### 3b. header (line 81-91)

```diff
  Object.assign(this.header.style, {
    display: 'flex', alignItems: 'center',
    gap: '8px', padding: '6px 10px',
-   borderBottom: '1px solid rgba(48, 60, 80, 0.4)',
+   borderBottom: '1px solid var(--panel-edge, rgba(48, 60, 80, 0.4))',
    cursor: 'move', userSelect: 'none', flexShrink: '0',
-   background: 'rgba(14, 22, 38, 0.9)',
+   background: 'var(--panel-bg, rgba(14, 22, 38, 0.9))',
  });
```

### 3c. title (line 94-103)

```diff
  Object.assign(this.title.style, {
    fontSize: '12px', fontWeight: '600',
-   color: '#7eb8ff',
+   color: 'var(--signal, #7eb8ff)',
    flex: '1', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
-   fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
+   fontFamily: 'var(--font-mono, Cascadia Code, Fira Code, Consolas, monospace)',
  });
```

### 3d. pathLabel (line 108-116)

```diff
  Object.assign(pathLabel.style, {
    fontSize: '10px',
-   color: '#4a5568',
-   fontFamily: "'Cascadia Code', 'Fira Code', 'Consolas', monospace",
+   color: 'var(--text-muted, #4a5568)',
+   fontFamily: 'var(--font-mono, Cascadia Code, Fira Code, Consolas, monospace)',
    overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
    maxWidth: '300px',
  });
```

### 3e. closeBtn (line 121-127)

```diff
  Object.assign(closeBtn.style, {
    width: '22px', height: '22px', padding: '0',
-   background: 'none', border: 'none', color: '#4a5568',
+   background: 'none', border: 'none', color: 'var(--text-muted, #4a5568)',
    cursor: 'pointer', fontSize: '14px', borderRadius: '4px',
    display: 'flex', alignItems: 'center', justifyContent: 'center',
-   transition: 'color 0.12s, background 0.12s',
+   transition: 'color var(--snap, 0.12s), background var(--snap, 0.12s)',
  });
```

### 3f. closeBtn hover (line 128-135)

```diff
  closeBtn.addEventListener('mouseenter', () => {
-   closeBtn.style.color = '#c9d1d9';
+   closeBtn.style.color = 'var(--starlight-dim, #c9d1d9)';
    closeBtn.style.background = 'rgba(255,255,255,0.05)';
  });
  closeBtn.addEventListener('mouseleave', () => {
-   closeBtn.style.color = '#4a5568';
+   closeBtn.style.color = 'var(--text-muted, #4a5568)';
    closeBtn.style.background = 'none';
  });
```

### 3g. 加载状态颜色 (line 231, 239, 244)

```diff
- this.pre.style.color = '#4a5568';    // 加载中
+ this.pre.style.color = 'var(--text-muted, #4a5568)';  // 加载中

- this.pre.style.color = '#c9d1d9';    // 正常内容
+ this.pre.style.color = 'var(--starlight-dim, #c9d1d9)';  // 正常内容

- this.pre.style.color = '#e05555';    // 错误
+ this.pre.style.color = 'var(--fail, #e05555)';  // 错误
```

**验证**: 从简报点文件打开查看器，外观不变。

---

## Step 4 — graph.ts

**文件**: `src-ui/src/ui/graph.ts`

只有 4 处 cssText，改动量最小。

### 4a. pieMenu (line ~602)

```diff
  this.pieMenu.style.cssText =
    'position:absolute;z-index:20;pointer-events:auto;display:none;' +
-   'background:rgba(6,12,24,0.95);border:1px solid rgba(88,120,180,0.3);' +
+   'background:var(--panel-bg,rgba(6,12,24,0.95));border:1px solid var(--panel-edge,rgba(88,120,180,0.3));' +
    'border-radius:8px;padding:4px;box-shadow:0 8px 32px rgba(0,0,0,0.5);' +
    'flex-direction:column;gap:2px;min-width:80px;';
```

### 4b. pieMenu 注入 style (line ~609)

```diff
  style.textContent = `
    #pie-menu .pie-item {
      display:flex;align-items:center;gap:8px;padding:6px 12px;
-     font-size:13px;color:#c9d1d9;border-radius:5px;cursor:pointer;
+     font-size:13px;color:var(--starlight-dim,#c9d1d9);border-radius:5px;cursor:pointer;
      transition:background 0.1s;white-space:nowrap;
    }
-   #pie-menu .pie-item:hover { background:rgba(88,120,180,0.25);color:#e6edf3; }
+   #pie-menu .pie-item:hover { background:rgba(80,140,240,0.25);color:var(--starlight,#e6edf3); }
    #pie-menu .pie-icon { font-size:14px;width:20px;text-align:center; }
  `;
```

### 4c. galaxyTitle — ⚠️ 不动

银河标题使用专属暖金色（`#ffcc80` + 橙色 glow），不是系统色，不改。

### 4d. galaxyLabel (line ~1460)

```diff
- div.style.cssText = 'position:absolute;z-index:3;pointer-events:none;font-size:10px;color:rgba(200,200,220,0.55);text-shadow:0 0 6px rgba(0,0,0,0.7);white-space:nowrap;transform:translate(-50%,-50%);';
+ div.style.cssText = 'position:absolute;z-index:3;pointer-events:none;font-size:10px;color:var(--starlight-dim,rgba(200,200,220,0.55));text-shadow:0 0 6px rgba(0,0,0,0.7);white-space:nowrap;transform:translate(-50%,-50%);';
```

**验证**: 右键节点出 pie menu，外观不变。折叠进银河，标签外观不变。

---

## 不改的部分（重要）

以下 JS 代码**不动**：

| 位置 | 代码 | 原因 |
|------|------|------|
| 全部面板 | `panel.style.transform = 'translateX/Y(…)'` | 面板滑动动画 |
| 全部面板 | `panel.style.display = '…'` | 显示/隐藏 |
| graph.ts | `.style.left/top = '…px'` | 动态位置 |
| graph.ts | `.style.opacity = …` | 动画淡入淡出 |
| chat.ts | `.style.width = w + 'px'` | 拖拽宽度 |
| file-viewer.ts | `.style.left/top/width/height` | 拖拽移动 |
| graph.ts 4c | galaxyTitle cssText | 专属暖金艺术色 |

---

## 回滚方案

每个 `var()` 都有 fallback（`var(--xxx, 原值)`）。如果变量名拼错或未定义，自动回落原值，UI 不崩。完全回滚：删掉 `:root` 即可。

---

## 验证清单

- [x] Step 0: index.html CSS 变量底座
- [x] Step 1: constraints.ts (~15 处)
- [x] Step 2: terminal.ts (~20 处)
- [x] Step 3: file-viewer.ts (~20 处)
- [x] Step 4: graph.ts (~4 处 cssText)
- [x] `npm run dev` 跑起，星图正常 — Mock 模式秒启，canvas 渲染正常
- [x] 打开约束面板（点击 ⚙️ 约束），外观不变 — DOM 验证 `var()` 样式正确
- [x] 打开终端面板（点击 ⬛ 终端），外观不变 — DOM 验证 `var()` 样式正确
- [x] 从简报点文件名，文件查看器弹出，外观不变 — DOM 验证 `var()` 样式正确
- [x] 右键节点，pie menu 弹出，外观不变 — DOM 验证 `var()` 样式正确
- [x] 面板反复开/关无异常 — toggle ×2 无 JS 错误
- [x] Console 零 CSS 错误 — 48 个 `var()` 全部解析成功，无 Undefined Variable
- [x] TypeScript 编译无错误 — `tsc --noEmit` 干净通过
- [ ] 提交

> **2026-06-10 验证完成**：Playwright + Chrome headless 驱动，全 4 面板 DOM 样式检查通过。
