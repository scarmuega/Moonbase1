# It-0 · Plan 05 — Hillshade & toggle

## Context

The last named It-0 feature (see [`00-it0-overview.md`](00-it0-overview.md)): an
**elevation-based hillshade toggle**. Computed at runtime in a shader from the real LOLA
DEM — not baked — so the sun angle is a live parameter. That makes a great video moment
(real terrain reacting to sun geometry) and directly seeds It-1's day/night illumination
overlay, while needing only the single heightmap PNG plan 02 already produces (no second
tile pyramid).

**Dependencies:** plan 01 (`world_to_dem_uv`, manifest), plan 02 (`assets/elevation/
shackleton.png` + elev min/max), plan 03 (app + manifest loaded).

## Scope

`moon_game/hillshade.rs` + `assets/shaders/hillshade.wgsl`: a `Material2d` that renders the
DEM as shaded relief, plus a toggle that swaps the imagery layer for the hillshade layer,
with egui sun-angle sliders.

## Approach

### Why runtime shader (not a baked hillshade tile layer)
- The **toggle + live sun** *is* the feature; baking fixes one sun angle and corners It-1.
- One small DEM texture vs. a whole parallel tile pyramid (less pipeline, disk, streaming).
- CPU recompute on every sun change would stutter; the shader does it per-fragment for free.

### Heightmap as a GPU texture (no custom loader)
- The 16-bit grayscale PNG loads through the stock `AssetServer`:
  `asset_server.load("elevation/shackleton.png")`. Bind it as **`R16Unorm`**, non-sRGB,
  sampler `linear`. Reconstruct meters in the shader: `elev = elev_min + sample*(elev_max-
  elev_min)` (min/max from the manifest as uniforms). **No CPU-side heightmap grid is needed
  for It-0** — the shader reads the GPU texture directly. (A CPU grid arrives in `moon_map`
  at It-1 for slope/buildability.)

### Material
`#[derive(AsBindGroup)] HillshadeMaterial { dem: Handle<Image>, sun_azimuth: f32,
sun_altitude: f32, mode: u32, dem_world_min: Vec2, dem_world_size: Vec2, mpp: f32,
elev_min: f32, elev_max: f32 }`. Implement `Material2d` (from `bevy::sprite`), register with
`Material2dPlugin::<HillshadeMaterial>`. Apply it to one full-site quad (`Mesh2d` rectangle
covering `world_bbox`) rendered above the imagery tiles.

### Shader (`hillshade.wgsl`)
- Map fragment world position → DEM UV using `dem_world_min`/`dem_world_size` (mirror
  `moon_data::world_to_dem_uv` exactly).
- Sample height at the fragment and at ±1 texel in x/y → gradient → **slope** and **aspect**
  (account for the height-to-horizontal ratio via `mpp` so slopes are physically correct).
- Standard hillshade:
  `illum = cos(zenith)*cos(slope) + sin(zenith)*sin(slope)*cos(azimuth - aspect)` where
  `zenith = 90° - sun_altitude`; clamp to `[0,1]`, output grayscale.

### Toggle + UI
- Key `H` and an egui checkbox flip the active layer (imagery tiles ↔ hillshade quad) — the
  simplest correct path is toggling the hillshade quad's `Visibility` (and optionally
  multiplying it over imagery for the best look, but a clean swap satisfies the spec).
- egui sliders for sun azimuth/altitude write the material uniforms live — the headline
  video interaction.

## Files to create

- `crates/moon_game/src/hillshade.rs` — `HillshadeMaterial`, plugin registration, quad
  spawn, toggle + slider systems.
- `assets/shaders/hillshade.wgsl` — the shading math.
- Extend `main.rs`/`debug_ui.rs` to wire the toggle + sliders.

## Verification

- Press `H` (or the checkbox): view swaps between raw imagery and shaded relief.
- The hillshade aligns with terrain features visible in the imagery (registration correct —
  same warped grid from plan 02).
- Dragging the sun-azimuth/altitude sliders moves shadows/lit faces plausibly in real time,
  with no stutter.
