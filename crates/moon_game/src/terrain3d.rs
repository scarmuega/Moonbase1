//! Sprint 02 · Tier 2 — 3D is the primary view.
//!
//! The app boots straight into a perspective `Camera3d` orbiting real lunar terrain:
//! the baked LOLA DEM is turned into displaced geometry and the streamed LOD imagery is
//! draped onto it as textured patches (see `streaming.rs`). A real [`DirectionalLight`]
//! sun casts shadows — the *physical* fix to the depth-reading problem and the seed for
//! the day/night / PSR story. The sun direction is the single shared parameter in
//! [`HillshadeState`] (egui sliders + `G` sweep), now driving the light instead of a
//! 2D shader.
//!
//! Terrain uses `StandardMaterial` (PBR) so it receives the sun + cascaded shadows for
//! free; the DEM is displaced on the CPU ([`DemHeights`]) and normals are computed from
//! the displaced geometry. Because the pyramid is shallow (max_zoom 3 ⇒ ≤49 tiles), the
//! streamer simply keeps the whole active-zoom level resident — no frustum math, and
//! same-zoom patches share exact edge heights, so there are no cracks.
//!
//! Coordinate mapping (fixed in Tier 1): world `(x, y)` with +Y = north → Bevy 3D
//! `(x, height·vexag, -y)`; the ground is the XZ plane and +Y is up.

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::Exposure;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::Image;
use bevy::input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy::light::CascadeShadowConfigBuilder;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy_egui::{EguiContexts, PrimaryEguiContext};
use moon_data::SiteManifest;

use crate::hillshade::HillshadeState;
use crate::Site;

/// The egui camera renders on an empty layer (no scene geometry) — it exists only to
/// host the primary egui context and draw the overlay on top of the scene camera.
const UI_LAYER: usize = 2;
/// Default vertical exaggeration; the relief at these poles is gentle. Slider 1–8.
const DEFAULT_VEXAG: f32 = 2.5;
/// Quads per side of each per-tile terrain patch (geometric fidelity vs. cost).
pub const PATCH_RES: u32 = 48;
/// Orbit drag sensitivity (radians per pixel of mouse motion).
const ORBIT_SENS: f32 = 0.005;
/// Fraction of `distance` changed per wheel "line" of scroll.
const ZOOM_SPEED: f32 = 0.12;
/// Trackpad pixel-delta → line-equivalent factor (one wheel notch ≈ 16 px).
const PIXEL_TO_LINE: f32 = 1.0 / 16.0;

/// Orbit/perspective camera rig + the live vertical-exaggeration control. Distances
/// derive from the site bbox so it adapts to any baked site.
#[derive(Resource)]
pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub target: Vec3,
    pub min_distance: f32,
    pub max_distance: f32,
    pub fov: f32,
    pub vexag: f32,
}

impl CameraRig {
    fn from_manifest(m: &SiteManifest) -> Self {
        let (min, max) = (m.world_min(), m.world_max());
        let center = (min + max) * 0.5;
        let diag = (max - min).length();
        Self {
            yaw: 45f32.to_radians(),
            pitch: 35f32.to_radians(),
            distance: diag * 0.8,
            target: Vec3::new(center.x, 0.0, -center.y),
            min_distance: diag * 0.05,
            max_distance: diag * 2.0,
            fov: 50f32.to_radians(),
            vexag: DEFAULT_VEXAG,
        }
    }

    /// Eye transform from spherical coords, looking at the site center.
    pub fn transform(&self) -> Transform {
        let cp = self.pitch.cos();
        let dir = Vec3::new(cp * self.yaw.sin(), self.pitch.sin(), cp * self.yaw.cos());
        Transform::from_translation(self.target + dir * self.distance).looking_at(self.target, Vec3::Y)
    }

    /// Approximate ground-sample distance (m/px) at the focus, for LOD selection.
    pub fn ground_sample_distance(&self, viewport_height_px: f32) -> f32 {
        let visible_world_height = 2.0 * self.distance * (self.fov * 0.5).tan();
        visible_world_height / viewport_height_px.max(1.0)
    }
}

/// DEM elevation cached on the CPU (decoded from the R16Uint heightmap) so terrain
/// patches can be displaced + normal-computed without a GPU readback.
#[derive(Resource)]
pub struct DemHeights {
    samples: Vec<u16>,
    width: usize,
    height: usize,
    world_min: Vec2,
    world_size: Vec2,
    elev_min: f32,
    elev_range: f32,
}

impl DemHeights {
    /// Elevation in meters at a world point (nearest texel), top-left = north origin —
    /// mirrors `moon_data::world_to_dem_uv` / the Tier-1 shader.
    pub fn height_at_world(&self, world: Vec2) -> f32 {
        let u = ((world.x - self.world_min.x) / self.world_size.x).clamp(0.0, 1.0);
        let v = ((self.world_min.y + self.world_size.y - world.y) / self.world_size.y).clamp(0.0, 1.0);
        let x = ((u * self.width as f32) as usize).min(self.width - 1);
        let y = ((v * self.height as f32) as usize).min(self.height - 1);
        let s = self.samples[y * self.width + x] as f32 / 65535.0;
        self.elev_min + s * self.elev_range
    }
}

/// Live look controls (egui-tunable). Defaults aim for the Tier-1 aesthetic: a direct,
/// untonemapped warm Lambertian with strong relief contrast and dark shadows.
#[derive(Resource)]
pub struct TerrainLook {
    /// Drape the streamed imagery (true) or show plain warm-gray regolith (false, the
    /// pure Tier-1 relief look — now with real cast shadows).
    pub imagery: bool,
    /// Directional sun strength (lux). ~100k = direct sunlight (paired with ev100 15).
    pub sun_lux: f32,
    /// Ambient fill strength, so shadowed crater floors aren't pure black.
    pub ambient: f32,
    /// Filmic tonemapping on (PBR look) or off (Tier-1 direct look).
    pub tonemap: bool,
}

impl Default for TerrainLook {
    fn default() -> Self {
        Self {
            imagery: true,
            sun_lux: 100_000.0,
            ambient: 2_500.0,
            tonemap: false,
        }
    }
}

/// Handle to the DEM image, used to build [`DemHeights`] once it finishes loading.
#[derive(Resource)]
struct DemHandle(Handle<Image>);

/// Marks the main scene camera (the perspective one the player drives).
#[derive(Component)]
pub struct MainCamera;

/// Marks the directional sun light.
#[derive(Component)]
struct SunLight;

pub struct Terrain3dPlugin;

impl Plugin for Terrain3dPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (build_dem_cache, orbit_camera, update_sun, apply_look));
    }
}

fn setup(mut commands: Commands, site: Res<Site>, asset_server: Res<AssetServer>) {
    let m = &site.0;
    let diag = (m.world_max() - m.world_min()).length();
    let rig = CameraRig::from_manifest(m);
    let look = TerrainLook::default();

    commands.insert_resource(DemHandle(asset_server.load(m.dem.path.clone())));

    // Main perspective scene camera. Exposure is matched to the sun (ev100 15 ≈ direct
    // sunlight) so values land in [0,1]; tonemapping starts off for the Tier-1 direct
    // Lambertian look (the BLENDER default exposure would massively overexpose 100k lux).
    commands.spawn((
        Camera3d::default(),
        Camera { order: 0, ..default() },
        Projection::Perspective(PerspectiveProjection {
            fov: rig.fov,
            near: (diag * 0.001).max(5.0),
            far: diag * 4.0,
            ..default()
        }),
        Exposure::SUNLIGHT,
        if look.tonemap { Tonemapping::TonyMcMapface } else { Tonemapping::None },
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

    // The sun: a directional light with cascaded shadow maps. Direction is set each
    // frame from HillshadeState; cast shadows give real crater self-shadowing.
    commands.spawn((
        DirectionalLight {
            illuminance: look.sun_lux,
            shadows_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            maximum_distance: diag,
            ..default()
        }
        .build(),
        Transform::from_translation(Vec3::ZERO)
            .looking_to(-sun_dir_3d(m.sun.azimuth_deg, m.sun.altitude_deg), Vec3::Y),
        SunLight,
    ));

    // A little neutral-warm ambient so shadowed crater floors aren't pure black (soft
    // PSR fill) — neutral, not blue, to keep the warm regolith mood.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(1.0, 0.97, 0.92),
        brightness: look.ambient,
        ..default()
    });

    commands.insert_resource(rig);
    commands.insert_resource(look);
}

/// Sun unit vector (ground → sun) in 3D: east = +x, up = +y, north = -z.
fn sun_dir_3d(azimuth_deg: f32, altitude_deg: f32) -> Vec3 {
    let az = azimuth_deg.to_radians();
    let alt = altitude_deg.to_radians();
    let ca = alt.cos();
    Vec3::new(ca * az.sin(), alt.sin(), -ca * az.cos())
}

/// Decode the DEM into [`DemHeights`] once the image asset is available (runs until it
/// succeeds, then idles because the resource exists).
fn build_dem_cache(
    mut commands: Commands,
    site: Res<Site>,
    images: Res<Assets<Image>>,
    dem: Res<DemHandle>,
    existing: Option<Res<DemHeights>>,
) {
    if existing.is_some() {
        return;
    }
    let Some(img) = images.get(&dem.0) else {
        return;
    };
    let Some(bytes) = img.data.as_ref() else {
        return;
    };
    let size = img.texture_descriptor.size;
    let (w, h) = (size.width as usize, size.height as usize);
    if bytes.len() < w * h * 2 {
        return;
    }
    // R16Uint, little-endian, one channel.
    let samples: Vec<u16> = bytes
        .chunks_exact(2)
        .take(w * h)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();

    let m = &site.0;
    commands.insert_resource(DemHeights {
        samples,
        width: w,
        height: h,
        world_min: m.world_min(),
        world_size: m.world_max() - m.world_min(),
        elev_min: m.dem.elev_min_m as f32,
        elev_range: (m.dem.elev_max_m - m.dem.elev_min_m) as f32,
    });
}

/// Build a CPU-displaced terrain patch over a tile's world bbox, with local `[0,1]` UVs
/// for the tile texture and smooth normals computed from the displaced geometry. Edge
/// vertices land on exact tile boundaries and sample the shared DEM, so adjacent
/// same-zoom patches meet without cracks.
pub fn build_patch_mesh(dem: &DemHeights, min: Vec2, max: Vec2, res: u32, vexag: f32) -> Mesh {
    let verts = ((res + 1) * (res + 1)) as usize;
    let mut positions = Vec::with_capacity(verts);
    let mut uvs = Vec::with_capacity(verts);
    for j in 0..=res {
        let fy = j as f32 / res as f32;
        // j: 0 → north (max_y, image top), res → south (min_y).
        let wy = max.y + (min.y - max.y) * fy;
        for i in 0..=res {
            let fx = i as f32 / res as f32;
            let wx = min.x + (max.x - min.x) * fx;
            let h = dem.height_at_world(Vec2::new(wx, wy)) * vexag;
            positions.push([wx, h, -wy]);
            uvs.push([fx, fy]);
        }
    }

    let stride = res + 1;
    let mut indices = Vec::with_capacity((res * res * 6) as usize);
    for j in 0..res {
        for i in 0..res {
            let a = j * stride + i;
            let b = a + 1;
            let c = a + stride;
            let d = c + 1;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices));
    mesh.compute_smooth_normals();
    mesh
}

/// Orbit the camera: left-drag rotates (yaw/pitch), wheel dollies. Yields the pointer
/// to egui. Skips while the scripted flythrough owns the camera.
fn orbit_camera(
    mut contexts: EguiContexts,
    flythrough: Res<crate::flythrough::Flythrough>,
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

/// Point the sun light per the shared [`HillshadeState`] angles when they change.
fn update_sun(state: Res<HillshadeState>, mut light: Query<&mut Transform, With<SunLight>>) {
    if !state.is_changed() {
        return;
    }
    if let Ok(mut t) = light.single_mut() {
        *t = Transform::from_translation(Vec3::ZERO)
            .looking_to(-sun_dir_3d(state.sun_azimuth_deg, state.sun_altitude_deg), Vec3::Y);
    }
}

/// Apply the live [`TerrainLook`] knobs (sun strength, ambient, tonemapping) when they
/// change. The imagery toggle is handled by the streamer (it rebuilds the patches).
fn apply_look(
    look: Res<TerrainLook>,
    mut sun: Query<&mut DirectionalLight, With<SunLight>>,
    ambient: Option<ResMut<GlobalAmbientLight>>,
    mut tonemapping: Query<&mut Tonemapping, With<MainCamera>>,
) {
    if !look.is_changed() {
        return;
    }
    if let Ok(mut light) = sun.single_mut() {
        light.illuminance = look.sun_lux;
    }
    if let Some(mut ambient) = ambient {
        ambient.brightness = look.ambient;
    }
    if let Ok(mut tm) = tonemapping.single_mut() {
        *tm = if look.tonemap {
            Tonemapping::TonyMcMapface
        } else {
            Tonemapping::None
        };
    }
}
