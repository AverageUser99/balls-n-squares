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
    assert.equal(w.begin_drag(0.5, 0.84, 1.6), 3);
    const pyramidX = w.object_position(4);
    assert.equal(w.drag_to(0.54, 0.84, 1.6), 1);
    assert.notEqual(w.object_position(4), pyramidX);
    const worker = new WebAssembly.Instance(module, {}).exports;
    worker.set_objects(...Array.from({ length: 6 }, (_, i) => w.object_position(i)));
    assert.equal(worker.object_position(4), w.object_position(4));
    assert.equal(worker.object_position(5), w.object_position(5));
    w.end_drag();
    const initial = Array.from({ length: 5 }, (_, i) => w.camera_value(i));
    w.look_camera(100, -40);
    assert.notEqual(w.camera_value(3), initial[3]);
    assert.equal(w.move_camera(0, 1, 0, 0.05), 1);
    assert.notEqual(w.camera_value(2), initial[2]);
    w.set_camera(0, 2.45, 8.6, 0, initial[4]);
    assert.equal(w.begin_drag(0.32, 0.69, 1.6), 1); // picking follows moved geometry
    w.end_drag();
    w.reset_camera();
    for (let i = 0; i < 5; i++) assert.ok(Math.abs(w.camera_value(i) - initial[i]) < 1e-6);
    // The camera can cross the former front, side, ceiling, and floor limits.
    w.set_camera(2.79, 4.69, 8.59, 0, 0);
    assert.equal(w.move_camera(1, -1, 1, 0.05), 1);
    assert.ok(w.camera_value(0) > 2.8);
    assert.ok(w.camera_value(1) > 4.7);
    assert.ok(w.camera_value(2) > 8.6);
    w.set_camera(0, 0.21, 0, 0, 0);
    assert.equal(w.move_camera(0, 0, -1, 0.05), 1);
    assert.ok(w.camera_value(1) < 0.2);
    w.reset_camera();
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
