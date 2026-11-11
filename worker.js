// Independent WASM instances: no SharedArrayBuffer or special server headers.
let wasm;
let tile;
let preview = false;
self.onmessage = async ({ data: message }) => {
  const { id, type } = message;
  try {
    let result = {};
    const transfer = [];
    if (type === 'init') {
      wasm = (await WebAssembly.instantiate(message.module, {})).exports;
    } else if (type === 'resize') {
      tile = message.tile;
      preview = !!message.preview;
      wasm.set_objects(...message.positions);
      wasm.interactive(preview ? 1 : 0);
      wasm.resize_tile(tile.width, tile.fullHeight, tile.start, tile.height);
    } else if (type === 'step') {
      const before = wasm.sample_count();
      const deadline = performance.now() + (preview ? 8 : 16);
      // Always complete the first pass: no black scan-line wipe after a scene edit.
      if (before === 0) wasm.render(tile.height);
      while (performance.now() < deadline && wasm.active_pixels() && wasm.sample_count() < before + 8) wasm.render(2);
      wasm.present();
      const offset = (tile.coreStart - tile.start) * tile.width * 4;
      const length = tile.coreHeight * tile.width * 4;
      const pixels = new Uint8ClampedArray(wasm.memory.buffer, wasm.pixels() + offset, length).slice();
      result = { pixels, active: wasm.active_pixels() };
      transfer.push(pixels.buffer);
    } else { throw new Error(`Unknown worker request: ${type}`); }
    self.postMessage({ id, ok: true, ...result }, transfer);
  } catch (error) { self.postMessage({ id, ok: false, error: String(error.stack || error) }); }
};
