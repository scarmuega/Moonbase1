# Sprint 02 · Plan 02 — Tier 2: 3D as the primary view (full integration)

## Context

Second plan of Sprint 02 (see [`00-overview.md`](00-overview.md)). Tier 1
([`01-tier1-3d-relief-view.md`](01-tier1-3d-relief-view.md)) proved the 3D relief as an
*optional mode*. Tier 2 **promotes 3D to the primary experience**: the perspective camera
becomes *the* camera, the streamed LOD imagery is **draped onto the terrain**, and a real
`DirectionalLight` sun casts **shadows** — which is the *physical* (not perceptual) fix to
the original depth-reading problem, and directly seeds the roadmap's day/night illumination
work (real PSR shadows at the lunar south pole are the whole point of the site).

This is the large rework: it migrates the **four** modules that assume
`Camera2d`/orthographic and rewrites tile streaming to produce 3D geometry instead of 2D
sprites. It is **multi-day** and should only start once Tier 1 is merged and the 3D feel is
signed off.

**Dependencies:** Tier 1 (coordinate mapping, DEM-displacement vertex shader,
`Terrain3dMaterial`, orbit/perspective camera). It-0 (baked tile pyramid + manifest +
streaming logic to port).

## Scope

Promote the whole runtime to 3D:

- Migrate `camera.rs`, `flythrough.rs`, `streaming.rs`, `debug_ui.rs` off
  `Camera2d`/`Projection::Orthographic`.
- Replace `Sprite` tiles with **DEM-displaced, image-textured terrain patches**; rework LOD
  selection for a perspective frustum.
- Add a `DirectionalLight` sun with **cascaded shadow maps**.
- Port the cinematic flythrough to a 3D camera path.
- Retire (or fold in) the Tier-1 view-mode toggle so 3D is the default.

## Approach

### 1. Camera migration (Camera2d → Camera3d, perspective)

- Replace the single `Camera2d` (`main.rs:97`) with a `Camera3d` + `Projection::Perspective`.
  Promote Tier 1's `Relief3dController` into the main `CameraController`, supporting both an
  **orbit** mode (inspect) and a **fly** mode (WASD/look for the video).
- Update every `With<Camera2d>` + `Projection::Orthographic` query — in `camera.rs`,
  `flythrough.rs`, `streaming.rs`, `debug_ui.rs` — to the 3D camera. Zoom semantics change
  from `OrthographicProjection::scale` to **camera distance / FOV**; the clamp logic in
  `camera.rs::clamp_view` becomes 3D bounds (orbit pitch limits, distance clamp, keep the eye
  above terrain, don't fly outside the bbox).
- `debug_ui.rs` readouts switch from ortho scale → camera height / ground-sample-distance at
  screen center.

### 2. Terrain as displaced, textured patches (the imagery drape)

The core rework. Two viable structures — recommend starting with **per-tile patches** since
it reuses the existing pyramid directly:

- **Per-tile displaced patch:** each streamed tile becomes a subdivided grid mesh over the
  tile's world bbox, displaced by the DEM in Tier 1's vertex shader, textured with the tile
  image. Replaces `Sprite { … } at z = zoom` in `streaming.rs:186` with
  `Mesh3d` + a tile material (`Terrain3dMaterial` variant carrying the tile texture).
- **Alternative (later, if perf/seams demand):** a single geometry **clipmap / quadtree**
  terrain with imagery fed as a streamed surface texture (virtual-texture style). More work,
  better seams and LOD continuity. Note as an escape hatch, don't build first.

**Seam handling:** adjacent patches must agree on shared-edge heights — guaranteed if every
patch samples the *same* DEM via `world_to_dem_uv` (no per-patch normalization). Add small
**vertical skirts** at patch edges to hide hairline cracks from LOD boundaries.

### 3. Streaming rework (ortho-viewport LOD → frustum + screen-space error)

- `streaming.rs` today selects one zoom from `OrthographicProjection::scale` and loads tiles
  in the visible AABB. Replace with:
  - **Frustum culling** against the 3D camera (load only tiles whose bbox intersects the
    view frustum, with a prefetch ring).
  - **Screen-space-error LOD:** choose each tile's zoom from its distance to the camera /
    projected texel size, so near terrain is high-res and the horizon is coarse — multiple
    active zooms at once (It-0 used a single active zoom).
  - Patch pooling/unloading by distance, hitch-free (reuse the It-0 spawn-cap / grace-ring
    discipline from [`../01-sprint/04-tile-streaming.md`](../01-sprint/04-tile-streaming.md)).

### 4. Sun & shadows (the physical depth fix)

- Add a `DirectionalLight` whose direction is derived from `HillshadeState`'s
  `sun_azimuth/altitude` (same single sun parameter as 2D and Tier 1). Enable **cascaded
  shadow maps**.
- This gives real crater **self-shadowing** — depth is now disambiguated by cast shadows and
  occlusion, not shading guesswork. At the south pole the sun is near-horizon, so shadows are
  long and dramatic (and physically correct — this is the PSR story).
- ⚠️ Grazing-angle **shadow acne / peter-panning**: tune shadow bias, cascade count, and
  depth range for the near-horizon sun. Keep the option to also apply the analytic hillshade
  term as a soft fill so shadowed faces aren't pure black (matching the real PSR look).
- The Tier-1 analytic hillshade material remains available as a **surface-shading toggle**
  (relief vs imagery), now layered with real shadows.

### 5. Cinematic flythrough (port to 3D)

- `flythrough.rs` keyframes pan/zoom of an ortho camera; port to a 3D path: keyframed
  eye position + look-at (+ optional roll/tilt), eased. The dramatic shot becomes a banking
  descent over the Shackleton rim into a shadowed PSR — far stronger footage than the 2D
  fly-over.

### 6. Retire the Tier-1 toggle

- With 3D primary, fold the `ViewMode` switch: either drop the 2D path, or keep a top-down
  orthographic "map" toggle as a convenience (cheap to retain since the 2D camera code
  already exists). Decide based on whether the flat map still earns its keep.

## Performance plan (60 fps target)

- Frustum cull + distance LOD + patch pooling are the primary levers.
- Escape hatches, in order: raise prefetch/grace ring; cap patch grid resolution; switch the
  finest tiles to **KTX2/BC7** (direct GPU upload, no decode — already an It-0 escape hatch);
  add edge skirts to allow coarser LOD; if still short, move to the clipmap/quadtree terrain
  (§2 alternative).
- **Precision:** 16 km fits f32 comfortably. If future sites grow past ~50–100 km, adopt a
  camera-relative origin (floating-origin) — out of scope for Shackleton/southpole.

## Reuse

| Reused | From | Used for |
| --- | --- | --- |
| Coord mapping, DEM-displacement vertex shader, `Terrain3dMaterial`, perspective controller | Sprint 02 Tier 1 | terrain patches + camera |
| Tile pyramid, manifest, `tile_to_world_min`, spawn-cap/grace-ring discipline | It-0 plans 02 & 04 | streaming rework |
| `HillshadeState` sun parameter | `crates/moon_game/src/hillshade.rs` | `DirectionalLight` + hillshade fill |
| `moon_data` transforms / bbox | `crates/moon_data` | culling, placement, seams |

## Risks & gotchas

- **Streaming rewrite is the bulk of the risk** — landing hitch-free multi-zoom LOD in 3D is
  harder than It-0's single-zoom 2D streamer. Budget accordingly; keep the per-tile-patch
  approach (not the clipmap) for the first cut.
- **LOD popping & cracks** at patch boundaries — skirts + consistent DEM sampling; consider
  geomorphing if popping is visible.
- **Grazing-sun shadows** — acne/bias tuning is fiddly at near-horizon altitude.
- **Two-pipeline cleanup** — once 3D is primary, prune dead 2D-only assumptions to avoid a
  confusing half-migrated state; keep the 2D "map" only if deliberately retained.
- **egui context** — single 3D primary camera simplifies this back to the It-0 single-camera
  setup (pin `PrimaryEguiContext` on the 3D camera).

## Verification / exit criterion

1. `cargo run -p moon_game` boots **directly** into perspective 3D over real Shackleton
   terrain with draped imagery.
2. Fast fly/orbit across the full **16×16 km** site holds **~60 fps** in `--release` (egui FPS
   + Bevy diagnostics), with hitch-free multi-zoom streaming.
3. The `DirectionalLight` sun casts shadows; moving the sun (sliders / `G` sweep) sweeps the
   shadows correctly. Crater depth reads correctly from shadows + occlusion at any sun angle.
4. The cinematic flythrough plays a smooth 3D descent over the rim into a shadowed PSR.
5. No seams/cracks/popping visible during normal fly-over.

**Exit:** 60 fps perspective fly-over of real lunar terrain with sun-cast shadows — the depth
illusion is gone physically, and the result is a stronger video than the It-0 2D fly-over.
