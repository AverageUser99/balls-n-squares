use crate::{
    lighting::{self, Lighting},
    math::Vec3,
    sampling::{self, Rng},
    scene::*,
};

const AA: usize = 8;
const MAX_SAMPLES: u32 = 512;
const INVALID: u32 = u32::MAX;
const MIRRORED: u32 = 256;

#[derive(Clone, Copy)]
struct CameraHit {
    point: Vec3,
    tag: u32,
    depth: f32,
}
impl Default for CameraHit {
    fn default() -> Self {
        Self {
            point: Vec3::default(),
            tag: INVALID,
            depth: 0.0,
        }
    }
}
impl CameraHit {
    fn surface(self) -> usize {
        (self.tag & 255) as usize
    }
    fn tint(self) -> Vec3 {
        if self.tag & MIRRORED != 0 {
            TINT
        } else {
            Vec3::new(1.0, 1.0, 1.0)
        }
    }
}

#[derive(Clone, Copy, Default)]
struct Pixel {
    sum: Vec3,
    cycle: Vec3,
    mean: f32,
    m2: f32,
    count: u32,
    stable: u8,
    done: bool,
}
impl Pixel {
    fn average(self) -> Vec3 {
        self.sum / self.count.max(1) as f32
    }
    fn variance(self) -> f32 {
        let cycles = self.count / AA as u32;
        if cycles < 2 {
            return 1.0;
        }
        self.m2 / (cycles * (cycles - 1)) as f32
    }
    fn add(&mut self, value: Vec3, adaptive: bool) {
        self.sum = self.sum + value;
        self.cycle = self.cycle + value;
        self.count += 1;
        if self.count.is_multiple_of(AA as u32) {
            let cycles = self.count / AA as u32;
            let luminance = (self.cycle / AA as f32).luminance();
            let delta = luminance - self.mean;
            self.mean += delta / cycles as f32;
            self.m2 += delta * (luminance - self.mean);
            self.cycle = Vec3::default();
            if adaptive && self.count >= 64 {
                // Assess complete AA cycles, not the artificial variance between subpixels.
                let threshold = 0.003 + self.mean * 0.018;
                if self.variance() < threshold * threshold {
                    self.stable += 1;
                } else {
                    self.stable = 0;
                }
                if self.stable >= 3 {
                    self.done = true;
                }
            }
        }
        if self.count >= MAX_SAMPLES {
            self.done = true;
        }
    }
}

fn srgb(mapped: f32) -> f32 {
    if mapped <= 0.0031308 {
        mapped * 12.92
    } else {
        1.055 * mapped.powf(1.0 / 2.4) - 0.055
    }
}
fn tone(linear: f32, lut: &[f32]) -> u8 {
    let x = linear.max(0.0) * 1.15;
    let mapped = ((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)).clamp(0.0, 1.0);
    let position = mapped * 4096.0;
    let index = position as usize;
    let f = position - index as f32;
    (lut[index] * (1.0 - f) + lut[(index + 1).min(4096)] * f + 0.5) as u8
}

pub struct Renderer {
    pub lighting: Lighting,
    pub pixels: Vec<u8>,
    pub passes: u32,
    pub active: usize,
    pub evaluated: u32,
    width: usize,
    full_height: usize,
    y_start: usize,
    height: usize,
    row: usize,
    options: u32,
    live: bool,
    preview: bool,
    state: Vec<Pixel>,
    camera: Vec<CameraHit>,
    lut: Vec<f32>,
}
impl Default for Renderer {
    fn default() -> Self {
        Self {
            lighting: Lighting::default(),
            pixels: Vec::new(),
            passes: 0,
            active: 0,
            evaluated: 0,
            width: 0,
            full_height: 0,
            y_start: 0,
            height: 0,
            row: 0,
            options: 0,
            live: false,
            preview: false,
            state: Vec::new(),
            camera: Vec::new(),
            lut: (0..=4096)
                .map(|i| srgb(i as f32 / 4096.0) * 255.0)
                .collect(),
        }
    }
}
impl Renderer {
    pub fn options(&mut self, options: u32) {
        self.options = options;
    }
    pub fn interactive(&mut self, preview: bool) {
        self.live = true;
        self.preview = preview;
        self.options |= 1; // Moving objects invalidate all static irradiance maps.
        self.lighting.ready = false;
    }
    pub fn resize(&mut self, width: usize, full_height: usize, y_start: usize, height: usize) {
        self.width = width.clamp(1, 1920);
        self.full_height = full_height.clamp(1, 1920);
        self.y_start = y_start.min(self.full_height - 1);
        self.height = height.clamp(1, self.full_height - self.y_start);
        self.passes = 0;
        self.row = 0;
        self.evaluated = 0;
        self.active = self.width * self.height;
        // Reuse allocations on scene edits instead of reallocating the whole camera cache.
        self.state.resize(self.active, Pixel::default());
        self.state.fill(Pixel::default());
        self.camera.resize(self.active * AA, CameraHit::default());
        self.camera.fill(CameraHit::default());
        self.pixels.resize(self.active * 4, 0);
        for p in self.pixels.chunks_exact_mut(4) {
            p[3] = 255;
        }
    }

    fn camera_hit(&self, x: usize, y: usize, slot: usize) -> CameraHit {
        let origin = crate::interaction::camera_eye();
        let index = (y * self.width + x) as u32;
        let (jx, jy) = sampling::pair(slot as u32, index ^ 0x51633e2d);
        let aspect = self.width as f32 / self.full_height as f32;
        let mut direction = crate::interaction::camera_ray(
            (x as f32 + jx) / self.width as f32,
            (y as f32 + jy) / self.full_height as f32,
            aspect,
        );
        let mut hit = trace(origin, direction);
        let depth = hit.t;
        let mirrored = hit.surface == MIRROR;
        if mirrored {
            direction = direction.reflect(hit.n);
            hit = trace(hit.point + hit.n * EPS, direction);
        }
        CameraHit {
            point: hit.point,
            tag: hit.surface as u32 | if mirrored { MIRRORED } else { 0 },
            depth,
        }
    }

    pub fn render(&mut self, rows: usize) {
        if self.width == 0 || self.active == 0 {
            return;
        }
        for _ in 0..rows.min(self.height) {
            let y = self.row;
            for x in 0..self.width {
                let index = y * self.width + x;
                let pixel = self.state[index];
                if pixel.done {
                    continue;
                }
                let slot = pixel.count as usize % AA;
                let cache_index = index * AA + slot;
                let mut hit = self.camera[cache_index];
                if hit.tag == INVALID || self.options & 8 != 0 {
                    hit = self.camera_hit(x, y + self.y_start, slot);
                    self.camera[cache_index] = hit;
                }
                let id = hit.surface();
                let global = ((y + self.y_start) * self.width + x) as u32;
                let mut color = Vec3::default();
                if id == EMITTER {
                    color = LIGHT;
                } else if id < SURFACES {
                    // Each cached AA ray gets its own full light sequence. Reusing the
                    // global sample index would lock AA slots to subsets of the light.
                    let (u, v) = sampling::pair(
                        pixel.count / AA as u32,
                        global ^ (slot as u32).wrapping_mul(0x9e3779b9) ^ 0xb5297a4d,
                    );
                    let mut incoming = direct(hit.point, id, u, v);
                    let cached = if self.options & 1 == 0 {
                        self.lighting.lookup(id, hit.point)
                    } else {
                        None
                    };
                    incoming = incoming
                        + match cached {
                            Some(value) => value,
                            None => {
                                let mut rng =
                                    Rng::new(global.wrapping_mul(512).wrapping_add(pixel.count));
                                let d = sampling::cosine(id, rng.next(), rng.next());
                                lighting::path(
                                    hit.point + normal(id) * EPS,
                                    d,
                                    &mut rng,
                                    false,
                                    if !self.live {
                                        5
                                    } else if self.preview {
                                        1
                                    } else {
                                        3
                                    },
                                )
                            }
                        };
                    color = incoming * albedo(id);
                }
                self.state[index].add(color * hit.tint(), self.options & 2 == 0);
                if self.live && self.state[index].count >= if self.preview { 32 } else { 256 } {
                    self.state[index].done = true;
                }
                self.evaluated += 1;
                if self.state[index].done {
                    self.active -= 1;
                }
            }
            self.row += 1;
            if self.row == self.height {
                self.row = 0;
                self.passes += 1;
            }
            if self.active == 0 {
                break;
            }
        }
    }

    pub fn present(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                let index = y * self.width + x;
                let pixel = self.state[index];
                let center = pixel.average();
                let guide = self.camera[index * AA];
                let mut sum = center;
                let mut weights = 1.0;
                if self.options & 4 == 0 && pixel.count >= 16 && guide.surface() < SURFACES {
                    // Reflections are guided by the *reflected* face and world position,
                    // not merely the smooth sphere normal: reflected edges remain sharp.
                    let fade = (64.0 / pixel.count as f32).min(1.0) * 0.4;
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            if dx == 0 && dy == 0 {
                                continue;
                            }
                            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                            if nx < 0
                                || ny < 0
                                || nx >= self.width as i32
                                || ny >= self.height as i32
                            {
                                continue;
                            }
                            let neighbor = ny as usize * self.width + nx as usize;
                            let other = self.state[neighbor];
                            let other_guide = self.camera[neighbor * AA];
                            if other.count < 16 || guide.tag != other_guide.tag {
                                continue;
                            }
                            let separation = guide.point - other_guide.point;
                            let max_distance = 0.05 + guide.depth * 0.012;
                            if separation.dot(separation) > max_distance * max_distance {
                                continue;
                            }
                            let value = other.average();
                            let difference = (value.luminance() - center.luminance()).abs();
                            let sigma = (pixel.variance() + other.variance()).sqrt();
                            let range = 0.015 + sigma * 2.0;
                            let weight = fade
                                / (1.0 + (difference / range).powi(2))
                                / (dx * dx + dy * dy) as f32;
                            sum = sum + value * weight;
                            weights += weight;
                        }
                    }
                }
                let value = sum / weights;
                let p = index * 4;
                self.pixels[p] = tone(value.x, &self.lut);
                self.pixels[p + 1] = tone(value.y, &self.lut);
                self.pixels[p + 2] = tone(value.z, &self.lut);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lut_stays_within_one_display_level() {
        let r = Renderer::default();
        for i in 0..10000 {
            let x = i as f32 / 1000.0;
            let v = x * 1.15;
            let mapped = ((v * (2.51 * v + 0.03)) / (v * (2.43 * v + 0.59) + 0.14)).clamp(0.0, 1.0);
            let exact = (srgb(mapped) * 255.0 + 0.5) as i32;
            assert!((tone(x, &r.lut) as i32 - exact).abs() <= 1);
        }
    }
    #[test]
    fn adaptive_sampling_keeps_complete_antialiasing_cycles() {
        let mut pixel = Pixel::default();
        while !pixel.done {
            pixel.add(Vec3::new(0.5, 0.5, 0.5), true);
        }
        assert_eq!(pixel.count, 80);
        assert_eq!(pixel.count as usize % AA, 0);
    }
}
