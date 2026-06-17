//! 2D orthographic camera: grab-the-world pan, zoom-to-cursor, exponential
//! smoothing, and manifest-driven clamps. Nothing geometric is hardcoded — the
//! bounds and scale limits all derive from the loaded [`SiteManifest`].
//!
//! # Scale == screen meters-per-pixel
//! Our world is 1 unit = 1 m and the projection uses `ScalingMode::WindowSize`
//! with `viewport_origin = (0.5, 0.5)`, so `OrthographicProjection::scale` is the
//! screen meters-per-pixel directly. That makes the clamp / LOD math below exact
//! and lets us convert cursor pixels ⇄ world meters with a one-line formula.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use moon_data::SiteManifest;

use crate::flythrough::Flythrough;
use crate::Site;

/// Mouse button that drags the world under the cursor.
const PAN_BUTTON: MouseButton = MouseButton::Left;
/// Fraction of `target_scale` changed per wheel "line" of scroll.
const ZOOM_SPEED: f32 = 0.12;
/// Exponential smoothing rate toward `target_scale` (larger = snappier).
const SMOOTH_K: f32 = 14.0;
/// Trackpad pixel-delta → line-equivalent factor (one wheel notch ≈ 16 px).
const PIXEL_TO_LINE: f32 = 1.0 / 16.0;
/// Slack (m) allowed beyond the world bbox when clamping the view, so the very
/// edge of the terrain isn't pinned hard against the screen border.
const EDGE_MARGIN_M: f32 = 256.0;

/// Runtime camera state. `min_scale`/`target_scale` are seeded from the manifest
/// at spawn; `max_scale` is recomputed each frame from the live window height
/// (it's the scale at which the whole site fits vertically).
#[derive(Resource, Debug)]
pub struct CameraController {
    /// Where `projection.scale` is lerping toward (screen m/px).
    pub target_scale: f32,
    /// Finest baked m/px — the hard zoom-in limit (texel density).
    pub min_scale: f32,
    /// World point pinned under the cursor while a drag-pan is active.
    pub drag_anchor: Option<Vec2>,
}

impl CameraController {
    /// Seed from the manifest: `min_scale` = finest baked mpp, initial
    /// `target_scale` = a mid zoom (clamped into range on the first frame).
    pub fn from_manifest(manifest: &SiteManifest) -> Self {
        let min_scale = finest_mpp(manifest);
        Self {
            target_scale: (min_scale * 4.0).max(min_scale),
            min_scale,
            drag_anchor: None,
        }
    }
}

/// Smallest meters-per-pixel across baked zoom levels (finest detail).
fn finest_mpp(manifest: &SiteManifest) -> f32 {
    manifest
        .zoom_levels
        .iter()
        .map(|z| z.mpp)
        .fold(manifest.base_mpp, f64::min) as f32
}

/// The LOD zoom the streaming plan (04) will select for a given screen mpp:
/// the coarsest baked level still at least as fine as the screen
/// (largest `mpp ≤ screen_mpp`), falling back to the finest level when the
/// camera is zoomed in past the finest baked density.
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

/// Drive the camera each frame: read input, smooth zoom toward the cursor, pan,
/// then clamp the visible AABB inside the world bounds.
pub fn camera_control(
    time: Res<Time>,
    mut contexts: EguiContexts,
    windows: Query<&Window, With<PrimaryWindow>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    mut controller: ResMut<CameraController>,
    flythrough: Res<Flythrough>,
    site: Res<Site>,
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) -> Result {
    // The scripted flythrough owns the camera while it plays — bail and drop any
    // wheel backlog so a resume doesn't apply scroll accumulated during playback.
    if flythrough.playing {
        wheel.clear();
        return Ok(());
    }

    // Don't steal pointer input while interacting with the egui overlay.
    if let Ok(ctx) = contexts.ctx_mut()
        && ctx.wants_pointer_input()
    {
        wheel.clear();
        return Ok(());
    }

    let Ok(window) = windows.single() else {
        return Ok(());
    };
    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return Ok(());
    };
    let Projection::Orthographic(ortho) = projection.as_mut() else {
        return Ok(());
    };

    let size = window.size();
    let center = size * 0.5;
    let cursor = window.cursor_position();

    // Pixel cursor (top-left, y-down) → world meters at a given translation/scale.
    let world_at = |cursor: Vec2, translation: Vec2, scale: f32| -> Vec2 {
        Vec2::new(
            translation.x + (cursor.x - center.x) * scale,
            translation.y - (cursor.y - center.y) * scale,
        )
    };

    let wb = site.0.world_bbox;
    let world_h = wb[3] - wb[1];
    // Whole site fits vertically at this scale (the zoom-out limit).
    let max_scale = (world_h / size.y.max(1.0)).max(controller.min_scale);
    let min_scale = controller.min_scale;

    // --- Zoom input: accumulate wheel into target_scale ---
    let mut scroll = 0.0;
    for ev in wheel.read() {
        scroll += match ev.unit {
            MouseScrollUnit::Line => ev.y,
            MouseScrollUnit::Pixel => ev.y * PIXEL_TO_LINE,
        };
    }
    if scroll != 0.0 {
        controller.target_scale *= 1.0 - scroll * ZOOM_SPEED;
    }
    controller.target_scale = controller.target_scale.clamp(min_scale, max_scale);

    // --- Pan input: pin the world point under the cursor on press ---
    if buttons.just_pressed(PAN_BUTTON) {
        if let Some(c) = cursor {
            controller.drag_anchor = Some(world_at(c, transform.translation.truncate(), ortho.scale));
        }
    }
    if buttons.just_released(PAN_BUTTON) {
        controller.drag_anchor = None;
    }

    // --- Smooth scale toward target, keeping the cursor's world point fixed ---
    let cur = ortho.scale;
    let new_scale = cur + (controller.target_scale - cur) * (1.0 - (-SMOOTH_K * time.delta_secs()).exp());
    if let Some(c) = cursor {
        let before = world_at(c, transform.translation.truncate(), cur);
        ortho.scale = new_scale;
        let after = world_at(c, transform.translation.truncate(), new_scale);
        let delta = before - after;
        transform.translation.x += delta.x;
        transform.translation.y += delta.y;
    } else {
        ortho.scale = new_scale;
    }

    // --- Pan: keep the grabbed world point under the cursor at the new scale ---
    if let (Some(anchor), Some(c)) = (controller.drag_anchor, cursor) {
        let s = ortho.scale;
        transform.translation.x = anchor.x - (c.x - center.x) * s;
        transform.translation.y = anchor.y + (c.y - center.y) * s;
    }

    // --- Clamp: keep the visible AABB inside the world bbox (+ margin) ---
    let half = size * 0.5 * ortho.scale;
    let clamped = clamp_view(transform.translation.truncate(), half, wb);
    transform.translation.x = clamped.x;
    transform.translation.y = clamped.y;

    Ok(())
}

/// Clamp a camera center so the visible AABB (`half` = half window extent in
/// world meters, i.e. `window/2 * scale`) stays inside the world `bbox`
/// (`[min_x, min_y, max_x, max_y]`) plus [`EDGE_MARGIN_M`] of slack. When the view
/// is larger than the world on an axis, it centers on the world midpoint instead.
/// Shared by [`camera_control`] and the scripted flythrough so both honor the same
/// bounds.
pub(crate) fn clamp_view(pos: Vec2, half: Vec2, bbox: [f32; 4]) -> Vec2 {
    let (min, max) = (Vec2::new(bbox[0], bbox[1]), Vec2::new(bbox[2], bbox[3]));
    let mid = (min + max) * 0.5;
    let lo = min + half - EDGE_MARGIN_M;
    let hi = max - half + EDGE_MARGIN_M;
    Vec2::new(
        clamp_axis(pos.x, lo.x, hi.x, mid.x),
        clamp_axis(pos.y, lo.y, hi.y, mid.y),
    )
}

/// Clamp `v` to `[lo, hi]`, or center on `mid` when the view is larger than the
/// world on this axis (i.e. `lo > hi`, so there's nothing valid to clamp into).
fn clamp_axis(v: f32, lo: f32, hi: f32, mid: f32) -> f32 {
    if lo > hi { mid } else { v.clamp(lo, hi) }
}
