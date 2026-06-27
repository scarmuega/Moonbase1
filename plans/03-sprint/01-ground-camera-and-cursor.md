# Sprint 03 · Plan 01 — Ground sampling, camera & cursor

## Context

First plan of Sprint 03 (see [`00-overview.md`](00-overview.md)). It builds the
**world/camera foundation** that placement (Plan 02) and selection (Plan 03) stand on.

The Sprint 02 terrain is a **flat CPU mesh displaced in the vertex shader**
([`../../crates/moon_game/src/terrain3d.rs`](../../crates/moon_game/src/terrain3d.rs)) — the
GPU knows the real relief, the CPU does not. So *any* gameplay that needs "where is the
ground here?" (place an entity, test a slope, find the point under the cursor) cannot use a
mesh raycast — it would hit the flat plane. This plan adds a **CPU height field** that reads
the same DEM the shader reads, plus the cursor ray-march and the entity-grounding rule that
keep objects glued to the rendered surface.

It also makes **vertical exaggeration a constant** (so the surface the entities sit on never
moves) and adds the **perspective ↔ orthographic ("isometric") camera toggle** the user asked
for when exploring rendering options.

**Dependencies:** Sprint 02 (`Site`, `CameraRig`, `MainCamera`, the DEM asset,
`world_to_dem_uv`). No new data bake.

## Scope

New module `crates/moon_game/src/ground.rs` (+ a `GroundPlugin`) and small edits to
`terrain3d.rs` / `debug_ui.rs`:

- Freeze `vexag` to a shared `TERRAIN_VEXAG` constant; remove the live slider.
- `TerrainField` resource: CPU DEM samples + `height_at` / `slope_deg_at`.
- `CursorGround` resource + `cursor_ground` system: mouse → world point on the terrain.
- `ground_on_spawn`: derive each new `GroundAnchor` entity's `Transform.y` from the field.
- `ProjectionMode` + `toggle_projection`: perspective orbit ↔ orthographic isometric.

## Approach

### Fix vertical exaggeration to a constant

`vexag` is currently a live uniform driven by `CameraRig.vexag` and a 1–8 egui slider
(`terrain3d.rs`, `debug_ui.rs`). Gameplay needs the surface to be stationary so entities can
be grounded once. Replace it with a single source of truth:

```rust
// ground.rs (shared by terrain rendering and CPU grounding)
pub const TERRAIN_VEXAG: f32 = 1.5; // was DEFAULT_VEXAG; tune once, never animate
```

- In `terrain3d.rs`: drop the `vexag` field from `CameraRig`; set `TerrainParams.vexag =
  TERRAIN_VEXAG` at material build time; remove `vexag` from the `sync_terrain` change-set
  (it never changes now). The `synth` and sun uniforms stay live.
- In `debug_ui.rs`: remove the `vertical exag.` slider line.

> Keep `TERRAIN_VEXAG` in `ground.rs` (next to `height_at`) so the *renderer* and the
> *grounding* read the identical value — a mismatch would float or sink every entity.

### `TerrainField` — the CPU height field

The DEM is loaded for rendering as a Bevy `Image` (render-world only), so read the PNG
*separately* on the CPU at startup with the `image` crate (already a workspace dep; add
`image = { workspace = true }` to `moon_game/Cargo.toml`).

```rust
#[derive(Resource)]
pub struct TerrainField {
    width: u32,
    height: u32,
    samples: Vec<u16>,  // row-major, row 0 = north edge (matches DEM UV v-flip)
    elev_min: f32,
    elev_max: f32,
}
```

`load_terrain_field` (Startup): open `{ASSET_ROOT}/{manifest.dem.path}`, decode as 16-bit
luma (`image::open(..).into_luma16()`), copy into `samples`, store `elev_min/max` from the
manifest's `DemInfo`.

**`height_at(&self, m: &SiteManifest, xy: Vec2) -> f32`** — mirror the shader decode exactly:

1. `uv = world_to_dem_uv(m, xy)` (from `moon_data` — the single source of truth; do **not**
   reinvent the y-flip).
2. Map `uv` to fractional pixel coords `(uv.x·(width−1), uv.y·(height−1))`, **clamp** to the
   grid.
3. **Bilinear** sample the four neighbouring `u16`s → `s ∈ [0,1]` via `raw / 65535`.
4. `elev = elev_min + s·(elev_max − elev_min)`. Return raw metres (callers multiply by
   `TERRAIN_VEXAG` for the 3D Y).

**`slope_deg_at(&self, m: &SiteManifest, xy: Vec2) -> f32`** — central finite difference one
DEM pixel (`world_per_pixel(m, 0)` metres, i.e. `base_mpp`) east/north of `xy`:

```text
dz/dx = (height_at(x+e) − height_at(x−e)) / (2e)
dz/dy = (height_at(y+e) − height_at(y−e)) / (2e)
slope = atan( sqrt(dz/dx² + dz/dy²) )   // radians → degrees
```

Use **true metres** here (not `TERRAIN_VEXAG`-scaled) so the buildability test reflects the
real Moon, independent of the rendering exaggeration.

> **Unit tests** (in `ground.rs`, with a tiny synthetic field + the `test_manifest` pattern
> from `moon_data`): the four DEM corners decode to the expected elevations; a flat field has
> `slope ≈ 0`; a known constant-gradient ramp gives the expected slope angle; `height_at` is
> continuous across a texel boundary (bilinear).

### `CursorGround` — mouse → point on the terrain

```rust
#[derive(Resource, Default)]
pub struct CursorGround(pub Option<Vec2>); // world XY (north-Y), None if off-terrain / over UI
```

`cursor_ground` (Update), after `EguiContexts::ctx_mut()?.wants_pointer_input()` early-out:

1. Window cursor position → `Camera::viewport_to_world(camera_transform, cursor) -> Ray3d`.
   This works for **both** projections (perspective rays diverge from the eye; orthographic
   rays are parallel along camera forward) — no special-casing.
2. **Ray-march vs. the height field.** The terrain is a height field and the camera is always
   above it, so march the ray in fixed world steps (e.g. `step = diag / 512`, cap ~512
   steps): at each sample point convert the 3D position back to world XY (`x = p.x`,
   `y = −p.z`) and compare `p.y` against `height_at(xy)·TERRAIN_VEXAG`. Detect the first
   sign change of `p.y − surface_y`, then **bisect** ~8 iterations for a sub-step hit.
3. Set `CursorGround(Some(xy))`, or `None` if the ray never crosses (looking at the sky).

> Robust and projection-agnostic; precision (~step/256) is far below a module footprint. A
> plane-intersection first guess is *not* reliable at oblique/iso angles — prefer the march.

### `ground_on_spawn` — keep entities on the surface

```rust
#[derive(Component, Copy, Clone)]
pub struct GroundAnchor { pub xy: Vec2 }
```

`ground_on_spawn` (Update), query `Added<GroundAnchor>` (also re-run if `xy` changes via
`Changed<GroundAnchor>`): set

```text
transform.translation = (xy.x, height_at(xy)·TERRAIN_VEXAG + half_height, −xy.y)
```

where `half_height` lifts a mesh whose origin is its centre so it rests *on* (not *in*) the
ground — pass it in from the spawner (Plan 02) or store it on the component. Because vexag is
constant there is **no** per-frame re-grounding system; this runs once per entity. (The ghost
in Plan 02 updates every frame as the cursor moves, reusing the same formula.)

### Perspective ↔ orthographic ("isometric") toggle

```rust
#[derive(Resource, Default, PartialEq)]
pub enum ProjectionMode { #[default] Perspective, Iso }
```

`toggle_projection` (Update): on key **`P`** (and an egui button — Plan 03) flip the mode and
rewrite the `MainCamera`'s `Projection`:

- **Perspective:** the existing `PerspectiveProjection` (FOV/near/far from `CameraRig`); the
  orbit controller behaves as today.
- **Iso:** `Projection::Orthographic(OrthographicProjection { scaling_mode:
  ScalingMode::FixedVertical, scale: …, near, far })`. Drive `scale` from `CameraRig.distance`
  (wheel still "zooms" by changing the ortho extent) and **lock pitch** to a fixed oblique
  angle (~35°) so it reads as a classic management-game isometric; yaw still rotates.

The `orbit_camera` system (in `terrain3d.rs`) keeps writing yaw/distance into `CameraRig`;
when `Iso`, clamp/ignore pitch and translate `distance → ortho scale` there (or in
`toggle_projection`). `CursorGround` already handles both projections, so placement works in
either mode.

> Keep the toggle additive: perspective is the default and the cinematic flythrough
> (`flythrough.rs`) only needs to run in perspective — gate it off (or snap back to
> perspective) while in `Iso`.

### Plugin & ordering

`GroundPlugin`: `Startup: load_terrain_field`; `Update: (cursor_ground, ground_on_spawn,
toggle_projection)`. Order `cursor_ground` **before** `update_ghost` (Plan 02) so the ghost
reads a fresh cursor. Register it in `main.rs` before `Terrain3dPlugin` reads `TERRAIN_VEXAG`
(or keep the constant import-only so ordering is irrelevant).

## Reuse

| Reused | From | For |
| --- | --- | --- |
| `world_to_dem_uv`, `world_per_pixel`, `DemInfo`, `world_min/max` | `crates/moon_data/src/lib.rs` | UV mapping + pixel spacing in `height_at`/`slope_deg_at` |
| DEM decode `elev = elev_min + s·(elev_max−elev_min)` | `assets/shaders/terrain.wgsl` | mirror byte-for-byte in `height_at` |
| `CameraRig` (yaw/pitch/distance, `transform()`), `MainCamera` | `crates/moon_game/src/terrain3d.rs` | cursor ray, projection toggle |
| `ASSET_ROOT`, manifest-as-`Res<Site>` | `crates/moon_game/src/main.rs` | CPU DEM file read |
| egui pointer-yield pattern (`wants_pointer_input`) | `terrain3d.rs::orbit_camera` | skip cursor pick over UI |
| `image` crate (workspace dep) | `tools/geo_pipeline` | decode the 16-bit DEM PNG |

## Risks & gotchas

- **DEM not in the main world.** Bevy's loaded DEM `Image` is render-world only; read the PNG
  independently (do not try to pull texels from the asset). Confirmed: `image` is a workspace
  dep.
- **UV / y-flip drift.** Always go through `moon_data::world_to_dem_uv`; row 0 of `samples`
  is the north edge. Re-deriving the flip is the classic way to mirror the terrain.
- **Vexag mismatch.** Renderer and grounding must read the *same* `TERRAIN_VEXAG`; keep it in
  one place.
- **Slope in real metres.** Use unscaled heights for `slope_deg_at`, exaggerated heights only
  for the visual Y — otherwise `TERRAIN_VEXAG` would silently change buildability.
- **Ortho clipping.** Orthographic near/far over a ±60 km site (southpole) needs a generous
  far plane and a near that can go slightly negative-relative; size from the bbox diag as the
  perspective path already does.

## Verification / exit criterion

1. `cargo run -p moon_game` and `MOON_SITE=shackleton cargo run -p moon_game` both boot;
   `cargo test -p moon_game` passes the new `ground.rs` unit tests; `cargo clippy` clean.
2. (Temporary debug marker — a small sphere spawned at `CursorGround` each frame): move the
   mouse → the marker rides the real relief, dipping into craters and climbing rims, under
   any orbit angle. Remove the marker before merge (Plan 02's ghost supersedes it).
3. Press **`P`**: the view switches to orthographic isometric (flat, distortion-free) and
   back; yaw still rotates, wheel still scales, and the cursor marker stays correct in both.
4. The egui overlay has **no `vertical exag.` slider**; terrain renders at the constant
   exaggeration.

**Exit:** the cursor resolves to a correct point on the real surface in both projections, and
new `GroundAnchor` entities sit exactly on the terrain — the hook Plans 02 and 03 hang on.
