// Each worker returns its band in a single pass. There is no idle refinement.
let wasm;
self.onmessage = async ({ data: message }) => {
  const { id, type } = message;
  try {
    if (type === 'init') {
      wasm = (await WebAssembly.instantiate(message.module, {})).exports;
      self.postMessage({ id, ok: true });
    } else if (type === 'render') {
      const { width, fullHeight, coreStart, coreHeight } = message.tile;
      wasm.set_objects(...message.positions);
      wasm.set_camera(...message.camera);
      wasm.interactive(1);
      wasm.resize_tile(width, fullHeight, coreStart, coreHeight);
      wasm.render(coreHeight);
      const pixels = new Uint8ClampedArray(wasm.memory.buffer, wasm.pixels(), width * coreHeight * 4).slice();
      self.postMessage({ id, ok: true, pixels }, [pixels.buffer]);
    } else {
      throw new Error(`Unknown worker request: ${type}`);
    }
  } catch (error) {
    self.postMessage({ id, ok: false, error: String(error.stack || error) });
  }
};
