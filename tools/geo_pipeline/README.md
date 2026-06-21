# `geo_pipeline` — offline terrain bake

Turns raw NASA GeoTIFFs (LOLA DEMs + LROC imagery) into the small pre-baked assets the game
loads: a tile pyramid, a 16-bit elevation heightmap, and a site manifest. It runs **offline** —
the committed `assets/{tiles,elevation,sites}/<site>.*` are its output, so most contributors
never need to run it. The game never reads anything under `data/`.

Each site auto-derives its grid (CRS, extent, finest m/px) from its DEM and warps the imagery
onto that grid so the two register.

## Prerequisites

The bake shells out to the GDAL command-line tools (it does **not** link the `gdal` crate):

```sh
brew install gdal      # provides gdalwarp, gdal_translate, gdalinfo
```

## Reproduce the baked assets

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

See [`data/README.md`](../../data/README.md) for full provenance and how to swap imagery sources.

## Data sources

- **Regional DEM (`southpole`):** PGDA LOLA 20 m/px south-polar LDEM (80°S), *A New View of
  the Lunar South Pole from LOLA* — <https://pgda.gsfc.nasa.gov/products/90>.
- **Close-up DEM (`shackleton`):** PGDA LOLA 5 m/px, Site04 (Shackleton rim) —
  <https://pgda.gsfc.nasa.gov/products/78>. Barker et al. (2021), *Improved LOLA Elevation
  Maps for South Pole Landing Sites*.
- **Imagery:** LROC NAC South Pole PSR mosaic (contrast-stretched), polar stereographic
  (~11.7 m/px) — LROC PDS node.

See [`CREDITS.md`](../../CREDITS.md) for required NASA attribution.
