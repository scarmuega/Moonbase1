//! Sprint 02 · Tier 1 — an optional 3D relief view.
//!
//! A [`Camera3d`] orbits a DEM-displaced terrain mesh, killing the relief-inversion
//! illusion of the top-down 2D view with genuine parallax + occlusion. This is
//! **additive and reversible**: the existing `Camera2d`, sprites, streaming, and 2D
//! hillshade are untouched — toggling [`ViewMode`] (key **`T`**) swaps which camera is
//! active and pauses the 2D-only systems.
//!
//! The heavy lifting is in [`assets/shaders/terrain3d.wgsl`]: the vertex stage samples
//! the same R16Uint DEM as `hillshade.rs` and displaces a flat grid; the fragment stage
//! reuses the hillshade sun + Lambertian math. The sun stays a single live parameter —
//! [`HillshadeState`] feeds both the 2D hillshade and this material.
//!
//! Coordinate mapping (fixed here, reused by Tier 2): world `(x, y)` with +Y = north →
//! Bevy 3D `(x, height·vexag, -y)`, so the ground is the XZ plane and +Y is up.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy_egui::{EguiContexts, PrimaryEguiContext};
use moon_data::{tile_path, SiteManifest, TileCoord};

use crate::hillshade::HillshadeState;
use crate::Site;

/// The 3D terrain renders on render layer 1 so the active scene camera sees only its
/// own content (2D sprites/hillshade stay on the default layer 0).
const TERRAIN_LAYER: usize = 1;
/// The dedicated egui camera renders on an empty layer (no scene geometry) — it exists
/// only to host the primary egui context and draw the overlay on top.
const UI_LAYER: usize = 2;
/// Grid resolution of the terrain plane, in quads per side (≈263 k verts at 512).
const GRID_QUADS: u32 = 512;
/// Default vertical exaggeration. Shackleton's ~4.65 km over 16 km (and southpole's
/// even gentler relief) reads flat at 1×; the slider spans 1–8.
const DEFAULT_VEXAG: f32 = 2.5;
/// Orbit drag sensitivity (radians per pixel of mouse motion).
const ORBIT_SENS: f32 = 0.005;
/// Fraction of `distance` changed per wheel "line" of scroll.
const ZOOM_SPEED: f32 = 0.12;
/// Trackpad pixel-delta → line-equivalent factor (one wheel notch ≈ 16 px).
const PIXEL_TO_LINE: f32 = 1.0 / 16.0;

/// Which view the app is showing. Default is the unchanged 2D map.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ViewMode {
    #[default]
    Map2d,
    Relief3d,
}

/// Run-condition: 2D-only systems (camera/flythrough/streaming) run here.
pub fn in_map2d(mode: Res<ViewMode>) -> bool {
    *mode == ViewMode::Map2d
}

/// Run-condition: the orbit controller runs only while the 3D view is active.
fn in_relief3d(mode: Res<ViewMode>) -> bool {
    *mode == ViewMode::Relief3d
}

/// Uniform block for `terrain3d.wgsl`; field order/types must match the WGSL struct.
/// Mirrors `HillshadeParams` plus `vexag` and `surface_mode`.
#[derive(Clone, ShaderType)]
pub struct Terrain3dParams {
    pub dem_world_min: Vec2,
    pub dem_world_size: Vec2,
    pub elev_min: f32,
    pub elev_max: f32,
    pub sun_azimuth: f32,
    pub sun_altitude: f32,
    pub vexag: f32,
    /// 0 = hillshade relief, 1 = draped imagery, 2 = height-ramp debug.
    pub surface_mode: u32,
}

/// The 3D terrain material: the uniform block, the R16Uint DEM (vertex + fragment,
/// `textureLoad`, no sampler — as in `hillshade.rs`), and the single zoom-0 drape tile.
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct Terrain3dMaterial {
    #[uniform(0)]
    pub params: Terrain3dParams,
    #[texture(1, sample_type = "u_int")]
    pub dem: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub imagery: Handle<Image>,
}

impl Material for Terrain3dMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/terrain3d.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain3d.wgsl".into()
    }
}

/// Orbit camera state + the live relief controls (vexag, surface mode) the egui panel
/// writes. Distances are derived from the site's bbox so it adapts to any baked site.
#[derive(Resource)]
pub struct Relief3dController {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub min_distance: f32,
    pub max_distance: f32,
    pub vexag: f32,
    pub surface_mode: u32,
}

impl Relief3dController {
    fn from_manifest(m: &SiteManifest) -> Self {
        let (min, max) = (m.world_min(), m.world_max());
        let center = (min + max) * 0.5;
        let diag = (max - min).length();
        Self {
            // Seed an oblique framing with the sun raking across the relief.
            yaw: 45f32.to_radians(),
            pitch: 40f32.to_radians(),
            distance: diag * 0.8,
            // World +Y (north) maps to 3D -Z; ground sits at y = 0.
            target: Vec3::new(center.x, 0.0, -center.y),
            min_distance: diag * 0.08,
            max_distance: diag * 2.0,
            vexag: DEFAULT_VEXAG,
            surface_mode: 0,
        }
    }

    /// Eye transform from spherical coords, looking at the site center.
    fn transform(&self) -> Transform {
        let cp = self.pitch.cos();
        let dir = Vec3::new(cp * self.yaw.sin(), self.pitch.sin(), cp * self.yaw.cos());
        Transform::from_translation(self.target + dir * self.distance).looking_at(self.target, Vec3::Y)
    }
}

/// Handle to the terrain material, so the sync system can rewrite its uniforms.
#[derive(Resource)]
struct Terrain3dHandle(Handle<Terrain3dMaterial>);

/// Marks the orbit/perspective 3D camera (distinct from the 2D `Camera2d`).
#[derive(Component)]
struct Relief3dCamera;

pub struct Terrain3dPlugin;

impl Plugin for Terrain3dPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<Terrain3dMaterial>::default())
            .init_resource::<ViewMode>()
            .add_systems(Startup, setup_terrain3d)
            .add_systems(
                Update,
                (
                    (toggle_view_mode, apply_view_mode).chain(),
                    relief3d_orbit.run_if(in_relief3d),
                    sync_terrain_uniforms,
                ),
            );
    }
}

/// Build the terrain mesh + material and spawn the (initially inactive) 3D camera.
fn setup_terrain3d(
    mut commands: Commands,
    site: Res<Site>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<Terrain3dMaterial>>,
) {
    let m = &site.0;
    let min = m.world_min();
    let max = m.world_max();
    let size = max - min;
    let diag = size.length();

    let controller = Relief3dController::from_manifest(m);

    let material = materials.add(Terrain3dMaterial {
        params: Terrain3dParams {
            dem_world_min: min,
            dem_world_size: size,
            elev_min: m.dem.elev_min_m as f32,
            elev_max: m.dem.elev_max_m as f32,
            // Seed from the manifest's sun (same source HillshadeState uses); the live
            // angles then flow in via `sync_terrain_uniforms` once that resource exists.
            sun_azimuth: m.sun.azimuth_deg.to_radians(),
            sun_altitude: m.sun.altitude_deg.to_radians(),
            vexag: controller.vexag,
            surface_mode: controller.surface_mode,
        },
        dem: asset_server.load(m.dem.path.clone()),
        // The zoom-0 tile is a single image covering the whole site (the drape texture).
        imagery: asset_server.load(tile_path(&m.site, TileCoord::new(0, 0, 0))),
    });

    commands.spawn((
        Mesh3d(meshes.add(build_grid(min, max, GRID_QUADS))),
        MeshMaterial3d(material.clone()),
        Transform::default(),
        RenderLayers::layer(TERRAIN_LAYER),
        // Heights are displaced in the vertex shader, so the CPU-side AABB (a flat
        // plane at y = 0) doesn't reflect the real bounds — skip frustum culling.
        NoFrustumCulling,
    ));

    commands.spawn((
        Camera3d::default(),
        Camera {
            is_active: false,
            order: 1,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: 50f32.to_radians(),
            near: (diag * 0.001).max(10.0),
            far: diag * 3.0,
            ..default()
        }),
        controller.transform(),
        RenderLayers::layer(TERRAIN_LAYER),
        Relief3dCamera,
    ));

    // A dedicated, always-active camera hosts the primary egui context and draws the
    // overlay last (highest `order`, no clear) over whichever scene camera is active.
    // Keeping the context on one stable, initialized camera avoids bevy_egui's
    // "No fonts loaded" panic that hits when a context is created/relocated mid-frame.
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 2,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(UI_LAYER),
        PrimaryEguiContext,
    ));

    commands.insert_resource(controller);
    commands.insert_resource(Terrain3dHandle(material));
}

/// Hand-rolled subdivided plane over the world bbox, in the XZ plane (y = 0; height
/// comes from the vertex shader). Row 0 = north (max_y) so it lines up with the DEM's
/// top-left UV origin. Front faces point +Y (up) under the default CCW winding.
fn build_grid(min: Vec2, max: Vec2, n: u32) -> Mesh {
    let verts = ((n + 1) * (n + 1)) as usize;
    let mut positions = Vec::with_capacity(verts);
    let mut normals = Vec::with_capacity(verts);
    for j in 0..=n {
        let fz = j as f32 / n as f32;
        // j: 0 → north (max_y), n → south (min_y). World y maps to 3D -z.
        let wy = max.y + (min.y - max.y) * fz;
        for i in 0..=n {
            let fx = i as f32 / n as f32;
            let wx = min.x + (max.x - min.x) * fx;
            positions.push([wx, 0.0, -wy]);
            normals.push([0.0, 1.0, 0.0]);
        }
    }

    let stride = n + 1;
    let mut indices = Vec::with_capacity((n * n * 6) as usize);
    for j in 0..n {
        for i in 0..n {
            let a = j * stride + i;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(Indices::U32(indices))
}

/// **`T`** swaps between the 2D map and the 3D relief.
fn toggle_view_mode(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<ViewMode>) {
    if keys.just_pressed(KeyCode::KeyT) {
        *mode = match *mode {
            ViewMode::Map2d => ViewMode::Relief3d,
            ViewMode::Relief3d => ViewMode::Map2d,
        };
    }
}

/// On a [`ViewMode`] change (and once at startup), activate exactly one scene camera.
/// egui lives on its own always-active camera, so nothing here touches egui.
///
/// The `Without<Relief3dCamera>` filter is load-bearing: two `&mut Camera` queries must
/// be provably disjoint or Bevy panics (the 3D camera also has a `Camera`).
fn apply_view_mode(
    mode: Res<ViewMode>,
    mut cam2d: Query<&mut Camera, (With<Camera2d>, Without<Relief3dCamera>)>,
    mut cam3d: Query<&mut Camera, With<Relief3dCamera>>,
) {
    if !mode.is_changed() {
        return;
    }
    let relief = *mode == ViewMode::Relief3d;
    if let Ok(mut c) = cam2d.single_mut() {
        c.is_active = !relief;
    }
    if let Ok(mut c) = cam3d.single_mut() {
        c.is_active = relief;
    }
}

/// Orbit the 3D camera: left-drag rotates (yaw/pitch), wheel changes distance. Yields
/// the pointer to egui (same guard as `camera_control`).
fn relief3d_orbit(
    mut contexts: EguiContexts,
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut ctrl: ResMut<Relief3dController>,
    mut cam: Query<&mut Transform, With<Relief3dCamera>>,
) -> Result {
    if let Ok(ctx) = contexts.ctx_mut()
        && ctx.wants_pointer_input()
    {
        motion.clear();
        wheel.clear();
        return Ok(());
    }

    let mut dyaw = 0.0;
    let mut dpitch = 0.0;
    if buttons.pressed(MouseButton::Left) {
        for ev in motion.read() {
            dyaw -= ev.delta.x * ORBIT_SENS;
            dpitch -= ev.delta.y * ORBIT_SENS;
        }
    } else {
        motion.clear();
    }

    let mut scroll = 0.0;
    for ev in wheel.read() {
        scroll += match ev.unit {
            MouseScrollUnit::Line => ev.y,
            MouseScrollUnit::Pixel => ev.y * PIXEL_TO_LINE,
        };
    }

    if dyaw != 0.0 || dpitch != 0.0 || scroll != 0.0 {
        ctrl.yaw += dyaw;
        // Clamp pitch away from the poles to avoid gimbal flip and sub-horizon views.
        ctrl.pitch = (ctrl.pitch + dpitch).clamp(5f32.to_radians(), 85f32.to_radians());
        if scroll != 0.0 {
            ctrl.distance =
                (ctrl.distance * (1.0 - scroll * ZOOM_SPEED)).clamp(ctrl.min_distance, ctrl.max_distance);
        }
        if let Ok(mut t) = cam.single_mut() {
            *t = ctrl.transform();
        }
    }

    Ok(())
}

/// Push the live sun angles (shared [`HillshadeState`]) + vexag + surface mode into the
/// material uniforms when either source changes.
fn sync_terrain_uniforms(
    state: Res<HillshadeState>,
    ctrl: Res<Relief3dController>,
    handle: Res<Terrain3dHandle>,
    mut materials: ResMut<Assets<Terrain3dMaterial>>,
) {
    if !state.is_changed() && !ctrl.is_changed() {
        return;
    }
    if let Some(mat) = materials.get_mut(&handle.0) {
        mat.params.sun_azimuth = state.sun_azimuth_deg.to_radians();
        mat.params.sun_altitude = state.sun_altitude_deg.to_radians();
        mat.params.vexag = ctrl.vexag;
        mat.params.surface_mode = ctrl.surface_mode;
    }
}
