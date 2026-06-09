// HoloGram 主入口
// 三模式星图：minimal / standard / full — 独立实例，切换即重建

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { StarGraph, VisualMode } from './ui/graph';

// ── UI ──
const welcome = document.getElementById('welcome')!;
const graphEl = document.getElementById('graph')!;
const statusText = document.getElementById('status-text')!;
const tbPath = document.getElementById('tb-path')!;
const btnOpen = document.getElementById('btn-open') as HTMLButtonElement;
const btnWelcomeOpen = document.getElementById('btn-welcome-open') as HTMLButtonElement;
const searchInput = document.getElementById('search-input') as HTMLInputElement;
const searchBtn = document.getElementById('search-btn') as HTMLButtonElement;
const btnFold = document.getElementById('btn-fold') as HTMLButtonElement;

// ── State ──
let currentPath: string | null = null;
let currentGraphData: any = null;
let currentMode: VisualMode = 'standard';
let starGraph: StarGraph = new StarGraph(graphEl, currentMode);

// ── Mode switch ──

function setupModeSwitch(): void {
  const buttons = document.querySelectorAll<HTMLButtonElement>('#mode-switch .mode-btn');
  buttons.forEach(btn => {
    btn.addEventListener('click', () => {
      const mode = btn.dataset['mode'] as VisualMode;
      if (mode === currentMode) return;
      currentMode = mode;
      buttons.forEach(b => b.classList.remove('active'));
      btn.classList.add('active');

      // Destroy old, create new with same data
      starGraph.destroy();
      starGraph = new StarGraph(graphEl, currentMode);
      if (currentGraphData) starGraph.render(currentGraphData);

      // Re-wire search (new instance)
      if (searchInput.value.trim()) {
        setTimeout(() => starGraph.focusNode(searchInput.value.trim()), 300);
      }
    });
  });
}

// ── Folder picker ──

async function pickFolder(): Promise<string | null> {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const result = await open({ directory: true, multiple: false, title: '选择工作区目录' });
    return result as string | null;
  } catch {
    return prompt('输入项目路径:');
  }
}

// ── Open & Analyze ──

async function openProject(path?: string): Promise<void> {
  const folder = path || (await pickFolder());
  if (!folder) return;

  if (currentPath) { try { await invoke('stop_watching'); } catch { /* ignore */ } }

  setLoading(true, folder);
  try {
    const json = await invoke<string>('analyze_and_load', { path: folder });
    const graph = JSON.parse(json);
    currentGraphData = graph;
    starGraph.render(graph);
    showGraphView(folder);
    try { await invoke('start_watching', { path: folder }); } catch { /* ignore */ }
  } catch (err: any) {
    statusText.textContent = `分析失败: ${err}`; setLoading(false); throw err;
  }
}

function setLoading(active: boolean, folder?: string): void {
  btnOpen.disabled = active;
  btnOpen.textContent = active ? '⏳ 分析中...' : '📂 打开文件夹';
  if (active) statusText.textContent = `正在分析 ${folder || ''}...`;
}

function showGraphView(path: string): void {
  currentPath = path;
  welcome.classList.add('hidden'); graphEl.classList.remove('hidden');
  btnOpen.disabled = false; btnOpen.textContent = '📂 打开文件夹';
  tbPath.textContent = path;
}

// ── Search ──

function doSearch(): void {
  const query = searchInput.value.trim(); if (!query) return;
  const found = starGraph.focusNode(query);
  if (!found) { statusText.textContent = `未找到 "${query}"`; setTimeout(() => { if (statusText.textContent === `未找到 "${query}"`) statusText.textContent = '就绪'; }, 2000); }
}

// ── Init ──

async function init(): Promise<void> {
  setupModeSwitch();

  const open = () => openProject();
  btnOpen.addEventListener('click', open);
  btnWelcomeOpen.addEventListener('click', open);

  searchBtn.addEventListener('click', doSearch);
  searchInput.addEventListener('keydown', (e) => { if (e.key === 'Enter') doSearch(); });

  // Fold toggle
  btnFold.addEventListener('click', () => { starGraph.toggleFold(); updateFoldBtn(); });
  window.addEventListener('keydown', (e) => {
    if ((e.key === 'f' || e.key === 'F') && document.activeElement?.tagName !== 'INPUT') {
      starGraph.toggleFold(); updateFoldBtn();
    }
    if (e.key === 'Escape' && starGraph.isInsideGalaxy) {
      starGraph.exitGalaxy();
    }
  });
  function updateFoldBtn(): void {
    btnFold.textContent = starGraph.isFolded ? '🌀 展开' : '🌀 折叠';
  }

  // Live updates from file watcher
  listen<string>('graph-updated', (event) => {
    try {
      const graph = JSON.parse(event.payload);
      const nodeCount = Array.isArray(graph.nodes) ? graph.nodes.length : Object.keys(graph.nodes || {}).length;
      if (nodeCount > 0) {
        currentGraphData = graph;
        starGraph.render(graph);
        statusText.textContent = `🔄 已更新 (${nodeCount} 节点)`;
        setTimeout(() => { if (statusText.textContent?.startsWith('🔄')) statusText.textContent = '就绪'; }, 3000);
      }
    } catch { /* ignore */ }
  });

  // Try cached graph
  try {
    const json = await invoke<string>('load_graph_json');
    const graph = JSON.parse(json);
    const nodeCount = Array.isArray(graph.nodes) ? graph.nodes.length : Object.keys(graph.nodes || {}).length;
    if (nodeCount > 0) {
      const root = graph.meta?.source_root || '';
      currentGraphData = graph;
      starGraph.render(graph);
      showGraphView(root);
      statusText.textContent = '已加载缓存图谱';
      if (root) { try { await invoke('start_watching', { path: root }); } catch { /* ignore */ } }
      return;
    }
  } catch { /* no cache */ }

  welcome.classList.remove('hidden'); graphEl.classList.add('hidden');
}

init();
