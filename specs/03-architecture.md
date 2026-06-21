# 03 — Software Architecture

## Principle

**The simulation is a library; the game is a client of it.** The sim core has zero engine dependencies, runs at a fixed timestep, is deterministic (seeded RNG), and can run headless. This buys: fast unit tests for balance, time-lapse/fast-forward for videos, replay-from-seed debugging, and engine-upgrade insulation.

## Cargo workspace

```
mooncraft/
├─ crates/
│  ├─ moon_sim/        # pure sim: state, systems, recipes, no Bevy
│  ├─ moon_map/        # terrain model: tiles, elevation, illumination, pathfind
│  ├─ moon_game/       # Bevy app: rendering, input, UI, audio
│  └─ moon_data/       # shared types: ids, recipes (RON), save format
├─ tools/
│  └─ geo_pipeline/    # offline: GeoTIFF → game tiles (runs GDAL)
├─ assets/
│  ├─ tiles/<site>/<zoom>/<x>_<y>.png   # baked imagery pyramid (not draped at runtime as of Sprint 02)
│  ├─ elevation/<site>.png              # 16-bit DEM heightmap (loaded as R16Uint) — the runtime surface
│  └─ data/*.ron                        # buildings, recipes, contracts
└─ xtask/              # build automation (bake-tiles, package, etc.)
```

## moon_sim — the core

- **State:** plain structs — `Station { modules, crew, stocks, networks }`, grid-indexed spatial state. No ECS here; the sim is small enough for SoA vectors and the determinism story is simpler.
- **Tick:** `fn tick(&mut SimState, dt: SimMinutes, rng) -> Vec<SimEvent>` at a fixed 1-minute sim step.
- **Order per tick:** illumination update → power network solve → thermal → life support flows → production/crafting jobs → crew needs/agenda → hazards roll → economy/contracts.
- **Networks:** power/O2/water/heat solved as simple supply-priority flow graphs (sources sorted, consumers by priority class; surplus → storage; deficit → shed lowest priority and emit alert events). No general LP solver needed at this scale.
- **Events out, commands in:** UI sends `SimCommand` (place building, set priority, edit manifest); sim emits `SimEvent` (alert, completion, death). This is the only coupling surface.

## moon_map

> *Status:* not yet a separate crate. Today the DEM is loaded directly by `moon_game`'s
> terrain renderer (manifest + DEM via `moon_data`); terrain shading uses a single **live
> sun direction**, not the precomputed timetable below. The illumination timetable, slope
> mask, and pathfinding remain planned for when gameplay lands.

- Loads the baked tile pyramid + elevation grid for the active site.
- **Illumination model:** precomputed per-cell shadow timetable (offline horizon-mask raycast over the real DEM × sun ephemeris) → at runtime a cheap lookup "is cell C lit at time T". This makes the real terrain *gameplay* — players learn which actual ridges hold light.
- Buildability mask from slope (derived from DEM) + A* pathfinding for rovers.

## moon_game (Bevy)

- **Schedule:** render at vsync; sim ticked from an accumulator (supports 1×/8×/64×/pause). ECS entities mirror sim objects via `SimEvent` application — render state is a projection, never the source of truth.
- **Terrain rendering (Sprint 02, current):** the DEM is rendered as **real 3D geometry** — a single GPU-displaced grid mesh (matched to the DEM grid, capped at 2048², vertical-exaggeration uniform) shaded by a **custom surface shader** (`assets/shaders/terrain.wgsl`) that computes the normal *per fragment* from DEM finite differences for crisp relief, plus optional **synthetic sub-DEM relief** (fractal detail in the lighting normal, footprint-faded) to fake resolution below the baked DEM. No streamed imagery drape; warm regolith albedo. The sun is a single live direction (`HillshadeState`) fed to the shader.
  - *Superseded:* the It-0 2D path (sprite tile pyramid streamed by viewport + a `Material2d` hillshade quad) and the brief Tier-2 `StandardMaterial` + `DirectionalLight`/cascaded-shadows experiment. The custom shader trades Bevy's cast shadows for crisp per-fragment relief; analytic DEM-raymarched shadows are the noted follow-up if PSR cast shadows are wanted.
- **Future layers (unbuilt):** lighting overlay (illumination lookup) → buildings/rovers → effects (dust) → egui panels. These will need to sit on the 3D terrain rather than the original 2D map.
- **Camera:** perspective 3D **orbit** rig (drag-rotate / wheel-dolly) over the site; cinematic **3D flythrough** (scripted keyframed orbit/descent) for dev videos. (Was: 2D ortho pan/zoom.)

## Persistence

Save = serialize `SimState` + site id + sim time (postcard, versioned header). Map data is immutable and never saved. Autosave each lunar dawn.

## Data-driven content

Buildings, recipes, contracts, hazard tables = RON files in `assets/data/`, hot-reloadable in dev builds. Balance iteration without recompiles.

## Testing

- moon_sim: scenario tests ("base X survives night Y", "O2 deficit triggers shed order") + golden-master determinism test (same seed → identical state hash after 10k ticks).
- geo_pipeline: snapshot tests on a tiny sample GeoTIFF.
