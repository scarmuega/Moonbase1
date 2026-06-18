// Runtime elevation hillshade (plan 05). Shades the baked LOLA DEM as relief with
// a live sun direction. The world→DEM-UV mapping mirrors `moon_data::world_to_dem_uv`
// byte-for-byte: u = (x - min_x)/span_x, v = (max_y - y)/span_y (y-flip, north on top).
//
// The DEM is a 16-bit grayscale PNG, which Bevy loads as an *integer* texture
// (`R16Uint`) — so we read it with `textureLoad` (no sampler). Nearest texel reads
// are exactly what finite-difference gradients want anyway.
//
// Reuses the stock mesh2d vertex shader; we only provide the fragment stage and read
// the interpolated world position it hands us.

#import bevy_sprite::mesh2d_vertex_output::VertexOutput

struct HillshadeParams {
    // World-space bbox of the DEM (= manifest world_bbox), as min corner + size.
    dem_world_min: vec2<f32>,
    dem_world_size: vec2<f32>,
    // Elevation decode bounds (meters): elev = elev_min + sample * (elev_max - elev_min).
    elev_min: f32,
    elev_max: f32,
    // Sun direction in radians (azimuth = compass bearing, altitude above horizon).
    sun_azimuth: f32,
    sun_altitude: f32,
    // Reserved for future shading modes (0 = standard hillshade).
    mode: u32,
};

@group(2) @binding(0) var<uniform> p: HillshadeParams;
@group(2) @binding(1) var dem: texture_2d<u32>;

// 16-bit unsigned max — normalizes the stored R16Uint sample to [0, 1].
const U16_MAX: f32 = 65535.0;
// Warm color grade applied to the relief, matching IMAGERY_TINT in streaming.rs so
// the hillshade and imagery views share a mood. Set to vec3(1.0) to disable.
const TINT: vec3<f32> = vec3<f32>(1.0, 0.96, 0.90);

// Decode elevation (meters) at an integer DEM texel, clamped to the texture.
fn height_at(coord: vec2<i32>, dims: vec2<i32>) -> f32 {
    let c = clamp(coord, vec2<i32>(0, 0), dims - vec2<i32>(1, 1));
    let s = f32(textureLoad(dem, c, 0).r) / U16_MAX;
    return p.elev_min + s * (p.elev_max - p.elev_min);
}

// Debug height ramp (mode 1): map t in [0,1] (low->high elevation) to a jet-like
// gradient. Low/deep = blue, mid = green/yellow, high = red. Lets us sanity-check that
// crater floors read as low and rims/peaks read as high, independent of any lighting.
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

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world = in.world_position.xy;

    // world -> DEM UV (identical to moon_data::world_to_dem_uv).
    let uv = vec2<f32>(
        (world.x - p.dem_world_min.x) / p.dem_world_size.x,
        (p.dem_world_min.y + p.dem_world_size.y - world.y) / p.dem_world_size.y,
    );

    let dims = vec2<i32>(textureDimensions(dem));
    let tc = vec2<i32>(floor(uv * vec2<f32>(dims)));

    // Mode 1: debug height ramp — color by absolute elevation, no lighting.
    if (p.mode == 1u) {
        let h = height_at(tc, dims);
        let t = (h - p.elev_min) / (p.elev_max - p.elev_min);
        return vec4<f32>(height_ramp(t), 1.0);
    }

    // Ground sample distance per DEM texel in meters (≈5 m for Shackleton).
    let ground = p.dem_world_size / vec2<f32>(dims);

    // Central differences over ±1 texel, scaled to physical meters.
    let h_l = height_at(tc + vec2<i32>(-1, 0), dims);
    let h_r = height_at(tc + vec2<i32>( 1, 0), dims);
    let h_d = height_at(tc + vec2<i32>(0,  1), dims); // +v = south (smaller world.y)
    let h_u = height_at(tc + vec2<i32>(0, -1), dims); // -v = north (larger world.y)

    // dz/dx eastward, dz/dy northward (note the v-flip: north is the -v direction).
    let dzdx = (h_r - h_l) / (2.0 * ground.x);
    let dzdy = (h_u - h_d) / (2.0 * ground.y);

    // Surface normal (z up) and sun unit vector, then Lambertian shading. Azimuth is a
    // compass bearing (0 = north = +y, 90 = east = +x); altitude is above the horizon.
    // A direct dot product avoids the aspect/atan2 convention trap that previously
    // rotated the apparent light direction 90° off the slider's compass bearing.
    let n = normalize(vec3<f32>(-dzdx, -dzdy, 1.0));
    let l = vec3<f32>(
        cos(p.sun_altitude) * sin(p.sun_azimuth), // east  (+x)
        cos(p.sun_altitude) * cos(p.sun_azimuth), // north (+y)
        sin(p.sun_altitude),                       // up    (+z)
    );
    let illum = clamp(dot(n, l), 0.0, 1.0);

    return vec4<f32>(vec3<f32>(illum) * TINT, 1.0);
}
