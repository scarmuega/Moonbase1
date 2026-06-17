//! Plan 06 — a scripted cinematic camera path for capture.
//!
//! Hand-flying a smooth pan+zoom for a trailer is fiddly and unrepeatable; this
//! plays a fixed keyframed path on a hotkey so every take is identical. `F` starts
//! it from a hard cut to the establishing shot; any mouse pan/zoom or `Esc` cancels
//! and hands control straight back without a scale snap.
//!
//! While playing, [`Flythrough::playing`] is set and `camera_control` bails (see
//! `camera.rs`), so this system has sole ownership of the camera. It writes the
//! `Transform` + orthographic `scale` directly and reuses [`crate::camera::clamp_view`]
//! so the path can never leave the world bbox.
//!
//! ## Authoring the path
//! Keyframes are **site-relative**, so one path works for any baked site (the small
//! 5 m/px Shackleton or the larger pole-wide `southpole`). `pos` is a fraction of the
//! world half-extent from the bbox center — `(0, 0)` is the middle, `(±1, ±1)` the
//! corners. `zoom` is `0` = fully zoomed in (finest baked m/px) … `1` = the whole
//! site framed. At playback these resolve against the live manifest + window. To
//! re-author, fly somewhere by hand and map the HUD `pos`/`scale` back to these
//! fractions (or just nudge the numbers and replay with `F`).

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::camera::{clamp_view, CameraController};
use crate::Site;

/// One stop on the path, in site-relative coordinates (see module docs).
#[derive(Clone, Copy)]
pub struct Keyframe {
    /// Camera center as a fraction of the world half-extent from bbox center
    /// (`0` = middle, `±1` = edge).
    pub pos: Vec2,
    /// Framing: `0` = finest baked detail (zoomed in), `1` = whole site (zoomed out).
    pub zoom: f32,
    /// Seconds spent easing *into* this stop from the previous one (ignored for the
    /// first keyframe, which is a hard cut).
    pub travel: f32,
    /// Seconds to dwell on this framing once arrived.
    pub hold: f32,
}

/// The default fly-over: a wide establishing shot, a slow push toward one quadrant,
/// a low pan across it, a dive to full detail, then a pull back out. Site-relative,
/// so it reframes itself to whatever site is loaded.
const KEYFRAMES: &[Keyframe] = &[
    // Establishing — nearly the whole site.
    Keyframe { pos: Vec2::new(0.0, 0.0), zoom: 0.92, travel: 0.0, hold: 2.5 },
    // Push in toward a rim quadrant.
    Keyframe { pos: Vec2::new(-0.45, -0.45), zoom: 0.34, travel: 6.0, hold: 1.5 },
    // Low pan across into the shadowed interior.
    Keyframe { pos: Vec2::new(0.45, -0.6), zoom: 0.18, travel: 7.0, hold: 1.5 },
    // Dive to finest baked detail near the center.
    Keyframe { pos: Vec2::new(0.1, 0.1), zoom: 0.0, travel: 6.0, hold: 2.0 },
    // Pull back out to the establishing frame.
    Keyframe { pos: Vec2::new(0.0, 0.0), zoom: 0.92, travel: 7.0, hold: 1.0 },
];

/// Playback state. `playing` is read by `camera_control` to yield the camera.
#[derive(Resource, Default)]
pub struct Flythrough {
    pub playing: bool,
    /// Seconds elapsed since playback started.
    t: f32,
}

/// Drive (or cancel) the scripted path. Runs after `camera_control` so it has the
/// final say on the camera transform while playing.
pub fn play_flythrough(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    mut fly: ResMut<Flythrough>,
    mut controller: ResMut<CameraController>,
    site: Res<Site>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
) -> Result {
    // `F` toggles: start from the top, or stop and keep the current framing.
    if keys.just_pressed(KeyCode::KeyF) {
        fly.playing = !fly.playing;
        fly.t = 0.0;
    }
    if !fly.playing {
        return Ok(());
    }

    let Ok((mut transform, mut projection)) = camera.single_mut() else {
        return Ok(());
    };
    let Projection::Orthographic(ortho) = projection.as_mut() else {
        return Ok(());
    };

    // Any manual input grabs control back: stop and seed the smoothing target with
    // the current scale so `camera_control` resumes without a jump.
    let touched_wheel = wheel.read().next().is_some();
    if keys.just_pressed(KeyCode::Escape)
        || buttons.just_pressed(MouseButton::Left)
        || touched_wheel
    {
        controller.target_scale = ortho.scale;
        fly.playing = false;
        return Ok(());
    }

    fly.t += time.delta_secs();

    let window_h = windows.single().map(|w| w.size().y).unwrap_or(1.0).max(1.0);

    match eval(fly.t) {
        Some((pos_frac, zoom)) => {
            let wb = site.0.world_bbox;
            let (min, max) = (Vec2::new(wb[0], wb[1]), Vec2::new(wb[2], wb[3]));
            let center = (min + max) * 0.5;
            let half_extent = (max - min) * 0.5;

            // Resolve site-relative framing against the live manifest + window.
            let min_scale = controller.min_scale;
            let max_scale = (half_extent.y * 2.0 / window_h).max(min_scale);
            let scale = lerp_log(min_scale, max_scale, zoom);

            ortho.scale = scale;
            let win = windows.single().map(|w| w.size()).unwrap_or(Vec2::splat(1.0));
            let clamped = clamp_view(center + pos_frac * half_extent, win * 0.5 * scale, wb);
            transform.translation.x = clamped.x;
            transform.translation.y = clamped.y;
        }
        None => {
            // Reached the end: hand control back at the final scale.
            controller.target_scale = ortho.scale;
            fly.playing = false;
        }
    }

    Ok(())
}

/// Sample the path at time `t` (seconds), returning the interpolated
/// `(pos_frac, zoom)` site-relative framing, or `None` once the path has finished.
/// Both ease with smoothstep; since `zoom` maps to scale geometrically downstream,
/// equal `zoom` steps read as a constant-rate zoom.
fn eval(t: f32) -> Option<(Vec2, f32)> {
    let first = KEYFRAMES[0];
    if t <= first.hold {
        return Some((first.pos, first.zoom));
    }
    let mut acc = first.hold;
    let mut cur = first;
    for &next in &KEYFRAMES[1..] {
        if t <= acc + next.travel {
            let s = smoothstep((t - acc) / next.travel);
            return Some((cur.pos.lerp(next.pos, s), lerp(cur.zoom, next.zoom, s)));
        }
        acc += next.travel;
        if t <= acc + next.hold {
            return Some((next.pos, next.zoom));
        }
        acc += next.hold;
        cur = next;
    }
    None
}

/// Smoothstep ease (`3x²−2x³`) on a clamped `[0, 1]` parameter.
fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Plain linear interpolation.
fn lerp(a: f32, b: f32, s: f32) -> f32 {
    a + (b - a) * s
}

/// Interpolate geometrically between two scales — equal `s` steps are equal zoom
/// ratios, so `zoom`'s linear easing becomes a constant-rate visual zoom.
fn lerp_log(a: f32, b: f32, s: f32) -> f32 {
    (a.ln() * (1.0 - s) + b.ln() * s).exp()
}
