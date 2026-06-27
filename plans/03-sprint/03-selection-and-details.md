# Sprint 03 · Plan 03 — Selection & details

## Context

Third and final plan of Sprint 03 (see [`00-overview.md`](00-overview.md)). With the lander
and modules placed (Plan 02), this plan lets the player **click to select** a placed entity,
gives it a **static highlight**, and shows a **details panel** — closing the
render → position → **select → inspect** loop the sprint is about.

**Dependencies:** Plan 02 (`Structure`, `ModuleKind`, `GroundAnchor`, `Footprint`, `Lander`,
`ModuleCatalog`, `BuildMode`), Plan 01 (`ProjectionMode` for the egui toggle button).

## Scope

New module `crates/moon_game/src/selection.rs` (+ a `SelectionPlugin`) and egui additions
(extend `debug_ui.rs` or add a `game_ui` system):

- `Selection` resource + `Selected` marker; click-to-select via `MeshPickingPlugin`.
- Static selection highlight (emissive tint or a flat ring under the entity).
- egui **Build** menu (drives Plan 02's `BuildMode`) + **Details** panel for the selection +
  a **projection toggle** button (Plan 01).

## Approach

### Picking: entities yes, terrain no

Add Bevy's `MeshPickingPlugin` (not in `DefaultPlugins`). It raycasts real meshes — perfect
for the lander/modules, which are genuine geometry. The **terrain mesh must be excluded**: it
is flat on the CPU (displaced only on the GPU), so a mesh ray would hit the wrong surface.

- On the terrain entity (spawned in `terrain3d.rs::setup`) add `Pickable::IGNORE`.
- Structures/lander get `Pickable::default()` (Plan 02); the ghost gets `Pickable::IGNORE`.

```rust
#[derive(Resource, Default)]
pub struct Selection(pub Option<Entity>);

#[derive(Component)]
pub struct Selected;
```

`on_pick_structure` — an **observer** on `Pointer<Click>` (added per pickable entity in Plan
02's spawner, or globally with `app.add_observer`):

- Ignore while `BuildMode.placing.is_some()` (a click then means *place*, Plan 02) or when
  egui wants the pointer.
- On a left click on a pickable entity: set `Selection(Some(entity))`.
- A click on empty space / terrain clears the selection (a global `Pointer<Click>` on the
  terrain, or "no hit this frame" → `Selection(None)`).

> **Fallback if the picking API has friction:** roll selection from the cursor ray
> (`viewport_to_world`, already used in Plan 01) tested against each structure's bounding
> sphere (`GroundAnchor` centre + `Footprint.radius_m`), nearest hit wins. Note which path
> was taken in the code comment. Either way the rest of this plan (highlight, panel) is
> unchanged.

### Selection highlight (static — no animation)

`update_highlight` (Update, on `Selection` change): clear `Selected` from the previous entity
and add it to the new one, then make it visually distinct **statically**:

- **Simplest:** swap the selected entity to a brightened/emissive copy of its material (cache
  the original handle so deselect restores it), or
- **Decal ring:** spawn a thin flat ring/disc child at the entity's base sized to
  `Footprint.radius_m` (a `Torus`/`Annulus` mesh, bright unlit colour), despawned on
  deselect.

Pick one (emissive tint is the least code). No pulsing/animation.

### egui panels

Extend the existing overlay (`debug_ui.rs`) or add a `game_ui` system on
`EguiPrimaryContextPass` (same constraints: multi-pass mode, `ctx_mut()?`). Three pieces:

**Build menu** — a row/section of buttons, one per `ModuleCatalog` entry:

```rust
for def in catalog.0.values() {
    let active = build.placing.as_ref() == Some(&def.id);
    if ui.selectable_label(active, &def.display_name).clicked() {
        build.placing = if active { None } else { Some(def.id.clone()) };
    }
}
if ui.button("Cancel (Esc)").clicked() { build.placing = None; }
```

**Projection toggle** — a button mirroring `P` (Plan 01): `if ui.button(match mode { Perspective
=> "Isometric view", Iso => "Perspective view" }).clicked() { /* flip ProjectionMode */ }`.

**Details panel** — when `Selection.0` is `Some(e)`, query the entity and show:

- For a `Structure`: `display_name`, world XY (`GroundAnchor.xy`), **elevation**
  (`TerrainField::height_at`, true metres), **slope** (`slope_deg_at`), and whichever catalog
  stats are `Some` (`power_kw` / `power_storage_kwh` / `crew_capacity`), plus the `blurb`.
- For the `Lander`: name "Lander" + position/elevation (no module stats).

```text
┌ Habitat ───────────────┐
│ Pressurised crew quarters
│ pos: (-120, 340) m
│ elevation: 812 m
│ slope: 3.2°
│ crew: 4
└────────────────────────┘
```

Read-only this sprint (no rename/move/delete; those are later). The panel reads live ECS, so
it reflects the current selection every frame.

### Plugin & wiring

`SelectionPlugin`: add `MeshPickingPlugin`; `init_resource::<Selection>()`; `Update:
update_highlight` (+ `add_observer(on_pick_structure)` if global). The egui additions register
on `EguiPrimaryContextPass` like `debug_ui`. Register `SelectionPlugin` after `BuildPlugin` in
`main.rs`.

## Reuse

| Reused | From | For |
| --- | --- | --- |
| `Structure`, `ModuleKind`, `GroundAnchor`, `Footprint`, `Lander`, `ModuleCatalog`, `BuildMode` | Plan 02 / `build.rs` | what to select + what to show |
| `TerrainField::height_at` / `slope_deg_at` | Plan 01 / `ground.rs` | elevation + slope in the details panel |
| `ProjectionMode` | Plan 01 | egui projection toggle button |
| egui overlay pattern (`EguiPrimaryContextPass`, `ctx_mut()?`, window) | `crates/moon_game/src/debug_ui.rs` | build menu + details panel |
| `viewport_to_world` cursor ray | `ground.rs` (Plan 01) | manual-pick fallback |
| `bevy_picking` (`MeshPickingPlugin`, `Pointer<Click>`, `Pickable`) | Bevy 0.18 | entity selection |

## Risks & gotchas

- **Click eaten by egui or build mode.** Gate `on_pick_structure` on `!wants_pointer_input()`
  and `BuildMode.placing.is_none()` so selecting never fights placing or UI clicks.
- **Terrain stealing picks.** Forgetting `Pickable::IGNORE` on the terrain makes every click
  "hit ground"; verify the terrain is excluded.
- **Highlight leak.** Restore the original material (or despawn the ring) on deselect, and
  handle the selected entity being despawned (clear `Selection` if the entity is gone).
- **Two cameras + picking.** Picking uses the active scene camera (`MainCamera`); the egui
  overlay camera (`RenderLayers` UI layer, no scene geometry) shouldn't interfere — confirm
  picks come from `MainCamera`.
- **Iso mode picks.** `MeshPickingPlugin` builds rays from the camera's projection, so
  orthographic selection works without special-casing (matches Plan 01's cursor).

## Verification / exit criterion

1. `cargo run -p moon_game`: with nothing in build mode, click the lander or a placed module →
   it highlights and the **Details** panel shows its name, position, elevation, slope, and
   stats. Click empty terrain → selection clears.
2. Stats match the catalog: a Solar Array shows its `power_kw`; a Battery its
   `power_storage_kwh`; a Habitat its `crew`.
3. The egui **Build** buttons enter/leave placing mode (and reflect the active module); the
   **projection** button toggles perspective ↔ isometric (same as `P`).
4. Selecting while placing does **not** drop a module; placing while a module is selected
   does **not** mis-select.
5. Toggle to isometric → selection + details still work.
6. `cargo clippy` clean.

**Exit:** click-to-select with a static highlight and a live details panel for the lander and
all three module types — completing the Sprint 03 render → position → select → inspect loop.

---

## Sprint 03 wrap-up (after all three plans)

- **End-to-end demo / `dev-clips/`:** boot → a few seconds orbiting the lander → open the
  build menu → place a hab, a solar array, a battery (green/red slope feedback visible) →
  select each and show its details → toggle to isometric and orbit the little base. This is
  the It-1 *"placing a base"* beat, captured without any landing animation.
- **Definition of done:** all three plans' exit criteria met; `cargo test` + `cargo clippy`
  green; `modules.ron` drives the catalog; `vexag` is a constant with no slider; day/night
  overlay and `moon_sim` remain explicitly out of scope (queued for later sprints).
