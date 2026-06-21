# `data/` — raw source rasters for the offline bake

The `geo_pipeline` bake (plan 02) reprojects and bakes these large source GeoTIFFs into the
small pre-baked assets the game loads (`assets/elevation`, `assets/sites`). It also still emits
a legacy `assets/tiles` imagery pyramid that is no longer committed or loaded. **The game never
reads anything in here** — these are build-time inputs only.

`data/raw/` and `data/raw_southpole/` are gitignored (the files are large and re-downloadable);
this README is tracked.

## 1. Install GDAL (one-time)

The bake shells out to the GDAL command-line tools (it does **not** link the `gdal` crate):

```sh
brew install gdal      # provides gdalwarp, gdal_translate, gdalinfo
```

## 2. Download the source rasters

```sh
scripts/fetch_data.sh                 # DEM only (~41 MB) — recommended first run
scripts/fetch_data.sh --with-imagery  # DEM + the NAC south-pole crop (~3 MB, seconds)
```

The helper resumes the DEM download and skips it when complete. For imagery it does **not**
download the 2.7 GB polar mosaic — it uses GDAL's `/vsicurl` to read only the bytes covering
the DEM extent (HTTP range requests), so the crop is a few MB and takes a few seconds. It
writes normalized filenames `geo_pipeline` auto-discovers:

| File | Source | Notes |
|---|---|---|
| `shackleton_ldem_5mpp.tif` | PGDA LOLA 5 m/px, **Site04** (Shackleton rim) | ~41 MB, 3200×3200, 16×16 km. Surface-interpolated elevation, south-polar-stereographic, MOON_ME / DE421. Matched by `ldem`. |
| `shackleton_nac_psr_mosaic.tif` | LROC NAC South Pole PSR mosaic (`*_STRETCH`), **cropped** to the DEM extent at 10 m/px | ~3 MB. Polar-stereographic + registered to the DEM. Matched by `nac`/`mosaic`. |

### Provenance / citations
- **DEM:** `https://pgda.gsfc.nasa.gov/products/78` →
  `https://pgda.gsfc.nasa.gov/data/LOLA_5mpp/Site04/Site04_final_adj_5mpp_surf.tif`.
  Barker et al. (2021), *Improved LOLA Elevation Maps for South Pole Landing Sites*.
- **Imagery:** LROC NAC South Pole PSR mosaic (contrast-stretched), read remotely via
  `/vsicurl/https://pds.lroc.im-ldi.com/data/LRO-L-LROC-5-RDR-V1.0/LROLRC_2001/EXTRAS/BROWSE/NAC_POLE/NAC_POLE_PSR_SOUTH/NAC_POLE_PSR_SOUTH_STRETCH.TIF`.
  NASA/GSFC/Arizona State University. ~11.7 m/px, polar stereographic, covers 80°S→pole.

### Regional `southpole` surface (a larger flyover area)
For a much larger pan/zoom surface, `scripts/fetch_data.sh --region` crops a
**120 × 120 km** pole-centered box from the LOLA **20 m/px** south-polar LDEM
(80°S), *A New View of the Lunar South Pole from LOLA*
(`https://pgda.gsfc.nasa.gov/data/LOLA_20mpp/LDEM_80S_20MPP_ADJ.TIF`, product 90), reading
its **40 m overview** (the full-res tile index of that COG is flaky over `/vsicurl`; the
overview reads reliably and fast). Outputs land in `data/raw_southpole/` and bake to the
`southpole` site. NASA LOLA / PGDA, GSFC. Use `--region-km` / `--region-mpp` to resize.

### Imagery choice (and how to swap)
The NAC South Pole mosaic is a controlled **polar-stereographic** product (~11.7 m/px), so
it has no equirectangular pole distortion and **no orbital seams**, and ~10× the WAC global
mosaic's resolution. Shackleton's interior is a permanently shadowed region with no NAC
coverage — it bakes to clean black (that's real); the sunlit rim/walls carry rich relief.
The in-shadow terrain is best shown via the DEM **hillshade** (plan 05), not imagery.

To crop from a different remote mosaic (e.g. the older WAC global 100 m, or a finer NAC
product), pass a URL; `--crop-mpp` sets the crop resolution:

```sh
scripts/fetch_data.sh --imagery-url <URL-to-a-georeferenced-GeoTIFF> --crop-mpp 10
```

The bake auto-discovers any `data/raw/` GeoTIFF whose name contains `nac`/`wac`/`mosaic`/
`img`. To **see inside the shadow** in true imagery, composite in **ShadowCam** (Danuri/KPLO)
COGs from `https://pds.shadowcam.im-ldi.com/` — the floor/walls NAC can't image. A NASA CGI
Moon Kit crop is *not* georeferenced, so it would need manual georeferencing first.

## 3. Run the bake

```sh
cargo run -p geo_pipeline -- bake --site shackleton --raw data/raw --out assets
```

The bake **derives its target grid (CRS, extent, finest m/px) from the DEM** — nothing
geometric is hardcoded — and warps the imagery onto that exact grid so the two register.
For the Site04 DEM that yields a 16×16 km area at 5 m/px (zoom 0 = 40 m/px, 4 zoom levels).

It produces `assets/tiles/shackleton/<z>/<x>_<y>.png` (70 tiles), `assets/elevation/shackleton.png`
(16-bit heightmap), and `assets/sites/shackleton.ron` (the `SiteManifest`). Intermediate
warped rasters land in `assets/.work/` (gitignored).

Verify the manifest parses back (plan 02's round-trip criterion):

```sh
cargo run -p geo_pipeline -- check assets/sites/shackleton.ron
```
