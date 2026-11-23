import "./audio.js";

// Browser plumbing/input only. Picking, dragging and rendering math live in WASM.
const canvas = document.querySelector("canvas");
const context = canvas.getContext("2d", { alpha: false });
// Render work is dispatched by the animation loop, not by input events.
const bounceSlider = document.querySelector("#bounces");
const cycleSlider = document.querySelector("#cycles");
const clients = [];
let control;
let ready = false;
let failed = false;
let refreshing = false;
let dirty = false;
for (const slider of [bounceSlider, cycleSlider]) {
  const output = document.querySelector(`output[for="${slider.id}"]`);
  const update = () => {
    output.value = slider.value;
    dirty = true;
  };
  slider.addEventListener("input", update);
  update();
}
let pointer = null;
let resizeTimer;
let cameraUnlocked = false;
let looking = false;
let lastPointer;
const keys = new Set();

function fail(error) {
  if (failed) return;
  failed = true;
  for (const client of clients) client.close();
  console.error("WASM scene failed:", error);
}

class RendererWorker {
  constructor() {
    this.worker = new Worker("./worker.js");
    this.next = 0;
    this.pending = new Map();
    this.worker.onmessage = ({ data }) => {
      const request = this.pending.get(data.id);
      if (!request) return;
      this.pending.delete(data.id);
      if (data.ok) request.resolve(data);
      else request.reject(new Error(data.error));
    };
    this.worker.onerror = (event) =>
      fail(new Error(event.message || "Renderer worker failed."));
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
    for (const request of this.pending.values())
      request.reject(new Error("Renderer stopped."));
    this.pending.clear();
  }
}

async function loadModule() {
  for (const file of ["./light_room-simd.wasm", "./light_room.wasm"]) {
    try {
      const response = await fetch(file);
      if (!response.ok) throw new Error(`${file}: HTTP ${response.status}`);
      return await WebAssembly.compile(await response.arrayBuffer());
    } catch (error) {
      if (file === "./light_room.wasm") throw error;
    }
  }
}

function dimensions() {
  const w = Math.max(1, window.innerWidth);
  const h = Math.max(1, window.innerHeight);
  const edge = 320;
  const budget = 36864;
  const scale = Math.min(1, edge / w, edge / h, Math.sqrt(budget / (w * h)));
  return [
    Math.max(1, Math.round(w * scale)),
    Math.max(1, Math.round(h * scale)),
  ];
}

function snapshot() {
  const [width, height] = dimensions();
  const positions = Array.from({ length: 6 }, (_, i) =>
    control.object_position(i),
  );
  const camera = Array.from({ length: 5 }, (_, i) => control.camera_value(i));
  const count = Math.min(clients.length, height);
  const bands = clients.slice(0, count).map((client, i) => {
    const coreStart = Math.floor((i * height) / count);
    const coreEnd = Math.floor(((i + 1) * height) / count);
    const tile = {
      width,
      fullHeight: height,
      coreStart,
      coreHeight: coreEnd - coreStart,
    };
    return { client, tile };
  });
  return { width, height, positions, camera, bands };
}

async function renderFrame() {
  if (!ready || failed || refreshing || document.hidden) return;
  refreshing = true;
  try {
    dirty = false;
    const { width, height, positions, camera, bands } = snapshot();
    const results = await Promise.all(
      bands.map(({ client, tile }) =>
        client.call("render", {
          tile,
          positions,
          camera,
          cycles: cycleSlider.valueAsNumber,
          bounces: bounceSlider.valueAsNumber,
        }),
      ),
    );
    // Finish motion frames even when input outruns rendering. The next snapshot
    // picks up the latest pose instead of repeatedly cancelling unseen frames.
    if (failed) return;
    // Resize and paint together in one task: the compositor sees a whole pose.
    if (canvas.width !== width) canvas.width = width;
    if (canvas.height !== height) canvas.height = height;
    bands.forEach(({ tile }, i) => {
      context.putImageData(
        new ImageData(results[i].pixels, width, tile.coreHeight),
        0,
        tile.coreStart,
      );
    });
  } finally {
    refreshing = false;
  }
}

function coordinates(event) {
  const rect = canvas.getBoundingClientRect();
  return [
    (event.clientX - rect.left) / rect.width,
    (event.clientY - rect.top) / rect.height,
    rect.width / rect.height,
  ];
}

canvas.addEventListener("pointerdown", (event) => {
  if (!ready || failed || pointer !== null || event.button !== 0) return;
  if (cameraUnlocked) {
    event.preventDefault();
    looking = true;
    lastPointer = [event.clientX, event.clientY];
    canvas.setPointerCapture(event.pointerId);
    pointer = event.pointerId;
    canvas.style.cursor = "grabbing";
    return;
  }
  if (!control.begin_drag(...coordinates(event))) return;
  event.preventDefault();
  pointer = event.pointerId;
  canvas.setPointerCapture(pointer);
  canvas.style.cursor = "grabbing";
});
canvas.addEventListener("pointermove", (event) => {
  if (event.pointerId !== pointer) return;
  if (looking) {
    control.look_camera(
      event.clientX - lastPointer[0],
      event.clientY - lastPointer[1],
    );
    lastPointer = [event.clientX, event.clientY];
  } else if (!control.drag_to(...coordinates(event))) return;
  event.preventDefault();
  dirty = true;
});
function finishDrag(event) {
  if (
    pointer === null ||
    (event?.pointerId !== undefined && event.pointerId !== pointer)
  )
    return;
  if (
    !looking &&
    event?.type === "pointerup" &&
    control.drag_to(...coordinates(event))
  )
    dirty = true;
  if (!looking) control.end_drag();
  looking = false;
  const released = pointer;
  pointer = null;
  if (canvas.hasPointerCapture(released))
    canvas.releasePointerCapture(released);
  canvas.style.cursor = cameraUnlocked ? "crosshair" : "grab";
}
canvas.addEventListener("pointerup", finishDrag);
canvas.addEventListener("pointercancel", finishDrag);
canvas.addEventListener("lostpointercapture", finishDrag);
window.addEventListener("keydown", (event) => {
  if (!ready || failed || event.altKey || event.ctrlKey || event.metaKey)
    return;
  if (event.code === "KeyC" && !event.repeat) {
    finishDrag();
    cameraUnlocked = !cameraUnlocked;
    keys.clear();
    canvas.style.cursor = cameraUnlocked ? "crosshair" : "grab";
    event.preventDefault();
  } else if (event.code === "KeyR" && !event.repeat) {
    finishDrag();
    control.reset_camera();
    dirty = true;
    event.preventDefault();
  } else if (
    cameraUnlocked &&
    ["KeyW", "KeyA", "KeyS", "KeyD", "KeyQ", "KeyE"].includes(event.code)
  ) {
    keys.add(event.code);
    event.preventDefault();
  }
});
window.addEventListener("keyup", (event) => keys.delete(event.code));
let previousFrame = 0;
function cameraFrame(now) {
  const dt = previousFrame ? Math.min((now - previousFrame) / 1000, 0.05) : 0;
  previousFrame = now;
  if (ready && cameraUnlocked && !document.hidden && keys.size) {
    const axis = (positive, negative) =>
      Number(keys.has(positive)) - Number(keys.has(negative));
    if (
      control.move_camera(
        axis("KeyD", "KeyA"),
        axis("KeyW", "KeyS"),
        axis("KeyE", "KeyQ"),
        dt,
      )
    )
      dirty = true;
  }
  if (dirty) renderFrame().catch(fail);
  if (!failed) requestAnimationFrame(cameraFrame);
}
requestAnimationFrame(cameraFrame);
window.addEventListener("blur", () => {
  keys.clear();
  finishDrag();
});
document.addEventListener("visibilitychange", () => {
  if (document.hidden) keys.clear();
});
window.addEventListener("resize", () => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    dirty = true;
  }, 80);
});
window.addEventListener("pagehide", (event) => {
  if (event.persisted) return;
  failed = true;
  clearTimeout(resizeTimer);
  for (const client of clients) client.close();
});

async function start() {
  if (!context) throw new Error("Canvas 2D is unavailable.");
  const module = await loadModule();
  control = (await WebAssembly.instantiate(module, {})).exports;
  const count = Math.max(
    1,
    Math.min(4, (navigator.hardwareConcurrency || 2) - 1),
  );
  for (let i = 0; i < count; i++) clients.push(new RendererWorker());
  await Promise.all(clients.map((client) => client.call("init", { module })));
  ready = true;
  dirty = true;
}
start().catch(fail);
