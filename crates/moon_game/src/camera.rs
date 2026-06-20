//! LOD selection helper.
//!
//! In Sprint 02 the interactive camera is the 3D orbit rig in `terrain3d.rs`; all that
//! remains here is the pure function that maps a screen meters-per-pixel (the camera's
//! ground-sample distance) to the matching baked zoom level, used by tile streaming.

use moon_data::SiteManifest;

/// The LOD zoom for a given screen mpp: the coarsest baked level still at least as fine
/// as the screen (largest `mpp ≤ screen_mpp`), falling back to the finest baked level
/// when zoomed in past the finest density.
pub fn select_lod(manifest: &SiteManifest, screen_mpp: f32) -> u8 {
    let screen = screen_mpp as f64;
    let mut best: Option<(u8, f64)> = None; // (zoom, mpp) with largest mpp ≤ screen
    let mut finest: Option<(u8, f64)> = None; // (zoom, mpp) with smallest mpp overall
    for z in &manifest.zoom_levels {
        if z.mpp <= screen && best.is_none_or(|(_, m)| z.mpp > m) {
            best = Some((z.zoom, z.mpp));
        }
        if finest.is_none_or(|(_, m)| z.mpp < m) {
            finest = Some((z.zoom, z.mpp));
        }
    }
    best.or(finest).map(|(z, _)| z).unwrap_or(0)
}
