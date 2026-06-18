# Sprint 02 — Tilted 3D terrain · Overview

> Index + shared context for the second roadmap iteration. The detailed work lives in the
> two sibling plans below; each is self-contained. This sprint follows It-0
> ([`../01-sprint/00-it0-overview.md`](../01-sprint/00-it0-overview.md)).

## Goal

Render the real lunar elevation as **actual 3D geometry** under a **tilt-able perspective
camera**, so depth reads correctly from parallax and occlusion instead of from shaded relief
alone.

### Why now (the motivating bug)

It-0 renders everything in Bevy's **2D pipeline** — `Camera2d` + `OrthographicProjection`,
imagery as `Sprite`s, the hillshade as a `Material2d` on a flat quad. A top-down shaded
relief is subject to the **relief-inversion illusion**: with the light low and from the
"bottom" of the screen, craters perceptually pop up as mounds. We already (Sprint 01 follow-up)
fixed the hillshade lighting math and a 90° azimuth-convention bug in
[`assets/shaders/hillshade.wgsl`](../../assets/shaders/hillshade.wgsl) and added a debug
**height-ramp** mode (`J`) — which *confirmed the elevation data is correct*; the inversion
is purely perceptual and subjective. True perspective + real geometry is the only robust
fix: parallax and self-occlusion disambiguate concave-vs-convex unconditionally.

## The two tiers

This sprint is deliberately split so the cheap, zero-risk win can land and be evaluated
before committing to the large rework.

- **[Plan 01 — Tier 1: 3D relief view (additive mode)](01-tier1-3d-relief-view.md).** A new
  `Camera3d` + a DEM-displaced terrain mesh, shown as an *optional view mode* toggled from
  the existing app. Reuses the It-0 hillshade lighting math as a 3D material; optionally
  drapes the single zoom-0 imagery texture. **Does not touch** the tile-streaming system or
  the existing 2D map view. Focused, ~1 new module + a 3D material/shader. **Exit:** a
  tilt-able, orbitable 3D relief of Shackleton; illusion gone in that view; 2D view
  unchanged.

- **[Plan 02 — Tier 2: 3D as the primary view (full integration)](02-tier2-3d-primary-view.md).**
  Promote 3D to the main experience: migrate the camera/flythrough/streaming/debug-UI off
  `Camera2d`/orthographic onto a perspective 3D camera, and **drape the streamed LOD imagery
  tiles onto the terrain** as displaced textured patches. Adds a real `DirectionalLight` sun
  with **cascaded shadow maps** — physical crater self-shadowing, which also seeds the
  roadmap's day/night illumination work. Multi-day; touches all six `moon_game` modules.
  **Exit:** 60 fps fly-over of real Shackleton terrain in perspective 3D with sun-cast
  shadows.

```
Tier 1 (additive, low risk)  ──►  evaluate  ──►  Tier 2 (primary, full rework)
```

Tier 2 **depends on Tier 1**: it reuses Tier 1's coordinate mapping, DEM-displacement vertex
shader, and terrain material. Do not start Tier 2 until Tier 1 is merged and the 3D feel is
validated.

## Verified codebase facts (carried into both plans)

- **Stack:** Bevy 0.18.1, `bevy_egui` 0.39.1 (egui 0.33). Confirmed against the It-0 build
  ([`../01-sprint/03-bevy-app-and-camera.md`](../01-sprint/03-bevy-app-and-camera.md)).
- **One camera today:** `Camera2d` spawned at [`crates/moon_game/src/main.rs:97`]. The
  query `With<Camera2d>` + `Projection::Orthographic` is assumed in **four** modules:
  `camera.rs`, `flythrough.rs`, `streaming.rs`, `debug_ui.rs`.
- **World space:** 1 unit = 1 m, XY plane, **+Y = north**, origin at bbox center
  (`moon_data`). `world_bbox = ±8000 m` (Shackleton = **16×16 km**).
- **DEM:** `assets/elevation/shackleton.png`, 3200×3200 @ 5 m/px, R16Uint, decoded
  `elev = elev_min + s*(elev_max-elev_min)`; Shackleton range **−2847.877 .. 1805.073 m**
  (~4.65 km of relief over 16 km → fairly flat; needs vertical exaggeration to read in 3D).
  `world_to_dem_uv` (in `moon_data`) is mirrored byte-for-byte in `hillshade.wgsl`.
- **Hillshade (reusable):** `HillshadeMaterial`/`HillshadeState` in
  `crates/moon_game/src/hillshade.rs`; the WGSL computes a surface normal from DEM finite
  differences and a sun unit vector, then a Lambertian dot product — **this math ports
  directly to 3D**. `HillshadeState` already carries `sun_azimuth_deg`, `sun_altitude_deg`,
  `sweeping`, and a `mode` (0 = relief, 1 = height-ramp debug).
- **Tiles:** baked pyramid `max_zoom = 3`, `tile_size_px = 512`, 70 tiles; zoom-0 is a
  **single tile covering the whole site** (handy as a one-shot drape texture for Tier 1).
- **Asset root** resolves via absolute `CARGO_MANIFEST_DIR`, not cwd.

## Decisions baked in (locked for Sprint 02)

1. **3D lives in Bevy's PBR/3D pipeline** (`Camera3d`, `Mesh3d`, `Material` from
   `bevy::pbr`). The 2D and 3D pipelines do not interoperate; "tilting" the 2D camera is not
   an option (`Mesh2d` z is draw-order only, no displaced geometry, no perspective depth).
2. **Vertical exaggeration is a first-class uniform** (default ~2.5×), applied consistently
   to geometry *and* the normals used for shading.
3. **Coordinate mapping is fixed once** (Tier 1) and reused: world `(x, y)` → 3D
   `(x, height, −y)` so the 3D ground plane is XZ, +Y is up, and DEM UV math is unchanged.
4. **Tier 1 is additive and reversible** — the height-ramp/`mode` plumbing and the existing
   2D view stay; 3D is gated behind a view-mode toggle.
5. **Sun stays a single live parameter** (`HillshadeState` angles) shared by the 2D
   hillshade, the Tier 1 material, and the Tier 2 `DirectionalLight`.

## Out of scope for this sprint

- Game systems (buildings, sim) — unchanged.
- New data bakes / additional sites (the `southpole` 40 m/px manifest already works with
  both tiers as-is).
- Atmosphere (there is none on the Moon — black sky; an optional starfield is a polish note,
  not a deliverable).
