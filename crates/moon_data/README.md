# `moon_data` — shared types

Engine-agnostic types shared between the offline bake and the runtime: the **site manifest**
(CRS, world extent, grid resolution, tile/elevation paths) and the **coordinate transforms**
that map between world metres and DEM texel space. No Bevy dependency.

`geo_pipeline` writes manifests; `moon_game` reads them to place the terrain mesh and drive the
shader's world→DEM-UV mapping, so DEM/imagery registration is data-driven rather than eyeballed.

Per the architecture spec (`specs/03-architecture.md`) this crate will also grow the data layer
for gameplay — building/recipe/contract IDs and the save format — as the sim core lands.
