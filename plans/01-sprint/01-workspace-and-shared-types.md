# It-0 · Plan 01 — Workspace & shared types (`moon_data`)

## Context

First plan of It-0 (see [`00-it0-overview.md`](00-it0-overview.md)). The repo is greenfield
(only `specs/` + empty git). This plan lays the Cargo workspace and the one crate that both
the offline pipeline and the runtime game must agree on: `moon_data`. Keeping it tiny and
**Bevy-free** now means the pipeline (which writes the manifest) and the game (which reads
it) share a single typed contract from day one, and later crates (`moon_map`, `moon_sim`)
consume an existing vocabulary instead of forcing a refactor.

**Dependencies:** none. This is the foundation.

## Scope

- Root `Cargo.toml` workspace with pinned, shared dependency versions.
- Empty-but-compiling skeletons for the three It-0 units (`moon_data`, `moon_game`,
  `tools/geo_pipeline`) so `cargo build` works from commit one.
- `moon_data`: the shared types + pure coordinate transforms, with unit tests.
- `.gitignore` + Git LFS attributes.

## Approach

### Workspace root `Cargo.toml`
- `[workspace]` with `resolver = "3"` (edition 2024 / Rust 1.95 default), `members =
  ["crates/moon_data", "crates/moon_game", "tools/geo_pipeline"]`.
- `[workspace.dependencies]` pinning everything once, referenced by member crates via
  `dep.workspace = true`:
  - `bevy = "0.18"` (exact minor pin per standing practice), `bevy_egui` (the release that
    targets Bevy 0.18 — confirm at add time), `serde = { version = "1", features =
    ["derive"] }`, `ron = "0.8"`, `glam = "0.29"` (match Bevy's), `image = "0.25"`,
    `bytemuck = "1"`, `thiserror = "2"`, `tracing = "0.1"`.
- Add a release profile note: build the game with `--release` for the 60 fps target.

### `crates/moon_data` — the shared contract
Pure Rust, **no Bevy dependency** (only `serde`, `glam`, optionally `thiserror`). Public API:

- `SiteId(String)` (or a small enum starting with `Shackleton`) — names a site; maps to the
  asset subdirectory.
- `TileCoord { zoom: u8, x: u32, y: u32 }` — integer tile address. Helper `tile_path(site,
  coord) -> String` producing `tiles/<site>/<zoom>/<x>_<y>.png` (the exact on-disk layout
  the pipeline writes and the game loads — single source of truth for the path format).
- `SiteManifest` (serde, round-trips through RON) — the keystone metadata:
  - `site: SiteId`
  - `projected_bbox: [f64; 4]` (min_x, min_y, max_x, max_y in projected meters)
  - `world_bbox: [f32; 4]` (same extent recentred so origin = center)
  - `base_mpp: f64` (meters/pixel at zoom 0)
  - `tile_size_px: u32` (512)
  - `zoom_levels: Vec<ZoomLevel>` where `ZoomLevel { zoom, mpp, tiles_x, tiles_y }`
  - `dem: DemInfo { path, width, height, elev_min_m, elev_max_m }`
  - `sun: SunDefaults { azimuth_deg, altitude_deg }`
  - `crs_proj4: String` (the exact PROJ string used to warp — provenance + reproducibility)
- Pure transform functions (the heart of the crate; these are what the game's streaming +
  hillshade math call):
  - `world_per_pixel(manifest, zoom) -> f64`
  - `tile_world_size(manifest, zoom) -> f32` (= `tile_size_px * world_per_pixel`)
  - `world_to_tile(manifest, world_xy, zoom) -> TileCoord`
  - `tile_to_world_min(manifest, coord) -> glam::Vec2` (bottom-left corner in world space)
  - `world_to_dem_uv(manifest, world_xy) -> glam::Vec2` (for the hillshade shader, also
    mirrored in WGSL — keep the formula identical in both places).

### Skeletons
- `crates/moon_game/src/main.rs` — `fn main() {}` placeholder (fleshed out in plan 03).
- `tools/geo_pipeline/src/main.rs` — `fn main() {}` placeholder (fleshed out in plan 02).
- Each member's `Cargo.toml` references workspace deps it needs (`moon_game` adds Bevy +
  egui + `moon_data`; `geo_pipeline` adds `image`, `serde`, `ron`, `moon_data`).

### `.gitignore` + Git LFS
- `.gitignore`: `/target`, `/data/raw/`, `**/*.tif`, `.DS_Store`.
- `.gitattributes` (Git LFS): `assets/tiles/** filter=lfs diff=lfs merge=lfs -text` and the
  same for `assets/elevation/**`. (Run `git lfs install` once on the machine.)

## Files to create

- `Cargo.toml` (workspace root)
- `.gitignore`, `.gitattributes`
- `crates/moon_data/Cargo.toml`, `crates/moon_data/src/lib.rs`
- `crates/moon_game/Cargo.toml`, `crates/moon_game/src/main.rs` (placeholder)
- `tools/geo_pipeline/Cargo.toml`, `tools/geo_pipeline/src/main.rs` (placeholder)

## Verification

- `cargo build` — whole workspace compiles.
- `cargo test -p moon_data` — coordinate-transform round-trip tests pass, e.g.
  `world_to_tile` then `tile_to_world_min` lands within one tile of the original point;
  `world_per_pixel(z+1) == world_per_pixel(z)/2`; a `SiteManifest` round-trips through RON
  (`ron::to_string` → `ron::from_str` → equal).
- `cargo clippy --workspace` clean.
