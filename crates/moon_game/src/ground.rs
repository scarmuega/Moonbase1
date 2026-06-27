//! Sprint 03 · Plan 01 — the world/camera foundation the game-object layer stands on.
//!
//! The Sprint 02 terrain mesh is **flat on the CPU**: it is a plane in the XZ plane,
//! displaced only in the vertex shader (`assets/shaders/terrain.wgsl`). The GPU knows the
//! relief; the CPU does not. So any gameplay query — "where is the ground under the cursor?",
//! "how steep is it here?", "what Y should this entity sit at?" — cannot raycast the mesh
//! (it would hit the flat plane). This module adds a **CPU height field** ([`TerrainField`])
//! that reads the same DEM the shader reads and decodes it byte-for-byte, plus:
//!
//! - [`CursorGround`] / [`cursor_ground`] — ray-march the mouse onto the real surface.
//! - [`GroundAnchor`] / [`ground_on_spawn`] — glue a spawned entity's `Transform.y` to it.
//! - [`ProjectionMode`] / [`toggle_projection`] — perspective orbit ↔ orthographic isometric.
//!
//! Vertical exaggeration is the constant [`TERRAIN_VEXAG`] — shared by the terrain renderer
//! (`terrain3d.rs`) and grounding here so the rendered surface and placed entities never
//! diverge. Slope is computed in **true metres** (unscaled) so buildability reflects the real
//! Moon regardless of the rendering exaggeration.

use bevy::camera::ScalingMode;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use moon_data::{world_per_pixel, world_to_dem_uv, SiteManifest};

use crate::flythrough::Flythrough;
use crate::terrain3d::{orbit_camera, CameraRig, MainCamera};
use crate::Site;

/// Vertical exaggeration — a **constant**, shared by the terrain shader uniform and CPU
/// grounding. The poles' relief is gentle; exaggerate it so craters read. Tuned once, never
/// animated: a mismatch between renderer and grounding would float or sink every entity.
pub const TERRAIN_VEXAG: f32 = 1.5;

/// Locked oblique pitch (degrees) for the orthographic "isometric" projection.
const ISO_PITCH_DEG: f32 = 35.0;

/// Ordering hooks other plugins depend on without naming `ground.rs`'s private systems:
/// `LoadField` (Startup — the height field is loaded) and `Cursor` (Update — `CursorGround` is
/// resolved). `build.rs` orders its lander spawn after `LoadField` and its ghost update after
/// `Cursor`. A set names the *contract*, so it survives a rename/split of the underlying system.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GroundSet {
    LoadField,
    Cursor,
}

/// CPU copy of the baked DEM (16-bit grayscale), decoded identically to the shader. The
/// rendered DEM is a render-world `Image`, so we read the PNG independently at startup.
#[derive(Resource)]
pub struct TerrainField {
    width: u32,
    height: u32,
    /// Row-major, row 0 = north edge (matches the DEM UV v-flip in `world_to_dem_uv`).
    samples: Vec<u16>,
    elev_min: f32,
    elev_max: f32,
}

impl TerrainField {
    /// Normalised `[0,1]` sample at integer texel `(x, y)`, clamped to the grid.
    fn sample_norm(&self, x: i32, y: i32) -> f32 {
        let xi = x.clamp(0, self.width as i32 - 1) as usize;
        let yi = y.clamp(0, self.height as i32 - 1) as usize;
        self.samples[yi * self.width as usize + xi] as f32 / 65535.0
    }

    /// Bilinear elevation in **true metres** at world `xy`. Mirrors `terrain.wgsl`'s decode:
    /// `elev = elev_min + (raw/65535)·(elev_max − elev_min)`. Callers multiply by
    /// [`TERRAIN_VEXAG`] for the rendered 3D Y; slope math uses these raw metres.
    pub fn height_at(&self, m: &SiteManifest, xy: Vec2) -> f32 {
        let uv = world_to_dem_uv(m, xy); // single source of truth for the y-flip
        let fx = (uv.x * (self.width - 1) as f32).clamp(0.0, (self.width - 1) as f32);
        let fy = (uv.y * (self.height - 1) as f32).clamp(0.0, (self.height - 1) as f32);
        let x0 = fx.floor() as i32;
        let y0 = fy.floor() as i32;
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;

        let s00 = self.sample_norm(x0, y0);
        let s10 = self.sample_norm(x0 + 1, y0);
        let s01 = self.sample_norm(x0, y0 + 1);
        let s11 = self.sample_norm(x0 + 1, y0 + 1);
        let top = s00 + (s10 - s00) * tx;
        let bot = s01 + (s11 - s01) * tx;
        let s = top + (bot - top) * ty;

        self.elev_min + s * (self.elev_max - self.elev_min)
    }

    /// Ground slope (degrees) via central finite difference one base DEM pixel E/N of `xy`,
    /// in **unscaled** metres — independent of [`TERRAIN_VEXAG`]. Used by `build.rs` for
    /// footprint buildability and the lander's lowest-slope spawn scan.
    pub fn slope_deg_at(&self, m: &SiteManifest, xy: Vec2) -> f32 {
        let e = world_per_pixel(m, 0) as f32;
        let dzdx = (self.height_at(m, xy + Vec2::new(e, 0.0)) - self.height_at(m, xy - Vec2::new(e, 0.0))) / (2.0 * e);
        let dzdy = (self.height_at(m, xy + Vec2::new(0.0, e)) - self.height_at(m, xy - Vec2::new(0.0, e))) / (2.0 * e);
        dzdx.hypot(dzdy).atan().to_degrees()
    }
}

/// Read the baked DEM PNG on the CPU (the rendered copy lives only in the render world).
fn load_terrain_field(mut commands: Commands, site: Res<Site>) {
    let m = &site.0;
    let path = format!("{}/{}", crate::ASSET_ROOT, m.dem.path);
    let img = image::open(&path)
        .unwrap_or_else(|e| panic!("opening DEM {path}: {e}"))
        .into_luma16();
    let (width, height) = (img.width(), img.height());
    commands.insert_resource(TerrainField {
        width,
        height,
        samples: img.into_raw(),
        elev_min: m.dem.elev_min_m as f32,
        elev_max: m.dem.elev_max_m as f32,
    });
}

/// World XY (north-Y) point under the cursor on the real surface; `None` off-terrain or over
/// the egui overlay.
#[derive(Resource, Default)]
pub struct CursorGround(pub Option<Vec2>);

/// Resolve the cursor to a point on the real relief by ray-marching the height field.
/// Projection-agnostic: `viewport_to_world` yields a diverging ray in perspective and a
/// parallel one in orthographic, and the march handles both.
fn cursor_ground(
    mut contexts: EguiContexts,
    field: Res<TerrainField>,
    site: Res<Site>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cam_q: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut cursor: ResMut<CursorGround>,
) {
    cursor.0 = None;
    // Yield to egui: don't pick the ground through a panel.
    if let Ok(ctx) = contexts.ctx_mut()
        && ctx.wants_pointer_input()
    {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(screen) = window.cursor_position() else { return };
    let Ok((camera, cam_gt)) = cam_q.single() else { return };
    let Ok(ray) = camera.viewport_to_world(cam_gt, screen) else { return };

    let m = &site.0;
    let diag = (m.world_max() - m.world_min()).length();
    cursor.0 = march_to_ground(&field, m, ray, diag);
}

/// March `ray` against the height field; on the first crossing of `p.y − surface_y`, bisect
/// for a sub-step hit. Returns the world XY of the hit, or `None` (ray never dips below the
/// surface — looking at the sky).
fn march_to_ground(field: &TerrainField, m: &SiteManifest, ray: Ray3d, diag: f32) -> Option<Vec2> {
    let step = diag / 512.0;
    let max_t = diag * 6.0;
    let signed = |t: f32| -> f32 {
        let p = ray.get_point(t);
        p.y - field.height_at(m, Vec2::new(p.x, -p.z)) * TERRAIN_VEXAG
    };

    let mut t_prev = 0.0;
    let mut f_prev = signed(t_prev);
    let mut t = step;
    while t <= max_t {
        let f = signed(t);
        if f_prev > 0.0 && f <= 0.0 {
            let (mut lo, mut hi) = (t_prev, t);
            for _ in 0..8 {
                let mid = 0.5 * (lo + hi);
                if signed(mid) > 0.0 {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let p = ray.get_point(0.5 * (lo + hi));
            return Some(Vec2::new(p.x, -p.z));
        }
        t_prev = t;
        f_prev = f;
        t += step;
    }
    None
}

/// Authoritative planar position of a grounded entity. `Transform.y` is derived from the
/// height field at spawn (and re-derived if `xy` changes). `half_height` lifts a centre-origin
/// mesh so it rests *on* the surface rather than half-buried.
#[derive(Component, Copy, Clone)]
pub struct GroundAnchor {
    pub xy: Vec2,
    pub half_height: f32,
}

/// Derive `Transform` from `GroundAnchor` once per entity (and again only if `xy` changes —
/// `Changed` covers both). Vexag is constant, so there is no per-frame re-grounding.
fn ground_on_spawn(
    field: Res<TerrainField>,
    site: Res<Site>,
    mut q: Query<(&GroundAnchor, &mut Transform), Changed<GroundAnchor>>,
) {
    let m = &site.0;
    for (anchor, mut t) in &mut q {
        let h = field.height_at(m, anchor.xy) * TERRAIN_VEXAG;
        t.translation = Vec3::new(anchor.xy.x, h + anchor.half_height, -anchor.xy.y);
    }
}

/// Camera projection: the default perspective orbit, or a distortion-free orthographic
/// "isometric" view for base-building readability.
#[derive(Resource, Default, PartialEq, Clone, Copy)]
pub enum ProjectionMode {
    #[default]
    Perspective,
    Iso,
}

/// One-shot request to flip the projection, set by the egui "Isometric/Perspective view" button
/// (`build.rs`) so it mirrors the `P` key. The button must *not* flip [`ProjectionMode`] itself —
/// only [`toggle_projection`] reconfigures the camera, so flipping the mode alone would desync it.
#[derive(Resource, Default)]
pub struct ProjectionToggleRequest(pub bool);

/// `P` flips perspective ↔ isometric. In iso the pitch is locked oblique and the wheel zooms
/// by changing the orthographic extent (driven from `rig.distance`, kept in sync each frame).
/// Runs after `orbit_camera` so it reads the up-to-date `rig.distance`.
fn toggle_projection(
    keys: Res<ButtonInput<KeyCode>>,
    site: Res<Site>,
    mut mode: ResMut<ProjectionMode>,
    mut rig: ResMut<CameraRig>,
    mut fly: ResMut<Flythrough>,
    mut req: ResMut<ProjectionToggleRequest>,
    mut cam: Query<(&mut Projection, &mut Transform), With<MainCamera>>,
) {
    let m = &site.0;
    let diag = (m.world_max() - m.world_min()).length();
    let Ok((mut projection, mut transform)) = cam.single_mut() else { return };

    // `P` key or the egui button (consumed here so it fires once).
    if keys.just_pressed(KeyCode::KeyP) || std::mem::take(&mut req.0) {
        *mode = match *mode {
            ProjectionMode::Perspective => ProjectionMode::Iso,
            ProjectionMode::Iso => ProjectionMode::Perspective,
        };
        match *mode {
            ProjectionMode::Iso => {
                rig.pitch = ISO_PITCH_DEG.to_radians(); // locked oblique angle
                fly.playing = false; // the flythrough is a perspective-only beauty pass
                // viewport_height ≈ the perspective vertical extent at the target
                // (2·distance·tan(fov/2), fov 50° ≈ distance) so framing matches across toggle.
                *projection = Projection::Orthographic(OrthographicProjection {
                    scaling_mode: ScalingMode::FixedVertical { viewport_height: rig.distance },
                    near: -2.0 * diag,
                    far: 4.0 * diag,
                    ..OrthographicProjection::default_3d()
                });
            }
            ProjectionMode::Perspective => {
                *projection = Projection::Perspective(PerspectiveProjection {
                    fov: rig.fov,
                    near: crate::terrain3d::CAMERA_NEAR,
                    far: diag * 4.0,
                    ..default()
                });
            }
        }
        *transform = rig.transform(); // apply the locked-pitch eye immediately
    } else if *mode == ProjectionMode::Iso {
        // Keep the ortho extent synced to the wheel-driven distance ("zoom").
        if let Projection::Orthographic(ortho) = projection.as_mut() {
            ortho.scaling_mode = ScalingMode::FixedVertical { viewport_height: rig.distance };
        }
    }
}

pub struct GroundPlugin;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CursorGround>()
            .init_resource::<ProjectionMode>()
            .init_resource::<ProjectionToggleRequest>()
            .add_systems(Startup, load_terrain_field.in_set(GroundSet::LoadField))
            .add_systems(
                Update,
                (
                    cursor_ground.in_set(GroundSet::Cursor),
                    ground_on_spawn,
                    toggle_projection.after(orbit_camera),
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 km square site centred on the origin, 10 m/px base. DEM dims are irrelevant to the
    /// field math (the field carries its own width/height); only the world bbox + base_mpp are
    /// used by `world_to_dem_uv` / `world_per_pixel`.
    fn test_manifest() -> SiteManifest {
        use moon_data::{DemInfo, SiteId, SunDefaults};
        SiteManifest {
            site: SiteId("test".to_string()),
            projected_bbox: [-1000.0, -1000.0, 1000.0, 1000.0],
            world_bbox: [-1000.0, -1000.0, 1000.0, 1000.0],
            base_mpp: 10.0,
            tile_size_px: 512,
            zoom_levels: vec![],
            dem: DemInfo {
                path: "elevation/test.png".to_string(),
                width: 2,
                height: 2,
                elev_min_m: 0.0,
                elev_max_m: 1000.0,
            },
            sun: SunDefaults { azimuth_deg: 0.0, altitude_deg: 30.0 },
            crs_proj4: String::new(),
        }
    }

    fn field(width: u32, height: u32, samples: Vec<u16>, elev_min: f32, elev_max: f32) -> TerrainField {
        assert_eq!(samples.len(), (width * height) as usize);
        TerrainField { width, height, samples, elev_min, elev_max }
    }

    #[test]
    fn corners_decode_with_yflip() {
        // 2×2 field: row 0 = north. samples = [NW, NE, SW, SE].
        let m = test_manifest();
        let f = field(2, 2, vec![0, 65535 / 4, 65535 / 2, 65535], 0.0, 1000.0);
        let (min, max) = (m.world_min(), m.world_max());
        // NW world (min_x, max_y) → texel (0,0) = 0 → 0 m.
        assert!((f.height_at(&m, Vec2::new(min.x, max.y)) - 0.0).abs() < 1e-2);
        // NE world (max_x, max_y) → texel (1,0) → 250 m.
        assert!((f.height_at(&m, Vec2::new(max.x, max.y)) - 250.0).abs() < 1.0);
        // SW world (min_x, min_y) → texel (0,1) → 500 m.
        assert!((f.height_at(&m, Vec2::new(min.x, min.y)) - 500.0).abs() < 1.0);
        // SE world (max_x, min_y) → texel (1,1) → 1000 m.
        assert!((f.height_at(&m, Vec2::new(max.x, min.y)) - 1000.0).abs() < 1e-2);
    }

    #[test]
    fn flat_field_is_zero_slope() {
        let m = test_manifest();
        let f = field(4, 4, vec![20_000; 16], 0.0, 1000.0);
        for probe in [Vec2::ZERO, Vec2::new(300.0, -200.0), Vec2::new(-500.0, 400.0)] {
            assert!(f.slope_deg_at(&m, probe) < 1e-3, "slope at {probe:?}");
        }
    }

    #[test]
    fn constant_x_ramp_matches_atan_gradient() {
        // samples constant in y, linear 0→1 in x ⇒ elevation linear in world x.
        let m = test_manifest();
        let f = field(2, 2, vec![0, 65535, 0, 65535], 0.0, 1000.0);
        // gradient = (elev_max−elev_min)/span_x = 1000/2000 = 0.5 ⇒ atan(0.5) = 26.565°.
        let expected = 0.5_f32.atan().to_degrees();
        assert!((f.slope_deg_at(&m, Vec2::ZERO) - expected).abs() < 1e-2, "got {}", f.slope_deg_at(&m, Vec2::ZERO));
    }

    #[test]
    fn height_is_continuous_across_texel_boundary() {
        // 3×3 varied field; a tiny step around an interior texel boundary must not jump.
        let m = test_manifest();
        let f = field(3, 3, vec![0, 30_000, 60_000, 10_000, 40_000, 65_000, 20_000, 50_000, 65_535], 0.0, 1000.0);
        // World x for the column-1 texel centre: uv.x = 1/(3-1) = 0.5 ⇒ x = 0.
        let d = 0.05; // metres — far below a texel (1000 m wide here)
        let lo = f.height_at(&m, Vec2::new(-d, 0.0));
        let hi = f.height_at(&m, Vec2::new(d, 0.0));
        assert!((hi - lo).abs() < 1.0, "discontinuity: {lo} vs {hi}");
    }
}
