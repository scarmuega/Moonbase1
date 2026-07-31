# Sprint 03 · Plan 02 — Modules & placement

## Context

Second plan of Sprint 03 (see [`00-overview.md`](00-overview.md)). With Plan 01's height
field and cursor in place, this plan adds the **buildables**: the static **lander**, a
**data-driven catalog** of three module types, a **build menu**, and a **ghost preview** that
shows where a module would go and whether the ground there is **buildable** (slope-based).

This is the visible heart of the roadmap It-1 beat — *"place 3 module types with slope-based
buildability"* — minus the animation.

**Dependencies:** Plan 01 (`TerrainField`, `CursorGround`, `GroundAnchor`, `ground_on_spawn`,
`TERRAIN_VEXAG`). `Site` manifest from Sprint 02.

## Scope

New module `crates/moon_game/src/build.rs` (+ a `BuildPlugin`) and a new data file
`assets/data/modules.ron`:

- `ModuleDef` + `ModuleCatalog`, loaded from RON (creates the `assets/data/` convention).
- `BuildMode` resource + a build menu (egui buttons in Plan 03's panel, or a minimal one
  here) to enter placing mode.
- `Ghost` preview entity: follows the cursor, tinted **green/red** by buildability.
- `place_module`: commit a placement; `cancel_build`: leave placing mode.
- `spawn_lander`: the single static lander at startup.

## Approach

### Data-driven module catalog

Per [`../../docs/03-architecture.md`](../../docs/03-architecture.md), buildings are RON in
`assets/data/`. This sprint creates that directory. `ModuleId` is a serde-transparent
`String` newtype (mirrors `SiteId` in `moon_data`) so adding a module is data-only.

```rust
// build.rs
#[derive(Clone, Deserialize)]
pub struct ModuleDef {
    pub id: ModuleId,
    pub display_name: String,
    pub blurb: String,
    pub footprint_radius_m: f32,  // buildability test radius + selection bounds
    pub max_slope_deg: f32,       // steepest ground it may sit on
    pub color: [f32; 3],          // base albedo (Plan 03 tints a copy for selection)
    pub shape: Shape,             // Cuboid | Panel | Cylinder
    pub size_m: Vec3,             // bounding dims; half-height = size_m.y * 0.5
    // optional stats, shown in the details panel (Plan 03):
    pub power_kw: Option<f32>,           // solar peak output
    pub power_storage_kwh: Option<f32>,  // battery capacity
    pub crew_capacity: Option<u32>,      // hab berths
}

#[derive(Clone, Copy, Deserialize)]
pub enum Shape { Cuboid, Panel, Cylinder }

#[derive(Resource)]
pub struct ModuleCatalog(pub HashMap<ModuleId, ModuleDef>);
```

`load_module_catalog` (Startup): read `{ASSET_ROOT}/data/modules.ron` (same direct-read
pattern as `main.rs::load_manifest`), parse a `Vec<ModuleDef>`, index by `id`.

`assets/data/modules.ron` — three entries (numbers are plausible-fun placeholders, tuned in
It-2, not load-bearing yet):

```ron
[
  ( id: "hab", display_name: "Habitat", blurb: "Pressurised crew quarters.",
    footprint_radius_m: 8.0, max_slope_deg: 8.0, color: (0.85, 0.85, 0.88),
    shape: Cuboid, size_m: (12.0, 5.0, 8.0), crew_capacity: Some(4),
    power_kw: None, power_storage_kwh: None ),
  ( id: "solar_array", display_name: "Solar Array", blurb: "Photovoltaic power.",
    footprint_radius_m: 10.0, max_slope_deg: 5.0, color: (0.20, 0.35, 0.70),
    shape: Panel, size_m: (16.0, 4.0, 1.0), power_kw: Some(50.0),
    power_storage_kwh: None, crew_capacity: None ),
  ( id: "battery", display_name: "Battery Bank", blurb: "Stores power for the night.",
    footprint_radius_m: 5.0, max_slope_deg: 10.0, color: (0.90, 0.70, 0.20),
    shape: Cuboid, size_m: (6.0, 3.0, 4.0), power_storage_kwh: Some(200.0),
    power_kw: None, crew_capacity: None ),
]
```

> Hot-reload (dev) is a nice-to-have, not required this sprint. A startup read is enough.

### Build mode

```rust
#[derive(Resource, Default)]
pub struct BuildMode { pub placing: Option<ModuleId> }
```

`None` = inspect/select (Plan 03 owns that); `Some(id)` = the player is placing `id`. Entered
from the build menu (a row of buttons, one per catalog entry — rendered in Plan 03's egui
panel; if Plan 03 isn't landed yet, a stub `egui::Window` "Build" here is fine). `Esc` or
right-click clears it (`cancel_build`).

### Ghost preview + buildability

One reusable mesh/material per shape (built once at startup); the ghost is a single entity
re-shown/retinted as the player switches modules.

`update_ghost` (Update, **after** `cursor_ground`):

1. If `BuildMode.placing` is `None` or `CursorGround` is `None` → hide the ghost
   (`Visibility::Hidden`) and return.
2. Look up the `ModuleDef`. Position the ghost via the **same grounding formula** as Plan 01
   (`y = height_at(xy)·TERRAIN_VEXAG + size_m.y·0.5`).
3. **Footprint-aware buildability:** sample `slope_deg_at` at the centre **and** a ring of
   points at `footprint_radius_m` (e.g. 4–8 compass points). `buildable = every sample ≤
   max_slope_deg` (also require the cursor be on-terrain, i.e. `CursorGround.is_some()`).
   Optionally reject overlap with existing footprints (cheap circle test over `Structure`
   `GroundAnchor`+`Footprint`) — nice-to-have.
4. Tint the ghost material: **green** (`buildable`) / **red** (not). Store the verdict
   somewhere `place_module` can read it (a field on `BuildMode`, or a `Ghost { buildable }`
   component).

```rust
#[derive(Component)]
pub struct Ghost { pub buildable: bool }
```

> Slope uses **unscaled** metres (Plan 01) so buildability reflects the real Moon regardless
> of `TERRAIN_VEXAG`.

### Placing a module

`place_module` (Update): on `MouseButton::Left` `just_pressed`, `BuildMode.placing = Some(id)`,
ghost `buildable`, and pointer not over egui → spawn the real entity:

```rust
commands.spawn((
    Structure,
    ModuleKind(id.clone()),
    GroundAnchor { xy },                 // ground_on_spawn sets Transform.y
    Footprint { radius_m: def.footprint_radius_m },
    Mesh3d(mesh_for(def.shape)),
    MeshMaterial3d(solid_material(def.color)),
    Transform::default(),                // y overwritten by ground_on_spawn
    Pickable::default(),                 // selectable in Plan 03
));
```

`GroundAnchor` triggers Plan 01's `ground_on_spawn`, so the new module snaps onto the
surface. Keep `BuildMode.placing` set so the player can place several (shift-free
chain-build); `Esc`/right-click exits. Emit a `tracing::info!` per placement for now (the
real `SimEvent`/`SimCommand` coupling is It-2).

### The lander (static)

`spawn_lander` (Startup): pick a buildable-ish spot near site centre (origin, or scan a small
neighbourhood for the lowest-slope point) and spawn:

```rust
commands.spawn((
    Lander,
    GroundAnchor { xy: spawn_xy },
    Mesh3d(meshes.add(Cylinder::new(4.0, 20.0))), // tall capsule/cylinder = Starship-ish
    MeshMaterial3d(materials.add(StandardMaterial { base_color: Color::srgb(0.8,0.8,0.82), .. })),
    Transform::default(),
    Pickable::default(),
));
```

No descent, no animation — it simply exists at startup, marking the base origin. It is
`GroundAnchor`-grounded like everything else. (Selectable in Plan 03; its details panel just
shows "Lander" + position.)

### Plugin

`BuildPlugin`: `Startup: (load_module_catalog, spawn_lander)`; `Update: (update_ghost,
place_module, cancel_build)`. Register after `GroundPlugin` in `main.rs`. Ensure system order
`cursor_ground → update_ghost → place_module`.

## Reuse

| Reused | From | For |
| --- | --- | --- |
| `TerrainField::height_at` / `slope_deg_at`, `TERRAIN_VEXAG`, `GroundAnchor`, `ground_on_spawn`, `CursorGround` | Plan 01 / `ground.rs` | ghost grounding + buildability + commit |
| `ModuleId`-style transparent newtype | `SiteId` in `crates/moon_data/src/lib.rs` | catalog keys |
| RON load + panic-on-bad-data | `main.rs::load_manifest` | `modules.ron` reader |
| `Site` / `ASSET_ROOT` | `main.rs` | manifest extent + data path |
| primitive meshes & `StandardMaterial` | `bevy::pbr` / Sprint 02 mesh-build idiom | module/lander visuals |

## Risks & gotchas

- **Footprint ring vs. resolution.** On `southpole` (40 m/px) a small footprint may sample
  within one DEM pixel; that's fine — the ring still captures gross slope. Don't over-sample.
- **Ghost vs. real picking.** Mark the **ghost** `Pickable::IGNORE` so it never intercepts
  Plan 03 selection; mark real structures `Pickable::default()`.
- **Chain-build double-spawn.** Use `just_pressed`, and gate on `wants_pointer_input()` so a
  click on the build button doesn't also drop a module.
- **Lander spawn on a cliff.** Origin can land on a steep cell; a tiny lowest-slope scan
  around centre avoids a tilted/buried lander. Keep it cheap.
- **`assets/data/` is new** — create the dir; confirm `ASSET_ROOT` resolves it (it's a
  sibling of `sites/` and `elevation/`).

## Verification / exit criterion

1. `cargo run -p moon_game` (and `MOON_SITE=shackleton`): a **lander** stands on the surface
   near centre, correctly grounded.
2. Build menu → **Habitat**: a green ghost follows the cursor across the relief; drag it onto
   a steep crater wall → it turns **red**; on gentle ground → **green**. Click on green →
   a hab spawns, glued to the surface. Place **Solar Array** and **Battery** the same way;
   each respects its own `max_slope_deg`.
3. Right-click / `Esc` exits build mode (ghost disappears).
4. Toggle to isometric (`P`, Plan 01): placement still works and modules stay grounded.
5. Editing a number in `modules.ron` (e.g. a footprint or colour) changes behaviour on
   restart with **no code change** — the catalog is data-driven.

**Exit:** all three module types place on buildable ground with green/red slope feedback, a
static lander anchors the site, and modules are defined entirely by `modules.ron`.
