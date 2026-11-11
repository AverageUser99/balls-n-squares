//! Picking and floor-plane dragging run in WASM, not JavaScript.
use crate::{
    math::Vec3,
    scene::{self, *},
};
use std::cell::Cell;

const INITIAL_CAMERA: [f32; 5] = [0.0, 2.45, 8.6, 0.0, -0.02221856];
#[derive(Clone, Copy)]
struct Camera {
    values: [f32; 5],
    forward: Vec3,
    right: Vec3,
    up: Vec3,
}
impl Camera {
    fn new(values: [f32; 5]) -> Self {
        let (sy, cy) = values[3].sin_cos();
        let (sp, cp) = values[4].sin_cos();
        Self {
            values,
            forward: Vec3::new(sy * cp, sp, -cy * cp),
            right: Vec3::new(cy, 0.0, sy),
            up: Vec3::new(-sy * sp, cp, cy * sp),
        }
    }
}
thread_local! { static CAMERA: Cell<Camera> = Cell::new(Camera::new(INITIAL_CAMERA)); }
pub fn camera_values() -> [f32; 5] {
    CAMERA.with(|c| c.get().values)
}
pub fn set_camera(mut values: [f32; 5]) {
    if !values.iter().all(|v| v.is_finite()) {
        return;
    }
    values[4] = values[4].clamp(-1.48, 1.48);
    CAMERA.with(|c| c.set(Camera::new(values)));
}
pub fn reset_camera() {
    set_camera(INITIAL_CAMERA);
}
pub fn camera_eye() -> Vec3 {
    let p = camera_values();
    Vec3::new(p[0], p[1], p[2])
}
pub fn camera_ray(u: f32, v: f32, aspect: f32) -> Vec3 {
    let half = 0.40 / aspect.clamp(0.05, 1.0);
    let x = (u * 2.0 - 1.0) * aspect * half;
    let y = (1.0 - v * 2.0) * half;
    CAMERA.with(|camera| {
        let camera = camera.get();
        (camera.forward + camera.right * x + camera.up * y).normalized()
    })
}
pub fn look(dx: f32, dy: f32) {
    if !dx.is_finite() || !dy.is_finite() {
        return;
    }
    let mut p = camera_values();
    p[3] = (p[3] + dx * 0.004).rem_euclid(std::f32::consts::TAU);
    p[4] -= dy * 0.004;
    set_camera(p);
}
pub fn move_camera(strafe: f32, forward: f32, vertical: f32, dt: f32) -> bool {
    if ![strafe, forward, vertical, dt]
        .iter()
        .all(|v| v.is_finite())
    {
        return false;
    }
    let old = camera_values();
    let mut p = old;
    let (sin, cos) = p[3].sin_cos();
    let length = (strafe * strafe + forward * forward + vertical * vertical)
        .sqrt()
        .max(1.0);
    let speed = 2.5 * dt.clamp(0.0, 0.05) / length;
    let dx = (strafe * cos + forward * sin) * speed;
    let dz = (strafe * sin - forward * cos) * speed;
    p[0] += dx;
    p[1] += vertical * speed;
    p[2] += dz;
    if p == old {
        return false;
    }
    set_camera(p);
    true
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
    let hit = trace(camera_eye(), camera_ray(u, v, aspect));
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
    if direction.y.abs() < 0.005 {
        return false;
    }
    let eye = camera_eye();
    let t = (drag.y - eye.y) / direction.y;
    if t <= 0.0 {
        return false;
    }
    let target = eye + direction * t;
    let old = positions();
    let x = target.x + drag.offset_x;
    let z = target.z + drag.offset_z;
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
