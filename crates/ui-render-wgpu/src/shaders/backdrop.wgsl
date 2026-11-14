@group(1) @binding(0) var src: texture_2d<f32>;
@group(1) @binding(1) var src_sampler: sampler;

struct Params {
    rect: vec4<f32>,       // backdrop rectangle (logical px)
    tint: vec4<f32>,       // sRGB, straight alpha (alpha = tint strength)
    direction: vec2<f32>,  // one texel along the blur axis, in uv units
    radius: f32,           // corner radius (logical px)
    sigma: f32,            // Gaussian sigma in texels
};

@group(2) @binding(0) var<uniform> params: Params;

struct FullOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> FullOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: FullOut;
    out.position = vec4<f32>(p * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = p;
    return out;
}

@fragment
fn fs_copy(in: FullOut) -> @location(0) vec4<f32> {
    return textureSample(src, src_sampler, in.uv);
}

@fragment
fn fs_blur(in: FullOut) -> @location(0) vec4<f32> {
    let sigma = max(params.sigma, 0.5);
    let reach = i32(min(ceil(sigma * 2.5), 32.0));
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var k = -reach; k <= reach; k++) {
        let x = f32(k);
        let w = exp(-0.5 * x * x / (sigma * sigma));
        sum += w * textureSampleLevel(src, src_sampler, in.uv + params.direction * x, 0.0);
        total += w;
    }
    return sum / total;
}

struct RectOut {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_rect(@builtin(vertex_index) i: u32) -> RectOut {
    let corner = strip_corner(i);
    let p = params.rect.xy + corner * params.rect.zw;
    var out: RectOut;
    out.position = to_clip(p);
    out.local = p - (params.rect.xy + params.rect.zw * 0.5);
    out.uv = p / globals.viewport;
    return out;
}

fn backdrop_box_sdf(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

@fragment
fn fs_composite(in: RectOut) -> @location(0) vec4<f32> {
    let half_size = params.rect.zw * 0.5;
    let r = min(params.radius, min(half_size.x, half_size.y));
    let d = backdrop_box_sdf(in.local, half_size, r) * globals.scale;
    let coverage = clamp(0.5 - d, 0.0, 1.0);
    let blurred = textureSample(src, src_sampler, in.uv);
    let tint = output_color(params.tint);
    let color = mix(blurred.rgb, tint.rgb, tint.a);
    return vec4<f32>(color, coverage);
}
