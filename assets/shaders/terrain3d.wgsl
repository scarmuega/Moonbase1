// Terrain3d (Sprint 02 · Tier 1). Renders the baked LOLA DEM as real 3D geometry:
// a flat XZ grid displaced in the *vertex* stage by the heightmap, shaded in the
// *fragment* stage with the same sun + Lambertian math as the 2D `hillshade.wgsl`.
//
// Coordinate mapping (fixed for Sprint 02): world `(x, y, height_m)` with +Y = north
// maps to Bevy 3D as `(x, height_m * vexag, -y)`. So the ground is the XZ plane, +Y is
// up, and `world_xy = (position.x, -position.z)`. DEM UV is then computed exactly as
// `moon_data::world_to_dem_uv` / `hillshade.wgsl`: `u = (x-min_x)/span_x`,
// `v = (max_y - y)/span_y` (the v-flip puts north on top).
//
// The DEM is a 16-bit grayscale PNG that Bevy loads as an *integer* texture (`R16Uint`),
// so it's read with `textureLoad` (no sampler) in both stages (`textureLoad` /
// `textureDimensions` take no derivatives, so they're legal in the vertex stage).
//
// The lighting/decode/ramp block below is duplicated from `hillshade.wgsl` (the 2D
// `mesh2d` and 3D `pbr` import paths differ, so a shared `#import` is awkward). Keep
// the two in sync — the math is identical; only the geometry source differs.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip
#import bevy_pbr::forward_io::Vertex

struct Terrain3dParams {
    // World-space bbox of the DEM (= manifest world_bbox), as min corner + size.
    dem_world_min: vec2<f32>,
    dem_world_size: vec2<f32>,
    // Elevation decode bounds (m): elev = elev_min + sample * (elev_max - elev_min).
    elev_min: f32,
    elev_max: f32,
    // Sun direction in radians (azimuth = compass bearing, altitude above horizon).
    sun_azimuth: f32,
    sun_altitude: f32,
    // Vertical exaggeration applied to geometry *and* the shading normal.
    vexag: f32,
    // 0 = hillshade relief, 1 = draped imagery (relit), 2 = height-ramp debug.
    surface_mode: u32,
};

// In Bevy's 3D pipeline group 0 = view, group 1 = lights, group 2 = mesh; the custom
// material's `AsBindGroup` lands in the next group, exposed as `#{MATERIAL_BIND_GROUP}`
// (hardcoding @group(2) would collide with `bevy_pbr::mesh_bindings`).
@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: Terrain3dParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var dem: texture_2d<u32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var imagery: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var imagery_sampler: sampler;

// 16-bit unsigned max — normalizes the stored R16Uint sample to [0, 1].
const U16_MAX: f32 = 65535.0;
// Warm color grade matching IMAGERY_TINT (streaming.rs) and hillshade.wgsl.
const TINT: vec3<f32> = vec3<f32>(1.0, 0.96, 0.90);

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_xy: vec2<f32>,
};

// world -> DEM UV in [0, 1], top-left origin (identical to moon_data::world_to_dem_uv).
fn world_to_dem_uv(world: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        (world.x - p.dem_world_min.x) / p.dem_world_size.x,
        (p.dem_world_min.y + p.dem_world_size.y - world.y) / p.dem_world_size.y,
    );
}

fn world_to_texel(world: vec2<f32>, dims: vec2<i32>) -> vec2<i32> {
    return vec2<i32>(floor(world_to_dem_uv(world) * vec2<f32>(dims)));
}

// Decode elevation (meters) at an integer DEM texel, clamped to the texture.
fn height_at(coord: vec2<i32>, dims: vec2<i32>) -> f32 {
    let c = clamp(coord, vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
    let s = f32(textureLoad(dem, c, 0).r) / U16_MAX;
    return p.elev_min + s * (p.elev_max - p.elev_min);
}

fn height_at_world(world: vec2<f32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(dem));
    return height_at(world_to_texel(world, dims), dims);
}

// Debug height ramp (surface_mode 2): low/deep = blue, high = red.
fn height_ramp(t: f32) -> vec3<f32> {
    let x = clamp(t, 0.0, 1.0);
    let blue   = vec3<f32>(0.0, 0.0, 1.0);
    let cyan   = vec3<f32>(0.0, 1.0, 1.0);
    let green  = vec3<f32>(0.0, 1.0, 0.0);
    let yellow = vec3<f32>(1.0, 1.0, 0.0);
    let red    = vec3<f32>(1.0, 0.0, 0.0);
    if (x < 0.25) {
        return mix(blue, cyan, x / 0.25);
    } else if (x < 0.5) {
        return mix(cyan, green, (x - 0.25) / 0.25);
    } else if (x < 0.75) {
        return mix(green, yellow, (x - 0.5) / 0.25);
    }
    return mix(yellow, red, (x - 0.75) / 0.25);
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    // Local mesh position carries x,z in world meters and y = 0; reconstruct the
    // planar world point (north = -z) and displace y by the sampled height.
    let world_xy = vec2<f32>(vertex.position.x, -vertex.position.z);
    var local = vertex.position;
    local.y = height_at_world(world_xy) * p.vexag;

    let world_from_local = get_world_from_local(vertex.instance_index);
    let world_pos = mesh_position_local_to_world(world_from_local, vec4<f32>(local, 1.0));
    out.clip_position = position_world_to_clip(world_pos.xyz);
    out.world_xy = world_xy;
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(dem));
    let tc = world_to_texel(in.world_xy, dims);

    // Mode 2: debug height ramp — color by absolute elevation, no lighting.
    if (p.surface_mode == 2u) {
        let h = height_at(tc, dims);
        let t = (h - p.elev_min) / (p.elev_max - p.elev_min);
        return vec4<f32>(height_ramp(t), 1.0);
    }

    // Finite-difference normal in DEM (east, north, up) space, scaled by vexag so the
    // shaded slopes match the exaggerated silhouette (identical math to hillshade.wgsl).
    let ground = p.dem_world_size / vec2<f32>(dims);
    let h_l = height_at(tc + vec2<i32>(-1, 0), dims);
    let h_r = height_at(tc + vec2<i32>( 1, 0), dims);
    let h_d = height_at(tc + vec2<i32>(0,  1), dims); // +v = south (smaller world.y)
    let h_u = height_at(tc + vec2<i32>(0, -1), dims); // -v = north (larger world.y)
    let dzdx = p.vexag * (h_r - h_l) / (2.0 * ground.x);
    let dzdy = p.vexag * (h_u - h_d) / (2.0 * ground.y);
    let n = normalize(vec3<f32>(-dzdx, -dzdy, 1.0));
    let l = vec3<f32>(
        cos(p.sun_altitude) * sin(p.sun_azimuth), // east  (+x)
        cos(p.sun_altitude) * cos(p.sun_azimuth), // north (+y)
        sin(p.sun_altitude),                       // up    (+z)
    );
    let illum = clamp(dot(n, l), 0.0, 1.0);

    // Mode 1: draped imagery, relit. A small ambient keeps shadowed faces from going
    // pure black (the single zoom-0 tile is sampled with the same UV as the DEM).
    if (p.surface_mode == 1u) {
        let img = textureSample(imagery, imagery_sampler, world_to_dem_uv(in.world_xy)).rgb;
        return vec4<f32>(img * (illum * 0.85 + 0.15), 1.0);
    }

    // Mode 0: analytic hillshade relief.
    return vec4<f32>(vec3<f32>(illum) * TINT, 1.0);
}
