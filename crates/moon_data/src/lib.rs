//! `moon_data` — the shared, Bevy-free contract between the offline geo pipeline
//! (which *writes* the manifest + tiles) and the runtime game (which *reads* them).
//!
//! It holds two things and nothing else:
//!   1. The serde types that round-trip through `assets/sites/<site>.ron`.
//!   2. The pure coordinate transforms both halves must agree on, byte-for-byte.
//!
//! # Coordinate conventions (shared across the whole project)
//! - **World:** 1 unit = 1 meter; origin at the site-bbox center (keeps float precision
//!   tight). `world_bbox = [min_x, min_y, max_x, max_y]`.
//! - **Tile:** integer `(zoom, x, y)`. Higher zoom = finer detail = smaller footprint.
//!   `world_per_pixel(z) = base_mpp / 2^z`. Tiles are `tile_size_px` square (512).
//!   Tile `(0, 0)`'s minimum corner sits at world `(min_x, min_y)`; `x` grows east,
//!   `y` grows north.
//! - **DEM UV:** texture space, top-left origin (row 0 = north edge = `max_y`). This
//!   flip is mirrored verbatim in `hillshade.wgsl`.

use glam::{DVec2, Vec2};
use serde::{Deserialize, Serialize};

/// Names a baked site; maps directly to the `tiles/<site>/...` asset subdirectory.
///
/// A newtype over `String` (serde-transparent, so it serializes as a bare string in RON)
/// rather than an enum, so adding a site never touches this crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SiteId(pub String);

impl SiteId {
    pub fn shackleton() -> Self {
        Self("shackleton".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SiteId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// An integer tile address within a site's pyramid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileCoord {
    pub zoom: u8,
    pub x: u32,
    pub y: u32,
}

impl TileCoord {
    pub fn new(zoom: u8, x: u32, y: u32) -> Self {
        Self { zoom, x, y }
    }
}

/// The on-disk relative path for a tile: `tiles/<site>/<zoom>/<x>_<y>.png`.
///
/// Single source of truth for the layout: the pipeline writes here, the game loads here.
pub fn tile_path(site: &SiteId, coord: TileCoord) -> String {
    format!(
        "tiles/{}/{}/{}_{}.png",
        site.as_str(),
        coord.zoom,
        coord.x,
        coord.y
    )
}

/// One level of the tile pyramid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ZoomLevel {
    pub zoom: u8,
    /// Meters per pixel at this zoom (= `base_mpp / 2^zoom`); stored for provenance.
    pub mpp: f64,
    pub tiles_x: u32,
    pub tiles_y: u32,
}

/// Metadata for the baked elevation heightmap (a 16-bit grayscale PNG over the site bbox).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DemInfo {
    /// Relative asset path, e.g. `elevation/shackleton.png`.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub elev_min_m: f64,
    pub elev_max_m: f64,
}

/// Default sun direction for the hillshade (degrees).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SunDefaults {
    pub azimuth_deg: f32,
    pub altitude_deg: f32,
}

/// The keystone: everything the game needs to place, stream, and shade a site.
/// Nothing geometric is hardcoded in the game — it all comes from here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SiteManifest {
    pub site: SiteId,
    /// `[min_x, min_y, max_x, max_y]` in projected meters (GDAL output).
    pub projected_bbox: [f64; 4],
    /// Same extent recentred so the origin is the center: `[min_x, min_y, max_x, max_y]`.
    pub world_bbox: [f32; 4],
    /// Meters per pixel at zoom 0.
    pub base_mpp: f64,
    pub tile_size_px: u32,
    pub zoom_levels: Vec<ZoomLevel>,
    pub dem: DemInfo,
    pub sun: SunDefaults,
    /// The exact PROJ string used to warp (provenance + reproducibility).
    pub crs_proj4: String,
}

impl SiteManifest {
    /// `[min_x, min_y, max_x, max_y]` accessors in world space.
    pub fn world_min(&self) -> Vec2 {
        Vec2::new(self.world_bbox[0], self.world_bbox[1])
    }
    pub fn world_max(&self) -> Vec2 {
        Vec2::new(self.world_bbox[2], self.world_bbox[3])
    }

    /// Look up a baked zoom level by its `zoom` index, if present.
    pub fn zoom_level(&self, zoom: u8) -> Option<&ZoomLevel> {
        self.zoom_levels.iter().find(|z| z.zoom == zoom)
    }
}

// ---------------------------------------------------------------------------
// Pure transforms — the heart of the crate. Called by the game's streaming +
// hillshade math; `world_to_dem_uv` is also mirrored in WGSL.
// ---------------------------------------------------------------------------

/// World meters per pixel at `zoom`: `base_mpp / 2^zoom`.
pub fn world_per_pixel(manifest: &SiteManifest, zoom: u8) -> f64 {
    manifest.base_mpp / (1u64 << zoom) as f64
}

/// World-space edge length of one tile at `zoom` (= `tile_size_px * world_per_pixel`).
pub fn tile_world_size(manifest: &SiteManifest, zoom: u8) -> f32 {
    (manifest.tile_size_px as f64 * world_per_pixel(manifest, zoom)) as f32
}

/// Which tile contains `world_xy` at the given `zoom`.
///
/// Tile `(0, 0)` covers `[min_x, min_x + tws) × [min_y, min_y + tws)`. The result is
/// clamped into the baked grid when the zoom level is known, so a point on the far
/// edge maps to the last valid tile rather than off-grid.
pub fn world_to_tile(manifest: &SiteManifest, world_xy: Vec2, zoom: u8) -> TileCoord {
    let tws = tile_world_size(manifest, zoom) as f64;
    let min = manifest.world_min();
    let fx = ((world_xy.x - min.x) as f64 / tws).floor();
    let fy = ((world_xy.y - min.y) as f64 / tws).floor();

    let mut x = fx.max(0.0) as u32;
    let mut y = fy.max(0.0) as u32;
    if let Some(level) = manifest.zoom_level(zoom) {
        x = x.min(level.tiles_x.saturating_sub(1));
        y = y.min(level.tiles_y.saturating_sub(1));
    }
    TileCoord::new(zoom, x, y)
}

/// The minimum (bottom-left, i.e. south-west) corner of `coord` in world space.
pub fn tile_to_world_min(manifest: &SiteManifest, coord: TileCoord) -> Vec2 {
    let tws = tile_world_size(manifest, coord.zoom) as f64;
    let min = manifest.world_min();
    Vec2::new(
        (min.x as f64 + coord.x as f64 * tws) as f32,
        (min.y as f64 + coord.y as f64 * tws) as f32,
    )
}

/// Map a world point to DEM texture UV in `[0, 1]`, top-left origin (row 0 = north edge).
///
/// **Keep this formula identical in `hillshade.wgsl`.** `u = (x - min_x) / span_x`,
/// `v = (max_y - y) / span_y` (the y flip puts north at the top of the texture).
pub fn world_to_dem_uv(manifest: &SiteManifest, world_xy: Vec2) -> Vec2 {
    let min = manifest.world_min();
    let max = manifest.world_max();
    // f64 math then narrow: spans can be tens of km, UV needs sub-pixel accuracy.
    let span = DVec2::new((max.x - min.x) as f64, (max.y - min.y) as f64);
    let u = (world_xy.x - min.x) as f64 / span.x;
    let v = (max.y - world_xy.y) as f64 / span.y;
    Vec2::new(u as f32, v as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small, internally-consistent manifest for transform tests.
    fn test_manifest() -> SiteManifest {
        // 20×20 km site, 512px tiles, base 5 m/px. At zoom 0 a tile spans 2560 m,
        // so ~8×8 tiles cover the extent.
        let base_mpp = 5.0;
        let half = 10_000.0_f32; // 20 km wide, centered on origin
        let tile_size_px = 512u32;
        let mut zoom_levels = Vec::new();
        for z in 0u8..=3 {
            let mpp = base_mpp / (1u64 << z) as f64;
            let tws = tile_size_px as f64 * mpp;
            let span = (half as f64) * 2.0;
            let tiles = (span / tws).ceil() as u32;
            zoom_levels.push(ZoomLevel {
                zoom: z,
                mpp,
                tiles_x: tiles,
                tiles_y: tiles,
            });
        }
        SiteManifest {
            site: SiteId::shackleton(),
            projected_bbox: [-10_000.0, -10_000.0, 10_000.0, 10_000.0],
            world_bbox: [-half, -half, half, half],
            base_mpp,
            tile_size_px,
            zoom_levels,
            dem: DemInfo {
                path: "elevation/shackleton.png".to_string(),
                width: 4000,
                height: 4000,
                elev_min_m: -500.0,
                elev_max_m: 1200.0,
            },
            sun: SunDefaults {
                azimuth_deg: 45.0,
                altitude_deg: 10.0,
            },
            crs_proj4: "+proj=stere +lat_0=-90 +lon_0=0 +R=1737400 +units=m +no_defs"
                .to_string(),
        }
    }

    #[test]
    fn world_per_pixel_halves_each_zoom() {
        let m = test_manifest();
        for z in 0u8..6 {
            assert_eq!(world_per_pixel(&m, z + 1), world_per_pixel(&m, z) / 2.0);
        }
        assert_eq!(world_per_pixel(&m, 0), m.base_mpp);
    }

    #[test]
    fn tile_path_layout() {
        let p = tile_path(&SiteId::shackleton(), TileCoord::new(2, 5, 7));
        assert_eq!(p, "tiles/shackleton/2/5_7.png");
    }

    #[test]
    fn world_to_tile_then_back_within_one_tile() {
        let m = test_manifest();
        let probes = [
            Vec2::new(0.0, 0.0),
            Vec2::new(-9_999.0, 9_999.0),
            Vec2::new(3_210.5, -7_654.3),
            Vec2::new(1234.0, 5678.0),
        ];
        for z in 0u8..=3 {
            let tws = tile_world_size(&m, z);
            for p in probes {
                let coord = world_to_tile(&m, p, z);
                let corner = tile_to_world_min(&m, coord);
                // The probe must lie within the tile we resolved to: [corner, corner+tws).
                // (Edge points may clamp to the last tile, hence the <= upper guard.)
                assert!(
                    p.x >= corner.x - 1.0 && p.x <= corner.x + tws + 1.0,
                    "x out of tile at zoom {z}: p={p:?} corner={corner:?} tws={tws}"
                );
                assert!(
                    p.y >= corner.y - 1.0 && p.y <= corner.y + tws + 1.0,
                    "y out of tile at zoom {z}: p={p:?} corner={corner:?} tws={tws}"
                );
            }
        }
    }

    #[test]
    fn tile_zero_corner_is_world_min() {
        let m = test_manifest();
        let corner = tile_to_world_min(&m, TileCoord::new(0, 0, 0));
        assert_eq!(corner, m.world_min());
    }

    #[test]
    fn dem_uv_corners_and_flip() {
        let m = test_manifest();
        let min = m.world_min();
        let max = m.world_max();
        // North-west world corner (min_x, max_y) -> texture top-left (0, 0).
        let nw = world_to_dem_uv(&m, Vec2::new(min.x, max.y));
        assert!((nw.x).abs() < 1e-6 && (nw.y).abs() < 1e-6, "nw={nw:?}");
        // South-east world corner (max_x, min_y) -> texture bottom-right (1, 1).
        let se = world_to_dem_uv(&m, Vec2::new(max.x, min.y));
        assert!((se.x - 1.0).abs() < 1e-6 && (se.y - 1.0).abs() < 1e-6, "se={se:?}");
        // Center -> (0.5, 0.5).
        let c = world_to_dem_uv(&m, Vec2::ZERO);
        assert!((c.x - 0.5).abs() < 1e-6 && (c.y - 0.5).abs() < 1e-6, "c={c:?}");
    }

    #[test]
    fn manifest_round_trips_through_ron() {
        let m = test_manifest();
        let s = ron::to_string(&m).expect("serialize");
        let back: SiteManifest = ron::from_str(&s).expect("deserialize");
        assert_eq!(m, back);
    }
}
