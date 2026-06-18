# It-0 — "Real Moon, my engine" · Overview

> Index + shared context for the first roadmap iteration. The detailed work lives in the
> six sibling plans listed below; each is self-contained and can be executed on its own.

## Goal

Turn **real** Lunar Reconnaissance Orbiter (LRO) imagery + elevation of the **Shackleton
crater rim** (lunar south pole) into a baked tile pyramid, and render it in a from-scratch
**Bevy 0.18** app with smooth pan/zoom and an elevation-based hillshade toggle.

This is the foundation everything else sits on, and it proves the project's #1 design
pillar — *real ground* — before any game systems exist.

**Exit criterion:** 60 fps pan/zoom over **≥20×20 km** of real Shackleton terrain, with a
working hillshade toggle.

**Video moment:** flying over real lunar terrain in a from-scratch Rust engine, with the
LRO source imagery shown side-by-side. (Per the roadmap, no later iteration starts until
this video is posted.)

## Verified environment facts

- `cargo`/`rustc` 1.95.0 present. **GDAL is NOT installed** — `brew install gdal` is a
  prerequisite for the pipeline (build-time only; never linked into the shipped game).
- **Bevy 0.18** released 2026-01-13. Pin it (spec says "0.18+"; standing practice = pin
  per iteration, upgrade only at iteration boundaries).
- Elevation source: **PGDA "LOLA_5mpp" south-pole DEMs** (product 78) — 5 m/px, 16-bit
  GeoTIFF, south polar stereographic, MOON_ME / DE421. Covers Shackleton (`Site04` =
  Shackleton rim). Files under `https://pgda.gsfc.nasa.gov/data/LOLA_5mpp/Site04/`.
- Imagery (chosen): **LROC NAC South Pole PSR mosaic** ~11.7 m/px, polar-stereographic,
  grayscale (`NAC_POLE_PSR_SOUTH_STRETCH`). Alternatives: 1 m/px NAC "Avg Merge" S-Pole
  mosaic; LROC WAC 100 m/px global mosaic (coarse, seams at the pole — original bootstrap);
  ShadowCam/Danuri (images *inside* permanent shadow); NASA CGI Moon Kit (not georeferenced).
- Attribution required on published imagery: **"NASA/GSFC/Arizona State University"**
  (LROC). NASA imagery must not imply NASA endorsement.

## Decisions baked in (locked for It-0)

1. **Lean crate layout** — build only `tools/geo_pipeline`, `crates/moon_data`,
   `crates/moon_game`. Defer `moon_map` (It-1), `moon_sim` (It-2), and `xtask` (later)
   until they hold real code. Tile-streaming logic is inherently Bevy-coupled, so it lives
   in `moon_game` for now, not a separate `moon_map`.
2. **Imagery: NAC South Pole mosaic** — the WAC-bootstrap step is **done**; we swapped to
   the LROC NAC South Pole mosaic (polar-stereographic, ~10× finer, seam-free). The sunlit
   rim/walls render as real terrain; the permanently shadowed interior has no NAC coverage
   and bakes black — the in-shadow relief comes from the DEM **hillshade** (plan 05), not
   imagery. Elevation/hillshade uses the 5 m LOLA DEM regardless.
3. **Git LFS** for baked assets (`assets/tiles/**`, `assets/elevation/**`; one site ≈
   300–600 MB). `data/raw/` (multi-GB source GeoTIFFs) is gitignored; the bake command is
   documented so assets are reproducible.

## Coordinate conventions (shared across all sub-plans)

- **Geo:** projected meters, south polar stereographic centered on Shackleton (GDAL output).
- **World:** 1 Bevy unit = 1 meter; world origin at the site-bbox center (float precision).
- **Tile:** integer `(zoom, x, y)`; zoom increases = finer detail = smaller footprint;
  `world_per_pixel(z) = base_mpp / 2^z`; tiles are 512×512 px.
- **Single source of truth:** `assets/sites/shackleton.ron` (the manifest) records the
  projected + world bbox, `base_mpp`, `tile_size_px`, per-zoom grid extents, DEM dims, and
  elevation min/max. Nothing geometric is hardcoded in the game.

## Sub-plans (build order)

Front-load the risky data path; the Bevy half can start as soon as **02** emits any tiles
(or immediately on CGI-Moon-Kit bootstrap tiles).

1. [`01-workspace-and-shared-types.md`](01-workspace-and-shared-types.md) — workspace +
   `moon_data` (the shared contract; pure, unit-tested). *No deps.*
2. [`02-geo-pipeline.md`](02-geo-pipeline.md) — bake real Shackleton tiles + heightmap +
   manifest. *Deps: 01.* **Highest uncertainty — do early.**
3. [`03-bevy-app-and-camera.md`](03-bevy-app-and-camera.md) — app boots, camera pan/zoom
   feel, egui debug overlay. *Deps: 01.*
4. [`04-tile-streaming.md`](04-tile-streaming.md) — stream the pyramid by viewport at
   60 fps. *Deps: 01, 02, 03.*
5. [`05-hillshade-and-toggle.md`](05-hillshade-and-toggle.md) — runtime hillshade shader +
   toggle + sun sliders. *Deps: 01, 02, 03.*
6. [`06-video-and-polish.md`](06-video-and-polish.md) — profile, polish, record the
   shareable fly-over. *Deps: 02–05.*

## Cross-cutting risks & mitigations

1. **GDAL data wrangling (biggest unknown).** Lunar polar-stereographic CRS needs an
   explicit moon-radius PROJ string (not an EPSG code); LOLA/LROC grid alignment + nodata.
   → Tackle first (plan 02), smallest bbox, verify with `gdalinfo`; bootstrap the Bevy half
   from the CGI Moon Kit so engine work is never blocked on the mosaic.
2. **Streaming hitches break 60 fps.** → throttle, spawn-cap, prefetch + grace rings;
   profile in `--release` early (plans 04, 06); optionally KTX2/BC7 for the finest layer.
3. **Bevy 0.18 API churn + dev new to Bevy** (`Camera2dBundle`/`SpriteBundle` removed,
   `Projection` enum, `Material2d` boilerplate, bevy_egui version skew). → pin exact
   versions, follow matching-release examples (notes in plans 03–05).
4. **DEM↔imagery registration** (hillshade must line up with terrain). → warp both to the
   *same* `-t_srs`/`-te`/grid; one shared world bbox in the manifest; the shader maps
   world→DEM-UV from manifest values, so registration is data-driven, not eyeballed.

## Final-state file tree (after all six plans)

```
mooncraft/
├─ Cargo.toml                     # workspace
├─ crates/
│  ├─ moon_data/                  # shared types + transforms (plan 01)
│  └─ moon_game/                  # Bevy app (plans 03–06)
│     └─ src/{main,streaming,hillshade}.rs
├─ tools/geo_pipeline/            # offline GDAL bake (plan 02)
├─ assets/
│  ├─ tiles/shackleton/<z>/<x>_<y>.png
│  ├─ elevation/shackleton.png    # 16-bit grayscale heightmap
│  ├─ sites/shackleton.ron        # manifest
│  └─ shaders/hillshade.wgsl
└─ data/raw/                      # gitignored source GeoTIFFs
```
