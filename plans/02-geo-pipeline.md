# It-0 · Plan 02 — Geo pipeline (`tools/geo_pipeline`)

## Context

The offline half of It-0 (see [`00-it0-overview.md`](00-it0-overview.md)). Real LRO data
ships as large GeoTIFFs in projections that distort badly at 89°S; the game must only ever
load small pre-baked tiles. This plan builds the one-time bake tool that reprojects + tiles
the data and emits the manifest the game reads. **This is the highest-uncertainty plan —
build it first** so the Bevy work has real terrain to render (and bootstrap from the CGI
Moon Kit if the NAC mosaic lags).

**Dependencies:** plan 01 (consumes `moon_data::SiteManifest` / `TileCoord` / `tile_path`).

GDAL is a **build-time tool only** — `geo_pipeline` shells out via `std::process::Command`;
it does **not** link the `gdal` crate (avoids FFI/version-pin pain). GDAL is never part of
`moon_game`.

## Prerequisites (one-time, manual)

- `brew install gdal` (provides `gdalinfo`, `gdalwarp`, `gdal_translate`, `gdaldem`).
- Download Shackleton source data into the gitignored `data/raw/`:
  - **DEM:** PGDA LOLA_5mpp `Site04` LDEM GeoTIFF
    (`https://pgda.gsfc.nasa.gov/data/LOLA_5mpp/Site04/`).
  - **Imagery:** the **LROC NAC South Pole PSR mosaic** (`NAC_POLE_PSR_SOUTH_STRETCH.TIF`,
    ~11.7 m/px, already polar-stereographic, contrast-stretched) — `scripts/fetch_data.sh
    --with-imagery` crops it to the bbox via `/vsicurl`. This replaced the original LROC WAC
    global 100 m bootstrap (equirectangular → orbital seams + NoData gaps at the pole). The
    shadowed crater interior has no NAC coverage (bakes to clean black — real, not missing).
    Finer/extra options: 1 m/px NAC "Avg Merge" S-Pole mosaic; **ShadowCam** (Danuri) COGs
    to image *inside* the shadow. Any georeferenced GeoTIFF works via `--imagery-url`.

## Scope

A runnable bin: `cargo run -p geo_pipeline -- bake --site shackleton --raw data/raw --out
assets`, producing the full tile pyramid + heightmap + manifest for one site.

## Approach

### Pipeline stages (the `bake` command)
1. **Reproject to a common grid.** `gdalwarp` both imagery and DEM into the *same* south
   polar-stereographic grid centered on Shackleton: explicit lunar-radius PROJ string
   (`+proj=stere +lat_0=-90 +lat_ts=-90 +lon_0=0 +R=1737400 +units=m`), shared `-te` bbox
   (≥20×20 km) and `-tr <mpp>`. **This fixes the 89°S distortion** and guarantees DEM↔imagery
   registration. Verify each output with `gdalinfo` (square, centered, correct CRS, extent).
2. **Normalize.** `gdal_translate -ot Byte -scale <lo> <hi> 0 255 [-exponent <gamma>]` to
   map the imagery to 8-bit (NoData → black). The stretch is **source-specific** (tunable
   consts in `main.rs`): the NAC `_STRETCH` product is already photometrically stretched, so
   a faithful linear passthrough (`1 255`, gamma 1.0); a rawer source like the WAC mosaic
   needs a shadow-floor clip + gamma lift (≈`2 120`, gamma 0.7) to reveal dim slopes. Keep
   DEM as Float32/Int16 for the next step.
3. **Tile the imagery.** A Rust crop loop (preferred over `gdal2tiles.py` so the on-disk
   names match `moon_data::tile_path` exactly): for each zoom level, downsample to that
   level's mpp, then emit 512×512 PNGs to `assets/tiles/shackleton/<z>/<x>_<y>.png`. Use
   `gdal_translate -srcwin` per tile, or read the warped raster once with the `image` crate
   and slice. 3–4 zoom levels: zoom 0 = whole site coarse, max zoom = finest source mpp.
4. **Heightmap.** `gdal_translate -ot UInt16 -scale <elev_min> <elev_max> 0 65535 -of PNG
   warped_dem.tif assets/elevation/shackleton.png` → a 16-bit grayscale PNG. (A 16-bit PNG
   *is* a Bevy-loadable r16 — avoids writing a custom asset loader in the game.) Record the
   real `elev_min_m`/`elev_max_m` so the shader can reconstruct meters.
5. **Write the manifest.** Serialize a `moon_data::SiteManifest` to
   `assets/sites/shackleton.ron`: projected + world bbox, `base_mpp`, `tile_size_px=512`,
   per-zoom `{mpp, tiles_x, tiles_y}`, DEM dims + elev min/max, sun defaults, and the exact
   `crs_proj4` used (provenance).

### Implementation notes
- Wrap each GDAL call in a helper that logs the command and checks the exit status
  (`thiserror` on failure). Determinism isn't required, but reproducibility is — the bake
  must be re-runnable from `data/raw/` alone.
- Keep tile + zoom geometry computed from the warped raster size, then written into the
  manifest — never hardcode counts.
- The bake is slow + offline; it's fine that it isn't part of `cargo build`.

## Files to create / produce

- `tools/geo_pipeline/src/main.rs` (arg parsing: `bake`), `gdal.rs` (Command wrappers),
  `tile.rs` (crop loop), `manifest.rs` (build + write `SiteManifest`).
- Produced assets: `assets/tiles/shackleton/<z>/<x>_<y>.png`,
  `assets/elevation/shackleton.png`, `assets/sites/shackleton.ron`.

## Verification

- `gdalinfo` on warped outputs: correct polar-stereographic CRS, square pixels, centered
  ≥20×20 km extent, DEM + imagery share the same grid/extent.
- Eyeball a handful of tiles across zoom levels and the heightmap PNG (recognizable
  Shackleton rim/PSR).
- `cat assets/sites/shackleton.ron` parses back via `ron::from_str::<SiteManifest>` (a tiny
  test or the game's startup load proves it).
- Re-running `bake` from `data/raw/` reproduces identical asset counts.
