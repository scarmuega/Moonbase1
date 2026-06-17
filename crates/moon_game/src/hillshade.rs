//! Plan 05 — runtime elevation hillshade + toggle.
//!
//! A [`Material2d`] renders the baked DEM as shaded relief in a fragment shader
//! (`assets/shaders/hillshade.wgsl`), so the sun direction is a live, draggable
//! parameter rather than baked in. The shaded quad covers the whole site and sits
//! above the imagery tiles (which render at `z = zoom`, 0..3); toggling its
//! `Visibility` swaps the view between imagery and relief.
//!
//! All geometry comes from the [`SiteManifest`] — the world→DEM-UV mapping in the
//! shader mirrors [`moon_data::world_to_dem_uv`] exactly.

use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d, Material2dPlugin};

use crate::Site;

/// Z of the hillshade quad — above every imagery tile (those sit at `z = zoom`).
const HILLSHADE_Z: f32 = 10.0;

/// Auto sun-sweep rate (degrees of azimuth per second) when sweeping is on.
const SUN_SWEEP_DEG_PER_SEC: f32 = 24.0;

/// Uniform block handed to the shader. Packed as a single `@group(2) @binding(0)`
/// uniform; field order/types must match the `HillshadeParams` struct in the WGSL.
#[derive(Clone, ShaderType)]
pub struct HillshadeParams {
    /// DEM world bbox as min corner + size (= manifest `world_bbox`).
    pub dem_world_min: Vec2,
    pub dem_world_size: Vec2,
    /// Elevation decode bounds (m): `elev = elev_min + sample*(elev_max-elev_min)`.
    pub elev_min: f32,
    pub elev_max: f32,
    /// Sun direction in radians (azimuth = compass bearing, altitude above horizon).
    pub sun_azimuth: f32,
    pub sun_altitude: f32,
    /// Reserved for future shading modes (0 = standard hillshade).
    pub mode: u32,
}

/// The hillshade material: one uniform block + the DEM heightmap texture.
///
/// The DEM is a 16-bit grayscale PNG, which Bevy decodes to the integer format
/// `R16Uint` — hence `sample_type = "u_int"` and no sampler (the shader uses
/// `textureLoad`). R16Uint is universally supported; the filterable `R16Unorm`
/// alternative needs a wgpu feature Bevy doesn't enable by default.
#[derive(Asset, AsBindGroup, TypePath, Clone)]
pub struct HillshadeMaterial {
    #[uniform(0)]
    pub params: HillshadeParams,
    #[texture(1, sample_type = "u_int")]
    pub dem: Handle<Image>,
}

impl Material2d for HillshadeMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/hillshade.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        // Opaque grayscale: when visible it fully covers the imagery beneath.
        AlphaMode2d::Opaque
    }
}

/// Live hillshade UI state: layer on/off and the sun angles (degrees) the egui
/// sliders write. Seeded from the manifest's `sun` defaults at startup.
#[derive(Resource)]
pub struct HillshadeState {
    pub visible: bool,
    pub sun_azimuth_deg: f32,
    pub sun_altitude_deg: f32,
    /// When set, the azimuth auto-rotates (the `G` sun-sweep for the relief reveal).
    pub sweeping: bool,
}

/// Handle to the spawned material, so the sync system can rewrite its uniforms.
#[derive(Resource)]
struct HillshadeHandle(Handle<HillshadeMaterial>);

/// Marks the full-site hillshade quad.
#[derive(Component)]
struct HillshadeLayer;

pub struct HillshadePlugin;

impl Plugin for HillshadePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(Material2dPlugin::<HillshadeMaterial>::default())
            .add_systems(Startup, setup_hillshade)
            // Chained so the sweep's azimuth change is reflected in the uniforms the
            // same frame: toggle/sweep mutate state, then sync reads `is_changed`.
            .add_systems(
                Update,
                (toggle_hillshade, sweep_sun, sync_visibility, sync_uniforms).chain(),
            );
    }
}

/// Spawn the hidden hillshade quad covering the world bbox and seed UI state.
fn setup_hillshade(
    mut commands: Commands,
    site: Res<Site>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<HillshadeMaterial>>,
) {
    let m = &site.0;
    let min = m.world_min();
    let max = m.world_max();
    let size = max - min;
    let center = (min + max) * 0.5;

    let state = HillshadeState {
        visible: false,
        sun_azimuth_deg: m.sun.azimuth_deg,
        sun_altitude_deg: m.sun.altitude_deg,
        sweeping: false,
    };

    let material = materials.add(HillshadeMaterial {
        params: HillshadeParams {
            dem_world_min: min,
            dem_world_size: size,
            elev_min: m.dem.elev_min_m as f32,
            elev_max: m.dem.elev_max_m as f32,
            sun_azimuth: state.sun_azimuth_deg.to_radians(),
            sun_altitude: state.sun_altitude_deg.to_radians(),
            mode: 0,
        },
        dem: asset_server.load(m.dem.path.clone()),
    });

    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(size.x, size.y))),
        MeshMaterial2d(material.clone()),
        Transform::from_translation(center.extend(HILLSHADE_Z)),
        Visibility::Hidden,
        HillshadeLayer,
    ));

    commands.insert_resource(HillshadeHandle(material));
    commands.insert_resource(state);
}

/// `H` flips the hillshade layer on/off (the egui checkbox writes the same flag).
fn toggle_hillshade(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<HillshadeState>) {
    if keys.just_pressed(KeyCode::KeyH) {
        state.visible = !state.visible;
    }
}

/// `G` toggles the auto sun-sweep; while on, advance the azimuth each frame
/// (wrapping at 360°). Mutating the state drives `sync_uniforms` through its
/// existing `is_changed` path — no extra plumbing.
fn sweep_sun(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<HillshadeState>,
) {
    if keys.just_pressed(KeyCode::KeyG) {
        state.sweeping = !state.sweeping;
    }
    if state.sweeping {
        let next = state.sun_azimuth_deg + SUN_SWEEP_DEG_PER_SEC * time.delta_secs();
        state.sun_azimuth_deg = next.rem_euclid(360.0);
    }
}

/// Mirror `HillshadeState.visible` onto the quad's `Visibility` when it changes.
fn sync_visibility(
    state: Res<HillshadeState>,
    mut layer: Query<&mut Visibility, With<HillshadeLayer>>,
) {
    if !state.is_changed() {
        return;
    }
    let target = if state.visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    for mut vis in &mut layer {
        *vis = target;
    }
}

/// Push the live sun angles into the material uniforms when the UI state changes.
fn sync_uniforms(
    state: Res<HillshadeState>,
    handle: Res<HillshadeHandle>,
    mut materials: ResMut<Assets<HillshadeMaterial>>,
) {
    if !state.is_changed() {
        return;
    }
    if let Some(mat) = materials.get_mut(&handle.0) {
        mat.params.sun_azimuth = state.sun_azimuth_deg.to_radians();
        mat.params.sun_altitude = state.sun_altitude_deg.to_radians();
    }
}
