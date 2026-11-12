# Balls n Squares

**[Play the live scene](https://averageuser99.github.io/balls-n-squares/)** · [Source](https://github.com/AverageUser99/balls-n-squares)

A Rust/WASM CPU ray tracer: room, mirror sphere, cube, glowing triangular pyramid, area light, soft shadows,
and bounced light. A small overlay shows keyboard shortcuts; there is no GPU/shader renderer.

## Run

The scalar and SIMD WASM binaries are already built:

```sh
python -m http.server 8000
```

Open **http://localhost:8000**. Music loops automatically when browser settings allow it;
otherwise it starts after your first click, tap, or keypress. Press **M** to mute/unmute.
**Click/touch and drag the sphere, cube, or glowing pyramid** to move
it across the floor. Objects can move beyond the room, but cannot pass through each other.
Release to refine the image. Dragging up/down changes depth, not object height.

### Camera (desktop)

- Press **C** to unlock or lock the camera. It starts locked, so object dragging works as before.
- When unlocked, **drag anywhere** to look around. **W/A/S/D** move horizontally;
  **Q/E** move down/up. Release the keys or pointer and the image refines.
- Press **R** to reset the camera, without moving the objects. Lock with **C** to drag objects
  again from the new viewpoint.
- On touch devices, unlocked-camera dragging controls looking; translation currently requires
  a keyboard. The camera can move freely through the room and objects. The room surfaces
  remain in place, so objects beyond a wall may be hidden and empty space can look black.

## Performance cuts

- No startup bake, lighting download, or stale static-lighting maps. Indirect light
  is traced against the current object positions, including inside mirror reflections.
- Lower-resolution, two-surface-bounce preview while dragging or moving the camera; up to four surface
  interactions while stationary (plus the deterministic mirror step).
- Preview stops at 32 samples; stationary rendering refines up to 256, with denoising
  and adaptive stopping. Some noise is intentional; smoothness improves after release.
- Input is coalesced, worker updates are synchronized, and obsolete images are discarded.
- Static camera hits are reused between edits, but invalidated whenever objects or camera move.
  Buffers are reused rather than reallocated on every drag.
- Up to four independent WASM workers, SIMD with scalar fallback, specialized shadow
  tests, Sobol light sampling, and lookup-table display conversion remain enabled.
- Preview is capped at 36,864 pixels/320 long edge; refinement at 147,456 pixels/640.

All picking, floor dragging, collision constraints, tracing, and filtering math executes
in WASM. JavaScript handles pointer events, worker messages, and displaying RGBA pixels.
Plain Python HTTP serving works without cross-origin isolation headers or dependencies.

## Music credit

*Clair de lune*, composed by Claude Debussy, performed by **Laurens Goedhart** (2011).
The recording is licensed under [Creative Commons Attribution 3.0](https://creativecommons.org/licenses/by/3.0/).
[Recording and source](https://commons.wikimedia.org/wiki/File:Clair_de_lune_(Claude_Debussy)_Suite_bergamasque.ogg).
This project uses Wikimedia Commons' MP3 transcode of the original Ogg recording;
the audio is embedded unchanged into a WASM module.

## Rebuild

```sh
rustup target add wasm32-unknown-unknown
python build.py
```

Rust and Python only; no light bake is needed. The build embeds the bundled recording
in `music.wasm`, so there is no third-party request at runtime. Web Audio decodes the
MP3 and plays it on a continuous loop; audio may require a user gesture due to browser
autoplay restrictions. For a quick interaction smoke test:

```sh
node --test tests/wasm.test.cjs
```

Visual quality and interaction feel are intentionally left to user testing.

## GitHub Pages

Pushing to `main` runs `.github/workflows/pages.yml`. It compiles both WASM variants,
performs the quick interaction smoke test, and publishes only the runtime assets
from `dist/` to GitHub Pages using the `github-pages` environment. No secrets or external
hosting are needed. Repository Settings → Pages uses **GitHub Actions** as its source.

To stage the same deployment locally:

```sh
python build.py --site-dir dist
python -m http.server 8000 --directory dist
```
