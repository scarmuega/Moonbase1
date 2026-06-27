//! Sprint 03 · Plan 02 — buildables: a data-driven module catalog, a build menu, a ghost
//! preview with slope-based buildability, placement, and the static lander.
//!
//! Everything here stands on Plan 01's [`crate::ground`] foundation: the cursor is resolved to
//! real-relief world XY by `cursor_ground` ([`CursorGround`]); the ghost and placed modules are
//! grounded with the same height-field formula via [`GroundAnchor`] / `ground_on_spawn`; and
//! buildability uses [`TerrainField::slope_deg_at`] in **true (unscaled) metres** so it reflects
//! the real Moon regardless of [`TERRAIN_VEXAG`].
//!
//! A module is defined entirely by `assets/data/modules.ron` — adding one is data-only.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use moon_data::SiteManifest;
use serde::Deserialize;

use crate::ground::{
    CursorGround, GroundAnchor, GroundSet, ProjectionMode, ProjectionToggleRequest, TerrainField, TERRAIN_VEXAG,
};
use crate::Site;

/// A catalog key. Serde-transparent `String` newtype mirroring `moon_data::SiteId`, so module
/// ids read as bare strings in RON and adding a module is data-only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct ModuleId(pub String);

/// Primitive shape family. `Panel` is a thin cuboid for now — its own variant so It-2 can give
/// it real (tilted, framed) geometry without touching the data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Shape {
    Cuboid,
    Panel,
    Cylinder,
}

/// One catalog entry, loaded from `modules.ron`. `color` / `size_m` are `[f32; 3]` (not glam
/// `Vec3`): glam's serde feature is not in the dependency tree, so `Vec3` is not `Deserialize`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModuleDef {
    pub id: ModuleId,
    pub display_name: String,
    /// Shown in Plan 03's details panel.
    #[allow(dead_code)] // consumed by Plan 03 details panel
    pub blurb: String,
    /// Buildability test radius (and Plan 03 selection bounds).
    pub footprint_radius_m: f32,
    /// Steepest ground (real metres) the module may sit on.
    pub max_slope_deg: f32,
    /// Base albedo (Plan 03 tints a copy for selection).
    pub color: [f32; 3],
    pub shape: Shape,
    /// Bounding dims `(x, y=up, z)`; half-height = `size_m[1] * 0.5`.
    pub size_m: [f32; 3],
    #[serde(default)]
    #[allow(dead_code)] // consumed by Plan 03 details panel
    pub power_kw: Option<f32>,
    #[serde(default)]
    #[allow(dead_code)] // consumed by Plan 03 details panel
    pub power_storage_kwh: Option<f32>,
    #[serde(default)]
    #[allow(dead_code)] // consumed by Plan 03 details panel
    pub crew_capacity: Option<u32>,
}

impl ModuleDef {
    fn size(&self) -> Vec3 {
        Vec3::from_array(self.size_m)
    }
    fn half_height(&self) -> f32 {
        self.size_m[1] * 0.5
    }
    fn base_color(&self) -> Color {
        Color::srgb(self.color[0], self.color[1], self.color[2])
    }
}

/// The loaded catalog. A `Vec` preserves file order (so the build menu is deterministic — a
/// `HashMap` alone would reshuffle the buttons between runs); the index gives O(1) lookup by id.
#[derive(Resource)]
pub struct ModuleCatalog {
    defs: Vec<ModuleDef>,
    index: HashMap<ModuleId, usize>,
}

impl ModuleCatalog {
    fn new(defs: Vec<ModuleDef>) -> Self {
        let index = defs.iter().enumerate().map(|(i, d)| (d.id.clone(), i)).collect();
        Self { defs, index }
    }
    pub fn get(&self, id: &ModuleId) -> Option<&ModuleDef> {
        self.index.get(id).map(|&i| &self.defs[i])
    }
    fn iter(&self) -> impl Iterator<Item = &ModuleDef> {
        self.defs.iter()
    }
}

/// Read the catalog at startup (direct `std::fs` read + panic-on-bad-data, like
/// `main.rs::load_manifest` — the game is meaningless without its buildables).
fn load_module_catalog(mut commands: Commands) {
    let path = format!("{}/data/modules.ron", crate::ASSET_ROOT);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
    let defs: Vec<ModuleDef> = ron::from_str(&text).unwrap_or_else(|e| panic!("parsing {path} as Vec<ModuleDef>: {e}"));
    assert!(!defs.is_empty(), "{path}: catalog is empty");
    commands.insert_resource(ModuleCatalog::new(defs));
}

/// Placement state. `None` = inspect/select (Plan 03 owns that); `Some(id)` = the player is
/// placing `id` (chain-build until `Esc` / right-click).
#[derive(Resource, Default)]
pub struct BuildMode {
    pub placing: Option<ModuleId>,
}

/// Build menu (Plan 03): a toggle per catalog entry that reflects/sets the active placement, a
/// Cancel button, and a projection toggle mirroring `P`. Runs on `EguiPrimaryContextPass`
/// (multi-pass mode), like `debug_ui`.
fn build_menu(
    mut contexts: EguiContexts,
    catalog: Res<ModuleCatalog>,
    mut build: ResMut<BuildMode>,
    mode: Res<ProjectionMode>,
    mut toggle_req: ResMut<ProjectionToggleRequest>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("build").show(ctx, |ui| {
        for def in catalog.iter() {
            let active = build.placing.as_ref() == Some(&def.id);
            if ui.selectable_label(active, &def.display_name).clicked() {
                // Click the active entry to leave placing; otherwise switch to it.
                build.placing = if active { None } else { Some(def.id.clone()) };
            }
        }
        if ui.button("Cancel (Esc)").clicked() {
            build.placing = None;
        }
        ui.separator();
        let label = match *mode {
            ProjectionMode::Perspective => "Isometric view",
            ProjectionMode::Iso => "Perspective view",
        };
        if ui.button(label).clicked() {
            toggle_req.0 = true;
        }
        ui.separator();
        ui.label("left-click ground to place · Esc / right-click to cancel");
    });
    Ok(())
}

/// The single follow-the-cursor preview entity. `buildable` is the slope verdict `place_module`
/// reads; `shown_id` tracks the mesh currently installed so we rebuild `Mesh3d` only when the
/// player switches modules; `material` is the ghost's own translucent material, retinted in place.
#[derive(Component)]
pub struct Ghost {
    pub buildable: bool,
    pub shown_id: Option<ModuleId>,
    pub material: Handle<StandardMaterial>,
}

/// Spawn the ghost hidden with a placeholder unit mesh and its own translucent, unlit material
/// (there are no lights in the scene — a lit material renders black). Needs no other resource.
fn spawn_ghost(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.2, 0.9, 0.2, 0.35),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    commands.spawn((
        Ghost { buildable: false, shown_id: None, material: material.clone() },
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(material),
        Transform::default(),
        Visibility::Hidden,
        Pickable::IGNORE, // the preview is never a selection target (Plan 03)
    ));
}

/// Build the primitive mesh for a shape at its real dimensions. `Panel` is a thin cuboid;
/// `Cylinder` takes `size.x` as diameter (radius = `size.x * 0.5`) and `size.y` as height.
fn shape_mesh(meshes: &mut Assets<Mesh>, shape: Shape, size: Vec3) -> Handle<Mesh> {
    match shape {
        Shape::Cuboid | Shape::Panel => meshes.add(Cuboid::new(size.x, size.y, size.z)),
        Shape::Cylinder => meshes.add(Cylinder::new(size.x * 0.5, size.y)),
    }
}

/// Centre + 8 compass points at the footprint radius; buildable iff every sample's ground slope
/// (true-metre degrees, independent of [`TERRAIN_VEXAG`]) is within the module's max.
fn footprint_buildable(field: &TerrainField, m: &SiteManifest, xy: Vec2, def: &ModuleDef) -> bool {
    let max = def.max_slope_deg;
    if field.slope_deg_at(m, xy) > max {
        return false;
    }
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const DIRS: [Vec2; 8] = [
        Vec2::new(1.0, 0.0),
        Vec2::new(-1.0, 0.0),
        Vec2::new(0.0, 1.0),
        Vec2::new(0.0, -1.0),
        Vec2::new(D, D),
        Vec2::new(-D, D),
        Vec2::new(D, -D),
        Vec2::new(-D, -D),
    ];
    DIRS.iter().all(|dir| field.slope_deg_at(m, xy + *dir * def.footprint_radius_m) <= max)
}

/// Move/retint the ghost each frame while placing (runs after `cursor_ground`). Hidden unless a
/// module is selected and the cursor is on real terrain.
#[allow(clippy::too_many_arguments)] // a Bevy system's params aren't a refactor smell
fn update_ghost(
    build: Res<BuildMode>,
    catalog: Res<ModuleCatalog>,
    cursor: Res<CursorGround>,
    field: Res<TerrainField>,
    site: Res<Site>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&mut Ghost, &mut Transform, &mut Mesh3d, &mut Visibility)>,
) {
    let Ok((mut ghost, mut transform, mut mesh, mut vis)) = q.single_mut() else { return };

    // Show only while placing a known module with the cursor on terrain.
    let (Some(id), Some(xy)) = (build.placing.as_ref(), cursor.0) else {
        *vis = Visibility::Hidden;
        return;
    };
    let Some(def) = catalog.get(id) else {
        *vis = Visibility::Hidden;
        return;
    };
    *vis = Visibility::Visible;

    // Swap the mesh only when the module changes — never `meshes.add` per frame.
    if ghost.shown_id.as_ref() != Some(id) {
        *mesh = Mesh3d(shape_mesh(&mut meshes, def.shape, def.size()));
        ghost.shown_id = Some(id.clone());
    }

    // Ground with the Plan-01 formula.
    let m = &site.0;
    let y = field.height_at(m, xy) * TERRAIN_VEXAG + def.half_height();
    transform.translation = Vec3::new(xy.x, y, -xy.y);

    // Footprint slope verdict → green/red. Retint by mutating the ghost's own material asset
    // (its dedicated handle, so the change can't bleed into placed structures).
    ghost.buildable = footprint_buildable(&field, m, xy, def);
    if let Some(mat) = materials.get_mut(&ghost.material) {
        mat.base_color = if ghost.buildable {
            Color::srgba(0.2, 0.9, 0.2, 0.35)
        } else {
            Color::srgba(0.9, 0.2, 0.2, 0.35)
        };
    }
}

/// A committed, placed structure (selectable in Plan 03).
#[derive(Component)]
pub struct Structure;

/// Which catalog entry a structure is (read by Plan 03's details panel).
#[derive(Component, Clone)]
#[allow(dead_code)] // consumed by Plan 03 selection / details
pub struct ModuleKind(pub ModuleId);

/// Planar buildability / selection radius, carried for Plan 03 selection bounds + overlap tests.
#[derive(Component, Copy, Clone)]
#[allow(dead_code)] // consumed by Plan 03 selection
pub struct Footprint {
    pub radius_m: f32,
}

/// Commit a placement on left-click (runs after `update_ghost`). Chain-builds: `placing` stays
/// set so the player can drop several. `CursorGround` being `Some` already implies the pointer is
/// not over egui (`cursor_ground` yields), so a click on a menu button can't also place.
#[allow(clippy::too_many_arguments)] // a Bevy system's params aren't a refactor smell
fn place_module(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    build: Res<BuildMode>,
    catalog: Res<ModuleCatalog>,
    cursor: Res<CursorGround>,
    field: Res<TerrainField>,
    site: Res<Site>,
    ghost_q: Query<&Ghost>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let (Some(id), Some(xy)) = (build.placing.as_ref(), cursor.0) else { return };
    let Ok(ghost) = ghost_q.single() else { return };
    if !ghost.buildable {
        return;
    }
    let Some(def) = catalog.get(id) else { return };

    // Set Transform.y now (TerrainField is in scope) so the module is grounded on its first
    // rendered frame. `ground_on_spawn` re-derives the identical value next frame (idempotent).
    let m = &site.0;
    let y = field.height_at(m, xy) * TERRAIN_VEXAG + def.half_height();

    commands.spawn((
        Structure,
        ModuleKind(id.clone()),
        GroundAnchor { xy, half_height: def.half_height() },
        Footprint { radius_m: def.footprint_radius_m },
        Mesh3d(shape_mesh(&mut meshes, def.shape, def.size())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: def.base_color(),
            unlit: true,
            ..default()
        })),
        Transform::from_translation(Vec3::new(xy.x, y, -xy.y)),
        Pickable::default(), // click-to-select (Plan 03)
    ));

    tracing::info!(module = %id.0, x = xy.x, y = xy.y, "placed module");
}

/// Leave placing mode on `Esc` or right-click. `update_ghost` hides the ghost next frame, so it
/// stays the single writer of the ghost's `Visibility`.
fn cancel_build(keys: Res<ButtonInput<KeyCode>>, mouse: Res<ButtonInput<MouseButton>>, mut build: ResMut<BuildMode>) {
    if keys.just_pressed(KeyCode::Escape) || mouse.just_pressed(MouseButton::Right) {
        build.placing = None;
    }
}

/// The single static lander marking the base origin (selectable in Plan 03).
#[derive(Component)]
pub struct Lander;

/// Spawn the lander at startup on the flattest spot near origin (so it doesn't perch on a crater
/// wall). Reads `TerrainField`, so it must run after `GroundSet::LoadField`. `GroundAnchor`
/// grounds it like everything else; no descent or animation.
fn spawn_lander(
    mut commands: Commands,
    field: Res<TerrainField>,
    site: Res<Site>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let m = &site.0;
    // Cheap lowest-slope scan over a tight grid near origin (the base sits at the view centre at
    // the default framing, so keep the lander there). On coarse DEMs the samples share a texel and
    // it resolves to ≈origin — fine; the point is only to avoid the single worst cell.
    let step = 30.0;
    let mut best_xy = Vec2::ZERO;
    let mut best_slope = f32::INFINITY;
    for iy in -2..=2 {
        for ix in -2..=2 {
            let p = Vec2::new(ix as f32 * step, iy as f32 * step);
            let s = field.slope_deg_at(m, p);
            if s < best_slope {
                best_slope = s;
                best_xy = p;
            }
        }
    }

    commands.spawn((
        Lander,
        GroundAnchor { xy: best_xy, half_height: 10.0 }, // Cylinder height 20 → half-height 10
        Mesh3d(meshes.add(Cylinder::new(4.0, 20.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.8, 0.82),
            unlit: true,
            ..default()
        })),
        Transform::default(),
        Pickable::default(), // click-to-select (Plan 03)
    ));
}

pub struct BuildPlugin;

impl Plugin for BuildPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildMode>()
            // `spawn_lander` reads `TerrainField` (inserted by GroundPlugin's `load_terrain_field`).
            .add_systems(Startup, (load_module_catalog, spawn_ghost, spawn_lander.after(GroundSet::LoadField)))
            .add_systems(
                Update,
                (update_ghost.after(GroundSet::Cursor), place_module.after(update_ghost), cancel_build),
            )
            .add_systems(EguiPrimaryContextPass, build_menu);
    }
}
