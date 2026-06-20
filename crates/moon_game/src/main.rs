//! `moon_game` — the Bevy 0.18 runtime. Boots the app, loads the site manifest, and
//! presents the baked site as **real 3D lunar terrain** under a perspective camera
//! (Sprint 02): the LOLA DEM becomes displaced geometry, shaded per-fragment as dusty
//! regolith relief (no streamed imagery). An egui overlay shows live framing, FPS, and
//! the sun / vertical-exaggeration / surface controls.
//!
//! Run from anywhere — the asset root is pinned to the workspace `assets/`:
//! `cargo run -p moon_game` (or `MOON_SITE=shackleton cargo run -p moon_game`).

mod debug_ui;
mod flythrough;
mod hillshade;
mod terrain3d;

use bevy::asset::AssetPlugin;
use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::prelude::*;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass};
use moon_data::SiteManifest;

use debug_ui::{debug_ui, ShowUi};
use flythrough::{play_flythrough, Flythrough};
use hillshade::HillshadePlugin;
use terrain3d::Terrain3dPlugin;

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
        .insert_resource(Site(manifest))
        .init_resource::<Flythrough>()
        .init_resource::<ShowUi>()
        // egui auto-assigns its primary context to the "first found" camera, which is
        // order-dependent with our 2 cameras; disable it (PreStartup, before any camera
        // spawns) so `terrain3d`'s dedicated egui camera owns it explicitly.
        .add_systems(PreStartup, disable_egui_auto_context)
        // `play_flythrough` (when playing) and `orbit_camera` (in Terrain3dPlugin) drive
        // the camera.
        .add_systems(Update, play_flythrough)
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

fn disable_egui_auto_context(mut egui_settings: ResMut<EguiGlobalSettings>) {
    egui_settings.auto_create_primary_context = false;
}
