use crate::{
    math::Vec3,
    sampling::{self, Rng},
    scene::*,
};

pub const DEFAULT_BOUNCES: u32 = 4;

pub fn indirect(mut origin: Vec3, mut direction: Vec3, rng: &mut Rng, bounces: u32) -> Vec3 {
    let mut throughput = Vec3::new(1.0, 1.0, 1.0);
    let mut radiance = Vec3::default();
    for depth in 0..bounces {
        let hit = trace(origin, direction);
        if hit.surface >= SURFACES {
            break;
        }
        throughput = throughput * albedo(hit.surface);
        radiance = radiance + throughput * direct(hit.point, hit.surface, rng.next(), rng.next());
        if depth + 1 < bounces {
            origin = hit.point + hit.n * EPS;
            direction = sampling::cosine(hit.surface, rng.next(), rng.next());
        }
    }
    radiance
}

// Small view-independent irradiance grid; the sample directions stay fixed on
// each face as objects move.
const GRID: usize = 8;
#[derive(Default)]
pub struct LiveCache {
    key: Option<([f32; 6], u32)>,
    values: Vec<Vec3>,
    exterior: Vec<Option<Vec3>>,
}

// Uniform grid in surface space from UV -2 to 3. Exterior probes are filled
// on demand, so unseen parts of the extension do not cost any traced rays.
const BORDER: usize = 2 * GRID;
const WIDE: usize = GRID + 2 * BORDER;
fn probe_uv(i: usize) -> f32 {
    (i as f32 + 0.5) / GRID as f32
}

fn probe(id: usize, u: f32, v: f32, seed: u32, bounces: u32) -> Vec3 {
    let p = surface_point(id, u, v) + normal(id) * EPS;
    let mut sum = Vec3::default();
    for s in 0..32 {
        let (u, v) = sampling::pair(s, seed);
        let mut rng = Rng::new(seed * 32 + s);
        sum = sum + indirect(p, sampling::cosine(id, u, v), &mut rng, bounces);
    }
    sum / 32.0
}
impl LiveCache {
    pub fn prepare(&mut self, bounces: u32) {
        let key = (positions(), bounces);
        if bounces == 0 || self.key == Some(key) {
            return;
        }
        self.values.clear();
        for id in 0..SURFACES {
            for cell in 0..GRID * GRID {
                self.values.push(probe(
                    id,
                    probe_uv(cell % GRID),
                    probe_uv(cell / GRID),
                    (id * GRID * GRID + cell) as u32,
                    bounces,
                ));
            }
        }
        self.exterior.clear();
        self.exterior.resize(5 * WIDE * WIDE, None);
        self.key = Some(key);
    }
    fn extended_probe(&mut self, id: usize, x: usize, y: usize, bounces: u32) -> Vec3 {
        if (BORDER..BORDER + GRID).contains(&x) && (BORDER..BORDER + GRID).contains(&y) {
            return self.values[id * GRID * GRID + (y - BORDER) * GRID + (x - BORDER)];
        }
        let i = (id * WIDE + y) * WIDE + x;
        *self.exterior[i].get_or_insert_with(|| {
            probe(
                id,
                probe_uv(x) - 2.0,
                probe_uv(y) - 2.0,
                (SURFACES * GRID * GRID + i) as u32,
                bounces,
            )
        })
    }

    pub fn lookup(&mut self, id: usize, p: Vec3) -> Option<Vec3> {
        let (u, v) = surface_uv(id, p);
        let x = u * GRID as f32 - 0.5;
        let y = v * GRID as f32 - 0.5;
        let inner = 0.0..=(GRID - 1) as f32;
        if inner.contains(&x) && inner.contains(&y) {
            return Some(bilinear(&self.values, GRID, id * GRID * GRID, x, y));
        }
        if id >= 5 {
            return interpolate(&self.values, id, p, GRID, id * GRID * GRID);
        }
        let (_, bounces) = self.key?;
        let x = (x + BORDER as f32).clamp(0.0, (WIDE - 1) as f32);
        let y = (y + BORDER as f32).clamp(0.0, (WIDE - 1) as f32);
        let (ix, iy) = (x as usize, y as usize);
        let (nx, ny) = ((ix + 1).min(WIDE - 1), (iy + 1).min(WIDE - 1));
        let (fx, fy) = (x - ix as f32, y - iy as f32);
        let mut at = |x, y| self.extended_probe(id, x, y, bounces);
        let value = (at(ix, iy) * (1.0 - fx) + at(nx, iy) * fx) * (1.0 - fy)
            + (at(ix, ny) * (1.0 - fx) + at(nx, ny) * fx) * fy;
        let distance = (-2.0 - u).max(u - 3.0).max(-2.0 - v).max(v - 3.0).max(0.0);
        Some(value / (1.0 + distance).powi(2))
    }
}

pub struct Lighting {
    pub bounces: u32,
    pub values: Vec<Vec3>,
    pub ready: bool,
}
impl Default for Lighting {
    fn default() -> Self {
        Self {
            values: vec![Vec3::default(); TEXELS],
            ready: false,
            bounces: DEFAULT_BOUNCES,
        }
    }
}
impl Lighting {
    pub fn bake(&mut self, start: usize, count: usize, max_samples: u32) {
        let max_samples = max_samples.clamp(64, 4096);
        for index in start.min(TEXELS)..start.saturating_add(count).min(TEXELS) {
            if self.bounces == 0 {
                self.values[index] = Vec3::default();
                continue;
            }
            let id = if index < offset(5) {
                index / 4096
            } else {
                5 + (index - offset(5)) / 576
            };
            let res = resolution(id);
            let cell = index - offset(id);
            let p = surface_point(
                id,
                (cell % res) as f32 / res as f32 + 0.5 / res as f32,
                (cell / res) as f32 / res as f32 + 0.5 / res as f32,
            );
            // Continue indirect lighting beneath the emitter as padding for bilinear
            // filtering. Zero padding would create a dark outline around the panel.
            let mut sum = Vec3::default();
            let mut mean = 0.0;
            let mut m2 = 0.0;
            let mut count = 0;
            for sample in 0..max_samples {
                let (u, v) = sampling::pair(sample, index as u32 ^ 0xa511e9b3);
                let direction = sampling::cosine(id, u, v);
                let mut rng = Rng::new((index as u32).wrapping_mul(512).wrapping_add(sample));
                let value = indirect(p + normal(id) * EPS, direction, &mut rng, self.bounces);
                sum = sum + value;
                count += 1;
                let delta = value.luminance() - mean;
                mean += delta / count as f32;
                m2 += delta * (value.luminance() - mean);
                // Conservative adaptive work allocation; never quit on a few dark samples.
                if count >= max_samples.min(256) && count % 32 == 0 {
                    let error2 = m2 / ((count - 1) * count) as f32;
                    if error2 < (0.002 + mean * 0.02).powi(2) {
                        break;
                    }
                }
            }
            self.values[index] = sum / count.max(1) as f32;
        }
    }

    pub fn finish(&mut self) {
        // Only indirect light is smoothed, within each surface, never across corners.
        let source = self.values.clone();
        for id in 0..SURFACES {
            let res = resolution(id);
            let offset = offset(id);
            for y in 0..res {
                for x in 0..res {
                    let center = source[offset + y * res + x];
                    let mut sum = center * 4.0;
                    let mut weights = 4.0;
                    for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                        if nx < 0 || ny < 0 || nx >= res as i32 || ny >= res as i32 {
                            continue;
                        }
                        let other = source[offset + ny as usize * res + nx as usize];
                        let delta = (center.luminance() - other.luminance()).abs();
                        let weight = 1.0 / (1.0 + delta * 12.0);
                        sum = sum + other * weight;
                        weights += weight;
                    }
                    self.values[offset + y * res + x] = sum / weights;
                }
            }
        }
        self.ready = true;
    }

    pub fn lookup(&self, id: usize, p: Vec3) -> Option<Vec3> {
        if !self.ready || id >= SURFACES {
            return None;
        }
        interpolate(&self.values, id, p, resolution(id), offset(id))
    }
}

fn interpolate(values: &[Vec3], id: usize, p: Vec3, res: usize, base: usize) -> Option<Vec3> {
    let (u, v) = surface_uv(id, p);
    // Rays can see beyond the mapped room through the mirror. Trace those normally.
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return None;
    }
    let x = (u * res as f32 - 0.5).clamp(0.0, (res - 1) as f32);
    let y = (v * res as f32 - 0.5).clamp(0.0, (res - 1) as f32);
    Some(bilinear(values, res, base, x, y))
}

fn bilinear(values: &[Vec3], res: usize, base: usize, x: f32, y: f32) -> Vec3 {
    let (ix, iy) = (x as usize, y as usize);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let at = |a: usize, b: usize| values[base + b.min(res - 1) * res + a.min(res - 1)];
    (at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx) * (1.0 - fy)
        + (at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx) * fy
}
