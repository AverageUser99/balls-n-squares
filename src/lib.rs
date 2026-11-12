//! Dependency-free WASM CPU renderer. Workers only transport RGBA and lighting maps.
mod interaction;
mod lighting;
mod math;
mod render;
mod sampling;
mod scene;

use render::Renderer;
use std::cell::RefCell;

thread_local! { static RENDERER: RefCell<Renderer> = RefCell::new(Renderer::default()); }

#[no_mangle]
pub extern "C" fn resize(width: u32, height: u32) {
    resize_tile(width, height, 0, height);
}
#[no_mangle]
pub extern "C" fn resize_tile(width: u32, full_height: u32, y: u32, height: u32) {
    RENDERER.with(|s| {
        s.borrow_mut().resize(
            width as usize,
            full_height as usize,
            y as usize,
            height as usize,
        )
    });
}
#[no_mangle]
pub extern "C" fn camera_value(index: u32) -> f32 {
    interaction::camera_values()
        .get(index as usize)
        .copied()
        .unwrap_or(0.0)
}
#[no_mangle]
pub extern "C" fn set_camera(x: f32, y: f32, z: f32, yaw: f32, pitch: f32) {
    interaction::set_camera([x, y, z, yaw, pitch]);
}
#[no_mangle]
pub extern "C" fn reset_camera() {
    interaction::reset_camera();
}
#[no_mangle]
pub extern "C" fn look_camera(dx: f32, dy: f32) {
    interaction::look(dx, dy);
}
#[no_mangle]
pub extern "C" fn move_camera(strafe: f32, forward: f32, vertical: f32, dt: f32) -> u32 {
    interaction::move_camera(strafe, forward, vertical, dt) as u32
}
#[no_mangle]
pub extern "C" fn begin_drag(u: f32, v: f32, aspect: f32) -> u32 {
    interaction::begin(u, v, aspect)
}
#[no_mangle]
pub extern "C" fn drag_to(u: f32, v: f32, aspect: f32) -> u32 {
    interaction::update(u, v, aspect) as u32
}
#[no_mangle]
pub extern "C" fn end_drag() {
    interaction::end();
}
#[no_mangle]
pub extern "C" fn object_position(index: u32) -> f32 {
    scene::positions()
        .get(index as usize)
        .copied()
        .unwrap_or(0.0)
}
// Worker synchronization: followed by resize_tile() to invalidate cached paths.
#[no_mangle]
pub extern "C" fn set_objects(sx: f32, sz: f32, cx: f32, cz: f32, px: f32, pz: f32) {
    scene::set_positions([sx, sz, cx, cz, px, pz]);
}
#[no_mangle]
pub extern "C" fn interactive(preview: u32) {
    RENDERER.with(|s| s.borrow_mut().interactive(preview != 0));
}
#[no_mangle]
pub extern "C" fn render(rows: u32) {
    RENDERER.with(|s| s.borrow_mut().render(rows as usize));
}
#[no_mangle]
pub extern "C" fn present() {
    RENDERER.with(|s| s.borrow_mut().present());
}
#[no_mangle]
pub extern "C" fn pixels() -> *const u8 {
    RENDERER.with(|s| s.borrow().pixels.as_ptr())
}
#[no_mangle]
pub extern "C" fn sample_count() -> u32 {
    RENDERER.with(|s| s.borrow().passes)
}
#[no_mangle]
pub extern "C" fn active_pixels() -> u32 {
    RENDERER.with(|s| s.borrow().active as u32)
}
#[no_mangle]
pub extern "C" fn evaluated_samples() -> u32 {
    RENDERER.with(|s| s.borrow().evaluated)
}
// Test/benchmark controls: 1=no irradiance cache, 2=no adaptation, 4=no denoising,
// 8=retrace primary geometry. Defaults enable every optimization.
#[no_mangle]
pub extern "C" fn set_options(flags: u32) {
    RENDERER.with(|s| s.borrow_mut().options(flags));
}
#[no_mangle]
pub extern "C" fn cache_ptr() -> *const f32 {
    RENDERER.with(|s| s.borrow().lighting.values.as_ptr().cast())
}
#[no_mangle]
pub extern "C" fn cache_texels() -> u32 {
    scene::TEXELS as u32
}
#[no_mangle]
pub extern "C" fn bake(start: u32, count: u32, samples: u32) {
    RENDERER.with(|s| {
        s.borrow_mut()
            .lighting
            .bake(start as usize, count as usize, samples)
    });
}
#[no_mangle]
pub extern "C" fn finish_cache() {
    RENDERER.with(|s| s.borrow_mut().lighting.finish());
}
#[no_mangle]
pub extern "C" fn simd_enabled() -> u32 {
    cfg!(all(target_arch = "wasm32", target_feature = "simd128")) as u32
}

// Cache files are bound to the exact integrator/geometry source, not just dimensions.
const fn signature() -> u32 {
    let sources = [
        include_str!("scene.rs"),
        include_str!("lighting.rs"),
        include_str!("sampling.rs"),
        include_str!("math.rs"),
    ];
    let mut hash = 2166136261u32;
    let mut file = 0;
    while file < sources.len() {
        let bytes = sources[file].as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            hash = (hash ^ bytes[i] as u32).wrapping_mul(16777619);
            i += 1;
        }
        file += 1;
    }
    hash
}
#[no_mangle]
pub extern "C" fn cache_signature() -> u32 {
    signature()
}

// Optional build-time acceleration. It uses exactly the same compiled Rust integrator.
#[cfg(not(target_arch = "wasm32"))]
pub fn bake_file(destination: &std::path::Path, samples: u32) -> std::io::Result<()> {
    use std::io::Write;
    let mut cache = lighting::Lighting::default();
    cache.bake(0, scene::TEXELS, samples);
    let mut file = std::io::BufWriter::new(std::fs::File::create(destination)?);
    for word in [0x31434c57u32, signature(), scene::TEXELS as u32, 4] {
        file.write_all(&word.to_le_bytes())?;
    }
    for value in cache.values {
        for channel in [value.x, value.y, value.z, 0.0] {
            file.write_all(&channel.to_le_bytes())?;
        }
    }
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use math::Vec3;
    use scene::*;

    #[test]
    fn sphere_and_parallel_cube_intersections() {
        let h = trace(Vec3::new(-1.08, 1.0, 5.0), Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(h.surface, MIRROR);
        assert!((h.point.z - 1.05).abs() < 0.0001);
        let h = trace(Vec3::new(1.08, 4.0, -0.7), Vec3::new(0.0, -1.0, 0.0));
        assert!((h.t - 2.2).abs() < 0.0001);
        assert!((h.n.y - 1.0).abs() < 0.0001);
    }

    #[test]
    fn pyramid_is_visible_and_lights_nearby_floor() {
        let hit = trace(Vec3::new(0.0, 0.2, 3.0), Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(hit.surface, PYRAMID);
        assert!(hit.n.z > 0.0);
        let point = Vec3::new(0.0, 0.0, 2.2);
        let mut total = Vec3::default();
        for sample in 0..128 {
            let (u, v) = sampling::pair(sample, 7);
            total = total + direct(point, 0, u, v);
        }
        // The warm emitter adds red light beyond the ceiling light's fixed ratio.
        assert!(total.x / total.z > LIGHT.x / LIGHT.z + 0.05);
    }

    #[test]
    fn shadow_segments_match_full_intersections() {
        for id in 0..SURFACES {
            for sample in 0..256 {
                let (u, v) = sampling::pair(sample, id as u32);
                let p = surface_point(id, u, v) + normal(id) * EPS;
                let (u, v) = sampling::pair(sample, id as u32 ^ 0x3521);
                let segment = Vec3::new((u - 0.5) * 1.8, LIGHT_Y, -0.65 + (v - 0.5) * 1.6) - p;
                if normal(id).dot(segment) <= 0.0 {
                    continue;
                }
                let length = segment.dot(segment).sqrt();
                let expected = trace(p, segment / length).t < length - 0.004;
                assert_eq!(
                    occluded(p, segment),
                    expected,
                    "surface {id}, sample {sample}"
                );
            }
        }
    }

    #[test]
    fn face_bases_are_orthonormal_and_uv_round_trips() {
        for id in 0..SURFACES {
            let (t, b, n) = basis(id);
            assert!((n.dot(n) - 1.0).abs() < 1e-5);
            assert!((t.dot(t) - 1.0).abs() < 1e-5);
            assert!((b.dot(b) - 1.0).abs() < 1e-5);
            assert!(t.dot(n).abs() < 1e-5 && b.dot(n).abs() < 1e-5);
            let (u, v) = surface_uv(id, surface_point(id, 0.23, 0.74));
            assert!((u - 0.23).abs() < 1e-5 && (v - 0.74).abs() < 1e-5);
        }
    }

    #[test]
    fn sobol_samples_cover_all_strata() {
        let mut x = [false; 64];
        let mut y = [false; 64];
        for i in 0..64 {
            let (u, v) = sampling::pair(i, 123);
            x[(u * 64.0) as usize] = true;
            y[(v * 64.0) as usize] = true;
        }
        assert!(x.iter().all(|v| *v) && y.iter().all(|v| *v));
    }

    #[test]
    fn cache_stays_on_its_surface_and_falls_back_outside_map() {
        let mut cache = lighting::Lighting::default();
        for id in 0..SURFACES {
            let start = offset(id);
            let res = resolution(id);
            cache.values[start..start + res * res].fill(Vec3::new(id as f32, 0.0, 0.0));
        }
        cache.finish();
        for id in 0..SURFACES {
            let value = cache.lookup(id, surface_point(id, 0.001, 0.999)).unwrap();
            assert!((value.x - id as f32).abs() < 1e-5);
        }
        assert!(cache.lookup(0, Vec3::new(0.0, 0.0, 50.0)).is_none());
    }

    #[test]
    fn frame_is_finite_opaque_and_lit() {
        let mut r = Renderer::default();
        r.resize(64, 48, 0, 48);
        for _ in 0..16 {
            r.render(48);
        }
        r.present();
        assert!(r.pixels.chunks_exact(4).all(|p| p[3] == 255));
        let lit = r
            .pixels
            .chunks_exact(4)
            .filter(|p| p[0].max(p[1]).max(p[2]) > 40)
            .count();
        assert!(lit > 64 * 48 * 9 / 10);
    }
}
