//! The shared **sun parameter**.
//!
//! Originally (It-0 plan 05) this drove a 2D `Material2d` hillshade quad. In Sprint 02
//! the terrain is real 3D geometry lit by a [`DirectionalLight`] (see `terrain3d.rs`),
//! so all that remains here is the one live sun direction — azimuth/altitude in degrees,
//! editable via the egui sliders and auto-swept with `G`. `terrain3d::update_sun` reads
//! it to aim the light; the values are seeded from the manifest's `sun` defaults.

use bevy::prelude::*;

use crate::Site;

/// Auto sun-sweep rate (degrees of azimuth per second) when sweeping is on.
const SUN_SWEEP_DEG_PER_SEC: f32 = 24.0;

/// Live sun state the egui sliders write and the directional light reads.
#[derive(Resource)]
pub struct HillshadeState {
    pub sun_azimuth_deg: f32,
    pub sun_altitude_deg: f32,
    /// When set, the azimuth auto-rotates (the `G` sun-sweep for the relief reveal).
    pub sweeping: bool,
}

pub struct HillshadePlugin;

impl Plugin for HillshadePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_sun_state)
            .add_systems(Update, sweep_sun);
    }
}

/// Seed the sun from the manifest's defaults at startup.
fn init_sun_state(mut commands: Commands, site: Res<Site>) {
    commands.insert_resource(HillshadeState {
        sun_azimuth_deg: site.0.sun.azimuth_deg,
        sun_altitude_deg: site.0.sun.altitude_deg,
        sweeping: false,
    });
}

/// `G` toggles the auto sun-sweep; while on, advance the azimuth each frame (wrapping
/// at 360°). Mutating the state drives `terrain3d::update_sun` via its `is_changed` path.
fn sweep_sun(time: Res<Time>, keys: Res<ButtonInput<KeyCode>>, state: Option<ResMut<HillshadeState>>) {
    let Some(mut state) = state else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyG) {
        state.sweeping = !state.sweeping;
    }
    if state.sweeping {
        let next = state.sun_azimuth_deg + SUN_SWEEP_DEG_PER_SEC * time.delta_secs();
        state.sun_azimuth_deg = next.rem_euclid(360.0);
    }
}
