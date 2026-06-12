// ═══════════════════════════════════════════════════════════════
// 文件树面板 — 左侧滑入，项目目录树形浏览
// ═══════════════════════════════════════════════════════════════

import { invoke } from '../bridge';
import { iconSvg } from './icons';
import { FileViewer } from './file-viewer';
import { bus } from './events';
import { askAgent } from './agent-visualizer';
import { dbg } from './debug';

interface DirEntry {
  name: string;
  path: string;
  is_dir: boolean;
  children: DirEntry[] | null;
}

export class FileTreePanel {
  el: HTMLElement;
  private treeEl: HTMLElement;
  private headerEl: HTMLElement;
  private open = false;
  private rootPath = '';
  private _transitioning = false;
  private _closeTimer: ReturnType<typeof setTimeout> | null = null;

  private static instance: FileTreePanel | null = null;
  static get(): FileTreePanel {
    if (!FileTreePanel.instance) FileTreePanel.instance = new FileTreePanel();
    return FileTreePanel.instance;
  }

  private constructor() {
    this.el = document.createElement('div');
    this.el.id = 'file-tree-panel';
    Object.assign(this.el.style, {
      position: 'absolute', left: '0', top: 'var(--toolbar-h)', bottom: 'var(--status-h)', zIndex: '25',
      width: '280px',
      background: 'var(--panel-bg, rgba(4, 10, 20, 0.97))',
      borderRight: '1px solid var(--panel-edge, rgba(48, 60, 80, 0.45))',
      display: 'none', flexDirection: 'column',
      backdropFilter: 'var(--blur, blur(12px))',
      WebkitBackdropFilter: 'var(--blur, blur(12px))',
      transform: 'translateX(-100%)',
      transition: 'transform var(--glide, 0.28s cubic-bezier(0.23, 1, 0.32, 1))',
    });

    // Corner brackets
    const brackets = document.createElement('div');
    brackets.className = 'corner-brackets';
    brackets.innerHTML = '<span class="cb-bottom left"></span><span class="cb-bottom right"></span>';
    this.el.appendChild(brackets);

    // Header
    this.headerEl = document.createElement('div');
    this.headerEl.className = 'ft-header';

    // Path label (root path display)
    const pathLabel = document.createElement('span');
    pathLabel.className = 'ft-path-label';
    Object.assign(pathLabel.style, {
      flex: '1', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
      fontSize: '10px', color: 'var(--text-muted)', fontFamily: 'var(--font-mono, monospace)',
    });
    this.headerEl.appendChild(pathLabel);

    // Refresh button
    const refreshBtn = document.createElement('button');
    refreshBtn.innerHTML = iconSvg('refresh', 12);
    Object.assign(refreshBtn.style, {
      background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)',
      padding: '2px', display: 'flex',
    });
    refreshBtn.title = '刷新';
    refreshBtn.addEventListener('click', () => this.refresh());

    // Close button
    const closeBtn = document.createElement('button');
    closeBtn.innerHTML = iconSvg('close', 12);
    Object.assign(closeBtn.style, {
      background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)',
      padding: '2px', display: 'flex',
    });
    closeBtn.title = '关闭';
    closeBtn.addEventListener('click', () => this.close());

    this.headerEl.appendChild(refreshBtn);
    this.headerEl.appendChild(closeBtn);
    this.el.appendChild(this.headerEl);

    // Tree container
    this.treeEl = document.createElement('div');
    this.treeEl.className = 'ft-tree';
    this.el.appendChild(this.treeEl);

    document.body.appendChild(this.el);
  }

  async load(rootPath: string): Promise<void> {
    this.rootPath = rootPath;
    const pathLabel = this.headerEl.querySelector('.ft-path-label');
    if (pathLabel) pathLabel.textContent = rootPath;
    try {
      const entries: DirEntry[] = await invoke('list_directory', { path: rootPath });
      this.renderTree(entries, this.treeEl, rootPath);
    } catch (e) {
      this.treeEl.innerHTML = `<div style="padding:12px;color:var(--danger)">读取目录失败</div>`;
    }
  }

  refresh(): void {
    if (this.rootPath) this.load(this.rootPath);
  }

  toggle(): void {
    if (this._transitioning) return;
    this.open ? this.close() : this.show();
  }

  show(): void {
    if (this._transitioning) return;
    this._transitioning = true;
    // Cancel any pending close timer
    if (this._closeTimer) { clearTimeout(this._closeTimer); this._closeTimer = null; }
    this.open = true;
    this.el.style.display = 'flex';
    requestAnimationFrame(() => {
      this.el.style.transform = 'translateX(0)';
    });
    // Unlock after transition completes
    setTimeout(() => { this._transitioning = false; }, 300);
    bus.emit('panel:toggle');
  }

  close(): void {
    if (this._transitioning) return;
    this._transitioning = true;
    this.open = false;
    this.el.style.transform = 'translateX(-100%)';
    this._closeTimer = setTimeout(() => {
      if (!this.open) this.el.style.display = 'none';
      this._transitioning = false;
      this._closeTimer = null;
    }, 300);
    bus.emit('panel:toggle');
  }

  isOpen(): boolean { return this.open; }

  /** Highlight and scroll to a file path in the tree. Used by graph→tree reverse linking. */
  highlightPath(filePath: string): void {
    const normalized = filePath.replace(/\\/g, '/');
    // Find all row elements and look for matching file path
    const rows = this.treeEl.querySelectorAll<HTMLElement>('div[data-file-path]');
    for (const row of rows) {
      const rowPath = (row.dataset['filePath'] || '').replace(/\\/g, '/');
      if (rowPath === normalized || rowPath.endsWith('/' + normalized) || normalized.endsWith('/' + rowPath)) {
        // Expand parent containers
        let parent = row.parentElement;
        while (parent && parent !== this.treeEl) {
          if (parent.style.display === 'none') {
            parent.style.display = 'block';
            // Update parent arrow icon
            const parentRow = parent.previousElementSibling as HTMLElement;
            const arrow = parentRow?.querySelector('.ft-arrow') as HTMLElement;
            if (arrow) arrow.textContent = '▾';
          }
          parent = parent.parentElement;
        }
        // Scroll into view and highlight
        row.scrollIntoView({ block: 'center', behavior: 'smooth' });
        row.style.background = 'rgba(60, 100, 170, 0.45)';
        row.style.borderLeftColor = 'rgba(100, 160, 240, 0.8)';
        setTimeout(() => {
          row.style.background = '';
          row.style.borderLeftColor = 'transparent';
        }, 2000);
        break;
      }
    }
  }

  // ── Tree rendering ──

  private renderTree(entries: DirEntry[], parent: HTMLElement, basePath: string, depth: number = 0): void {
    parent.innerHTML = '';
    for (const entry of entries) {
      const row = this.buildRow(entry, basePath, depth);
      parent.appendChild(row);

      if (entry.children && entry.children.length > 0) {
        const childContainer = document.createElement('div');
        childContainer.style.display = 'none';
        this.renderTree(entry.children, childContainer, basePath, depth + 1);
        parent.appendChild(childContainer);

        row.addEventListener('click', (e) => {
          e.stopPropagation();
          const icon = row.querySelector('.ft-arrow') as HTMLElement;
          const expanded = childContainer.style.display !== 'none';
          if (expanded) {
            childContainer.style.display = 'none';
            if (icon) icon.textContent = '▸';
            dbg('FileTree.collapse', entry.path);
            bus.emit('highlight:clear');
          } else {
            childContainer.style.display = 'block';
            if (icon) icon.textContent = '▾';
            dbg('FileTree.expand', entry.path);
            bus.emit('highlight:folder', entry.path);
          }
        });
      } else if (entry.is_dir) {
        // Empty directory — brief flash feedback, no expand/collapse
        row.addEventListener('click', (e) => {
          e.stopPropagation();
          row.style.background = 'rgba(48, 60, 80, 0.35)';
          setTimeout(() => { row.style.background = ''; }, 300);
        });
      } else {
        row.addEventListener('click', () => {
          FileViewer.get().open(entry.path);
          dbg('FileTree.clickFile', entry.path);
          bus.emit('highlight:file', entry.path);
        });
      }
    }
  }

  private buildRow(entry: DirEntry, basePath: string, depth: number): HTMLElement {
    const row = document.createElement('div');
    row.className = 'ft-row';
    row.style.setProperty('--indent', `${12 + depth * 16}px`);
    row.dataset['filePath'] = entry.path;

    // Arrow / spacer
    const arrow = document.createElement('span');
    arrow.className = 'ft-arrow';
    arrow.textContent = entry.is_dir ? '▸' : '';
    row.appendChild(arrow);

    // Icon
    const icon = document.createElement('span');
    icon.className = 'ft-icon';
    icon.innerHTML = entry.is_dir ? iconSvg('folder-closed', 12) : fileIcon(entry.name);
    row.appendChild(icon);

    // Name
    const name = document.createElement('span');
    name.className = entry.is_dir ? 'ft-name ft-name-dir' : 'ft-name ft-name-file';
    name.textContent = entry.name;
    row.appendChild(name);

    // "Ask Agent" icon — appears on hover for files
    if (!entry.is_dir) {
      const askIcon = document.createElement('span');
      askIcon.innerHTML = iconSvg('agent', 11);
      askIcon.title = '问 Agent 分析这个文件';
      askIcon.className = 'ft-ask-btn';
      askIcon.addEventListener('click', (e) => {
        e.stopPropagation();
        askAgent(`分析文件 "${entry.path}"。它在依赖图中的位置是什么？和其他模块的耦合关系如何？修改它会影响什么？`);
      });
      row.appendChild(askIcon);
    }

    return row;
  }
}

// ── File icon by extension ──

function fileIcon(fileName: string): string {
  const ext = fileName.split('.').pop()?.toLowerCase() || '';
  const map: Record<string, string> = {
    ts: 'code', tsx: 'code', js: 'code', jsx: 'code', mjs: 'code',
    py: 'code', rs: 'code', go: 'code', java: 'code',
    c: 'code', cpp: 'code', h: 'code', cs: 'code', rb: 'code', php: 'code',
    html: 'code', htm: 'code', css: 'code', scss: 'code',
    json: 'file', yaml: 'file', yml: 'file', toml: 'file',
    md: 'file', txt: 'file', log: 'file',
    svg: 'file', png: 'file', jpg: 'file', gif: 'file', ico: 'file',
  };
  return iconSvg(map[ext] || 'file', 12);
}
