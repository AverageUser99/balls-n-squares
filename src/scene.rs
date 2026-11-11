use crate::math::Vec3;
use std::cell::Cell;

pub const EPS: f32 = 0.001;
pub const LIGHT_Y: f32 = 4.98;
pub const LIGHT: Vec3 = Vec3::new(14.0, 12.8, 10.5);
pub const TINT: Vec3 = Vec3::new(0.94, 0.96, 0.98);
pub const EMITTER: usize = 11;
pub const MIRROR: usize = 12;
pub const MISS: usize = 13;
pub const SURFACES: usize = 11;
thread_local! {
    static OBJECTS: Cell<[f32; 4]> = const { Cell::new([-1.08, 0.05, 1.08, -0.7]) };
}
pub fn positions() -> [f32; 4] {
    OBJECTS.with(Cell::get)
}
pub fn set_positions(p: [f32; 4]) {
    if p.iter().all(|v| v.is_finite()) {
        OBJECTS.with(|s| s.set(p));
    }
}
fn sphere_center() -> Vec3 {
    let p = positions();
    Vec3::new(p[0], 1.0, p[1])
}
fn cube_center() -> Vec3 {
    let p = positions();
    Vec3::new(p[2], 0.9, p[3])
}
const AXES: [Vec3; 3] = [
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(0.0, 1.0, 0.0),
    Vec3::new(0.0, 0.0, 1.0),
];
const EXTENTS: [f32; 3] = [0.78, 0.9, 0.78];

pub const fn rotate(v: Vec3, inverse: bool) -> Vec3 {
    let s = if inverse { -0.29552022 } else { 0.29552022 };
    let c = 0.9553365;
    Vec3::new(c * v.x + s * v.z, v.y, -s * v.x + c * v.z)
}

pub fn normal(id: usize) -> Vec3 {
    if id < SURFACES {
        FRAMES[id].2
    } else {
        Vec3::default()
    }
}

pub fn albedo(id: usize) -> Vec3 {
    match id {
        0 => Vec3::new(0.73, 0.71, 0.65),
        1 => Vec3::new(0.75, 0.74, 0.69),
        2 => Vec3::new(0.74, 0.73, 0.68),
        3 => Vec3::new(0.65, 0.075, 0.055),
        4 => Vec3::new(0.055, 0.43, 0.36),
        5..=10 => Vec3::new(0.72, 0.53, 0.23),
        _ => Vec3::default(),
    }
}

// Construct all face frames at compile time, including the rotated cube faces.
const fn frames() -> [(Vec3, Vec3, Vec3); SURFACES] {
    let zero = Vec3::new(0.0, 0.0, 0.0);
    let room = [
        AXES[1],
        Vec3::new(0.0, -1.0, 0.0),
        AXES[2],
        AXES[0],
        Vec3::new(-1.0, 0.0, 0.0),
    ];
    let mut result = [(zero, zero, zero); SURFACES];
    let mut id = 0;
    while id < SURFACES {
        let n = if id < 5 {
            room[id]
        } else {
            let axis = AXES[(id - 5) / 2];
            let sign = if (id - 5) % 2 == 0 { -1.0 } else { 1.0 };
            rotate(
                Vec3::new(axis.x * sign, axis.y * sign, axis.z * sign),
                false,
            )
        };
        let t = if id < 2 {
            AXES[0]
        } else if id < 5 {
            AXES[1]
        } else {
            rotate(AXES[((id - 5) / 2 + 1) % 3], false)
        };
        let b = Vec3::new(
            n.y * t.z - n.z * t.y,
            n.z * t.x - n.x * t.z,
            n.x * t.y - n.y * t.x,
        );
        result[id] = (t, b, n);
        id += 1;
    }
    result
}
const FRAMES: [(Vec3, Vec3, Vec3); SURFACES] = frames();
pub fn basis(id: usize) -> (Vec3, Vec3, Vec3) {
    FRAMES[id]
}

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: f32,
    pub point: Vec3,
    pub n: Vec3,
    pub surface: usize,
}
impl Hit {
    pub fn empty() -> Self {
        Self {
            t: f32::INFINITY,
            point: Vec3::default(),
            n: Vec3::default(),
            surface: MISS,
        }
    }
}

// This version accepts unnormalized directions too (shadow segments use t in 0..1).
fn sphere_distance(o: Vec3, d: Vec3) -> f32 {
    let r = o - sphere_center();
    let a = d.dot(d);
    let b = r.dot(d);
    let discriminant = b * b - a * (r.dot(r) - 1.0);
    if discriminant < 0.0 {
        return f32::INFINITY;
    }
    let root = discriminant.sqrt();
    let near = (-b - root) / a;
    if near > 1e-5 {
        near
    } else {
        (-b + root) / a
    }
}

fn box_interval(o: Vec3, d: Vec3) -> (f32, usize) {
    let o = rotate(o - cube_center(), true);
    let d = rotate(d, true);
    let origins = [o.x, o.y, o.z];
    let directions = [d.x, d.y, d.z];
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    let mut near_face = 0;
    let mut far_face = 0;
    for axis in 0..3 {
        let (o, d, e) = (origins[axis], directions[axis], EXTENTS[axis]);
        if d.abs() < 1e-8 {
            if o.abs() > e {
                return (f32::INFINITY, 0);
            }
            continue;
        }
        let reciprocal = 1.0 / d;
        let a = (-e - o) * reciprocal;
        let b = (e - o) * reciprocal;
        let (entry, exit, side) = if a < b { (a, b, 0) } else { (b, a, 1) };
        if entry > near {
            near = entry;
            near_face = 5 + axis * 2 + side;
        }
        if exit < far {
            far = exit;
            far_face = 5 + axis * 2 + (1 - side);
        }
        if near > far {
            return (f32::INFINITY, 0);
        }
    }
    if near > 1e-5 {
        (near, near_face)
    } else {
        (far, far_face)
    }
}

pub fn trace(o: Vec3, d: Vec3) -> Hit {
    let mut h = Hit::empty();
    // Axis-aligned, inward-facing room planes: no generic dot products.
    let ts = [
        if d.y < -1e-6 {
            -o.y / d.y
        } else {
            f32::INFINITY
        },
        if d.y > 1e-6 {
            (LIGHT_Y - o.y) / d.y
        } else {
            f32::INFINITY
        },
        if d.z < -1e-6 {
            (-3.0 - o.z) / d.z
        } else {
            f32::INFINITY
        },
        if d.x < -1e-6 {
            (-3.0 - o.x) / d.x
        } else {
            f32::INFINITY
        },
        if d.x > 1e-6 {
            (3.0 - o.x) / d.x
        } else {
            f32::INFINITY
        },
    ];
    for (id, t) in ts.into_iter().enumerate() {
        if t > EPS && t < h.t {
            h.t = t;
            h.surface = id;
        }
    }
    let sphere = sphere_distance(o, d);
    if sphere > EPS && sphere < h.t {
        h.t = sphere;
        h.surface = MIRROR;
    }
    let (cube, face) = box_interval(o, d);
    if cube > EPS && cube < h.t {
        h.t = cube;
        h.surface = face;
    }
    if h.surface != MISS {
        h.point = o + d * h.t;
        h.n = if h.surface == MIRROR {
            (h.point - sphere_center()).normalized()
        } else {
            normal(h.surface)
        };
        if h.surface == 1 && h.point.x.abs() < 0.9 && (h.point.z + 0.65).abs() < 0.8 {
            h.surface = EMITTER;
        }
    }
    h
}

// No material/normal/room work, no normalized ray, and early-out on a blocker.
// A segment from any interior surface to the ceiling panel cannot cross a room wall.
pub fn occluded(o: Vec3, segment: Vec3) -> bool {
    let t = sphere_distance(o, segment);
    if t > 1e-5 && t < 0.9999 {
        return true;
    }
    let (t, _) = box_interval(o, segment);
    t > 1e-5 && t < 0.9999
}

pub fn direct(point: Vec3, id: usize, u: f32, v: f32) -> Vec3 {
    let o = point + normal(id) * EPS;
    let segment = Vec3::new((u - 0.5) * 1.8, LIGHT_Y, -0.65 + (v - 0.5) * 1.6) - o;
    let cos_product = normal(id).dot(segment).max(0.0) * segment.y.max(0.0);
    if cos_product <= 0.0 || occluded(o, segment) {
        return Vec3::default();
    }
    let d2 = segment.dot(segment);
    LIGHT * (cos_product * (2.88 / std::f32::consts::PI) / (d2 * d2))
}

pub fn resolution(id: usize) -> usize {
    if id < 5 {
        64
    } else {
        24
    }
}
pub fn offset(id: usize) -> usize {
    if id < 5 {
        id * 64 * 64
    } else {
        5 * 64 * 64 + (id - 5) * 24 * 24
    }
}
pub const TEXELS: usize = 5 * 64 * 64 + 6 * 24 * 24;

// Indirect-only lightmaps are isolated per face; direct shadows never use these maps.
pub fn surface_point(id: usize, u: f32, v: f32) -> Vec3 {
    let x = u * 6.0 - 3.0;
    let z = v * 13.0 - 3.0;
    match id {
        0 => Vec3::new(x, 0.0, z),
        1 => Vec3::new(x, LIGHT_Y, z),
        2 => Vec3::new(x, v * LIGHT_Y, -3.0),
        3 => Vec3::new(-3.0, u * LIGHT_Y, z),
        4 => Vec3::new(3.0, u * LIGHT_Y, z),
        _ => {
            let axis = (id - 5) / 2;
            let mut p = [0.0; 3];
            p[axis] = EXTENTS[axis]
                * if (id - 5).is_multiple_of(2) {
                    -1.0
                } else {
                    1.0
                };
            p[(axis + 1) % 3] = (u * 2.0 - 1.0) * EXTENTS[(axis + 1) % 3];
            p[(axis + 2) % 3] = (v * 2.0 - 1.0) * EXTENTS[(axis + 2) % 3];
            cube_center() + rotate(Vec3::new(p[0], p[1], p[2]), false)
        }
    }
}

pub fn surface_uv(id: usize, p: Vec3) -> (f32, f32) {
    match id {
        0..=1 => ((p.x + 3.0) / 6.0, (p.z + 3.0) / 13.0),
        2 => ((p.x + 3.0) / 6.0, p.y / LIGHT_Y),
        3..=4 => (p.y / LIGHT_Y, (p.z + 3.0) / 13.0),
        _ => {
            let q = rotate(p - cube_center(), true);
            let a = [q.x, q.y, q.z];
            let axis = (id - 5) / 2;
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            (
                (a[u] / EXTENTS[u] + 1.0) * 0.5,
                (a[v] / EXTENTS[v] + 1.0) * 0.5,
            )
        }
    }
}
