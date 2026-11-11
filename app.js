// Browser plumbing/input only. Picking, dragging and rendering math live in WASM.
const canvas = document.querySelector('canvas');
const context = canvas.getContext('2d', { alpha: false });
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
const clients = [];
let control;
let generation = 0;
let ready = false;
let failed = false;
let refreshing = false;
let dirty = false;
let pointer = null;
let movementTimer;
let resizeTimer;

function fail(error) {
  if (failed) return;
  failed = true;
  for (const client of clients) client.close();
  console.error('WASM scene failed:', error);
}

class RendererWorker {
  constructor() {
    this.worker = new Worker('./worker.js');
    this.next = 0;
    this.pending = new Map();
    this.worker.onmessage = ({ data }) => {
      const request = this.pending.get(data.id);
      if (!request) return;
      this.pending.delete(data.id);
      if (data.ok) request.resolve(data);
      else request.reject(new Error(data.error));
    };
    this.worker.onerror = event => fail(new Error(event.message || 'Renderer worker failed.'));
  }
  call(type, data = {}) {
    return new Promise((resolve, reject) => {
      const id = ++this.next;
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ id, type, ...data });
    });
  }
  close() {
    this.worker.terminate();
    for (const request of this.pending.values()) request.reject(new Error('Renderer stopped.'));
    this.pending.clear();
  }
}

async function loadModule() {
  for (const file of ['./light_room-simd.wasm', './light_room.wasm']) {
    try {
      const response = await fetch(file);
      if (!response.ok) throw new Error(`${file}: HTTP ${response.status}`);
      return await WebAssembly.compile(await response.arrayBuffer());
    } catch (error) {
      if (file === './light_room.wasm') throw error;
    }
  }
}

function dimensions(preview) {
  const w = Math.max(1, window.innerWidth);
  const h = Math.max(1, window.innerHeight);
  const edge = preview ? 320 : 640;
  const budget = preview ? 36864 : 147456;
  const scale = Math.min(1, edge / w, edge / h, Math.sqrt(budget / (w * h)));
  return [Math.max(1, Math.round(w * scale)), Math.max(1, Math.round(h * scale))];
}

async function renderBand(client, tile, version) {
  while (version === generation && !failed) {
    if (document.hidden) { await sleep(150); continue; }
    const result = await client.call('step');
    if (version !== generation || failed) return;
    context.putImageData(new ImageData(result.pixels, tile.width, tile.coreHeight), 0, tile.coreStart);
    if (!result.active) return;
  }
}

async function refreshScene() {
  if (!ready || failed) return;
  dirty = true;
  generation++;
  if (refreshing) return;
  refreshing = true;
  try {
    // At most one scene update in flight. Rapid drags replace stale updates.
    while (dirty && !failed) {
      dirty = false;
      const version = generation;
      const preview = pointer !== null;
      const [width, height] = dimensions(preview);
      const positions = [0, 1, 2, 3].map(i => control.object_position(i));
      if (canvas.width !== width || canvas.height !== height) {
        // Preserve the last image while the new resolution starts rendering.
        const old = document.createElement('canvas');
        old.width = canvas.width; old.height = canvas.height;
        old.getContext('2d').drawImage(canvas, 0, 0);
        canvas.width = width; canvas.height = height;
        context.drawImage(old, 0, 0, width, height);
      }
      const tiles = clients.map((client, i) => {
        const coreStart = Math.floor(i * height / clients.length);
        const coreEnd = Math.floor((i + 1) * height / clients.length);
        const start = Math.max(0, coreStart - 1);
        const end = Math.min(height, coreEnd + 1);
        return { width, fullHeight: height, start, height: Math.max(1, end - start), coreStart, coreHeight: coreEnd - coreStart };
      });
      await Promise.all(clients.map((client, i) => client.call('resize', { tile: tiles[i], positions, preview })));
      if (version !== generation || failed) continue;
      for (let i = 0; i < clients.length; i++) {
        if (tiles[i].coreHeight) renderBand(clients[i], tiles[i], version).catch(fail);
      }
    }
  } finally { refreshing = false; }
}

function coordinates(event) {
  const rect = canvas.getBoundingClientRect();
  return [(event.clientX - rect.left) / rect.width, (event.clientY - rect.top) / rect.height, rect.width / rect.height];
}

canvas.addEventListener('pointerdown', event => {
  if (!ready || failed || pointer !== null || event.button !== 0) return;
  if (!control.begin_drag(...coordinates(event))) return;
  event.preventDefault();
  pointer = event.pointerId;
  canvas.setPointerCapture(pointer);
  canvas.style.cursor = 'grabbing';
  refreshScene().catch(fail);
});
canvas.addEventListener('pointermove', event => {
  if (event.pointerId !== pointer || !control.drag_to(...coordinates(event))) return;
  event.preventDefault();
  // Coalesce input instead of flooding every worker with obsolete geometry.
  if (!movementTimer) movementTimer = setTimeout(() => {
    movementTimer = null;
    refreshScene().catch(fail);
  }, 50);
});
function finishDrag(event) {
  if (pointer === null || (event?.pointerId !== undefined && event.pointerId !== pointer)) return;
  if (event?.type === 'pointerup') control.drag_to(...coordinates(event));
  control.end_drag();
  const released = pointer;
  pointer = null;
  if (canvas.hasPointerCapture(released)) canvas.releasePointerCapture(released);
  canvas.style.cursor = 'grab';
  clearTimeout(movementTimer); movementTimer = null;
  refreshScene().catch(fail);
}
canvas.addEventListener('pointerup', finishDrag);
canvas.addEventListener('pointercancel', finishDrag);
canvas.addEventListener('lostpointercapture', finishDrag);
window.addEventListener('blur', () => finishDrag());
window.addEventListener('resize', () => {
  generation++;
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => refreshScene().catch(fail), 80);
});
window.addEventListener('pagehide', event => {
  if (event.persisted) return;
  generation++; failed = true;
  clearTimeout(movementTimer); clearTimeout(resizeTimer);
  for (const client of clients) client.close();
});

async function start() {
  if (!context) throw new Error('Canvas 2D is unavailable.');
  const module = await loadModule();
  control = (await WebAssembly.instantiate(module, {})).exports;
  const count = Math.max(1, Math.min(4, (navigator.hardwareConcurrency || 2) - 1));
  for (let i = 0; i < count; i++) clients.push(new RendererWorker());
  await Promise.all(clients.map(client => client.call('init', { module })));
  ready = true;
  await refreshScene();
}
start().catch(fail);
