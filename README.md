# Balls n Squares

**[Play the live scene](https://averageuser99.github.io/balls-n-squares/)** · [Source](https://github.com/AverageUser99/balls-n-squares)

A Rust/WASM CPU ray tracer: room, mirror sphere, cube, area light, soft shadows,
and bounced light. No visible text or controls; no GPU/shader renderer.

## Run

The scalar and SIMD WASM binaries are already built:

```sh
python -m http.server 8000
```

Open **http://localhost:8000**. **Click/touch and drag the sphere or cube** to move
it across the floor. Objects stay inside the room and cannot pass through each other.
Release to refine the image. Dragging up/down changes depth, not object height.

## Performance cuts

- No startup bake, lighting download, or stale static-lighting maps. Indirect light
  is traced against the current object positions, including inside mirror reflections.
- Lower-resolution, two-surface-bounce preview while dragging; up to four surface
  interactions while stationary (plus the deterministic mirror step).
- Preview stops at 32 samples; stationary rendering refines up to 256, with denoising
  and adaptive stopping. Some noise is intentional; smoothness improves after release.
- Input is coalesced, worker updates are synchronized, and obsolete images are discarded.
- Static camera hits are reused between edits, but invalidated whenever objects move.
  Buffers are reused rather than reallocated on every drag.
- Up to four independent WASM workers, SIMD with scalar fallback, specialized shadow
  tests, Sobol light sampling, and lookup-table display conversion remain enabled.
- Preview is capped at 36,864 pixels/320 long edge; refinement at 147,456 pixels/640.

All picking, floor dragging, collision constraints, tracing, and filtering math executes
in WASM. JavaScript handles pointer events, worker messages, and displaying RGBA pixels.
Plain Python HTTP serving works without cross-origin isolation headers or dependencies.

## Rebuild

```sh
rustup target add wasm32-unknown-unknown
python build.py
```

Rust and Python only; no light bake is needed. For a quick interaction smoke test:

```sh
node --test tests/wasm.test.cjs
```

Visual quality and interaction feel are intentionally left to user testing.

## GitHub Pages

Pushing to `main` runs `.github/workflows/pages.yml`. It compiles both WASM variants,
performs the quick interaction smoke test, and publishes only the five runtime assets
from `dist/` to GitHub Pages using the `github-pages` environment. No secrets or external
hosting are needed. Repository Settings → Pages uses **GitHub Actions** as its source.

To stage the same deployment locally:

```sh
python build.py --site-dir dist
python -m http.server 8000 --directory dist
```
