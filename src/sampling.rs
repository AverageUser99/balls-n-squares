use crate::{math::Vec3, scene};

pub fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^ (x >> 16)
}

const fn sobol_table() -> [[u32; 2]; 4096] {
    let mut table = [[0; 2]; 4096];
    let mut n = 0;
    while n < 4096 {
        let mut gray = n as u32 ^ ((n as u32) >> 1);
        let mut direction = 1u32 << 31;
        let mut second = 0;
        while gray != 0 {
            if gray & 1 != 0 {
                second ^= direction;
            }
            gray >>= 1;
            direction ^= direction >> 1;
        }
        let first = (n as u32 ^ ((n as u32) >> 1)).reverse_bits();
        table[n] = [first, second];
        n += 1;
    }
    table
}
static SOBOL: [[u32; 2]; 4096] = sobol_table();

// Independent per-pixel digital shifts preserve stratification in power-of-two sets.
pub fn pair(index: u32, seed: u32) -> (f32, f32) {
    let p = SOBOL[index as usize & 4095];
    let scale = 1.0 / 16777216.0;
    (
        ((p[0] ^ hash(seed)) >> 8) as f32 * scale,
        ((p[1] ^ hash(seed ^ 0x68bc21eb)) >> 8) as f32 * scale,
    )
}

pub struct Rng(u32);
impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(hash(seed))
    }
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(747796405).wrapping_add(2891336453);
        let word = ((self.0 >> ((self.0 >> 28) + 4)) ^ self.0).wrapping_mul(277803737);
        (((word >> 22) ^ word) >> 8) as f32 * (1.0 / 16777216.0)
    }
}

pub fn cosine(id: usize, u: f32, v: f32) -> Vec3 {
    let radius = u.sqrt();
    let (sin, cos) = (std::f32::consts::TAU * v).sin_cos();
    let (tangent, bitangent, n) = scene::basis(id);
    tangent * (radius * cos) + bitangent * (radius * sin) + n * (1.0 - u).sqrt()
}
