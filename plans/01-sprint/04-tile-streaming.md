# It-0 · Plan 04 — Tile streaming

## Context

This is where the **exit criterion is met**: 60 fps pan/zoom over the full real terrain
site (see [`00-it0-overview.md`](00-it0-overview.md)). The baked pyramid is far too large to
load at once, so the game streams only the tiles the camera can see, at the right level of
detail, without frame hitches.

> Note: the overview's "≥20×20 km" target reflects the PGDA Site04 DEM, which bakes to a
> **16×16 km** patch — that's the full extent of the available 5 m/px source. The
> streaming + 60 fps requirement is what matters here; a larger footprint just means a
> larger DEM in a future bake, no code change.

**Dependencies:** plan 01 (`TileCoord`, transforms, `tile_path`), plan 02 (baked tiles +
manifest), plan 03 (camera + manifest already loaded).

> **Verified against the baked output (2026-06):** the real Site04 manifest has
> `max_zoom = 3`, `base_mpp = 40`, `tile_size_px = 512`, `world_bbox = ±8000 m`
> (**16×16 km**), 70 tiles total. Tile orientation is **correct in the bake**: tile
> `(x, 0)` is the south strip and `y` grows north (Bevy Y-up), so placement via
> `tile_to_world_min` reconstructs the north-up raster and agrees with the DEM/hillshade —
> no per-sprite flip needed. The manifest is held as a `Res<Site>` newtype (not
> `Res<SiteManifest>` — see plan 03). The Shackleton interior bakes black (a permanently
> shadowed region with no NAC coverage); that's real, not a missing/failed tile — the
> in-shadow relief is shown via the DEM hillshade (plan 05), not imagery.

## Scope

`moon_game/streaming.rs`: load/unload tiles by camera viewport, select the LOD zoom from
camera scale, and keep it hitch-free.

## Approach

### State
`#[derive(Resource, Default)] struct LoadedTiles { map: HashMap<TileCoord, Entity> }`. Each
tile entity = `Sprite::from_image(handle)` with `custom_size: Some(splat(tile_world_size(z)))`
at `Transform` placing it at `tile_to_world_min + half_tile`, `translation.z = zoom`.

### LOD selection (one active zoom at a time for It-0)
- `screen_mpp = ortho_world_height / window_height_px` — with `ScalingMode::WindowSize` and
  1 unit = 1 m this equals `projection.scale` directly (see plan 03).
- `desired_zoom = clamp(round(log2(base_mpp / screen_mpp)), 0, max_zoom)` — picks the zoom
  whose texel ≈ 1 screen pixel. With real numbers (`base_mpp = 40`, `max_zoom = 3`):
  `screen_mpp = 40 → zoom 0`, `= 5 → zoom 3`. Use `max_zoom = manifest.zoom_levels.len()-1`,
  never a literal.
- **Hysteresis:** only switch zoom when `screen_mpp` crosses a boundary by ~±15%, so a tiny
  scroll near a boundary doesn't thrash load/unload. No cross-fade between LODs — the pop is
  acceptable for It-0 (cross-fade is later polish).

### Visible set + diff (throttled, not every frame)
Run on a `Timer` (e.g. ~100 ms) or only when the camera moved/zoomed past a threshold:
1. Camera world AABB from `Transform.translation` + ortho half-extents.
2. Inflate by a **1-tile prefetch ring** so tiles load before they scroll on-screen.
3. Convert AABB → tile-coord rectangle at `desired_zoom` via `world_to_tile`; clamp to the
   manifest's `tiles_x/tiles_y` (never request off-grid tiles) → the **desired set**.
4. Diff vs `LoadedTiles.map`:
   - `to_spawn = desired − loaded`: spawn `Sprite::from_image(asset_server.load(path))` —
     PNG decode happens **off the main thread** on Bevy's async IO pool; Bevy renders the
     sprite once the image is ready (no manual placeholder needed with a good prefetch ring).
   - `to_despawn = loaded − desired`, but keep a **grace ring** (don't despawn immediately)
     so oscillating pans reuse resident tiles. `commands.entity(e).despawn()` and drop the
     `Handle<Image>` to free VRAM once refcount hits zero.

### Hitch avoidance (the 60 fps work)
- **Throttle** the streaming pass (above) — panning produces far fewer set-changes than 60
  evals/sec.
- **Cap spawns per pass** (e.g. K=4) via a small queue drained across passes, so a fast
  zoom-out can't queue 100 loads in one frame.
- Off-thread decode is free; main-thread cost is only the entity spawn + GPU upload — the
  spawn-cap bounds uploads/frame.
- **Optional:** ship the finest zoom as **KTX2/BC7** (uploads directly, no CPU decode, ~4×
  less VRAM) — reach for this only if `--release` profiling shows upload hitches; PNG
  everywhere is likely fine at this tile count.

## Files to create

- `crates/moon_game/src/streaming.rs` — `LoadedTiles`, LOD selection, visible-set diff,
  spawn/despawn systems, spawn-cap queue.
- Wire the systems + resource into `main.rs`; add loaded-tile count to the egui overlay.

## Verification

- Launch and pan across the full 16×16 km site: tiles appear ahead of the viewport (no
  visible gaps with the prefetch ring), unload behind it. North is up (south row = tile
  `y=0`); verify the map isn't vertically mirrored against the hillshade in plan 05.
- Zoom in/out crosses LOD levels cleanly; the hysteresis prevents flicker at boundaries.
- **FPS holds ~60 in `--release`** during a continuous fast pan + zoom across the whole
  site (the exit criterion). egui shows a bounded loaded-tile count (no unbounded growth).
