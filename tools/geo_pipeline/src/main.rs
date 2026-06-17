//! `geo_pipeline` — the offline It-0 bake tool.
//!
//! Reprojects the large source LRO GeoTIFFs (DEM + imagery) into one shared
//! south-polar-stereographic grid centered on the lunar south pole, slices the imagery
//! into a tile pyramid, bakes a 16-bit heightmap PNG, and writes the `SiteManifest` the
//! game reads. GDAL does the heavy raster work (shelled out, never linked).
//!
//! Usage:
//!   cargo run -p geo_pipeline -- bake --site shackleton --raw data/raw --out assets
//!
//! Prerequisites (manual, one-time): `brew install gdal`, and source rasters in
//! `data/raw/` (a `*dem*`/`*ldem*` GeoTIFF and an imagery GeoTIFF). See plan 02.

mod gdal;
mod manifest;
mod tile;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use moon_data::{DemInfo, SiteId, SunDefaults};

// --- Bake geometry (It-0) --------------------------------------------------
// The target grid (CRS, extent, finest resolution) is NOT hardcoded — it is read from
// the source DEM, so the bake always matches whatever data lives in data/raw/ and the
// imagery is guaranteed to register against the DEM. The DEM ships in south-polar-
// stereographic at the true lunar radius; for the PGDA Site04 DEM that is a 16×16 km
// patch on the Shackleton rim (centered near (-1000, -7000) m, not the pole).
const NUM_ZOOMS: u8 = 4;
const TILE_SIZE_PX: u32 = 512;
/// Sun defaults: low grazing light, the characteristic south-polar look.
const SUN: SunDefaults = SunDefaults { azimuth_deg: 135.0, altitude_deg: 1.5 };

// Imagery tonal mapping (visualization only — does not affect geometry/registration).
// The default source is the LROC NAC South Pole PSR mosaic (the *_STRETCH product),
// which is already photometrically stretched by the LROC team: valid (sunlit) pixels
// span a rich DN 1..254 (mean ≈ 93), and the in-shadow interior is a clean NoData=0.
// So the bake just needs a faithful linear passthrough — map [1,255]→[0,255] so NoData
// (0) renders as solid black and the rim/wall detail is preserved unaltered.
// (For the rawer WAC global mosaic, which sits near the noise floor, drop LO≈2 / HI≈120
// and GAMMA≈0.7 instead to lift its dim slopes — see git history.)
const IMG_SCALE_LO: &str = "1"; // src DN mapped to black (NoData 0 falls below → black)
const IMG_SCALE_HI: &str = "255"; // src DN mapped to white
const IMG_GAMMA: &str = "1.0"; // linear: keep the source's existing stretch faithful

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("bake") => bake(&parse_bake(&args[1..])?),
        Some("check") => check(args.get(1)),
        Some(other) => Err(format!("unknown command `{other}` (expected `bake` or `check`)")),
        None => Err(usage()),
    }
}

fn usage() -> String {
    "usage:\n  geo_pipeline bake --site <id> --raw <dir> --out <dir>\n  geo_pipeline check <manifest.ron>".to_string()
}

/// Parse a baked manifest back through `moon_data` and print a summary — the offline
/// proof of plan 02's round-trip criterion (and a stand-in for the game's startup load).
fn check(path: Option<&String>) -> Result<(), String> {
    let path = path.ok_or_else(|| format!("missing manifest path\n{}", usage()))?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;
    let m: moon_data::SiteManifest =
        ron::from_str(&text).map_err(|e| format!("parsing {path}: {e}"))?;
    let tiles: u32 = m.zoom_levels.iter().map(|z| z.tiles_x * z.tiles_y).sum();
    println!("✓ {path} parses as a SiteManifest");
    println!("  site:    {}", m.site);
    println!("  world:   {:?} m", m.world_bbox);
    println!("  zooms:   {} levels, {tiles} tiles total", m.zoom_levels.len());
    println!("  dem:     {}x{} px, elev {} .. {} m", m.dem.width, m.dem.height, m.dem.elev_min_m, m.dem.elev_max_m);
    Ok(())
}

struct BakeArgs {
    site: SiteId,
    raw: PathBuf,
    out: PathBuf,
}

/// Minimal hand-rolled `--flag value` parser (no clap dependency).
fn parse_bake(args: &[String]) -> Result<BakeArgs, String> {
    let mut site = None;
    let mut raw = None;
    let mut out = None;
    let mut i = 0;
    while i < args.len() {
        let flag = &args[i];
        let value = || {
            args.get(i + 1)
                .cloned()
                .ok_or_else(|| format!("`{flag}` needs a value"))
        };
        match flag.as_str() {
            "--site" => site = Some(SiteId(value()?)),
            "--raw" => raw = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            other => return Err(format!("unexpected argument `{other}`\n{}", usage())),
        }
        i += 2;
    }
    Ok(BakeArgs {
        site: site.ok_or_else(|| format!("missing --site\n{}", usage()))?,
        raw: raw.ok_or_else(|| format!("missing --raw\n{}", usage()))?,
        out: out.ok_or_else(|| format!("missing --out\n{}", usage()))?,
    })
}

fn bake(args: &BakeArgs) -> Result<(), String> {
    // Locate the source rasters in the raw dir.
    let dem_src = find_raster(&args.raw, &["dem", "ldem"])
        .ok_or_else(|| format!("no DEM GeoTIFF (matching *dem*/*ldem*) found in {}", args.raw.display()))?;
    let img_src = find_raster(&args.raw, &["wac", "nac", "mosaic", "img", "moon", "kit"])
        .ok_or_else(|| format!("no imagery GeoTIFF found in {}", args.raw.display()))?;
    eprintln!("DEM source:     {}", dem_src.display());
    eprintln!("imagery source: {}", img_src.display());

    // Derive the target grid from the DEM: its CRS, extent, and native pixel size become
    // the bake grid (= the finest zoom). Nothing geometric is hardcoded, and the imagery
    // is warped onto this exact grid so it registers with the DEM.
    let src = gdal::raster_info(&dem_src).map_err(|e| e.to_string())?;
    let proj4 = src.proj4.clone();
    let bbox = src.extent;
    let finest_mpp = src.mpp;
    if finest_mpp <= 0.0 {
        return Err(format!("DEM {} reports no usable pixel size", dem_src.display()));
    }
    let base_mpp = finest_mpp * (1u64 << (NUM_ZOOMS - 1)) as f64;
    eprintln!(
        "grid from DEM: extent {bbox:?} @ {finest_mpp} m/px (zoom 0 = {base_mpp} m/px), CRS `{proj4}`"
    );

    // A scratch dir for intermediate warped rasters (kept out of `assets/`).
    let work = args.out.join(".work");
    std::fs::create_dir_all(&work).map_err(|e| format!("creating {}: {e}", work.display()))?;

    // 1. Reproject both into the same grid (same -t_srs / -te / -tr). -------
    let warped_img_tif = work.join("warped_imagery.tif");
    let warped_dem_tif = work.join("warped_dem.tif");
    gdal::warp(&img_src, &warped_img_tif, &proj4, bbox, finest_mpp, &[])
        .map_err(|e| e.to_string())?;
    gdal::warp(&dem_src, &warped_dem_tif, &proj4, bbox, finest_mpp, &[])
        .map_err(|e| e.to_string())?;

    // Verify the two outputs share the same grid.
    let img_info = gdal::raster_info(&warped_img_tif).map_err(|e| e.to_string())?;
    let dem_info = gdal::raster_info(&warped_dem_tif).map_err(|e| e.to_string())?;
    if (img_info.width, img_info.height) != (dem_info.width, dem_info.height) {
        return Err(format!(
            "imagery {}x{} and DEM {}x{} are not on the same grid",
            img_info.width, img_info.height, dem_info.width, dem_info.height
        ));
    }
    eprintln!("warped grid: {}x{} px @ {finest_mpp} m/px", img_info.width, img_info.height);

    // 2. Normalize imagery to an 8-bit PNG the `image` crate can read. ------
    // A clipped gamma stretch (see IMG_SCALE_* / IMG_GAMMA) reveals the few sunlit
    // slopes while keeping shadow + coverage-gaps a clean black — far better than a
    // naive full-range `-scale` for this mostly-shadowed polar site.
    let warped_img_png = work.join("warped_imagery.png");
    gdal::translate_to_png(
        &warped_img_tif,
        &warped_img_png,
        &[
            "-ot", "Byte", "-scale", IMG_SCALE_LO, IMG_SCALE_HI, "0", "255", "-exponent",
            IMG_GAMMA,
        ],
    )
    .map_err(|e| e.to_string())?;

    // 3. Tile the imagery (counts measured from the real raster). -----------
    let zoom_levels = tile::build_pyramid(
        &warped_img_png,
        &args.site,
        &args.out,
        TILE_SIZE_PX,
        base_mpp,
        NUM_ZOOMS,
    )
    .map_err(|e| e.to_string())?;

    // 4. Bake the 16-bit heightmap PNG (Bevy-loadable r16). -----------------
    let dem_dir = args.out.join("elevation");
    std::fs::create_dir_all(&dem_dir).map_err(|e| format!("creating {}: {e}", dem_dir.display()))?;
    let dem_png = dem_dir.join(format!("{}.png", args.site));
    let (emin, emax) = (dem_info.min.to_string(), dem_info.max.to_string());
    gdal::translate_to_png(
        &warped_dem_tif,
        &dem_png,
        &["-ot", "UInt16", "-scale", &emin, &emax, "0", "65535"],
    )
    .map_err(|e| e.to_string())?;
    eprintln!("heightmap: {} (elev {} .. {} m)", dem_png.display(), dem_info.min, dem_info.max);

    // 5. Write the manifest. ------------------------------------------------
    let dem = DemInfo {
        path: format!("elevation/{}.png", args.site),
        width: dem_info.width,
        height: dem_info.height,
        elev_min_m: dem_info.min,
        elev_max_m: dem_info.max,
    };
    let m = manifest::build_manifest(
        args.site.clone(),
        bbox,
        base_mpp,
        TILE_SIZE_PX,
        zoom_levels,
        dem,
        SUN,
        proj4,
    );
    let path = manifest::write_manifest(&m, &args.out).map_err(|e| e.to_string())?;
    eprintln!("manifest: {}", path.display());

    eprintln!("bake complete for site `{}`", args.site);
    Ok(())
}

/// Find the first `*.tif`/`*.tiff` in `dir` whose (lowercased) name contains any keyword.
fn find_raster(dir: &Path, keywords: &[&str]) -> Option<PathBuf> {
    let mut matches: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            matches!(ext.as_deref(), Some("tif") | Some("tiff"))
        })
        .filter(|p| {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
            keywords.iter().any(|k| name.contains(k))
        })
        .collect();
    matches.sort();
    matches.into_iter().next()
}
