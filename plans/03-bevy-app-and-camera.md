# It-0 · Plan 03 — Bevy app & camera

## Context

The runtime half of It-0 begins here (see [`00-it0-overview.md`](00-it0-overview.md)). This
plan stands up the Bevy 0.18 app and gets the **camera feel** right — pan/zoom that feels
good is the backbone of the fly-over video, and it's worth nailing before tile-streaming
complexity lands on top. A debug egui overlay gives visibility into camera state for the
later plans.

**Dependencies:** plan 01 (loads `SiteManifest` for bounds/scale clamps). Plan 02 is now
baked, so the test sprite can be a real tile (`assets/tiles/shackleton/0/0_0.png` — zoom 0
is a single tile covering the whole site).

> **Verified against the built crates (2026-06):** bevy 0.18.1, bevy_egui 0.39.1 (egui
> 0.33). The camera API notes below are correct as written; the `bevy_egui` and manifest
> details have been corrected from the original sketch — see the ⚠️ notes.

## Scope

- `moon_game` boots as a Bevy 0.18 desktop app (windowed, vsync).
- One 2D orthographic camera with drag-pan and scroll-zoom.
- Manifest-driven clamps (don't pan off the void, don't zoom past texel density).
- `bevy_egui` debug overlay.

## Approach

### App bootstrap
- `App::new().add_plugins(DefaultPlugins).add_plugins(EguiPlugin::default())…`.
  ⚠️ **bevy_egui 0.39 API change:** add the plugin via `EguiPlugin::default()` (the
  `enable_multipass_for_primary_context` field is now deprecated). The default plugin runs
  egui in **multi-pass** mode, which means **egui UI systems must be added to the
  `EguiPrimaryContextPass` schedule, not `Update`** — a UI system scheduled on `Update`
  silently won't render. The primary egui context auto-attaches to the primary camera, so
  no extra setup is needed for our single `Camera2d` (a second camera would need the
  `PrimaryEguiContext` marker).
- Startup: load `assets/sites/shackleton.ron` with a small `std::fs` + `ron::from_str`
  system (same as `geo_pipeline check`). ⚠️ **`SiteManifest` is not a Bevy `Resource`** —
  `moon_data` is deliberately Bevy-free (plan 01), so wrap it: `#[derive(Resource)] struct
  Site(pub SiteManifest)` and insert `Res<Site>`. Spawn the camera. ⚠️ **Asset root
  (corrected during impl):** Bevy resolves `assets/` via `BEVY_ASSET_ROOT` →
  `CARGO_MANIFEST_DIR` → exe-dir — **never the cwd**, so "run from the workspace root" does
  *not* work in a workspace (`cargo run -p moon_game` sets `CARGO_MANIFEST_DIR` to
  `crates/moon_game`). Pin it instead via
  `AssetPlugin { file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets").into(), .. }`
  (and load the manifest from the same absolute path); then launch method / cwd don't matter.

### Camera (Bevy 0.18 specifics — verified correct against 0.18.1)
- **No `Camera2dBundle`.** Spawn the `Camera2d` marker component; required-components
  auto-insert `Transform`, `Projection`, etc.
- `OrthographicProjection` lives **inside the `Projection` enum** — insert
  `Projection::Orthographic(OrthographicProjection { .. })`; zoom by mutating its `scale`.
- Input via `Res<ButtonInput<MouseButton>>` (not the old `Input<T>`),
  `EventReader<MouseMotion>` and `EventReader<MouseWheel>` from `bevy::input::mouse`.
- **Scale ↔ meters:** `OrthographicProjection` defaults to `ScalingMode::WindowSize`, and
  our world is 1 unit = 1 m, so `projection.scale` **is** the screen meters-per-pixel
  directly (`screen_mpp == scale`). This makes the clamp/LOD math below exact.

### Controller
A `CameraController` resource: `target_scale`, `min_scale`, `max_scale`,
`drag_anchor: Option<(Vec2 cursor_world, Vec3 cam_translation)>`.

- **Pan (grab-the-world):** on drag-button held, pin the world point under the cursor —
  store it at drag start and each frame move `camera.translation` so that point stays under
  the cursor. Feels far better than raw pixel-delta panning.
- **Zoom-to-cursor:** scroll adjusts `target_scale *= (1 - wheel * zoom_speed)`; compute the
  world point under the cursor before and after applying the scale and shift
  `camera.translation` to keep it fixed. This is what makes zoom feel right.
- **Smoothing:** lerp actual `projection.scale` toward `target_scale` each frame with an
  exponential smooth (`cur += (target-cur)*(1 - exp(-k*dt))`); no snapping.
- **Clamps (from manifest):** `target_scale` ∈ `[min, max]` (max-out shows the whole site,
  max-in matches finest mpp). Clamp `camera.translation` so the visible AABB stays within
  `world_bbox` (+ small margin). Compute limits from `SiteManifest`, never hardcode. With
  the baked manifest these resolve to: `world_bbox = ±8000 m` (a **16×16 km** site, not 20),
  `min_scale = 5.0` (finest mpp, zoom 3), `max_scale ≈ 16000 / viewport_height_px` (whole
  site fits vertically). 4 zoom levels at 40/20/10/5 m/px.

### Debug overlay (egui)
A UI system **on the `EguiPrimaryContextPass` schedule** (see ⚠️ above) that reads
`EguiContexts`. ⚠️ **`EguiContexts::ctx_mut()` returns a `Result` in 0.39**, so the system
signature is `fn debug_ui(mut contexts: EguiContexts) -> Result` using `contexts.ctx_mut()?`.
Show camera world pos, `projection.scale` (= `screen_mpp`), the LOD zoom the streaming plan
will select, FPS, and (later) loaded-tile count. Leave hooks for plan 04/05 to add fields.

## Files to create

- `crates/moon_game/src/main.rs` — app, plugins, manifest load, system registration.
- `crates/moon_game/src/camera.rs` — `CameraController`, pan/zoom/clamp/smooth systems.
- `crates/moon_game/src/debug_ui.rs` — egui overlay.

## Verification

- `cargo run -p moon_game` (from anywhere, given the pinned `AssetPlugin.file_path` above)
  opens a window with a visible test sprite — load the real `tiles/shackleton/0/0_0.png`
  (whole site) rather than a synthetic marker. (Site04 is a permanently shadowed polar
  crater, so the zoom-0 tile shows the sunlit NAC rim with a black interior — that's real.)
- Drag-pan keeps the grabbed point under the cursor; scroll zooms toward the cursor;
  motion is smooth (no snapping/jitter); pan can't leave the world bounds; zoom clamps at
  both ends.
- egui overlay updates camera pos/scale/FPS live.
