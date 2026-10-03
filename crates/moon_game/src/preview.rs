//! Opt-in, repeatable native game capture; absent during ordinary startup.
//!
//! MOON_SITE=shackleton MOON_PREVIEW=hab|baseline cargo run -p moon_game --release
//! Writes to ignored dev-clips/hab-v2/. Both runs have identical view and lighting.
use crate::{
    Site,
    build::{ModuleCatalog, ModuleId, footprint_buildable, spawn_structure},
    debug_ui::ShowUi,
    ground::{TERRAIN_VEXAG, TerrainField},
    terrain3d::{CameraRig, MainCamera},
};
use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk},
    scene::SceneInstance,
    window::{PresentMode, WindowResolution},
    winit::WinitSettings,
};
use std::path::PathBuf;

const WARMUP: usize = 300;
const SAMPLES: usize = 600;

#[derive(Resource)]
struct Preview {
    label: String,
    root: Option<Entity>,
    framed: bool,
    frames: usize,
    samples: Vec<f64>,
    captured: bool,
}

pub fn window() -> Window {
    let mut window = Window::default();
    if std::env::var_os("MOON_PREVIEW").is_some() {
        window.resolution = WindowResolution::new(1280, 960).with_scale_factor_override(1.0);
        window.present_mode = PresentMode::AutoNoVsync;
    }
    window
}

pub struct PreviewPlugin;
impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        if let Ok(label) = std::env::var("MOON_PREVIEW") {
            assert!(
                label == "hab" || label == "baseline",
                "MOON_PREVIEW must be hab or baseline"
            );
            assert_eq!(
                std::env::var("MOON_SITE").as_deref(),
                Ok("shackleton"),
                "preview requires Shackleton"
            );
            app.insert_resource(WinitSettings::continuous())
                .insert_resource(Preview {
                    label,
                    root: None,
                    framed: false,
                    frames: 0,
                    samples: Vec::new(),
                    captured: false,
                })
                .add_systems(Update, capture.after(crate::terrain3d::orbit_camera));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn capture(
    mut commands: Commands,
    mut preview: ResMut<Preview>,
    field: Res<TerrainField>,
    site: Res<Site>,
    catalog: Res<ModuleCatalog>,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rig: ResMut<CameraRig>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
    mut show_ui: ResMut<ShowUi>,
    instances: Query<&SceneInstance>,
    spawner: Res<SceneSpawner>,
    time: Res<Time<Real>>,
) {
    if !preview.framed {
        let def = catalog
            .get(&ModuleId("hab".into()))
            .expect("hab catalogue entry");
        // Deterministic lowest-slope footprint away from the ordinary lander.
        let mut best = None;
        let mut best_slope = f32::INFINITY;
        for y in -150..=150 {
            for x in -150..=150 {
                let xy = Vec2::new(x as f32 * 50.0, y as f32 * 50.0);
                let slope = field.slope_deg_at(&site.0, xy);
                if xy.length() > 100.0
                    && slope < best_slope
                    && footprint_buildable(&field, &site.0, xy, def)
                {
                    best = Some(xy);
                    best_slope = slope;
                }
            }
        }
        let xy = best.expect("no buildable habitat footprint in preview scan");
        let ground = field.height_at(&site.0, xy) * TERRAIN_VEXAG;
        if preview.label == "hab" {
            preview.root = Some(spawn_structure(
                &mut commands,
                &server,
                &mut meshes,
                &mut materials,
                &field,
                &site.0,
                xy,
                def,
            ));
        }
        rig.target = Vec3::new(xy.x, ground + 7.0, -xy.y);
        rig.yaw = 140_f32.to_radians(); // front (-Z), three-quarter view with side lighting
        rig.pitch = 15_f32.to_radians();
        rig.distance = 36.0;
        *camera.single_mut().expect("main camera") = rig.transform();
        show_ui.0 = false;
        info!(
            ?xy,
            best_slope, ground, "hab preview: grounded buildable footprint, 1280x960, no vsync"
        );
        preview.framed = true;
        return;
    }
    // Warm-up begins only once all scene entities exist, plus 300 rendered frames for
    // texture uploads/pipelines. Baseline has no scene to await.
    if let Some(root) = preview.root {
        let Ok(instance) = instances.get(root) else {
            return;
        };
        if !spawner.instance_is_ready(**instance) {
            return;
        }
    }
    if preview.captured {
        return;
    }
    preview.frames += 1;
    if preview.frames > WARMUP {
        preview.samples.push(time.delta_secs_f64() * 1000.0);
    }
    if preview.samples.len() < SAMPLES {
        return;
    }
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../dev-clips/hab-v2");
    std::fs::create_dir_all(&directory).expect("create ignored capture directory");
    let mut sorted = preview.samples.clone();
    sorted.sort_by(f64::total_cmp);
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let evidence = format!(
        "mode={}\nresolution=1280x960\npresent_mode=AutoNoVsync\nwarmup_frames={WARMUP}\nsamples={SAMPLES}\nmean_ms={mean:.4}\nmean_fps={:.2}\nmedian_ms={:.4}\np95_ms={:.4}\np99_ms={:.4}\nmax_ms={:.4}\n",
        preview.label,
        1000.0 / mean,
        sorted[SAMPLES / 2],
        sorted[SAMPLES * 95 / 100],
        sorted[SAMPLES * 99 / 100],
        sorted[SAMPLES - 1]
    );
    std::fs::write(
        directory.join(format!("{}-timings.txt", preview.label)),
        &evidence,
    )
    .expect("write timings");
    info!("{evidence}");
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(
            directory.join(format!("{}.png", preview.label)),
        ))
        .observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
    preview.captured = true;
}
