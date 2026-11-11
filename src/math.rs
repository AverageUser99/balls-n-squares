use std::ops::{Add, Div, Mul, Neg, Sub};

// Four lanes give SIMD a useful, contiguous RGB workload. The spare lane is zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub pad: f32,
}

impl Vec3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z, pad: 0.0 }
    }
    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }
    pub fn normalized(self) -> Self {
        self / self.dot(self).sqrt().max(1e-10)
    }
    pub fn max_component(self) -> f32 {
        self.x.max(self.y).max(self.z)
    }
    pub fn luminance(self) -> f32 {
        self.dot(Self::new(0.2126, 0.7152, 0.0722))
    }
    pub fn reflect(self, n: Self) -> Self {
        self - n * (2.0 * self.dot(n))
    }
}

macro_rules! vector_op {
    ($trait:ident, $method:ident, $intrinsic:ident, $op:tt) => {
        impl $trait for Vec3 {
            type Output = Self;
            #[inline(always)]
            fn $method(self, rhs: Self) -> Self {
                #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
                unsafe {
                    use core::arch::wasm32::*;
                    let mut result = Self::default();
                    v128_store((&mut result as *mut Self).cast(), $intrinsic(
                        v128_load((&self as *const Self).cast()),
                        v128_load((&rhs as *const Self).cast()),
                    ));
                    result
                }
                #[cfg(not(all(target_arch = "wasm32", target_feature = "simd128")))]
                { Self::new(self.x $op rhs.x, self.y $op rhs.y, self.z $op rhs.z) }
            }
        }
    };
}
vector_op!(Add, add, f32x4_add, +);
vector_op!(Sub, sub, f32x4_sub, -);
vector_op!(Mul, mul, f32x4_mul, *);
impl Mul<f32> for Vec3 {
    type Output = Self;
    #[inline(always)]
    fn mul(self, rhs: f32) -> Self {
        self * Self::new(rhs, rhs, rhs)
    }
}
impl Div<f32> for Vec3 {
    type Output = Self;
    #[inline(always)]
    fn div(self, rhs: f32) -> Self {
        self * (1.0 / rhs)
    }
}
impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self {
        self * -1.0
    }
}
