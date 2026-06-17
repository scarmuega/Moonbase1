//! Tile streaming (plan 04): load only the tiles the camera can see, at the LOD
//! that matches the current zoom, and keep it hitch-free at ~60 fps.
//!
//! Two systems cooperate:
//! - [`stream_tiles`] runs on a ~100 ms throttle. It picks the active LOD (with
//!   hysteresis so a scroll parked on a boundary doesn't thrash), computes the
//!   visible tile rectangle plus a 1-tile prefetch ring, and diffs that against
//!   the resident set — queueing spawns and despawning tiles that fell outside a
//!   wider grace ring.
//! - [`drain_spawn_queue`] runs every frame and spawns at most `SPAWN_CAP` tiles,
//!   so a fast zoom-out can't queue dozens of GPU uploads into a single frame.
//!
//! All geometry comes from [`moon_data`] — nothing here is hardcoded to the bake.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use moon_data::{tile_path, tile_to_world_min, tile_world_size, world_to_tile, TileCoord};

use crate::camera::select_lod;
use crate::Site;

/// How often the visible-set diff runs. Panning changes the tile set far less
/// than 60×/s, so a 10 Hz pass is plenty and avoids load/unload churn.
const STREAM_PERIOD_SECS: f32 = 0.1;
/// Max tiles spawned per frame (bounds entity-spawn + GPU upload cost).
const SPAWN_CAP: usize = 4;
/// Tiles loaded beyond the visible rect so they're ready before they scroll on.
const PREFETCH_RING: u32 = 1;
/// Tiles kept resident beyond the prefetch ring (reused by oscillating pans).
const GRACE_RING: u32 = 2;
/// Fractional scale margin a zoom boundary must be crossed by before the active
/// LOD switches — the hysteresis deadband (±15%).
const HYSTERESIS: f32 = 0.15;
/// Subtle warm color grade multiplied onto the gray imagery (plan 06 polish). A
/// light tint reads as intentional mood rather than fake detail; set to
/// `Color::WHITE` to disable. Keep in step with the relief tint in `hillshade.wgsl`.
const IMAGERY_TINT: Color = Color::srgb(1.0, 0.96, 0.90);

/// The resident tile entities, keyed by coordinate. Bounded by the visible rect
/// plus the grace ring at one zoom, so it never grows without limit.
#[derive(Resource, Default)]
pub struct LoadedTiles {
    pub map: HashMap<TileCoord, Entity>,
}

/// Streaming bookkeeping: the throttle timer, the current LOD (held with
/// hysteresis), and the bounded spawn queue.
#[derive(Resource)]
pub struct Streamer {
    timer: Timer,
    active_zoom: u8,
    queue: VecDeque<TileCoord>,
}

impl Streamer {
    /// Seed the active LOD from the camera's initial target scale.
    pub fn new(active_zoom: u8) -> Self {
        Self {
            timer: Timer::from_seconds(STREAM_PERIOD_SECS, TimerMode::Repeating),
            active_zoom,
            queue: VecDeque::new(),
        }
    }
}

/// Marks a streamed tile sprite with its coordinate (handy for debugging/queries).
#[derive(Component)]
struct StreamedTile(#[allow(dead_code)] TileCoord);

/// Throttled visible-set diff: pick the LOD, compute desired/keep rects, queue
/// spawns, and despawn tiles that left the grace ring or belong to a stale LOD.
pub fn stream_tiles(
    time: Res<Time>,
    site: Res<Site>,
    mut streamer: ResMut<Streamer>,
    mut loaded: ResMut<LoadedTiles>,
    mut commands: Commands,
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Transform, &Projection), With<Camera2d>>,
) {
    if !streamer.timer.tick(time.delta()).just_finished() {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((transform, Projection::Orthographic(ortho))) = camera.single() else {
        return;
    };

    let manifest = &site.0;
    let scale = ortho.scale;

    // --- LOD selection with hysteresis (deadband around the mpp boundary) ---
    let raw = select_lod(manifest, scale);
    let active = streamer.active_zoom;
    if raw > active {
        // Want finer (zoomed in): confirm even after biasing the scale coarser.
        if select_lod(manifest, scale * (1.0 + HYSTERESIS)) > active {
            streamer.active_zoom = raw;
        }
    } else if raw < active {
        // Want coarser (zoomed out): confirm even after biasing the scale finer.
        if select_lod(manifest, scale * (1.0 - HYSTERESIS)) < active {
            streamer.active_zoom = raw;
        }
    }
    let z = streamer.active_zoom;

    let Some(level) = manifest.zoom_level(z) else {
        return;
    };
    let (nx, ny) = (level.tiles_x, level.tiles_y);

    // --- Visible AABB → tile rect (same half-extent convention as the clamp) ---
    let half = window.size() * 0.5 * scale;
    let cam = transform.translation.truncate();
    let min_t = world_to_tile(manifest, cam - half, z);
    let max_t = world_to_tile(manifest, cam + half, z);

    // Expand the visible rect by `ring` tiles on every side, clamped to the grid.
    let expand = |ring: u32| -> HashSet<TileCoord> {
        let x0 = min_t.x.saturating_sub(ring);
        let y0 = min_t.y.saturating_sub(ring);
        let x1 = (max_t.x + ring).min(nx.saturating_sub(1));
        let y1 = (max_t.y + ring).min(ny.saturating_sub(1));
        let mut set = HashSet::new();
        for x in x0..=x1 {
            for y in y0..=y1 {
                set.insert(TileCoord::new(z, x, y));
            }
        }
        set
    };
    let desired = expand(PREFETCH_RING);
    let keep = expand(GRACE_RING);

    // --- Despawn: stale LOD, or fell outside the grace ring ---
    let to_despawn: Vec<(TileCoord, Entity)> = loaded
        .map
        .iter()
        .filter(|(c, _)| c.zoom != z || !keep.contains(c))
        .map(|(c, e)| (*c, *e))
        .collect();
    for (c, e) in to_despawn {
        commands.entity(e).despawn();
        loaded.map.remove(&c);
    }

    // --- Queue spawns: desired tiles not resident and not already queued ---
    streamer
        .queue
        .retain(|c| desired.contains(c) && !loaded.map.contains_key(c));
    let queued: HashSet<TileCoord> = streamer.queue.iter().copied().collect();
    for c in &desired {
        if !loaded.map.contains_key(c) && !queued.contains(c) {
            streamer.queue.push_back(*c);
        }
    }
}

/// Drain up to [`SPAWN_CAP`] queued tiles per frame: load the PNG (decoded
/// off-thread on the async IO pool) and spawn the sprite at its world position.
pub fn drain_spawn_queue(
    site: Res<Site>,
    asset_server: Res<AssetServer>,
    mut streamer: ResMut<Streamer>,
    mut loaded: ResMut<LoadedTiles>,
    mut commands: Commands,
) {
    let manifest = &site.0;
    for _ in 0..SPAWN_CAP {
        let Some(c) = streamer.queue.pop_front() else {
            break;
        };
        if loaded.map.contains_key(&c) {
            continue;
        }
        let size = tile_world_size(manifest, c.zoom);
        let center = tile_to_world_min(manifest, c) + Vec2::splat(size * 0.5);
        let entity = commands
            .spawn((
                Sprite {
                    image: asset_server.load(tile_path(&manifest.site, c)),
                    custom_size: Some(Vec2::splat(size)),
                    color: IMAGERY_TINT,
                    ..default()
                },
                Transform::from_translation(center.extend(c.zoom as f32)),
                StreamedTile(c),
            ))
            .id();
        loaded.map.insert(c, entity);
    }
}
