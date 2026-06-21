# 01 — Game Mechanics

## Setting

Shackleton crater rim, lunar south pole — the real Artemis / SpaceX target zone. It's a gift to game design:

- **Rim ridges get ~80–90% sunlight** → near-continuous solar power, but with predictable outage windows (the night-survival challenge).
- **Permanently shadowed regions (PSRs) hold water ice** at ~40–100 K → the prize resource sits in the most hostile terrain.
- **Real elevation drama** — 4 km deep crater next to "peaks of eternal light" → terrain meaningfully constrains placement.

## Core loop

```
LAND cargo → DEPLOY/BUILD modules → MANAGE flows (power, O2, H2O, heat)
→ MINE regolith & ice → REFINE (ISRU) → CRAFT parts → EXPAND
→ reduce Earth-dependence → unlock exports → repeat at larger scale
```

Session rhythm: short-term firefighting (a dust storm of alerts during the night cycle) layered over long-term base planning (RimWorld/Factorio cadence).

## Time

1 real second ≈ 1 game minute (adjustable 1×/8×/64×, pause). A full lunar day-night cycle (~29.5 Earth days) ≈ 7 hours at 1× — nights are survival events you prepare for, not background flavor. At the polar site, "night" means your specific ridge's shadow windows, precomputed from real sun geometry.

## Resources

### Flows (per-tick rates through a network)
| Resource | Produced by | Consumed by |
|---|---|---|
| Power (kW) | Solar arrays, fuel cells, (late) fission | Everything |
| Heat | Electronics, RTG waste heat | Radiators must dump it; night needs it |
| Oxygen | Regolith electrolysis, water splitting | Crew, propellant mix |
| Water | PSR ice mining, fuel-cell exhaust | Crew, greenhouse, electrolysis |

### Stocks (stored in tanks/silos, hauled by rovers)
| Resource | Source | Use |
|---|---|---|
| Regolith | Surface excavation | O2 feedstock, sintered bricks, radiation shielding berms |
| Ice | PSR mining (dark, cold, power-hungry) | Water |
| Metals (Fe/Al/Ti/Si) | Regolith refining byproduct | Structures, panels, spare parts |
| Propellant (LOX/LH2) | Electrolysis + liquefaction | Hoppers, **export commodity** |
| Volatiles/food | Greenhouse, Earth import | Crew |
| Spare parts | Workshop (metals + imported electronics) | Maintenance |
| Electronics | **Import only** (until late game) | Everything advanced |

Real anchor points (tuned, not exact): regolith is ~40–45% oxygen by mass; molten regolith electrolysis yields O2 + metal alloys; PSR ice content ~5% by mass in ore.

## Crafting tiers

- **T0 — Imported.** Everything arrives by lander. Habs, panels, rovers. Mass-limited (~100 t per Starship-class delivery) and budget-limited.
- **T1 — Regolith.** Sintered bricks, landing pads, roads, shielding berms. First dependence-ratio wins.
- **T2 — Metals.** Local structural parts, spare parts, basic solar panels from Si/Al. Maintenance becomes self-sufficient.
- **T3 — Advanced.** Local electronics (Si refining), propellant export at scale, He-3 extraction as endgame flavor.

Each tier shifts a recipe input from "imported" to "local" — the crafting tree *is* the sustainability mechanic.

## Hazards & maintenance

- **Night/shadow windows** — solar drops to zero; survive on batteries/fuel cells or shed load (triage UI).
- **Dust** — every excavation/drive raises dust; machines accrue wear → spare-part demand. The Apollo-real "dust is the enemy" loop.
- **Solar storms** — random warning (~hours of game time) → crew must shelter under regolith shielding; unshielded EVA = injury.
- **Micrometeorites** — rare random damage; pressurized modules can leak (O2 drain until patched).
- **Thermal** — equipment in PSRs needs heaters; daytime equipment needs radiators.

## Crew

Small named crew (4–12). Needs: O2, water, calories, sleep, morale (variety of food, comms with Earth, workload). Crew are assignment-driven (job priorities), not micromanaged. Death is possible but telegraphed.

## Economy

- **Budget ($)** earned from: NASA-style milestone contracts ("demonstrate 1 t of local O2"), propellant exports to orbital depots, science data from PSR cores.
- **Spent on:** lander deliveries — you choose the manifest, mass-capped. The manifest screen is a key strategic moment.
- **Sustainability score** = 1 − (imported mass consumed / total mass consumed), rolling window. Drives contract unlocks and the win condition: a fully self-sustaining station surviving a full lunar cycle with zero deliveries.

## Win/lose

Lose: crew all dead or bankrupt with a dead base. Win (campaign): sustainability ≥ 95% + crew ≥ N + survive a no-resupply lunar cycle. Sandbox continues forever.
