// ═══════════════════════════════════════════════════════════════
// 深空全息星图 · Deep Space Holographic Star Chart
// 三模式：minimal | standard | full
// ═══════════════════════════════════════════════════════════════

import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';

// ── Types ────────────────────────────────────────────────────

interface GraphNode {
  id: string; name: string; type?: string; kind?: string;
  location?: string; properties?: Record<string, unknown>;
}
interface GraphEdge {
  id: string; source: string; target: string; type?: string;
  properties?: Record<string, unknown>;
}
interface GraphJSON {
  nodes: GraphNode[] | Record<string, GraphNode>;
  edges: GraphEdge[] | Record<string, GraphEdge>;
  meta?: Record<string, unknown>;
}

interface EdgeData { s: number; t: number; couplingDepth: number; edgeType: string; direction: string; }

export type VisualMode = 'minimal' | 'standard' | 'full';

// ── Color Palette ────────────────────────────────────────────

const NODE_COLORS: Record<string, number> = {
  symbol: 0x7eb8ff, SYMBOL: 0x7eb8ff,
  function: 0x8ec8ff, method: 0x8ec8ff,
  class: 0x6aadff, module: 0x7eb8ff,
  interface: 0x7eb8ff, variable: 0x94d0ff, constant: 0x94d0ff,
  medium: 0xf0c060, MEDIUM: 0xf0c060,
  file: 0xf0c060, database: 0xe8b84c, cache: 0xe8b84c, queue: 0xe8b84c,
  temporal: 0xc098ff, TEMPORAL: 0xc098ff,
  thread: 0xc098ff, timer: 0xb888f8, trigger: 0xb888f8,
};
const GLOW_COLORS: Record<string, number> = {
  symbol: 0x4488cc, SYMBOL: 0x4488cc,
  function: 0x4499dd, method: 0x4499dd,
  class: 0x3377bb, module: 0x4488cc,
  interface: 0x4488cc, variable: 0x55aadd, constant: 0x55aadd,
  medium: 0xcc8800, MEDIUM: 0xcc8800,
  file: 0xcc8800, database: 0xbb7700, cache: 0xbb7700, queue: 0xbb7700,
  temporal: 0x8855cc, TEMPORAL: 0x8855cc,
  thread: 0x8855cc, timer: 0x7744bb, trigger: 0x7744bb,
};

function edgeColorByType(edgeType: string, direction: string): THREE.Color {
  if (edgeType === 'data' || edgeType === 'DATA') {
    return direction === 'write' ? new THREE.Color(0xcc5555) : new THREE.Color(0x55aa55);
  }
  if (edgeType === 'temporal' || edgeType === 'TEMPORAL') {
    return new THREE.Color(0xdd9944);
  }
  return new THREE.Color(0x5599cc);
}
function edgeOpacityByDepth(depth: number): number {
  switch (depth) { case 1: return 0.08; case 2: return 0.10; case 3: return 0.14; case 4: return 0.20; default: return 0.09; }
}

const BG_COLOR = 0x030812;
const TYPE_LABELS: Record<string, string> = {
  symbol: 'SYM', function: 'FN', method: 'MTH', class: 'CLS',
  module: 'MOD', variable: 'VAR', constant: 'CST', interface: 'IFC',
  medium: 'MED', file: 'FILE', database: 'DB', cache: 'CACHE', queue: 'Q',
  temporal: 'TMP', thread: 'THR', timer: 'TIM', trigger: 'TRG',
};

// ── Glow Textures ─────────────────────────────────────────────

function createGlowTexture(): THREE.Texture {
  const size = 128, c = document.createElement('canvas');
  c.width = c.height = size; const ctx = c.getContext('2d')!;
  const h = size / 2;
  const g = ctx.createRadialGradient(h, h, 0, h, h, h);
  g.addColorStop(0, 'rgba(255,255,255,1)'); g.addColorStop(0.02, 'rgba(255,255,255,0.95)');
  g.addColorStop(0.08, 'rgba(255,255,255,0.55)'); g.addColorStop(0.2, 'rgba(255,255,255,0.18)');
  g.addColorStop(0.45, 'rgba(255,255,255,0.03)'); g.addColorStop(0.7, 'rgba(255,255,255,0.004)');
  g.addColorStop(1, 'rgba(255,255,255,0)');
  ctx.fillStyle = g; ctx.fillRect(0, 0, size, size);
  return new THREE.CanvasTexture(c);
}

function createSpikeTexture(): THREE.Texture {
  const size = 256, c = document.createElement('canvas');
  c.width = c.height = size; const ctx = c.getContext('2d')!;
  const cx = size / 2, cy = size / 2;
  const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, size / 2);
  g.addColorStop(0, 'rgba(255,255,255,1)'); g.addColorStop(0.03, 'rgba(255,255,255,0.9)');
  g.addColorStop(0.1, 'rgba(255,255,255,0.5)'); g.addColorStop(0.25, 'rgba(255,255,255,0.15)');
  g.addColorStop(0.5, 'rgba(255,255,255,0.02)'); g.addColorStop(1, 'rgba(255,255,255,0)');
  ctx.fillStyle = g; ctx.fillRect(0, 0, size, size);
  for (let i = 0; i < 6; i++) {
    const a = (i * Math.PI) / 3, sx = Math.cos(a), sy = Math.sin(a);
    const w = ctx.createLinearGradient(cx, cy, cx + sx * size * 0.45, cy + sy * size * 0.45);
    w.addColorStop(0, 'rgba(255,255,255,0.7)'); w.addColorStop(0.15, 'rgba(255,240,220,0.4)');
    w.addColorStop(0.5, 'rgba(255,200,150,0.08)'); w.addColorStop(1, 'transparent');
    ctx.fillStyle = w; ctx.beginPath();
    ctx.moveTo(cx + sx * 3, cy + sy * 3); ctx.lineTo(cx + sx * size * 0.48, cy + sy * size * 0.48);
    ctx.lineTo(cx - sy * 1.5, cy + sx * 1.5); ctx.lineTo(cx + sy * 1.5, cy - sx * 1.5); ctx.fill();
    const cg = ctx.createLinearGradient(cx, cy, cx - sx * size * 0.35, cy - sy * size * 0.35);
    cg.addColorStop(0, 'rgba(255,255,255,0.5)'); cg.addColorStop(0.15, 'rgba(200,220,255,0.3)');
    cg.addColorStop(0.5, 'rgba(150,180,255,0.05)'); cg.addColorStop(1, 'transparent');
    ctx.fillStyle = cg; ctx.beginPath();
    ctx.moveTo(cx - sx * 3, cy - sy * 3); ctx.lineTo(cx - sx * size * 0.38, cy - sy * size * 0.38);
    ctx.lineTo(cx + sy * 1.2, cy - sx * 1.2); ctx.lineTo(cx - sy * 1.2, cy + sx * 1.2); ctx.fill();
  }
  return new THREE.CanvasTexture(c);
}

// ── Fibonacci Sphere ─────────────────────────────────────────

function fibonacciSphere(n: number, radius: number): Float32Array {
  const pos = new Float32Array(n * 3), phi = Math.PI * (3 - Math.sqrt(5));
  for (let i = 0; i < n; i++) {
    const y = 1 - (i / (n - 1 || 1)) * 2, r = Math.sqrt(1 - y * y), theta = phi * i;
    pos[i * 3] = Math.cos(theta) * r * radius;
    pos[i * 3 + 1] = y * radius;
    pos[i * 3 + 2] = Math.sin(theta) * r * radius;
  }
  return pos;
}

// ── 3D Force-Directed Layout ─────────────────────────────────

function layout3D(n: number, edgePairs: [number, number][]): Float32Array {
  if (n === 0) return new Float32Array(0);
  const shellRadius = Math.cbrt(n) * 14, pos = fibonacciSphere(n, shellRadius);
  const vel = new Float32Array(n * 3);
  const rep = 600, att = 0.018, damp = 0.72, sp = 0.006;
  const maxIter = Math.min(70, 20 + Math.floor(n / 4));
  for (let iter = 0; iter < maxIter; iter++) {
    for (let i = 0; i < n; i++) {
      for (let j = i + 1; j < n; j++) {
        const dx = pos[i * 3] - pos[j * 3], dy = pos[i * 3 + 1] - pos[j * 3 + 1], dz = pos[i * 3 + 2] - pos[j * 3 + 2];
        const dist = Math.max(0.3, Math.sqrt(dx * dx + dy * dy + dz * dz));
        const f = rep / (dist * dist + 1);
        vel[i * 3] += (dx / dist) * f; vel[i * 3 + 1] += (dy / dist) * f; vel[i * 3 + 2] += (dz / dist) * f;
        vel[j * 3] -= (dx / dist) * f; vel[j * 3 + 1] -= (dy / dist) * f; vel[j * 3 + 2] -= (dz / dist) * f;
      }
    }
    for (const [s, t] of edgePairs) {
      const dx = pos[s * 3] - pos[t * 3], dy = pos[s * 3 + 1] - pos[t * 3 + 1], dz = pos[s * 3 + 2] - pos[t * 3 + 2];
      const dist = Math.max(0.3, Math.sqrt(dx * dx + dy * dy + dz * dz)), f = dist * att;
      vel[s * 3] -= (dx / dist) * f; vel[s * 3 + 1] -= (dy / dist) * f; vel[s * 3 + 2] -= (dz / dist) * f;
      vel[t * 3] += (dx / dist) * f; vel[t * 3 + 1] += (dy / dist) * f; vel[t * 3 + 2] += (dz / dist) * f;
    }
    for (let i = 0; i < n; i++) { vel[i * 3] -= pos[i * 3] * 0.0004; vel[i * 3 + 1] -= pos[i * 3 + 1] * 0.0004; vel[i * 3 + 2] -= pos[i * 3 + 2] * 0.0004; }
    for (let i = 0; i < n * 3; i++) { vel[i] *= damp; pos[i] += vel[i]; }
    for (let i = 0; i < n; i++) {
      const dx = pos[i * 3], dy = pos[i * 3 + 1], dz = pos[i * 3 + 2], dist = Math.sqrt(dx * dx + dy * dy + dz * dz);
      if (dist > 1) { const drift = (dist - shellRadius) * sp; pos[i * 3] -= (dx / dist) * drift; pos[i * 3 + 1] -= (dy / dist) * drift; pos[i * 3 + 2] -= (dz / dist) * drift; }
    }
  }
  return pos;
}

// ═══════════════════════════════════════════════════════════════
// StarGraph — 深空星图 (mode-aware from construction)
// ═══════════════════════════════════════════════════════════════

export class StarGraph {
  private mode: VisualMode;
  private scene: THREE.Scene;
  private camera: THREE.PerspectiveCamera;
  private renderer: THREE.WebGLRenderer;
  private controls: OrbitControls;
  private container: HTMLElement;
  private animId = 0;
  private starfield!: THREE.Points;
  private nodeGroup = new THREE.Group();
  private edgeGroup = new THREE.Group();
  private highlightEdgeGroup = new THREE.Group();
  private sphereGeo: THREE.SphereGeometry;
  private glowTex: THREE.Texture;

  // Graph data
  private graphNodes: GraphNode[] = [];
  private nodePositions: Float32Array<ArrayBufferLike> = new Float32Array(0);
  private deg: number[] = [];
  private edgeDataList: EdgeData[] = [];
  private maxDeg = 1;
  private neighborMap: number[][] = [];
  private edgeIndexOf: number[][] = [];
  private nodeLabelIdx: number[] = [];
  private l34Count: number[] = [];

  // Meshes
  private nodeCores: THREE.Mesh[] = [];
  private nodeGlows: THREE.Sprite[] = [];
  private nodeGlowColors: number[] = [];
  private edgeLineGroups: THREE.LineSegments[] = [];

  // Full-FX extras
  private twinklePhases: number[] = [];
  private twinkleSpeeds: number[] = [];
  private edgeParticles!: THREE.Points;
  private edgeParticleData: { edgeIdx: number; t: number; speed: number; dir: number }[] = [];
  private nodeGlows2: THREE.Sprite[] = []; // second glow layer (full mode)

  // Hover
  private raycaster: THREE.Raycaster;
  private mouse = new THREE.Vector2(-999, -999);
  private hoveredIdx = -1;
  private hoverScale = 0;
  private targetHoverScale = 0;

  // Labels
  private labelsContainer!: HTMLDivElement;
  private labelDivs: HTMLDivElement[] = [];

  // Tooltip & Detail card
  private tooltipEl!: HTMLDivElement;
  private detailCard!: HTMLDivElement;
  private selectedIdx = -1;

  // Focus
  private focusTarget = new THREE.Vector3();
  private focusActive = false;
  private focusProgress = 0;
  private focusNodeIdx = -1;
  private focusStartCam = new THREE.Vector3();
  private focusStartLook = new THREE.Vector3();
  private focusFlash = 0;

  // Blast
  private blastMode = false;
  private blastSource = -1;
  private blastDistances: number[] = [];
  private blastMaxDist = 8;

  // Animation
  private pulseTime = 0;
  private tmpVec3 = new THREE.Vector3();

  constructor(container: HTMLElement, mode: VisualMode = 'standard') {
    this.container = container;
    this.mode = mode;

    const bg = mode === 'minimal' ? 0x000005 : BG_COLOR;

    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color(bg);
    if (mode !== 'minimal') this.scene.fog = new THREE.FogExp2(bg, 0.000012);

    this.camera = new THREE.PerspectiveCamera(48, 2, 2, 10000);

    this.renderer = new THREE.WebGLRenderer({ antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    if (mode === 'full') { this.renderer.toneMapping = THREE.ACESFilmicToneMapping; this.renderer.toneMappingExposure = 1.4; }
    container.appendChild(this.renderer.domElement);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.dampingFactor = 0.06;
    this.controls.minDistance = 15;
    this.controls.maxDistance = 4000;

    this.glowTex = mode === 'full' ? createSpikeTexture() : createGlowTexture();
    this.sphereGeo = new THREE.SphereGeometry(1, 24, 16);

    if (mode !== 'minimal') this.buildStarfield();
    this.scene.add(this.edgeGroup);
    this.scene.add(this.highlightEdgeGroup);
    this.scene.add(this.nodeGroup);

    this.raycaster = new THREE.Raycaster();
    this.setupHover();
    this.setupTooltip();
    this.setupDetailCard();

    // Labels container (not in minimal mode — but always create, hide via CSS)
    this.labelsContainer = document.createElement('div');
    this.labelsContainer.id = 'graph-labels';
    if (mode === 'minimal') this.labelsContainer.style.display = 'none';
    this.container.appendChild(this.labelsContainer);

    // Events
    this.container.addEventListener('click', (e: MouseEvent) => this.onClick(e));
    window.addEventListener('keydown', (e: KeyboardEvent) => {
      if (e.key === 'Escape' && this.blastMode) { this.exitBlastMode(); }
      if (e.key === 'b' || e.key === 'B') {
        if (this.blastMode) { this.exitBlastMode(); }
        else if (this.hoveredIdx >= 0) { this.startBlastMode(this.hoveredIdx); }
        else if (this.selectedIdx >= 0) { this.startBlastMode(this.selectedIdx); }
      }
    });

    this.onResize();
    window.addEventListener('resize', this.onResize);
    this.animate();
  }

  // ── Starfield ────────────────────────────────────────────

  private buildStarfield(): void {
    const isFull = this.mode === 'full';
    const count = isFull ? 4000 : 2200;
    const posArr = new Float32Array(count * 3), colArr = new Float32Array(count * 3);
    const layers = isFull ? [
      { r: [600, 1400], n: 600, hue: [200, 240], sat: 0.5, l: [0.4, 0.7] },
      { r: [300, 800], n: 1200, hue: [190, 220], sat: 0.35, l: [0.5, 0.85] },
      { r: [80, 450], n: 1200, hue: [180, 210], sat: 0.25, l: [0.65, 1.0] },
      { r: [15, 250], n: 1000, hue: [25, 55], sat: 0.55, l: [0.7, 1.0] },
    ] : [
      { r: [500, 1000], n: 300, hue: [210, 230], sat: 0.4, l: [0.5, 0.8] },
      { r: [250, 600], n: 700, hue: [200, 220], sat: 0.3, l: [0.6, 0.9] },
      { r: [60, 350], n: 700, hue: [190, 210], sat: 0.2, l: [0.7, 1.0] },
      { r: [10, 180], n: 500, hue: [30, 50], sat: 0.5, l: [0.7, 0.95] },
    ];
    let idx = 0;
    for (const L of layers) {
      for (let i = 0; i < L.n && idx < count; i++) {
        const theta = Math.random() * Math.PI * 2, phi = Math.acos(2 * Math.random() - 1);
        const r = L.r[0] + Math.random() * (L.r[1] - L.r[0]);
        posArr[idx * 3] = Math.cos(theta) * Math.sin(phi) * r;
        posArr[idx * 3 + 1] = Math.sin(phi) * r * 0.55;
        posArr[idx * 3 + 2] = Math.sin(theta) * Math.sin(phi) * r;
        const hsl = new THREE.Color();
        hsl.setHSL((L.hue[0] + Math.random() * (L.hue[1] - L.hue[0])) / 360, L.sat, L.l[0] + Math.random() * (L.l[1] - L.l[0]));
        colArr[idx * 3] = hsl.r; colArr[idx * 3 + 1] = hsl.g; colArr[idx * 3 + 2] = hsl.b;
        idx++;
      }
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(posArr, 3));
    geo.setAttribute('color', new THREE.BufferAttribute(colArr, 3));
    const mat = new THREE.PointsMaterial({ size: this.mode === 'full' ? 2.2 : 1.6, map: this.glowTex, blending: THREE.AdditiveBlending, depthWrite: false, vertexColors: true, transparent: true, opacity: this.mode === 'full' ? 1.0 : 0.55 });
    this.starfield = new THREE.Points(geo, mat);
    this.scene.add(this.starfield);
  }

  // ── Tooltip ──────────────────────────────────────────────

  private setupTooltip(): void {
    this.tooltipEl = document.createElement('div');
    this.tooltipEl.id = 'graph-tooltip';
    this.tooltipEl.innerHTML = '<div class="tt-name"></div><div class="tt-meta"></div><div class="tt-loc"></div>';
    this.container.appendChild(this.tooltipEl);
  }

  private updateTooltip(): void {
    if (this.hoveredIdx < 0 || this.hoveredIdx >= this.graphNodes.length) { this.tooltipEl.classList.remove('visible'); return; }
    const node = this.graphNodes[this.hoveredIdx];
    const kind = ((node.type || node.kind || 'symbol') as string).toLowerCase();
    this.tooltipEl.querySelector('.tt-name')!.textContent = node.name;
    const metaEl = this.tooltipEl.querySelector('.tt-meta')!;
    metaEl.textContent = `${TYPE_LABELS[kind] || kind.toUpperCase()} · 度 ${this.deg[this.hoveredIdx]}`;
    (metaEl as HTMLElement).dataset['kind'] = kind;
    this.tooltipEl.querySelector('.tt-loc')!.textContent = node.location || '';
    const i = this.hoveredIdx;
    this.tmpVec3.set(this.nodePositions[i * 3], this.nodePositions[i * 3 + 1], this.nodePositions[i * 3 + 2]);
    this.tmpVec3.project(this.camera);
    if (this.tmpVec3.z > 1) { this.tooltipEl.classList.remove('visible'); return; }
    const x = (this.tmpVec3.x * 0.5 + 0.5) * this.container.clientWidth;
    const y = (-this.tmpVec3.y * 0.5 + 0.5) * this.container.clientHeight;
    this.tooltipEl.style.left = `${x + 18}px`; this.tooltipEl.style.top = `${y - 10}px`;
    this.tooltipEl.classList.add('visible');
  }

  // ── Detail Card ──────────────────────────────────────────

  private setupDetailCard(): void {
    this.detailCard = document.createElement('div');
    this.detailCard.id = 'detail-card';
    this.detailCard.innerHTML =
      '<button class="dc-close">✕</button>' +
      '<div class="dc-name"></div><div class="dc-meta"></div><div class="dc-divider"></div>' +
      '<div class="dc-coupling"></div><div class="dc-divider"></div>' +
      '<div class="dc-location"></div>' +
      '<div class="dc-actions"><button class="dc-blast-btn">💥 波及</button><button class="dc-focus-btn">🔍 聚焦</button></div>';
    this.container.appendChild(this.detailCard);
    this.detailCard.querySelector('.dc-close')!.addEventListener('click', (e) => { e.stopPropagation(); this.hideDetail(); });
    this.detailCard.querySelector('.dc-focus-btn')!.addEventListener('pointerdown', (e) => {
      e.stopPropagation(); e.preventDefault();
      if (this.selectedIdx >= 0) this.flyToNode(this.selectedIdx);
    });
    this.detailCard.querySelector('.dc-blast-btn')!.addEventListener('pointerdown', (e) => {
      e.stopPropagation(); e.preventDefault();
      if (this.selectedIdx >= 0) this.startBlastMode(this.selectedIdx);
    });
  }

  private onClick(e: MouseEvent): void {
    if (this.nodeCores.length === 0) return;
    const rect = this.container.getBoundingClientRect();
    const mx = ((e.clientX - rect.left) / rect.width) * 2 - 1;
    const my = -((e.clientY - rect.top) / rect.height) * 2 + 1;
    this.raycaster.setFromCamera(new THREE.Vector2(mx, my), this.camera);
    const hits = this.raycaster.intersectObjects(this.nodeCores);
    const idx = hits.length > 0 ? this.nodeCores.indexOf(hits[0].object as THREE.Mesh) : -1;
    if (idx >= 0 && idx !== this.selectedIdx) this.showDetail(idx);
    else if (idx < 0) this.hideDetail();
  }

  private showDetail(idx: number): void {
    this.selectedIdx = idx;
    const node = this.graphNodes[idx];
    const kind = ((node.type || node.kind || 'symbol') as string).toLowerCase();
    const dist = [0, 0, 0, 0, 0];
    for (const e of this.edgeDataList) { if (e.s === idx || e.t === idx) dist[e.couplingDepth] = (dist[e.couplingDepth] || 0) + 1; }
    const maxDist = Math.max(...dist, 1);
    this.detailCard.querySelector('.dc-name')!.textContent = node.name;
    const metaEl = this.detailCard.querySelector('.dc-meta')!;
    metaEl.textContent = `${TYPE_LABELS[kind] || kind.toUpperCase()} · 度 ${this.deg[idx]}${this.deg[idx] >= 10 ? ' · hub' : ''}`;
    (metaEl as HTMLElement).dataset['kind'] = kind;
    const bars = [
      { label: 'L1 公开API', v: dist[1], cls: 'l1' }, { label: 'L2 内部导入', v: dist[2], cls: 'l2' },
      { label: 'L3 共享数据', v: dist[3], cls: 'l3' }, { label: 'L4 封装穿透', v: dist[4], cls: 'l4' },
    ];
    this.detailCard.querySelector('.dc-coupling')!.innerHTML = bars.filter(b => b.v > 0).map(b => {
      const pct = Math.round((b.v / maxDist) * 100);
      const warn = b.cls === 'l3' ? ' ⚠' : b.cls === 'l4' ? ' ⛔' : '';
      return `<div class="dc-bar-row"><span class="dc-bar-label">${b.label}</span><span class="dc-bar-count">${b.v} 条</span><span class="dc-bar-track"><span class="dc-bar-fill ${b.cls}" style="width:${pct}%"></span></span>${warn}</div>`;
    }).join('') || '<div class="dc-empty">无耦合边</div>';
    this.detailCard.querySelector('.dc-location')!.textContent = node.location || '';
    this.positionDetailCard(idx);
    this.detailCard.classList.add('visible');
  }

  private hideDetail(): void { this.selectedIdx = -1; this.detailCard.classList.remove('visible'); }

  private positionDetailCard(idx: number): void {
    this.tmpVec3.set(this.nodePositions[idx * 3], this.nodePositions[idx * 3 + 1], this.nodePositions[idx * 3 + 2]);
    this.tmpVec3.project(this.camera);
    const x = (this.tmpVec3.x * 0.5 + 0.5) * this.container.clientWidth;
    const y = (-this.tmpVec3.y * 0.5 + 0.5) * this.container.clientHeight;
    let left = x + 24, top = y - 60;
    if (left + 220 > this.container.clientWidth - 10) left = x - 244;
    if (top < 10) top = 10;
    if (top + 200 > this.container.clientHeight - 10) top = this.container.clientHeight - 210;
    if (left < 10) left = 10;
    this.detailCard.style.left = `${left}px`; this.detailCard.style.top = `${top}px`;
  }

  // ── Hover ────────────────────────────────────────────────

  private setupHover(): void {
    this.container.addEventListener('mousemove', (e: MouseEvent) => {
      const rect = this.container.getBoundingClientRect();
      this.mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
      this.mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
    });
    this.container.addEventListener('mouseleave', () => { this.mouse.x = -999; this.mouse.y = -999; });
  }

  private updateHover(): void {
    if (this.nodeCores.length === 0) return;
    this.raycaster.setFromCamera(this.mouse, this.camera);
    const hits = this.raycaster.intersectObjects(this.nodeCores);
    const newIdx = hits.length > 0 ? this.nodeCores.indexOf(hits[0].object as THREE.Mesh) : -1;
    if (newIdx !== this.hoveredIdx) { this.hoveredIdx = newIdx; this.targetHoverScale = newIdx >= 0 ? 1 : 0; this.rebuildHighlightEdges(newIdx); }
  }

  private rebuildHighlightEdges(nodeIdx: number): void {
    if (this.blastMode) return;
    while (this.highlightEdgeGroup.children.length) this.highlightEdgeGroup.remove(this.highlightEdgeGroup.children[0]);
    if (nodeIdx < 0 || nodeIdx >= this.graphNodes.length) return;
    const edges = this.edgeIndexOf[nodeIdx];
    if (edges.length === 0) return;
    const pos = this.nodePositions, verts: number[] = [], colors: number[] = [];
    for (const ei of edges) {
      const d = this.edgeDataList[ei];
      verts.push(pos[d.s * 3], pos[d.s * 3 + 1], pos[d.s * 3 + 2], pos[d.t * 3], pos[d.t * 3 + 1], pos[d.t * 3 + 2]);
      const c = edgeColorByType(d.edgeType, d.direction), bright = this.mode === 'full' ? 2.5 : 1.6;
      colors.push(Math.min(1, c.r * bright), Math.min(1, c.g * bright), Math.min(1, c.b * bright), Math.min(1, c.r * bright), Math.min(1, c.g * bright), Math.min(1, c.b * bright));
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(verts, 3));
    geo.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3));
    this.highlightEdgeGroup.add(new THREE.LineSegments(geo, new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: this.mode === 'full' ? 0.9 : 0.7, depthWrite: false, blending: THREE.AdditiveBlending })));
  }

  // ── Labels ───────────────────────────────────────────────

  private updateLabels(): void {
    const halfW = this.container.clientWidth * 0.5, halfH = this.container.clientHeight * 0.5;
    for (let k = 0; k < this.nodeLabelIdx.length; k++) {
      const i = this.nodeLabelIdx[k], div = this.labelDivs[k];
      if (!div) continue;
      this.tmpVec3.set(this.nodePositions[i * 3], this.nodePositions[i * 3 + 1], this.nodePositions[i * 3 + 2]);
      this.tmpVec3.project(this.camera);
      const behind = this.tmpVec3.z > 1;
      const dist = this.camera.position.distanceTo(new THREE.Vector3(this.nodePositions[i * 3], this.nodePositions[i * 3 + 1], this.nodePositions[i * 3 + 2]));
      const opacity = behind ? 0 : Math.max(0, 1 - dist / 2000);
      div.style.left = `${this.tmpVec3.x * halfW + halfW}px`;
      div.style.top = `${-this.tmpVec3.y * halfH + halfH}px`;
      div.style.opacity = String(opacity);
      div.style.display = opacity > 0.05 ? '' : 'none';
    }
  }

  // ── Blast ────────────────────────────────────────────────

  private startBlastMode(idx: number): void {
    this.blastMode = true; this.blastSource = idx; this.computeBlastDistances(); this.buildBlastEdges();
    const st = document.getElementById('status-text');
    const inRadius = this.blastDistances.filter(d => d >= 0).length;
    if (st) st.textContent = `💥 波及: ${this.graphNodes[idx]?.name || '?'}  ·  ${inRadius} 节点  ·  B/ESC 退出`;
  }

  private computeBlastDistances(): void {
    const n = this.graphNodes.length;
    this.blastDistances = new Array(n).fill(-1);
    if (this.blastSource < 0) return;
    this.blastDistances[this.blastSource] = 0;
    const queue = [this.blastSource];
    while (queue.length > 0) {
      const u = queue.shift()!, du = this.blastDistances[u];
      if (du >= this.blastMaxDist) continue;
      for (const v of this.neighborMap[u] || []) {
        if (this.blastDistances[v] === -1) { this.blastDistances[v] = du + 1; queue.push(v); }
      }
    }
  }

  private buildBlastEdges(): void {
    while (this.highlightEdgeGroup.children.length) this.highlightEdgeGroup.remove(this.highlightEdgeGroup.children[0]);
    if (!this.blastMode) return;
    const pos = this.nodePositions, verts: number[] = [], colors: number[] = [];
    for (const d of this.edgeDataList) {
      const ds = this.blastDistances[d.s], dt = this.blastDistances[d.t];
      if (ds < 0 || dt < 0) continue;
      verts.push(pos[d.s * 3], pos[d.s * 3 + 1], pos[d.s * 3 + 2], pos[d.t * 3], pos[d.t * 3 + 1], pos[d.t * 3 + 2]);
      const minD = Math.min(ds, dt);
      const c = minD === 0 ? new THREE.Color(0xffffff) : minD === 1 ? new THREE.Color(0xff6644) : minD <= 3 ? new THREE.Color(0xffaa44) : new THREE.Color(0xffdd88);
      colors.push(c.r, c.g, c.b, c.r, c.g, c.b);
    }
    if (verts.length === 0) return;
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(verts, 3));
    geo.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3));
    this.highlightEdgeGroup.add(new THREE.LineSegments(geo, new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.55, depthWrite: false, blending: THREE.AdditiveBlending })));
  }

  private exitBlastMode(): void {
    this.blastMode = false; this.blastSource = -1; this.blastDistances = [];
    while (this.highlightEdgeGroup.children.length) this.highlightEdgeGroup.remove(this.highlightEdgeGroup.children[0]);
    for (let i = 0; i < this.nodeGlows.length; i++) {
      (this.nodeGlows[i].material as THREE.SpriteMaterial).color.set(this.nodeGlowColors[i]);
      (this.nodeGlows[i].material as THREE.SpriteMaterial).opacity = this.mode === 'minimal' ? 0 : 0.55;
      const kind = ((this.graphNodes[i]?.type || this.graphNodes[i]?.kind || 'symbol') as string).toLowerCase();
      (this.nodeCores[i].material as THREE.MeshBasicMaterial).color.set(
        this.mode === 'full' ? 0xffffff : (NODE_COLORS[kind] || 0x7eb8ff)
      );
    }
    const st = document.getElementById('status-text');
    if (st && st.textContent?.startsWith('💥')) st.textContent = '就绪';
  }

  // ── Focus ────────────────────────────────────────────────

  private flyToNode(idx: number): void {
    const px = this.nodePositions[idx * 3], py = this.nodePositions[idx * 3 + 1], pz = this.nodePositions[idx * 3 + 2];
    this.focusTarget.set(px, py, pz); this.focusStartCam.copy(this.camera.position); this.focusStartLook.copy(this.controls.target);
    this.focusActive = true; this.focusProgress = 0; this.focusNodeIdx = idx; this.focusFlash = 1;
  }

  focusNode(query: string): boolean {
    const q = query.trim().toLowerCase();
    if (!q || this.graphNodes.length === 0) return false;
    let idx = this.graphNodes.findIndex(n => n.name.toLowerCase() === q);
    if (idx < 0) idx = this.graphNodes.findIndex(n => n.name.toLowerCase().startsWith(q));
    if (idx < 0) idx = this.graphNodes.findIndex(n => n.name.toLowerCase().includes(q));
    if (idx < 0) return false;
    this.flyToNode(idx); return true;
  }

  private updateFocus(): void {
    if (!this.focusActive) return;
    this.focusProgress += 0.025;
    const t = easeInOutCubic(Math.min(1, this.focusProgress));
    this.camera.position.lerpVectors(this.focusStartCam, this.focusTarget.clone().add(new THREE.Vector3(80, 60, 100)), t);
    this.controls.target.lerpVectors(this.focusStartLook, this.focusTarget, t);
    if (this.focusNodeIdx >= 0 && this.focusNodeIdx < this.nodeGlows.length) {
      const base = 0.6 + (this.deg[this.focusNodeIdx] / this.maxDeg) * 2.8;
      const flashScale = 1 + Math.sin(this.focusProgress * 20) * 0.5 * this.focusFlash;
      this.nodeGlows[this.focusNodeIdx].scale.setScalar(base * 5.5 * flashScale);
      (this.nodeGlows[this.focusNodeIdx].material as THREE.SpriteMaterial).opacity = 0.55 + 0.45 * this.focusFlash;
      this.nodeCores[this.focusNodeIdx].scale.setScalar(base * flashScale);
      this.focusFlash *= 0.97;
    }
    if (t >= 1) { this.focusActive = false; setTimeout(() => this.restoreFocusNode(), 800); }
  }

  private restoreFocusNode(): void {
    if (this.focusNodeIdx < 0 || this.focusNodeIdx >= this.nodeGlows.length) return;
    const base = 0.6 + (this.deg[this.focusNodeIdx] / this.maxDeg) * 2.8;
    this.nodeGlows[this.focusNodeIdx].scale.setScalar(base * 5.5);
    (this.nodeGlows[this.focusNodeIdx].material as THREE.SpriteMaterial).opacity = 0.55;
    this.nodeCores[this.focusNodeIdx].scale.setScalar(base);
    this.focusNodeIdx = -1;
  }

  // ── Render ───────────────────────────────────────────────

  render(graph: GraphJSON): void {
    this.clearGraph();
    const nodes = Array.isArray(graph.nodes) ? graph.nodes : Object.values(graph.nodes);
    const edges = Array.isArray(graph.edges) ? graph.edges : Object.values(graph.edges);
    if (nodes.length === 0) { this.updateStatus(0, 0); return; }
    this.graphNodes = nodes;

    const nodeIdx = new Map<string, number>();
    const pairs: [number, number][] = [];
    const eData: EdgeData[] = [];
    const deg = new Array<number>(nodes.length).fill(0);
    for (let i = 0; i < nodes.length; i++) nodeIdx.set(nodes[i].id, i);
    for (const e of edges) {
      const s = nodeIdx.get(e.source), t = nodeIdx.get(e.target);
      if (s !== undefined && t !== undefined && s !== t) {
        pairs.push([s, t]); deg[s]++; deg[t]++;
        eData.push({ s, t, couplingDepth: (e.properties?.['coupling_depth'] as number) || 0, edgeType: e.type || '', direction: (e as any).direction || '' });
      }
    }
    this.deg = deg; this.edgeDataList = eData; this.maxDeg = Math.max(...deg, 1);

    this.neighborMap = Array.from({ length: nodes.length }, () => []);
    this.edgeIndexOf = Array.from({ length: nodes.length }, () => []);
    for (let ei = 0; ei < eData.length; ei++) {
      const { s, t } = eData[ei];
      this.neighborMap[s].push(t); this.neighborMap[t].push(s);
      this.edgeIndexOf[s].push(ei); this.edgeIndexOf[t].push(ei);
    }

    this.l34Count = new Array(nodes.length).fill(0);
    for (const e of eData) { if (e.couplingDepth >= 3) { this.l34Count[e.s]++; this.l34Count[e.t]++; } }

    const rawPos = layout3D(nodes.length, pairs);
    let cx = 0, cy = 0, cz = 0;
    for (let i = 0; i < nodes.length; i++) { cx += rawPos[i * 3]; cy += rawPos[i * 3 + 1]; cz += rawPos[i * 3 + 2]; }
    cx /= nodes.length; cy /= nodes.length; cz /= nodes.length;
    for (let i = 0; i < nodes.length; i++) { rawPos[i * 3] -= cx; rawPos[i * 3 + 1] -= cy; rawPos[i * 3 + 2] -= cz; }
    this.nodePositions = rawPos;

    let maxR = 50;
    for (let i = 0; i < nodes.length; i++) maxR = Math.max(maxR, Math.sqrt(rawPos[i * 3] ** 2 + rawPos[i * 3 + 1] ** 2 + rawPos[i * 3 + 2] ** 2));
    const camDist = maxR * 2.6;
    this.camera.position.set(camDist * 0.55, camDist * 0.45, camDist * 0.65);
    this.controls.target.set(0, 0, 0);
    this.camera.aspect = this.container.clientWidth / this.container.clientHeight;
    this.camera.updateProjectionMatrix(); this.controls.update();

    this.buildEdges(rawPos, eData);
    this.buildNodes(nodes, rawPos, deg);
    this.buildLabels(nodes, deg);

    // Full mode: edge particle flow + twinkle data
    if (this.mode === 'full') {
      this.initTwinkleData(nodes.length);
      this.initEdgeParticles(rawPos, eData);
    }

    this.updateStatus(nodes.length, edges.length, graph.meta);
  }

  private clearGraph(): void {
    while (this.nodeGroup.children.length) this.nodeGroup.remove(this.nodeGroup.children[0]);
    while (this.edgeGroup.children.length) this.edgeGroup.remove(this.edgeGroup.children[0]);
    while (this.highlightEdgeGroup.children.length) this.highlightEdgeGroup.remove(this.highlightEdgeGroup.children[0]);
    this.labelsContainer.innerHTML = '';
    this.labelDivs = []; this.nodeLabelIdx = [];
    this.nodeCores = []; this.nodeGlows = []; this.nodeGlows2 = []; this.nodeGlowColors = []; this.edgeLineGroups = [];
    this.neighborMap = []; this.edgeIndexOf = [];
    this.hoveredIdx = -1; this.targetHoverScale = 0;
    this.focusActive = false; this.focusNodeIdx = -1; this.selectedIdx = -1;
    this.blastMode = false; this.blastSource = -1; this.blastDistances = []; this.l34Count = [];
    this.tooltipEl?.classList.remove('visible');
    this.detailCard?.classList.remove('visible');
  }

  // ── Edges ────────────────────────────────────────────────

  private buildEdges(pos: Float32Array, data: EdgeData[]): void {
    if (data.length === 0) return;
    const key = (d: EdgeData) => `${d.edgeType}:${d.direction}:${d.couplingDepth}`;
    const groups = new Map<string, { verts: number[]; colors: number[]; depth: number }>();
    for (const d of data) {
      const k = key(d);
      if (!groups.has(k)) { const c = edgeColorByType(d.edgeType, d.direction); groups.set(k, { verts: [], colors: [], depth: d.couplingDepth }); }
      const g = groups.get(k)!;
      g.verts.push(pos[d.s * 3], pos[d.s * 3 + 1], pos[d.s * 3 + 2], pos[d.t * 3], pos[d.t * 3 + 1], pos[d.t * 3 + 2]);
      const c = edgeColorByType(d.edgeType, d.direction);
      g.colors.push(c.r, c.g, c.b, c.r, c.g, c.b);
    }
    for (const [, g] of groups) {
      const B = 2000;
      for (let b = 0; b < g.verts.length; b += B * 6) {
        const v = g.verts.slice(b, b + B * 6), cl = g.colors.slice(b, b + B * 6);
        const geo = new THREE.BufferGeometry();
        geo.setAttribute('position', new THREE.Float32BufferAttribute(v, 3));
        geo.setAttribute('color', new THREE.Float32BufferAttribute(cl, 3));
        const mat = new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: edgeOpacityByDepth(g.depth), depthWrite: false, blending: g.depth >= 3 ? THREE.AdditiveBlending : THREE.NormalBlending });
        const lines = new THREE.LineSegments(geo, mat);
        this.edgeGroup.add(lines); this.edgeLineGroups.push(lines);
      }
    }
  }

  // ── Nodes ────────────────────────────────────────────────

  private buildNodes(nodes: GraphNode[], pos: Float32Array, deg: number[]): void {
    const isFull = this.mode === 'full';
    for (let i = 0; i < nodes.length; i++) {
      const kind = ((nodes[i].type || nodes[i].kind || 'symbol') as string).toLowerCase();
      const coreColor = isFull ? 0xffffff : (NODE_COLORS[kind] || 0x7eb8ff); // white-hot core in full mode
      const glowColor = GLOW_COLORS[kind] || 0x4488cc;
      const baseScale = 0.6 + (deg[i] / this.maxDeg) * 2.8;
      const glowOpacity = this.mode === 'minimal' ? 0 : 0.55;
      const glowScaleMul = isFull ? 9 : 5.5;

      // Full mode: large soft outer glow first (behind everything)
      if (isFull) {
        const outerGlow = new THREE.Sprite(new THREE.SpriteMaterial({
          map: this.glowTex, color: glowColor,
          blending: THREE.AdditiveBlending, depthWrite: false, transparent: true, opacity: 0.35,
        }));
        outerGlow.position.set(pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]);
        outerGlow.scale.setScalar(baseScale * 16);
        this.nodeGroup.add(outerGlow); this.nodeGlows2.push(outerGlow);
      }

      // Inner spike glow (or standard glow)
      const glow = new THREE.Sprite(new THREE.SpriteMaterial({
        map: this.glowTex, color: glowColor,
        blending: THREE.AdditiveBlending, depthWrite: false, transparent: true, opacity: glowOpacity,
      }));
      glow.position.set(pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]);
      glow.scale.setScalar(baseScale * glowScaleMul);
      this.nodeGroup.add(glow); this.nodeGlows.push(glow); this.nodeGlowColors.push(glowColor);

      // Core — small bright white center in full mode, colored in standard
      const core = new THREE.Mesh(this.sphereGeo, new THREE.MeshBasicMaterial({ color: coreColor }));
      core.position.copy(glow.position);
      core.scale.setScalar(isFull ? baseScale * 0.4 : baseScale); // smaller core in full mode = point-like star
      core.userData = { nodeIndex: i };
      this.nodeGroup.add(core); this.nodeCores.push(core);
    }
  }

  private buildLabels(nodes: GraphNode[], deg: number[]): void {
    const sorted = deg.map((d, i) => ({ d, i })).sort((a, b) => b.d - a.d);
    const pct = this.mode === 'full' ? 0.5 : 0.2;
    const maxCount = this.mode === 'full' ? 120 : 60;
    const count = Math.max(3, Math.min(maxCount, Math.ceil(nodes.length * pct)));
    this.nodeLabelIdx = sorted.slice(0, count).filter(x => x.d > 0).map(x => x.i);
    for (const i of this.nodeLabelIdx) {
      const div = document.createElement('div'); div.className = 'node-label';
      div.dataset['kind'] = ((nodes[i].type || nodes[i].kind || 'symbol') as string).toLowerCase();
      div.textContent = nodes[i].name;
      this.labelsContainer.appendChild(div); this.labelDivs.push(div);
    }
  }

  // ── Status ───────────────────────────────────────────────

  private updateStatus(nodeCount: number, edgeCount: number, meta?: Record<string, unknown>): void {
    const ns = document.getElementById('status-nodes'), es = document.getElementById('status-edges'), st = document.getElementById('status-text');
    if (ns) ns.textContent = `${nodeCount} 节点`;
    if (es) es.textContent = `${edgeCount} 边`;
    let sCount = 0, dCount = 0, tCount = 0;
    for (const e of this.edgeDataList) {
      if (e.edgeType === 'structural' || e.edgeType === 'STRUCTURAL') sCount++;
      else if (e.edgeType === 'data' || e.edgeType === 'DATA') dCount++;
      else if (e.edgeType === 'temporal' || e.edgeType === 'TEMPORAL') tCount++;
    }
    const coup = (meta?.coupling || {}) as Record<string, number>;
    const l3 = coup.total_l3 || 0, l4 = coup.total_l4 || 0;
    if (st) {
      let text = `${nodeCount} 节点 · ${edgeCount} 边 · S${sCount} D${dCount} T${tCount}`;
      if (l4 > 0) text += ` · ⛔ L4×${l4}`;
      else if (l3 > 0) text += ` · ⚠ L3×${l3}`;
      st.textContent = text;
    }
  }

  // ── Full-FX: edge particle flow ──────────────────────────

  private initTwinkleData(n: number): void {
    this.twinklePhases = new Array(n).fill(0).map(() => Math.random() * Math.PI * 2);
    this.twinkleSpeeds = new Array(n).fill(0).map(() => 0.5 + Math.random() * 2.5);
  }

  private initEdgeParticles(pos: Float32Array, data: EdgeData[]): void {
    // Remove old
    if (this.edgeParticles) { this.scene.remove(this.edgeParticles); (this.edgeParticles.material as THREE.Material).dispose(); this.edgeParticles.geometry.dispose(); }
    this.edgeParticleData = [];

    const count = Math.min(300, data.length * 2);
    const pPos = new Float32Array(count * 3);
    const pCol = new Float32Array(count * 3);

    for (let i = 0; i < count; i++) {
      const ei = Math.floor(Math.random() * data.length);
      const d = data[ei];
      const t = Math.random();
      pPos[i * 3]     = pos[d.s * 3]     + (pos[d.t * 3]     - pos[d.s * 3])     * t;
      pPos[i * 3 + 1] = pos[d.s * 3 + 1] + (pos[d.t * 3 + 1] - pos[d.s * 3 + 1]) * t;
      pPos[i * 3 + 2] = pos[d.s * 3 + 2] + (pos[d.t * 3 + 2] - pos[d.s * 3 + 2]) * t;

      const c = edgeColorByType(d.edgeType, d.direction);
      pCol[i * 3] = c.r; pCol[i * 3 + 1] = c.g; pCol[i * 3 + 2] = c.b;

      this.edgeParticleData.push({ edgeIdx: ei, t, speed: 0.002 + Math.random() * 0.008, dir: Math.random() > 0.5 ? 1 : -1 });
    }

    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(pPos, 3));
    geo.setAttribute('color', new THREE.BufferAttribute(pCol, 3));
    const mat = new THREE.PointsMaterial({ size: 0.8, map: this.glowTex, blending: THREE.AdditiveBlending, depthWrite: false, vertexColors: true, transparent: true, opacity: 0.7 });
    this.edgeParticles = new THREE.Points(geo, mat);
    this.scene.add(this.edgeParticles);
  }

  private animateEdgeParticles(): void {
    if (!this.edgeParticles) return;
    const pos = this.edgeParticles.geometry.attributes['position'].array as Float32Array;
    for (let i = 0; i < this.edgeParticleData.length; i++) {
      const pd = this.edgeParticleData[i];
      const d = this.edgeDataList[pd.edgeIdx];
      if (!d) continue;
      pd.t += pd.speed * pd.dir;
      if (pd.t > 1) { pd.t = 1; pd.dir = -1; }
      if (pd.t < 0) { pd.t = 0; pd.dir = 1; }
      pos[i * 3]     = this.nodePositions[d.s * 3]     + (this.nodePositions[d.t * 3]     - this.nodePositions[d.s * 3])     * pd.t;
      pos[i * 3 + 1] = this.nodePositions[d.s * 3 + 1] + (this.nodePositions[d.t * 3 + 1] - this.nodePositions[d.s * 3 + 1]) * pd.t;
      pos[i * 3 + 2] = this.nodePositions[d.s * 3 + 2] + (this.nodePositions[d.t * 3 + 2] - this.nodePositions[d.s * 3 + 2]) * pd.t;
    }
    this.edgeParticles.geometry.attributes['position'].needsUpdate = true;
  }

  // ── Animate ──────────────────────────────────────────────

  private animate(): void {
    this.animId = requestAnimationFrame(() => this.animate());
    const isMinimal = this.mode === 'minimal';
    const isFull = this.mode === 'full';
    const fx = isFull ? 3 : isMinimal ? 0 : 1;

    // Full mode: auto-rotate the entire galaxy slowly
    if (isFull) { this.nodeGroup.rotation.y += 0.0008; this.nodeGroup.rotation.x += 0.0002; this.edgeGroup.rotation.y += 0.0008; this.edgeGroup.rotation.x += 0.0002; }

    if (!isMinimal) {
      this.starfield.rotation.y += 0.00006 * fx;
      this.starfield.rotation.x += 0.00002 * fx;
    }
    if (isFull) this.animateEdgeParticles();

    if (isMinimal) {
      this.controls.update();
      this.renderer.render(this.scene, this.camera);
      return;
    }

    this.updateHover(); this.updateFocus();

    // Hover effects
    this.hoverScale += (this.targetHoverScale - this.hoverScale) * 0.18;
    const neighborSet = new Set(this.hoveredIdx >= 0 ? this.neighborMap[this.hoveredIdx] || [] : []);
    if (this.hoveredIdx >= 0 && this.hoveredIdx < this.nodeCores.length) {
      const base = 0.6 + (this.deg[this.hoveredIdx] / this.maxDeg) * 2.8;
      const s = 1 + this.hoverScale * 1.2;
      this.nodeCores[this.hoveredIdx].scale.setScalar(base * s);
      if (this.nodeGlows[this.hoveredIdx]) {
        this.nodeGlows[this.hoveredIdx].scale.setScalar(base * (isFull ? 7 : 5.5) * s);
        (this.nodeGlows[this.hoveredIdx].material as THREE.SpriteMaterial).opacity = 0.55 + this.hoverScale * 0.45;
      }
      for (const ni of neighborSet) {
        if (ni !== this.hoveredIdx && ni < this.nodeGlows.length) {
          (this.nodeGlows[ni].material as THREE.SpriteMaterial).opacity = 0.55 + this.hoverScale * 0.3;
        }
      }
    }

    // Pulse + blast
    this.pulseTime += 0.03 * (isFull ? 1.5 : 1);
    const galTime = performance.now() * 0.001; // galaxy time for color cycling
    for (let i = 0; i < this.nodeGlows.length; i++) {
      if (i === this.hoveredIdx || neighborSet.has(i) || i === this.focusNodeIdx) continue;
      if (this.blastMode) {
        const d = this.blastDistances[i];
        if (d >= 0) {
          const c = new THREE.Color();
          if (d === 0) c.set(0xffffff); else if (d === 1) c.set(0xff4422); else if (d === 2) c.set(0xff8800); else if (d === 3) c.set(0xffcc00); else c.setHSL(0.55 - (d / this.blastMaxDist) * 0.3, 0.6, 0.4 + (1 - d / this.blastMaxDist) * 0.3);
          (this.nodeGlows[i].material as THREE.SpriteMaterial).color.set(c);
          (this.nodeGlows[i].material as THREE.SpriteMaterial).opacity = 0.7;
          (this.nodeCores[i].material as THREE.MeshBasicMaterial).color.set(c);
          const base = 0.6 + (this.deg[i] / this.maxDeg) * 2.8;
          this.nodeGlows[i].scale.setScalar(base * (isFull ? 7 : 5.5) * (d === 0 ? 2 : 1.2));
          this.nodeCores[i].scale.setScalar(base * (d === 0 ? 2 : 1));
        } else {
          (this.nodeGlows[i].material as THREE.SpriteMaterial).opacity = 0.12;
        }
      } else {
        const risk = this.l34Count[i];
        if (isFull) {
          // Full mode: individual twinkle + color cycling
          const twinkle = 1 + Math.sin(galTime * this.twinkleSpeeds[i] + this.twinklePhases[i]) * 0.35;
          const wave = 1 + Math.sin(this.pulseTime * (1 + risk * 0.7)) * (risk > 0 ? 0.4 : 0.15);
          const combined = twinkle * wave;
          (this.nodeGlows[i].material as THREE.SpriteMaterial).opacity = 0.75 * combined;
          // Animate outer glow layer too
          if (this.nodeGlows2[i]) {
            (this.nodeGlows2[i].material as THREE.SpriteMaterial).opacity = 0.35 * combined;
            const base = 0.6 + (this.deg[i] / this.maxDeg) * 2.8;
            this.nodeGlows2[i].scale.setScalar(base * 16 * combined);
          }
          // Hue shift
          const hueShift = (Math.sin(galTime * 0.3 + this.twinklePhases[i]) * 0.05);
          const origColor = new THREE.Color(this.nodeGlowColors[i]);
          const hsl: { h: number; s: number; l: number } = { h: 0, s: 0, l: 0 };
          origColor.getHSL(hsl);
          const newColor = new THREE.Color();
          newColor.setHSL((hsl.h + hueShift + 1) % 1, Math.min(1, hsl.s * 1.2), Math.min(1, hsl.l * 1.3));
          (this.nodeGlows[i].material as THREE.SpriteMaterial).color.set(newColor);
          const base = 0.6 + (this.deg[i] / this.maxDeg) * 2.8;
          this.nodeGlows[i].scale.setScalar(base * 9 * combined);
        } else {
          const freq = 1 + risk * 0.7;
          const amp = risk > 0 ? Math.min(0.4, risk * 0.13) : 0.06;
          const wave = 1 + Math.sin(this.pulseTime * freq) * amp;
          (this.nodeGlows[i].material as THREE.SpriteMaterial).opacity = 0.55 * wave;
          const base = 0.6 + (this.deg[i] / this.maxDeg) * 2.8;
          this.nodeGlows[i].scale.setScalar(base * 5.5);
        }
      }
    }

    this.updateTooltip(); this.updateLabels();
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  }

  // ── Resize ───────────────────────────────────────────────

  private onResize = (): void => {
    this.camera.aspect = this.container.clientWidth / this.container.clientHeight;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(this.container.clientWidth, this.container.clientHeight);
  };

  // ── Destroy ──────────────────────────────────────────────

  destroy(): void {
    cancelAnimationFrame(this.animId);
    window.removeEventListener('resize', this.onResize);
    this.renderer.dispose();
    this.renderer.domElement.remove();
    this.glowTex.dispose(); this.sphereGeo.dispose();
    this.tooltipEl?.remove(); this.labelsContainer?.remove(); this.detailCard?.remove();
  }
}

function easeInOutCubic(t: number): number { return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2; }
