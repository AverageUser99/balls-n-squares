// Deliberately small interaction smoke test; visual quality is user-tested.
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const { test } = require('node:test');

for (const file of ['light_room.wasm', 'light_room-simd.wasm']) {
  test(`${file}: pick, drag both objects, render a live preview`, () => {
    const module = new WebAssembly.Module(readFileSync(resolve(__dirname, '..', file)));
    const w = new WebAssembly.Instance(module, {}).exports;
    assert.equal(w.begin_drag(0.4, 0.675, 1.6), 1);
    const oldX = w.object_position(0);
    assert.equal(w.drag_to(0.32, 0.69, 1.6), 1);
    assert.notEqual(w.object_position(0), oldX);
    w.end_drag();
    assert.equal(w.begin_drag(0.60, 0.675, 1.6), 2);
    const oldCubeX = w.object_position(2);
    assert.equal(w.drag_to(0.68, 0.69, 1.6), 1);
    assert.notEqual(w.object_position(2), oldCubeX);
    w.end_drag();
    w.interactive(1);
    w.resize(96, 60);
    for (let i = 0; i < 8; i++) w.render(60);
    w.present();
    const pointer = w.pixels();
    const p = new Uint8Array(w.memory.buffer, pointer, 96 * 60 * 4);
    assert.ok(p.some((v, i) => i % 4 !== 3 && v > 128));
    assert.ok(p.every((v, i) => i % 4 !== 3 || v === 255));
  });
}
