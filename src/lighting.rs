use crate::{
    math::Vec3,
    sampling::{self, Rng},
    scene::*,
};

pub fn path(
    mut origin: Vec3,
    mut direction: Vec3,
    rng: &mut Rng,
    mut specular: bool,
    limit: usize,
) -> Vec3 {
    let mut result = Vec3::default();
    let mut throughput = Vec3::new(1.0, 1.0, 1.0);
    for depth in 0..limit {
        let hit = trace(origin, direction);
        if hit.surface == MISS {
            break;
        }
        if hit.surface == EMITTER {
            if specular {
                result = result + throughput * LIGHT;
            }
            break;
        }
        origin = hit.point + hit.n * EPS;
        if hit.surface == MIRROR {
            if depth + 1 == limit {
                break;
            }
            throughput = throughput * TINT;
            direction = direction.reflect(hit.n);
            specular = true;
            continue;
        }
        result = result
            + throughput
                * albedo(hit.surface)
                * direct(hit.point, hit.surface, rng.next(), rng.next());
        // Neither a direction nor a roulette draw is useful on the final bounce.
        if depth + 1 == limit {
            break;
        }
        throughput = throughput * albedo(hit.surface);
        if depth >= 2 {
            let survival = throughput.max_component().clamp(0.1, 0.9);
            if rng.next() >= survival {
                break;
            }
            throughput = throughput / survival;
        }
        // Roulette precedes expensive direction sampling; expectation is unchanged.
        direction = sampling::cosine(hit.surface, rng.next(), rng.next());
        specular = false;
    }
    result
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
                let value = path(p + normal(id) * EPS, direction, &mut rng, false, 5);
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
        let (u, v) = surface_uv(id, p);
        // Rays can see beyond the mapped room through the mirror. Trace those normally.
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return None;
        }
        let res = resolution(id);
        let x = (u * res as f32 - 0.5).clamp(0.0, (res - 1) as f32);
        let y = (v * res as f32 - 0.5).clamp(0.0, (res - 1) as f32);
        let (ix, iy) = (x as usize, y as usize);
        let (fx, fy) = (x - ix as f32, y - iy as f32);
        let base = offset(id);
        let at = |a: usize, b: usize| self.values[base + b.min(res - 1) * res + a.min(res - 1)];
        Some(
            (at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx) * (1.0 - fy)
                + (at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx) * fy,
        )
    }
}
