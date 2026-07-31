# 02 — Tech Stack

## Recommendation: Rust + Bevy

| Layer | Choice | Why |
|---|---|---|
| Engine | **Bevy 0.18+** | ECS is a natural fit for a tick-based colony sim; strong 2D path; wgpu rendering; healthy 2026 ecosystem (indie sims have shipped on it). You're proficient in Rust — the main Bevy tax (no mature editor) doesn't hurt a code-first sim with no level design. |
| Rendering | wgpu (via Bevy) | Cross-platform desktop (Win/macOS/Linux) from one codebase. |
| UI | **bevy_egui** for all sim/management panels; Bevy UI only for HUD chrome | egui is immediate-mode — perfect for the data-dense inspector/manifest/flow screens a sim needs, and free dev tooling. |
| Sim math | Plain Rust in an engine-free crate | Determinism + headless tests (see architecture doc). |
| Serialization | serde + RON (configs/recipes) + bincode or postcard (saves) | Recipes/buildings as RON data files → balance without recompiling, moddable later. |
| Geo pipeline (offline) | **GDAL CLI** (`gdal_translate`, `gdalwarp`) driven by a small Rust/`xtask` tool | Crop + reproject GeoTIFFs, build the tile pyramid at build time. GDAL stays out of the shipped game. |
| Image runtime | Bevy asset loader (PNG) | As of Sprint 02 the runtime loads the **16-bit DEM heightmap** (decoded to R16Uint) and renders terrain as 3D geometry; the baked imagery tile pyramid is **not** streamed/draped at runtime. The custom viewport tile-streamer was removed with the move to 3D (kept in git history; revivable if imagery draping returns). |
| Audio | bevy_audio / kira | Ambience matters for video appeal. |
| Dev video | Bevy screenshot API + a time-lapse camera mode | Cheap, recurring content for your progress videos. |

### Why not the alternatives

- **Godot (+ gdext Rust):** faster editor-driven iteration, but splits the codebase across GDScript/scenes and Rust; the sim core is the hard part and it's pure Rust anyway. Choose this only if UI authoring speed becomes the bottleneck.
- **macroquad / comfy / ggez:** delightfully simple, but you'd hand-roll ECS, asset management, and UI that Bevy gives you. Fine for the Iteration-0 terrain demo, wasteful afterward.
- **Unity/Unreal:** strongest tooling, zero Rust leverage, license friction for a solo/indie project.

### Risk & mitigation

Bevy's known cost is **breaking changes between releases** (~3/year). Mitigation: pin per iteration, upgrade only between roadmap milestones, and keep the sim core engine-free so churn touches only the presentation layer.

## Key crates (initial Cargo workspace)

```
bevy, bevy_egui, serde, ron, postcard, rand(+rand_chacha for determinism),
glam, image, thiserror, tracing
dev/pipeline: gdal (or shell out to gdal CLI), xtask pattern
```
