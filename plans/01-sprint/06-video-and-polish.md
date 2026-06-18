# It-0 · Plan 06 — Video & polish

## Context

The deliverable that closes It-0 (see [`00-it0-overview.md`](00-it0-overview.md)): a
shareable video of flying over real lunar terrain in a from-scratch Rust engine, with the
LRO source imagery side-by-side. Per the roadmap, **no later iteration starts until this
video is posted** — the cadence is the accountability mechanism. This plan is the final
mile: confirm the performance bar, add the small touches that make the footage look good,
and capture it.

**Dependencies:** plans 02–05 (a working, streaming, hillshade-capable app over real data).

## Scope

Performance validation, visual polish, a cinematic capture path, and attribution — no new
systems.

## Approach

### 1. Hit the exit criterion (measure, don't assume)
- Profile in `--release`: continuous fast pan + zoom across the full ≥20×20 km site must
  hold ~60 fps. Use the egui FPS readout + Bevy diagnostics.
- If hitches appear, apply the plan-04 escape hatches in order: raise the prefetch/grace
  ring, lower the spawn-cap, then convert the finest zoom to **KTX2/BC7** (direct GPU
  upload, no decode). Re-measure.

### 2. Visual polish (cheap, high-impact for video)
- Subtle color grade / tint on the gray imagery for mood (the Moon is gray — a light grade
  reads as intentional, not fake).
- Tune the hillshade default sun angle to a flattering low azimuth that shows the rim/PSR
  relief.
- Clean the egui overlay into a minimal, screenshot-friendly HUD (toggle it off for the
  hero shots, on for the "from-scratch engine" tech-flex shots).

### 3. Cinematic capture
- Add a simple **scripted camera path** (keyframed pan/zoom over the most dramatic terrain —
  Shackleton rim into a PSR) playable on a hotkey, so the fly-over is smooth and repeatable
  rather than hand-flown.
- Capture with Bevy's screenshot API for stills and an external screen recorder for the
  clip. Keep a `dev-clips/` folder (standing practice: record 30 s after every working
  feature — trailer material compounds).
- Assemble the side-by-side: engine fly-over next to the source LRO frame — the LROC NAC
  South Pole mosaic (our basemap) and/or a QuickMap view of the same area.

### 4. Attribution & README
- Credit **"NASA/GSFC/Arizona State University"** (LROC NAC imagery) and LOLA/PGDA for the
  DEM in the video description and a repo `README`/`CREDITS`. If ShadowCam imagery is added
  for the in-shadow interior, also credit **"NASA/KARI/Arizona State University"**. Note NASA
  imagery must not imply NASA endorsement.
- README: the bake command (plan 02), run command, and data-source links so the build is
  reproducible.

## Files to create

- `crates/moon_game/src/flythrough.rs` — scripted camera path + hotkey (optional but
  recommended for a clean capture).
- `README.md` / `CREDITS.md` — run/bake instructions + attribution.
- `dev-clips/` — captured footage (gitignored or LFS as preferred).

## Verification

- `cargo run -p moon_game --release`: a full-site fly-over holds ~60 fps with the hillshade
  toggle working — the **It-0 exit criterion, demonstrated on camera**.
- A recorded clip exists showing real Shackleton terrain pan/zoom + hillshade sun-sweep,
  with the LRO source shown side-by-side, and correct attribution.
- README reproduces the build from `data/raw/` + the bake command.
