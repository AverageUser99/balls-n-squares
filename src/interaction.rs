//! Picking and floor-plane dragging run in WASM, not JavaScript.
use crate::{
    math::Vec3,
    scene::{self, *},
};
use std::cell::Cell;

pub const EYE: Vec3 = Vec3::new(0.0, 2.45, 8.6);
pub fn camera_ray(u: f32, v: f32, aspect: f32) -> Vec3 {
    let half = 0.40 / aspect.min(1.0).max(0.05);
    let x = (u * 2.0 - 1.0) * aspect * half;
    let y = (1.0 - v * 2.0) * half;
    Vec3::new(
        x,
        -0.022216737 + 0.9997532 * y,
        -0.9997532 - 0.022216737 * y,
    )
    .normalized()
}

#[derive(Clone, Copy)]
struct Drag {
    index: usize,
    y: f32,
    offset_x: f32,
    offset_z: f32,
}
thread_local! { static DRAG: Cell<Option<Drag>> = const { Cell::new(None) }; }

pub fn begin(u: f32, v: f32, aspect: f32) -> u32 {
    end();
    let hit = trace(EYE, camera_ray(u, v, aspect));
    let (object, index) = match hit.surface {
        MIRROR => (1, 0),
        5..=10 => (2, 2),
        _ => return 0,
    };
    let p = positions();
    DRAG.with(|s| {
        s.set(Some(Drag {
            index,
            y: hit.point.y,
            offset_x: p[index] - hit.point.x,
            offset_z: p[index + 1] - hit.point.z,
        }))
    });
    object
}

fn clear(p: [f32; 4]) -> bool {
    // Conservative sphere/rotated-box footprint collision keeps both objects separate.
    let q = rotate(Vec3::new(p[0] - p[2], 0.0, p[1] - p[3]), true);
    let dx = q.x - q.x.clamp(-0.78, 0.78);
    let dz = q.z - q.z.clamp(-0.78, 0.78);
    dx * dx + dz * dz >= 1.02 * 1.02
}

pub fn update(u: f32, v: f32, aspect: f32) -> bool {
    let Some(drag) = DRAG.with(Cell::get) else {
        return false;
    };
    let direction = camera_ray(u, v, aspect);
    if direction.y >= -0.005 {
        return false;
    }
    let t = (drag.y - EYE.y) / direction.y;
    if t <= 0.0 {
        return false;
    }
    let target = EYE + direction * t;
    let old = positions();
    let x = (target.x + drag.offset_x).clamp(-1.99, 1.99);
    let z = (target.z + drag.offset_z).clamp(-1.99, 2.8);
    let candidate = |fraction: f32| {
        let mut p = old;
        p[drag.index] += (x - p[drag.index]) * fraction;
        p[drag.index + 1] += (z - p[drag.index + 1]) * fraction;
        p
    };
    // Sweep rather than testing only the endpoint, so fast drags cannot tunnel.
    let mut fraction = 1.0;
    for step in 1..=16 {
        let f = step as f32 / 16.0;
        if !clear(candidate(f)) {
            let (mut lo, mut hi) = ((step - 1) as f32 / 16.0, f);
            for _ in 0..8 {
                let mid = (lo + hi) * 0.5;
                if clear(candidate(mid)) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            fraction = lo;
            break;
        }
    }
    let p = candidate(fraction);
    if (p[drag.index] - old[drag.index]).abs() + (p[drag.index + 1] - old[drag.index + 1]).abs()
        < 0.0001
    {
        return false;
    }
    scene::set_positions(p);
    true
}

pub fn end() {
    DRAG.with(|s| s.set(None));
}
