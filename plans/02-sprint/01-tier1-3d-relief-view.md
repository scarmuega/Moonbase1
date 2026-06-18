# Sprint 02 · Plan 01 — Tier 1: 3D relief view (additive mode)

## Context

First plan of Sprint 02 (see [`00-overview.md`](00-overview.md)). The goal is the
**smallest change that puts the real elevation under a tilt-able perspective camera**, so the
relief-inversion illusion is killed by genuine parallax + occlusion rather than by tuning the
2D hillshade.

This is **additive and zero-risk**: a new `Camera3d` and a DEM-displaced terrain mesh shown
as an *optional view mode*. It does **not** touch tile streaming, the 2D camera, the
flythrough, or the existing 2D hillshade quad — toggling the mode hides those and shows the
3D scene instead. It reuses the It-0 hillshade lighting math (now correct) as a 3D material.

**Dependencies:** It-0 (app, manifest as `Res<Site>`, DEM asset, `world_to_dem_uv`,
`HillshadeState`). No new data bake.

## Scope

`crates/moon_game/src/terrain3d.rs` (new) + `assets/shaders/terrain3d.wgsl` (new):

- A `Camera3d` orbit camera (yaw / pitch / distance) over the site center.
- A subdivided ground-plane mesh whose vertices are displaced in a **vertex shader** that
  samples the DEM, with a vertical-exaggeration uniform.
- A `Material` (3D) whose fragment stage reuses the hillshade normal+sun lighting, with a
  switch to instead drape the single zoom-0 imagery texture.
- A **view-mode toggle** (key + egui) that swaps the app between the existing 2D map view and
  the new 3D relief view.

## Approach

### View-mode switch & dual cameras

- Add `#[derive(Resource)] enum ViewMode { Map2d, Relief3d }` (or a `bool in_3d`), toggled by
  a key (propose **`T`** for "tilt") and an egui button.
- Keep the existing `Camera2d`; add a `Camera3d`. Drive visibility by toggling each camera's
  `Camera.is_active` (and `Camera.order`) on `ViewMode` change — exactly one active at a time.
  The 2D sprites, the 2D hillshade quad, and the 3D terrain are separated with
  `RenderLayers` (2D content on layer 0, 3D terrain on layer 1) so the inactive view never
  bleeds through.
- ⚠️ **egui multi-camera gotcha** (already documented in It-0 plan 03): with two cameras the
  primary egui context must be pinned. Mark the camera that should own egui with
  `PrimaryEguiContext`, and keep egui UI systems on `EguiPrimaryContextPass` (not `Update`).
  Simplest: keep egui on the 2D camera and render the egui panel in both modes by leaving
  that camera's UI active; or move the marker with the active camera. Decide at impl time and
  note which.
- Pause `streaming` and `flythrough` systems while in `Relief3d` (run-condition on
  `ViewMode`) so they don't fight the hidden 2D camera. Tier 1 explicitly leaves those
  systems otherwise untouched.

### Coordinate mapping (fixed here, reused by Tier 2)

World is XY with +Y = north. Map to Bevy 3D as:

```
world (x, y, height_m)  →  3D (x, height_m * vexag, -y)
```

so the ground is the XZ plane, +Y is up, and **DEM UV is computed from the planar (x, −z)
exactly as `world_to_dem_uv`** — no new sampling convention. Document this mapping at the top
of `terrain3d.wgsl` the way `hillshade.wgsl` documents its UV flip.

### Terrain mesh

- A flat **subdivided plane** sized to `world_bbox` (16×16 km for Shackleton), centered at
  origin, in the XZ plane. Grid resolution is a constant — start at **512×512 quads**
  (~263 k verts; trivial for a static mesh) and tune. The mesh carries only position + planar
  UV; **height comes from the vertex shader** so no CPU heightmap read is needed.
- Generate with Bevy's mesh builders (a `Plane3d`/`Mesh::from` grid) or a small hand-rolled
  grid function. UVs map linearly to `[0,1]²` over the bbox.

### `Terrain3dMaterial` + `terrain3d.wgsl`

- `#[derive(Asset, AsBindGroup)] Terrain3dMaterial` implementing `Material` (from
  `bevy::pbr`), registered via `MaterialPlugin::<Terrain3dMaterial>`. Bindings:
  - `dem: Handle<Image>` (the same R16Uint PNG, `sample_type = "u_int"`, no sampler — as in
    `hillshade.rs`).
  - optional `imagery: Handle<Image>` (the zoom-0 tile, a single full-site texture) + sampler.
  - a uniform block mirroring `HillshadeParams` (dem world bbox, `elev_min/max`,
    `sun_azimuth/altitude`) **plus** `vexag: f32` and `surface_mode: u32`
    (0 = hillshade relief, 1 = draped imagery, 2 = height-ramp debug — reuse the existing
    ramp).
- **Vertex stage:** for each vertex, compute its planar world `(x, z)` → DEM UV → texel →
  `height_at` (port the `height_at` decode from `hillshade.wgsl`) and output
  `position.y = height * vexag`. `textureLoad` works in the vertex stage (no sampler needed).
- **Fragment stage:** reuse the It-0 lighting verbatim — finite-difference DEM normal
  (scaled by `vexag` so shading matches the exaggerated geometry) + sun unit vector +
  Lambertian dot. For `surface_mode = 1`, multiply the lit term by the sampled imagery color
  (cheap relit drape); for `surface_mode = 2`, return `height_ramp(t)`.
- **Vertical exaggeration & normals:** apply `vexag` to the heights used for *both* the
  vertex displacement and the finite-difference normal, so the relief shading stays
  consistent with the visible slopes. (Shackleton's ~4.65 km over 16 km is gentle; default
  `vexag = 2.5`, slider 1–5.)

> Reuse note: rather than copy the lighting block, consider `#import`-ing shared WGSL. If a
> shared include is awkward across the 2D/3D pipelines, duplicate the ~10-line
> normal+sun+dot snippet and add a comment cross-linking `hillshade.wgsl` so the two stay in
> sync (the math is identical; only the geometry source differs).

### Orbit camera controller

- New `Relief3dController { yaw, pitch, distance, target }` (target = site center). Drag
  rotates (yaw/pitch), wheel changes `distance`. Each frame, recompute the `Camera3d`
  `Transform` from spherical coords looking at `target`.
- Clamps: `pitch` in roughly `[5°, 85°]` (avoid gimbal flip and sub-horizon views),
  `distance` in `[~2 km, ~40 km]`, keep the eye above the highest terrain. A perspective
  `Projection::Perspective` with ~45–55° FOV; near/far sized to the 16 km site (e.g.,
  near 10 m, far 60 km).
- Seed the initial pitch at an oblique ~35–45° and yaw so the sun (from `HillshadeState`)
  rakes across the rim — the most legible framing.

### Sun & controls

- The material's sun uniform is fed from the existing `HillshadeState` angles (reuse
  `sync_uniforms`-style logic), so the egui sun sliders and the `G` sweep drive the 3D relief
  too. No second sun source in Tier 1 (real `DirectionalLight` + shadows arrive in Tier 2).
- egui additions (in `debug_ui.rs`, gated to `Relief3d`): view-mode toggle, vertical
  exaggeration slider, surface mode (relief / imagery / ramp), and a tilt/pitch readout.

### What Tier 1 deliberately does NOT do

- No change to `streaming.rs` (no tile draping — the zoom-0 single texture is the only
  imagery option in 3D here).
- No change to the 2D `camera.rs` / `flythrough.rs` logic (only gated off while in 3D).
- No `DirectionalLight` / shadow maps (Tier 2).

## Reuse (existing code/assets leaned on)

| Reused | From | Used for |
| --- | --- | --- |
| DEM decode `height_at`, normal+sun Lambertian | `assets/shaders/hillshade.wgsl` | terrain3d vertex + fragment |
| `height_ramp` debug | `assets/shaders/hillshade.wgsl` | `surface_mode = 2` |
| `HillshadeState` (sun angles, sweep, mode) | `crates/moon_game/src/hillshade.rs` | shared sun + UI |
| `world_to_dem_uv`, manifest bbox, `DemInfo` | `crates/moon_data` | mapping + uniforms |
| R16Uint texture binding (`sample_type="u_int"`) | `crates/moon_game/src/hillshade.rs` | DEM bind group |
| zoom-0 single tile | baked pyramid | optional drape texture |

## Risks & gotchas

- **Two-camera egui context** — pin `PrimaryEguiContext`; UI on `EguiPrimaryContextPass`
  (It-0 plan 03 hit this with one camera; two cameras make it mandatory).
- **Vertex-shader DEM sampling** must use `textureLoad` (integer texture, no filtering);
  nearest-texel heights can look faceted at 512² — bump grid res or do a 4-tap manual bilinear
  in the vertex shader if needest.
- **Exaggeration vs shading mismatch** — apply `vexag` to the normal calculation too, or lit
  slopes won't match the silhouette.
- **Far-plane / depth precision** over 16 km — keep near plane ≥ a few meters; fine for f32.
- **Mesh memory** — 512² static mesh is cheap; if pushing to 1024²+, generate once at startup.

## Verification / exit criterion

1. `cargo run -p moon_game`; press **`T`** → camera tilts into an oblique 3D relief of
   Shackleton; press again → returns to the unchanged 2D map.
2. Drag to orbit, wheel to zoom. Confirm craters are unambiguously **concave** from any
   azimuth (parallax + self-occlusion), regardless of sun angle — the illusion is gone.
3. Move the sun sliders / press `G`: shading tracks the sun; the *shape* stays correct.
4. Toggle surface mode: relief ↔ draped imagery ↔ height-ramp all render; vertical
   exaggeration slider visibly steepens/flattens.
5. The 2D view (sprites, streaming, flythrough, 2D hillshade) is **byte-for-byte unchanged**
   when in `Map2d`.

**Exit:** a tilt-able, orbitable 3D relief view that reads correct depth, behind a toggle,
with the existing 2D experience untouched.
