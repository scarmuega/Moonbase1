# Mooncraft — Spec

A crafting / sim game about building a sustainable moon station, set on **real lunar terrain** — the actual LOLA elevation model rendered in 3D.

> **Rendering update (Sprint 02).** The view moved from the original top-down 2D imagery
> map to a **perspective 3D camera over real DEM terrain** (displaced geometry, per-fragment
> relief shading). Streamed surface imagery is **not** draped at runtime — the elevation
> model is the surface. See [03-architecture.md](03-architecture.md) and `plans/02-sprint/`.

**Pitch:** You run the first commercial moon base at the lunar south pole. Starship-class landers drop hardware and crew; your job is to turn imported mass into local self-sufficiency — mine ice, crack regolith for oxygen and metals, and grow the station until it exports more than it imports.

## Design pillars

1. **Real ground.** The terrain is the actual LOLA elevation model of the Shackleton crater rim, rendered as 3D relief. Craters you mine are real craters.
2. **Plausible-fun.** Systems mirror real lunar engineering (ISRU, PSR ice, illumination cycles, dust) with numbers tuned for play, not papers.
3. **Sustainability as score.** The core metric is the import-dependence ratio: % of consumed mass that came from Earth. Drive it to zero.
4. **Show, don't ship.** Every milestone produces something video-worthy.

## Documents

| File | Contents |
|---|---|
| [01-game-mechanics.md](01-game-mechanics.md) | Core loop, resources, crafting chains, hazards, economy |
| [02-tech-stack.md](02-tech-stack.md) | Recommended stack (Rust/Bevy) and alternatives |
| [03-architecture.md](03-architecture.md) | Crate layout, sim core design, data flow |
| [04-roadmap.md](04-roadmap.md) | 7 video-friendly iterations |
| [05-geo-images-feasibility.md](05-geo-images-feasibility.md) | Imagery sources, licensing, pipeline |
| [wireframe-main-screen.svg](wireframe-main-screen.svg) | Main screen UI wireframe |

## Decisions locked so far

3D perspective view over real DEM terrain (Sprint 02 — superseded the original top-down 2D imagery approach) · plausible-fun realism · MVP = single site (Shackleton rim) with the power/oxygen/regolith loop · desktop-first · Rust.
