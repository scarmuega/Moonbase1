# Mooncraft

A from-scratch Rust + [Bevy](https://bevyengine.org) engine that flies over **real lunar
terrain** at the Moon's south pole — rendered from NASA LRO imagery and elevation. Pan and
zoom a streaming tile pyramid of the LROC NAC South Pole mosaic, then toggle a live,
sun-angle-adjustable **hillshade** computed from the LOLA DEM to reveal the relief inside the
permanently shadowed regions that imagery can't show.

This is iteration It-0. Two baked sites are available:

| `MOON_SITE` | Area | Detail | Source DEM |
|---|---|---|---|
| `southpole` *(default)* | **120 × 120 km**, pole-wide (Shackleton, de Gerlache, Sverdrup, …) | 40 m/px | LOLA 20 m/px 80°S polar LDEM |
| `shackleton` | 16 × 16 km Shackleton rim | 5 m/px close-up | LOLA 5 m/px Site04 |

## Run

The baked assets ship in the repo via Git LFS (`git lfs install` once, then `git lfs pull`),
so you can run without re-baking:

```sh
cargo run -p moon_game --release                 # default: the pole-wide southpole site
MOON_SITE=shackleton cargo run -p moon_game --release   # the 5 m/px Shackleton close-up
```

(`--release` is needed to hold 60 fps; the asset path resolves from the workspace root
automatically, so the launch directory doesn't matter. `MOON_SITE` selects any baked
`assets/sites/<id>.ron`.)

### Controls

| Input | Action |
|---|---|
| Left-drag | Pan (grab the world) |
| Scroll / pinch | Zoom to cursor |
| `H` | Toggle imagery ↔ DEM hillshade |
| `G` | Toggle the auto sun-sweep (rotates the hillshade sun azimuth) |
| `F` | Play/stop the scripted cinematic fly-over |
| `F1` | Show/hide the HUD overlay |

The HUD shows live FPS, camera position (m), scale (m/px), active LOD, and tile count, plus
sliders for the sun azimuth/altitude. The fly-over keyframes in
[`crates/moon_game/src/flythrough.rs`](crates/moon_game/src/flythrough.rs) are **site-relative**
(fractions of the world extent), so one path reframes itself to whichever site is loaded — to
re-author, just nudge the fractions and replay with `F`.

## Reproduce the build from raw data

The committed assets are produced offline by the `geo_pipeline` bake from raw NASA
GeoTIFFs (`brew install gdal` first — the bake shells out to GDAL). Each site auto-derives
its grid (CRS, extent, finest m/px) from its DEM and warps the imagery onto that grid so the
two register; it writes `assets/{tiles,elevation,sites}/<site>.*`.

**Pole-wide `southpole` site** (the default flyover). The 120 km box is cropped from the
LOLA 20 m/px polar LDEM at its 40 m overview — the full-res tile index of that COG is flaky
over `/vsicurl`, but the overview reads reliably and fast:

```sh
scripts/fetch_data.sh --region                                   # → data/raw_southpole (~45 MB)
cargo run -p geo_pipeline -- bake --site southpole --raw data/raw_southpole --out assets
cargo run -p geo_pipeline -- check assets/sites/southpole.ron
```

`--region-km <km>` / `--region-mpp <m>` resize the crop (e.g. `--region-km 160`). At 40 m/px
the hillshade DEM stays one modest GPU texture; finer/larger crops grow it.

**Shackleton 5 m/px close-up:**

```sh
scripts/fetch_data.sh --with-imagery                             # DEM (~41 MB) + NAC crop (~3 MB)
cargo run -p geo_pipeline -- bake --site shackleton --raw data/raw --out assets
```

See [`data/README.md`](data/README.md) for full provenance and how to swap imagery sources.

## Data sources

- **Regional DEM (`southpole`):** PGDA LOLA 20 m/px south-polar LDEM (80°S), *A New View of
  the Lunar South Pole from LOLA* — <https://pgda.gsfc.nasa.gov/products/90>.
- **Close-up DEM (`shackleton`):** PGDA LOLA 5 m/px, Site04 (Shackleton rim) —
  <https://pgda.gsfc.nasa.gov/products/78>. Barker et al. (2021), *Improved LOLA Elevation
  Maps for South Pole Landing Sites*.
- **Imagery:** LROC NAC South Pole PSR mosaic (contrast-stretched), polar stereographic
  (~11.7 m/px) — LROC PDS node.

See [`CREDITS.md`](CREDITS.md) for required NASA attribution.

## Workspace layout

| Crate / dir | Role |
|---|---|
| `crates/moon_game` | The Bevy runtime: camera, tile streaming, hillshade, flythrough. |
| `crates/moon_data` | Engine-agnostic manifest types + coordinate transforms. |
| `tools/geo_pipeline` | Offline GDAL bake: raw GeoTIFFs → tile pyramid + heightmap + manifest. |
| `scripts/fetch_data.sh` | Downloads the raw source rasters into `data/raw/`. |
| `assets/` | Baked tiles, elevation, shaders, and the site manifest (tiles/elevation in LFS). |
