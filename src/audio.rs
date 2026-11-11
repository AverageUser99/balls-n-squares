//! The recording is part of this standalone WASM module, not an external URL.
static TRACK: &[u8] = include_bytes!("../assets/clair-de-lune.mp3");

#[no_mangle]
pub extern "C" fn track_ptr() -> *const u8 { TRACK.as_ptr() }

#[no_mangle]
pub extern "C" fn track_len() -> usize { TRACK.len() }
