//! Thin wrappers over the GDAL command-line tools.
//!
//! GDAL is a **build-time** dependency only: we shell out to `gdalwarp`,
//! `gdal_translate`, and `gdalinfo` rather than linking the `gdal` crate. This keeps
//! `moon_game` free of any FFI / GDAL version pin — the bake is a separate offline step.
//!
//! Every call is logged (so a bake is auditable / reproducible from `data/raw/` alone)
//! and its exit status is checked.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GdalError {
    #[error("failed to spawn `{tool}` — is GDAL installed? try `brew install gdal` ({source})")]
    Spawn {
        tool: String,
        source: std::io::Error,
    },
    #[error("`{tool}` exited with status {code}\n--- stderr ---\n{stderr}")]
    NonZero {
        tool: String,
        code: String,
        stderr: String,
    },
    #[error("could not parse `gdalinfo -json` output: {0}")]
    Json(#[from] serde_json::Error),
    #[error("`gdalinfo` reported no raster bands for {0}")]
    NoBands(String),
}

/// Run a GDAL tool, logging the full command line, and fail on a non-zero exit.
/// Returns captured stdout (useful for `gdalinfo`; ignored by the warp/translate calls).
pub fn run(tool: &str, args: &[&str]) -> Result<String, GdalError> {
    eprintln!("$ {tool} {}", args.join(" "));
    let output = Command::new(tool)
        .args(args)
        .output()
        .map_err(|source| GdalError::Spawn {
            tool: tool.to_string(),
            source,
        })?;

    if !output.status.success() {
        return Err(GdalError::NonZero {
            tool: tool.to_string(),
            code: output
                .status
                .code()
                .map_or_else(|| "signal".to_string(), |c| c.to_string()),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `gdalwarp` a source raster into the shared south-polar-stereographic grid.
///
/// `bbox` is `[min_x, min_y, max_x, max_y]` in projected meters (`-te`); `mpp` is the
/// pixel size (`-tr`). Forcing both extent and resolution makes the imagery and DEM
/// land on the *same* grid, which is what guarantees registration and fixes the 89°S
/// distortion of the source projections.
pub fn warp(
    src: &Path,
    dst: &Path,
    proj4: &str,
    bbox: [f64; 4],
    mpp: f64,
    extra: &[&str],
) -> Result<(), GdalError> {
    let (te_a, te_b, te_c, te_d) = (
        bbox[0].to_string(),
        bbox[1].to_string(),
        bbox[2].to_string(),
        bbox[3].to_string(),
    );
    let tr = mpp.to_string();
    let src = src.to_string_lossy();
    let dst = dst.to_string_lossy();

    let mut args = vec![
        "-overwrite",
        "-t_srs",
        proj4,
        "-te",
        &te_a,
        &te_b,
        &te_c,
        &te_d,
        "-tr",
        &tr,
        &tr,
        "-r",
        "bilinear",
    ];
    args.extend_from_slice(extra);
    args.push(&src);
    args.push(&dst);
    run("gdalwarp", &args)?;
    Ok(())
}

/// Convert a (warped) raster to a plain PNG the `image` crate can read reliably.
/// `extra` lets callers add `-ot`, `-scale`, `-expand rgb`, etc.
pub fn translate_to_png(src: &Path, dst: &Path, extra: &[&str]) -> Result<(), GdalError> {
    let src = src.to_string_lossy();
    let dst = dst.to_string_lossy();
    // GDAL_PAM_ENABLED NO suppresses the `.aux.xml` sidecar GDAL would drop next to the PNG.
    let mut args = vec!["--config", "GDAL_PAM_ENABLED", "NO", "-of", "PNG"];
    args.extend_from_slice(extra);
    args.push(&src);
    args.push(&dst);
    run("gdal_translate", &args)?;
    Ok(())
}

/// Raster summary parsed from `gdalinfo -json` (+ `gdalsrsinfo` for the PROJ string).
#[derive(Debug, Clone)]
pub struct RasterInfo {
    pub width: u32,
    pub height: u32,
    /// Min / max of band 1 (computed via `-mm`). Meaningful for the DEM (elevation in m).
    pub min: f64,
    pub max: f64,
    /// `[min_x, min_y, max_x, max_y]` in the raster's own projected CRS.
    pub extent: [f64; 4],
    /// Pixel size in projected units (meters per pixel).
    pub mpp: f64,
    /// The raster's CRS as a PROJ.4 string (the grid the bake reprojects onto).
    pub proj4: String,
}

// Minimal shapes for the slice of `gdalinfo -json` we consume.
#[derive(Deserialize)]
struct GdalInfoJson {
    size: [u32; 2],
    bands: Vec<BandJson>,
    #[serde(rename = "geoTransform")]
    geo_transform: Option<[f64; 6]>,
    #[serde(rename = "cornerCoordinates")]
    corner_coordinates: Option<Corners>,
}

#[derive(Deserialize)]
struct Corners {
    #[serde(rename = "lowerLeft")]
    lower_left: [f64; 2],
    #[serde(rename = "upperRight")]
    upper_right: [f64; 2],
}

#[derive(Deserialize)]
struct BandJson {
    #[serde(default)]
    #[serde(rename = "computedMin")]
    computed_min: Option<f64>,
    #[serde(default)]
    #[serde(rename = "computedMax")]
    computed_max: Option<f64>,
    #[serde(default)]
    minimum: Option<f64>,
    #[serde(default)]
    maximum: Option<f64>,
}

/// Run `gdalinfo -json -mm` (+ `gdalsrsinfo`) and extract size, extent, mpp, CRS, min/max.
pub fn raster_info(path: &Path) -> Result<RasterInfo, GdalError> {
    let p = path.to_string_lossy();
    let stdout = run("gdalinfo", &["-json", "-mm", &p])?;
    let info: GdalInfoJson = serde_json::from_str(&stdout)?;
    let band = info
        .bands
        .first()
        .ok_or_else(|| GdalError::NoBands(p.to_string()))?;

    let corners = info
        .corner_coordinates
        .ok_or_else(|| GdalError::NoBands(format!("{p} (no cornerCoordinates)")))?;
    let extent = [
        corners.lower_left[0],
        corners.lower_left[1],
        corners.upper_right[0],
        corners.upper_right[1],
    ];
    // geoTransform[1] is the pixel width (negative height is at index 5).
    let mpp = info.geo_transform.map_or(0.0, |gt| gt[1].abs());

    Ok(RasterInfo {
        width: info.size[0],
        height: info.size[1],
        min: band.computed_min.or(band.minimum).unwrap_or(0.0),
        max: band.computed_max.or(band.maximum).unwrap_or(0.0),
        extent,
        mpp,
        proj4: proj4(path)?,
    })
}

/// The raster's CRS as a single-line PROJ.4 string (via `gdalsrsinfo -o proj4`).
pub fn proj4(path: &Path) -> Result<String, GdalError> {
    let p = path.to_string_lossy();
    let out = run("gdalsrsinfo", &["-o", "proj4", &p])?;
    Ok(out.trim().to_string())
}
