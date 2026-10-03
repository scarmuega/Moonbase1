//! Presentation light for authored PBR, sharing the custom terrain shader's sun.
use crate::hillshade::HillshadeState;
use bevy::prelude::*;

#[derive(Component)]
struct ModelSun;

fn spawn_sun(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 6_000.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::default(),
        ModelSun,
    ));
    // Stylized regolith bounce, not a physical lunar atmosphere. No baked highlights.
    commands.insert_resource(GlobalAmbientLight {
        brightness: 150.0,
        ..default()
    });
}

fn sync_sun(state: Res<HillshadeState>, mut sun: Query<&mut Transform, With<ModelSun>>) {
    let az = state.sun_azimuth_deg.to_radians();
    let alt = state.sun_altitude_deg.to_radians();
    // Terrain WGSL uses east/north/up; Bevy uses east/up/south.
    let toward_sun = Vec3::new(alt.cos() * az.sin(), alt.sin(), -alt.cos() * az.cos());
    for mut transform in &mut sun {
        *transform = Transform::default().looking_to(-toward_sun, Vec3::Y);
    }
}

pub struct ModelLightingPlugin;
impl Plugin for ModelLightingPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_sun)
            .add_systems(Update, sync_sun);
    }
}
