//! egui debug overlay: FPS, camera framing, sun controls, vertical exaggeration.
//!
//! ⚠️ This system MUST be scheduled on `EguiPrimaryContextPass` (see `main.rs`), not
//! `Update` — under bevy_egui 0.39's multi-pass mode a UI system on `Update` silently
//! won't render. `EguiContexts::ctx_mut()` returns a `Result`, hence `-> Result`/`?`.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::hillshade::HillshadeState;
use crate::streaming::LoadedTiles;
use crate::terrain3d::{CameraRig, TerrainLook};

/// Whether the egui overlay is drawn. `F1` toggles it — off for clean hero frames.
#[derive(Resource)]
pub struct ShowUi(pub bool);

impl Default for ShowUi {
    fn default() -> Self {
        Self(true)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn debug_ui(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    mut show: ResMut<ShowUi>,
    diagnostics: Res<DiagnosticsStore>,
    loaded: Res<LoadedTiles>,
    mut sun: ResMut<HillshadeState>,
    mut rig: ResMut<CameraRig>,
    mut look: ResMut<TerrainLook>,
) -> Result {
    if keys.just_pressed(KeyCode::F1) {
        show.0 = !show.0;
    }
    if !show.0 {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);

    egui::Window::new("mooncraft").show(ctx, |ui| {
        ui.label(format!("FPS: {fps:.0}"));
        ui.label(format!("pitch: {:.0}°", rig.pitch.to_degrees()));
        ui.label(format!("yaw: {:.0}°", rig.yaw.to_degrees().rem_euclid(360.0)));
        ui.label(format!("distance: {:.0} m", rig.distance));
        ui.label(format!("patches: {}", loaded.map.len()));

        ui.separator();
        ui.add(egui::Slider::new(&mut rig.vexag, 1.0..=8.0).text("vertical exag."));
        ui.checkbox(&mut sun.sweeping, "sun sweep (G)");
        ui.add(egui::Slider::new(&mut sun.sun_azimuth_deg, 0.0..=360.0).text("sun azimuth°"));
        ui.add(egui::Slider::new(&mut sun.sun_altitude_deg, 0.0..=90.0).text("sun altitude°"));

        ui.separator();
        ui.label("look");
        ui.checkbox(&mut look.imagery, "drape imagery (off = relief)");
        ui.checkbox(&mut look.tonemap, "filmic tonemapping");
        ui.add(egui::Slider::new(&mut look.sun_lux, 5_000.0..=150_000.0).text("sun lux"));
        ui.add(egui::Slider::new(&mut look.ambient, 0.0..=10_000.0).text("ambient"));

        ui.separator();
        ui.label("drag orbit · wheel zoom · F flythrough · G sun sweep · F1 hide UI");
    });

    Ok(())
}
