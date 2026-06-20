//! Tile streaming (Sprint 02 · Tier 2): drape the baked imagery pyramid onto the 3D
//! terrain as DEM-displaced, textured patches.
//!
//! The pyramid is shallow (max_zoom 3 ⇒ at most ~49 tiles in a level), so rather than
//! frustum-cull a viewport we simply keep the **whole active-zoom level** resident as
//! `StandardMaterial` patches. The active zoom is chosen from the camera's
//! ground-sample distance (with hysteresis so a parked zoom doesn't thrash); zooming in
//! loads a finer level, zooming out a coarser one. Patches at one zoom share exact edge
//! heights (same DEM, same world coords), so they meet without cracks — no skirts.
//!
//! Two systems cooperate, mirroring It-0's discipline:
//! - [`stream_tiles`] (throttled) picks the LOD, diffs the desired vs. resident set,
//!   despawns stale-zoom patches, and queues spawns. It also rebuilds everything when
//!   the vertical-exaggeration changes (the displacement is baked into the geometry).
//! - [`drain_spawn_queue`] spawns at most `SPAWN_CAP` patches per frame (bounds the
//!   per-frame mesh build + GPU upload), each a displaced grid textured with its tile.
//!
//! Until the DEM finishes decoding into [`DemHeights`] no patches are built; the scene
//! shows the clear color for that brief window.

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use moon_data::{tile_path, tile_to_world_min, tile_world_size, TileCoord};

use crate::camera::select_lod;
use crate::terrain3d::{build_patch_mesh, CameraRig, DemHeights, TerrainLook, PATCH_RES};
use crate::Site;

/// How often the visible-set diff runs (the camera changes the tile set far less than
/// 60×/s, so a 10 Hz pass avoids churn).
const STREAM_PERIOD_SECS: f32 = 0.1;
/// Max patches built + spawned per frame (bounds mesh build + GPU upload cost).
const SPAWN_CAP: usize = 4;
/// Fractional GSD margin a zoom boundary must be crossed by before the active LOD
/// switches — the hysteresis deadband (±15%).
const HYSTERESIS: f32 = 0.15;
/// Subtle warm grade multiplied onto the gray imagery (matches the It-0 mood).
const IMAGERY_TINT: Color = Color::srgb(1.0, 0.96, 0.90);

/// Resident terrain patches, keyed by tile coordinate.
#[derive(Resource, Default)]
pub struct LoadedTiles {
    pub map: HashMap<TileCoord, Entity>,
}

/// Streaming bookkeeping: throttle timer, current LOD (held with hysteresis), the
/// bounded spawn queue, and the vexag the resident geometry was built at.
#[derive(Resource)]
pub struct Streamer {
    timer: Timer,
    active_zoom: u8,
    queue: VecDeque<TileCoord>,
    last_vexag: f32,
    last_imagery: bool,
}

impl Streamer {
    pub fn new(active_zoom: u8) -> Self {
        Self {
            timer: Timer::from_seconds(STREAM_PERIOD_SECS, TimerMode::Repeating),
            active_zoom,
            queue: VecDeque::new(),
            last_vexag: f32::NAN,
            last_imagery: true,
        }
    }
}

/// Marks a streamed terrain patch with its coordinate.
#[derive(Component)]
struct StreamedTile(#[allow(dead_code)] TileCoord);

/// Pick the LOD, diff the desired vs. resident set, despawn stale, queue spawns; and
/// rebuild everything when the vertical exaggeration or imagery toggle changes.
#[allow(clippy::too_many_arguments)]
pub fn stream_tiles(
    time: Res<Time>,
    site: Res<Site>,
    rig: Res<CameraRig>,
    look: Res<TerrainLook>,
    mut streamer: ResMut<Streamer>,
    mut loaded: ResMut<LoadedTiles>,
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if !streamer.timer.tick(time.delta()).just_finished() {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let manifest = &site.0;

    // Vexag changed (geometry stale) or imagery toggled (material stale) → rebuild all.
    if streamer.last_vexag != rig.vexag || streamer.last_imagery != look.imagery {
        for (_, e) in loaded.map.drain() {
            commands.entity(e).despawn();
        }
        streamer.queue.clear();
        streamer.last_vexag = rig.vexag;
        streamer.last_imagery = look.imagery;
    }

    // --- LOD selection with hysteresis (deadband around the mpp boundary) ---
    let gsd = rig.ground_sample_distance(window.size().y);
    let raw = select_lod(manifest, gsd);
    let active = streamer.active_zoom;
    if raw > active {
        if select_lod(manifest, gsd * (1.0 + HYSTERESIS)) > active {
            streamer.active_zoom = raw;
        }
    } else if raw < active && select_lod(manifest, gsd * (1.0 - HYSTERESIS)) < active {
        streamer.active_zoom = raw;
    }
    let z = streamer.active_zoom;

    let Some(level) = manifest.zoom_level(z) else {
        return;
    };

    // --- Despawn: anything not at the active zoom ---
    let stale: Vec<(TileCoord, Entity)> = loaded
        .map
        .iter()
        .filter(|(c, _)| c.zoom != z)
        .map(|(c, e)| (*c, *e))
        .collect();
    for (c, e) in stale {
        commands.entity(e).despawn();
        loaded.map.remove(&c);
    }

    // --- Queue spawns: the whole active level, minus what's resident/queued ---
    streamer
        .queue
        .retain(|c| c.zoom == z && !loaded.map.contains_key(c));
    let queued: std::collections::HashSet<TileCoord> = streamer.queue.iter().copied().collect();
    for x in 0..level.tiles_x {
        for y in 0..level.tiles_y {
            let c = TileCoord::new(z, x, y);
            if !loaded.map.contains_key(&c) && !queued.contains(&c) {
                streamer.queue.push_back(c);
            }
        }
    }
}

/// Drain up to [`SPAWN_CAP`] queued tiles per frame: build the displaced patch mesh and
/// spawn it textured with the tile image. Skips until the DEM cache is ready.
#[allow(clippy::too_many_arguments)]
pub fn drain_spawn_queue(
    site: Res<Site>,
    rig: Res<CameraRig>,
    look: Res<TerrainLook>,
    dem: Option<Res<DemHeights>>,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut streamer: ResMut<Streamer>,
    mut loaded: ResMut<LoadedTiles>,
    mut commands: Commands,
) {
    let Some(dem) = dem else {
        return;
    };
    let manifest = &site.0;
    for _ in 0..SPAWN_CAP {
        let Some(c) = streamer.queue.pop_front() else {
            break;
        };
        if loaded.map.contains_key(&c) {
            continue;
        }
        let size = tile_world_size(manifest, c.zoom);
        let min = tile_to_world_min(manifest, c);
        let max = min + Vec2::splat(size);

        let mesh = meshes.add(build_patch_mesh(&dem, min, max, PATCH_RES, rig.vexag));
        // Imagery on → draped tile texture; off → plain warm-gray regolith so the sun +
        // shadows render the pure relief (the Tier-1 look).
        let material = materials.add(if look.imagery {
            StandardMaterial {
                base_color: IMAGERY_TINT,
                base_color_texture: Some(asset_server.load(tile_path(&manifest.site, c))),
                perceptual_roughness: 1.0,
                metallic: 0.0,
                ..default()
            }
        } else {
            StandardMaterial {
                base_color: Color::srgb(0.82, 0.79, 0.74),
                perceptual_roughness: 1.0,
                metallic: 0.0,
                ..default()
            }
        });
        let entity = commands
            .spawn((Mesh3d(mesh), MeshMaterial3d(material), StreamedTile(c)))
            .id();
        loaded.map.insert(c, entity);
    }
}
