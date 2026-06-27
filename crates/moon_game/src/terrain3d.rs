//! Sprint 02 — the 3D lunar terrain view (primary and only view).
//!
//! The app boots into a perspective `Camera3d` orbiting the baked LOLA DEM, rendered as
//! real displaced geometry. There is **no streamed imagery**: the surface look comes
//! from per-fragment DEM relief shading plus a synthetic "dusty" procedural texture
//! (see `assets/shaders/terrain.wgsl`), which reads far cleaner than draping the
//! low-value LROC patches. The sun is the shared [`HillshadeState`] parameter (egui
//! sliders + `G` sweep), fed into the shader.
//!
//! Geometry is displaced in the vertex shader and the fragment stage samples the DEM per
//! fragment for a crisp normal — independent of mesh density. Vertical exaggeration is the
//! constant [`crate::ground::TERRAIN_VEXAG`] (shared with CPU grounding so the rendered
//! surface and placed entities never diverge). Coordinate mapping: world `(x, y)` +Y = north
//! → 3D `(x, h·vexag, -y)`.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy_egui::{EguiContexts, PrimaryEguiContext};
use moon_data::SiteManifest;

use crate::ground::{GroundSet, TerrainField, TERRAIN_VEXAG};
use crate::hillshade::HillshadeState;
use crate::Site;

/// The egui camera renders on an empty layer (no scene geometry) — it exists only to
/// host the primary egui context and draw the overlay on top of the scene camera.
const UI_LAYER: usize = 2;
/// Cap on terrain grid resolution (quads per side). The mesh is matched to the DEM's
/// native grid up to this cap so crater rims read in the *silhouette* (the per-fragment
/// normal already carries shading detail). Capped to bound vertex memory: 2048² ≈ 4.2 M
/// verts. The grid is flat and displaced on the GPU, so this is a one-time startup cost
/// and `vexag` stays a live uniform (no rebuilds).
const MAX_GRID_QUADS: u32 = 2048;
/// Default synthetic sub-DEM relief strength. Slider 0–2.
const DEFAULT_SYNTH: f32 = 0.4;
/// Orbit drag sensitivity (radians per pixel of mouse motion).
const ORBIT_SENS: f32 = 0.005;
/// Fraction of `distance` changed per wheel "line" of scroll.
const ZOOM_SPEED: f32 = 0.12;
/// Trackpad pixel-delta → line-equivalent factor (one wheel notch ≈ 16 px).
const PIXEL_TO_LINE: f32 = 1.0 / 16.0;
/// Camera near-clip (metres). A small **absolute** value so the camera can dolly down to base
/// scale (a single 5–20 m module) on any site — the old `diag·0.001` was ~170 m on southpole,
/// which clipped everything at the base. The far plane (`diag·4`) + reverse-Z keep the
/// whole-site overview precise. Shared with `ground::toggle_projection` so both projections clip
/// identically.
pub(crate) const CAMERA_NEAR: f32 = 10.0;

/// Orbit/perspective camera rig. Distances derive from the site bbox so it adapts to any
/// baked site.
#[derive(Resource)]
pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub min_distance: f32,
    pub max_distance: f32,
    pub fov: f32,
}

impl CameraRig {
    fn from_manifest(m: &SiteManifest) -> Self {
        let (min, max) = (m.world_min(), m.world_max());
        let center = (min + max) * 0.5;
        let diag = (max - min).length();
        Self {
            yaw: 45f32.to_radians(),
            pitch: 35f32.to_radians(),
            // Buildables are metre-scale, so the working framing is an absolute distance, not a
            // fraction of the (km-scale) site: start at the base near origin and let the wheel
            // dolly down to a single module (`min_distance`) or out to the whole-site overview
            // (`max_distance`). `F` flythrough still sweeps the full range.
            distance: 500.0,
            // `y` is a placeholder: the manifest knows `elev_min/max` but not the elevation
            // *at the base*, and the terrain renders at absolute elevations. Corrected onto
            // the real surface by `frame_base_on_surface` once the CPU height field loads.
            target: Vec3::new(center.x, 0.0, -center.y),
            min_distance: 50.0,
            max_distance: diag * 2.0,
            fov: 50f32.to_radians(),
        }
    }

    /// Eye transform from spherical coords, looking at the site center.
    pub fn transform(&self) -> Transform {
        let cp = self.pitch.cos();
        let dir = Vec3::new(cp * self.yaw.sin(), self.pitch.sin(), cp * self.yaw.cos());
        Transform::from_translation(self.target + dir * self.distance).looking_at(self.target, Vec3::Y)
    }
}

/// Live surface-look control (egui-tunable).
#[derive(Resource)]
pub struct TerrainLook {
    /// Synthetic sub-DEM relief strength (0 = real DEM relief only).
    pub synth: f32,
}

impl Default for TerrainLook {
    fn default() -> Self {
        Self { synth: DEFAULT_SYNTH }
    }
}

/// Uniform block for `terrain.wgsl`; field order/types must match the WGSL struct.
#[derive(Clone, ShaderType)]
pub struct TerrainParams {
    pub dem_world_min: Vec2,
    pub dem_world_size: Vec2,
    pub elev_min: f32,
    pub elev_max: f32,
    pub sun_azimuth: f32,
    pub sun_altitude: f32,
    pub vexag: f32,
    pub synth: f32,
}

/// The terrain material: a uniform block + the R16Uint DEM (vertex + fragment,
/// `textureLoad`, no sampler — Bevy decodes the 16-bit PNG to an integer texture).
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct TerrainMaterial {
    #[uniform(0)]
    pub params: TerrainParams,
    #[texture(1, sample_type = "u_int")]
    pub dem: Handle<Image>,
}

impl Material for TerrainMaterial {
    fn vertex_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/terrain.wgsl".into()
    }
}

/// Handle to the terrain material so the sync system can rewrite its uniforms.
#[derive(Resource)]
struct TerrainHandle(Handle<TerrainMaterial>);

/// Marks the main scene camera (the perspective one the player drives).
#[derive(Component)]
pub struct MainCamera;

pub struct Terrain3dPlugin;

impl Plugin for Terrain3dPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default())
            .init_resource::<TerrainLook>()
            // `frame_base_on_surface` needs the rig + `MainCamera` (from `setup`) and the
            // CPU height field (from `GroundSet::LoadField`), so it runs after both.
            .add_systems(
                Startup,
                (setup, frame_base_on_surface.after(setup).after(GroundSet::LoadField)),
            )
            .add_systems(Update, (orbit_camera, sync_terrain));
    }
}

fn setup(
    mut commands: Commands,
    site: Res<Site>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    look: Res<TerrainLook>,
) {
    let m = &site.0;
    let min = m.world_min();
    let max = m.world_max();
    let diag = (max - min).length();
    let rig = CameraRig::from_manifest(m);

    let material = materials.add(TerrainMaterial {
        params: TerrainParams {
            dem_world_min: min,
            dem_world_size: max - min,
            elev_min: m.dem.elev_min_m as f32,
            elev_max: m.dem.elev_max_m as f32,
            sun_azimuth: m.sun.azimuth_deg.to_radians(),
            sun_altitude: m.sun.altitude_deg.to_radians(),
            vexag: crate::ground::TERRAIN_VEXAG,
            synth: look.synth,
        },
        dem: asset_server.load(m.dem.path.clone()),
    });

    // Match the mesh to the DEM's native grid (capped) so the silhouette is as detailed
    // as the data allows.
    let quads = m.dem.width.max(m.dem.height).min(MAX_GRID_QUADS);
    commands.spawn((
        Mesh3d(meshes.add(build_grid(min, max, quads))),
        MeshMaterial3d(material.clone()),
        Transform::default(),
        // The terrain mesh is flat on the CPU (displaced only in the vertex shader), so a mesh
        // ray would hit the wrong surface — exclude it from picking (selection in `selection.rs`).
        Pickable::IGNORE,
    ));

    // Main perspective scene camera.
    commands.spawn((
        Camera3d::default(),
        Camera { order: 0, ..default() },
        Projection::Perspective(PerspectiveProjection {
            fov: rig.fov,
            near: CAMERA_NEAR,
            far: diag * 4.0,
            ..default()
        }),
        rig.transform(),
        MainCamera,
    ));

    // Dedicated, always-active egui camera (highest order, no clear) — keeps the egui
    // context on one stable, initialized camera (avoids bevy_egui "No fonts loaded").
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

    commands.insert_resource(rig);
    commands.insert_resource(TerrainHandle(material));
}

/// Lift the camera onto the real terrain surface at the base. The rig is built from the
/// manifest alone (`CameraRig::from_manifest`), which can't know the elevation at the base,
/// so it defaults the look-at to `y = 0`. But the terrain renders at *absolute* lunar
/// elevations (`height_at · vexag`, ±km around datum), so `y = 0` spawns the eye under (or
/// far below) the ground — the base and lander end up out of frame. Runs once at startup,
/// after the height field loads, and re-applies the camera transform with the fixed target.
fn frame_base_on_surface(
    site: Res<Site>,
    field: Res<TerrainField>,
    mut rig: ResMut<CameraRig>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
) {
    let m = &site.0;
    let base_xy = (m.world_min() + m.world_max()) * 0.5; // base = site center (≈ origin)
    rig.target = Vec3::new(base_xy.x, field.height_at(m, base_xy) * TERRAIN_VEXAG, -base_xy.y);
    if let Ok(mut t) = cam.single_mut() {
        *t = rig.transform();
    }
}

/// Hand-rolled subdivided plane over the world bbox, in the XZ plane (y = 0; height
/// comes from the vertex shader). Front faces point +Y under the default CCW winding.
/// Only positions are stored — the fragment shader derives its own normal from the DEM,
/// so the mesh normal attribute is omitted (halves the vertex memory).
fn build_grid(min: Vec2, max: Vec2, n: u32) -> Mesh {
    let verts = ((n + 1) * (n + 1)) as usize;
    let mut positions = Vec::with_capacity(verts);
    for j in 0..=n {
        let fz = j as f32 / n as f32;
        let wy = max.y + (min.y - max.y) * fz; // j: 0 → north (max_y)
        for i in 0..=n {
            let fx = i as f32 / n as f32;
            let wx = min.x + (max.x - min.x) * fx;
            positions.push([wx, 0.0, -wy]);
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
        .with_inserted_indices(Indices::U32(indices))
}

/// Orbit the camera: left-drag rotates (yaw/pitch), wheel dollies. Yields the pointer
/// to egui. Skips while the scripted flythrough owns the camera. In isometric mode the pitch
/// is locked (yaw + wheel still apply); `toggle_projection` maps the wheel-driven distance to
/// the orthographic extent.
#[allow(clippy::too_many_arguments)] // a Bevy system's params aren't a refactor smell
pub(crate) fn orbit_camera(
    mut contexts: EguiContexts,
    flythrough: Res<crate::flythrough::Flythrough>,
    proj_mode: Res<crate::ground::ProjectionMode>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut motion: MessageReader<MouseMotion>,
    mut wheel: MessageReader<MouseWheel>,
    mut rig: ResMut<CameraRig>,
    mut cam: Query<&mut Transform, With<MainCamera>>,
) -> Result {
    if flythrough.playing {
        motion.clear();
        wheel.clear();
        return Ok(());
    }
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
    if *proj_mode == crate::ground::ProjectionMode::Iso {
        dpitch = 0.0; // pitch is locked oblique in the isometric projection
    }

    let mut scroll = 0.0;
    for ev in wheel.read() {
        scroll += match ev.unit {
            MouseScrollUnit::Line => ev.y,
            MouseScrollUnit::Pixel => ev.y * PIXEL_TO_LINE,
        };
    }

    if dyaw != 0.0 || dpitch != 0.0 || scroll != 0.0 {
        rig.yaw += dyaw;
        rig.pitch = (rig.pitch + dpitch).clamp(5f32.to_radians(), 85f32.to_radians());
        if scroll != 0.0 {
            rig.distance = (rig.distance * (1.0 - scroll * ZOOM_SPEED)).clamp(rig.min_distance, rig.max_distance);
        }
        if let Ok(mut t) = cam.single_mut() {
            *t = rig.transform();
        }
    }

    Ok(())
}

/// Push the live sun angles (shared [`HillshadeState`]) + dust into the material uniforms when
/// a source changes. Vexag is the constant [`crate::ground::TERRAIN_VEXAG`], set once at build.
fn sync_terrain(
    state: Res<HillshadeState>,
    look: Res<TerrainLook>,
    handle: Res<TerrainHandle>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
) {
    if !state.is_changed() && !look.is_changed() {
        return;
    }
    if let Some(mat) = materials.get_mut(&handle.0) {
        mat.params.sun_azimuth = state.sun_azimuth_deg.to_radians();
        mat.params.sun_altitude = state.sun_altitude_deg.to_radians();
        mat.params.synth = look.synth;
    }
}
