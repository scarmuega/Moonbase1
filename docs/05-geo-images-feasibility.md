# 05 — Real Geo-Imagery: Feasibility & Sources

> **Runtime status (Sprint 02).** The game now renders terrain from the **LOLA DEM as 3D
> geometry** and does **not** drape the LROC surface imagery at runtime — draped tiles read
> as isolated, low-value patches that fought the relief and carried baked-in shadows. The
> imagery pipeline/sources below remain valid and the baked tiles are kept (possible future
> use: context layers, a textured close-up mode, or a higher-res re-bake), but the surface
> look currently comes from DEM relief shading + synthetic detail. This makes challenge #5
> (PSR interiors black in imagery → use LOLA shading) effectively the whole approach. The
> DEM resolution is now the binding constraint — see `plans/nice-to-have/resolution-improvements.md`.

## Verdict: highly feasible

NASA/USGS lunar data is **public domain** (US government work), high resolution, well-documented, and already packaged for creators. The only real work is a one-time offline pipeline. Caveat: NASA imagery use must not imply NASA endorsement; credit "NASA/GSFC/Arizona State University" (LROC) — standard and easy.

## Sources, best-fit first

| Source | Resolution | What it gives us |
|---|---|---|
| **LROC NAC** (RDR products via [lroc.im-ldi.com](https://lroc.im-ldi.com/) / QuickMap) | **0.5–2 m/px** | The hero asset: photographic detail where individual boulders are visible. Grayscale. Site-sized frames (5 km swaths) — exactly what a single-site game needs. |
| **LROC WAC global mosaic** ([USGS Astrogeology](https://astrogeology.usgs.gov/search/map/Moon/LRO/LROC_WAC/Lunar_LRO_LROC-WAC_Mosaic_global_100m_June2013)) | 100 m/px | Zoomed-out context layer and site-selection map. |
| **LOLA DEM** ([118 m global](https://astrogeology.usgs.gov/search/map/moon_lro_lola_dem_118m), SLDEM2015 ~59 m, [south-pole products to ~5 m](https://pgda.gsfc.nasa.gov/products/90)) | 5–118 m/px | Elevation → slope/buildability, hillshade, and the precomputed illumination timetable. The 5 m south-pole LOLA DEM is ideal for Shackleton. |
| **NASA CGI Moon Kit** ([svs.gsfc.nasa.gov/4720](https://svs.gsfc.nasa.gov/4720)) | global color + displacement TIFFs | Explicitly published for artists/games; quickest possible start for It-0 before the NAC pipeline exists. |
| **Moon Trek WMTS API** ([trek.nasa.gov](https://trek.nasa.gov/tiles/apidoc/trekAPI.html?body=moon)) | tiled, many layers | Standard `{zoom}/{row}/{col}.png` tile service — useful for prototyping the streaming code and for bulk-fetching layers. Ship baked local tiles, don't depend on the service at runtime. |

## Known challenges → mitigations

1. **NAC is grayscale.** Fine — the Moon *is* gray. Add a subtle game-side tint/grade for mood; color authenticity is a feature ("this is the actual surface").
2. **Baked-in lighting.** NAC frames have real shadows at capture time, which will fight the dynamic day/night overlay. Mitigation: pick frames with high sun angle (flat lighting) where available; treat residual shadows as albedo texture; the multiply-darkness overlay dominates perceptually at gameplay zoom.
3. **Projection at the pole.** Equirectangular products distort badly near 89°S. Pipeline must `gdalwarp` to **polar stereographic** centered on the site. Solved problem in GDAL.
4. **File sizes.** A 20×20 km site at 1 m/px = 400 Mpx ≈ manageable as a tile pyramid (~300–600 MB PNG; less with KTX2/BCn compression). Global data never ships — only baked site tiles.
5. **PSR interiors are black in imagery.** By definition. Use LOLA elevation-derived shading inside PSRs — also reads as great game-feel ("you only see what you light").
6. **Gaps/seams in NAC coverage.** Mosaicking artifacts exist; acceptable for a game, patchable by hand for the one site we ship.

## Pipeline (one-time per site, automated in `tools/geo_pipeline`)

```
1. Fetch: NAC RDR GeoTIFFs for site bbox + LOLA DEM crop (+ WAC for context zooms)
2. gdalwarp → polar stereographic, common grid, 1 m/px (imagery) & 5–20 m/px (DEM)
3. Bake illumination timetable: horizon-mask × sun ephemeris → per-cell lit/shadow schedule
4. Derive: slope mask, hillshade, PSR mask
5. Tile: 512×512 pyramid, 3–4 zoom levels → assets/tiles/shackleton/
```

## SpaceX flavor (design reference, not assets)

Starship HLS / "Moonbase Alpha" concepts inform lander silhouette, ~100 t delivery scale, propellant-export economy, and marketing tone. **Use as inspiration only** — SpaceX renders and trademarks are not licensable assets; our lander is an original "Starship-class" design. NASA's published [Artemis/HLS concept material](https://www.nasa.gov/humans-in-space/nasa-spacex-illustrate-key-moments-of-artemis-lunar-lander-mission/) is safe visual reference.

## Sources

- [LROC site / curated downloads](https://lroc.im-ldi.com/images/downloads)
- [USGS Astrogeology — WAC global mosaic](https://astrogeology.usgs.gov/search/map/Moon/LRO/LROC_WAC/Lunar_LRO_LROC-WAC_Mosaic_global_100m_June2013)
- [USGS Astrogeology — LOLA DEM 118 m](https://astrogeology.usgs.gov/search/map/moon_lro_lola_dem_118m)
- [PGDA — South Pole LOLA products](https://pgda.gsfc.nasa.gov/products/90) · [SLDEM2015](https://pgda.gsfc.nasa.gov/products/54)
- [NASA CGI Moon Kit](https://svs.gsfc.nasa.gov/4720)
- [Moon Trek tile API](https://trek.nasa.gov/tiles/apidoc/trekAPI.html?body=moon)
