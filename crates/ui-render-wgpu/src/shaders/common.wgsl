// Shared by every pipeline. Prepended to each shader at build time.

struct Globals {
    // Target size in logical pixels.
    viewport: vec2<f32>,
    // Physical pixels per logical pixel.
    scale: f32,
    // 1 when the render target stores sRGB (colors must be converted to linear).
    srgb_output: u32,
};

@group(0) @binding(0) var<uniform> globals: Globals;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// Input colors are sRGB-encoded with straight alpha.
fn output_color(c: vec4<f32>) -> vec4<f32> {
    if globals.srgb_output == 1u {
        return vec4<f32>(srgb_to_linear(c.rgb), c.a);
    }
    return c;
}

// Logical pixel position (top-left origin, y down) to clip space.
fn to_clip(p: vec2<f32>) -> vec4<f32> {
    let ndc = p / globals.viewport * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);
    return vec4<f32>(ndc, 0.0, 1.0);
}

// Unit quad corner for a 4-vertex triangle strip: (0,0) (1,0) (0,1) (1,1).
fn strip_corner(vertex_index: u32) -> vec2<f32> {
    return vec2<f32>(f32(vertex_index & 1u), f32(vertex_index >> 1u));
}
