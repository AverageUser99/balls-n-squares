"""Build scalar + SIMD WASM artifacts. No third-party Python packages required."""
from pathlib import Path
import argparse
import os
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent


def build(simd=False):
    env = os.environ.copy()
    target = ROOT / "target" / ("simd" if simd else "scalar")
    env["CARGO_TARGET_DIR"] = str(target)
    if simd:
        env["RUSTFLAGS"] = env.get("RUSTFLAGS", "") + " -C target-feature=+simd128"
    else:
        env["RUSTFLAGS"] = env.get("RUSTFLAGS", "") + " -C target-feature=-simd128"
    subprocess.run(
        ["cargo", "build", "--lib", "--release", "--target", "wasm32-unknown-unknown"],
        cwd=ROOT, env=env, check=True,
    )
    source = target / "wasm32-unknown-unknown/release/light_room.wasm"
    destination = ROOT / ("light_room-simd.wasm" if simd else "light_room.wasm")
    shutil.copyfile(source, destination)
    print(f"Built {destination.name} ({destination.stat().st_size:,} bytes)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site-dir", type=Path, help="Stage only public site assets for deployment")
    args = parser.parse_args()
    build()
    build(simd=True)
    subprocess.run(
        ["rustc", "--edition=2021", "--crate-type=cdylib", "--target=wasm32-unknown-unknown",
         "-C", "opt-level=s", "-C", "strip=symbols", "src/audio.rs", "-o", "music.wasm"],
        cwd=ROOT, check=True,
    )
    print(f"Built music.wasm ({(ROOT / 'music.wasm').stat().st_size:,} bytes)")
    if args.site_dir:
        destination = ROOT / args.site_dir
        destination.mkdir(parents=True, exist_ok=True)
        for name in ["index.html", "app.js", "audio.js", "worker.js", "light_room.wasm", "light_room-simd.wasm", "music.wasm"]:
            shutil.copyfile(ROOT / name, destination / name)
        (destination / ".nojekyll").touch()
        print(f"Staged static site in {destination}")
