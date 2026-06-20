// Mooncraft terrain surface shader.
//
// Renders the baked LOLA DEM as real 3D geometry shaded to look like lunar regolith,
// with NO streamed imagery — the surface detail comes from per-fragment DEM relief plus
// a synthetic "dusty" procedural texture.
//
// Geometry is displaced in the vertex stage (live `vexag`); the fragment stage computes
// the surface normal *per fragment* from DEM finite differences (so the relief stays
// crisp at the DEM's native resolution, independent of mesh density) and Lambertian-
// shades it against the sun. A procedural fbm mottle + fine grain modulate a warm
// regolith albedo for the powdery feel.
//
// Coordinate mapping (Sprint 02): world (x, y) with +Y = north → 3D (x, h·vexag, -y), so
// world_xy = (position.x, -position.z). DEM UV mirrors moon_data::world_to_dem_uv:
// u = (x-min_x)/span_x, v = (max_y - y)/span_y. The DEM is an R16Uint integer texture
// read with textureLoad (no sampler), legal in both stages.

#import bevy_pbr::mesh_functions::{get_world_from_local, mesh_position_local_to_world}
#import bevy_pbr::view_transformations::position_world_to_clip
#import bevy_pbr::forward_io::Vertex

struct TerrainParams {
    dem_world_min: vec2<f32>,
    dem_world_size: vec2<f32>,
    elev_min: f32,
    elev_max: f32,
    sun_azimuth: f32,
    sun_altitude: f32,
    vexag: f32,
    // Strength of the synthetic dusty albedo texture (0 = flat albedo).
    detail: f32,
    // Strength of synthetic sub-DEM relief added to the lighting normal (0 = off).
    synth: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> p: TerrainParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var dem: texture_2d<u32>;

const U16_MAX: f32 = 65535.0;
// Warm regolith albedo (matte, slightly dark — "dusty" rather than chalk white).
const REGOLITH: vec3<f32> = vec3<f32>(0.80, 0.76, 0.70);
// Soft ambient floor so shadowed slopes aren't pure black.
const AMBIENT: f32 = 0.05;

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_xy: vec2<f32>,
};

fn world_to_dem_uv(world: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        (world.x - p.dem_world_min.x) / p.dem_world_size.x,
        (p.dem_world_min.y + p.dem_world_size.y - world.y) / p.dem_world_size.y,
    );
}

fn world_to_texel(world: vec2<f32>, dims: vec2<i32>) -> vec2<i32> {
    return vec2<i32>(floor(world_to_dem_uv(world) * vec2<f32>(dims)));
}

fn height_at(coord: vec2<i32>, dims: vec2<i32>) -> f32 {
    let c = clamp(coord, vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
    let s = f32(textureLoad(dem, c, 0).r) / U16_MAX;
    return p.elev_min + s * (p.elev_max - p.elev_min);
}

fn height_at_world(world: vec2<f32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(dem));
    return height_at(world_to_texel(world, dims), dims);
}

// --- value noise / fbm for the synthetic dust ---
fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i: i32 = 0; i < 4; i = i + 1) {
        v += amp * vnoise(q);
        q *= 2.0;
        amp *= 0.5;
    }
    return v;
}

// Synthetic sub-DEM relief (meters): a few fine fractal octaves (~2–32 m features) used
// to fake resolution below the baked DEM. Plausible fabrication, not measured data.
fn synth_height(w: vec2<f32>) -> f32 {
    var h = 0.0;
    var amp = 6.0; // meters at the coarsest synthetic octave
    var freq = 1.0 / 32.0;
    for (var i: i32 = 0; i < 5; i = i + 1) {
        h += amp * (vnoise(w * freq) * 2.0 - 1.0);
        amp *= 0.5;
        freq *= 2.03;
    }
    return h;
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
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
    let world = in.world_xy;
    let dims = vec2<i32>(textureDimensions(dem));
    let tc = world_to_texel(world, dims);

    // Per-fragment normal from DEM finite differences (vexag-scaled), in DEM (east,
    // north, up) space — crisp at native DEM resolution regardless of mesh density.
    let ground = p.dem_world_size / vec2<f32>(dims);
    let h_l = height_at(tc + vec2<i32>(-1, 0), dims);
    let h_r = height_at(tc + vec2<i32>( 1, 0), dims);
    let h_d = height_at(tc + vec2<i32>(0,  1), dims); // +v = south
    let h_u = height_at(tc + vec2<i32>(0, -1), dims); // -v = north
    var dzdx = p.vexag * (h_r - h_l) / (2.0 * ground.x);
    var dzdy = p.vexag * (h_u - h_d) / (2.0 * ground.y);

    // Synthetic sub-DEM relief: add fine fractal slopes to the lighting normal so the
    // surface reads finer than the baked DEM. Faded out by the screen footprint (world
    // meters per pixel) so it doesn't shimmer/alias when zoomed out.
    if (p.synth > 0.0) {
        let footprint = max(fwidth(world.x), fwidth(world.y));
        let aa = 1.0 - smoothstep(10.0, 60.0, footprint);
        if (aa > 0.0) {
            let e = max(footprint, 1.5);
            let sx = (synth_height(world + vec2<f32>(e, 0.0)) - synth_height(world - vec2<f32>(e, 0.0))) / (2.0 * e);
            let sy = (synth_height(world + vec2<f32>(0.0, e)) - synth_height(world - vec2<f32>(0.0, e))) / (2.0 * e);
            let k = p.synth * p.vexag * aa;
            dzdx += k * sx;
            dzdy += k * sy;
        }
    }

    let n = normalize(vec3<f32>(-dzdx, -dzdy, 1.0));
    let l = vec3<f32>(
        cos(p.sun_altitude) * sin(p.sun_azimuth), // east  (+x)
        cos(p.sun_altitude) * cos(p.sun_azimuth), // north (+y)
        sin(p.sun_altitude),                       // up    (+z)
    );
    let illum = max(clamp(dot(n, l), 0.0, 1.0), AMBIENT);

    // Synthetic dusty texture: broad mottle (~20 m) + fine grain (~1 m) modulating albedo.
    let mottle = fbm(world * 0.05) - 0.5;
    let grain = vnoise(world * 0.8) - 0.5;
    let dust = 1.0 + p.detail * (mottle * 0.35 + grain * 0.18);

    return vec4<f32>(illum * REGOLITH * dust, 1.0);
}
