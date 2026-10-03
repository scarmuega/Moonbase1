//! Sprint 03 · Plan 03 — selection & details: click a placed entity to select it, give it a
//! static highlight, and show a live read-only Details panel. This closes the sprint's
//! render → position → **select → inspect** loop.
//!
//! Picking uses Bevy's [`MeshPickingPlugin`] (default-on in 0.18), which raycasts real meshes —
//! perfect for the lander/modules (genuine geometry). The terrain mesh is **flat on the CPU**
//! (displaced only in the vertex shader), so its `Pickable::IGNORE` (set in `terrain3d.rs`) keeps
//! a mesh ray from hitting the wrong surface; the ghost is `IGNORE`d in `build.rs` too.
//!
//! Selection is driven by a global observer on `Pointer<Click>` ([`on_pick_structure`]); a click
//! on empty space (nothing pickable under the cursor) clears it ([`clear_selection_on_empty_click`]).

use bevy::color::Mix;
use bevy::picking::{hover::HoverMap, pointer::PointerId};
use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::Site;
use crate::build::{BuildMode, Footprint, Lander, ModuleCatalog, ModuleKind, Structure};
use crate::ground::{GroundAnchor, TerrainField};

/// The currently selected entity (a [`Structure`] or the [`Lander`]), or `None`.
#[derive(Resource, Default)]
pub struct Selection(pub Option<Entity>);

type Selectable = Or<(With<Structure>, With<Lander>)>;

/// Marks the selected entity and caches its original material colour so deselect can restore it.
#[derive(Component)]
struct Selected {
    original_base_color: Color,
}

/// Set the selection on a left-click of a pickable entity. A global observer; terrain and the ghost
/// are `Pickable::IGNORE`, so the only pick targets are structures and the lander — no marker filter
/// is needed (and the details panel reads the entity's components live anyway).
fn on_pick_structure(
    on: On<Pointer<Click>>,
    build: Res<BuildMode>,
    egui_wants: Res<EguiWantsInput>,
    mut selection: ResMut<Selection>,
    parents: Query<&ChildOf>,
    owners: Query<(), Selectable>,
) {
    // While placing, a click means *place* (Plan 02) — never select.
    if build.placing.is_some() {
        return;
    }
    if on.event.button != PointerButton::Primary {
        return;
    }
    // Don't pick through an egui panel. Read the plain `EguiWantsInput` resource (maintained by
    // bevy_egui) rather than `EguiContexts`, which is awkward in an observer's deferred context.
    if egui_wants.wants_pointer_input() {
        return;
    }
    selection.0 = structure_owner(on.entity, &parents, &owners);
}

/// Scene meshes may be arbitrarily deep; gameplay state stays on their owning root.
fn structure_owner(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    owners: &Query<(), Selectable>,
) -> Option<Entity> {
    loop {
        if owners.contains(entity) {
            return Some(entity);
        }
        entity = parents.get(entity).ok()?.parent();
    }
}

/// GLB materials are shared assets: use a footprint marker instead of tinting them.
fn highlight_model(
    selection: Res<Selection>,
    models: Query<(&GlobalTransform, &Footprint), With<SceneRoot>>,
    mut gizmos: Gizmos,
) {
    if let Some(entity) = selection.0
        && let Ok((transform, footprint)) = models.get(entity)
    {
        gizmos.circle(
            Isometry3d::new(
                transform.translation() + Vec3::Y * 0.08,
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            ),
            footprint.radius_m,
            Color::srgb(1.0, 0.95, 0.4),
        );
    }
}

/// A left-click on empty space / terrain clears the selection. `HoverMap` is empty for the mouse
/// pointer when nothing pickable is under the cursor (terrain and ghost are `IGNORE`d), so this can
/// never race the select observer: a module hit ⇒ non-empty ⇒ no clear; empty hit ⇒ no observer.
fn clear_selection_on_empty_click(
    mouse: Res<ButtonInput<MouseButton>>,
    hover: Res<HoverMap>,
    build: Res<BuildMode>,
    egui_wants: Res<EguiWantsInput>,
    mut selection: ResMut<Selection>,
) {
    if !mouse.just_pressed(MouseButton::Left) || build.placing.is_some() {
        return;
    }
    if egui_wants.wants_pointer_input() {
        return;
    }
    let empty = hover.get(&PointerId::Mouse).is_none_or(|m| m.is_empty());
    if empty {
        selection.0 = None;
    }
}

/// Static highlight (no animation): brighten the selected entity's base colour and cache the
/// original so deselect restores it. Brightening `base_color` works for the modules' `unlit: true`
/// materials, and each entity owns its own material handle, so mutating the asset in place is safe.
fn update_highlight(
    selection: Res<Selection>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mats: Query<&MeshMaterial3d<StandardMaterial>>,
    prev: Query<(Entity, &Selected)>,
    mut commands: Commands,
) {
    if !selection.is_changed() {
        return;
    }
    // Restore + unmark the previous selection (handles select→deselect and select→reselect).
    for (e, sel) in &prev {
        if let Ok(h) = mats.get(e)
            && let Some(m) = materials.get_mut(&h.0)
        {
            m.base_color = sel.original_base_color;
        }
        commands.entity(e).remove::<Selected>();
    }
    // Highlight the new selection (no-op if the entity is gone or has no standard material).
    if let Some(e) = selection.0
        && let Ok(h) = mats.get(e)
        && let Some(m) = materials.get_mut(&h.0)
    {
        let original = m.base_color;
        m.base_color = original.mix(&Color::srgb(1.0, 0.95, 0.4), 0.6); // bright warm tint
        commands.entity(e).insert(Selected {
            original_base_color: original,
        });
    }
}

/// Read-only Details panel for the current selection. Reads live ECS every frame, so it always
/// reflects the current selection. Runs on `EguiPrimaryContextPass` like `debug_ui` / `build_menu`.
fn details_panel(
    mut contexts: EguiContexts,
    selection: Res<Selection>,
    catalog: Res<ModuleCatalog>,
    field: Res<TerrainField>,
    site: Res<Site>,
    structures: Query<(&ModuleKind, &GroundAnchor), With<Structure>>,
    landers: Query<&GroundAnchor, With<Lander>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let Some(e) = selection.0 else {
        return Ok(());
    };
    let m = &site.0;
    egui::Window::new("details").show(ctx, |ui| {
        if let Ok((kind, a)) = structures.get(e) {
            let Some(def) = catalog.get(&kind.0) else {
                return;
            };
            ui.heading(&def.display_name);
            ui.label(&def.blurb);
            ui.label(format!("pos: ({:.0}, {:.0}) m", a.xy.x, a.xy.y));
            ui.label(format!("elevation: {:.0} m", field.height_at(m, a.xy))); // true metres (unscaled)
            ui.label(format!("slope: {:.1}°", field.slope_deg_at(m, a.xy)));
            if let Some(p) = def.power_kw {
                ui.label(format!("power: {p} kW"));
            }
            if let Some(s) = def.power_storage_kwh {
                ui.label(format!("storage: {s} kWh"));
            }
            if let Some(c) = def.crew_capacity {
                ui.label(format!("crew: {c}"));
            }
        } else if let Ok(a) = landers.get(e) {
            ui.heading("Lander");
            ui.label(format!("pos: ({:.0}, {:.0}) m", a.xy.x, a.xy.y));
            ui.label(format!("elevation: {:.0} m", field.height_at(m, a.xy)));
        }
    });
    Ok(())
}

pub struct SelectionPlugin;

impl Plugin for SelectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MeshPickingPlugin)
            .init_resource::<Selection>()
            .add_observer(on_pick_structure)
            .add_systems(
                Update,
                (
                    update_highlight,
                    highlight_model,
                    clear_selection_on_empty_click,
                ),
            )
            .add_systems(EguiPrimaryContextPass, details_panel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_child_resolves_owner_and_non_structure_is_ignored() {
        let mut world = World::new();
        let root = world.spawn(Structure).id();
        let node = world.spawn(ChildOf(root)).id();
        let mesh = world.spawn(ChildOf(node)).id();
        let unrelated = world.spawn_empty().id();
        let mut state = bevy::ecs::system::SystemState::<(
            Query<&ChildOf>,
            Query<(), Or<(With<Structure>, With<Lander>)>>,
        )>::new(&mut world);
        let (parents, owners) = state.get(&world);
        assert_eq!(structure_owner(mesh, &parents, &owners), Some(root));
        assert_eq!(structure_owner(root, &parents, &owners), Some(root));
        assert_eq!(structure_owner(unrelated, &parents, &owners), None);
    }
}
