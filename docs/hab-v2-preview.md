# Hab-v2 native preview

Implementation of domain plan `plans/hab-v2-game-integration.md`. The PR is an
experiment for founder review, not asset acceptance or permission to merge.

## Reproduce the actual game capture

Run from the game workspace root on a native GPU-capable desktop:

```sh
git lfs install --local
git lfs pull --include='assets/elevation/shackleton.png' --exclude=''
git lfs checkout assets/elevation/shackleton.png
cargo build --release --locked -p moon_game
MOON_SITE=shackleton MOON_PREVIEW=baseline target/release/moon_game
MOON_SITE=shackleton MOON_PREVIEW=hab target/release/moon_game
```

This opt-in fixture finds a genuinely buildable footprint using the ordinary
catalogue/terrain slope tests and places through the same `spawn_structure` path
as gameplay. Both cases choose the same footprint, camera, lighting and UI;
only `hab` spawns the model. It frames the full 14 m assembly at 36 m distance,
140° yaw and 15° pitch. The chosen position is (-2450, -6650) m, centre slope
0.03205°, rendered ground Y 1882.8164 m (shared 1.5× terrain exaggeration).
Buildability checks the centre plus eight points at the existing 8 m radius.

Native window resolution is 1280 × 960 physical pixels (scale factor 1),
`AutoNoVsync` presentation, with continuous updates even when unfocused. These
settings apply **only** when `MOON_PREVIEW` is present. Ordinary startup, input,
build menu, projection switching and primitive solar/battery presentation remain
available. The fixed catalogue-backed hab is the new habitat presentation.

After the scene is ready, 300 warm-up frames precede 600 measured frames. Bevy
captures the actual window into `dev-clips/hab-v2/hab.png` and `baseline.png`,
writes corresponding `*-timings.txt`, and exits. This directory is ignored by
Git; the images are not Blender renders. The build menu remains visible as
in-game context, while the debug panel is hidden for the capture.

## Verification

```sh
cargo test --workspace --release --locked
cargo clippy --workspace --release --locked --all-targets -- -D warnings
cargo fmt --all --check
rustfmt --edition 2024 --config skip_children=true --check \
  crates/moon_game/src/build.rs crates/moon_game/src/main.rs \
  crates/moon_game/src/selection.rs crates/moon_game/src/model_lighting.rs \
  crates/moon_game/src/preview.rs
git diff --check
python3 tools/prepare_hab_model.py --check /path/to/source/hab-v2.glb
```

Workspace tests include optional/legacy catalogue parsing, unchanged hab
gameplay values, primitive versus model ground lift under terrain exaggeration,
idempotent grounding, and nested scene-child owner resolution. The shared domain
GLB validator also passes the explicit experimental 14 m target and 8 m footprint.
See [model provenance](../assets/models/README.md) for hashes and contract exception.

Verified: release build **passes**; all **16 workspace tests pass**; release
Clippy with `-D warnings` **passes**; targeted rustfmt and `git diff --check`
**pass**; name-only/embedded-UV delivery check and shared GLB mechanical gate
**pass**. Workspace formatting was also tested against an isolated snapshot of
the original HEAD: both baseline and current workspace return exit code 1.
Ignored `baseline-format-check.txt` and `format-check.txt` retain that evidence.

The full-workspace formatter reports **pre-existing** style discrepancies in
moon_data, geo_pipeline and existing game source. Unrelated formatting was not
included in this PR. Changed implementation files pass targeted rustfmt; the
existing ground file preserves its surrounding style. The existing `block 0.1.6`
dependency produces a future-incompatibility warning. Bevy's existing Metal
egui bindless-texture fallback warning is non-fatal.

## Measured run

Final inspected captures were taken on 2026-10-03. Hardware:
Mac mini, Apple M4 (10-core CPU / 10-core integrated GPU), 16 GB RAM,
macOS 26.5.2, Metal backend. Values measure warmed frame wall time, not GPU-only
profiling or guaranteed performance on other hardware. No background build was
running during the final measurements.

| Run order | Case | Mean ms / fps | Median ms | p95 ms | p99 ms | Max ms |
|---|---|---|---|---|---|---|
| Pair 1, first | baseline | 10.0769 / 99.24 | 12.9516 | 13.8680 | 15.4767 | 26.8565 |
| Pair 1, second | hab | 4.6612 / 214.54 | 4.6236 | 5.9119 | 6.7276 | 9.4812 |
| Pair 2, first | hab | 4.8588 / 205.81 | 4.7228 | 6.9896 | 9.8615 | 10.6113 |
| Pair 2, second | baseline | 4.8649 / 205.56 | 4.7049 | 6.8888 | 9.5570 | 26.7701 |

All four runs use the same release executable and fixture settings. Pair 2
reversed order to assess variability. The first baseline showed substantial
system/window-pacing variance; these data do **not** mean adding geometry makes
the renderer faster. The close pair 2 is consistent with no measurable
asset-induced regression at this scale. Hab runs averaged 205.81–214.54 fps;
baseline averaged 99.24–205.56 fps. Hab p99 was 6.73–9.86 ms, while isolated
26.77–26.86 ms baseline hitches demonstrate that a strict **every-frame** 60 fps
floor is not proven. No asset-induced below-floor regression was observed;
larger bases, moving cameras, shadow rendering and other hardware remain untested.

Raw records are kept as `baseline-timings-1.txt`, `hab-timings-1.txt`,
`hab-timings-2.txt`, `baseline-timings-2.txt` in the ignored capture directory.
For repeat pairs, copy each `*-timings.txt` immediately after its command before
running that case again; the canonical filenames are deliberately overwritten.
The final `hab.png` was inspected: quilting, blue solar cells, gold railings,
ladder/footpads and full deployed silhouette are visible on genuine LOLA terrain.

Preserve screenshot attribution: NASA LOLA / Planetary Geodynamics Laboratory
(PGDA), NASA GSFC. NASA data does not imply NASA endorsement.
