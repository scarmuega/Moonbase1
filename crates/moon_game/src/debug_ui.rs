//! egui debug overlay: camera pos / scale / selected LOD / FPS.
//!
//! ⚠️ This system MUST be scheduled on `EguiPrimaryContextPass` (see `main.rs`),
//! not `Update` — under bevy_egui 0.39's default multi-pass mode a UI system on
//! `Update` silently won't render. `EguiContexts::ctx_mut()` returns a `Result`,
//! hence the `-> Result` signature and `?`.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::camera::{select_lod, CameraController};
use crate::hillshade::HillshadeState;
use crate::streaming::LoadedTiles;
use crate::Site;

/// Whether the egui overlay is drawn. `F1` toggles it — off for clean hero frames,
/// on for the "from-scratch engine" stats reveal.
#[derive(Resource)]
pub struct ShowUi(pub bool);

impl Default for ShowUi {
    fn default() -> Self {
        Self(true)
    }
}

pub fn debug_ui(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    mut show: ResMut<ShowUi>,
    diagnostics: Res<DiagnosticsStore>,
    controller: Res<CameraController>,
    site: Res<Site>,
    loaded: Res<LoadedTiles>,
    mut hillshade: ResMut<HillshadeState>,
    camera: Query<(&Transform, &Projection), With<Camera2d>>,
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

    let (pos, scale) = match camera.single() {
        Ok((transform, Projection::Orthographic(ortho))) => {
            (transform.translation.truncate(), ortho.scale)
        }
        _ => (Vec2::ZERO, 0.0),
    };
    let lod = select_lod(&site.0, scale);

    egui::Window::new("camera").show(ctx, |ui| {
        ui.label(format!("FPS: {fps:.0}"));
        ui.label(format!("pos: ({:.0}, {:.0}) m", pos.x, pos.y));
        ui.label(format!("scale: {scale:.2} m/px"));
        ui.label(format!("target: {:.2} m/px", controller.target_scale));
        ui.label(format!("LOD zoom: {lod}"));
        ui.label(format!("tiles: {}", loaded.map.len()));

        ui.separator();
        ui.checkbox(&mut hillshade.visible, "hillshade (H)");
        ui.checkbox(&mut hillshade.sweeping, "sun sweep (G)");
        let mut ramp = hillshade.mode == 1;
        if ui.checkbox(&mut ramp, "height ramp debug (J)").changed() {
            hillshade.mode = u32::from(ramp);
        }
        ui.add(
            egui::Slider::new(&mut hillshade.sun_azimuth_deg, 0.0..=360.0).text("sun azimuth°"),
        );
        ui.add(
            egui::Slider::new(&mut hillshade.sun_altitude_deg, 0.0..=90.0).text("sun altitude°"),
        );
        ui.separator();
        ui.label("F flythrough · F1 hide UI · drag pan · wheel zoom");
    });

    Ok(())
}
