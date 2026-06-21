# 04 — Development Roadmap

Seven iterations, each ending in a shareable video. Durations assume part-time solo dev; scale to taste. Rule: **never start iteration N+1 until N's video is posted** — the cadence is the accountability mechanism.

## It-0 — "Real Moon, my engine" (~2 weeks) ✅ + Sprint 02 follow-up
Geo pipeline produces Shackleton-rim tiles from LRO data; Bevy app streams them with smooth pan/zoom; elevation-based hillshade toggle.
**Video:** flying over real lunar terrain in a from-scratch Rust engine, with the LRO source imagery shown side-by-side.
*Exit:* 60 fps pan/zoom over ≥20×20 km of real terrain.

> **Sprint 02 follow-up (post It-0):** the renderer was rebuilt from the 2D imagery map to
> **perspective 3D over the real DEM** (displaced geometry, per-fragment relief shading,
> synthetic sub-DEM detail), to kill the top-down relief-inversion illusion with genuine
> parallax/occlusion. Runtime imagery draping was dropped; the DEM is the surface. See
> `plans/02-sprint/` and `plans/nice-to-have/resolution-improvements.md`.

## It-1 — "Touchdown" (~2 weeks)
Starship-style lander arrives (simple animation); place 3 module types (hab, solar array, battery) with slope-based buildability; day/night lighting overlay from the real illumination model.
**Video:** landing, placing a base, watching real shadows sweep the site.

## It-2 — "Alive" (~3 weeks)
Sim core online: power + O2 + water networks, 4 crew with needs, fixed-timestep tick, time controls, HUD with resource bars, alert feed. First lose condition.
**Video:** surviving (or dramatically failing) the first shadow window.

## It-3 — "Dig in" (~3 weeks)
Rovers, regolith excavation, PSR ice mining, ISRU chain (regolith→O2+metals, ice→water→propellant), T1 crafting (bricks, pads, berms), workshop + spare parts.
**Video:** full mine→refine→craft→build chain time-lapse.

## It-4 — "The Moon fights back" (~2 weeks)
Dust wear + maintenance loop, solar storms + shelter, micrometeorite leaks, thermal constraints in PSRs. Triage/priority UI.
**Video:** storm-survival sequence.

## It-5 — "Balance sheet" (~3 weeks)
Economy: budget, milestone contracts, lander manifest screen, propellant exports, sustainability score + dependence-ratio graph. Win condition.
**Video:** "from 100% imported to 80% self-sufficient" arc.

## It-6 — "Demo" (~3 weeks)
Save/load, tutorial-ish onboarding contract chain, audio, settings, packaging (Win/macOS/Linux), itch.io or Steam playtest build.
**Video:** trailer cut from It-0…It-5 footage + call for playtesters.

## After the demo (backlog, unordered)
Second site (Mare Tranquillitatis — flat, classic, different challenge profile: full 14-day nights, no PSR ice → different meta), T3 electronics, He-3 endgame, named-crew stories, Steam page, workshop/modding via the RON data layer.

## Standing practices

- Pin Bevy per iteration; upgrade only at iteration boundaries.
- Keep a `dev-clips/` folder; record 30 s after every working feature — trailer material compounds.
- Determinism test in CI from It-2 onward.
