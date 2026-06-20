//! `moon_game` — the Bevy 0.18 runtime. Boots the app, loads the site manifest,
//! spawns a 2D camera with grab-pan / zoom-to-cursor (plan 03), and streams the
//! baked tile pyramid by viewport at the matching LOD (plan 04). An egui overlay
//! shows live camera state and the loaded-tile count.
//!
//! Run from the workspace root so Bevy's default `assets/` path resolves:
//! `cargo run -p moon_game`.

mod camera;
mod debug_ui;
mod flythrough;
mod hillshade;
mod streaming;
mod terrain3d;

use bevy::asset::AssetPlugin;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass};
use moon_data::SiteManifest;

use camera::{camera_control, select_lod, CameraController};
use debug_ui::{debug_ui, ShowUi};
use flythrough::{play_flythrough, Flythrough};
use hillshade::HillshadePlugin;
use streaming::{drain_spawn_queue, stream_tiles, LoadedTiles, Streamer};
use terrain3d::{in_map2d, Terrain3dPlugin};

// Bevy resolves assets via BEVY_ASSET_ROOT → CARGO_MANIFEST_DIR → exe-dir, never
// the cwd. In this workspace `cargo run -p moon_game` sets CARGO_MANIFEST_DIR to
// `crates/moon_game`, so the workspace-root `assets/` wouldn't be found. Pin the
// asset root to an absolute path built from the compile-time crate dir, so launch
// method / cwd no longer matter.
/// Absolute path to the workspace `assets/` directory.
const ASSET_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");
/// Site baked by default. Override at launch with `MOON_SITE=<id>` to fly a
/// different baked site (e.g. `MOON_SITE=shackleton` for the 5 m/px close-up).
const DEFAULT_SITE: &str = "southpole";

/// Bevy-side wrapper for the (deliberately Bevy-free) [`SiteManifest`] so it can
/// live as a `Resource`. `moon_data` stays engine-agnostic; the game adapts it.
#[derive(Resource, Debug)]
pub struct Site(pub SiteManifest);

fn main() {
    let site = std::env::var("MOON_SITE").unwrap_or_else(|_| DEFAULT_SITE.to_string());
    let manifest_path = format!("{ASSET_ROOT}/sites/{site}.ron");
    let manifest = load_manifest(&manifest_path);
    let controller = CameraController::from_manifest(&manifest);
    let initial_zoom = select_lod(&manifest, controller.target_scale);

    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: format!("Mooncraft — {site}"),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: ASSET_ROOT.into(),
                    ..default()
                }),
        )
        .add_plugins(EguiPlugin::default())
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(HillshadePlugin)
        .add_plugins(Terrain3dPlugin)
        .insert_resource(controller)
        .insert_resource(Site(manifest))
        .init_resource::<LoadedTiles>()
        .init_resource::<Flythrough>()
        .init_resource::<ShowUi>()
        .insert_resource(Streamer::new(initial_zoom))
        .add_systems(Startup, setup)
        // `camera_control` runs first; `play_flythrough` overrides it while a scripted
        // path is playing; streaming then reads the resulting camera transform. The
        // whole 2D chain pauses while the 3D relief view is active (Tier 1), so the 2D
        // camera stays frozen and unchanged when toggled back.
        .add_systems(
            Update,
            (camera_control, play_flythrough, stream_tiles, drain_spawn_queue)
                .chain()
                .run_if(in_map2d),
        )
        // ⚠️ egui UI must run on EguiPrimaryContextPass, not Update (multi-pass mode).
        .add_systems(EguiPrimaryContextPass, debug_ui)
        .run();
}

/// Read + parse the manifest at startup (same path the pipeline's `check` uses).
/// Panicking here is correct: the game is meaningless without a valid manifest.
fn load_manifest(path: &str) -> SiteManifest {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("reading {path} (run from the workspace root): {e}"));
    ron::from_str(&text).unwrap_or_else(|e| panic!("parsing {path} as a SiteManifest: {e}"))
}

/// Spawn the 2D camera. Tiles are owned by the streaming systems (plan 04),
/// which load and unload them by viewport on the first throttle pass.
///
/// With multiple cameras (2D scene, 3D scene, and the dedicated egui camera), egui's
/// auto-creation of a primary context on the "first found" camera is order-dependent,
/// so we disable it; `terrain3d` spawns one always-active camera that owns the primary
/// egui context explicitly.
fn setup(mut commands: Commands, mut egui_settings: ResMut<EguiGlobalSettings>) {
    egui_settings.auto_create_primary_context = false;
    commands.spawn(Camera2d);
}
