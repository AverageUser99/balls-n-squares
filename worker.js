// Resolve every sample for this pose before returning a single display-ready band.
let wasm;
self.onmessage = async ({ data: message }) => {
  const { id, type } = message;
  try {
    if (type === "init") {
      wasm = (await WebAssembly.instantiate(message.module, {})).exports;
      self.postMessage({ id, ok: true });
    } else if (type === "render") {
      const { width, fullHeight, coreStart, coreHeight } = message.tile;
      const { cycles } = message;
      // The still-frame denoiser reads one neighbor row on either side.
      const start = cycles ? Math.max(0, coreStart - 1) : coreStart;
      const end = Math.min(
        fullHeight,
        coreStart + coreHeight + (cycles ? 1 : 0),
      );
      wasm.set_bounces(message.bounces);
      wasm.set_objects(...message.positions);
      wasm.set_camera(...message.camera);
      wasm.interactive(cycles ? 0 : 1);
      wasm.resize_tile(width, fullHeight, start, end - start);
      for (
        let sample = 0;
        sample < (cycles ? 8 * cycles : 1) && wasm.active_pixels();
        sample++
      ) {
        wasm.render(end - start);
      }
      if (cycles) wasm.present();
      const offset = (coreStart - start) * width * 4;
      const pixels = new Uint8ClampedArray(
        wasm.memory.buffer,
        wasm.pixels() + offset,
        width * coreHeight * 4,
      ).slice();
      self.postMessage({ id, ok: true, pixels }, [pixels.buffer]);
    } else {
      throw new Error(`Unknown worker request: ${type}`);
    }
  } catch (error) {
    self.postMessage({ id, ok: false, error: String(error.stack || error) });
  }
};
