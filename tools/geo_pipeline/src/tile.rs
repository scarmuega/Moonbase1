//! The imagery tiling crop loop.
//!
//! We read the warped imagery once (as a plain PNG via `gdal_translate`), then for each
//! zoom level downsample it and slice it into `tile_size`² PNGs named **exactly** the way
//! `moon_data::tile_path` expects — that name agreement is why we do this in Rust instead
//! of `gdal2tiles.py`.
//!
//! Zoom convention (matches `moon_data`): `world_per_pixel(z) = base_mpp / 2^z`, so
//! **zoom 0 is the coarsest** whole-site view and `max zoom` is the finest. The warped
//! raster is the finest level; each lower zoom is a 2× downsample of it.

use std::path::Path;

use image::{Rgb, RgbImage};
use image::imageops::{FilterType, resize};
use moon_data::{SiteId, TileCoord, ZoomLevel, tile_path};

#[derive(Debug, thiserror::Error)]
pub enum TileError {
    #[error("reading warped imagery {path}: {source}")]
    Open {
        path: String,
        source: image::ImageError,
    },
    #[error("writing tile {path}: {source}")]
    Write {
        path: String,
        source: image::ImageError,
    },
    #[error("creating directory {path}: {source}")]
    Dir {
        path: String,
        source: std::io::Error,
    },
}

/// Build the full tile pyramid from the finest-resolution warped imagery PNG.
///
/// `num_zooms` levels are emitted (`0..num_zooms`); the finest (`num_zooms - 1`) is the
/// raster at full size, each lower level a halving. `base_mpp` is the zoom-0 resolution,
/// used only to fill in each `ZoomLevel::mpp`. Returns the levels actually written, with
/// tile counts measured from the real pixel sizes (never hardcoded).
pub fn build_pyramid(
    warped_png: &Path,
    site: &SiteId,
    out_root: &Path,
    tile_size: u32,
    base_mpp: f64,
    num_zooms: u8,
) -> Result<Vec<ZoomLevel>, TileError> {
    let full = image::open(warped_png)
        .map_err(|source| TileError::Open {
            path: warped_png.to_string_lossy().into_owned(),
            source,
        })?
        .to_rgb8();

    let max_zoom = num_zooms - 1;
    let mut levels = Vec::with_capacity(num_zooms as usize);

    for zoom in 0..num_zooms {
        // Downsample the full raster by 2^(max_zoom - zoom).
        let shift = max_zoom - zoom;
        let scaled = if shift == 0 {
            full.clone()
        } else {
            let w = (full.width() >> shift).max(1);
            let h = (full.height() >> shift).max(1);
            resize(&full, w, h, FilterType::Lanczos3)
        };

        let tiles_x = scaled.width().div_ceil(tile_size);
        let tiles_y = scaled.height().div_ceil(tile_size);
        emit_tiles(&scaled, site, out_root, tile_size, zoom, tiles_x, tiles_y)?;

        levels.push(ZoomLevel {
            zoom,
            mpp: base_mpp / (1u64 << zoom) as f64,
            tiles_x,
            tiles_y,
        });
        eprintln!("zoom {zoom}: {}x{} px -> {tiles_x}x{tiles_y} tiles", scaled.width(), scaled.height());
    }

    Ok(levels)
}

/// Slice one zoom level into `tile_size`² PNGs, padding the right/bottom edge tiles with
/// black where the raster runs out.
fn emit_tiles(
    img: &RgbImage,
    site: &SiteId,
    out_root: &Path,
    tile_size: u32,
    zoom: u8,
    tiles_x: u32,
    tiles_y: u32,
) -> Result<(), TileError> {
    let zoom_dir = out_root.join(format!("tiles/{site}/{zoom}"));
    std::fs::create_dir_all(&zoom_dir).map_err(|source| TileError::Dir {
        path: zoom_dir.to_string_lossy().into_owned(),
        source,
    })?;

    let height = img.height() as i64;
    let tsz = tile_size as i64;
    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            let mut tile = RgbImage::from_pixel(tile_size, tile_size, Rgb([0, 0, 0]));
            for py in 0..tile_size {
                // Tile y grows NORTH (moon_data convention, Bevy Y-up) but the warped
                // raster is north-up (row 0 = north). Map so tile (·, 0) is the SOUTH
                // strip and py=0 is each tile's north edge: laid out per
                // `tile_to_world_min`, the tiles reconstruct the north-up raster and the
                // ragged edge padding lands at the north (off the real extent).
                let sy = height - (ty as i64 + 1) * tsz + py as i64;
                if sy < 0 || sy >= height {
                    continue;
                }
                let sy = sy as u32;
                for px in 0..tile_size {
                    let sx = tx * tile_size + px;
                    if sx >= img.width() {
                        break;
                    }
                    tile.put_pixel(px, py, *img.get_pixel(sx, sy));
                }
            }

            // tile_path is relative (`tiles/<site>/<z>/<x>_<y>.png`); join under out_root.
            let rel = tile_path(site, TileCoord::new(zoom, tx, ty));
            let path = out_root.join(&rel);
            tile.save(&path).map_err(|source| TileError::Write {
                path: path.to_string_lossy().into_owned(),
                source,
            })?;
        }
    }
    Ok(())
}
