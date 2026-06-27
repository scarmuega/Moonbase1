//! Scripted cinematic camera path (Sprint 02 · Tier 2 — ported to 3D).
//!
//! `F` plays a fixed keyframed orbit/descent for repeatable trailer takes; any mouse
//! drag/zoom or `Esc` cancels and hands control back without a jump. While playing,
//! [`Flythrough::playing`] is set and `orbit_camera` (in `terrain3d.rs`) yields, so this
//! system owns the camera. It animates the [`CameraRig`] (yaw / pitch / distance) and
//! writes the resulting transform, so when it stops the orbit controls simply continue
//! from wherever the path left off.
//!
//! Keyframes are site-relative (angles + a log-distance fraction between the rig's min
//! and max), so one path reads well on any baked site: a wide establishing orbit, a
//! banking push toward a rim quadrant, a dive low over the rim into the shadowed
//! interior, then a pull back out.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

use crate::ground::ProjectionMode;
use crate::terrain3d::{CameraRig, MainCamera};

/// One stop on the path. `dist_frac` is `0` = closest (rig min distance) … `1` = widest
/// (rig max distance), interpolated geometrically.
#[derive(Clone, Copy)]
struct Keyframe {
    yaw_deg: f32,
    pitch_deg: f32,
    dist_frac: f32,
    /// Seconds easing *into* this stop (ignored for the first, which is a hard cut).
    travel: f32,
    /// Seconds dwelling once arrived.
    hold: f32,
}

const KEYFRAMES: &[Keyframe] = &[
    // Establishing: high and wide.
    Keyframe { yaw_deg: 30.0, pitch_deg: 55.0, dist_frac: 0.9, travel: 0.0, hold: 2.5 },
    // Bank toward a rim quadrant, pushing in.
    Keyframe { yaw_deg: 120.0, pitch_deg: 35.0, dist_frac: 0.45, travel: 6.0, hold: 1.5 },
    // Dive low over the rim into the shadowed interior.
    Keyframe { yaw_deg: 200.0, pitch_deg: 12.0, dist_frac: 0.16, travel: 7.0, hold: 2.0 },
    // Sweep across at low altitude.
    Keyframe { yaw_deg: 280.0, pitch_deg: 14.0, dist_frac: 0.22, travel: 6.0, hold: 1.5 },
    // Pull back out to an establishing frame.
    Keyframe { yaw_deg: 390.0, pitch_deg: 50.0, dist_frac: 0.9, travel: 7.0, hold: 1.0 },
];

/// Playback state. `playing` is read by `orbit_camera` to yield the camera.
#[derive(Resource, Default)]
pub struct Flythrough {
    pub playing: bool,
    /// Seconds elapsed since playback started.
    t: f32,
}

/// Drive (or cancel) the scripted path.
#[allow(clippy::too_many_arguments)] // a Bevy system's params aren't a refactor smell
pub fn play_flythrough(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    proj_mode: Res<ProjectionMode>,
    mut wheel: MessageReader<MouseWheel>,
    mut fly: ResMut<Flythrough>,
    mut rig: ResMut<CameraRig>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
) {
    // The scripted path is a perspective-only beauty pass; `F` is a no-op in isometric.
    if *proj_mode == ProjectionMode::Iso {
        fly.playing = false;
        return;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        fly.playing = !fly.playing;
        fly.t = 0.0;
    }
    if !fly.playing {
        return;
    }

    // Any manual input grabs control back; the rig already holds the current framing.
    let touched_wheel = wheel.read().next().is_some();
    if keys.just_pressed(KeyCode::Escape) || buttons.just_pressed(MouseButton::Left) || touched_wheel {
        fly.playing = false;
        return;
    }

    fly.t += time.delta_secs();

    match eval(fly.t) {
        Some((yaw_deg, pitch_deg, dist_frac)) => {
            rig.yaw = yaw_deg.to_radians();
            rig.pitch = pitch_deg.to_radians();
            // Geometric (log) interpolation of distance reads as a constant-rate dolly.
            rig.distance = lerp_log(rig.min_distance, rig.max_distance, dist_frac);
            if let Ok(mut t) = camera.single_mut() {
                *t = rig.transform();
            }
        }
        None => fly.playing = false,
    }
}

/// Sample the path at time `t` (seconds): `(yaw_deg, pitch_deg, dist_frac)`, or `None`
/// once finished. Each leg eases with smoothstep.
fn eval(t: f32) -> Option<(f32, f32, f32)> {
    let first = KEYFRAMES[0];
    if t <= first.hold {
        return Some((first.yaw_deg, first.pitch_deg, first.dist_frac));
    }
    let mut acc = first.hold;
    let mut cur = first;
    for &next in &KEYFRAMES[1..] {
        if t <= acc + next.travel {
            let s = smoothstep((t - acc) / next.travel);
            return Some((
                lerp(cur.yaw_deg, next.yaw_deg, s),
                lerp(cur.pitch_deg, next.pitch_deg, s),
                lerp(cur.dist_frac, next.dist_frac, s),
            ));
        }
        acc += next.travel;
        if t <= acc + next.hold {
            return Some((next.yaw_deg, next.pitch_deg, next.dist_frac));
        }
        acc += next.hold;
        cur = next;
    }
    None
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn lerp(a: f32, b: f32, s: f32) -> f32 {
    a + (b - a) * s
}

/// Geometric interpolation between two distances — equal `s` steps are equal ratios.
fn lerp_log(a: f32, b: f32, s: f32) -> f32 {
    (a.ln() * (1.0 - s) + b.ln() * s).exp()
}
