# `moon_game` — the Bevy runtime

The desktop client: camera, terrain rendering, input, and (eventually) the UI that drives the
simulation. Today it renders **real lunar south-pole terrain** in perspective 3D from the baked
DEM, with a live, sun-angle-adjustable relief shading — the visual foundation the game is built
on. Gameplay (landers, modules, the sim core) is not wired in yet; see the repo root
[`README.md`](../../README.md) for status.

## Run

The baked DEM ships in the repo via Git LFS (`git lfs install` once, then `git lfs pull`),
so you can run without re-baking:

```sh
cargo run -p moon_game --release                          # default: the pole-wide southpole site
MOON_SITE=shackleton cargo run -p moon_game --release     # the 5 m/px Shackleton close-up
```

(`--release` is needed to hold 60 fps; the asset path resolves from the workspace root
automatically, so the launch directory doesn't matter. `MOON_SITE` selects any baked
`assets/sites/<id>.ron`.)

Two baked sites are available:

| `MOON_SITE` | Area | Detail | Source DEM |
|---|---|---|---|
| `southpole` *(default)* | **120 × 120 km**, pole-wide (Shackleton, de Gerlache, Sverdrup, …) | 40 m/px | LOLA 20 m/px 80°S polar LDEM |
| `shackleton` | 16 × 16 km Shackleton rim | 5 m/px close-up | LOLA 5 m/px Site04 |

To re-bake these from raw NASA GeoTIFFs, see [`tools/geo_pipeline`](../../tools/geo_pipeline/README.md).

## Controls

| Input | Action |
|---|---|
| Left-drag | Orbit (rotate around the site) |
| Scroll / pinch | Dolly (zoom in/out) |
| `G` | Toggle the auto sun-sweep (rotates the relief-shading sun azimuth) |
| `F` | Play/stop the scripted cinematic fly-over |
| `F1` | Show/hide the HUD overlay |

The HUD shows live FPS, camera framing, and a sun azimuth/altitude control. The fly-over
keyframes in
[`src/flythrough.rs`](src/flythrough.rs) are **site-relative** (fractions of the world extent),
so one path reframes itself to whichever site is loaded — to re-author, just nudge the fractions
and replay with `F`.

## Terrain rendering (Sprint 02)

The DEM is rendered as **real 3D geometry** — a single GPU-displaced grid mesh (matched to the
DEM grid, capped at 2048², with a vertical-exaggeration uniform) shaded by a custom surface
shader (`assets/shaders/terrain.wgsl`) that computes the normal *per fragment* from DEM finite
differences for crisp relief, plus optional **synthetic sub-DEM relief** (fractal detail in the
lighting normal, footprint-faded) to fake resolution below the baked DEM. There is no streamed
imagery drape — the elevation model *is* the surface — over a warm regolith albedo. The sun is a
single live direction (`HillshadeState`) fed to the shader.

This superseded the It-0 2D path (a sprite tile pyramid streamed by viewport + a `Material2d`
hillshade quad). See `specs/03-architecture.md` and `plans/02-sprint/` for the full rationale.

### Source files

| File | Role |
|---|---|
| `src/main.rs` | App setup, camera/orbit rig, site loading |
| `src/terrain3d.rs` | DEM mesh build + displacement, surface material |
| `src/hillshade.rs` | Live sun direction + hillshade state |
| `src/flythrough.rs` | Scripted cinematic fly-over keyframes |
| `src/debug_ui.rs` | egui HUD overlay |

> **Bevy 0.18 notes.** The 16-bit DEM PNG decodes to `R16Uint` (sample with `textureLoad`, no
> sampler). The 3D material bind group is `@group(#{MATERIAL_BIND_GROUP})`, not `@group(2)`.
> egui needs one dedicated always-active `PrimaryEguiContext` camera.
