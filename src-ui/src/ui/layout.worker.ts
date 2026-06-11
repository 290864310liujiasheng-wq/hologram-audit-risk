// Layout Worker — runs layout3D off the main thread
// ⛔ CANONICAL LAYOUT — DO NOT MODIFY PARAMETERS ⛔
// See graph.ts for the locked parameter documentation.

function fibonacciSphere(n: number, radius: number): Float32Array {
  const pos = new Float32Array(n * 3);
  const phi = Math.PI * (1 + Math.sqrt(5));
  for (let i = 0; i < n; i++) {
    const y = 1 - (i / (n - 1)) * 2;
    const r = Math.sqrt(1 - y * y);
    const theta = phi * i;
    pos[i * 3] = Math.cos(theta) * r * radius;
    pos[i * 3 + 1] = y * radius;
    pos[i * 3 + 2] = Math.sin(theta) * r * radius;
  }
  return pos;
}

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
        const fRaw = rep / (dist * dist + 1);
        const f = Math.min(fRaw, shellRadius * 8);
        vel[i * 3] += (dx / dist) * f; vel[i * 3 + 1] += (dy / dist) * f; vel[i * 3 + 2] += (dz / dist) * f;
        vel[j * 3] -= (dx / dist) * f; vel[j * 3 + 1] -= (dy / dist) * f; vel[j * 3 + 2] -= (dz / dist) * f;
      }
    }
    for (const [s, t] of edgePairs) {
      const dx = pos[s * 3] - pos[t * 3], dy = pos[s * 3 + 1] - pos[t * 3 + 1], dz = pos[s * 3 + 2] - pos[t * 3 + 2];
      const dist = Math.max(0.3, Math.sqrt(dx * dx + dy * dy + dz * dz)), f = Math.min(dist * att, shellRadius);
      vel[s * 3] -= (dx / dist) * f; vel[s * 3 + 1] -= (dy / dist) * f; vel[s * 3 + 2] -= (dz / dist) * f;
      vel[t * 3] += (dx / dist) * f; vel[t * 3 + 1] += (dy / dist) * f; vel[t * 3 + 2] += (dz / dist) * f;
    }
    for (let i = 0; i < n; i++) { vel[i * 3] -= pos[i * 3] * 0.0004; vel[i * 3 + 1] -= pos[i * 3 + 1] * 0.0004; vel[i * 3 + 2] -= pos[i * 3 + 2] * 0.0004; }
    for (let i = 0; i < n * 3; i++) { vel[i] *= damp; pos[i] += vel[i]; }
    // ── NaN guard ──
    if (iter % 5 === 0) {
      let diverged = false;
      for (let i = 0; i < n * 3; i++) {
        if (!isFinite(pos[i]) || !isFinite(vel[i])) { diverged = true; break; }
      }
      if (diverged) {
        const fresh = fibonacciSphere(n, shellRadius);
        for (let i = 0; i < n * 3; i++) { pos[i] = fresh[i]; vel[i] = 0; }
      }
    }
    for (let i = 0; i < n; i++) {
      const dx = pos[i * 3], dy = pos[i * 3 + 1], dz = pos[i * 3 + 2], dist = Math.sqrt(dx * dx + dy * dy + dz * dz);
      if (dist > 1) { const drift = (dist - shellRadius) * sp; pos[i * 3] -= (dx / dist) * drift; pos[i * 3 + 1] -= (dy / dist) * drift; pos[i * 3 + 2] -= (dz / dist) * drift; }
    }
  }
  return pos;
}

self.onmessage = (e: MessageEvent) => {
  const { nodes, pairs } = e.data;
  const pos = layout3D(nodes, pairs);
  self.postMessage({ pos }, undefined as any);
};
