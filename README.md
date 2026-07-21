# Moonbase 1

**Run the first commercial moon base at the lunar south pole — on the real Moon.**

Moonbase 1 is a crafting / survival-sim about turning imported mass into local self-sufficiency.
Starship-class landers drop hardware and crew at the Shackleton crater rim; your job is to mine
ice, crack regolith for oxygen and metals, survive the shadow windows, and grow the station
until it exports more than it imports. The terrain isn't invented — it's the actual NASA LOLA
elevation model of the south pole, rendered in 3D. **Craters you mine are real craters.**

## The game

You land cargo, deploy modules, and manage the flows that keep a base alive:

```
LAND cargo → DEPLOY/BUILD modules → MANAGE flows (power, O₂, H₂O, heat)
→ MINE regolith & ice → REFINE (ISRU) → CRAFT parts → EXPAND
→ reduce Earth-dependence → unlock exports → repeat at larger scale
```

The Shackleton rim is a gift to game design: ridges in **near-continuous sunlight** (your power
budget) sit beside **permanently shadowed regions** that hold water ice at ~40–100 K (the prize,
in the most hostile terrain). A full lunar day-night cycle runs ~29.5 Earth days — at the pole,
"night" is *your* ridge's shadow window, precomputed from real sun geometry, and surviving it is
the recurring survival event you plan around.

Crafting is the sustainability mechanic: each tier moves a recipe input from *imported* to
*local* — sintered regolith bricks → local metals and spare parts → local electronics and
propellant export. The score is the **import-dependence ratio**: the fraction of consumed mass
that came from Earth. Drive it to zero.

### Design pillars

1. **Real ground.** The terrain is the actual LOLA south-pole DEM, rendered as 3D relief.
2. **Plausible-fun.** Systems mirror real lunar engineering (ISRU, PSR ice, illumination cycles,
   dust) with numbers tuned for play, not papers.
3. **Sustainability as score.** Win by reaching a self-sustaining station that survives a full
   lunar cycle with zero resupply.
4. **Show, don't ship.** Every milestone produces something video-worthy.

The full design lives in [`specs/`](specs/) — [game mechanics](specs/01-game-mechanics.md),
[architecture](specs/03-architecture.md), and the [roadmap](specs/04-roadmap.md).

## Current status

The project is at **iteration It-0 + Sprint 02** of a [seven-iteration roadmap](specs/04-roadmap.md).
What exists today is the *terrain foundation*, not yet the game:

- ✅ **Real-Moon renderer.** An offline pipeline bakes a Shackleton-rim and a pole-wide DEM from
  LRO LOLA data; the Bevy app flies over them in perspective 3D — GPU-displaced DEM geometry,
  per-fragment relief shading, a live sun angle, and a scripted cinematic fly-over. Holds 60 fps.
- 🔜 **Next — It-1 "Touchdown":** a Starship-style lander arrives, you place the first modules
  (hab, solar array, battery) with slope-based buildability, and a day/night lighting overlay
  sweeps real shadows across the site.
- ⏳ **Then:** the deterministic sim core (power/O₂/water networks, crew, time controls), mining
  + ISRU + crafting chains, hazards (dust, solar storms, thermal), and the economy/sustainability
  score that closes the loop.

There is **no gameplay yet** — no landers, modules, crew, or sim. The current build is a
terrain/rendering demo you can fly over.

## Run it

The baked DEM ships via Git LFS, so you can fly over the terrain without re-baking:

```sh
git lfs install && git lfs pull
cargo run -p moon_game --release      # default: the pole-wide south-pole site
```

See [`crates/moon_game`](crates/moon_game/README.md) for controls, sites, and rendering details,
and [`tools/geo_pipeline`](tools/geo_pipeline/README.md) to re-bake the assets from raw NASA data.

## Workspace layout

| Crate / dir | Role |
|---|---|
| [`crates/moon_game`](crates/moon_game/README.md) | The Bevy runtime: camera, terrain rendering, input, UI. |
| [`crates/moon_data`](crates/moon_data/README.md) | Engine-agnostic manifest types + coordinate transforms. |
| [`tools/geo_pipeline`](tools/geo_pipeline/README.md) | Offline GDAL bake: raw GeoTIFFs → DEM heightmap + manifest. |
| [`data/`](data/README.md) | Raw source rasters for the bake (build-time only; gitignored). |
| `specs/` | Game design, architecture, and roadmap. |
| `assets/` | Baked elevation, shaders, and site manifests (elevation in LFS). |

Planned but not yet built: `moon_sim` (the pure, deterministic simulation core) and `moon_map`
(illumination timetable, buildability, pathfinding). See [`specs/03-architecture.md`](specs/03-architecture.md).

## Credits

Moonbase 1 renders publicly released NASA LRO data. See [`CREDITS.md`](CREDITS.md) for required
attribution — please preserve it in any video, screenshot, or derivative work.
