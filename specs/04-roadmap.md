# 04 — Development Roadmap

Nine sprints, each ending in a shareable video. Durations assume part-time solo dev; scale
to taste. Rule: **never start sprint N+1 until N's video is posted** — the cadence is the
accountability mechanism. Each sprint's detailed plan lives in `plans/<NN>-sprint/`.

## Done

### Sprint 01 — "Real Moon, my engine" (~2 weeks) ✅
Geo pipeline produces Shackleton-rim tiles from LRO data; from-scratch **Bevy 0.18** app
streams them with smooth 2D pan/zoom; elevation-based hillshade toggle. The foundation that
proves the #1 design pillar — *real ground* — before any game systems exist.
**Video:** flying over real lunar terrain in a from-scratch Rust engine, with the LRO source
imagery shown side-by-side.
*Exit:* 60 fps pan/zoom over ≥20×20 km of real terrain. *Plan:* `plans/01-sprint/`.

### Sprint 02 — "Tilted 3D terrain" (~1 week) ✅
Rebuilt the renderer from the 2D imagery map to **perspective 3D over the real DEM** —
displaced geometry, per-fragment relief shading, synthetic sub-DEM detail — killing the
top-down relief-inversion illusion with genuine parallax and occlusion. Runtime imagery
draping was dropped; the DEM *is* the surface.
**Video:** orbiting and diving over real Shackleton relief in true 3D — craters unambiguously
concave from any angle.
*Exit:* 60 fps fly-over of real Shackleton terrain in perspective 3D. *Plans:*
`plans/02-sprint/`, `plans/nice-to-have/resolution-improvements.md`.

## Planned

### Sprint 03 — "Touchdown" (~1–2 weeks) — *current*
The first **game-object layer** over the real terrain: a static lander, three placeable
module types (hab, solar array, battery) with slope-based buildability, click-to-select, and
a details panel — rendered as color-coded primitive 3D meshes, with a perspective ↔ isometric
camera toggle. (The original "Touchdown" scope also bundled the landing animation and the
day/night overlay; those move to Sprint 04.)
**Video:** placing a base — dropping a hab, solar array, and battery onto real slopes, then
selecting each to inspect it.
*Plan:* `plans/03-sprint/`.

### Sprint 04 — "Real shadows" (~1–2 weeks)
The rest of the old "Touchdown" scope: the **day/night lighting overlay from the real
illumination model** (precomputed horizon-mask shadow timetable over the DEM × sun ephemeris,
plus terrain cast shadows) and the **lander's arrival animation** (deferred from Sprint 03's
no-animation scope). This is where the real polar illumination becomes *gameplay* — players
learn which actual ridges hold light.
**Video:** the lander descending onto the site, then real shadows sweeping across the base as
the lunar day turns.

### Sprint 05 — "Alive" (~3 weeks)
Sim core online (`moon_sim`): power + O₂ + water networks, 4 crew with needs, fixed-timestep
tick, time controls (1×/8×/64×/pause), HUD with resource bars, alert feed. First lose
condition.
**Video:** surviving (or dramatically failing) the first shadow window.

### Sprint 06 — "Dig in" (~3 weeks)
Rovers, regolith excavation, PSR ice mining, the ISRU chain (regolith→O₂+metals,
ice→water→propellant), T1 crafting (bricks, pads, berms), workshop + spare parts.
**Video:** full mine→refine→craft→build chain time-lapse.

### Sprint 07 — "The Moon fights back" (~2 weeks)
Dust wear + maintenance loop, solar storms + shelter, micrometeorite leaks, thermal
constraints in PSRs. Triage/priority UI.
**Video:** storm-survival sequence.

### Sprint 08 — "Balance sheet" (~3 weeks)
Economy: budget, milestone contracts, lander manifest screen, propellant exports,
sustainability score + dependence-ratio graph. Win condition.
**Video:** "from 100% imported to 80% self-sufficient" arc.

### Sprint 09 — "Demo" (~3 weeks)
Save/load, tutorial-ish onboarding contract chain, audio, settings, packaging
(Win/macOS/Linux), itch.io or Steam playtest build.
**Video:** trailer cut from Sprint 01…08 footage + call for playtesters.

## After the demo (backlog, unordered)
Second site (Mare Tranquillitatis — flat, classic, different challenge profile: full 14-day
nights, no PSR ice → different meta), T3 electronics, He-3 endgame, named-crew stories, Steam
page, workshop/modding via the RON data layer.

## Standing practices

- Pin Bevy per sprint; upgrade only at sprint boundaries.
- Keep a `dev-clips/` folder; record 30 s after every working feature — trailer material
  compounds.
- Determinism test in CI from Sprint 05 (the sim core) onward.
