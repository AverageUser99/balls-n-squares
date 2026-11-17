use crate::{
    math::Vec3,
    sampling::{self, Rng},
    scene::*,
};

// One diffuse indirect segment. Directly sampled emitters are not counted twice.
pub fn one_bounce(origin: Vec3, direction: Vec3, rng: &mut Rng) -> Vec3 {
    let hit = trace(origin, direction);
    if hit.surface < SURFACES {
        albedo(hit.surface) * direct(hit.point, hit.surface, rng.next(), rng.next())
    } else {
        Vec3::default()
    }
}

// Small view-independent irradiance grid; the sample directions stay fixed on
// each face as objects move. Only geometry edits require rebuilding it.
const GRID: usize = 8;
#[derive(Default)]
pub struct LiveCache {
    geometry: Option<[f32; 6]>,
    values: Vec<Vec3>,
}
impl LiveCache {
    pub fn prepare(&mut self) {
        let geometry = positions();
        if self.geometry == Some(geometry) {
            return;
        }
        self.values.clear();
        for id in 0..SURFACES {
            for cell in 0..GRID * GRID {
                let p = surface_point(
                    id,
                    ((cell % GRID) as f32 + 0.5) / GRID as f32,
                    ((cell / GRID) as f32 + 0.5) / GRID as f32,
                ) + normal(id) * EPS;
                let seed = (id * GRID * GRID + cell) as u32;
                let mut sum = Vec3::default();
                for s in 0..32 {
                    let (u, v) = sampling::pair(s, seed);
                    let mut rng = Rng::new(seed * 32 + s);
                    sum = sum + one_bounce(p, sampling::cosine(id, u, v), &mut rng);
                }
                self.values.push(sum / 32.0);
            }
        }
        self.geometry = Some(geometry);
    }
    pub fn lookup(&self, id: usize, p: Vec3) -> Option<Vec3> {
        interpolate(&self.values, id, p, GRID, id * GRID * GRID)
    }
}

pub struct Lighting {
    pub values: Vec<Vec3>,
    pub ready: bool,
}
impl Default for Lighting {
    fn default() -> Self {
        Self {
            values: vec![Vec3::default(); TEXELS],
            ready: false,
        }
    }
}
impl Lighting {
    pub fn bake(&mut self, start: usize, count: usize, max_samples: u32) {
        let max_samples = max_samples.clamp(64, 4096);
        for index in start.min(TEXELS)..start.saturating_add(count).min(TEXELS) {
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
                let value = one_bounce(p + normal(id) * EPS, direction, &mut rng);
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
    let (ix, iy) = (x as usize, y as usize);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let at = |a: usize, b: usize| values[base + b.min(res - 1) * res + a.min(res - 1)];
    Some(
        (at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx) * (1.0 - fy)
            + (at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx) * fy,
    )
}
