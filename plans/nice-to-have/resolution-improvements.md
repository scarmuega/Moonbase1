# Nice-to-have — Improving terrain resolution

Ideas for pushing the lunar terrain past its current detail, captured from the Sprint 02
work. Not scheduled — a menu to pull from. Split into **real** (genuinely more measured
detail) and **synthetic** (plausible fabrication for the look).

## Context — where detail comes from today

The terrain is a single GPU-displaced DEM mesh shaded by a custom surface shader
(`assets/shaders/terrain.wgsl`, `crates/moon_game/src/terrain3d.rs`). Two independent
"resolutions" matter:

- **Shading detail** — the fragment stage samples the DEM *per pixel* and computes the
  normal from finite differences, so lighting is crisp at the DEM's native resolution.
- **Geometry / silhouette** — the displaced mesh grid; this is what makes rims read
  against the sky and self-occlude.

Baked DEM native resolution (the hard ceiling on *real* detail):

| Site | DEM | Extent | Native resolution |
| --- | --- | --- | --- |
| shackleton | 3200² | 16 km | **5 m/px** |
| southpole | 3000² | 120 km | **40 m/px** |

Current state: the mesh is matched to the DEM grid, capped at `MAX_GRID_QUADS = 2048`
(near-native, bounded VRAM; flat grid displaced on the GPU so `vexag` stays live and there
are no rebuilds). Synthetic sub-DEM relief is added to the lighting normal via the
`synthetic relief` knob, faded by screen footprint (`fwidth`) to avoid shimmer. The
renderer reads DEM dimensions from the manifest, so **a finer bake "just works"** at
runtime (only the 2048² cap might need revisiting for very large DEMs).

---

## Real detail (more measured data)

### R1. Higher-resolution re-bake — the only true resolution gain ★ recommended
Re-run the geo bake from a finer source DEM. The runtime already adapts to whatever
dimensions the manifest declares.

- **southpole** is the obvious win (currently 40 m/px). LOLA polar DTMs exist at finer
  scales; a re-bake at ~10–20 m/px would be a large, visible jump.
- **shackleton** (5 m/px) could push further from LOLA + LROC-NAC stereo DTMs (~1–5 m for
  the crater).
- Effort: pipeline/data work (source acquisition + warp + tile/DEM bake), not renderer.
  Watch bake time, PNG size, and the 16-bit elevation range. Revisit `MAX_GRID_QUADS` and
  consider mesh LOD (R3) if the DEM grows past a few thousand px per side.
- Risk: source availability/coverage at the pole; storage; bake throughput.

### R2. Baked high-res normal map decoupled from mesh density
Bake a normal map from the *full-resolution* DEM and sample it in the shader, so shading
detail is independent of both mesh density and (if the source normal map is finer than the
displayed DEM) the displayed elevation grid. Mostly redundant with today's per-fragment DEM
sampling unless the normal map is baked from a higher-res source than the displacement DEM
(a "coarse displacement + fine normals" split) — useful as a cheaper alternative to a full
high-res displacement re-bake.

- Effort: medium (bake step + tangent/encoding care).
- Risk: tangent-space encoding correctness; only worth it with a finer-than-DEM source.

### R3. Mesh LOD (quadtree / clipmap) to afford full native geometry
Today's single capped grid is a deliberate simplification (works because the sites are
small). To carry full-native geometry over large/finer DEMs without huge static meshes,
add a distance-based LOD terrain (finer near the camera). This was the deferred Sprint-02
"escape hatch."

- Effort: high. Risk: LOD seams/popping (skirts, geomorphing).
- Only needed once R1 makes the DEM large enough that a uniform 2048² grid is too coarse
  near the camera or too heavy overall.

---

## Synthetic detail (plausible fabrication, for the look)

Already shipped: fractal relief added to the lighting normal (`synthetic relief` slider,
footprint-faded). These are enhancements to it. All are "looks better, isn't real data" —
keep them tunable/disable-able and clearly labeled.

### S1. Better noise basis — ridged / multifractal ★ cheap, high impact
Swap plain value-noise fbm for **ridged multifractal** (and/or domain warping) in
`synth_height`. Ridged noise reads far more like eroded/impact terrain (sharp crests,
smoother basins) than the current rounded fbm. Lowest-effort visual upgrade.

- Effort: small (shader only). Risk: tuning; keep amplitude modest.

### S2. Procedural craterlets
Stamp synthetic small craters (rim + bowl profile) onto the relief for a lunar-specific
look at close range, density/size driven by noise. More convincing than generic fractal
bumps where the DEM is very coarse (southpole).

- Effort: medium (shader; crater SDF/profile + scatter). Risk: can look repetitive/fake;
  needs footprint fade like the existing synth detail.

### S3. Slope/elevation-aware synthesis
Modulate synthetic relief by real DEM slope or elevation (e.g., more detail on steep
crater walls, smoother on basin floors) so the fabrication tracks the real terrain instead
of being uniform. Cheap add-on to S1.

### S4. Detail micro-normal / sparkle (optional)
A very fine high-frequency normal perturbation for a powdery regolith sheen at extreme
close-up. (Note: the broader "dusty albedo" mottle was tried and removed for negligible
impact — keep this strictly a close-range, footprint-gated effect if revisited.)

---

## Suggested order

1. **R1 re-bake southpole finer** — the only change that adds *real* resolution; biggest
   bang, and the renderer needs no changes.
2. **S1 ridged/warped noise** — cheap shader upgrade to the synthetic relief look meanwhile.
3. **S3 slope-aware** synthesis to make S1 track the real terrain.
4. **R3 mesh LOD** only if/when a finer re-bake makes a uniform grid insufficient.
5. **S2 craterlets** / **R2 normal-map split** if still wanting more after the above.
