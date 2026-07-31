# Sprint 03 — Touchdown: the first game-object layer · Overview

> Index + shared context for the third roadmap iteration. The detailed work lives in the
> three sibling plans below; each is self-contained. This sprint follows Sprint 02
> ([`../02-sprint/00-overview.md`](../02-sprint/00-overview.md)) and is the first slice of
> roadmap **It-1 "Touchdown"** ([`../../docs/04-roadmap.md`](../../docs/04-roadmap.md)).

## Goal

Put the **first game objects** on the real lunar terrain: a static **lander** plus three
placeable **module types** (hab, solar array, battery) with **slope-based buildability**,
**selection**, and a **details panel**. This is the *game-object layer* — rendering,
positioning, selecting, and inspecting entities — sitting on top of the Sprint 02 terrain.

There is no simulation yet (power/O₂/water networks, crew, time controls all arrive in It-2,
`moon_sim`). This sprint is purely about *placing and looking at things on the Moon*.

### Refinement of the roadmap It-1

The roadmap bundles It-1 as: *"Starship-style lander arrives (simple animation); place 3
module types with slope-based buildability; day/night lighting overlay from the real
illumination model."* This sprint **deliberately narrows** that to the game-object layer and
**defers** two pieces (see *Out of scope*):

- **No landing animation** — the lander is a static placed entity. (No animations at all
  this sprint; selection/placement feedback is static tint/ring.)
- **Day/night illumination overlay deferred** — that is an illumination/cast-shadow feature
  (precomputed horizon-mask timetable + true geometric shadows; the current shader does
  per-fragment relief shading only). The live sun + `G` sweep already reads as "shadows
  move." It returns as its own sprint.

## The three plans

This sprint is split so the **world/camera foundation** lands and is validated before the
gameplay built on top of it.

- **[Plan 01 — Ground sampling, camera & cursor](01-ground-camera-and-cursor.md).** The
  foundation everything else needs: a **CPU height field** (`TerrainField`) that samples the
  real DEM in Rust (height + slope), the **cursor → ground** ray-march that turns a mouse
  position into a world point on the terrain, the **fixed vertical-exaggeration constant**
  (so entities sit exactly on the rendered surface), one-shot entity **grounding**, and the
  **perspective ↔ orthographic ("isometric") camera toggle**. New module
  `crates/moon_game/src/ground.rs`. **Exit:** a marker tracks the cursor across the real
  relief; the camera toggles between perspective and iso.

- **[Plan 02 — Modules & placement](02-modules-and-placement.md).** Data-driven module
  catalog (`assets/data/modules.ron`), the **build menu**, the **ghost preview** with
  **footprint-aware slope buildability** (green/red), `place_module`, and the static
  **lander**. New module `crates/moon_game/src/build.rs`. **Exit:** place a hab / solar array
  / battery on buildable ground; steep slopes reject placement.

- **[Plan 03 — Selection & details](03-selection-and-details.md).** Click-to-select
  (`MeshPickingPlugin`), a static **selection highlight**, and the egui **details panel**
  (type, position, elevation, slope, stats). New module
  `crates/moon_game/src/selection.rs` + egui additions. **Exit:** click a placed module →
  it highlights and its details show.

```
Plan 01 (foundation) ──► Plan 02 (placement) ──► Plan 03 (selection/details)
```

Plans 02 and 03 **depend on Plan 01** (height field, cursor, grounding). Land 01 first.

## ECS entity model (shared across all three plans)

All new state is real Bevy ECS. For It-1 the ECS *is* the source of truth; the deterministic
`moon_sim` projection (events in / commands out) arrives in It-2 — see
[`../../docs/03-architecture.md`](../../docs/03-architecture.md).

**Components**

| Component | Fields | Meaning |
| --- | --- | --- |
| `Structure` | — | marker: a placed buildable (vs lander / terrain / cameras) |
| `ModuleKind` | `ModuleId` | which catalog entry; `ModuleId` = serde-transparent `String` newtype (mirrors `SiteId` in `moon_data`) |
| `GroundAnchor` | `xy: Vec2` | **authoritative** planar position (world X / north-Y). `Transform.y` is derived from it at spawn |
| `Footprint` | `radius_m: f32` | from the catalog; buildability test radius + pick bounds |
| `Lander` | — | marker: the single, pre-placed lander |
| `Ghost` | — | marker: the live placement-preview entity |
| `Selected` | — | marker: the currently selected entity |

**Resources**

| Resource | Shape | Meaning |
| --- | --- | --- |
| `TerrainField` | `{ width, height, samples: Vec<u16>, elev_min, elev_max }` | CPU copy of the DEM; `height_at` / `slope_deg_at` |
| `ModuleCatalog` | `HashMap<ModuleId, ModuleDef>` | loaded from `assets/data/modules.ron` |
| `BuildMode` | `{ placing: Option<ModuleId> }` | `None` = inspect/select; `Some` = placing |
| `CursorGround` | `Option<Vec2>` | world XY under the cursor (ray-march result) |
| `Selection` | `Option<Entity>` | the selected structure |
| `ProjectionMode` | `Perspective \| Iso` | camera projection toggle state |

`TERRAIN_VEXAG` is a **module constant** (not a resource) — shared by the terrain material
setup and CPU grounding so the rendered surface and the placed entities never diverge.

**Systems** (schedule in parens; details in the per-plan docs)

- `load_terrain_field`, `load_module_catalog`, `spawn_lander` (Startup)
- `cursor_ground` → `update_ghost` → `place_module` / `cancel_build` (Update)
- `ground_on_spawn` (Update) — derive `Transform.y` once per new `GroundAnchor`
- `toggle_projection` (Update) — perspective ↔ orthographic
- `on_pick_structure` (observer) → `update_highlight` (Update)
- `game_ui` (EguiPrimaryContextPass) — build buttons + details panel

## Rendering options considered (3D / 2.5D / isometric)

| Option | What it means here | Verdict |
| --- | --- | --- |
| **Full perspective 3D** (current) | `Camera3d` + perspective, displaced DEM, orbit rig; entities are 3D meshes in the same scene | **Primary.** Already built (Sprint 02); parallax + occlusion is what killed the relief-inversion illusion. Entities just join the scene. |
| **Orthographic "isometric" 3D** | same scene, swap `Projection::Perspective → Orthographic` + a locked oblique pitch | **Included as a runtime toggle** — distortion-free base-building readability. Perspective stays the default. |
| **2.5D billboards / sprite icons** | entities as camera-facing sprites over the 3D terrain | **Rejected** — fights the 3D parallax the terrain relies on; reintroduces depth ambiguity. |
| **Drop to 2D / iso tilemap** | abandon the 3D terrain | **Rejected** — discards the Sprint 02 foundation and the "real ground" pillar. |

**Entity mesh style:** Bevy **primitive meshes** (`Cuboid`, `Cylinder`, a thin tilted panel)
color-coded per module — zero asset pipeline, no rigging, fits "no animations." **glTF models
are deferred.**

## Verified codebase facts (carried into all three plans)

- **Stack:** Bevy 0.18.1, `bevy_egui` 0.39 (egui 0.33). One perspective `Camera3d` (the
  orbit rig) + one always-active egui camera; see
  [`../../crates/moon_game/src/terrain3d.rs`](../../crates/moon_game/src/terrain3d.rs).
- **World space:** 1 unit = 1 m, XY plane, **+Y = north**, origin at bbox center. 3D mapping
  is `world (x, y, h) → (x, h·vexag, −y)` (`terrain3d.rs`).
- **The terrain mesh is flat on the CPU** — it is a plane in the XZ plane, displaced only in
  the **vertex shader** (`assets/shaders/terrain.wgsl`). A standard mesh raycast therefore
  hits the *flat* plane, not the real surface. **This is the central technical constraint of
  the sprint** and is why Plan 01 builds a CPU height field for terrain queries and why
  terrain is excluded from mesh picking (Plan 03).
- **DEM:** `assets/elevation/<site>.png`, 16-bit grayscale, decoded by Bevy as **R16Uint**
  (sampled with `textureLoad`, no sampler). Decode: `elev = elev_min + (raw/65535)·(elev_max
  − elev_min)`. `southpole` 3000×3000 @ 40 m/px (±60 km); `shackleton` 3200×3200 @ 5 m/px
  (±8 km). `moon_data::world_to_dem_uv` is the single source of truth for world→texture UV.
- **`vexag` becomes a constant this sprint** (was a live 1–8 slider, default 1.5). See
  Plan 01.
- **Asset root** resolves via absolute `CARGO_MANIFEST_DIR`, not cwd (`main.rs::ASSET_ROOT`);
  `image` 0.25 is already a workspace dependency.

## Decisions locked for Sprint 03

1. **Vertical exaggeration is a fixed constant** (`TERRAIN_VEXAG`), shared by terrain
   rendering and CPU grounding. The live slider / `CameraRig.vexag` is removed. Entities are
   grounded **once at spawn** — no per-frame re-grounding.
2. **Terrain queries go through the CPU `TerrainField`**, never a mesh raycast. The render
   mesh stays GPU-displaced and flat on the CPU.
3. **2.5D is a projection toggle**, not a new pipeline — same `Camera3d`, swap perspective ↔
   orthographic.
4. **Entities are color-coded primitive meshes.** No glTF, no animation.
5. **Modules are data-driven** (`assets/data/modules.ron`); the game hardcodes no module
   stats or geometry.

## Out of scope for this sprint

- **Day/night illumination overlay & cast shadows** — precomputed horizon-mask timetable and
  true geometric shadows; their own future sprint. (Live sun + `G` sweep unchanged.)
- **Simulation** — power / O₂ / water networks, crew, time controls, alerts → It-2
  (`moon_sim`).
- **Mining / ISRU / rovers / crafting** → It-3.
- **Animations** of any kind (landing, build, hover).
- **New data bakes / sites** — both baked sites work as-is.
