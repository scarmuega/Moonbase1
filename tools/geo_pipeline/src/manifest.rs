//! Build and write the `moon_data::SiteManifest` — the contract the game reads.
//!
//! The only non-trivial step is **recentering**: GDAL emits a projected bbox in meters
//! (potentially far from the origin), but the game's World space puts the origin at the
//! bbox center to keep float precision tight. We translate the projected extent by its
//! own center to get `world_bbox`.

use std::path::Path;

use moon_data::{DemInfo, SiteId, SiteManifest, SunDefaults, ZoomLevel};

/// Assemble the manifest from the values the bake measured.
#[allow(clippy::too_many_arguments)]
pub fn build_manifest(
    site: SiteId,
    projected_bbox: [f64; 4],
    base_mpp: f64,
    tile_size_px: u32,
    zoom_levels: Vec<ZoomLevel>,
    dem: DemInfo,
    sun: SunDefaults,
    crs_proj4: String,
) -> SiteManifest {
    let [min_x, min_y, max_x, max_y] = projected_bbox;
    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    // Recenter to origin, narrowing to f32 (extents are ≤ tens of km — well within f32).
    let world_bbox = [
        (min_x - cx) as f32,
        (min_y - cy) as f32,
        (max_x - cx) as f32,
        (max_y - cy) as f32,
    ];

    SiteManifest {
        site,
        projected_bbox,
        world_bbox,
        base_mpp,
        tile_size_px,
        zoom_levels,
        dem,
        sun,
        crs_proj4,
    }
}

/// Serialize the manifest to `<out_root>/sites/<site>.ron` (pretty, for human review).
pub fn write_manifest(manifest: &SiteManifest, out_root: &Path) -> std::io::Result<std::path::PathBuf> {
    let dir = out_root.join("sites");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.ron", manifest.site));
    let pretty = ron::ser::PrettyConfig::default();
    let text = ron::ser::to_string_pretty(manifest, pretty)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&path, text)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SiteManifest {
        build_manifest(
            SiteId::shackleton(),
            // Projected extent NOT centered on origin, to exercise recentering.
            [-10_000.0, -10_000.0, 10_000.0, 10_000.0],
            40.0,
            512,
            vec![
                ZoomLevel { zoom: 0, mpp: 40.0, tiles_x: 1, tiles_y: 1 },
                ZoomLevel { zoom: 3, mpp: 5.0, tiles_x: 8, tiles_y: 8 },
            ],
            DemInfo {
                path: "elevation/shackleton.png".to_string(),
                width: 4000,
                height: 4000,
                elev_min_m: -1234.5,
                elev_max_m: 678.9,
            },
            SunDefaults { azimuth_deg: 45.0, altitude_deg: 1.5 },
            "+proj=stere +lat_0=-90 +lat_ts=-90 +lon_0=0 +R=1737400 +units=m +no_defs".to_string(),
        )
    }

    #[test]
    fn recenters_projected_bbox_to_origin() {
        let m = sample();
        // Symmetric input was already centered; world center must be the origin.
        assert_eq!(m.world_bbox, [-10_000.0, -10_000.0, 10_000.0, 10_000.0]);
    }

    #[test]
    fn recenters_offset_bbox() {
        // A bbox far from origin (e.g. a non-pole-centered site) still lands centered.
        let m = build_manifest(
            SiteId::shackleton(),
            [100_000.0, 200_000.0, 120_000.0, 220_000.0],
            40.0,
            512,
            vec![],
            DemInfo { path: "e.png".into(), width: 1, height: 1, elev_min_m: 0.0, elev_max_m: 1.0 },
            SunDefaults { azimuth_deg: 0.0, altitude_deg: 0.0 },
            "proj".into(),
        );
        assert_eq!(m.world_bbox, [-10_000.0, -10_000.0, 10_000.0, 10_000.0]);
    }

    /// Verification criterion 3: the manifest we write parses back via `ron::from_str`.
    #[test]
    fn manifest_round_trips_through_ron() {
        let m = sample();
        let pretty = ron::ser::to_string_pretty(&m, ron::ser::PrettyConfig::default()).unwrap();
        let back: SiteManifest = ron::from_str(&pretty).unwrap();
        assert_eq!(m, back);
    }
}
